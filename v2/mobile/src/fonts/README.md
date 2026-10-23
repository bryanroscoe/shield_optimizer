# Bundled fonts

`material-symbols-rounded.woff2` is a modified subset of Material Symbols Rounded from the
Apache-2.0 `material-symbols` npm package (currently 0.45.5). It is generated from the original
package font by `scripts/subset-material-symbols.py`; do not replace it with a hand-edited binary.

Geist and Geist Mono are the checked-in, unmodified WOFF2 assets from `geist` 1.7.2, under
SIL Open Font License 1.1. The full license and attribution remain in `THIRD-PARTY-NOTICES.md`
and the in-app notices. CSS loads these files directly; no `geist` or Next.js npm installation
is required to build them. Removing the unused npm package does not change the font bytes.
