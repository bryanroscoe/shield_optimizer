#!/bin/bash
# Cut a desktop-app release. Bumps the version atomically across the four files
# that carry it (tauri.conf.json, Cargo.toml, Cargo.lock, package.json), tags the
# commit `desktop-VERSION`, and pushes — the GitHub Actions workflow at
# .github/workflows/v2-release.yml takes it from there to produce installers.
#
# The `desktop-` prefix is the release-track namespace that keeps this app's
# tags separate from v1's PowerShell-debloater tags; the version itself is plain
# semver. (Older releases used a `v2-` prefix — the workflow still accepts both.)
#
# Usage:
#   ./release.sh                  patch:  desktop-0.1.0 -> desktop-0.1.1
#   ./release.sh --minor          minor:  desktop-0.1.5 -> desktop-0.2.0
#   ./release.sh --major          major:  desktop-0.9.0 -> desktop-1.0.0
#   ./release.sh --beta           beta:   desktop-0.1.0 -> desktop-0.1.0-beta (or beta.N)
#   ./release.sh --rc             rc:     desktop-0.1.0 -> desktop-0.1.0-rc (or -rc.N)
#   ./release.sh --set 0.2.0      explicit: any value, e.g. 0.2.0-preview
#
# Combine bump kind with a pre-release flag if needed:
#   ./release.sh --minor --beta   desktop-0.1.5 -> desktop-0.2.0-beta
#
# Pre-release tags are auto-flagged as GitHub prereleases by the workflow's
# regex (matches -(alpha|beta|rc|preview|pre)([.-]?[0-9]+)?).

# Release-track tag prefix. Keep in sync with the workflow trigger in
# .github/workflows/v2-release.yml.
TAG_PREFIX="desktop"

set -euo pipefail

cd "$(dirname "$0")"

TAURI_CONF="src-tauri/tauri.conf.json"
CARGO_TOML="src-tauri/Cargo.toml"
CARGO_LOCK="src-tauri/Cargo.lock"
PACKAGE_JSON="package.json"

# --- Parse flags -------------------------------------------------------------

BUMP="patch"
PRE=""
EXPLICIT=""
ASSUME_YES=0

# Auto-confirm a y/N gate when --yes was passed; otherwise prompt as usual.
# Returns success (proceed) / failure (the caller aborts).
confirm() {
  local prompt="$1"
  if [[ "$ASSUME_YES" == "1" ]]; then
    echo "$prompt [auto-yes]"
    return 0
  fi
  read -p "$prompt " -n 1 -r; echo
  [[ $REPLY =~ ^[Yy]$ ]]
}

usage() {
  cat <<EOF
Usage: $0 [--major|--minor|--patch] [--beta|--rc|--alpha] [--set X.Y.Z] [--yes]
  bump kind:    --patch (default) | --minor | --major
  pre-release:  --beta | --rc | --alpha | --preview
  explicit:     --set X.Y.Z[-tag]   (overrides bump kind, used verbatim)
  --yes, -y:    skip all confirmation prompts (non-interactive / CI use)

Examples:
  $0                       # 0.1.0 -> 0.1.1
  $0 --minor               # 0.1.5 -> 0.2.0
  $0 --beta                # 0.1.0 -> 0.1.0-beta (or .N if -beta already exists)
  $0 --minor --beta        # 0.1.5 -> 0.2.0-beta
  $0 --set 0.3.0-preview   # exact value, tagged as desktop-0.3.0-preview
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
  # Strip any existing prerelease suffix when bumping.
  BASE="${CURRENT%%-*}"
  IFS='.' read -r MAJOR MINOR PATCH <<< "$BASE"

  case "$BUMP" in
    major) MAJOR=$((MAJOR + 1)); MINOR=0; PATCH=0 ;;
    minor) MINOR=$((MINOR + 1)); PATCH=0 ;;
    patch) PATCH=$((PATCH + 1)) ;;
  esac

  NEW="$MAJOR.$MINOR.$PATCH"

  if [[ -n "$PRE" ]]; then
    # If the existing version already has the same pre-release marker,
    # increment its trailing number. Otherwise start a fresh -PRE.
    if [[ "$CURRENT" =~ ^${MAJOR}\.${MINOR}\.${PATCH}-${PRE}([.-]?([0-9]+))?$ ]]; then
      n="${BASH_REMATCH[2]:-1}"
      n=$((n + 1))
      NEW="$NEW-$PRE.$n"
    else
      NEW="$NEW-$PRE"
    fi
  fi
fi

