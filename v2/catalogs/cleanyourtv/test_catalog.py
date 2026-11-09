import copy
import json
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest.mock import patch

import catalog


class CatalogTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.data = catalog.check()

    def copied_snapshot(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name) / "snapshot"
        shutil.copytree(catalog.ROOT, root)
        return root

    def test_active_commented_and_duplicate_lines_stay_distinct(self):
        entries = catalog.parse_lines(b"# keep the network\ncom.example.app # disable\n# com.example.app # keep\ncom.example.app\n")
        self.assertEqual([(row["line"], row["upstream_form"]) for row in entries],
                         [(2, "active_disable"), (3, "commented_out"), (4, "active_disable")])

    def test_commands_and_malformed_active_lines_are_rejected(self):
        for line in [b"pm disable-user com.example.app", b"com.example.app;echo bad", b"../com.example.app", b"com.example.app extra"]:
            with self.subTest(line=line), self.assertRaises(ValueError):
                catalog.parse_lines(line)

    def test_source_notice_is_complete_and_pinned(self):
        raw = (catalog.ROOT / "upstream/LICENSE").read_bytes()
        self.assertEqual(catalog.digest(raw), "e0941eca3b403700368b495fd9a91af4e15242fabb992ed27af95c0d4cdc6f83")
        self.assertIn(b"OUT OF OR IN CONNECTION WITH THE SOFTWARE", raw)
        self.assertIn(b"Outfit", raw)

    def test_raw_source_tampering_fails_before_lookup(self):
        root = self.copied_snapshot()
        path = root / "upstream/examples/acer-ar3-r3/debloat.txt"
        path.write_bytes(path.read_bytes() + b"com.example.invented\n")
        with self.assertRaisesRegex(ValueError, "integrity mismatch"):
            catalog.check(root)

    def test_paths_extra_files_and_symlinks_are_rejected(self):
        for kind in ["path", "extra", "symlink"]:
            with self.subTest(kind=kind):
                root = self.copied_snapshot()
                if kind == "path":
                    manifest = catalog.read_json(root / "input-manifest.json")
                    manifest["sources"][0]["path"] = "../outside.txt"
                    (root / "input-manifest.json").write_bytes(catalog.encoded(manifest))
                elif kind == "extra":
                    (root / "upstream/unexpected.sh").write_text("exit 0\n")
                else:
                    path = root / "upstream/examples/acer-ar3-r3/debloat.txt"
                    path.unlink()
                    path.symlink_to(root / "upstream/LICENSE")
                with self.assertRaises(ValueError):
                    catalog.check(root)

    def test_changed_generated_verdict_is_rejected(self):
        root = self.copied_snapshot()
        data = copy.deepcopy(self.data)
        data["actionable"] = True
        data["safety"] = "safe"
        (root / "catalog.json").write_bytes(catalog.encoded(data))
        with self.assertRaisesRegex(ValueError, "modified catalog"):
            catalog.check(root)

    def test_each_source_is_attributed_to_the_pinned_commit(self):
        manifest = catalog.read_json(catalog.ROOT / "input-manifest.json")
        self.assertEqual(len(manifest["sources"]), 855)
        for source in manifest["sources"]:
            self.assertEqual(source["commit"], catalog.COMMIT)
            self.assertEqual(len(source["git_blob"]), 40)
            self.assertEqual(len(source["sha256"]), 64)
            self.assertTrue(source["path"] == "LICENSE" or catalog.INPUT.fullmatch(source["path"]))

    def test_import_refuses_parent_symlinks_before_writing(self):
        raw = b"com.example.app\n"
        tree = f"100644 blob {catalog.blob_id(raw)}\texamples/test/debloat.txt\0".encode()
        for component in ["upstream", "profile", "manifest"]:
            with self.subTest(component=component), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary) / "catalog"
                root.mkdir()
                outside = Path(temporary) / "outside"
                outside.mkdir()
                if component == "manifest":
                    protected = outside / "manifest.json"
                    protected.write_bytes(b"preserve existing data\n")
                    (root / "input-manifest.json").symlink_to(protected)
                elif component == "upstream":
                    (root / "upstream").symlink_to(outside, target_is_directory=True)
                    protected = outside / "examples/test/debloat.txt"
                    protected.parent.mkdir(parents=True)
                else:
                    (root / "upstream/examples").mkdir(parents=True)
                    (root / "upstream/examples/test").symlink_to(outside, target_is_directory=True)
                    protected = outside / "debloat.txt"
                protected.write_bytes(b"preserve existing data\n")
                with patch.object(catalog.subprocess, "check_output", side_effect=[tree, raw]):
                    with self.assertRaisesRegex(ValueError, "Symlink"):
                        catalog.import_sources(Path("unused-repository"), root)
                self.assertEqual(protected.read_bytes(), b"preserve existing data\n")
                if component != "manifest":
                    self.assertFalse((root / "input-manifest.json").exists())

    def test_oversized_source_fails_before_hashing_or_reading(self):
        root = self.copied_snapshot()
        path = root / "upstream/examples/acer-ar3-r3/debloat.txt"
        with path.open("wb") as stream:
            stream.truncate(catalog.MAX_SOURCE_BYTES + 1)
        with self.assertRaisesRegex(ValueError, "size limit"):
            catalog.check(root)

    def test_real_profile_retains_conflicting_keep_context(self):
        result = catalog.lookup(self.data, "com.realtek.leanbacklauncher.partnercustomizer", "acer-ar3-r3")
        self.assertTrue(result["observations"])
        self.assertTrue(any("Inputs button" in row["original_line"] for row in result["observations"]))
        self.assertIn("comment_reference", {row["upstream_form"] for row in result["observations"]})
        self.assertIn("UKR1.251116", "\n".join(result["context_comments"]))
        self.assertIn("not installed", "\n".join(result["context_comments"]))
        self.assertEqual(result["license_notice"], (catalog.ROOT / "upstream/LICENSE").read_text())

    def test_profile_lookup_never_unions_other_models(self):
        package = "com.example.only_on_first_model"
        data = copy.deepcopy(self.data)
        data["profiles"][0]["entries"].append({"package": package, "line": 1, "upstream_form": "active_disable"})
        other = data["profiles"][1]["profile"]
        self.assertEqual(catalog.lookup(data, package, other)["observations"], [])
        with self.assertRaisesRegex(ValueError, "Unknown profile"):
            catalog.lookup(data, package, "acer")

    def test_unknown_context_withholds_observations_for_device_queries(self):
        for observed in [{"model": "Acer R3"}, {"platform": "Fire OS"}, {"firmware": "UKR1.251116"},
                         {"model": "Acer R3", "platform": "Android TV 14", "firmware": "UKR1.251116"}]:
            with self.subTest(observed=observed):
                result = catalog.lookup(self.data, "com.google.android.feedback", "acer-ar3-r3", observed)
                self.assertEqual(result["context_status"], "unknown")
                self.assertEqual(result["observations"], [])
                self.assertEqual(result["context_comments"], [])
                self.assertIsNone(result["upstream_context"])
                self.assertFalse(result["actionable"])

    def test_mismatched_model_firmware_and_platform_never_leak(self):
        data = copy.deepcopy(self.data)
        profile = next(p for p in data["profiles"] if p["profile"] == "acer-ar3-r3")
        profile.update(verified_model="R3", verified_platform="Android TV 14", verified_firmware="UKR1.251116")
        profile["upstream_context"]["notes"] = "other-model instruction marker"
        matching = {"model": "R3", "platform": "Android TV 14", "firmware": "UKR1.251116"}
        for field in matching:
            observed = {**matching, field: "mismatch"}
            with self.subTest(field=field):
                result = catalog.lookup(data, "com.google.android.feedback", "acer-ar3-r3", observed)
                self.assertEqual(result["context_status"], "mismatch")
                self.assertEqual(result["observations"], [])
                self.assertEqual(result["context_comments"], [])
                self.assertIsNone(result["upstream_context"])
                self.assertNotIn("other-model instruction marker", json.dumps(result))
                self.assertNotIn("com.realtek.leanbacklauncher.partnercustomizer", json.dumps(result))
        result = catalog.lookup(data, "com.google.android.feedback", "acer-ar3-r3", matching)
        self.assertTrue(result["observations"])
        self.assertFalse(result["actionable"])
        self.assertEqual(result["review_status"], "unreviewed")

    def test_catalog_has_no_global_safety_or_support_authority(self):
        self.assertEqual(set(self.data), {"schema_version", "upstream_commit", "license", "review_status", "actionable", "profiles"})
        self.assertFalse(self.data["actionable"])
        for profile in self.data["profiles"]:
            self.assertEqual(set(profile), {"profile", "source", "metadata_source", "upstream_context", "verified_model", "verified_firmware", "verified_platform", "entries"})
            self.assertIsNone(profile["verified_model"])
            self.assertIsNone(profile["verified_firmware"])
            self.assertIsNone(profile["verified_platform"])
            for entry in profile["entries"]:
                self.assertEqual(set(entry), {"package", "line", "upstream_form"})

    def test_research_disposition_is_separate_from_upstream_observations(self):
        ledger = catalog.read_json(catalog.ROOT / "research-ledger.json")
        self.assertEqual({row["package"] for row in ledger["packages"]},
                         {"com.google.android.partnersetup", "com.realtek.leanbacklauncher.partnercustomizer", "android.autoinstalls.config.nvidia"})
        for row in ledger["packages"]:
            self.assertEqual(row["review_status"], "unreviewed")
            self.assertIsNone(row["proposed_catalog_verdict"])
            self.assertFalse(row["actionable"])
            self.assertTrue(row["evidence_gaps"])
            self.assertTrue(any(entry["package"] == row["package"] for p in self.data["profiles"] for entry in p["entries"]))


if __name__ == "__main__":
    unittest.main()
