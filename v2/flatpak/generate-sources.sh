#!/usr/bin/env bash
# Writes generated-sources/{cargo,node}-sources.json from v2's lockfiles: every
# crate and npm package the offline Flatpak build needs, with its checksum.
# Rerun after any Cargo.lock or package-lock.json change.
set -euo pipefail
cd "$(dirname "$0")"

# flatpak/flatpak-builder-tools master as of 2026-10-25.
TOOLS_REV=9d86d336a39b9af38b86f114b6ce5820fe260f91
OUT=generated-sources

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

python3 -m venv "$work/venv"
"$work/venv/bin/pip" install --quiet \
  "aiohttp>=3.9.5,<4" "tomlkit>=0.13.3,<1" "PyYAML>=6.0.2,<7" \
  "git+https://github.com/flatpak/flatpak-builder-tools.git@${TOOLS_REV}#subdirectory=node"
curl -fsSL -o "$work/flatpak-cargo-generator.py" \
  "https://raw.githubusercontent.com/flatpak/flatpak-builder-tools/${TOOLS_REV}/cargo/flatpak-cargo-generator.py"

# The npm generator reads node_modules when it exists; give it the lockfile alone.
mkdir -p "$work/npm" "$OUT"
cp ../package-lock.json ../package.json "$work/npm/"

"$work/venv/bin/python" "$work/flatpak-cargo-generator.py" ../Cargo.lock -o "$OUT/cargo-sources.json"
"$work/venv/bin/flatpak-node-generator" npm "$work/npm/package-lock.json" -o "$OUT/node-sources.json"

# The generator also fetches Playwright's test browsers (~500 MB). Only the
# screenshot and e2e tools use them, and their CDN has served a different zip
# than the one hashed minutes earlier, which fails the build. The npm package
# itself stays; it downloads nothing on install.
python3 - "$OUT/node-sources.json" <<'PY'
import json, sys
path = sys.argv[1]
sources = json.load(open(path))
kept = [s for s in sources if not s.get("dest", "").startswith("flatpak-node/cache/ms-playwright")]
json.dump(kept, open(path, "w"), indent=4)
print(f"dropped {len(sources) - len(kept)} Playwright browser sources")
PY
echo "wrote $OUT/cargo-sources.json and $OUT/node-sources.json"
