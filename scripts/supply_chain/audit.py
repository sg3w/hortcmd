"""Explicit online audit; only public package names/versions go to npm's registry.
RustSec is downloaded and matched locally. There is no source upload.
"""
import json
import os
import pathlib
import subprocess
import sys
from inventory import ROOT, collect
from policy import check_npm_audit, check_rust_report, validate_reviews


def main():
    policy=json.loads((ROOT/'policy/dependencies.json').read_text())
    scanner=os.environ.get('HORTCMD_CARGO_DENY','cargo-deny')
    version=subprocess.check_output([scanner,'--version'],text=True).strip()
    if version != 'cargo-deny '+policy['tools']['cargoDeny']: raise ValueError('wrong cargo-deny version')
    if subprocess.check_output(['npm','--version'],text=True).strip() != policy['tools']['npm']: raise ValueError('wrong npm audit version')
    if policy.get('publicNpmAuditRegistry') != 'https://registry.npmjs.org': raise ValueError('public npm audit not configured')
    out=ROOT/'artifacts/supply-chain'; out.mkdir(parents=True,exist_ok=True)
    rust=subprocess.run([scanner,'--manifest-path','src-tauri/Cargo.toml','--all-features','--format','json','check','--config','policy/deny.toml','advisories'],cwd=ROOT,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
    (out/'rust-advisories.jsonl').write_text(rust.stdout)
    records=[json.loads(line) for line in rust.stdout.splitlines() if line.startswith('{')]
    errors,findings=check_rust_report(records,policy)
    if rust.returncode not in [0,1] or (rust.returncode and not findings and not errors): errors.append('cargo-deny execution failed')
    npm=subprocess.run(['npm','audit','--json','--package-lock-only','--ignore-scripts','--registry='+policy['publicNpmAuditRegistry']],cwd=ROOT,text=True,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
    report=json.loads(npm.stdout)
    (out/'npm-advisories.json').write_text(json.dumps(report,indent=2)+'\n')
    errors+=check_npm_audit(report,collect(),policy)+validate_reviews(policy)
    if npm.returncode not in [0,1]: errors.append('npm audit execution failed')
    revisions=[]
    for db in pathlib.Path(os.environ.get('CARGO_HOME',pathlib.Path.home()/'.cargo')).glob('advisory-dbs/*'):
        revision=subprocess.run(['git','-C',str(db),'rev-parse','HEAD'],text=True,capture_output=True)
        if revision.returncode==0: revisions.append(revision.stdout.strip())
    (out/'audit-summary.json').write_text(json.dumps({'schema':1,'tools':policy['tools'],'rustsecRevisions':sorted(revisions),'findings':findings,'errors':errors},indent=2)+'\n')
    for error in errors: print(error,file=sys.stderr)
    print(f'Advisory gate: {len(findings)} Rust findings; {len(errors)} blockers')
    return bool(errors)


if __name__=='__main__':
    try: sys.exit(main())
    except (OSError,ValueError,KeyError,subprocess.CalledProcessError) as error:
        print('Advisory gate failed: '+str(error),file=sys.stderr); sys.exit(1)
