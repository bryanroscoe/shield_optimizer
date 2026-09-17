#!/usr/bin/env python3
"""Rebuild the bundled Material Symbols font from src/lib/icons.ts.

The mobile app's version of this script regex-scans every .svelte file for
lowercase quoted tokens. That is harmlessly over-broad there; here it is not —
the desktop sources are full of quoted identifiers like "enabled", "missing",
"home", "settings" and "refresh" that collide with real ligature names, so the
subset would silently grow while a genuine typo stayed invisible.

Reading the explicit allowlist instead means the font contains exactly what
`IconName` permits, and a name that is not a real ligature fails the build here
rather than rendering as a word in the UI.
"""

from __future__ import annotations

import re
from pathlib import Path

try:
    from fontTools import subset
    from fontTools.ttLib import TTFont
except ImportError:
    raise SystemExit(
        "fonttools and brotli are required: python3 -m pip install fonttools brotli"
    )


ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "node_modules/material-symbols/material-symbols-rounded.woff2"
DESTINATION = ROOT / "src/fonts/material-symbols-rounded.woff2"
REGISTRY = ROOT / "src/lib/icons.ts"


def ligatures(font: TTFont) -> dict[str, str]:
    cmap = font.getBestCmap()
    glyph_chars = {glyph: chr(codepoint) for codepoint, glyph in cmap.items()}
    found: dict[str, str] = {}
    for lookup in font["GSUB"].table.LookupList.Lookup:
        for raw_subtable in lookup.SubTable:
            subtable = (
                raw_subtable.ExtSubTable if lookup.LookupType == 7 else raw_subtable
            )
            if getattr(subtable, "LookupType", 4) != 4 or not hasattr(
                subtable, "ligatures"
            ):
                continue
            for first, rows in subtable.ligatures.items():
                for row in rows:
                    glyphs = [first, *row.Component]
                    if all(glyph in glyph_chars for glyph in glyphs):
                        key = "".join(glyph_chars[glyph] for glyph in glyphs)
                        found[key] = row.LigGlyph
    return found


def requested_icons() -> list[str]:
    """The ICONS array from src/lib/icons.ts, in file order."""
    text = REGISTRY.read_text()
    match = re.search(r"export const ICONS = \[(.*?)\] as const;", text, re.S)
    if not match:
        raise SystemExit(f"Could not find the ICONS array in {REGISTRY}")
    icons = re.findall(r'"([a-z0-9_]+)"', match.group(1))
    if not icons:
        raise SystemExit("The ICONS array is empty.")
    duplicates = {name for name in icons if icons.count(name) > 1}
    if duplicates:
        raise SystemExit(f"Duplicate icon names: {', '.join(sorted(duplicates))}")
    return sorted(set(icons))


def main() -> None:
    if not SOURCE.is_file():
        raise SystemExit(f"Missing source font: {SOURCE}. Run npm install first.")

    font = TTFont(SOURCE, recalcTimestamp=False)
    available = ligatures(font)
    icons = requested_icons()

    unknown = sorted(set(icons) - set(available))
    if unknown:
        raise SystemExit(
            "These names in src/lib/icons.ts are not Material Symbols ligatures: "
            + ", ".join(unknown)
        )

    options = subset.Options()
    options.layout_features = ["rclt", "rlig"]
    options.layout_closure = False
    options.notdef_glyph = True
    options.recommended_glyphs = True
    subsetter = subset.Subsetter(options=options)
    subsetter.populate(text=" ".join(icons), glyphs=[available[icon] for icon in icons])
    subsetter.subset(font)
    font.flavor = "woff2"
    DESTINATION.parent.mkdir(parents=True, exist_ok=True)
    font.save(DESTINATION)

    rebuilt = TTFont(DESTINATION)
    missing = sorted(set(icons) - set(ligatures(rebuilt)))
    if missing:
        DESTINATION.unlink(missing_ok=True)
        raise SystemExit(
            f"Subset validation failed; missing ligatures: {', '.join(missing)}"
        )

    print(
        f"Wrote {DESTINATION.relative_to(ROOT)} with {len(icons)} icons "
        f"({DESTINATION.stat().st_size:,} bytes)."
    )


if __name__ == "__main__":
    main()
