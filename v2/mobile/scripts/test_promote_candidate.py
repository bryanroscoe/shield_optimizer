import copy
import importlib.util
import json
from pathlib import Path
import unittest
from unittest.mock import patch
import test_runtime_16kb as fixtures
import test_play_publish as play_fixtures

spec = importlib.util.spec_from_file_location('promote', Path(__file__).with_name('promote-candidate.py'))
promote = importlib.util.module_from_spec(spec)
spec.loader.exec_module(promote)


class Provenance(unittest.TestCase):
    def setUp(self):
        source = fixtures.SourceProvenance()
        source.setUp()
        self.sha = source.sha
        qa = {**copy.deepcopy(source.run), 'id': 43, 'path': '.github/workflows/mobile-16kb-runtime.yml'}
        ci = {**copy.deepcopy(source.run), 'id': 44, 'path': '.github/workflows/v2-tests.yml', 'event': 'push'}
        artifact = copy.deepcopy(source.artifact)
        artifact.update(id=9, name='mobile-16kb-runtime-43')
        artifact['workflow_run']['id'] = 43
        self.data = dict(build_run=source.run, build_artifacts=source.artifacts,
                         source_comparison=source.comparison, qa_run=qa, ci_run=ci,
                         qa_comparison=copy.deepcopy(source.comparison),
                         qa_artifacts={'total_count': 1, 'artifacts': [artifact]},
                         ci_jobs={'total_count': len(promote.CI_JOBS), 'jobs': [dict(name=name, status='completed',
                                  conclusion='success', run_id=44, head_sha=self.sha, run_attempt=1)
                                  for name in promote.CI_JOBS]})

    def plan(self):
        return promote.plan(self.data, 'owner/repo', '42', self.sha, '43', '44')

    def test_exact_main_runs_and_unique_artifacts(self):
        result = self.plan()
        self.assertEqual((result['artifact_id'], result['qa_artifact_id'], result['ci_run_id']), (8, 9, 44))

    def test_qa_and_ci_fail_closed(self):
        for name in ('qa_run', 'ci_run'):
            for key, value in [('id', 99), ('path', 'wrong.yml'), ('event', 'pull_request'),
                               ('head_branch', 'feature'), ('conclusion', 'failure'), ('status', 'in_progress'),
                               ('head_repository', {'full_name': 'fork/repo'})]:
                before = copy.deepcopy(self.data)
                self.data[name][key] = value
                with self.subTest(name=name, key=key), self.assertRaises(ValueError): self.plan()
                self.data = before
        self.data['ci_run']['head_sha'] = 'f' * 40
        with self.assertRaises(ValueError): self.plan()

    def test_each_substantive_ci_job_must_succeed_on_current_attempt(self):
        for key, value in [('name', 'not a gate'), ('conclusion', 'skipped'), ('status', 'queued'),
                           ('head_sha', 'f' * 40), ('run_id', 99), ('run_attempt', 2)]:
            before = copy.deepcopy(self.data)
            self.data['ci_jobs']['jobs'][0][key] = value
            with self.subTest(key=key), self.assertRaises(ValueError): self.plan()
            self.data = before
        self.data['ci_jobs']['total_count'] += 1
        with self.assertRaises(ValueError): self.plan()

    def test_runtime_artifact_expiry_ambiguity_provenance(self):
        for change in ('expired', 'empty', 'duplicate', 'incomplete', 'origin', 'digest', 'not-main'):
            before = copy.deepcopy(self.data)
            listing = self.data['qa_artifacts']
            artifact = listing['artifacts'][0]
            if change == 'expired': artifact['expired'] = True
            elif change == 'empty': artifact['size_in_bytes'] = 0
            elif change == 'duplicate': listing['artifacts'].append(copy.deepcopy(artifact)); listing['total_count'] = 2
            elif change == 'incomplete': listing['total_count'] = 2
            elif change == 'origin': artifact['workflow_run']['head_repository_id'] = 10
            elif change == 'digest': artifact['digest'] = ''
            else: self.data['qa_comparison']['status'] = 'diverged'
            with self.subTest(change=change), self.assertRaises(ValueError): self.plan()
            self.data = before


