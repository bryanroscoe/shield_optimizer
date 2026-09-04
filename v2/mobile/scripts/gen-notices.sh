#!/usr/bin/env bash
# Regenerate the third-party notices for the ATV Optimizer Android app:
#
#   mobile/THIRD-PARTY-NOTICES.md      the shippable notice document
#   mobile/src/lib/notices.generated.ts  the same text as a TS module for in-app display
#
# Sources:
#   cargo-about + mobile/about.toml    every Cargo crate linked into the APK
#   mobile/scripts/notices-header.md   hand-maintained non-Cargo components
#                                      (fonts, scrcpy server, adb_client patch, npm)
#
# Run after any dependency change, and again before a release. The script fails
# if a GPL/LGPL/AGPL dependency appears — that would block a paid release.

set -euo pipefail

MOBILE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORKSPACE_DIR="$(cd "$MOBILE_DIR/.." && pwd)"

if ! command -v cargo-about >/dev/null 2>&1; then
  echo "cargo-about not found. Install it with:" >&2
  echo "  cargo install cargo-about --locked --features cli" >&2
  exit 127
fi

VERSION="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["version"])' \
  "$MOBILE_DIR/src-tauri/tauri.conf.json")"

# The header pins npm versions by hand; warn (don't fail) when they drift.
if [[ -d "$MOBILE_DIR/node_modules" ]]; then
  for pkg in svelte @tauri-apps/api geist material-symbols; do
    installed="$(node -e "process.stdout.write(require('$MOBILE_DIR/node_modules/$pkg/package.json').version)" 2>/dev/null || true)"
    if [[ -n "$installed" ]] && ! grep -q "\`$pkg\` | $installed " "$MOBILE_DIR/scripts/notices-header.md"; then
      echo "warning: $pkg is installed at $installed but scripts/notices-header.md says otherwise" >&2
    fi
  done
fi

ABOUT_JSON="$(mktemp -t atvopt-about)"
trap 'rm -f "$ABOUT_JSON"' EXIT

echo "Inventorying Cargo dependencies (cargo-about, Android targets)"
(cd "$WORKSPACE_DIR" && cargo about generate \
  --format json \
  --config "$MOBILE_DIR/about.toml" \
  --manifest-path "$MOBILE_DIR/src-tauri/Cargo.toml" \
  --output-file "$ABOUT_JSON")

echo "Rendering notices for version $VERSION"
python3 "$MOBILE_DIR/scripts/render-notices.py" \
  "$ABOUT_JSON" \
  "$MOBILE_DIR/scripts/notices-header.md" \
  "$VERSION" \
  "$MOBILE_DIR/THIRD-PARTY-NOTICES.md" \
  "$MOBILE_DIR/src/lib/notices.generated.ts"

echo "Done. Review the diff and commit both generated files."
