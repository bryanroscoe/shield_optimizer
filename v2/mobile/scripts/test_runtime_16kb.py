import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
import zipfile

spec = importlib.util.spec_from_file_location('runtime16', Path(__file__).with_name('runtime-16kb.py'))
qa = importlib.util.module_from_spec(spec)
spec.loader.exec_module(qa)


class SourceProvenance(unittest.TestCase):
    def setUp(self):
        self.sha = 'a' * 40
        self.run = {'id': 42, 'head_sha': self.sha, 'repository': {'full_name': 'owner/repo', 'id': 9},
                    'head_repository': {'full_name': 'owner/repo'}, 'path': qa.WORKFLOW,
                    'event': 'workflow_dispatch', 'head_branch': 'main', 'status': 'completed',
                    'conclusion': 'success', 'run_attempt': 1}
        self.artifact = {'id': 8, 'name': 'atv-optimizer-android-0.1.2-1003', 'expired': False,
                         'size_in_bytes': 100, 'digest': 'sha256:' + 'b' * 64,
                         'workflow_run': {'id': 42, 'head_sha': self.sha, 'head_branch': 'main',
                                          'repository_id': 9, 'head_repository_id': 9}}
        self.artifacts = {'total_count': 1, 'artifacts': [self.artifact]}
        self.comparison = {'status': 'ahead', 'merge_base_commit': {'sha': self.sha}}

    def select(self):
        return qa.select_artifact(self.run, self.artifacts, self.comparison, 'owner/repo', '42', self.sha)

    def test_success_selects_exact_artifact_id(self):
        self.assertEqual(self.select()['artifact_id'], 8)
        self.comparison['status'] = 'identical'
        self.assertEqual(self.select()['source_sha'], self.sha)

    def test_run_must_be_successful_main_release_from_same_repository(self):
        for key, value in [('id', 43), ('head_sha', 'c' * 40), ('path', 'another.yml'),
                           ('event', 'pull_request'), ('head_branch', 'feature'),
                           ('status', 'in_progress'), ('conclusion', 'failure'),
                           ('head_repository', {'full_name': 'fork/repo'})]:
            with self.subTest(key=key):
                original = self.run[key]
                self.run[key] = value
                with self.assertRaises(ValueError):
                    self.select()
                self.run[key] = original

    def test_source_must_remain_on_main_history(self):
        self.comparison['status'] = 'diverged'
        with self.assertRaises(ValueError):
            self.select()
        self.comparison['status'] = 'ahead'
        self.comparison['merge_base_commit']['sha'] = 'c' * 40
        with self.assertRaises(ValueError):
            self.select()

    def test_missing_ambiguous_expired_incomplete_or_foreign_artifact_fails(self):
        for change in ('missing', 'duplicate', 'expired', 'incomplete', 'foreign', 'digest'):
            with self.subTest(change=change):
                before = copy.deepcopy(self.artifacts)
                if change == 'missing':
                    self.artifacts = {'total_count': 0, 'artifacts': []}
                elif change == 'duplicate':
                    self.artifacts['total_count'] = 2
                    self.artifacts['artifacts'].append(copy.deepcopy(self.artifact))
                elif change == 'expired':
                    self.artifacts['artifacts'][0]['expired'] = True
                elif change == 'incomplete':
                    self.artifacts['total_count'] = 2
                elif change == 'foreign':
                    self.artifacts['artifacts'][0]['workflow_run']['head_repository_id'] = 10
                else:
                    self.artifacts['artifacts'][0]['digest'] = None
                with self.assertRaises((ValueError, TypeError)):
                    self.select()
                self.artifacts = before


