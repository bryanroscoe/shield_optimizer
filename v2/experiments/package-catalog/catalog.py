"""Offline descriptive catalog experiment. Never imported by either application."""

import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parent
REPO = ROOT.parents[2]
DATA = ROOT / "data"
PACKAGE = re.compile(r"[A-Za-z][A-Za-z0-9_]*(?:\.[A-Za-z][A-Za-z0-9_]*)+")
TV_FEATURES = {"android.software.leanback", "android.hardware.type.television"}
CATEGORY_GROUPS = {
    "media": "Audiobook|Ambient Sound|Cast|Gallery|Local Media Player|Lyrics|Multimedia|Music Practice Tool|Online Media Player|Podcast|Radio|Recorder|Volume",
    "games": "Action Game|Board Game|Card Game|Casual Game|Dice|Educational Game|Emulator|Game Helper|Party Game|Platformer Game|Puzzle Game|Role-Playing Game|Shooter Game|Sport Game|Strategy Game|Visual Novel|Word Game",
    "network": "Connectivity|DNS & Hosts|Download|File Transfer|Firewall|Internet|Network Analyzer|Remote Access|Remote Controller|VPN & Proxy",
    "system": "App Manager|App Store & Updater|Battery|Flashlight|Keyboard & IME|Launcher|Notification|Push|System|Theming|Wallpaper",
    "productivity": "Alarm Clock|Bookmark|Calculator|Calendar & Agenda|Clock|Cloud Storage & File Sync|Contact|File Manager|Inventory|Note|Schedule|Shopping List|Stopwatch|Task|Text Editor|Time Tracker|Timer|Unit Convertor|Writing",
    "communication": "Email|Forum|Messaging|Phone & SMS|Social Network|Voice & Video Chat",
    "reading": "Browser|Ebook Reader|News|Reading|Translation & Dictionary",
    "security": "File Encryption & Vault|Password & 2FA|Security|Text Encryption",
    "development": "Code & Forge|Development",
    "graphics": "Camera|Draw|Graphics|Icon Pack|OCR",
    "location": "Location Tracker & Sharer|Navigation|Public Transport|Weather",
    "health": "Diet|Emergency Action|Habit Tracker|Health Manager|Medication|Meditation|Mental Health|Sports & Health|Workout",
    "finance": "Finance Manager|Market & Price|Pass Wallet|Wallet",
}
CATEGORY_MAP = {term: group for group, terms in CATEGORY_GROUPS.items() for term in terms.split("|")}
DESCRIPTIONS = {
    "media": "Media playback, capture or library tools.",
    "games": "Games, emulation or game utilities.",
    "network": "Networking, transfers or remote connectivity tools.",
    "system": "Android interface or system utilities.",
    "productivity": "Personal organization, documents or file tools.",
    "communication": "Messaging or communication tools.",
    "reading": "Web, news or reading tools.",
    "security": "Security or data protection tools.",
    "development": "Software development tools.",
    "graphics": "Image creation or visual processing tools.",
    "location": "Maps, transport, location or weather tools.",
    "health": "Health or personal wellbeing tools.",
    "finance": "Finance or wallet tools.",
    "unknown": "Purpose has not been classified in this experiment.",
}


def read(path):
    return json.loads(path.read_text())


def encoded(value):
    return json.dumps(value, indent=2, ensure_ascii=False) + "\n"


def extract(raw, source):
    if hashlib.sha256(raw).hexdigest() != source["sha256"]:
        raise ValueError("Input does not match the pinned metadata SHA256")
    index = json.loads(raw)
    if index["repo"]["timestamp"] != source["repo_timestamp_ms"]:
        raise ValueError("Unexpected repository timestamp")
    rows = {}
    observations = 0
    for app in index["apps"]:
        package = app["packageName"]
        if not PACKAGE.fullmatch(package):
            raise ValueError(f"Invalid package identifier: {package}")
        observations += 1
        row = rows.setdefault(package, {"package": package, "source_categories": set(), "tv_versions": set()})
        row["source_categories"].update(app.get("categories", []))
        for version in index["packages"].get(package, []):
            for feature in set(version.get("features", [])) & TV_FEATURES:
                row["tv_versions"].add((int(version["versionCode"]), feature, int(version.get("minSdkVersion", 1))))
    return {
        "source": "fdroid-index",
        "observations": observations,
        "rows": [
            {"package": p, "source_categories": sorted(r["source_categories"]),
             "tv_versions": [{"version_code": v, "required_feature": f, "min_sdk": s} for v, f, s in sorted(r["tv_versions"])]}
            for p, r in sorted(rows.items())
        ],
    }


def baseline():
    app_lists = REPO / "v2/crates/core/data/app-lists"
    curated = {r["package"] for name in ("common", "shield", "googletv") for r in read(app_lists / f"{name}.json")}
    known = set(read(app_lists / "known-names.json"))
    return curated, known