TAG="$TAG_PREFIX-$NEW"
echo "New version:     $NEW"
echo "Tag:             $TAG"

if git rev-parse "$TAG" >/dev/null 2>&1; then
  echo "Tag $TAG already exists. Aborting." >&2
  exit 1
fi

confirm "Bump + tag? (y/N)" || { echo "Aborted."; exit 1; }

# --- Patch the version into all three files ---------------------------------

python3 - <<PY
import json, re, pathlib

new = "$NEW"

# Compute an MSI-compliant version. Two WiX constraints drive this:
#   1. Pre-release identifiers must be numeric-only (so `0.1.0-beta.1` is
#      invalid as-is for MSI).
#   2. Windows Installer IGNORES the 4th version field for upgrade detection —
#      it compares only major.minor.build (each <= 65535). A scheme that only
#      varied the 4th field made every pre-release read as the same version,
#      so Windows refused in-place upgrades ("uninstall the existing version
#      first"). Homebrew/Linux are unaffected and keep the human version.
#
# So we encode (patch, pre-release type, pre-release number) into the 3rd
# (build) field, which IS compared, guaranteeing each release strictly
# increases in the correct semver order (alpha < beta < rc < stable):
#   build3 = patch*1000 + typeBase + n
# where typeBase bands keep the ordering (alpha 0, beta 300, rc 600, stable
# 999) and n (0-99) is the trailing number within a type. Examples:
#   0.1.0-beta.3 -> 0.1.303     0.1.0-rc.1 -> 0.1.601     0.1.0 -> 0.1.999
#   0.1.1-beta.1 -> 0.1.1301    1.0.0      -> 1.0.999
# Caps patch at 64 (64*1000+999 < 65535). Bare `-beta` (no number) -> n=1.
m = re.match(r'^(\d+)\.(\d+)\.(\d+)(?:-([A-Za-z0-9.\-]+))?$', new)
assert m, f"version {new!r} does not match major.minor.patch[-pre]"
major, minor, patch_s, pre = m.groups()
patch = int(patch_s)
assert patch <= 64, "patch > 64 overflows the MSI 3rd-field encoding"
if pre:
    type_match = re.match(r'^([A-Za-z]+)', pre)
    ptype = type_match.group(1).lower() if type_match else ""
    type_base = {"alpha": 0, "preview": 0, "pre": 0, "beta": 300, "rc": 600}.get(ptype, 300)
    tail = re.search(r'(\d+)$', pre)
    n = min(int(tail.group(1)), 99) if tail else 1
    build3 = patch * 1000 + type_base + n
else:
    build3 = patch * 1000 + 999  # stable sentinel — outranks every pre-release
wix_version = f"{major}.{minor}.{build3}"

# tauri.conf.json
p = pathlib.Path("$TAURI_CONF")
conf = json.loads(p.read_text())
conf["version"] = new
conf.setdefault("bundle", {}).setdefault("windows", {}).setdefault("wix", {})["version"] = wix_version
p.write_text(json.dumps(conf, indent=2) + "\n")

# Cargo.toml — keep formatting, edit only the [package].version line
cargo = pathlib.Path("$CARGO_TOML").read_text()
cargo, n = re.subn(r'(?m)^(version\s*=\s*)"[^"]+"', rf'\1"{new}"', cargo, count=1)
assert n == 1, "Cargo.toml: no top-level version= line found"
pathlib.Path("$CARGO_TOML").write_text(cargo)

# Cargo.lock — find the [[package]] block whose name matches the crate and
# rewrite its version line. Without this, the next `cargo build` rewrites
# Cargo.lock and leaves the working tree dirty after a release.
lock_path = pathlib.Path("$CARGO_LOCK")
lock = lock_path.read_text()
lock, n = re.subn(
    r'(\[\[package\]\]\r?\nname = "shield-optimizer-v2"\r?\nversion = )"[^"]+"',
    rf'\1"{new}"',
    lock,
    count=1,
)
assert n == 1, "Cargo.lock: shield-optimizer-v2 package entry not found"
lock_path.write_text(lock)

# package.json
p = pathlib.Path("$PACKAGE_JSON")
pkg = json.loads(p.read_text())
pkg["version"] = new
p.write_text(json.dumps(pkg, indent=2) + "\n")
PY

git diff --stat "$TAURI_CONF" "$CARGO_TOML" "$CARGO_LOCK" "$PACKAGE_JSON"

# --- Commit + tag + push -----------------------------------------------------

git add "$TAURI_CONF" "$CARGO_TOML" "$CARGO_LOCK" "$PACKAGE_JSON"
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
