import importlib.util
import fnmatch
import json
from pathlib import Path
import re
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('evidence', Path(__file__).with_name('release-evidence.py'))
evidence = importlib.util.module_from_spec(spec)
spec.loader.exec_module(evidence)


class Evidence(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        self.receipt = self.root / 'publish.json'

    def test_failed_upload_never_claims_success_from_a_tee_file(self):
        for content in ['', '{', json.dumps({'track': 'internal'})]:
            self.receipt.write_text(content)
            for outcome in ['failure', 'cancelled']:
                summary = evidence.publish_summary(outcome, self.receipt, 'internal', 'draft', '')
                self.assertNotIn('Uploaded', summary)
                self.assertIn('not confirmed', summary)

    def test_skipped_upload_needs_no_receipt_or_version(self):
        for outcome in ['', 'skipped']:
            self.assertEqual(evidence.publish_summary(outcome, self.receipt, 'internal', 'draft', ''),
                             '- Play upload did not run.')

    def test_success_requires_a_complete_matching_receipt(self):
        valid = {'package': 'com.atvoptimizer.mobile', 'track': 'internal', 'status': 'draft',
                 'version_code': 1003, 'aab_sha256': 'a' * 64}
        for content in ['', '{}', '{', json.dumps({**valid, 'track': 'alpha'}),
                        json.dumps({**valid, 'version_code': 1002}),
                        json.dumps({**valid, 'aab_sha256': ''})]:
            self.receipt.write_text(content)
            with self.assertRaises(ValueError):
                evidence.publish_summary('success', self.receipt, 'internal', 'draft', '1003')
        self.receipt.write_text(json.dumps(valid))
        result = evidence.publish_summary('success', self.receipt, 'internal', 'draft', '1003')
        self.assertIn('Uploaded versionCode 1003', result)
        self.assertIn('not a rollout', result)

    def report(self, counts, content=''):
        (self.root / 'TEST-example.xml').write_text(f'<testsuite {counts}>{content}</testsuite>')

    def test_junit_requires_real_executed_tests(self):
        with self.assertRaisesRegex(ValueError, 'No Kotlin JUnit'):
            evidence.junit_summary(self.root)
        for counts in ['tests="0" failures="0" errors="0"',
                       'tests="2" skipped="2" failures="0" errors="0"']:
            self.report(counts)
            with self.assertRaisesRegex(ValueError, 'actually ran'):
                evidence.junit_summary(self.root)

    def test_junit_rejects_failure_errors_and_invalid_counts(self):
        for counts in ['tests="2" failures="1" errors="0"',
                       'tests="2" failures="0" errors="1"',
                       'tests="2" skipped="3" failures="0" errors="0"',
                       'tests="-1" failures="0" errors="0"']:
            self.report(counts)
            with self.assertRaises(ValueError):
                evidence.junit_summary(self.root)
        self.report('tests="2" failures="0" errors="0"', '<testcase><failure/></testcase>')
        with self.assertRaises(ValueError):
            evidence.junit_summary(self.root)

    def test_junit_records_passed_and_skipped_tests(self):
        self.report('tests="33" skipped="1" failures="0" errors="0"')
        self.assertEqual(evidence.junit_summary(self.root),
                         'Kotlin JVM tests: 32 passed, 1 skipped in 1 reports.')


class WorkflowCoverage(unittest.TestCase):
    def test_mobile_inputs_trigger_pr_and_main_smoke(self):
        root = Path(__file__).resolve().parents[3]
        workflow = (root / '.github/workflows/mobile-play-release.yml').read_text()
        for event, following in [('push', 'pull_request'), ('pull_request', 'workflow_dispatch')]:
            event_block = workflow.split(f'  {event}:\n', 1)[1].split(f'  {following}:', 1)[0]
            patterns = re.findall(r"^      - '([^']+)'$", event_block.split('    paths:\n', 1)[1], re.M)
            for source in [
                'v2/mobile/tauri-plugin-atv-adb/android/src/main/java/DiscoveryLogic.kt',
                'v2/mobile/tauri-plugin-atv-billing/android/build.gradle.kts',
                'v2/mobile/src-tauri/gen/android/app/src/main/AndroidManifest.xml',
                'v2/mobile/src-tauri/src/lib.rs', 'v2/mobile/src/App.svelte',
                'v2/shared/components/HelpTip.svelte', 'v2/crates/core/src/lib.rs',
                'v2/vendor/adb_client/src/lib.rs', 'v2/Cargo.toml', 'v2/Cargo.lock',
                'v2/src-tauri/resources/scrcpy-server-v3.1',
                'v2/src/lib/app-files-catalog.json', 'v2/src/lib/sideload-catalog.json',
                'v2/catalogs/cleanyourtv/catalog.json',
                'v2/mobile/scripts/release-evidence.py',
                '.github/actions/android-release-toolchain/action.yml',
            ]:
                with self.subTest(event=event, source=source):
                    self.assertTrue(any(fnmatch.fnmatchcase(source, pattern) for pattern in patterns))
            self.assertFalse(any(fnmatch.fnmatchcase('v2/telemetry-worker/src/index.ts', p) for p in patterns))
        smoke = workflow.split('  smoke:\n', 1)[1].split('  release:\n', 1)[0]
        release = workflow.split('  release:\n', 1)[1]
        self.assertIn("github.ref == 'refs/heads/main'", smoke)
        for job in [smoke, release]:
            self.assertIn(':tauri-plugin-atv-adb:testDebugUnitTest', job)
            self.assertIn('release-evidence.py junit', job)
        self.assertIn('UPLOAD_OUTCOME: ${{ steps.upload.outcome }}', release)
        self.assertIn('release-evidence.py publish', release)


if __name__ == '__main__':
    unittest.main()
