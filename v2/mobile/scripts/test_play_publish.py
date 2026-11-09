import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
import urllib.parse

spec = importlib.util.spec_from_file_location('play', Path(__file__).with_name('play-publish.py'))
play = importlib.util.module_from_spec(spec)
spec.loader.exec_module(play)

ACCOUNT = {'client_email': 'ci@example.iam.gserviceaccount.com', 'private_key': 'PEM'}


class FakePlay:
    """Records every request; answers like the Android Publisher API."""

    def __init__(self, bundles=(), apks=(), tracks=(), fail=None, uploaded_code=None, uploaded_sha=None):
        self.requests = []
        self.bundles, self.apks, self.tracks = list(bundles), list(apks), list(tracks)
        self.fail = fail
        self.uploaded_code = uploaded_code
        self.uploaded_sha = uploaded_sha

    def __call__(self, method, url, headers, body, timeout):
        self.requests.append((method, url, headers, body))
        if url == play.TOKEN_URL:
            form = urllib.parse.parse_qs(body.decode())
            assert form['grant_type'] == ['urn:ietf:params:oauth:grant-type:jwt-bearer']
            return 200, json.dumps({'access_token': 'token'}).encode()
        assert headers['Authorization'] == 'Bearer token'
        path = url.split('/applications/com.atvoptimizer.mobile/')[1].split('?')[0]
        if self.fail and self.fail in path:
            return 400, json.dumps({'error': {'message': f'{self.fail} refused'}}).encode()
        if method == 'POST' and path == 'edits':
            return 200, b'{"id": "e1"}'
        if path == 'edits/e1/bundles' and method == 'GET':
            return 200, json.dumps({'bundles': [{'versionCode': c} for c in self.bundles]}).encode()
        if path == 'edits/e1/apks':
            return 200, json.dumps({'apks': [{'versionCode': c} for c in self.apks]}).encode()
        if path == 'edits/e1/tracks':
            return 200, json.dumps({'tracks': self.tracks}).encode()
        if path == 'edits/e1/bundles' and method == 'POST':
            return 200, json.dumps({'versionCode': self.uploaded_code,
                                    'sha256': self.uploaded_sha or hashlib.sha256(body).hexdigest()}).encode()
        return 200, b'{}'

    def calls(self):
        return [(m, u.split('/applications/com.atvoptimizer.mobile/')[-1]) for m, u, _, _ in self.requests
                if u != play.TOKEN_URL]


def client(fake):
    return play.Play(ACCOUNT, http=fake, sign=lambda key, message: b'signature', clock=lambda: 1000)


class Version(unittest.TestCase):
    def config(self, data):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        path = Path(directory.name) / 'tauri.conf.json'
        path.write_text(json.dumps(data))
        return path

    def test_derives_code_and_checks_tag(self):
        path = self.config({'version': '0.2.3'})
        self.assertEqual(play.resolve_version(path, 'mobile-v0.2.3')['version_code'], 2003)
        with self.assertRaisesRegex(play.PlayError, 'expected mobile-v0.2.3'):
            play.resolve_version(path, 'mobile-v0.2.4')
        with self.assertRaisesRegex(play.PlayError, 'expected'):
            play.resolve_version(path, 'v2-0.2.3')

    def test_explicit_code_then_override(self):
        path = self.config({'version': '0.3.0-beta.1', 'bundle': {'android': {'versionCode': 3001}}})
        self.assertEqual(play.resolve_version(path)['version_code'], 3001)
        self.assertEqual(play.resolve_version(path, override='3002')['version_code'], 3002)

    def test_rejects_bad_version_and_code(self):
        with self.assertRaises(play.PlayError):
            play.resolve_version(self.config({'version': '1.0'}))
        with self.assertRaises(play.PlayError):
            play.resolve_version(self.config({'version': '1.0.0'}), override='0')