class Candidate(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name)
        self.provenance = {'source_sha': 'a' * 40}
        for path, member in [(qa.APK_PATH, 'lib/x86_64/' + qa.LIBRARY),
                             (qa.AAB_PATH, 'base/lib/x86_64/' + qa.LIBRARY)]:
            full = self.root / path
            full.parent.mkdir(parents=True)
            with zipfile.ZipFile(full, 'w') as archive:
                archive.writestr(member, b'same native bytes')
        self.apk_sha = qa.digest(self.root / qa.APK_PATH)
        self.build = {**self.provenance, 'version_code': 1003,
                      'previous_version_code_source': 'play-api', 'upload_certificate_sha256': 'c' * 64,
                      'artifacts': [{'path': p, 'sha256': qa.digest(self.root / p),
                                     'size_bytes': (self.root / p).stat().st_size}
                                    for p in (qa.APK_PATH, qa.AAB_PATH)]}
        self.verification = {'package': qa.PACKAGE, 'version_code': 1003,
                             'upload_certificate_sha256': 'c' * 64,
                             'aab_sha256': qa.digest(self.root / qa.AAB_PATH),
                             'verified': ['aab-jar-signature', 'upload-certificate', 'bundletool-validate',
                                          'manifest-package-and-versionCode', 'generated-universal-apk',
                                          'apk-signer-certificate'],
                             'apks': [{'apk': Path(qa.APK_PATH).name, 'sha256': self.apk_sha,
                                       'version_code': 1003, 'target_sdk': 36,
                                       'verified': ['signature', 'non-debuggable', 'application-id',
                                                    'zip-16kb', 'elf-load-alignment-by-abi']}]}

    def verify(self, expected=None):
        (self.root / 'play-build-receipt.json').write_text(json.dumps(self.build))
        (self.root / 'play-verification.json').write_text(json.dumps(self.verification))
        return qa.verify_candidate(self.root, self.provenance, expected or self.apk_sha)

    def test_matches_both_receipts_and_both_native_libraries(self):
        result = self.verify()
        self.assertEqual(result['version_code'], 1003)
        self.assertEqual(result['apk_sha256'], self.apk_sha)
        self.assertEqual(len(result['native_x86_64_sha256']), 64)

    def test_explicit_pin_receipt_source_and_smoke_rejected(self):
        with self.assertRaises(ValueError):
            self.verify('f' * 64)
        self.build['source_sha'] = 'b' * 40
        with self.assertRaises(ValueError):
            self.verify()
        self.build['source_sha'] = self.provenance['source_sha']
        self.build['previous_version_code_source'] = 'smoke'
        with self.assertRaises(ValueError):
            self.verify()

    def test_tampered_bytes_and_incomplete_verification_rejected(self):
        with (self.root / qa.APK_PATH).open('ab') as output:
            output.write(b'tamper')
        with self.assertRaises(ValueError):
            self.verify()

    def test_missing_or_ambiguous_apk_receipt_rejected(self):
        self.verification['apks'].append(copy.deepcopy(self.verification['apks'][0]))
        with self.assertRaises(ValueError):
            self.verify()
        self.verification['apks'] = []
        with self.assertRaises(ValueError):
            self.verify()

    def test_missing_signature_gate_or_wrong_package_rejected(self):
        self.verification['apks'][0]['verified'].remove('signature')
        with self.assertRaises(ValueError):
            self.verify()
        self.verification['apks'][0]['verified'].append('signature')
        self.verification['package'] = 'wrong.package'
        with self.assertRaises(ValueError):
            self.verify()

    def test_native_byte_mismatch_even_with_updated_receipt_rejected(self):
        with zipfile.ZipFile(self.root / qa.AAB_PATH, 'w') as archive:
            archive.writestr('base/lib/x86_64/' + qa.LIBRARY, b'different native bytes')
        row = self.build['artifacts'][1]
        row.update(sha256=qa.digest(self.root / qa.AAB_PATH), size_bytes=(self.root / qa.AAB_PATH).stat().st_size)
        self.verification['aab_sha256'] = row['sha256']
        with self.assertRaisesRegex(ValueError, 'native library mismatch'):
            self.verify()


