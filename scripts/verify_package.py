"""Verify candidate ZIP metadata and the narrow Permesh package profile without executing it."""
import argparse
import hashlib
import json
from pathlib import Path
import stat
import struct
import zipfile

from package_provider import PROVIDERS, MAX_ARCHIVE_BYTES, TARGETS, native_matches
from provider_notices import MAX_NOTICE_BYTES, regular_bytes


def verify(archive, entry):
    archive, entry = Path(archive), Path(entry)
    release = json.loads(regular_bytes(entry, 16 * 1024))
    fields = {'provider', 'version', 'target', 'capabilities', 'protocols', 'archive_sha256', 'executable_sha256', 'archive_size'}
    if not isinstance(release, dict) or set(release) not in (fields, fields | {'discovery_protocol'}):
        raise ValueError('unexpected catalog entry fields')
    negotiated = 'discovery_protocol' in release
    if negotiated and release['discovery_protocol'] != 'negotiated_v1':
        raise ValueError('unsupported candidate discovery contract')
    protocols = release['protocols']
    if not isinstance(protocols, list) or any(type(version) is not int for version in protocols) or protocols != ([3] if negotiated else [2, 3]):
        raise ValueError('unexpected candidate protocol families')
    target = release['target']
    if target not in TARGETS or release['provider'] not in PROVIDERS or release['capabilities'] != PROVIDERS[release['provider']]:
        raise ValueError('unexpected catalog release identity')
    if archive.name != f"permesh-provider-{release['provider']}-{release['version']}-{target}.zip":
        raise ValueError('archive name differs from release metadata')
    data = regular_bytes(archive, MAX_ARCHIVE_BYTES)
    digest = hashlib.sha256(data).hexdigest()
    if digest != release['archive_sha256'] or len(data) != release['archive_size']:
        raise ValueError('archive digest or size mismatch')
    checksum = regular_bytes(archive.with_name(archive.name + '.sha256'), 1024).decode('ascii')
    if checksum != f'{digest}  {archive.name}\n':
        raise ValueError('checksum sidecar mismatch')
    if len(data) < 22 or data[-22:-18] != b'PK\x05\x06':
        raise ValueError('archive comments or trailing records are not supported')
    _, disk, central_disk, disk_count, count, size, offset, comment_len = struct.unpack('<4s4H2IH', data[-22:])
    if (disk, central_disk, disk_count, count, comment_len) != (0, 0, 2, 2, 0) or size > 8192 or offset + size != len(data) - 22:
        raise ValueError('ZIP central directory layout is invalid')
    binary_name = 'provider.exe' if 'windows' in target else 'provider'
    with zipfile.ZipFile(archive) as stream:
        if stream.namelist() != [binary_name, 'LICENSE'] or stream.comment:
            raise ValueError('ZIP must contain exactly the executable and LICENSE')
        cursor = 0
        for member in stream.infolist():
            mode = member.external_attr >> 16
            if member.extra or member.comment or member.flag_bits & ~0x800 or member.header_offset != cursor:
                raise ValueError('ZIP extensions, encryption or noncontiguous entries are unsupported')
            if not stat.S_ISREG(mode) or mode & 0o7000 or member.compress_type not in (zipfile.ZIP_STORED, zipfile.ZIP_DEFLATED):
                raise ValueError('ZIP entries require ordinary regular files')
            limit = MAX_NOTICE_BYTES if member.filename == 'LICENSE' else MAX_ARCHIVE_BYTES
            if member.file_size > limit:
                raise ValueError('ZIP member exceeds its limit')
            header = data[cursor:cursor + 30]
            if len(header) != 30 or header[:4] != b'PK\x03\x04':
                raise ValueError('ZIP local header is invalid')
            name_len, extra_len = struct.unpack_from('<HH', header, 26)
            if extra_len or data[cursor + 30:cursor + 30 + name_len] != member.filename.encode('ascii'):
                raise ValueError('ZIP local filename or extra fields are invalid')
            cursor += 30 + name_len + member.compress_size
            content = stream.read(member)
            if len(content) != member.file_size:
                raise ValueError('ZIP output length mismatch')
            if member.filename == binary_name:
                if hashlib.sha256(content).hexdigest() != release['executable_sha256'] or not native_matches(content, target):
                    raise ValueError('native executable digest or target mismatch')
            elif not content or len(content) > MAX_NOTICE_BYTES:
                raise ValueError('license bundle is missing or oversized')
        if cursor != offset:
            raise ValueError('unexpected bytes before central directory')
    return release


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('archive', type=Path)
    parser.add_argument('--entry', type=Path, required=True)
    arguments = parser.parse_args()
    verify(arguments.archive, arguments.entry)
    print('Candidate bytes, checksums, target and strict ZIP layout verified.')
