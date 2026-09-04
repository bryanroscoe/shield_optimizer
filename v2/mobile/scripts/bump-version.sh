#!/usr/bin/env bash
# Set the ATV Optimizer mobile version in every place that carries it, and print
# the Android versionCode Tauri will derive from it.
#
# Usage:  mobile/scripts/bump-version.sh 0.2.0
#         mobile/scripts/bump-version.sh 0.2.0-beta.1
#
# Files written:
#   mobile/package.json                 -> .version
#   mobile/src-tauri/tauri.conf.json    -> .version   (source of truth for Android)
#   mobile/src-tauri/Cargo.toml         -> [package] version
#   v2/Cargo.lock                       -> via `cargo update -p atv-optimizer-mobile --offline`
#
# This script does NOT commit, tag, or push. Mobile has no release tag namespace
# yet; see mobile/RELEASE.md.
#
# Android versionCode
# -------------------
# `tauri android build` regenerates gen/android/app/tauri.properties (gitignored)
# with tauri.android.versionName / tauri.android.versionCode, and
# app/build.gradle.kts reads both from it. versionName is the tauri.conf.json
# version verbatim. versionCode, when bundle.android.versionCode is unset, is
#
#     versionCode = major * 1000000 + minor * 1000 + patch
#
# (tauri-utils AndroidConfig::version_code doc comment; verified against the
# current tauri.properties: 0.1.0 -> 1000). Pre-release identifiers are ignored,
# so 0.2.0-beta.1 and 0.2.0 both derive 2000 — Play rejects a re-used
# versionCode, so for a pre-release track set an explicit code in
# mobile/src-tauri/tauri.conf.json:
#
#     "bundle": { "android": { "versionCode": 2001 } }
#
# The cap is 2,100,000,000. `bundle.android.autoIncrementVersionCode` is the
# other option but requires committing tauri.properties (currently gitignored).

set -euo pipefail

MOBILE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORKSPACE_DIR="$(cd "$MOBILE_DIR/.." && pwd)"

if [[ $# -ne 1 ]]; then
  echo "usage: $(basename "$0") X.Y.Z[-prerelease]" >&2
  exit 64
fi

VERSION="$1"
if [[ ! "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]]; then
  echo "error: '$VERSION' is not a semver version (X.Y.Z or X.Y.Z-prerelease)" >&2
  exit 64
fi

CORE="${VERSION%%-*}"
IFS='.' read -r MAJOR MINOR PATCH <<<"$CORE"
VERSION_CODE=$(( MAJOR * 1000000 + MINOR * 1000 + PATCH ))

python3 - "$VERSION" "$MOBILE_DIR" <<'PY'
import json
import re
import sys
from pathlib import Path

version, mobile_dir = sys.argv[1], Path(sys.argv[2])


def set_json_version(path):
    text = path.read_text()
    data = json.loads(text)
    old = data.get("version")
    if old == version:
        print(f"  {path.name}: already {version}")
        return
    # Rewrite the single "version" line so formatting and key order survive.
    new_text, count = re.subn(
        r'^(\s*"version"\s*:\s*)"[^"]*"',
        lambda m: m.group(1) + json.dumps(version),
        text,
        count=1,
        flags=re.MULTILINE,
    )
    if count != 1:
        sys.exit(f"error: could not find a top-level \"version\" in {path}")
    json.loads(new_text)
    path.write_text(new_text)
    print(f"  {path.name}: {old} -> {version}")


def set_cargo_version(path):
    text = path.read_text()
    new_text, count = re.subn(
        r'(?m)^version\s*=\s*"[^"]*"', f'version = "{version}"', text, count=1
    )
    if count != 1:
        sys.exit(f"error: could not find a [package] version in {path}")
    path.write_text(new_text)
    print(f"  {path.name}: -> {version}")


set_json_version(mobile_dir / "package.json")
set_json_version(mobile_dir / "src-tauri" / "tauri.conf.json")
set_cargo_version(mobile_dir / "src-tauri" / "Cargo.toml")
PY

echo "Refreshing Cargo.lock"
if ! (cd "$WORKSPACE_DIR" && cargo update -p atv-optimizer-mobile --offline 2>&1 | sed 's/^/  /'); then
  echo "  offline update failed; retrying online" >&2
  (cd "$WORKSPACE_DIR" && cargo update -p atv-optimizer-mobile)
fi

cat <<SUMMARY

version      $VERSION
versionName  $VERSION
versionCode  $VERSION_CODE   (major*1000000 + minor*1000 + patch)

Next:
  1. Review the diff (package.json, tauri.conf.json, Cargo.toml, ../Cargo.lock).
  2. If this is a pre-release on a track that already shipped $VERSION_CODE, set
     bundle.android.versionCode in mobile/src-tauri/tauri.conf.json.
  3. mobile/scripts/gen-notices.sh   (refresh third-party notices)
  4. See mobile/RELEASE.md for the rest of the checklist.
SUMMARY
