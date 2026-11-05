"""Read-only review candidates; this module never feeds the Optimize planner."""

import argparse
from collections import defaultdict
from datetime import date
import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]


def review_queue(catalog, curated, progress, today, limit):
    if catalog.get("actionable") is not False or catalog.get("review_status") != "unreviewed":
        raise ValueError("Expected the unreviewed, non-actionable source catalog")
    commit = catalog["upstream_commit"]
    decisions = progress.get("packages", {})
    observations = defaultdict(lambda: defaultdict(set))
    for profile in catalog["profiles"]:
        for entry in profile["entries"]:
            observations[entry["package"]][entry["upstream_form"]].add(profile["profile"])
    held = 0
    note_only = 0
    candidates = []
    for package, forms in observations.items():
        if package in curated:
            continue
        decision = decisions.get(package, {})
        if decision.get("status") == "held" and decision.get("source_commit") == commit:
            reconsider = decision.get("reconsider_after")
            if not reconsider or date.fromisoformat(reconsider) > today:
                held += 1
                continue
        active = len(forms["active_disable"])
        if not active:
            note_only += 1
            continue
        candidates.append({
            "package": package,
            "review_status": "unreviewed",
            "actionable": False,
            "active_profiles": active,
            "commented_profiles": len(forms["commented_out"]),
            "note_profiles": len(forms["comment_reference"]),
            "profiles": sorted(set().union(*forms.values())),
        })
    candidates.sort(key=lambda c: (-c["active_profiles"], c["package"]))
    return {
        "source_commit": commit,
        "review_status": "unreviewed",
        "actionable": False,
        "source_identities": len(observations),
        "already_curated": len(set(observations) & curated),
        "held_until_new_evidence_or_due": held,
        "no_active_disable_observation": note_only,
        "pending_active_candidates": len(candidates),
        "candidates": candidates[:limit],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--limit", type=int, default=25)
    args = parser.parse_args()
    if not 1 <= args.limit <= 100:
        parser.error("--limit must be between 1 and 100")
    catalog = json.loads((ROOT / "catalogs/cleanyourtv/catalog.json").read_text())
    progress = json.loads((Path(__file__).parent / "progress.json").read_text())
    curated = {
        entry["package"]
        for name in ("common", "shield", "googletv")
        for entry in json.loads((ROOT / f"crates/core/data/app-lists/{name}.json").read_text())
    }
    print(json.dumps(review_queue(catalog, curated, progress, date.today(), args.limit), indent=2))


if __name__ == "__main__":
    main()
