#!/usr/bin/env python3
"""Set a staging version in the working tree only, the way release.sh does.

Usage: stage_version.py X.Y.Z [v2-dir]

Edits tauri.conf.json (version and the numeric WiX version), package.json and
the [package] version of src-tauri and crates/core, so the installers and the
running app both report X.Y.Z. Cargo refreshes Cargo.lock on the next build.
Never commit the result: the upgrade tests build it and throw it away.
"""

import json
import re
import sys
from pathlib import Path


def main() -> None:
    version = sys.argv[1]
    root = Path(sys.argv[2] if len(sys.argv) > 2 else "v2")
    m = re.fullmatch(r"(\d+)\.(\d+)\.(\d+)", version)
    if not m:
        sys.exit(f"{version!r}: only a stable X.Y.Z can be staged")
    major, minor, patch = (int(x) for x in m.groups())
    wix = f"{major}.{minor}.{patch * 1000 + 999}"

    conf_path = root / "src-tauri" / "tauri.conf.json"
    conf = json.loads(conf_path.read_text(encoding="utf-8"))
    conf["version"] = version
    conf.setdefault("bundle", {}).setdefault("windows", {}).setdefault("wix", {})["version"] = wix
    conf_path.write_text(json.dumps(conf, indent=2) + "\n", encoding="utf-8")

    pkg_path = root / "package.json"
    pkg = json.loads(pkg_path.read_text(encoding="utf-8"))
    pkg["version"] = version
    pkg_path.write_text(json.dumps(pkg, indent=2) + "\n", encoding="utf-8")

    for cargo in (root / "src-tauri" / "Cargo.toml", root / "crates" / "core" / "Cargo.toml"):
        text = cargo.read_text(encoding="utf-8")
        text, n = re.subn(r'(?m)^(version\s*=\s*)"[^"]+"', rf'\g<1>"{version}"', text, count=1)
        if n != 1:
            sys.exit(f"{cargo}: no top-level version line")
        cargo.write_text(text, encoding="utf-8")

    print(f"staged {version} (wix {wix})")


if __name__ == "__main__":
    main()
