"""Offline, attributed source leads. This module has no app or ADB integration."""

import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess


ROOT = Path(__file__).resolve().parent
COMMIT = "8672e68e8a24c1258e4662cd11daa4ed13795dad"
UPSTREAM = "https://github.com/Gathin23/cleanyourtv"
PACKAGE = re.compile(r"[A-Za-z_][A-Za-z0-9_]*(?:\.[A-Za-z_][A-Za-z0-9_]*)+")
TOKEN = re.compile(r"(?<![\w./@-])[A-Za-z_][A-Za-z0-9_]*(?:\.[A-Za-z_][A-Za-z0-9_]*)+(?![\w/@-])")
INPUT = re.compile(r"examples/[a-zA-Z0-9_-]+/(?:debloat\.txt|tv\.json)")
MAX_SOURCE_BYTES = 131072


def encoded(value):
    return (json.dumps(value, ensure_ascii=False, indent=2) + "\n").encode()


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def blob_id(raw):
    return hashlib.sha1(b"blob " + str(len(raw)).encode() + b"\0" + raw).hexdigest()


def read_json(path):
    return json.loads(path.read_bytes())


def parse_lines(raw):
    entries = []
    for number, line in enumerate(raw.decode("utf-8").splitlines(), 1):
        active = line.split("#", 1)[0].strip()
        if active:
            if not PACKAGE.fullmatch(active):
                raise ValueError(f"Unexpected active line {number}")
            entries.append({"package": active, "line": number, "upstream_form": "active_disable"})
        elif line.lstrip().startswith("#"):
            candidate = line.lstrip()[1:].split("#", 1)[0].strip()
            if PACKAGE.fullmatch(candidate):
                entries.append({"package": candidate, "line": number, "upstream_form": "commented_out"})
    return entries


def confined_path(root, relative):
    destination = root
    for part in Path(relative).parts:
        destination = destination / part
        if destination.is_symlink():
            raise ValueError("Symlink in catalog path")
    if not destination.resolve().is_relative_to(root.resolve()):
        raise ValueError("Path escapes catalog directory")
    return destination


def source_path(root, path):
    if path != "LICENSE" and not INPUT.fullmatch(path):
        raise ValueError("Invalid source path")
    return confined_path(root, Path("upstream") / path)


def import_sources(repository, root=ROOT):
    def git(*args):
        return subprocess.check_output(["git", "-C", str(repository), *args])

    manifest_path = confined_path(root, "input-manifest.json")
    records = []
    pending = []
    for item in git("ls-tree", "-r", "-z", COMMIT).split(b"\0"):
        if not item:
            continue
        meta, path = item.decode().split("\t", 1)
        if path != "LICENSE" and not INPUT.fullmatch(path):
            continue
        mode, kind, blob = meta.split()
        if mode != "100644" or kind != "blob":
            raise ValueError(f"Unexpected source mode: {path}")
        raw = git("cat-file", "blob", blob)
        if len(raw) > MAX_SOURCE_BYTES or blob_id(raw) != blob:
            raise ValueError(f"Invalid source blob: {path}")
        destination = source_path(root, path)
        pending.append((destination, raw))
        records.append({"commit": COMMIT, "path": path, "git_blob": blob,
                        "sha256": digest(raw), "bytes": len(raw)})
    for destination, raw in pending:
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(raw)
    manifest = {"repository": UPSTREAM, "commit": COMMIT, "license": "MIT",
                "sources": sorted(records, key=lambda row: row["path"])}
    manifest_path.write_bytes(encoded(manifest))


def verified_sources(root):
    manifest = read_json(confined_path(root, "input-manifest.json"))
    if (manifest["repository"], manifest["commit"], manifest["license"]) != (UPSTREAM, COMMIT, "MIT"):
        raise ValueError("Unexpected upstream provenance")
    sources = {}
    for record in manifest["sources"]:
        path = record["path"]
        if path in sources or (path != "LICENSE" and not INPUT.fullmatch(path)):
            raise ValueError("Invalid or duplicate source path")
        source = source_path(root, path)
        if source.stat().st_size > MAX_SOURCE_BYTES:
            raise ValueError(f"Source exceeds size limit: {path}")
        raw = source.read_bytes()
        if record != {"commit": COMMIT, "path": path, "git_blob": blob_id(raw),
                      "sha256": digest(raw), "bytes": len(raw)}:
            raise ValueError(f"Source integrity mismatch: {path}")
        sources[path] = raw
    license_text = sources["LICENSE"].decode()
    if "Copyright (c) 2026 Gathin" not in license_text or "Permission is hereby granted" not in license_text:
        raise ValueError("Missing upstream MIT notice")
    actual = {p.relative_to(root / "upstream").as_posix()
              for p in (root / "upstream").rglob("*") if p.is_file()}
    if actual != set(sources):
        raise ValueError("Unmanifested upstream content")
    return sources


