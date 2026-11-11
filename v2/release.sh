#!/bin/bash
# Cut a v2 release. Bumps the version atomically across the release metadata files that
# carry it (tauri.conf.json, Cargo.toml, Cargo.lock, package.json), tags the commit
# `v2-VERSION`, and pushes — the GitHub Actions workflow at
# .github/workflows/v2-release.yml takes it from there to produce installers.
#
# Usage:
#   ./release.sh                  patch:  v2-0.1.0        -> v2-0.1.1
#   ./release.sh --minor          minor:  v2-0.1.5        -> v2-0.2.0
#   ./release.sh --major          major:  v2-0.9.0        -> v2-1.0.0
#   ./release.sh --beta           beta:   v2-0.1.0        -> v2-0.1.1-beta.1
#   ./release.sh --minor --beta           v2-0.1.5        -> v2-0.2.0-beta.1
#   ./release.sh --set 0.2.0      explicit: any value, e.g. 0.2.0-preview
#
# From a pre-release, with no bump flag, the base version is kept:
#   ./release.sh --beta           v2-0.2.0-beta.1 -> v2-0.2.0-beta.2
#   ./release.sh --rc             v2-0.2.0-beta.2 -> v2-0.2.0-rc.1
#   ./release.sh                  v2-0.2.0-rc.1   -> v2-0.2.0 (promote to stable)
# An explicit --patch/--minor/--major bumps from the base instead.
# The arithmetic and the MSI version encoding live in scripts/release_version.py
# (tested by scripts/test_release_scripts.py); it refuses any version Windows
# would not see as an upgrade.
#
# Pre-release tags are auto-flagged as GitHub prereleases by the workflow's
# regex (matches -(alpha|beta|rc|preview|pre)([.-]?[0-9]+)?).

set -euo pipefail

cd "$(dirname "$0")"

TAURI_CONF="src-tauri/tauri.conf.json"
CARGO_TOML="src-tauri/Cargo.toml"
CORE_CARGO_TOML="crates/core/Cargo.toml"
CARGO_LOCK="Cargo.lock"
PACKAGE_JSON="package.json"
METAINFO="flatpak/io.github.bryanroscoe.ATVOptimizer.metainfo.xml"

# --- Parse flags -------------------------------------------------------------

BUMP=""
PRE=""
EXPLICIT=""

# Piping `yes` into this script is blocked by the auto-mode classifier, and
# rightly so — these gates are the safety net. `--yes` is the sanctioned way to
# skip them deliberately, for CI or an agent that has already decided.
ASSUME_YES=0

# Ask a y/N question, or answer it automatically under --yes. Returns 0 for
# yes, 1 for no, so callers read as `confirm "..." || abort`.
confirm() {
  local prompt="$1"
  if [[ "$ASSUME_YES" == "1" ]]; then
    echo "$prompt [auto-yes]"
    return 0
  fi
  local REPLY
  read -p "$prompt " -n 1 -r; echo
  [[ $REPLY =~ ^[Yy]$ ]]
}

usage() {
  cat <<EOF
Usage: $0 [--major|--minor|--patch] [--beta|--rc|--alpha|--preview] [--set X.Y.Z] [--yes]
  bump kind:    --patch (default from a stable version) | --minor | --major
  pre-release:  --beta | --rc | --alpha | --preview
  explicit:     --set X.Y.Z[-tag]   (overrides bump kind, used verbatim)
  --yes, -y:    auto-confirm every gate (non-interactive / CI use)

Examples:
  $0                       # 0.1.0 -> 0.1.1;  0.2.0-rc.1 -> 0.2.0
  $0 --minor               # 0.1.5 -> 0.2.0
  $0 --beta                # 0.1.0 -> 0.1.1-beta.1;  0.2.0-beta.1 -> 0.2.0-beta.2
  $0 --rc                  # 0.2.0-beta.2 -> 0.2.0-rc.1
  $0 --minor --beta        # 0.1.5 -> 0.2.0-beta.1
  $0 --set 0.3.0-preview   # exact value, tagged as v2-0.3.0-preview
EOF
  exit 1
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --major) BUMP="major"; shift ;;
    --minor) BUMP="minor"; shift ;;
    --patch) BUMP="patch"; shift ;;
    --beta) PRE="beta"; shift ;;
    --rc) PRE="rc"; shift ;;
    --alpha) PRE="alpha"; shift ;;
    --preview) PRE="preview"; shift ;;
    --set) EXPLICIT="$2"; shift 2 ;;
    --yes|-y) ASSUME_YES=1; shift ;;
    -h|--help) usage ;;
    *) echo "Unknown flag: $1" >&2; usage ;;
  esac
done

# --- Sanity checks -----------------------------------------------------------

if [[ ! -f "$TAURI_CONF" ]]; then
  echo "Run this from v2/. Couldn't find $TAURI_CONF." >&2
  exit 1
fi

if [[ -n "$(git status --porcelain)" ]]; then
  echo "Working tree has uncommitted changes:" >&2
  git status --short >&2
  confirm "Continue anyway? (y/N)" || { echo "Aborted."; exit 1; }
fi

# --- Read current version ----------------------------------------------------

