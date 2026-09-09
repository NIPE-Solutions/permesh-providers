"""Keep AWS offline smoke aligned with the explicit caller-role setup contract."""
import copy
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from package_provider import PROVIDERS
from smoke_provider import smoke


class AwsSetupSmokeTests(unittest.TestCase):
    def fields(self):
        return [dict(key=key, input={'type': kind}, required=required)
                for key, kind, required in [
                    ('account_id', 'text', True), ('region', 'text', True),
                    ('access_key_id', 'credential', True),
                    ('secret_access_key', 'credential', True),
                    ('session_token', 'credential', False),
                    ('caller_role', 'text', False)]]

    def exercise(self, fields):
        frames = [dict(protocol=3, id='handshake', event='handshake', provider='aws',
                       capabilities=PROVIDERS['aws'], draft=True),
                  dict(protocol=3, id='describe', event='setup',
                       spec={'schema_version': 1, 'steps': [{'fields': fields}]})]
        response = b''.join(json.dumps(frame).encode() + b'\n' for frame in frames)
        with tempfile.TemporaryDirectory() as folder:
            binary = Path(folder) / 'provider'
            binary.touch()
            with patch('smoke_provider.subprocess.run', return_value=
                       subprocess.CompletedProcess([str(binary)], 0, response, b'')) as run:
                smoke(binary, 'aws')
            requests = [json.loads(line) for line in run.call_args.kwargs['input'].splitlines()]
            self.assertEqual([frame['method'] for frame in requests], ['handshake', 'describe'])
            self.assertTrue(all('credentials' not in frame for frame in requests))

    def test_accepts_optional_nonsecret_caller_role_and_reference_slots(self):
        self.exercise(self.fields())

    def test_rejects_missing_extra_or_retyped_caller_role(self):
        fields = self.fields()
        invalid = [fields[:-1], fields + [dict(key='extra', input={'type': 'text'}, required=False)]]
        for changes in [{'required': True}, {'input': {'type': 'credential'}}]:
            changed = copy.deepcopy(fields)
            changed[-1].update(changes)
            invalid.append(changed)
        for candidate in invalid:
            with self.subTest(fields=candidate), self.assertRaises(ValueError):
                self.exercise(candidate)


if __name__ == '__main__':
    unittest.main()
