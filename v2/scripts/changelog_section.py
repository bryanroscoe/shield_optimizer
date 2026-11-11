#!/usr/bin/env python3
"""Print one release's section from v2/CHANGELOG.md.

    changelog_section.py CHANGELOG TAG

A section starts at a `## ` heading whose first word is exactly TAG
(`## v2-2.5.0` or `## v2-2.5.0 — 2026-11-04`) and runs to the next `## `
heading. The tag is compared as a literal whole word, so `v2-2.0.0` never
matches `## v2-2.0.0-beta.15` and `v2-2.5.1` never matches `## v2-2.5.10`.

Exit 0 with the section on stdout (surrounding blank lines trimmed), or exit 3
with nothing on stdout when there is no section for TAG. Any other failure
exits 1. Stdlib only.
"""

from __future__ import annotations

import sys

NOT_FOUND = 3


def heading_tag(line: str) -> str | None:
    if not line.startswith("## "):
        return None
    words = line[3:].split()
    return words[0] if words else ""


def section(text: str, tag: str) -> str | None:
    out: list[str] | None = None
    for line in text.splitlines():
        h = heading_tag(line)
        if out is not None:
            if h is not None:
                break
            out.append(line)
        elif h == tag:
            out = []
    if out is None:
        return None
    return "\n".join(out).strip("\n")


def main(argv: list[str]) -> int:
    if len(argv) != 2 or not argv[1]:
        print("usage: changelog_section.py CHANGELOG TAG", file=sys.stderr)
        return 1
    path, tag = argv
    with open(path, encoding="utf-8") as f:
        body = section(f.read(), tag)
    if body is None:
        return NOT_FOUND
    print(body)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