def build(seed):
    curated, known = baseline()
    rows = []
    for item in seed["rows"]:
        groups = sorted({CATEGORY_MAP[c] for c in item["source_categories"] if c in CATEGORY_MAP})
        unmapped = sorted(set(item["source_categories"]) - set(CATEGORY_MAP))
        categories = groups or ["unknown"]
        versions = item["tv_versions"]
        rows.append({
            "package": item["package"],
            "categories": categories,
            "description": " ".join(DESCRIPTIONS[c] for c in categories),
            "classification": "partial" if groups and unmapped else "classified" if groups else "unknown",
            "unmapped_source_categories": unmapped,
            "confidence": "source_metadata_unreviewed",
            "source": "fdroid-index",
            "source_url": f"https://f-droid.org/packages/{item['package']}/",
            "observed_at": read(ROOT / "sources.json")["sources"]["fdroid-index"]["retrieved_at"],
            "tv_evidence": {"status": "required_tv_feature_reported" if versions else "unknown", "versions": versions},
            "device_scope": "android_unspecified",
            "shield_compatibility": "unverified",
            "removal_safety": "unreviewed",
        })
    ids = {r["package"] for r in rows}
    report = {
        "schema_version": 1,
        "source_observations": seed["observations"],
        "discovered_unique_ids": len(ids),
        "duplicate_observations_collapsed": seed["observations"] - len(ids),
        "baseline_curated_ids": len(curated),
        "baseline_known_name_ids": len(known),
        "baseline_union_ids": len(curated | known),
        "overlap_curated_ids": len(ids & curated),
        "overlap_known_name_ids": len(ids & known),
        "new_ids_beyond_baseline": len(ids - curated - known),
        "classification_counts": dict(sorted(Counter(r["classification"] for r in rows).items())),
        "category_counts_nonexclusive": dict(sorted(Counter(c for r in rows for c in r["categories"]).items())),
        "tv_evidence_counts": dict(sorted(Counter(r["tv_evidence"]["status"] for r in rows).items())),
        "shield_verified_ids": 0,
        "removal_safety_reviewed_ids": 0,
        "scope": "F-Droid active index snapshot only; not an Android TV population estimate. Baseline excludes launcher and sideload catalogs.",
    }
    return {"schema_version": 1, "experimental": True, "packages": rows}, report


def validate(catalog):
    if set(catalog) != {"schema_version", "experimental", "packages"} or catalog["schema_version"] != 1 or catalog["experimental"] is not True:
        raise ValueError("Invalid catalog envelope")
    seen = set()
    expected = {"package", "categories", "description", "classification", "unmapped_source_categories", "confidence", "source", "source_url", "observed_at", "tv_evidence", "device_scope", "shield_compatibility", "removal_safety"}
    for row in catalog["packages"]:
        p = row["package"]
        if set(row) != expected or not PACKAGE.fullmatch(p) or p in seen:
            raise ValueError("Invalid fields, package identifier or duplicate")
        seen.add(p)
        cats = row["categories"]
        if not cats or cats != sorted(set(cats)) or any(c not in DESCRIPTIONS for c in cats) or ("unknown" in cats and len(cats) != 1):
            raise ValueError("Invalid categories")
        if row["description"] != " ".join(DESCRIPTIONS[c] for c in cats):
            raise ValueError("Description/category mismatch")
        classification = "unknown" if cats == ["unknown"] else "partial" if row["unmapped_source_categories"] else "classified"
        if row["classification"] != classification:
            raise ValueError("Classification/category mismatch")
        if row["source"] != "fdroid-index" or row["source_url"] != f"https://f-droid.org/packages/{p}/" or row["observed_at"] != read(ROOT / "sources.json")["sources"]["fdroid-index"]["retrieved_at"]:
            raise ValueError("Invalid provenance")
        for field, value in {"confidence": "source_metadata_unreviewed", "device_scope": "android_unspecified", "shield_compatibility": "unverified", "removal_safety": "unreviewed"}.items():
            if row[field] != value:
                raise ValueError("Unsupported confidence, compatibility or safety claim")
        evidence = row["tv_evidence"]
        if set(evidence) != {"status", "versions"} or evidence["status"] != ("required_tv_feature_reported" if evidence["versions"] else "unknown"):
            raise ValueError("TV evidence/status mismatch")
        for v in evidence["versions"]:
            if set(v) != {"version_code", "required_feature", "min_sdk"} or v["required_feature"] not in TV_FEATURES or type(v["version_code"]) is not int or type(v["min_sdk"]) is not int or v["min_sdk"] < 1:
                raise ValueError("Invalid version TV evidence")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--extract", type=Path, help="Extract only pinned public metadata; never fetches anything")
    parser.add_argument("--check", action="store_true", help="Verify committed outputs without writing")
    args = parser.parse_args()
    source = read(ROOT / "sources.json")["sources"]["fdroid-index"]
    if args.extract:
        seed_text = encoded(extract(args.extract.read_bytes(), source))
    else:
        seed_text = (DATA / "seed.json").read_text()
    if hashlib.sha256(seed_text.encode()).hexdigest() != source["normalized_seed_sha256"]:
        raise ValueError("Normalized seed does not match pinned provenance")
    if args.extract:
        if args.check:
            if (DATA / "seed.json").read_text() != seed_text:
                raise ValueError("Committed normalized seed differs from extraction")
        else:
            (DATA / "seed.json").write_text(seed_text)
    catalog, report = build(json.loads(seed_text))
    validate(catalog)
    for name, value in (("catalog.json", catalog), ("coverage.json", report)):
        path = DATA / name
        if args.check:
            if path.read_text() != encoded(value):
                raise ValueError(f"Stale output: {name}")
        else:
            path.write_text(encoded(value))
    print(encoded(report), end="")


if __name__ == "__main__":
    main()
