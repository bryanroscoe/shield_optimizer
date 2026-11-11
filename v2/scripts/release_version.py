#!/usr/bin/env python3
"""Version arithmetic for v2/release.sh, kept here so it can be tested.

    release_version.py next CURRENT [--bump patch|minor|major] [--pre TYPE]
    release_version.py wix VERSION

`next` prints the version the next release gets. With no --bump, a current
pre-release keeps its base version: the same --pre type counts up (beta.1 ->
beta.2), a later type starts at .1 (beta.2 -> rc.1), and no --pre promotes it
to stable (rc.1 -> its X.Y.Z). From a stable version, or with an explicit
--bump, the base is bumped first (patch by default) and --pre starts at .1.

`wix` prints the MSI version for bundle.windows.wix.version. Windows Installer
compares only major.minor.build, ignores a 4th field and rejects non-numeric
pre-release identifiers, so the pre-release type and number go into the build
field: build = patch*1000 + band + n, with bands alpha 0 / beta 300 / rc 600
and stable 999. `preview` and `pre` share the alpha band. Every limit that
would make two releases encode the same, or overflow a field, is an error:
n must be 1-299, patch at most 64 (64*1000+999 <= 65535), major and minor at
most 255.

Stdlib only; exits non-zero with a message on any invalid input.
"""

from __future__ import annotations

import argparse
import re
import sys

BANDS = {"alpha": 0, "preview": 0, "pre": 0, "beta": 300, "rc": 600}
STABLE_BAND = 999
MAX_N = 299
MAX_PATCH = 64
MAX_MAJOR_MINOR = 255

VERSION_RE = re.compile(r"^(\d+)\.(\d+)\.(\d+)(?:-([A-Za-z0-9.\-]+))?$")
PRE_RE = re.compile(r"^([A-Za-z]+)(?:[.-]?(\d+))?$")


class VersionError(ValueError):
    pass


def parse(version: str) -> tuple[int, int, int, str | None, int | None]:
    """(major, minor, patch, pre type or None, pre number or None)."""
    m = VERSION_RE.match(version)
    if not m:
        raise VersionError(f"version {version!r} is not MAJOR.MINOR.PATCH[-TYPE[.N]]")
    major, minor, patch, pre = m.groups()
    if pre is None:
        return int(major), int(minor), int(patch), None, None
    pm = PRE_RE.match(pre)
    if not pm or pm.group(1).lower() not in BANDS:
        raise VersionError(
            f"version {version!r}: pre-release must be one of {', '.join(sorted(BANDS))} "
            "optionally followed by a number"
        )
    ptype = pm.group(1).lower()
    # A bare `-beta` counts as beta.1.
    n = int(pm.group(2)) if pm.group(2) is not None else 1
    return int(major), int(minor), int(patch), ptype, n


def wix_version(version: str) -> str:
    major, minor, patch, ptype, n = parse(version)
    if major > MAX_MAJOR_MINOR or minor > MAX_MAJOR_MINOR:
        raise VersionError(f"version {version!r}: MSI major and minor must be at most {MAX_MAJOR_MINOR}")
    if patch > MAX_PATCH:
        raise VersionError(f"version {version!r}: patch > {MAX_PATCH} overflows the MSI build field")
    if ptype is None:
        return f"{major}.{minor}.{patch * 1000 + STABLE_BAND}"
    if not 1 <= n <= MAX_N:
        raise VersionError(f"version {version!r}: pre-release number must be 1-{MAX_N}")
    return f"{major}.{minor}.{patch * 1000 + BANDS[ptype] + n}"


def wix_key(version: str) -> tuple[int, ...]:
    return tuple(int(p) for p in wix_version(version).split("."))


def next_version(current: str, bump: str | None = None, pre: str | None = None) -> str:
    major, minor, patch, ptype, n = parse(current)
    if bump not in (None, "patch", "minor", "major"):
        raise VersionError(f"unknown bump {bump!r}")
    if pre is not None:
        pre = pre.lower()
        if pre not in BANDS:
            raise VersionError(f"unknown pre-release type {pre!r}")

    if ptype is not None and bump is None:
        base = f"{major}.{minor}.{patch}"
        if pre is None:
            new = base
        elif pre == ptype:
            new = f"{base}-{pre}.{n + 1}"
        else:
            new = f"{base}-{pre}.1"
    else:
        bump = bump or "patch"
        if bump == "major":
            major, minor, patch = major + 1, 0, 0
        elif bump == "minor":
            minor, patch = minor + 1, 0
        else:
            patch += 1
        new = f"{major}.{minor}.{patch}"
        if pre is not None:
            new = f"{new}-{pre}.1"

    # Never hand out a version Windows would see as the same or older.
    if wix_key(new) <= wix_key(current):
        raise VersionError(f"{current} -> {new} would not upgrade on Windows (MSI {wix_version(current)} -> {wix_version(new)})")
    return new


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    nx = sub.add_parser("next")
    nx.add_argument("current")
    nx.add_argument("--bump", choices=["patch", "minor", "major"])
    nx.add_argument("--pre", choices=sorted(BANDS))
    wx = sub.add_parser("wix")
    wx.add_argument("version")
    args = ap.parse_args(argv)
    try:
        if args.cmd == "next":
            print(next_version(args.current, args.bump, args.pre))
        else:
            print(wix_version(args.version))
    except VersionError as e:
        print(f"release_version: {e}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
