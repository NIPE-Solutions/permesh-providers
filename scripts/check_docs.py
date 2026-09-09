#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Offline local-link check for Git-tracked Markdown (working-tree contents).

Paths and directory prefixes must match Git's exact tracked case, even on macOS.
Checks inline/image links, full/collapsed/defined-shortcut references, HTML href/src,
GitHub-style heading fragments (single-line ATX/setext and duplicate headings), and explicit
HTML id/name anchors. Ignores fenced/indented code, inline code, and HTML comments.
External schemes and protocol-relative URLs are never fetched. Queries are ignored;
fragments are checked only on Markdown files (not source-line/binary/directory URLs).
This intentionally targets repository Markdown, not a complete CommonMark renderer.
"""
import argparse
import html
from html.parser import HTMLParser
import posixpath
import re
import subprocess
from pathlib import Path
from urllib.parse import unquote, urlsplit

LABEL = re.compile(r"(?<!\\)\[((?:\\.|[^\[\]\\]|\[[^\[\]]*\])*)\]")
DEFINITION = re.compile(r"^ {0,3}\[(?!\^)([^\]\n]+)\]:[ \t]*(?:\n[ \t]*)?", re.M)


def blank(text):
    return "".join("\n" if char == "\n" else " " for char in text)


def prose(text):
    text = re.sub(r"<!--.*?-->", lambda match: blank(match[0]), text, flags=re.S)
    fence = None
    lines = []
    for line in text.splitlines(keepends=True):
        marker = re.match(r"^ {0,3}(`{3,}|~{3,})(.*)$", line)
        if fence:
            if marker and marker[1][0] == fence[0] and len(marker[1]) >= len(fence) and not marker[2].strip():
                fence = None
            lines.append(blank(line))
        elif marker:
            fence = marker[1]
            lines.append(blank(line))
        elif line.startswith(("    ", "\t")):
            lines.append(blank(line))
        else:
            lines.append(line)
    return "".join(lines)


def label_key(label):
    return " ".join(label.split()).casefold()


def destination(text, start):
    """Read an angle destination or an escaped/balanced-parenthesis URL token."""
    while start < len(text) and text[start].isspace():
        start += 1
    if start < len(text) and text[start] == "<":
        end = text.find(">", start + 1)
        if end != -1 and "\n" not in text[start:end]:
            return text[start + 1:end], end + 1
        return None
    end, depth = start, 0
    while end < len(text):
        char = text[end]
        if char == "\\" and end + 1 < len(text):
            end += 2
            continue
        if char.isspace() or (char == ")" and depth == 0):
            break
        if char == "(":
            depth += 1
        elif char == ")":
            depth -= 1
        end += 1
    return (text[start:end], end) if depth == 0 else None


class HtmlLinks(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.links, self.anchors = [], set()

    def handle_starttag(self, tag, attrs):
        for key, value in attrs:
            if value is not None and key in ("href", "src"):
                self.links.append((self.getpos()[0], value))
            if value is not None and (key == "id" or (tag == "a" and key == "name")):
                self.anchors.add(value)


def anchors(text):
    parsed = HtmlLinks()
    parsed.feed(text)
    result, used = parsed.anchors, set()
    lines = text.splitlines()
    for index, line in enumerate(lines):
        heading = re.match(r"^ {0,3}#{1,6}[ \t]+(.*?)(?:[ \t]+#+[ \t]*)?$", line)
        title = heading[1] if heading else None
        if title is None and index + 1 < len(lines) and line.strip() and re.match(r"^ {0,3}(?:=+|-+)[ \t]*$", lines[index + 1]):
            title = line.strip()
        if title is None:
            continue
        title = re.sub(r"!?\[([^\]]+)\]\([^)]*\)", r"\1", title)
        title = re.sub(r"<[^>]*>", "", title)
        title = html.unescape(title).replace("`", "")
        slug = re.sub(r"[^\w\s-]", "", title.lower())
        slug = re.sub(r"\s", "-", slug)
        unique, suffix = slug, 0
        while unique in used:
            suffix += 1
            unique = f"{slug}-{suffix}"
        used.add(unique)
        result.add(unique)
    return result


def links(text):
    text = re.sub(r"(`+)(.+?)\1", lambda match: blank(match[0]), text, flags=re.S)
    references, found, errors = {}, [], []
    spans = []
    for match in DEFINITION.finditer(text):
        target = destination(text, match.end())
        if target:
            references.setdefault(label_key(match[1]), target[0])
            found.append((text.count("\n", 0, match.start()) + 1, target[0]))
            end = text.find("\n", target[1])
            spans.append((match.start(), len(text) if end == -1 else end))
    for start, end in reversed(spans):
        text = text[:start] + blank(text[start:end]) + text[end:]
    parsed = HtmlLinks()
    parsed.feed(text)
    found.extend(parsed.links)
    consumed = 0
    for match in LABEL.finditer(text):
        if match.start() < consumed:
            continue
        end = match.end()
        line = text.count("\n", 0, match.start()) + 1
        if text[end:end + 1] == "(":
            target = destination(text, end + 1)
            if target:
                # A title is optional; do not treat arbitrary bracketed prose as a link.
                tail = re.match(r'''\s*(?:"(?:\\.|[^"\\\n])*"|'(?:\\.|[^'\\\n])*'|\([^\n]*?\))?\s*\)''', text[target[1]:])
                if tail:
                    found.append((line, target[0]))
                    consumed = target[1] + tail.end()
        elif text[end:end + 1] == "[":
            reference = LABEL.match(text, end)
            if reference:
                key = label_key(reference[1] or match[1])
                if key not in references:
                    errors.append((line, f"undefined reference [{key}]"))
                consumed = reference.end()
        # Defined shortcut references are already checked at their definition.
        # Undefined shortcuts are ordinary prose in Markdown, not broken links.
    return found, errors


def check(root):
    output = subprocess.check_output(["git", "-C", str(root), "ls-files", "-z"])
    tracked = {name for name in output.decode("utf-8").split("\0") if name}
    directories = {"."}
    for name in tracked:
        parent = posixpath.dirname(name)
        while parent:
            directories.add(parent)
            parent = posixpath.dirname(parent)
    available = tracked | directories
    folded = {}
    for name in sorted(available):
        folded.setdefault(name.casefold(), name)
    markdown = sorted(name for name in tracked if name.lower().endswith(".md"))
    contents, known_anchors, errors = {}, {}, []
    for name in markdown:
        try:
            contents[name] = prose((root / name).read_text(encoding="utf-8"))
            known_anchors[name] = anchors(contents[name])
        except (OSError, UnicodeError) as error:
            errors.append(f"{name}:1: cannot read tracked Markdown ({type(error).__name__})")
    for source, content in contents.items():
        found, parse_errors = links(content)
        errors.extend(f"{source}:{line}: {message}" for line, message in parse_errors)
        for line, raw in found:
            raw = html.unescape(re.sub(r"\\([!\"#$%&'()*+,\-./:;<=>?@\[\]\\^_`{|}~])", r"\1", raw))
            if re.match(r"^[A-Za-z][A-Za-z0-9+.-]*:", raw) or raw.startswith("//"):
                continue
            try:
                url = urlsplit(raw)
                path, fragment = unquote(url.path, errors="strict"), unquote(url.fragment, errors="strict")
            except (ValueError, UnicodeError):
                errors.append(f"{source}:{line}: malformed local URL {raw!r}")
                continue
            target = source if not path else posixpath.normpath(
                path.lstrip("/") if path.startswith("/") else posixpath.join(posixpath.dirname(source), path)
            )
            if target not in available:
                expected = folded.get(target.casefold())
                reason = f"case mismatch; use {expected}" if expected else "missing tracked target"
                errors.append(f"{source}:{line}: {reason}: {raw}")
            elif not (root / target).exists():
                errors.append(f"{source}:{line}: tracked target is missing from working tree: {raw}")
            elif fragment and target in known_anchors and fragment not in known_anchors[target]:
                errors.append(f"{source}:{line}: missing anchor: {raw}")
    return markdown, sorted(errors)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    try:
        markdown, errors = check(args.root)
    except (OSError, subprocess.CalledProcessError, UnicodeError) as error:
        parser.exit(2, f"Cannot inspect Git-tracked documentation: {type(error).__name__}\n")
    for error in errors:
        print(error)
    print(f"Checked {len(markdown)} tracked Markdown files; {len(errors)} local-link errors.")
    return bool(errors)


if __name__ == "__main__":
    raise SystemExit(main())