class Publish(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.aab = Path(directory.name) / 'app.aab'
        self.aab.write_bytes(b'bundle')

    def test_highest_covers_bundles_apks_and_tracks_and_discards_edit(self):
        fake = FakePlay(bundles=[1000], apks=[900],
                        tracks=[{'track': 'internal', 'releases': [{'versionCodes': ['1001']}]}])
        self.assertEqual(play.highest(client(fake)), 1001)
        self.assertEqual(fake.calls()[-1], ('DELETE', 'edits/e1'))
        self.assertEqual(play.highest(client(FakePlay())), 0)

    def test_upload_draft_and_commit(self):
        fake = FakePlay(bundles=[1000], uploaded_code=1002)
        result = play.upload(client(fake), self.aab, 'internal', 'draft', 1002, '0.1.2 (1002)')
        self.assertEqual(result['previous_highest_version_code'], 1000)
        self.assertEqual(result['aab_sha256'], hashlib.sha256(b'bundle').hexdigest())
        calls = fake.calls()
        self.assertIn(('POST', 'edits/e1/bundles?uploadType=media'), calls)
        self.assertEqual(calls[-3:], [('PUT', 'edits/e1/tracks/internal'), ('POST', 'edits/e1:validate'),
                                      ('POST', 'edits/e1:commit')])
        track = next(json.loads(b) for m, u, _, b in fake.requests if m == 'PUT')
        self.assertEqual(track, {'track': 'internal', 'releases': [
            {'name': '0.1.2 (1002)', 'versionCodes': ['1002'], 'status': 'draft'}]})
        self.assertNotIn(('DELETE', 'edits/e1'), calls)

    def test_refuses_reused_code_before_uploading(self):
        fake = FakePlay(tracks=[{'track': 'alpha', 'releases': [{'versionCodes': ['1002']}]}], uploaded_code=1002)
        with self.assertRaisesRegex(play.PlayError, 'not above'):
            play.upload(client(fake), self.aab, 'internal', 'draft', 1002, 'x')
        self.assertFalse(any(m == 'POST' and 'bundles' in u for m, u in fake.calls()))
        self.assertEqual(fake.calls()[-1], ('DELETE', 'edits/e1'))

    def test_mismatched_upload_is_discarded(self):
        for fake, pattern in [(FakePlay(uploaded_code=999), 'versionCode'),
                              (FakePlay(uploaded_code=1002, uploaded_sha='0' * 64), 'SHA-256')]:
            with self.assertRaisesRegex(play.PlayError, pattern):
                play.upload(client(fake), self.aab, 'internal', 'draft', 1002, 'x')
            self.assertFalse(any(u.endswith(':commit') for _, u in fake.calls()))
            self.assertEqual(fake.calls()[-1], ('DELETE', 'edits/e1'))

    def test_api_error_message_surfaces_and_edit_is_discarded(self):
        fake = FakePlay(uploaded_code=1002, fail=':validate')
        with self.assertRaisesRegex(play.PlayError, 'HTTP 400.*:validate refused'):
            play.upload(client(fake), self.aab, 'internal', 'draft', 1002, 'x')
        self.assertEqual(fake.calls()[-1], ('DELETE', 'edits/e1'))

    def test_token_failure_does_not_echo_credentials(self):
        def http(method, url, headers, body, timeout):
            return 401, b'{"error": "invalid_grant", "error_description": "bad"}'
        with self.assertRaisesRegex(play.PlayError, 'HTTP 401') as caught:
            play.highest(play.Play(ACCOUNT, http=http, sign=lambda k, m: b's'))
        self.assertNotIn('PEM', str(caught.exception))

    def test_jwt_claims(self):
        fake = FakePlay()
        play.highest(client(fake))
        assertion = urllib.parse.parse_qs(fake.requests[0][3].decode())['assertion'][0]
        header, claims, signature = assertion.split('.')
        decoded = json.loads(play.base64.urlsafe_b64decode(claims + '=='))
        self.assertEqual(decoded, {'iss': ACCOUNT['client_email'], 'scope': play.SCOPE,
                                   'aud': play.TOKEN_URL, 'iat': 1000, 'exp': 4600})
        self.assertEqual(signature, play.b64url(b'signature'))

    def test_service_account_file_validation(self):
        path = Path(self.aab.parent) / 'sa.json'
        path.write_text('{"client_email": "x"}')
        with self.assertRaisesRegex(play.PlayError, 'private_key'):
            play.Play.from_file(path)
        path.write_text('not json')
        with self.assertRaisesRegex(play.PlayError, 'Cannot read'):
            play.Play.from_file(path)


if __name__ == '__main__':
    unittest.main()
