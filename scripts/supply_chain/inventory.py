"""Resolve both lockfiles, including optional and target-specific components."""
import base64
import hashlib
import json
import pathlib
import re
import subprocess
import tomllib
from urllib.parse import quote

ROOT = pathlib.Path(__file__).resolve().parents[2]


def sha(data):
    return hashlib.sha256(data).hexdigest()


def tree_digest(root):
    h = hashlib.sha256()
    for path in sorted(root.rglob('*')):
        if path.is_file() and not any(p in ['target', '.git', '__pycache__'] for p in path.relative_to(root).parts):
            data = path.read_bytes()
            try:
                data = data.decode().replace('\r\n', '\n').encode()
            except UnicodeDecodeError:
                pass
            h.update(path.relative_to(root).as_posix().encode() + b'\0' + data + b'\0')
    return h.hexdigest()


def metadata():
    return json.loads(subprocess.check_output(['cargo', 'metadata', '--manifest-path', str(ROOT/'src-tauri/Cargo.toml'), '--locked', '--offline', '--all-features', '--format-version', '1'], cwd=ROOT))


def collect(meta=None):
    meta = meta or metadata()
    lock = tomllib.loads((ROOT/'src-tauri/Cargo.lock').read_text())['package']
    nodes = {n['id']: n for n in meta['resolve']['nodes']}
    root_id = meta['resolve']['root']
    direct = {d['pkg'] for d in nodes[root_id]['deps']}
    packages = {p['id']:p for p in meta['packages']}
    runtime = set()
    pending = [root_id]
    while pending:
        current = pending.pop()
        if current in runtime:
            continue
        if any('proc-macro' in t['kind'] for t in packages[current]['targets']):
            continue
        runtime.add(current)
        pending.extend(d['pkg'] for d in nodes[current]['deps'] if any(k['kind'] is None for k in d['dep_kinds']))
    components = []
    refs = {p['id']: f'cargo:{p["name"]}@{p["version"]}' for p in meta['packages']}
    for p in meta['packages']:
        own = p['id'] == root_id
        directory = pathlib.Path(p['manifest_path']).parent
        entry = next((c for c in lock if c['name'] == p['name'] and c['version'] == p['version'] and c.get('source') == p['source']), None)
        if not entry:
            raise ValueError(f'Cargo metadata/lock mismatch: {p["name"]}')
        source = p['source'] or 'path:' + directory.relative_to(ROOT).as_posix()
        checksum = entry.get('checksum') or tree_digest(directory) if not own else sha((ROOT/'src-tauri/Cargo.toml').read_bytes())
        proc_macro = any('proc-macro' in t['kind'] for t in p['targets'])
        components.append({'ref': refs[p['id']], 'ecosystem': 'cargo', 'name': p['name'], 'version': p['version'], 'license': p['license'] or '', 'source': source, 'checksum': checksum, 'direct': p['id'] in direct, 'runtimeCandidate': p['id'] in runtime and not proc_macro, 'own': own, 'installScript': any('custom-build' in t['kind'] for t in p['targets']) and not own, 'authors': p['authors'], 'repository': p['repository'], 'directory': directory, 'dependencies': sorted({refs[d['pkg']] for d in nodes[p['id']]['deps']}), 'features': nodes[p['id']]['features'], 'targets': sorted({k['target'] or 'all' for n in nodes.values() for d in n['deps'] if d['pkg'] == p['id'] for k in d['dep_kinds']})})
    if {(c['name'], c['version']) for c in components} != {(c['name'], c['version']) for c in lock}:
        raise ValueError('Cargo metadata does not cover every lockfile component')
    npm = json.loads((ROOT/'package-lock.json').read_text())['packages']
    direct_npm = set(npm[''].get('dependencies', {})) | set(npm[''].get('devDependencies', {}))
    def resolve_npm(path, name):
        parts = path.split('/')
        while parts:
            candidate = '/'.join(parts) + '/node_modules/' + name
            if candidate in npm:
                return 'npm:' + candidate + '@' + npm[candidate]['version']
            parts.pop()
        candidate = 'node_modules/' + name
        return 'npm:' + candidate + '@' + npm[candidate]['version'] if candidate in npm else None
    for path, p in npm.items():
        if not path:
            continue
        name = p.get('name') or path.rsplit('node_modules/', 1)[-1]
        deps = {**p.get('dependencies', {}), **p.get('optionalDependencies', {}), **p.get('peerDependencies', {})}
        components.append({'ref': 'npm:' + path + '@' + p['version'], 'ecosystem': 'npm', 'name': name, 'version': p['version'], 'license': p.get('license', ''), 'source': p.get('resolved', 'path:' + path), 'checksum': p.get('integrity', ''), 'direct': path == 'node_modules/' + name and name in direct_npm, 'runtimeCandidate': not p.get('dev', False), 'own': False, 'installScript': p.get('hasInstallScript', False), 'lockPath': path, 'directory': ROOT/path, 'dependencies': sorted({r for n in deps if (r := resolve_npm(path, n))}), 'targets': p.get('os', []) + p.get('cpu', []), 'features': ['optional'] if p.get('optional') else []})
    for c in components:
        if c['ecosystem']=='cargo' and not c['own'] and c.get('installScript'):
            c['nativeSources']=any(p.suffix in ['.c','.cc','.cpp','.S'] for p in c['directory'].rglob('*') if p.is_file())
    return components


