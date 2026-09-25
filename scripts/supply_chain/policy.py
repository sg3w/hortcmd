"""Offline, deterministic policy primitives. No package code is imported/executed."""
import datetime
import re


def reviewed(policy, kind, key, fingerprint=None):
    for entry in policy.get('reviews', []):
        if entry['kind'] == kind and entry['key'] == key:
            if fingerprint is not None and entry.get('fingerprint') != fingerprint:
                return False
            return bool(entry.get('owner') and entry.get('reason') and entry.get('expires', '') >= datetime.date.today().isoformat())
    return False


def validate_reviews(policy):
    errors = []
    seen = set()
    for entry in policy.get('reviews', []) + policy.get('advisoryExceptions', []):
        key = (entry.get('kind', 'advisory'), entry.get('key', ''))
        if key in seen:
            errors.append(f'duplicate review {key}')
        seen.add(key)
        try:
            expiry = datetime.date.fromisoformat(entry['expires'])
            if expiry < datetime.date.today():
                errors.append(f'expired review {key}')
        except (KeyError, ValueError):
            errors.append(f'invalid expiry {key}')
        if not entry.get('owner') or not entry.get('reason'):
            errors.append(f'unowned/unexplained review {key}')
        if key[0] == 'advisory' and not entry.get('control'):
            errors.append(f'no compensating control {key}')
    return errors


def license_allowed(expression, allowed):
    """Evaluate SPDX AND/OR/WITH and legacy Cargo slash alternatives; fail closed."""
    if not isinstance(expression, str) or not expression:
        return False
    expression = re.sub(r'\s*/\s*', ' OR ', expression)
    tokens = re.findall(r'\(|\)|[^\s()]+', expression)
    index = 0

    def atom():
        nonlocal index
        if index >= len(tokens):
            raise ValueError('missing license')
        item = tokens[index]; index += 1
        if item == '(':
            value = either()
            if index >= len(tokens) or tokens[index] != ')':
                raise ValueError('unclosed expression')
            index += 1
            return value
        if item in ['AND', 'OR', 'WITH', ')']:
            raise ValueError('unexpected operator')
        if index < len(tokens) and tokens[index] == 'WITH':
            index += 1
            if index >= len(tokens):
                raise ValueError('missing exception')
            item += ' WITH ' + tokens[index]; index += 1
        return item in allowed

    def both():
        nonlocal index
        value = atom()
        while index < len(tokens) and tokens[index] == 'AND':
            index += 1
            other = atom(); value = value and other
        return value

    def either():
        nonlocal index
        value = both()
        while index < len(tokens) and tokens[index] == 'OR':
            index += 1
            other = both(); value = value or other
        return value
    try:
        value = either()
        return value and index == len(tokens)
    except (ValueError, RecursionError):
        return False


def check_components(components, policy):
    errors = validate_reviews(policy)
    groups = {}
    for c in components:
        key = c['ref']
        licenses = policy['allowedLicenses'][:]
        if reviewed(policy, 'license', key, c['license']):
            licenses += policy['reviewLicenses']
        if not license_allowed(c['license'], licenses):
            errors.append(f'license refused: {key}: {c["license"]}')
        known_registry = c['source'] == 'registry+https://github.com/rust-lang/crates.io-index' or c['source'].startswith('https://registry.npmjs.org/')
        if not c.get('own') and not known_registry and not reviewed(policy, 'source', key, c['checksum']):
            errors.append(f'unreviewed source: {key}')
        if not c.get('own') and not c['checksum']:
            errors.append(f'missing provenance checksum: {key}')
        if c.get('nativeSources') and not reviewed(policy, 'native', key, c['checksum']):
            errors.append(f'unreviewed native sources: {key}')
        if c.get('installScript') and not reviewed(policy, 'install', key, c['checksum']):
            errors.append(f'unreviewed install script: {key}')
        if c.get('direct'):
            review = next((r for r in policy['reviews'] if r['kind'] == 'direct' and r['key'] == key), {})
            if not reviewed(policy, 'direct', key) or not all(review.get('checklist', {}).get(k) for k in ['changelog','security','msrv','platforms','features']):
                errors.append(f'unreviewed direct dependency: {key}')
        groups.setdefault((c['ecosystem'], c['name']), set()).add(c['version'])
    duplicates = []
    for (ecosystem, name), versions in sorted(groups.items()):
        if len(versions) > 1:
            key = f'{ecosystem}:{name}'; fingerprint = ','.join(sorted(versions))
            duplicates.append({'name': key, 'versions': sorted(versions)})
            if any(re.search(pattern, name) for pattern in policy['securityCritical']):
                if not reviewed(policy, 'duplicate', key, fingerprint):
                    errors.append(f'unreviewed security duplicate: {key}: {fingerprint}')
    return errors, duplicates


def advisory_allowed(policy, ecosystem, name, version, advisory):
    key = f'{ecosystem}:{name}@{version}:{advisory}'
    return any(e.get('key') == key and e.get('owner') and e.get('reason') and e.get('control') and e.get('expires', '') >= datetime.date.today().isoformat() for e in policy.get('advisoryExceptions', []))


def check_npm_audit(report, components, policy):
    errors = []
    if report.get('error') or 'vulnerabilities' not in report or 'metadata' not in report:
        return ['npm advisory service failed or returned an invalid report']
    for item in report['vulnerabilities'].values():
        if item.get('severity') not in ['high', 'critical']:
            continue
        # npm nodes identify exact resolved lockfile entries, including duplicates.
        for node in item.get('nodes', []):
            c = next((c for c in components if c.get('lockPath') == node), None)
            if c is None:
                errors.append(f'unknown vulnerable npm node {node}'); continue
            advisories = [str(v['source']) for v in item.get('via', []) if isinstance(v, dict)]
            if not advisories or not all(advisory_allowed(policy, 'npm', c['name'], c['version'], a) for a in advisories):
                errors.append(f'npm {item["severity"]}: {c["ref"]}')
    return errors


def check_rust_report(records, policy):
    errors, findings = [], []
    summary = next((r for r in records if r.get('type') == 'summary' and 'advisories' in r.get('fields', {})), None)
    if summary is None:
        return ['Rust advisory scanner failed without a summary'], []
    observed_errors = 0
    for record in records:
        if record.get('type') != 'diagnostic':
            continue
        f = record['fields']
        if f.get('severity') != 'error':
            continue
        observed_errors += 1
        advisory = f.get('advisory')
        graphs = f.get('graphs', [])
        if not advisory or not graphs:
            # Includes yanks, invalid config and registry failures; never waived
            # by a generic advisory exception.
            errors.append(f.get('code', 'scanner failure')); continue
        for graph in graphs:
            crate = graph['Krate']
            finding = {'name': crate['name'], 'version': crate['version'], 'id': advisory['id'], 'kind': f['code'], 'title': advisory['title']}
            finding['excepted'] = advisory_allowed(policy, 'cargo', crate['name'], crate['version'], advisory['id'])
            findings.append(finding)
            if not finding['excepted']:
                errors.append(f'Rust {f["code"]}: {crate["name"]}@{crate["version"]} {advisory["id"]}')
    if summary['fields']['advisories']['errors'] != observed_errors:
        errors.append('Rust scanner error count mismatch')
    return errors, findings
