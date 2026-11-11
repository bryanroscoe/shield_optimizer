"""Tests for release_version.py and changelog_section.py.

Run: python3 -m unittest discover -s v2/scripts -p 'test_*.py'
"""

import os
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from changelog_section import NOT_FOUND, section  # noqa: E402
from release_version import VersionError, next_version, wix_key, wix_version  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))


class WixVersion(unittest.TestCase):
    def test_documented_examples(self):
        self.assertEqual(wix_version("0.1.0-beta.3"), "0.1.303")
        self.assertEqual(wix_version("0.1.0-rc.1"), "0.1.601")
        self.assertEqual(wix_version("0.1.0"), "0.1.999")
        self.assertEqual(wix_version("0.1.1-beta.1"), "0.1.1301")
        self.assertEqual(wix_version("1.0.0"), "1.0.999")
        self.assertEqual(wix_version("2.5.0"), "2.5.999")
        self.assertEqual(wix_version("64.255.64"), "64.255.64999")

    def test_bare_type_counts_as_one(self):
        self.assertEqual(wix_version("0.1.0-beta"), wix_version("0.1.0-beta.1"))
        self.assertEqual(wix_version("0.1.0-rc1"), "0.1.601")

    def test_strictly_increasing_across_every_band(self):
        order = [f"2.6.0-alpha.{n}" for n in range(1, 300)]
        order += [f"2.6.0-beta.{n}" for n in range(1, 300)]
        order += [f"2.6.0-rc.{n}" for n in range(1, 300)]
        order += ["2.6.0", "2.6.1-alpha.1", "2.6.1-beta.1", "2.6.1", "2.7.0-alpha.1", "3.0.0"]
        keys = [wix_key(v) for v in order]
        for (a, ka), (b, kb) in zip(zip(order, keys), zip(order[1:], keys[1:])):
            self.assertLess(ka, kb, f"{a} ({ka}) must encode below {b} ({kb})")

    def test_limits_fail_loudly(self):
        for bad in [
            "2.6.0-beta.300",  # would collide with the rc band
            "2.6.0-rc.300",
            "2.6.0-beta.0",
            "2.6.0-dev.1",  # unknown type, used to fall into the beta band
            "2.6.0-gamma",
            "2.6.0-beta.1.2",
            "2.6.65",  # 65*1000+999 > 65535
            "256.0.0",
            "2.6",
            "v2-2.6.0",
        ]:
            with self.subTest(bad=bad), self.assertRaises(VersionError):
                wix_version(bad)

    def test_upper_bounds_are_accepted(self):
        self.assertEqual(wix_version("2.6.0-beta.299"), "2.6.599")
        self.assertEqual(wix_version("2.6.0-rc.299"), "2.6.899")
        self.assertEqual(wix_version("2.6.64"), "2.6.64999")