CURRENT=$(python3 -c "import json; print(json.load(open('$TAURI_CONF'))['version'])")
echo "Current version: $CURRENT"

# --- Compute the new version -------------------------------------------------

if [[ -n "$EXPLICIT" ]]; then
  NEW="$EXPLICIT"
else
  NEW=$(python3 scripts/release_version.py next "$CURRENT" ${BUMP:+--bump "$BUMP"} ${PRE:+--pre "$PRE"})
fi

# Fails on anything the MSI version can't encode (unknown pre-release type,
# pre-release number over 299, patch over 64), before anything is written.
WIX_VERSION=$(python3 scripts/release_version.py wix "$NEW")

TAG="v2-$NEW"
echo "New version:     $NEW"
echo "Tag:             $TAG"
echo "MSI version:     $WIX_VERSION"

if git rev-parse "$TAG" >/dev/null 2>&1; then
  echo "Tag $TAG already exists. Aborting." >&2
  exit 1
fi

confirm "Bump + tag? (y/N)" || { echo "Aborted."; exit 1; }

# --- Patch the version into all three files ---------------------------------

NEW="$NEW" WIX_VERSION="$WIX_VERSION" TAURI_CONF="$TAURI_CONF" CARGO_TOML="$CARGO_TOML" \
CORE_CARGO_TOML="$CORE_CARGO_TOML" CARGO_LOCK="$CARGO_LOCK" \
PACKAGE_JSON="$PACKAGE_JSON" METAINFO="$METAINFO" python3 - <<'PY'
import os
import json, re, pathlib

new = os.environ["NEW"]

# MSI version from scripts/release_version.py (see its docstring for the
# encoding); computed and validated before the confirmation prompt.
wix_version = os.environ["WIX_VERSION"]

# tauri.conf.json
p = pathlib.Path(os.environ["TAURI_CONF"])
conf = json.loads(p.read_text())
conf["version"] = new
conf.setdefault("bundle", {}).setdefault("windows", {}).setdefault("wix", {})["version"] = wix_version
p.write_text(json.dumps(conf, indent=2) + "\n")

# Cargo.toml files — keep formatting, edit only each [package].version line.
for cargo_path in [os.environ["CARGO_TOML"], os.environ["CORE_CARGO_TOML"]]:
    cargo = pathlib.Path(cargo_path).read_text()
    cargo, n = re.subn(r'(?m)^(version\s*=\s*)"[^"]+"', rf'\1"{new}"', cargo, count=1)
    assert n == 1, f"{cargo_path}: no top-level version= line found"
    pathlib.Path(cargo_path).write_text(cargo)

# Cargo.lock — find the [[package]] block whose name matches the crate and
# rewrite its version line. Without this, the next `cargo build` rewrites
# Cargo.lock and leaves the working tree dirty after a release.
lock_path = pathlib.Path(os.environ["CARGO_LOCK"])
lock = lock_path.read_text()
for package_name in ["atv-optimizer", "atv-optimizer-core"]:
    lock, n = re.subn(
        rf'(\[\[package\]\]\r?\nname = "{package_name}"\r?\nversion = )"[^"]+"',
        rf'\1"{new}"',
        lock,
        count=1,
    )
    assert n == 1, f"Cargo.lock: {package_name} package entry not found"
lock_path.write_text(lock)

# package.json
p = pathlib.Path(os.environ["PACKAGE_JSON"])
pkg = json.loads(p.read_text())
pkg["version"] = new
p.write_text(json.dumps(pkg, indent=2) + "\n")

# Flatpak metainfo — Flathub shows this list as the version history.
import datetime
p = pathlib.Path(os.environ["METAINFO"])
xml = p.read_text()
if f'<release version="{new}"' not in xml:
    kind = ' type="development"' if "-" in new else ""
    entry = (
        f'    <release version="{new}" date="{datetime.date.today().isoformat()}"{kind}>\n'
        f'      <url type="details">https://github.com/bryanroscoe/shield_optimizer/releases/tag/v2-{new}</url>\n'
        f'    </release>\n'
    )
    xml, n = re.subn(r"(<releases>\n)", lambda m: m.group(1) + entry, xml, count=1)
    assert n == 1, f"{p}: no <releases> element found"
    p.write_text(xml)
PY

git diff --stat "$TAURI_CONF" "$CARGO_TOML" "$CORE_CARGO_TOML" "$CARGO_LOCK" "$PACKAGE_JSON" "$METAINFO"

# --- Commit + tag + push -----------------------------------------------------

git add "$TAURI_CONF" "$CARGO_TOML" "$CORE_CARGO_TOML" "$CARGO_LOCK" "$PACKAGE_JSON" "$METAINFO"
git commit -m "Release $TAG"
git tag -a "$TAG" -m "Release $TAG"

if confirm "Push the tag? (this fires the build workflow) (y/N)"; then
  git push origin HEAD
  git push origin "$TAG"
  echo
  echo "✓ Pushed. Watch the build at:"
  echo "  https://github.com/$(git remote get-url origin | sed -E 's|.*github.com[:/](.*)\.git|\1|')/actions/workflows/v2-release.yml"
else
  echo
  echo "Local commit + tag created. Push when ready:"
  echo "  git push origin HEAD && git push origin $TAG"
fi