def build(root=ROOT):
    sources = verified_sources(root)
    parsed = {path: parse_lines(raw) for path, raw in sources.items() if path.endswith("/debloat.txt")}
    identities = {entry["package"] for entries in parsed.values() for entry in entries}
    profiles = []
    for path, entries in sorted(parsed.items()):
        profile = path.split("/")[1]
        metadata = json.loads(sources[f"examples/{profile}/tv.json"])
        if set(metadata) - {"tv", "os", "brand", "notes", "readme_os"}:
            raise ValueError(f"Unexpected metadata fields: {profile}")
        if not all(isinstance(value, str) for value in metadata.values()):
            raise ValueError(f"Invalid metadata: {profile}")
        lines = sources[path].decode().splitlines()
        indexed = {entry["line"] for entry in entries}
        for number, line in enumerate(lines, 1):
            if number not in indexed and line.lstrip().startswith("#"):
                for package in sorted(set(TOKEN.findall(line)) & identities):
                    entries.append({"package": package, "line": number, "upstream_form": "comment_reference"})
        profiles.append({"profile": profile, "source": path,
                         "metadata_source": f"examples/{profile}/tv.json",
                         "upstream_context": metadata,
                         "verified_model": None, "verified_firmware": None,
                         "verified_platform": None,
                         "entries": sorted(entries, key=lambda row: (row["line"], row["package"]))})
    expected_metadata = {profile["metadata_source"] for profile in profiles}
    if expected_metadata != {path for path in sources if path.endswith("/tv.json")}:
        raise ValueError("Unpaired profile metadata")
    return {"schema_version": 1, "upstream_commit": COMMIT, "license": "MIT",
            "review_status": "unreviewed", "actionable": False,
            "profiles": profiles}


def check(root=ROOT):
    catalog = build(root)
    if confined_path(root, "catalog.json").read_bytes() != encoded(catalog):
        raise ValueError("Stale or modified catalog")
    receipt = read_json(confined_path(root, "provenance.json"))
    expected = {"upstream_commit": COMMIT,
                "input_manifest_sha256": digest((root / "input-manifest.json").read_bytes()),
                "catalog_sha256": digest(encoded(catalog)),
                "license_sha256": digest((root / "upstream/LICENSE").read_bytes())}
    if receipt != expected:
        raise ValueError("Provenance receipt mismatch")
    return catalog


def context_status(profile, observed):
    fields = {"model": "verified_model", "firmware": "verified_firmware", "platform": "verified_platform"}
    for field, key in fields.items():
        if profile[key] is not None and observed.get(field) is not None and profile[key] != observed[field]:
            return "mismatch"
    if any(profile[key] is None or observed.get(field) is None for field, key in fields.items()):
        return "unknown"
    return "exact_context_only"


def lookup(catalog, package, profile_id, observed=None, root=ROOT):
    if not PACKAGE.fullmatch(package):
        raise ValueError("Invalid package identity")
    profile = next((row for row in catalog["profiles"] if row["profile"] == profile_id), None)
    if profile is None:
        raise ValueError("Unknown profile; select an exact upstream profile id")
    lines = (root / "upstream" / profile["source"]).read_text().splitlines()
    status = context_status(profile, observed or {})
    observations = [{**entry, "original_line": lines[entry["line"] - 1],
                     "source_url": f"{UPSTREAM}/blob/{COMMIT}/{profile['source']}#L{entry['line']}"}
                    for entry in profile["entries"] if entry["package"] == package]
    withhold = observed and any(value is not None for value in observed.values()) and status != "exact_context_only"
    if withhold:
        observations = []
    return {"package": package, "profile": profile_id,
            "upstream_context": None if withhold else profile["upstream_context"],
            "context_status": status,
            "review_status": "unreviewed", "actionable": False,
            "observations": observations,
            "context_comments": [] if withhold else [line for line in lines if line.lstrip().startswith("#")],
            "license": "MIT", "license_notice": (root / "upstream/LICENSE").read_text()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("check")
    commands.add_parser("rebuild")
    importer = commands.add_parser("import-git")
    importer.add_argument("repository", type=Path)
    search = commands.add_parser("lookup")
    search.add_argument("package")
    search.add_argument("--profile", required=True)
    for field in ["model", "firmware", "platform"]:
        search.add_argument(f"--{field}")
    args = parser.parse_args()
    if args.command == "import-git":
        import_sources(args.repository)
    if args.command in {"rebuild", "import-git"}:
        result = build()
        catalog_path = confined_path(ROOT, "catalog.json")
        receipt_path = confined_path(ROOT, "provenance.json")
        receipt = {"upstream_commit": COMMIT,
                   "input_manifest_sha256": digest((ROOT / "input-manifest.json").read_bytes()),
                   "catalog_sha256": digest(encoded(result)),
                   "license_sha256": digest((ROOT / "upstream/LICENSE").read_bytes())}
        catalog_path.write_bytes(encoded(result))
        receipt_path.write_bytes(encoded(receipt))
    result = check()
    if args.command == "lookup":
        observed = {field: getattr(args, field) for field in ["model", "firmware", "platform"]}
        print(encoded(lookup(result, args.package, args.profile, observed)).decode(), end="")
    else:
        print(f"Verified {len(result['profiles'])} informational profiles at {COMMIT}")


if __name__ == "__main__":
    main()
