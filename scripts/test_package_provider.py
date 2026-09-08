"""Exercise actual archive and notice generation with synthetic source trees."""
import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import zipfile

import package_provider as package
import verify_package


def native(target):
    data = bytearray(512)
    if target.endswith('linux-gnu'):
        data[:7] = b'\x7fELF\x02\x01\x01'
        data[16], data[18], data[20], data[52] = 2, 183 if target.startswith('aarch64') else 62, 1, 64
    elif target.endswith('apple-darwin'):
        data[:4] = b'\xcf\xfa\xed\xfe'
        data[4:8] = (0x0100000c if target.startswith('aarch64') else 0x01000007).to_bytes(4, 'little')
        data[12] = 2
    else:
        data[:2] = b'MZ'
        data[60] = 64
        data[64:68] = b'PE\0\0'
        data[68:70] = (0x8664).to_bytes(2, 'little')
        data[84], data[86], data[88], data[89] = 240, 2, 11, 2
    return bytes(data)


class PackageTests(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name).resolve()
        (self.root / 'Cargo.toml').write_text('[workspace.package]\nversion="0.1.0"\n', encoding='utf-8')
        (self.root / 'LICENSE').write_text('Provider project dual-license text', encoding='utf-8')
        dependency = self.root / 'dependency'
        dependency.mkdir()
        (dependency / 'Cargo.toml').write_text('[package]\nname="dependency"\n', encoding='utf-8')
        (dependency / 'LICENSE').write_text('Dependency license text', encoding='utf-8')
        (dependency / 'NOTICE').write_text('Required dependency attribution', encoding='utf-8')
        self.metadata = {
            'packages': [
                {'id': 'provider', 'name': 'permesh-provider-github', 'version': '0.1.0',
                 'manifest_path': str(self.root / 'Cargo.toml'), 'license': 'MIT OR Apache-2.0', 'license_file': None},
                {'id': 'dependency', 'name': 'dependency', 'version': '1.0.0', 'source': 'registry+https://github.com/rust-lang/crates.io-index',
                 'manifest_path': str(dependency / 'Cargo.toml'), 'license': 'MIT', 'license_file': None}],
            'resolve': {'nodes': [
                {'id': 'provider', 'deps': [{'pkg': 'dependency', 'dep_kinds': [{'kind': None}]}]},
                {'id': 'dependency', 'deps': []}]},
        }
        fake = patch.object(package, 'cargo_metadata', return_value=self.metadata)
        fake.start()
        self.addCleanup(fake.stop)

    def binary(self, target):
        name = 'permesh-provider-github' + ('.exe' if 'windows' in target else '')
        binary = self.root / 'target' / target / 'release' / name
        binary.parent.mkdir(parents=True, exist_ok=True)
        binary.write_bytes(native(target))
        return binary

    def test_deterministic_two_file_zip_and_bound_metadata_for_every_target(self):
        for target in package.TARGETS:
            with self.subTest(target=target):
                binary = self.binary(target)
                one = package.package(self.root, target, self.root / (target + '-one'))
                binary.touch()
                two = package.package(self.root, target, self.root / (target + '-two'))
                self.assertEqual(one.read_bytes(), two.read_bytes())
                expected = 'provider.exe' if 'windows' in target else 'provider'
                with zipfile.ZipFile(one) as archive:
                    self.assertEqual(archive.namelist(), [expected, 'LICENSE'])
                    self.assertEqual(archive.comment, b'')
                    self.assertTrue(all(not f.extra and not f.comment and not f.flag_bits & 1 for f in archive.infolist()))
                    self.assertEqual(archive.read(expected), native(target))
                    license_text = archive.read('LICENSE').decode('utf-8')
                    for text in ('Provider project dual-license text', 'Dependency license text', 'Required dependency attribution'):
                        self.assertIn(text, license_text)
                    self.assertNotIn(str(self.root), license_text)
                digest = hashlib.sha256(one.read_bytes()).hexdigest()
                self.assertEqual(one.with_name(one.name + '.sha256').read_text(), f'{digest}  {one.name}\n')
                metadata = json.loads((one.parent / 'catalog-entry.json').read_text())
                self.assertEqual(metadata['provider'], 'github')
                self.assertEqual(metadata['version'], '0.1.0')
                self.assertEqual(metadata['protocols'], [2, 3])
                self.assertEqual(metadata['target'], target)
                self.assertEqual(metadata['archive_sha256'], digest)
                self.assertEqual(metadata['executable_sha256'], hashlib.sha256(native(target)).hexdigest())
                self.assertEqual(metadata['archive_size'], one.stat().st_size)
                self.assertEqual(len(list(one.parent.iterdir())), 3)
                self.assertEqual(verify_package.verify(one, one.parent / 'catalog-entry.json'), metadata)

    def test_google_package_has_its_identity_capabilities_and_mit_notice(self):
        target = package.TARGETS[0]
        github = self.binary(target)
        github.with_name('permesh-provider-google').write_bytes(github.read_bytes())
        self.metadata['packages'][0]['name'] = 'permesh-provider-google'
        self.metadata['packages'][0]['license'] = 'MIT'
        result = package.package(self.root, target, self.root / 'google-output', provider='google')
        metadata = verify_package.verify(result, result.parent / 'catalog-entry.json')
        self.assertEqual(metadata['provider'], 'google')
        self.assertEqual(metadata['capabilities'], ['accounts', 'identities'])
        self.assertTrue(result.name.startswith('permesh-provider-google-'))

    def test_missing_notices_fail_before_creating_output(self):
        target = package.TARGETS[0]
        self.binary(target)
        for name in ('LICENSE', 'NOTICE'):
            (self.root / 'dependency' / name).unlink()
        with self.assertRaises(ValueError):
            package.package(self.root, target, self.root / 'out')
        self.assertFalse((self.root / 'out').exists())

    def test_wrong_target_binary_and_existing_output_are_rejected(self):
        target = package.TARGETS[0]
        binary = self.binary(target)
        binary.write_bytes(b'#!/bin/sh\nexit 0\n')
        with self.assertRaises(ValueError):
            package.package(self.root, target, self.root / 'out')
        self.binary(target)
        (self.root / 'out').mkdir()
        (self.root / 'out' / 'keep').write_text('keep')
        with self.assertRaises(FileExistsError):
            package.package(self.root, target, self.root / 'out')
        self.assertEqual((self.root / 'out' / 'keep').read_text(), 'keep')

    def test_symlink_notice_and_external_license_file_are_rejected(self):
        target = package.TARGETS[0]
        self.binary(target)
        license_file = self.root / 'dependency' / 'LICENSE'
        license_file.unlink()
        try:
            license_file.symlink_to(self.root / 'LICENSE')
        except OSError:
            self.skipTest('symlinks unavailable')
        with self.assertRaises(ValueError):
            package.package(self.root, target, self.root / 'out')
        license_file.unlink()
        self.metadata['packages'][1]['license_file'] = '../LICENSE'
        with self.assertRaises(ValueError):
            package.package(self.root, target, self.root / 'out')

    def test_nested_license_directories_and_suffix_names_are_preserved(self):
        target = package.TARGETS[0]
        self.binary(target)
        licenses = self.root / 'dependency' / 'LICENSES'
        licenses.mkdir()
        (licenses / 'MIT.txt').write_text('Nested required license text')
        (self.root / 'dependency' / 'THIRD-PARTY-NOTICES.txt').write_text('Third-party required attribution')
        result = package.package(self.root, target, self.root / 'out')
        with zipfile.ZipFile(result) as archive:
            self.assertIn(b'Nested required license text', archive.read('LICENSE'))
            self.assertIn(b'Third-party required attribution', archive.read('LICENSE'))

    def test_required_git_workspace_license_is_included(self):
        target = package.TARGETS[0]
        self.binary(target)
        git_root = self.root / 'sdk-source'
        crate = git_root / 'crates' / 'sdk'
        crate.mkdir(parents=True)
        (git_root / 'Cargo.toml').write_text('[workspace]\nmembers=["crates/sdk"]\n')
        (git_root / 'LICENSE').write_text('Required SDK workspace license')
        (crate / 'Cargo.toml').write_text('[package]\nname="dependency"\n')
        dep = self.metadata['packages'][1]
        dep['manifest_path'] = str(crate / 'Cargo.toml')
        dep['source'] = 'git+https://github.com/NIPE-Solutions/permesh?rev=' + 'a' * 40 + '#' + 'a' * 40
        result = package.package(self.root, target, self.root / 'out')
        with zipfile.ZipFile(result) as archive:
            self.assertIn(b'Required SDK workspace license', archive.read('LICENSE'))


if __name__ == '__main__':
    unittest.main()
