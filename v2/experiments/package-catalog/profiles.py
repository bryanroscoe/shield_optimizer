"""Offline profile research leads; no upstream scripts or safety labels are used."""

import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path

import catalog

ROOT = catalog.ROOT
LEADS = ROOT / "profile-leads"
COMMIT = "30626a8e4714f86bf837384b109b5c73ee736917"
BASE = f"https://github.com/Gathin23/cleanyourtv/blob/{COMMIT}/"


def extract_profile(raw):
    packages = set()
    occurrences = 0
    for line in raw.decode("utf-8").splitlines():
        active = line.split("#", 1)[0].strip()
        if not active:
            continue
        package = active.split()[0]
        if not catalog.PACKAGE.fullmatch(package):
            raise ValueError("Unexpected active profile line")
        packages.add(package)
        occurrences += 1
    return sorted(packages), occurrences


def extract(directory):
    profiles = []
    for source in catalog.read(LEADS / "input-manifest.json"):
        path = source["path"]
        if not path.startswith("examples/") or not path.endswith("/debloat.txt") or ".." in Path(path).parts:
            raise ValueError("Invalid source path")
        raw = (directory / path).read_bytes()
        if len(raw) > 131072 or hashlib.sha256(raw).hexdigest() != source["sha256"]:
            raise ValueError(f"Source hash/size mismatch: {path}")
        blob = hashlib.sha1(b"blob " + str(len(raw)).encode() + b"\0" + raw).hexdigest()
        if blob != source["git_blob"]:
            raise ValueError(f"Source Git blob mismatch: {path}")
        ids, occurrences = extract_profile(raw)
        profiles.append({"profile": path.split("/")[1], "source_url": BASE + path,
                         "model": None, "firmware": None,
                         "testing_level": "upstream_profile_only", "confidence": "research_lead",
                         "packages": ids, "active_occurrences": occurrences})
    return {"upstream_commit": COMMIT, "observed_at": "2026-10-22", "profiles": profiles}


def build(seed):
    fdroid = {r["package"]: r for r in catalog.read(catalog.DATA / "catalog.json")["packages"]}
    curated, known = catalog.baseline()
    by_package = {}
    for p in seed["profiles"]:
        for package in p["packages"]:
            by_package.setdefault(package, []).append(p["profile"])
    rows = []
    for package, profiles in sorted(by_package.items()):
        matched = fdroid.get(package)
        rows.append({"package": package, "profile_refs": sorted(set(profiles)),
                     "identity_evidence": "fdroid_metadata_cross_reference" if matched else "upstream_profile_only",
                     "fdroid_ref": package if matched else None,
                     "local_registry_ref": package if package in curated | known else None,
                     "categories": matched["categories"] if matched else ["unknown"],
                     "description": matched["description"] if matched else "Purpose unclassified; this ID appears in an upstream device profile.",
                     "classification": matched["classification"] if matched else "unknown",
                     "confidence": "source_metadata_unreviewed" if matched else "research_lead",
                     "tv_evidence": matched["tv_evidence"]["status"] if matched else "unknown",
                     "installed_on_profile_device": "unverified", "device_compatibility": "unverified",
                     "removal_safety": "unreviewed"})
    ids = set(by_package)
    note_index = catalog.read(LEADS / "note-index.json")
    report = {"schema_version": 1, "profiles": len(seed["profiles"]),
              "active_line_occurrences": sum(p["active_occurrences"] for p in seed["profiles"]),
              "unique_candidate_ids": len(ids), "new_ids_beyond_baseline": len(ids - curated - known),
              "baseline_overlap": len(ids & (curated | known)),
              "fdroid_identity_matches": len(ids & set(fdroid)),
              "new_ids_beyond_fdroid_snapshot": len(ids - set(fdroid)),
              "combined_unique_ids": len(ids | set(fdroid)),
              "classification_counts": dict(sorted(Counter(r["classification"] for r in rows).items())),
              "tv_evidence_counts": dict(sorted(Counter(r["tv_evidence"] for r in rows).items())),
              "device_note_assets": len(note_index), "text_note_leads": sum(n["kind"] == "text" for n in note_index),
              "image_note_leads": sum(n["kind"] == "image" for n in note_index),
              "hardware_verified_ids": 0, "removal_safety_reviewed_ids": 0,
              "scope": "Profile membership is a research lead, not installed-device inventory, TV compatibility or removal advice."}
    return {"schema_version": 1, "experimental": True, "upstream_commit": COMMIT, "packages": rows}, report


def validate(seed, result):
    if seed["upstream_commit"] != COMMIT or result["upstream_commit"] != COMMIT:
        raise ValueError("Unexpected provenance")
    manifest = {BASE + s["path"] for s in catalog.read(LEADS / "input-manifest.json")}
    profiles = {p["profile"] for p in seed["profiles"]}
    if len(profiles) != len(seed["profiles"]) or len(manifest) != len(profiles):
        raise ValueError("Duplicate or missing profiles")
    for p in seed["profiles"]:
        if p["source_url"] != BASE + f"examples/{p['profile']}/debloat.txt" or p["source_url"] not in manifest or p["model"] is not None or p["firmware"] is not None or p["testing_level"] != "upstream_profile_only" or p["confidence"] != "research_lead":
            raise ValueError("Unsupported device/testing claim")
        if type(p["active_occurrences"]) is not int or p["active_occurrences"] < len(p["packages"]):
            raise ValueError("Invalid occurrence count")
        if p["packages"] != sorted(set(p["packages"])) or any(not catalog.PACKAGE.fullmatch(i) for i in p["packages"]):
            raise ValueError("Invalid profile identities")
    expected, _ = build(seed)
    if result != expected:
        raise ValueError("Candidate output differs from isolated descriptive join")
    for row in result["packages"]:
        if row["removal_safety"] != "unreviewed" or row["device_compatibility"] != "unverified" or row["installed_on_profile_device"] != "unverified":
            raise ValueError("Unsupported safety/compatibility claim")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--extract", type=Path, help="Read pinned text files from an external directory; never downloads or executes")
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    seed_path = LEADS / "seed.json"
    seed = extract(args.extract) if args.extract else catalog.read(seed_path)
    seed_text = catalog.encoded(seed)
    receipt = catalog.read(LEADS / "provenance.json")
    for name in ["input-manifest.json", "note-index.json"]:
        key = name.replace(".json", "").replace("-", "_") + "_sha256"
        if hashlib.sha256((LEADS / name).read_text().encode()).hexdigest() != receipt[key]:
            raise ValueError("Manifest/note-index provenance mismatch")
    if hashlib.sha256(seed_text.encode()).hexdigest() != receipt["normalized_seed_sha256"]:
        raise ValueError("Normalized profile seed hash mismatch")
    result, report = build(seed)
    validate(seed, result)
    outputs = [(seed_path, seed), (catalog.DATA / "profile-candidates.json", result), (catalog.DATA / "profile-coverage.json", report)]
    for path, value in outputs:
        if args.check:
            if path.read_text() != catalog.encoded(value):
                raise ValueError(f"Stale output: {path.name}")
        else:
            path.write_text(catalog.encoded(value))
    print(catalog.encoded(report), end="")


if __name__ == "__main__":
    main()
