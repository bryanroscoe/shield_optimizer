from copy import deepcopy
from datetime import date
import unittest

from review_queue import review_queue


class QueueTests(unittest.TestCase):
    def catalog(self):
        return {
            "upstream_commit": "pinned",
            "actionable": False,
            "review_status": "unreviewed",
            "profiles": [
                {"profile": "one", "entries": [
                    {"package": "org.example.active", "upstream_form": "active_disable"},
                    {"package": "org.example.active", "upstream_form": "active_disable"},
                    {"package": "org.example.active", "upstream_form": "commented_out"},
                    {"package": "org.example.note", "upstream_form": "comment_reference"},
                    {"package": "org.example.held", "upstream_form": "active_disable"},
                    {"package": "org.example.curated", "upstream_form": "active_disable"},
                ]},
                {"profile": "two", "entries": [
                    {"package": "org.example.active", "upstream_form": "comment_reference"},
                    {"package": "org.example.held", "upstream_form": "active_disable"},
                ]},
            ],
        }

    def held(self):
        return {"packages": {"org.example.held": {
            "status": "held", "source_commit": "pinned", "reconsider_after": "2026-12-05",
        }}}

    def test_conflicts_and_duplicate_lines_never_become_votes_or_verdicts(self):
        catalog = self.catalog()
        original = deepcopy(catalog)
        result = review_queue(catalog, {"org.example.curated"}, self.held(), date(2026, 11, 5), 25)
        self.assertEqual(catalog, original)
        self.assertFalse(result["actionable"])
        self.assertEqual(result["already_curated"], 1)
        self.assertEqual(result["held_until_new_evidence_or_due"], 1)
        self.assertEqual(result["no_active_disable_observation"], 1)
        self.assertEqual(len(result["candidates"]), 1)
        candidate = result["candidates"][0]
        self.assertEqual(candidate["active_profiles"], 1)
        self.assertEqual(candidate["commented_profiles"], 1)
        self.assertEqual(candidate["note_profiles"], 1)
        self.assertEqual(candidate["profiles"], ["one", "two"])
        self.assertFalse(candidate["actionable"])
        self.assertNotIn("risk", candidate)
        self.assertNotIn("default_optimize", candidate)

    def test_due_or_changed_source_requeues_only_as_unreviewed(self):
        for when, commit in [(date(2026, 12, 5), "pinned"), (date(2026, 11, 5), "new")]:
            catalog = self.catalog()
            catalog["upstream_commit"] = commit
            result = review_queue(catalog, set(), self.held(), when, 1)
            self.assertEqual(result["candidates"][0]["package"], "org.example.held")
            self.assertFalse(result["candidates"][0]["actionable"])
            self.assertEqual(result["candidates"][0]["review_status"], "unreviewed")

    def test_curated_membership_is_exact_and_case_sensitive(self):
        result = review_queue(self.catalog(), {"ORG.EXAMPLE.ACTIVE"}, self.held(), date(2026, 11, 5), 25)
        self.assertIn("org.example.active", [p["package"] for p in result["candidates"]])

    def test_unaccepted_source_contract_fails_closed(self):
        for key, value in [("actionable", True), ("review_status", "reviewed")]:
            catalog = self.catalog()
            catalog[key] = value
            with self.assertRaises(ValueError):
                review_queue(catalog, set(), {}, date(2026, 11, 5), 25)


if __name__ == "__main__":
    unittest.main()
