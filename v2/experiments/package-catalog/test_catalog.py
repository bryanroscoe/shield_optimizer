import copy
import hashlib
import json
import subprocess
import unittest

import catalog


class CatalogTests(unittest.TestCase):
    def setUp(self):
        self.seed = {"observations": 2, "rows": [{"package": "org.example.app", "source_categories": ["Launcher"], "tv_versions": []}]}

    def test_extraction_deduplicates_and_never_infers_tv_from_name(self):
        index = {"repo": {"timestamp": 7}, "apps": [
            {"packageName": "org.example.tv", "categories": ["Launcher"]},
            {"packageName": "org.example.tv", "categories": ["System"]},
        ], "packages": {"org.example.tv": [{"versionCode": 2, "minSdkVersion": 26, "features": ["android.hardware.touchscreen"]}]}}
        raw = json.dumps(index).encode()
        seed = catalog.extract(raw, {"sha256": hashlib.sha256(raw).hexdigest(), "repo_timestamp_ms": 7})
        self.assertEqual(len(seed["rows"]), 1)
        self.assertEqual(seed["rows"][0]["source_categories"], ["Launcher", "System"])
        result, report = catalog.build(seed)
        self.assertEqual(report["duplicate_observations_collapsed"], 1)
        self.assertEqual(result["packages"][0]["tv_evidence"]["status"], "unknown")
        self.assertEqual(result["packages"][0]["removal_safety"], "unreviewed")

    def test_raw_hash_mismatch_is_rejected(self):
        with self.assertRaises(ValueError):
            catalog.extract(b"{}", {"sha256": "0" * 64})

    def test_unknown_and_partial_categories_remain_explicit(self):
        self.seed["rows"][0]["source_categories"] = ["New upstream category"]
        result, _ = catalog.build(self.seed)
        self.assertEqual(result["packages"][0]["classification"], "unknown")
        self.seed["rows"][0]["source_categories"].append("Launcher")
        result, _ = catalog.build(self.seed)
        self.assertEqual(result["packages"][0]["classification"], "partial")
        catalog.validate(result)

    def test_tv_evidence_keeps_version_and_does_not_claim_shield_support(self):
        self.seed["rows"][0]["tv_versions"] = [{"version_code": 42, "min_sdk": 26, "required_feature": "android.software.leanback"}]
        result, report = catalog.build(self.seed)
        row = result["packages"][0]
        self.assertEqual(row["tv_evidence"]["versions"][0]["version_code"], 42)
        self.assertEqual(row["shield_compatibility"], "unverified")
        self.assertEqual(report["shield_verified_ids"], 0)
        catalog.validate(result)

    def test_schema_rejects_unsafe_claims_provenance_duplicates_and_drift(self):
        result, _ = catalog.build(self.seed)
        for field, value in [("removal_safety", "safe"), ("confidence", "verified"), ("source_url", "https://example.com"), ("categories", ["bogus"]), ("description", "unrelated"), ("package", "not valid"), ("risk", "safe")]:
            with self.subTest(field=field):
                bad = copy.deepcopy(result)
                bad["packages"][0][field] = value
                with self.assertRaises(ValueError):
                    catalog.validate(bad)
        result["packages"].append(copy.deepcopy(result["packages"][0]))
        with self.assertRaises(ValueError):
            catalog.validate(result)

    def test_committed_outputs_are_exact_and_accounting_reconciles(self):
        result, report = catalog.build(catalog.read(catalog.DATA / "seed.json"))
        catalog.validate(result)
        self.assertEqual((catalog.DATA / "catalog.json").read_text(), catalog.encoded(result))
        self.assertEqual((catalog.DATA / "coverage.json").read_text(), catalog.encoded(report))
        self.assertEqual(sum(report["classification_counts"].values()), report["discovered_unique_ids"])
        self.assertEqual(sum(report["tv_evidence_counts"].values()), report["discovered_unique_ids"])
        self.assertEqual(report["new_ids_beyond_baseline"] + report["overlap_curated_ids"] + report["overlap_known_name_ids"], report["discovered_unique_ids"])
        self.assertEqual(len(catalog.CATEGORY_MAP), sum(len(x.split("|")) for x in catalog.CATEGORY_GROUPS.values()))

    def test_no_production_files_reference_experiment(self):
        tracked = subprocess.check_output(["git", "ls-files", "v2", ".github"], cwd=catalog.REPO, text=True).splitlines()
        for name in tracked:
            if name.startswith("v2/experiments/") or name == ".github/workflows/package-catalog-experiment.yml":
                continue
            path = catalog.REPO / name
            if path.suffix in {".rs", ".ts", ".svelte", ".json", ".toml", ".yml", ".yaml"}:
                self.assertNotIn("experiments/package-catalog", path.read_text(), name)


if __name__ == "__main__":
    unittest.main()
