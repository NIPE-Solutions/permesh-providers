"""Keep native candidate coverage aligned with registered packaging identities."""
from pathlib import Path
import re
import unittest

from package_provider import PROVIDERS, TARGETS


class CandidateWorkflowTests(unittest.TestCase):
    def test_every_registered_binary_is_exercised_by_both_workflows(self):
        root = Path(__file__).resolve().parents[1]
        for name in ('ci.yml', 'candidates.yml'):
            with self.subTest(workflow=name):
                workflow = (root / '.github' / 'workflows' / name).read_text()
                loops = re.findall(r'for provider in ([a-z -]+); do', workflow)
                self.assertEqual(len(loops), 1)
                self.assertEqual(set(loops[0].split()), set(PROVIDERS))
                self.assertIn('scripts/smoke_provider.py --provider "$provider"', workflow)
        candidate = (root / '.github' / 'workflows' / 'candidates.yml').read_text()
        self.assertEqual(set(re.findall(r'target: ([a-z0-9_-]+)', candidate)), set(TARGETS))


if __name__ == '__main__':
    unittest.main()
