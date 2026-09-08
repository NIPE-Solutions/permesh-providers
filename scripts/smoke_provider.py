"""Exercise the compiled provider's describe protocol offline, without credentials."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile

from package_provider import PROVIDERS


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError('provider response contains a duplicate field')
        result[key] = value
    return result


def smoke(binary, provider='github'):
    if provider not in PROVIDERS:
        raise ValueError('unsupported provider')
    capabilities_expected = PROVIDERS[provider]
    binary = Path(binary).resolve(strict=True)
    environment = {key: os.environ[key] for key in ('SystemRoot',) if key in os.environ}
    handshake = {'protocol': 3, 'id': 'handshake', 'method': 'handshake', 'instance': f'{provider}-main'}
    describe = {'protocol': 3, 'id': 'describe', 'method': 'describe'}
    payload = ''.join(json.dumps(frame, separators=(',', ':')) + '\n' for frame in (handshake, describe))
    with tempfile.TemporaryDirectory(prefix='permesh-provider-smoke-') as directory:
        result = subprocess.run([str(binary)], input=payload.encode('utf-8'), cwd=directory,
                                env=environment, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                timeout=15, check=True)
        if result.stderr or len(result.stdout) > 128 * 1024 or not result.stdout.endswith(b'\n'):
            raise ValueError('unexpected provider description output')
        frames = [json.loads(line, object_pairs_hook=unique_object) for line in result.stdout.splitlines()]
        if len(frames) != 2:
            raise ValueError('description must contain handshake, one terminal and EOF')
        expected = {'protocol': 3, 'id': 'handshake', 'event': 'handshake', 'provider': provider,
                    'capabilities': capabilities_expected, 'draft': True}
        actual = dict(frames[0])
        capabilities = actual.pop('capabilities', [])
        expected.pop('capabilities')
        if actual != expected or sorted(capabilities) != sorted(capabilities_expected):
            raise ValueError('provider handshake differs from release metadata')
        terminal = dict(frames[1])
        spec = terminal.pop('spec', {})
        if terminal != {'protocol': 3, 'id': 'describe', 'event': 'setup'} or spec.get('schema_version') != 1:
            raise ValueError('provider setup terminal is invalid')
        fields = {field['key']: field for step in spec['steps'] for field in step['fields']}
        if provider == 'github' and fields.get('organizations', {}).get('input', {}).get('type') != 'string_list':
            raise ValueError('GitHub setup is missing organization selection')
        if fields.get('token', {}).get('input', {}).get('type') != 'credential':
            raise ValueError('GitHub setup is missing a token-reference question')
        if provider == 'cloudflare' and fields.get('account_id', {}).get('input', {}).get('type') != 'text':
            raise RuntimeError('Cloudflare setup missing account ID')
        if provider == 'google' and any(fields.get(key, {}).get('input', {}).get('type') != kind for key, kind in [('customer_id', 'text'), ('auth_mode', 'choice'), ('client_id', 'text'), ('refresh_token', 'credential'), ('client_secret', 'credential')]):
            raise ValueError('Google setup is missing a required authentication question')
        if list(Path(directory).iterdir()):
            raise ValueError('description unexpectedly wrote files')
    print('Offline native description passed; no workspace or credentials supplied.')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary', type=Path)
    parser.add_argument('--provider', choices=PROVIDERS, default='github')
    args = parser.parse_args()
    smoke(args.binary, args.provider)
