#!/usr/bin/env python3
"""Rebuild the bundled Material Symbols font from icons referenced by Svelte."""

from __future__ import annotations

import re
import sys
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


def ligatures(font: TTFont) -> dict[str, str]:
    cmap = font.getBestCmap()
    glyph_chars = {glyph: chr(codepoint) for codepoint, glyph in cmap.items()}
    found: dict[str, str] = {}
    for lookup in font["GSUB"].table.LookupList.Lookup:
        for raw_subtable in lookup.SubTable:
            subtable = raw_subtable.ExtSubTable if lookup.LookupType == 7 else raw_subtable
            if getattr(subtable, "LookupType", 4) != 4 or not hasattr(subtable, "ligatures"):
                continue
            for first, rows in subtable.ligatures.items():
                for row in rows:
                    glyphs = [first, *row.Component]
                    if all(glyph in glyph_chars for glyph in glyphs):
                        found["".join(glyph_chars[glyph] for glyph in glyphs)] = row.LigGlyph
    return found


def referenced_icons(available: set[str]) -> list[str]:
    tokens: set[str] = set()
    for source in (ROOT / "src").rglob("*.svelte"):
        text = source.read_text()
        tokens.update(re.findall(r'''["']([a-z][a-z0-9_]+)["']''', text))
        tokens.update(re.findall(r">\s*([a-z][a-z0-9_]+)\s*</span>", text))
    icons = sorted(tokens & available)
    if not icons:
        raise SystemExit("No Material Symbols references found in Svelte sources.")
    return icons


def main() -> None:
    if not SOURCE.is_file():
        raise SystemExit(f"Missing source font: {SOURCE}. Run npm install first.")

    font = TTFont(SOURCE, recalcTimestamp=False)
    available = ligatures(font)
    icons = referenced_icons(set(available))

    options = subset.Options()
    options.layout_features = ["rclt", "rlig"]
    options.layout_closure = False
    options.notdef_glyph = True
    options.recommended_glyphs = True
    subsetter = subset.Subsetter(options=options)
    subsetter.populate(text=" ".join(icons), glyphs=[available[icon] for icon in icons])
    subsetter.subset(font)
    font.flavor = "woff2"
    font.save(DESTINATION)

    rebuilt = TTFont(DESTINATION)
    retained = ligatures(rebuilt)
    missing = sorted(set(icons) - set(retained))
    if missing:
        DESTINATION.unlink(missing_ok=True)
        raise SystemExit(f"Subset validation failed; missing ligatures: {', '.join(missing)}")

    print(
        f"Wrote {DESTINATION.relative_to(ROOT)} with {len(icons)} icons "
        f"({DESTINATION.stat().st_size:,} bytes)."
    )


if __name__ == "__main__":
    main()
