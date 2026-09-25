import json
import pathlib
import tempfile
import unittest
from unittest.mock import patch
from inventory import ROOT, collect, bom, tree_digest
from main import verify
from package import package

class InventoryTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.components=collect()
    def test_lock_coverage_scopes_and_no_private_source_paths(self):
        sbom=bom(self.components)
        self.assertEqual(len(sbom['components']),len(self.components))
        by_name={c['name']:c for c in sbom['components']}
        self.assertEqual(by_name['rcgen']['scope'],'excluded')
        self.assertEqual(by_name['vite']['scope'],'excluded')
        self.assertEqual(by_name['react']['scope'],'required')
        self.assertEqual(by_name['russh']['scope'],'required')
        self.assertIn('windows',by_name)
        self.assertNotIn(str(ROOT),json.dumps(sbom))
        refs={c['bom-ref'] for c in sbom['components']}
        self.assertEqual(len(refs),len(self.components))
        for node in sbom['dependencies']:self.assertTrue(set(node['dependsOn']) <= refs)
    def test_source_hash_is_stable_across_line_endings_and_changes_on_content(self):
        with tempfile.TemporaryDirectory() as d:
            path=pathlib.Path(d)/'lib.rs';path.write_bytes(b'a\nb\n');a=tree_digest(path.parent)
            path.write_bytes(b'a\r\nb\r\n');self.assertEqual(a,tree_digest(path.parent))
            path.write_bytes(b'a\nc\n');self.assertNotEqual(a,tree_digest(path.parent))
    def test_release_overlay_carries_both_sbom_and_notices(self):
        cfg=json.loads((ROOT/'src-tauri/tauri.release.conf.json').read_text())
        targets=set(cfg['bundle']['resources'].values())
        self.assertIn('supply-chain/sbom.cdx.json',targets)
        self.assertIn('supply-chain/THIRD-PARTY-NOTICES.txt',targets)
        self.assertIn('supply-chain/manifest.json',targets)
    def test_missing_or_corrupt_release_evidence_fails_closed(self):
        with tempfile.TemporaryDirectory() as d:
            path=pathlib.Path(d);(path/'manifest.json').write_text(json.dumps({'files':{'sbom.cdx.json':'wrong'},'lockfiles':{}}))
            (path/'sbom.cdx.json').write_text('{}')
            with self.assertRaises(ValueError):verify(path)
    def test_handoff_contains_verified_installers_and_notices(self):
        # The package verifier is exercised on an inert fixture, not an executable.
        with tempfile.TemporaryDirectory() as d:
            root=pathlib.Path(d);evidence=root/'artifacts/supply-chain';evidence.mkdir(parents=True)
            (evidence/'manifest.json').write_text(json.dumps({'files':{'THIRD-PARTY-NOTICES.txt':'fixture'}}))
            (evidence/'THIRD-PARTY-NOTICES.txt').write_text('fixture attribution')
            bundles=root/'bundle';bundles.mkdir();(bundles/'fixture.deb').write_bytes(b'fixture')
            with patch('package.ROOT',root),patch('package.verify'):
                package(bundles,root/'release.zip')
            import zipfile
            with zipfile.ZipFile(root/'release.zip') as archive:
                self.assertEqual(archive.read('supply-chain/THIRD-PARTY-NOTICES.txt'),b'fixture attribution')
                self.assertEqual(archive.read('installers/fixture.deb'),b'fixture')