class RuntimeEvidence(unittest.TestCase):
    def setUp(self):
        self.activities = 'topResumedActivity=ActivityRecord{abc u0 com.atvoptimizer.mobile/.MainActivity t19}'
        self.ui = '<hierarchy><node package="com.atvoptimizer.mobile" text="Find your TV"/>' \
                  '<node package="com.atvoptimizer.mobile" text="Enter IP address manually"/></hierarchy>'
        self.logs = 'D nativeloader: Load /data/app/base.apk!/lib/x86_64/libatv_optimizer_mobile_lib.so using class loader: ok\n'

    def test_requires_actual_16kb_api36_x86_64(self):
        qa.verify_environment('16384\n', '36', 'x86_64')
        for values in [('4096', '36', 'x86_64'), ('', '36', 'x86_64'),
                       ('16384', '35', 'x86_64'), ('16384', '36', 'arm64-v8a')]:
            with self.assertRaises(ValueError):
                qa.verify_environment(*values)

    def test_true_onboarding_and_native_loader_pass(self):
        qa.verify_started('4787', self.activities, self.ui, self.logs)

    def test_blank_or_foreign_ui_launcher_missing_process_or_loader_fail(self):
        for pid, activities, ui, logs in [('', self.activities, self.ui, self.logs),
                                         ('1 2', self.activities, self.ui, self.logs),
                                         ('1', 'launcher', self.ui, self.logs),
                                         ('1', self.activities, '<hierarchy/>', self.logs),
                                         ('1', self.activities, self.ui.replace(qa.PACKAGE, 'other'), self.logs),
                                         ('1', self.activities, self.ui, self.logs.replace(': ok', ': failed'))]:
            with self.assertRaises(ValueError):
                qa.verify_started(pid, activities, ui, logs)

    def test_crash_or_loader_failure_cannot_pass(self):
        for error in ('FATAL EXCEPTION', 'Fatal signal 11', 'UnsatisfiedLinkError',
                      'dlopen failed', 'ANR in ' + qa.PACKAGE):
            with self.assertRaises(ValueError):
                qa.verify_started('1', self.activities, self.ui, self.logs + error)

    def test_new_pid_cannot_erase_a_startup_crash_even_if_onboarding_later_renders(self):
        qa.verify_process('41', '41', self.logs, '')
        qa.verify_started('42', self.activities, self.ui, self.logs)
        with self.assertRaisesRegex(ValueError, 'died or changed'):
            qa.verify_process('41', '42', self.logs, '')
        with self.assertRaisesRegex(ValueError, 'died or changed'):
            qa.verify_process('41', '', self.logs, '')

    def test_fatal_and_prior_app_crash_evidence_is_not_readiness_retry(self):
        for log, crashes in [(self.logs + '\nFATAL EXCEPTION', ''),
                             (self.logs, 'Process: com.atvoptimizer.mobile, PID: 40'),
                             (self.logs, '>>> com.atvoptimizer.mobile <<<')]:
            with self.assertRaises(ValueError):
                qa.verify_process('41', '41', log, crashes)
        qa.verify_process('41', '41', self.logs, 'Process: unrelated.system.app, PID: 40')

    def test_workflow_stays_manual_public_bounded_and_without_release_secrets(self):
        root = Path(__file__).resolve().parents[3]
        workflow = (root / '.github/workflows/mobile-16kb-runtime.yml').read_text()
        triggers = workflow.split('on:\n')[1].split('\npermissions:')[0]
        self.assertIn('workflow_dispatch:', triggers)
        for forbidden in ('pull_request:', 'push:', 'schedule:', 'workflow_run:'):
            self.assertNotIn(forbidden, triggers)
        for forbidden in ('secrets.', 'environment:', 'sudo ', '--licenses', 'play-publish.py',
                          'build-play-release.py', 'continue-on-error:'):
            self.assertNotIn(forbidden, workflow)
        for required in ('runs-on: ubuntu-24.04', 'timeout-minutes: 35', 'retention-days: 7',
                         "github.event.repository.visibility == 'public'", 'persist-credentials: false',
                         'contents: read', 'actions: read', 'system-images;android-36;google_apis_ps16k;x86_64'):
            self.assertIn(required, workflow)
        preflight = (root / '.github/workflows/mobile-release-preflight.yml').read_text()
        self.assertEqual(preflight.count("'.github/workflows/mobile-16kb-runtime.yml'"), 2)


if __name__ == '__main__':
    unittest.main()