class Receipts(unittest.TestCase):
    def setUp(self):
        self.fixture = fixtures.Candidate()
        self.fixture.setUp()
        self.addCleanup(self.fixture.doCleanups)
        self.selected = dict(source_sha='a' * 40, source_run_id=42, source_run_attempt=1,
                             repository='owner/repo', artifact_id=8, artifact_name='candidate',
                             artifact_digest='sha256:' + 'b' * 64, qa_workflow_sha='c' * 40,
                             qa_run_id=43, qa_run_attempt=1)
        self.fixture.provenance = self.selected
        self.candidate = self.fixture.verify()
        self.receipt = {**self.candidate, 'status': 'passed', 'page_size': 16384, 'api_level': 36,
                        'system_abi': 'x86_64', 'tested_abi': 'x86_64',
                        'installed_apk_sha256': self.fixture.apk_sha,
                        'passed': ['signature/package/version/static APK checks', 'PAGE_SIZE=16384',
                                   'installed APK hash', 'x86_64 native loader',
                                   'foreground onboarding on cold start and restart', 'stable process']}
        self.root = self.fixture.root
        self.apk = self.fixture.apk_sha
        self.aab = self.candidate['aab_sha256']

    def verify(self):
        (self.root / 'runtime-receipt.json').write_text(json.dumps(self.receipt))
        return promote.verify(self.root, self.root, self.selected, self.apk, self.aab, 'c' * 64)

    def test_matches_all_candidate_and_runtime_identity(self):
        result = self.verify()
        self.assertEqual(result['track'], 'internal')
        self.assertIn('phone/TV acceptance', result['not_proven'])

    def test_every_identity_and_runtime_gate_mismatch_fails(self):
        for field in self.receipt:
            if field == 'passed': bad = []
            elif isinstance(self.receipt[field], int): bad = self.receipt[field] + 1
            else: bad = 'wrong'
            before = copy.deepcopy(self.receipt)
            self.receipt[field] = bad
            with self.subTest(field=field), self.assertRaises(ValueError): self.verify()
            self.receipt = before

    def test_explicit_aab_and_current_certificate_bindings(self):
        self.verify()
        with self.assertRaises(ValueError):
            promote.verify(self.root, self.root, self.selected, self.apk, 'e' * 64, 'c' * 64)
        with self.assertRaises(ValueError):
            promote.verify(self.root, self.root, self.selected, self.apk, self.aab, 'e' * 64)

    def test_no_runtime_receipt_is_not_qualification(self):
        with self.assertRaises(FileNotFoundError):
            promote.verify(self.root, self.root, self.selected, self.apk, self.aab, 'c' * 64)

    def test_changed_bytes_rejected_before_any_play_request(self):
        fake = play_fixtures.FakePlay(uploaded_code=1003)
        client = play_fixtures.client(fake)
        with self.assertRaises(play_fixtures.play.PlayError):
            play_fixtures.play.upload(client, self.root / promote.runtime.AAB_PATH,
                                     'internal', 'draft', 1003, 'candidate', expected_sha256='e' * 64)
        self.assertEqual(fake.requests, [])
        result = play_fixtures.play.upload(client, self.root / promote.runtime.AAB_PATH,
                                           'internal', 'draft', 1003, 'candidate', expected_sha256=self.aab)
        self.assertEqual(result['aab_sha256'], self.aab)


class Workflow(unittest.TestCase):
    def test_internal_only_manual_no_build_and_shared_mutex(self):
        text = (Path(__file__).resolve().parents[3] / '.github/workflows/mobile-play-promote.yml').read_text()
        self.assertIn('workflow_dispatch:', text)
        self.assertNotIn('workflow_run:', text)
        self.assertNotIn('  push:', text)
        self.assertIn('group: Mobile Play release-release', text)
        self.assertIn('cancel-in-progress: false', text)
        self.assertIn('--track internal', text)
        self.assertIn('--expected-aab-sha256 "$AAB_SHA"', text)
        self.assertIn('default: draft', text)
        self.assertNotIn('build-play-release.py', text)
        self.assertNotIn('KEYSTORE', text)
        self.assertNotIn('continue-on-error', text)
        self.assertLess(text.index('promote-candidate.py verify'), text.index('secrets.PLAY_SERVICE_ACCOUNT_JSON'))
        self.assertIn('mobile-play-promote.yml', (Path(__file__).resolve().parents[3] / '.github/workflows/mobile-release-preflight.yml').read_text())


if __name__ == '__main__':
    unittest.main()
