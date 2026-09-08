"""Pinned source notices are offline, version-specific distribution inputs."""
import json
from pathlib import Path
import shutil
import tempfile
import unittest
import provider_notices


class SupplementTests(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name).resolve()
        source = Path(__file__).resolve().parents[1]
        shutil.copytree(source / 'third-party', self.root / 'third-party')
        self.entry = json.loads((self.root / 'third-party/notice-supplements.json').read_text())[0]
        e = self.entry
        self.lock = {'name': e['name'], 'version': e['version'], 'source': e['source'], 'checksum': e['checksum']}
        self.write_lock()
        self.directory = self.root / 'dependency'
        self.directory.mkdir()
        (self.directory / 'Cargo.toml').write_text('[package]\nname="base64-simd"\n')
        self.vcs = {'git': {'sha1': e['upstream_revision']}, 'path_in_vcs': e['upstream_path']}
        self.write_vcs()
        self.package = {key: e[key] for key in ('name', 'version', 'source', 'license', 'repository')}
        self.package['manifest_path'] = str(self.directory / 'Cargo.toml')
        (self.root / 'LICENSE').write_text('Project notice')

    def write_lock(self):
        (self.root / 'Cargo.lock').write_text('[[package]]\n' + ''.join(f'{key}={json.dumps(value)}\n' for key, value in self.lock.items()))

    def write_vcs(self):
        (self.directory / '.cargo_vcs_info.json').write_text(json.dumps(self.vcs))

    def test_exact_reviewed_source_includes_upstream_text_in_bundle(self):
        provider = {'id': 'provider', 'name': 'permesh-provider-aws', 'version': '0.1.0', 'manifest_path': str(self.root / 'Cargo.toml'), 'license': 'MIT'}
        (self.root / 'Cargo.toml').write_text('[workspace]\n')
        dependency = dict(self.package, id='dependency')
        metadata = {'packages': [provider, dependency], 'resolve': {'nodes': [
            {'id': 'provider', 'deps': [{'pkg': 'dependency', 'dep_kinds': [{'kind': None}]}]},
            {'id': 'dependency', 'deps': []}]}}
        output = provider_notices.bundle(self.root, metadata, 'aws')
        self.assertIn(b'Copyright (c) 2021 Nugine', output)
        self.assertNotIn(str(self.root).encode(), output)

    def test_unreviewed_version_source_spdx_and_checksum_fail_closed(self):
        for key, value in [('version', '0.8.1'), ('source', 'registry+https://unreviewed.invalid'), ('license', 'Apache-2.0'), ('repository', 'https://unreviewed.invalid')]:
            with self.subTest(key=key), self.assertRaises(ValueError):
                provider_notices.notice_files(dict(self.package, **{key: value}), self.root)
        self.lock['checksum'] = '0' * 64
        self.write_lock()
        with self.assertRaises(ValueError):
            provider_notices.notice_files(self.package, self.root)

    def test_changed_upstream_revision_or_notice_digest_is_rejected(self):
        self.vcs['git']['sha1'] = '0' * 40
        self.write_vcs()
        with self.assertRaises(ValueError):
            provider_notices.notice_files(self.package, self.root)
        self.vcs['git']['sha1'] = self.entry['upstream_revision']
        self.write_vcs()
        (self.root / self.entry['notice_file']).write_text('Substituted notice')
        with self.assertRaises(ValueError):
            provider_notices.notice_files(self.package, self.root)