def bom(components):
    items = []
    for c in components:
        properties = {k: c[k] for k in ['ecosystem', 'source', 'checksum', 'direct', 'runtimeCandidate', 'features', 'targets']}
        properties['scopeNote'] = 'Runtime candidate across supported targets; presence in this lockfile does not imply linkage into every binary.'
        items.append({'type': 'application' if c['own'] else 'library', 'bom-ref': c['ref'], 'name': c['name'], 'version': c['version'], 'purl': f'pkg:{c["ecosystem"]}/{quote(c["name"], safe="/")}@{c["version"]}', 'scope': 'required' if c['runtimeCandidate'] else 'excluded', 'licenses': [{'expression': re.sub(r'\s*/\s*', ' OR ', c['license'])}], 'properties': [{'name': 'hortcmd:' + k, 'value': json.dumps(v, ensure_ascii=False, sort_keys=True)} for k, v in properties.items()]})
    for item,c in zip(items,components):
        checksum=c['checksum']
        if re.fullmatch('[a-f0-9]{64}',checksum):
            item['hashes']=[{'alg':'SHA-256','content':checksum}]
        elif checksum.startswith(('sha512-','sha256-')):
            algorithm,data=checksum.split('-',1)
            item['hashes']=[{'alg':{'sha512':'SHA-512','sha256':'SHA-256'}[algorithm],'content':base64.b64decode(data).hex()}]
    policy=json.loads((ROOT/'policy/dependencies.json').read_text())
    for item,c in zip(items,components):
        native=[n for n in policy.get('nativeComponents',[]) if n['ownerPackage']==c['ref']]
        if native:
            item['components']=[{'type':'library','bom-ref':'native:'+n['name']+'@'+n['version'],'name':n['name'],'version':n['version'],'licenses':[{'expression':n['license']}],'hashes':[{'alg':'SHA-256','content':n['sha256']}],'properties':[{'name':'hortcmd:source','value':n['source']},{'name':'hortcmd:scope','value':n['scope']}]} for n in native]
    return {'bomFormat': 'CycloneDX', 'specVersion': '1.6', 'version': 1, 'metadata': {'properties': [{'name': 'hortcmd:'+p, 'value': sha((ROOT/p).read_bytes())} for p in ['src-tauri/Cargo.lock', 'package-lock.json']]}, 'components': sorted(items, key=lambda c:c['bom-ref']), 'dependencies': sorted([{'ref': c['ref'], 'dependsOn': c['dependencies']} for c in components], key=lambda c:c['ref'])}


def notices(components):
    lines = ['hortcmd third-party notices', 'Inventory includes build/test and all target-specific dependencies; excluded components are not asserted to ship.', '']
    for c in sorted(components, key=lambda c:c['ref']):
        lines.extend([c['ref'], 'License: ' + c['license'], 'Source: ' + c['source']])
        lines.extend(c.get('authors', []))
        directory = c['directory']
        if not directory.is_dir():
            lines.append('Platform-optional package is not installed on this host; license expression from its locked registry metadata.')
            continue
        candidates = []
        for path in ([] if c['own'] else directory.rglob('*')):
            relative = path.relative_to(directory)
            if any(part in ['node_modules','target','.git'] for part in relative.parts):
                continue
            if path.is_file() and path.name.lower().startswith(('license','licence','copying','notice','copyright')):
                candidates.append(path)
        if c['own']:
            candidates.append(ROOT/'LICENSE')
        for path in sorted(set(candidates)):
            if path.stat().st_size <= 2_000_000:
                lines.extend(['--- ' + path.name + ' ---', path.read_text(errors='replace')])
        if not candidates:
            # Published packages sometimes omit license files. Preserve copyright
            # declarations from their source in addition to manifest attribution.
            for path in sorted(directory.rglob('*')):
                if path.is_file() and path.suffix in ['.rs','.c','.h','.md']:
                    for line in path.read_text(errors='replace').splitlines():
                        if re.search(r'copyright\s*(?:\(c\)|[12][0-9]{3})', line, re.I):
                            lines.append(line.strip())
            lines.append('License text: see the SPDX texts included below; package metadata and source attribution above.')
        lines.append('')
    texts = ROOT/'policy/license-texts'
    if texts.exists():
        for p in sorted(texts.glob('*.txt')):
            lines.extend(['SPDX ' + p.stem, p.read_text()])
    return '\n'.join(lines)
