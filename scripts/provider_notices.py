"""Collect source-supplied license and notice texts from a locked Cargo graph."""
import hashlib
import os
from pathlib import Path
import re
import stat
import tomllib

MAX_NOTICE_BYTES = 1024 * 1024
NOTICE_NAME = re.compile(r'(?:^|[._-])(?:licen[sc]es?|copying|notices?|copyright|unlicense)(?:[._-]|$)', re.I)
NOTICE_DIRECTORIES = {'licenses', 'licences', 'legal', 'notices'}


def regular_bytes(path, limit):
    path = Path(path).absolute()
    for ancestor in (path, *path.parents):
        if ancestor.is_symlink() or ancestor.is_junction():
            raise ValueError('package inputs must not contain links or junctions')
    metadata = path.stat()
    if not stat.S_ISREG(metadata.st_mode) or metadata.st_size > limit:
        raise ValueError('package input is not a bounded regular file')
    with path.open('rb') as stream:
        data = stream.read(limit + 1)
    if len(data) > limit:
        raise ValueError('package input exceeds its limit')
    return data


def workspace_license_root(package, directory, root):
    if package.get('source') is None and (directory == root or root in directory.parents):
        return root
    source = package.get('source') or ''
    if not source.startswith('git+https://github.com/NIPE-Solutions/permesh?rev='):
        return None
    # Git SDK crates inherit the monorepo's dual license. Stop at its workspace
    # manifest, not at an arbitrary file elsewhere on the build machine.
    for parent in list(directory.parents)[:4]:
        manifest = parent / 'Cargo.toml'
        if manifest.is_file():
            parsed = tomllib.loads(regular_bytes(manifest, MAX_NOTICE_BYTES).decode('utf-8'))
            if 'workspace' in parsed:
                return parent
    raise ValueError('pinned SDK workspace license root is missing')


def notice_files(package, root):
    directory = Path(package['manifest_path']).absolute().parent
    regular_bytes(directory / 'Cargo.toml', MAX_NOTICE_BYTES)
    found = set()
    checked = 0
    for current, directories, files in os.walk(directory, followlinks=False):
        current = Path(current)
        if len(current.relative_to(directory).parts) > 24:
            raise ValueError('dependency notice tree exceeds its depth limit')
        directories[:] = sorted(d for d in directories if d not in ('.git', 'target', '.codegraph', '__pycache__'))
        for name in directories:
            child = current / name
            if child.is_symlink() or child.is_junction():
                raise ValueError('dependency source contains a linked directory')
        for name in sorted(files):
            checked += 1
            if checked > 30000:
                raise ValueError('dependency notice tree exceeds its file limit')
            if NOTICE_NAME.search(name) or any(part.lower() in NOTICE_DIRECTORIES for part in current.relative_to(directory).parts):
                found.add(current / name)
    explicit = package.get('license_file')
    if explicit:
        relative = Path(explicit)
        if '..' in relative.parts:
            raise ValueError('dependency license-file leaves its package')
        explicit_path = relative if relative.is_absolute() else directory / relative
        if explicit_path != directory and directory not in explicit_path.parents:
            raise ValueError('dependency license-file leaves its package')
        found.add(explicit_path)
    workspace = workspace_license_root(package, directory, root)
    if workspace is not None:
        found.update(path for path in workspace.iterdir() if path.is_file() and NOTICE_NAME.search(path.name))
    if not found:
        raise ValueError(f"missing source license/notice files: {package['name']} {package['version']}")
    return sorted(found), directory, workspace


def reachable(metadata, provider='github'):
    packages = {package['id']: package for package in metadata['packages']}
    roots = [p['id'] for p in metadata['packages'] if p['name'] == f'permesh-provider-{provider}']
    if len(roots) != 1 or metadata.get('resolve') is None:
        raise ValueError('provider dependency graph is missing or ambiguous')
    nodes = {node['id']: node for node in metadata['resolve']['nodes']}
    pending, seen = list(roots), set()
    while pending:
        package_id = pending.pop()
        if package_id in seen:
            continue
        if len(seen) >= 4096 or package_id not in packages or package_id not in nodes:
            raise ValueError('provider dependency graph is invalid or oversized')
        seen.add(package_id)
        for dependency in nodes[package_id]['deps']:
            if any(kind['kind'] in (None, 'build') for kind in dependency['dep_kinds']):
                pending.append(dependency['pkg'])
    return sorted((packages[key] for key in seen), key=lambda p: (p['name'], p['version'], p['id']))


def bundle(root, metadata, provider='github'):
    root = Path(root)
    project = regular_bytes(root / 'LICENSE', MAX_NOTICE_BYTES).decode('utf-8')
    sections = ['Permesh provider distribution licenses and notices', '', project.rstrip(), '',
                'Dependency source licenses and notices',
                'Each dependency below refers to the complete deduplicated text blocks that follow.', '']
    texts = {}
    identities = set()
    for package in reachable(metadata, provider):
        identity = (package['name'], package['version'])
        if identity in identities:
            raise ValueError('different dependency sources share a name and version')
        identities.add(identity)
        if not package.get('license') and not package.get('license_file'):
            raise ValueError('dependency license metadata is missing')
        paths, directory, workspace = notice_files(package, root)
        sections += [f"{package['name']} {package['version']}", f"SPDX: {package.get('license') or 'see source license-file'}"]
        for path in paths:
            content = regular_bytes(path, MAX_NOTICE_BYTES).decode('utf-8').replace('\r\n', '\n').rstrip() + '\n'
            if not content.strip():
                raise ValueError('dependency notice file is empty')
            digest = hashlib.sha256(content.encode('utf-8')).hexdigest()
            texts[digest] = content
            base = directory if path.is_relative_to(directory) else workspace
            if base is None:
                raise ValueError('dependency notice path is outside its source')
            sections.append(f"  {path.relative_to(base).as_posix()} -> SHA-256 {digest}")
        sections.append('')
    for digest, content in sorted(texts.items()):
        sections += [f'----- BEGIN NOTICE SHA-256 {digest} -----', content.rstrip(),
                     f'----- END NOTICE SHA-256 {digest} -----', '']
    result = ('\n'.join(sections) + '\n').encode('utf-8')
    if len(result) > MAX_NOTICE_BYTES:
        raise ValueError('combined license exceeds the provider package limit')
    return result
