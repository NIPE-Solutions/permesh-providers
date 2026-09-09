#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Exercise the actual checker against temporary Git indexes, not this repository."""
import subprocess
import tempfile
import sys
import unittest
from pathlib import Path

CHECKER = Path(__file__).with_name("check_docs.py")


class DocumentationLinks(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        subprocess.run(["git", "init", "-q", str(self.root)], check=True)

    def write(self, name, text, tracked=True):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")
        if tracked:
            subprocess.run(["git", "-C", str(self.root), "add", "--", name], check=True)

    def check(self):
        return subprocess.run(
            [sys.executable, str(CHECKER), "--root", str(self.root)],
            text=True, capture_output=True, check=False,
        )

    def test_tracks_case_even_on_case_insensitive_filesystems(self):
        self.write("docs/ARCHITECTURE.md", "# Architecture\n")
        self.write("README.md", "[wrong](docs/architecture.md)\n")
        result = self.check()
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn("case mismatch", result.stdout)
        self.assertIn("docs/ARCHITECTURE.md", result.stdout)
        self.write("README.md", "[right](docs/ARCHITECTURE.md#architecture)\n")
        self.assertEqual(self.check().returncode, 0)

    def test_missing_untracked_and_outside_paths_fail(self):
        self.write("local.md", "# Untracked\n", tracked=False)
        self.write("README.md", "[missing](gone.md) [local](local.md) [outside](../outside.md)\n")
        result = self.check()
        self.assertEqual(result.returncode, 1)
        self.assertEqual(result.stdout.count("README.md:1:"), 3)

    def test_references_encoded_paths_titles_images_and_directories(self):
        self.write("docs/Space (name).md", "# Café & tools!\n")
        self.write("docs/image.svg", "<svg/>\n")
        self.write("README.md", '''[inline](docs/Space%20%28name%29.md#caf%C3%A9--tools "Title")
[angle](<docs/Space (name).md#café--tools>)
[long][Guide] [guide][] [GUIDE] ![image](docs/image.svg)
[folder](docs/) [root](/docs/image.svg?raw=1#ignored-non-markdown-fragment)
[guide]: <docs/Space (name).md> "Title"
[unused]: docs/image.svg
''')
        result = self.check()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_heading_duplicates_setext_and_explicit_anchors(self):
        self.write("README.md", '''# API `code` & [guide](#api-code--guide)
# Repeat
# Repeat
Setext heading
--------------
<a id="explicit"></a>
[one](#repeat) [two](#repeat-1) [setext](#setext-heading) [explicit](#explicit)
''')
        self.assertEqual(self.check().returncode, 0)
        self.write("broken.md", "[missing](README.md#absent)\n")
        self.assertIn("missing anchor", self.check().stdout)

    def test_code_comments_external_urls_and_plain_brackets_are_not_links(self):
        self.write("README.md", '''# Example
```md
[example](absent.md)
# Ignored heading
```
~~~markdown
[example][undefined]
~~~
    [indented](absent.md)
`[inline](absent.md)` and ``[inline](absent.md)``
<!-- [comment](absent.md) -->
[remote](https://invalid.example/missing#fragment) [mail](mailto:person@example.test)
![remote](//invalid.example/image.svg) [plain text] \\[escaped](absent.md)
''')
        result = self.check()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.write("broken.md", "[example](README.md#ignored-heading)\n")
        self.assertIn("missing anchor", self.check().stdout)

    def test_explicit_undefined_reference_and_html_link_fail(self):
        self.write("README.md", '[label][undefined]\n<a href="missing.md">missing</a>\n')
        result = self.check()
        self.assertEqual(result.returncode, 1)
        self.assertIn("undefined reference", result.stdout)
        self.assertIn("missing.md", result.stdout)

    def test_missing_destination_is_checked_with_escaped_title_quotes(self):
        self.write("README.md", r'[link](missing.md "A \"quoted\" title")' + "\n")
        result = self.check()
        self.assertEqual(result.returncode, 1)
        self.assertIn("missing tracked target", result.stdout)

    def test_footnote_prose_is_not_a_link_definition(self):
        self.write("README.md", "A note[^note].\n\n[^note]: This is ordinary prose with [a link](target.md).\n")
        self.write("target.md", "# Target\n")
        self.assertEqual(self.check().returncode, 0)
        self.write("target.md", "# Target\n[bad](missing.md)\n")
        self.assertEqual(self.check().returncode, 1)

    def test_untracked_markdown_is_not_scanned_but_deleted_targets_fail(self):
        self.write("draft.md", "[draft](missing.md)\n", tracked=False)
        self.write("target.md", "# Target\n")
        self.write("README.md", "[target](target.md)\n")
        self.assertEqual(self.check().returncode, 0)
        (self.root / "target.md").unlink()
        self.assertEqual(self.check().returncode, 1)

    def test_inline_code_in_heading_keeps_anchor_text(self):
        self.write("README.md", "# Use `one` and **two**\n[heading](#use-one-and-two)\n")
        self.assertEqual(self.check().returncode, 0)

    def test_balanced_parentheses_and_fragment_only_links(self):
        self.write("docs/guide(one).md", "# Here\n[local](#here)\n")
        self.write("README.md", "[guide](docs/guide(one).md#here 'title')\n")
        self.assertEqual(self.check().returncode, 0)


if __name__ == "__main__":
    unittest.main()
