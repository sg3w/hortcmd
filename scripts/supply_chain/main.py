#!/usr/bin/env python3
"""Offline policy/SBOM gate. Network scanners are separate, explicit commands."""
import argparse
import json
import pathlib
import subprocess
import sys
from inventory import ROOT, collect, bom, notices, sha, tree_digest
from policy import check_components, reviewed, license_allowed


def asset_inventory():
    paths = subprocess.check_output(['git','ls-files','--cached','--others','--exclude-standard','-z'],cwd=ROOT).decode().split('\0')
    extensions = {'.png','.svg','.ico','.icns','.jpg','.jpeg','.webp','.ttf','.otf','.woff','.woff2'}
    return {p:sha((ROOT/p).read_bytes()) for p in paths if pathlib.Path(p).suffix.lower() in extensions and not p.startswith(('vendor/','harness/'))}


def check_assets(policy):
    assets = asset_inventory()
    errors = []
    for path, digest in assets.items():
        entry=next((r for r in policy['reviews'] if r['kind']=='asset' and r['key']==path),{})
        if not reviewed(policy,'asset',path,digest) or not entry.get('source') or not license_allowed(entry.get('license'),policy['allowedLicenses']):
            errors.append('unreviewed asset: '+path)
    return errors


def asset_evidence(policy):
    reviews={r['key']:r for r in policy['reviews'] if r['kind']=='asset'}
    return {'assets': {p: {'sha256':d,'license':reviews[p]['license'],'source':reviews[p]['source']} for p,d in asset_inventory().items()},'externalComponents':policy['externalComponents']}


def check_native(components,policy):
    errors=[]
    for n in policy.get('nativeComponents',[]):
        owner=next((c for c in components if c['ref']==n['ownerPackage']),None)
        if owner is None or tree_digest(owner['directory']/n['directory']) != n['sha256']:
            errors.append('changed native source: '+n['name'])
        if not license_allowed(n['license'],policy['allowedLicenses']+policy['reviewLicenses']) or not reviewed(policy,'native-license',n['name'],n['sha256']):
            errors.append('unreviewed native license: '+n['name'])
    return errors


def run(output=None):
    policy=json.loads((ROOT/'policy/dependencies.json').read_text())
    components=collect()
    errors, duplicates=check_components(components,policy)
    errors.extend(check_assets(policy))
    errors.extend(check_native(components,policy))
    license_manifest=json.loads((ROOT/'policy/license-texts/manifest.json').read_text())
    for name,digest in license_manifest['sha256'].items():
        if pathlib.Path(name).name != name or sha((ROOT/'policy/license-texts'/name).read_bytes()) != digest:
            errors.append('altered SPDX reference: '+name)
    print(f'Inventory: {sum(c["ecosystem"]=="cargo" for c in components)} Rust, {sum(c["ecosystem"]=="npm" for c in components)} npm; {len(duplicates)} duplicate groups')
    for error in errors: print(error,file=sys.stderr)
    if errors: return 1
    if output:
        output=pathlib.Path(output); output.mkdir(parents=True,exist_ok=True)
        for name,value in [('sbom.cdx.json',bom(components)),('duplicates.json',duplicates)]:
            (output/name).write_text(json.dumps(value,indent=2,ensure_ascii=False,sort_keys=True)+'\n',newline='\n')
        (output/'THIRD-PARTY-NOTICES.txt').write_text(notices(components),newline='\n')
        (output/'assets.json').write_text(json.dumps(asset_evidence(policy),indent=2,sort_keys=True)+'\n',newline='\n')
        names=['sbom.cdx.json','duplicates.json','THIRD-PARTY-NOTICES.txt','assets.json']
        manifest={'schema':1,'files':{name:sha((output/name).read_bytes()) for name in names},'lockfiles':{p:sha((ROOT/p).read_bytes()) for p in ['src-tauri/Cargo.lock','package-lock.json']}}
        (output/'manifest.json').write_text(json.dumps(manifest,indent=2,sort_keys=True)+'\n',newline='\n')
        verify(output)
    return 0


def verify(output):
    manifest=json.loads((output/'manifest.json').read_text())
    expected_files={'sbom.cdx.json','duplicates.json','THIRD-PARTY-NOTICES.txt','assets.json'}
    if manifest.get('schema') != 1 or set(manifest.get('files',{})) != expected_files or set(manifest.get('lockfiles',{})) != {'src-tauri/Cargo.lock','package-lock.json'}: raise ValueError('incomplete release manifest')
    for name,digest in manifest['files'].items():
        if pathlib.Path(name).name != name or sha((output/name).read_bytes()) != digest: raise ValueError('altered release evidence: '+name)
    for path,digest in manifest['lockfiles'].items():
        if sha((ROOT/path).read_bytes()) != digest: raise ValueError('release evidence uses a different lockfile')
    policy=json.loads((ROOT/'policy/dependencies.json').read_text())
    if json.loads((output/'assets.json').read_text()) != asset_evidence(policy): raise ValueError('asset evidence does not match reviewed assets')
    components=collect()
    errors,_=check_components(components,policy)
    errors+=check_assets(policy)+check_native(components,policy)
    if errors: raise ValueError('release policy no longer passes: '+', '.join(errors))
    expected=bom(components)
    actual=json.loads((output/'sbom.cdx.json').read_text())
    if actual != expected: raise ValueError('SBOM does not match resolved lockfiles')
    if not (output/'THIRD-PARTY-NOTICES.txt').read_text().startswith('hortcmd third-party notices'): raise ValueError('missing notices')
    print('Release evidence: lockfile coverage, scopes, relationships and hashes verified')


if __name__=='__main__':
    parser=argparse.ArgumentParser(); parser.add_argument('--output'); parser.add_argument('--verify'); args=parser.parse_args()
    try:
        if args.verify: verify(pathlib.Path(args.verify))
        else: sys.exit(run(args.output))
    except (ValueError,KeyError,OSError,subprocess.CalledProcessError) as error:
        print('Supply-chain check failed: '+str(error),file=sys.stderr); sys.exit(1)
