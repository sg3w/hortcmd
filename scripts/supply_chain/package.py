"""Create a release handoff ZIP with installers and verified sidecar evidence.
No signing, publication or network access. Native resources also use the Tauri overlay.
"""
import hashlib
import json
import pathlib
import sys
import zipfile
from inventory import ROOT
from main import verify


def package(bundle_root, destination):
    evidence=ROOT/'artifacts/supply-chain'
    verify(evidence)
    files=[p for p in bundle_root.rglob('*') if p.is_file()]
    if not files: raise ValueError('No Tauri bundles to package')
    manifest=json.loads((evidence/'manifest.json').read_text())
    names=sorted([*manifest['files'], 'manifest.json'])
    installers=[p for p in files if p.suffix in ['.dmg','.deb','.rpm','.msi','.exe']]
    if any(p.is_symlink() for p in installers): raise ValueError('Installer paths must be regular files')
    if not installers: raise ValueError('No supported installer (AppImage needs native-library review)')
    if any(p.suffix=='.AppImage' for p in files): raise ValueError('AppImage native runtime license inventory is not approved')
    contents={**{'supply-chain/'+n:evidence/n for n in names},**{'installers/'+p.relative_to(bundle_root).as_posix():p for p in installers}}
    if destination.exists(): raise ValueError('Release handoff already exists; choose a new destination')
    destination.parent.mkdir(parents=True,exist_ok=True)
    with zipfile.ZipFile(destination,'x',compression=zipfile.ZIP_DEFLATED) as archive:
        for name,path in sorted(contents.items()): archive.write(path,name)
    with zipfile.ZipFile(destination) as archive:
        if set(archive.namelist()) != set(contents): raise ValueError('Incorrect handoff members')
        for name,path in contents.items():
            if hashlib.sha256(archive.read(name)).digest() != hashlib.sha256(path.read_bytes()).digest(): raise ValueError('Packaged artifact checksum mismatch')
    print('Verified release handoff:',destination)


if __name__=='__main__':
    try: package(ROOT/'src-tauri/target/release/bundle',pathlib.Path(sys.argv[1]))
    except (ValueError,OSError,IndexError) as error: print('Release handoff refused:',error,file=sys.stderr);sys.exit(1)
