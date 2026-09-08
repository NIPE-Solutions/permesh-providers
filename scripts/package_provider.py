"""Build a deterministic, unsigned provider ZIP and digest-bound catalog entry."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import stat
import subprocess
import tomllib
import zipfile

import provider_notices

TARGETS = ('aarch64-apple-darwin', 'x86_64-apple-darwin', 'x86_64-unknown-linux-gnu', 'aarch64-unknown-linux-gnu', 'x86_64-pc-windows-msvc')
CAPABILITIES = ['accounts', 'resources', 'groups', 'memberships', 'grants']
MAX_ARCHIVE_BYTES = 128 * 1024 * 1024


def cargo_metadata(root, target):
    result = subprocess.run(['cargo', 'metadata', '--format-version', '1', '--locked', '--filter-platform', target],
                            cwd=root, check=True, stdout=subprocess.PIPE, timeout=180)
    if len(result.stdout) > 32 * 1024 * 1024:
        raise ValueError('Cargo metadata exceeds its limit')
    return json.loads(result.stdout)


def native_matches(data, target):
    def number(offset, length):
        return int.from_bytes(data[offset:offset + length], 'little') if offset + length <= len(data) else None
    if target.endswith('linux-gnu'):
        return (len(data) >= 64 and data.startswith(b'\x7fELF\x02\x01\x01') and number(16, 2) in (2, 3)
                and number(18, 2) == (183 if target.startswith('aarch64') else 62)
                and number(20, 4) == 1 and number(52, 2) == 64)
    if target.endswith('apple-darwin'):
        return (len(data) >= 32 and data.startswith(b'\xcf\xfa\xed\xfe') and number(12, 4) == 2
                and number(4, 4) == (0x0100000c if target.startswith('aarch64') else 0x01000007))
    if target == 'x86_64-pc-windows-msvc':
        offset = number(60, 4)
        return (data.startswith(b'MZ') and offset is not None and len(data) >= offset + 26
                and data[offset:offset + 4] == b'PE\0\0' and number(offset + 4, 2) == 0x8664
                and number(offset + 20, 2) >= 112 and number(offset + 22, 2) & 2 != 0
                and number(offset + 22, 2) & 0x2000 == 0 and number(offset + 24, 2) == 0x20b)
    return False


def package(root, target, output):
    if target not in TARGETS:
        raise ValueError('unsupported target')
    root = Path(root).resolve(strict=True)
    manifest = tomllib.loads(provider_notices.regular_bytes(root / 'Cargo.toml', 1024 * 1024).decode('utf-8'))
    version = manifest['workspace']['package']['version']
    if not isinstance(version, str) or re.fullmatch(r'(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)', version) is None:
        raise ValueError('provider release requires an exact stable semantic version')
    executable = 'permesh-provider-github' + ('.exe' if 'windows' in target else '')
    binary = provider_notices.regular_bytes(root / 'target' / target / 'release' / executable, MAX_ARCHIVE_BYTES)
    if not native_matches(binary, target):
        raise ValueError('provider executable header does not match the target')
    license_bytes = provider_notices.bundle(root, cargo_metadata(root, target))
    output = Path(output).absolute()
    if any(parent.is_symlink() or parent.is_junction() for parent in (output, *output.parents)):
        raise ValueError('package output must not contain links or junctions')
    output.mkdir()
    archive = output / f'permesh-provider-github-{version}-{target}.zip'
    archive_binary = 'provider.exe' if 'windows' in target else 'provider'
    with archive.open('xb') as raw:
        with zipfile.ZipFile(raw, 'w', compression=zipfile.ZIP_DEFLATED, compresslevel=9, allowZip64=False) as stream:
            for name, data, mode in [(archive_binary, binary, 0o755), ('LICENSE', license_bytes, 0o644)]:
                info = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
                info.create_system = 3
                info.external_attr = (stat.S_IFREG | mode) << 16
                info.extra = b''
                info.comment = b''
                stream.writestr(info, data, compress_type=zipfile.ZIP_DEFLATED, compresslevel=9)
    archive_bytes = provider_notices.regular_bytes(archive, MAX_ARCHIVE_BYTES)
    digest = hashlib.sha256(archive_bytes).hexdigest()
    with archive.with_name(archive.name + '.sha256').open('x', encoding='ascii', newline='\n') as stream:
        stream.write(f'{digest}  {archive.name}\n')
    release = {'provider': 'github', 'version': version, 'target': target, 'capabilities': CAPABILITIES,
               'protocols': [2, 3], 'archive_sha256': digest,
               'executable_sha256': hashlib.sha256(binary).hexdigest(), 'archive_size': len(archive_bytes)}
    with (output / 'catalog-entry.json').open('x', encoding='utf-8', newline='\n') as stream:
        json.dump(release, stream, indent=2, sort_keys=True)
        stream.write('\n')
    return archive


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument('--target', choices=TARGETS, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    print(package(args.root, args.target, args.output))