class NextVersion(unittest.TestCase):
    TABLE = [
        # current, bump, pre, expected
        ("2.5.0", None, None, "2.5.1"),
        ("2.5.0", "patch", None, "2.5.1"),
        ("2.5.0", "minor", None, "2.6.0"),
        ("2.5.0", "major", None, "3.0.0"),
        ("2.5.0", None, "beta", "2.5.1-beta.1"),
        ("2.5.0", "minor", "beta", "2.6.0-beta.1"),
        ("2.5.0", "minor", "rc", "2.6.0-rc.1"),
        ("2.6.0-beta.1", None, "beta", "2.6.0-beta.2"),
        ("2.6.0-beta", None, "beta", "2.6.0-beta.2"),
        ("2.6.0-beta.9", None, "beta", "2.6.0-beta.10"),
        ("2.6.0-alpha.4", None, "beta", "2.6.0-beta.1"),
        ("2.6.0-beta.2", None, "rc", "2.6.0-rc.1"),
        ("2.6.0-rc.1", None, "rc", "2.6.0-rc.2"),
        ("2.6.0-beta.3", None, None, "2.6.0"),
        ("2.6.0-rc.2", None, None, "2.6.0"),
        ("2.6.0-beta.1", "patch", None, "2.6.1"),
        ("2.6.0-beta.1", "minor", "beta", "2.7.0-beta.1"),
        ("2.6.0-rc.1", "major", None, "3.0.0"),
    ]

    def test_transition_table(self):
        for current, bump, pre, expected in self.TABLE:
            with self.subTest(current=current, bump=bump, pre=pre):
                self.assertEqual(next_version(current, bump, pre), expected)
                self.assertGreater(wix_key(expected), wix_key(current))

    def test_refuses_versions_that_would_not_upgrade(self):
        for current, bump, pre in [
            ("2.6.0-rc.1", None, "beta"),  # backwards
            ("2.6.0-beta.2", None, "alpha"),
            ("2.6.0-alpha.3", None, "preview"),  # same band, lower number
            ("2.6.0-beta.299", None, "beta"),  # beta.300 does not fit
            ("2.6.64", None, None),  # patch 65 overflows
        ]:
            with self.subTest(current=current, bump=bump, pre=pre), self.assertRaises(VersionError):
                next_version(current, bump, pre)

    def test_cli(self):
        script = os.path.join(HERE, "release_version.py")
        run = lambda *a: subprocess.run([sys.executable, script, *a], capture_output=True, text=True)
        r = run("next", "2.6.0-beta.1", "--pre", "beta")
        self.assertEqual((r.returncode, r.stdout.strip()), (0, "2.6.0-beta.2"))
        r = run("wix", "2.6.0-beta.2")
        self.assertEqual((r.returncode, r.stdout.strip()), (0, "2.6.302"))
        r = run("wix", "2.6.0-dev.1")
        self.assertEqual(r.returncode, 1)
        self.assertEqual(r.stdout, "")
        self.assertIn("pre-release", r.stderr)


CHANGELOG = """# Changelog

## Unreleased

- not yet

## v2-2.5.10 — 2026-12-01

- ten

## v2-2.5.1 — 2026-11-20

- one

## v2-2.0.0 — 2026-06-15

- stable

## v2-2.0.0-beta.15 — 2026-06-15

- beta fifteen

## v2-2.0.0-beta.1

- beta one
"""


class ChangelogSection(unittest.TestCase):
    def test_whole_word_match(self):
        self.assertEqual(section(CHANGELOG, "v2-2.0.0"), "- stable")
        self.assertEqual(section(CHANGELOG, "v2-2.0.0-beta.15"), "- beta fifteen")
        self.assertEqual(section(CHANGELOG, "v2-2.0.0-beta.1"), "- beta one")
        self.assertEqual(section(CHANGELOG, "v2-2.5.1"), "- one")
        self.assertEqual(section(CHANGELOG, "v2-2.5.10"), "- ten")

    def test_regex_characters_are_literal(self):
        self.assertIsNone(section(CHANGELOG, "v2-2x0x0"))
        self.assertIsNone(section(CHANGELOG, "v2-2.0.0-beta"))
        self.assertIsNone(section(CHANGELOG, "v2-2.5"))
        self.assertIsNone(section(CHANGELOG, "v2-.*"))

    def test_real_changelog_sections_do_not_overlap(self):
        path = os.path.join(HERE, "..", "CHANGELOG.md")
        with open(path, encoding="utf-8") as f:
            text = f.read()
        body = section(text, "v2-2.0.0")
        self.assertIsNotNone(body)
        self.assertFalse([l for l in body.splitlines() if l.startswith("## ")])
        self.assertNotIn("beta.15", body)

    def test_cli_exit_codes(self):
        script = os.path.join(HERE, "changelog_section.py")
        with tempfile.NamedTemporaryFile("w", suffix=".md", delete=False, encoding="utf-8") as f:
            f.write(CHANGELOG)
        try:
            r = subprocess.run([sys.executable, script, f.name, "v2-2.5.1"], capture_output=True, text=True)
            self.assertEqual((r.returncode, r.stdout), (0, "- one\n"))
            r = subprocess.run([sys.executable, script, f.name, "v2-9.9.9"], capture_output=True, text=True)
            self.assertEqual((r.returncode, r.stdout), (NOT_FOUND, ""))
        finally:
            os.unlink(f.name)


if __name__ == "__main__":
    unittest.main()
