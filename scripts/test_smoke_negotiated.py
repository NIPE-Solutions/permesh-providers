"""Validate native candidate negotiation checks without live APIs or credentials."""
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from smoke_provider import smoke_negotiated
from package_provider import PROVIDERS


class NegotiatedSmokeTests(unittest.TestCase):
    def response(self, provider='github'):
        return [dict(protocol_version=1, id='handshake', event='handshake', provider=provider,
                     capabilities=PROVIDERS[provider], operations=['check', 'discover'], draft=True),
                dict(protocol_version=1, id='cancel', event='cancelled')]

    def exercise(self, frames, code=0, stderr=b''):
        calls = []
        def run(command, **kwargs):
            requests = [json.loads(line) for line in kwargs['input'].splitlines()]
            calls.append(requests)
            self.assertEqual(requests[0]['protocol_version'], 1)
            self.assertNotIn('protocol', requests[0])
            self.assertNotIn('credentials', requests[0])
            self.assertEqual(requests[1], dict(protocol_version=1, id='cancel', method='cancel'))
            return subprocess.CompletedProcess(command, code,
                b''.join(json.dumps(frame).encode() + b'\n' for frame in frames), stderr)
        with tempfile.TemporaryDirectory() as folder:
            binary = Path(folder) / 'provider'
            binary.touch()
            with patch('smoke_provider.subprocess.run', run):
                smoke_negotiated(binary, 'github')
        return calls

    def test_both_operations_are_checked_without_invocations(self):
        calls = self.exercise(self.response())
        self.assertEqual([c[0]['operation'] for c in calls], ['check', 'discover'])

    def test_rejects_mismatched_identity_capabilities_operations_and_family(self):
        for field, value in [('provider', 'other'), ('capabilities', []),
                             ('operations', ['discover']), ('protocol_version', 2)]:
            frames = self.response()
            frames[0][field] = value
            with self.subTest(field=field), self.assertRaises(ValueError):
                self.exercise(frames)
        frames = self.response()
        frames[0]['protocol'] = frames[0].pop('protocol_version')
        with self.assertRaises(ValueError):
            self.exercise(frames)

    def test_rejects_loose_scalar_types(self):
        for index, field, value in [(0, 'protocol_version', True), (0, 'protocol_version', 1.0),
                                     (0, 'draft', 1), (1, 'protocol_version', True),
                                     (1, 'protocol_version', 1.0)]:
            frames = self.response()
            frames[index][field] = value
            with self.subTest(index=index, field=field, value=value), self.assertRaises(ValueError):
                self.exercise(frames)

    def test_rejects_error_terminal_extra_output_and_diagnostics(self):
        frames = self.response()
        frames[1]['event'] = 'error'
        with self.assertRaises(ValueError):
            self.exercise(frames)
        with self.assertRaises(ValueError):
            self.exercise(self.response() * 2)
        with self.assertRaises(ValueError):
            self.exercise(self.response(), stderr=b'private')
        with self.assertRaises(ValueError):
            self.exercise(self.response(), code=2)


if __name__ == '__main__':
    unittest.main()
