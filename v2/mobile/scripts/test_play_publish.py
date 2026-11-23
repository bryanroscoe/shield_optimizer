import contextlib
import hashlib
import io
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

    def __init__(self, bundles=(), apks=(), tracks=(), fail=None, uploaded_code=None, uploaded_sha=None,
                 countries=None, products=None):
        self.requests = []
        self.countries = countries or {}
        self.products = products or []
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
        if path.startswith('edits/e1/countryAvailability/'):
            track = path.rsplit('/', 1)[1]
            if track == 'internal':
                return 400, json.dumps({'error': {'message': 'Track internal does not support country availability.'}}).encode()
            codes = self.countries.get(track, [])
            return 200, json.dumps({'countries': [{'countryCode': c} for c in codes]} if codes else {}).encode()
        if path == 'oneTimeProducts':
            query = urllib.parse.parse_qs(url.split('?', 1)[1]) if '?' in url else {}
            page = int(query.get('pageToken', ['0'])[0])
            body = {'oneTimeProducts': self.products[page:page + 1]}
            if page + 1 < len(self.products):
                body['nextPageToken'] = str(page + 1)
            return 200, json.dumps(body).encode()
        if path == 'edits/e1/bundles' and method == 'POST':
            return 200, json.dumps({'versionCode': self.uploaded_code,
                                    'sha256': self.uploaded_sha or hashlib.sha256(body).hexdigest()}).encode()
        return 200, b'{}'

    def calls(self):
        return [(m, u.split('/applications/com.atvoptimizer.mobile/')[-1]) for m, u, _, _ in self.requests
                if u != play.TOKEN_URL]


EU = sorted(play.EU_COUNTRIES)
WORLD = EU + ['US', 'GB', 'CA']


def product(product_id, regions, state='ACTIVE'):
    return {'productId': product_id, 'purchaseOptions': [{
        'purchaseOptionId': 'pro-unlock', 'state': state,
        'regionalPricingAndAvailabilityConfigs': [{'regionCode': r, 'availability': a} for r, a in regions.items()]}]}


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


class NextCode(unittest.TestCase):
    def released(self, *tracks):
        return FakePlay(bundles=[1007], tracks=[{'track': t, 'releases': [{'name': n, 'versionCodes': [c]}]}
                                                for t, n, c in tracks])

    def test_derived_code_moves_past_internal_builds(self):
        fake = self.released(('internal', '0.1.5+internal.1007 (1007)', '1007'))
        result = play.choose_version_code(client(fake), '0.1.6', 1006, play.DERIVED, newer_than_testers=True)
        self.assertEqual(result, {'highest': 1007, 'version_code': 1008, 'version_code_source': 'next free code on Play'})
        self.assertEqual(fake.calls()[-1], ('DELETE', 'edits/e1'))

    def test_derived_code_above_play_is_kept(self):
        result = play.choose_version_code(client(self.released()), '0.2.0', 2000, play.DERIVED)
        self.assertEqual(result, {'highest': 1007, 'version_code': 2000, 'version_code_source': play.DERIVED})

    def test_a_set_code_never_moves(self):
        for source in ['override', 'tauri.conf.json bundle.android.versionCode']:
            with self.assertRaisesRegex(play.PlayError, 'not above 1007'):
                play.choose_version_code(client(self.released()), '0.1.6', 1007, source)
        result = play.choose_version_code(client(self.released()), '0.1.6', 1010, 'override')
        self.assertEqual(result['version_code'], 1010)

    def test_tester_release_refuses_a_version_a_tester_track_has_or_passed(self):
        for version in ['0.1.6', '0.1.5', '0.1.6-beta.3']:
            fake = self.released(('alpha', '0.1.6 (1006)', '1006'))
            with self.subTest(version=version), self.assertRaisesRegex(
                    play.PlayError, "alpha already has release '0.1.6 \\(1006\\)', which is not older than"):
                play.choose_version_code(client(fake), version, 1006, play.DERIVED, newer_than_testers=True)
            self.assertEqual(fake.calls()[-1], ('DELETE', 'edits/e1'))
        fake = self.released(('alpha', '0.1.6 (1006)', '1006'))
        self.assertEqual(play.choose_version_code(client(fake), '0.1.6', 1006, play.DERIVED)['version_code'], 1008)

    def test_internal_builds_and_odd_names_do_not_block_a_newer_version(self):
        fake = self.released(('internal', '0.1.9+internal.1007 (1007)', '1007'), ('alpha', '0.1.5 (1005)', '1005'),
                             ('beta', None, '1004'), ('qa', 'hand upload', '1003'))
        result = play.choose_version_code(client(fake), '0.1.6', 1006, play.DERIVED, newer_than_testers=True)
        self.assertEqual(result['version_code'], 1008)

    def test_semver_order(self):
        ordered = ['0.1.5', '0.1.10', '0.2.0-alpha', '0.2.0-alpha.2', '0.2.0-alpha.10', '0.2.0-beta',
                   '0.2.0-beta.1', '0.2.0', '0.2.1+internal.9', '1.0.0']
        keys = [play.semver_key(v) for v in ordered]
        self.assertEqual(keys, sorted(keys))
        self.assertEqual(len(set(keys)), len(keys))
        self.assertEqual(play.semver_key('0.2.0+internal.5'), play.semver_key('0.2.0'))
        self.assertIsNone(play.semver_key('hand upload'))

    def test_movable_lets_a_pinned_code_take_the_next_free_one(self):
        result = play.choose_version_code(client(self.released()), '0.1.6', 1003,
                                          'tauri.conf.json bundle.android.versionCode', movable=True)
        self.assertEqual(result['version_code'], 1008)

    def test_command_prints_the_choice(self):
        fake = self.released()
        original = play.Play.from_file
        play.Play.from_file = classmethod(lambda cls, path, **kw: client(fake))
        self.addCleanup(setattr, play.Play, 'from_file', original)
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            code = play.main(['next-code', '--service-account', 'unused.json', '--version', '0.1.5',
                              '--version-code', '1005', '--version-code-source', play.DERIVED])
        self.assertEqual(code, 0)
        self.assertEqual(json.loads(out.getvalue())['version_code'], 1008)


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

    def test_one_upload_released_on_internal_and_closed_in_one_commit(self):
        fake = FakePlay(bundles=[1002], uploaded_code=1003)
        result = play.upload(client(fake), self.aab, 'internal,alpha', 'completed', 1003, '0.1.3 (1003)')
        self.assertEqual(result['track'], 'internal,alpha')
        calls = fake.calls()
        self.assertEqual(sum(1 for m, u in calls if m == 'POST' and 'bundles' in u), 1)
        self.assertEqual(calls[-4:], [('PUT', 'edits/e1/tracks/internal'), ('PUT', 'edits/e1/tracks/alpha'),
                                      ('POST', 'edits/e1:validate'), ('POST', 'edits/e1:commit')])
        bodies = [json.loads(b) for m, u, _, b in fake.requests if m == 'PUT']
        self.assertEqual([b['track'] for b in bodies], ['internal', 'alpha'])
        for body in bodies:
            self.assertEqual(body['releases'], [{'name': '0.1.3 (1003)', 'versionCodes': ['1003'], 'status': 'completed'}])

    def test_refuses_production_duplicate_or_odd_tracks_before_any_request(self):
        for track, pattern in [('internal,production', 'production'), ('tv:production', 'production'),
                               ('alpha,alpha', 'twice'), ('internal,Alpha Track', 'Unexpected'),
                               ('internal,', 'Unexpected'), ('', 'Unexpected'), ('internal, alpha', 'Unexpected'),
                               ('internal\nalpha', 'Unexpected')]:
            fake = FakePlay(uploaded_code=1003)
            with self.assertRaisesRegex(play.PlayError, pattern):
                play.upload(client(fake), self.aab, track, 'completed', 1003, 'x')
            self.assertEqual(fake.calls(), [])

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

    def test_upload_refuses_a_track_missing_an_eu_state_before_uploading(self):
        fake = FakePlay(uploaded_code=1003, countries={'alpha': [c for c in WORLD if c != 'MT']})
        with self.assertRaisesRegex(play.PlayError, 'Track alpha is not available in MT'):
            play.upload(client(fake), self.aab, 'internal,alpha', 'completed', 1003, 'x')
        calls = fake.calls()
        self.assertFalse(any(m == 'POST' and 'bundles' in u for m, u in calls))
        self.assertFalse(any(m == 'PUT' for m, _ in calls))
        self.assertEqual(calls[-1], ('DELETE', 'edits/e1'))

    def test_upload_passes_eu_wide_track_and_internal_without_a_list(self):
        fake = FakePlay(uploaded_code=1003, countries={'alpha': WORLD})
        play.upload(client(fake), self.aab, 'internal,alpha', 'completed', 1003, 'x')
        self.assertEqual(fake.calls()[-1], ('POST', 'edits/e1:commit'))

    def test_geo_check_passes_when_everything_covers_the_eu(self):
        fake = FakePlay(
            tracks=[{'track': 'internal', 'releases': [{'versionCodes': ['1']}]},
                    {'track': 'alpha', 'releases': [{'versionCodes': ['1']}]},
                    {'track': 'production', 'releases': []}],
            countries={'alpha': WORLD},
            products=[product('atvopt_pro', {c: 'AVAILABLE' for c in WORLD}),
                      product('atvopt_pro_support_1', {}, state='INACTIVE')])
        lines, problems = play.geo_check(client(fake))
        self.assertEqual(problems, [])
        self.assertIn('track alpha: all EU states', lines)
        self.assertIn('product atvopt_pro/pro-unlock: all EU states', lines)
        self.assertIn('product atvopt_pro_support_1/pro-unlock: INACTIVE, skipped', lines)
        self.assertEqual(fake.calls()[-3], ('DELETE', 'edits/e1'))
        self.assertFalse(any(u.endswith(':commit') for _, u in fake.calls()))

    def test_geo_check_reports_every_gap(self):
        regions = {c: 'AVAILABLE' for c in WORLD}
        regions['DE'] = 'NO_LONGER_AVAILABLE'
        del regions['LU']
        fake = FakePlay(
            tracks=[{'track': 'alpha', 'releases': [{'versionCodes': ['1']}]},
                    {'track': 'production', 'releases': [{'versionCodes': ['1']}]}],
            countries={'alpha': ['US', 'FR']},
            products=[product('atvopt_pro', regions)])
        _, problems = play.geo_check(client(fake))
        self.assertEqual(len(problems), 3)
        self.assertTrue(problems[0].startswith('track alpha is missing AT, BE'))
        self.assertEqual(problems[1], 'track production has releases but no country list')
        self.assertEqual(problems[2], 'product atvopt_pro/pro-unlock is not available in DE, LU')

    def test_geo_check_flags_eu_payment_restrictions(self):
        restricted = product('atvopt_pro', {c: 'AVAILABLE' for c in WORLD})
        restricted['restrictedPaymentCountries'] = {'regionCodes': ['US', 'FR']}
        _, problems = play.geo_check(client(FakePlay(products=[restricted])))
        self.assertEqual(problems, ['product atvopt_pro restricts payment methods in FR'])

    def test_unexpected_country_error_propagates_and_discards(self):
        fake = FakePlay(uploaded_code=1003, fail='countryAvailability/alpha')
        with self.assertRaisesRegex(play.PlayError, 'HTTP 400'):
            play.upload(client(fake), self.aab, 'alpha', 'draft', 1003, 'x')
        self.assertEqual(fake.calls()[-1], ('DELETE', 'edits/e1'))
        self.assertFalse(any(m == 'POST' and 'bundles' in u for m, u in fake.calls()))

    def test_geo_check_command_exits_nonzero_with_problems_on_stderr(self):
        fake = FakePlay(tracks=[{'track': 'alpha', 'releases': []}], countries={'alpha': ['US']})
        original = play.Play.from_file
        play.Play.from_file = classmethod(lambda cls, path, **kw: client(fake))
        self.addCleanup(setattr, play.Play, 'from_file', original)
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            code = play.main(['geo-check', '--service-account', 'unused.json'])
        self.assertEqual(code, 1)
        self.assertIn('track alpha: missing AT', out.getvalue())
        self.assertIn('EU geo-blocking: track alpha is missing AT', err.getvalue())

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


def png(width, height, tag=b''):
    return (play.PNG_SIGNATURE + (13).to_bytes(4, 'big') + b'IHDR' + width.to_bytes(4, 'big')
            + height.to_bytes(4, 'big') + b'\x08\x06\x00\x00\x00' + tag)


def sha(data):
    return hashlib.sha256(data).hexdigest()


TEXT = {'title': 'ATV Optimizer', 'shortDescription': 'Tune your TV.', 'fullDescription': 'Line one\nLine two'}


class FakeStore:
    """Stateful Android Publisher listing endpoints: edits copy the live state, commit applies it."""

    def __init__(self, listing, images, fail=None):
        self.live = {'listing': dict(listing), 'images': {k: list(v) for k, v in images.items()}}
        self.edits, self.requests, self.fail, self.commits, self.opened = {}, [], fail, 0, 0

    def __call__(self, method, url, headers, body, timeout):
        self.requests.append((method, url, headers, body))
        if url == play.TOKEN_URL:
            return 200, json.dumps({'access_token': 'token'}).encode()
        assert headers['Authorization'] == 'Bearer token'
        path = url.split('/applications/com.atvoptimizer.mobile/')[1].split('?')[0]
        if self.fail and self.fail(method, url):
            return 500, json.dumps({'error': {'message': 'backend error'}}).encode()
        if method == 'POST' and path == 'edits':
            self.opened += 1
            edit = f'e{self.opened}'
            self.edits[edit] = json.loads(json.dumps(self.live))
            return 200, json.dumps({'id': edit}).encode()
        edit = path.split('/')[1]
        if ':' in edit:
            edit, action = edit.split(':')
            if action == 'commit':
                self.live = self.edits.pop(edit)
                self.commits += 1
            return 200, b'{}'
        state = self.edits.get(edit)
        if method == 'DELETE' and path.count('/') == 1:
            self.edits.pop(path.split('/')[1], None)
            return 204, b''
        parts = path.split('/')
        if parts[2] != 'listings':
            return 200, b'{}'
        if len(parts) == 4:
            if method == 'GET':
                if not state['listing']:
                    return 404, json.dumps({'error': {'message': 'not found'}}).encode()
                return 200, json.dumps(state['listing']).encode()
            state['listing'] = json.loads(body)
            return 200, body
        kind = parts[4]
        if method == 'GET':
            return 200, json.dumps({'images': [{'sha256': h} for h in state['images'].get(kind, [])]}
                                   if state['images'].get(kind) else {}).encode()
        if method == 'DELETE':
            state['images'][kind] = []
            return 200, b'{}'
        assert headers['Content-Type'] == 'image/png' and url.startswith(play.UPLOAD_API)
        state['images'].setdefault(kind, []).append(sha(body))
        return 200, json.dumps({'image': {'sha256': sha(body)}}).encode()

    def calls(self):
        return [(m, u.split('/applications/com.atvoptimizer.mobile/')[-1]) for m, u, _, _ in self.requests
                if u != play.TOKEN_URL]


class Listing(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.dir = Path(directory.name)
        self.icon, self.feature = png(512, 512), png(1024, 500)
        self.shots = [png(1080, 1920, bytes([n])) for n in range(3)]
        self.write()

    def write(self, text=TEXT, shots=None, icon=None, feature=None):
        shots = self.shots if shots is None else shots
        (self.dir / 'screenshots').mkdir(exist_ok=True)
        (self.dir / 'icon.png').write_bytes(icon or self.icon)
        (self.dir / 'feature-graphic.png').write_bytes(feature or self.feature)
        for n, data in enumerate(shots):
            (self.dir / f'screenshots/{n}.png').write_bytes(data)
        (self.dir / 'listing.json').write_text(json.dumps(
            {'language': 'en-US', **text, 'screenshots': [{'file': f'screenshots/{n}.png'} for n in range(len(shots))]}))

    def store(self, text=TEXT, shots=None, **kwargs):
        shots = self.shots if shots is None else shots
        images = {'icon': [sha(self.icon)], 'featureGraphic': [sha(self.feature)],
                  'phoneScreenshots': [sha(s) for s in shots]}
        return FakeStore({'language': 'en-US', **text}, images, **kwargs)

    def local(self):
        return play.load_listing(self.dir)

    def test_diff_in_sync_reads_only_and_discards(self):
        fake = self.store()
        differences, lines = play.listing_diff(client(fake), self.local())
        self.assertEqual(lines[-1], 'in sync')
        self.assertEqual(differences['text'], {})
        self.assertEqual(fake.calls()[-1], ('DELETE', 'edits/e1'))
        self.assertTrue(all(m in ('GET', 'POST') for m, _ in fake.calls()[:-1]))
        self.assertEqual([u for m, u in fake.calls() if m == 'POST'], ['edits'])
        self.assertEqual(fake.commits, 0)

    def test_diff_reports_text_and_image_changes(self):
        live_shots = [self.shots[0], png(1080, 1920, b'old')]
        fake = self.store(text={**TEXT, 'title': 'Old', 'fullDescription': 'Line one\nOld two'}, shots=live_shots)
        fake.live['images']['icon'] = []
        _, lines = play.listing_diff(client(fake), self.local())
        report = '\n'.join(lines)
        self.assertIn('title: changed (3 -> 13 characters)', report)
        self.assertIn('    -Old two\n    +Line two', report)
        self.assertIn('shortDescription: same', report)
        self.assertIn('icon: differs\n    1. icon.png: added', report)
        self.assertIn('featureGraphic: same (1)', report)
        self.assertIn('1. screenshots/0.png: same\n    2. screenshots/1.png: changed\n    3. screenshots/2.png: added', report)
        self.assertNotIn('in sync', report)
        self.assertEqual(fake.commits, 0)

    def test_diff_reports_removed_images(self):
        fake = self.store(shots=self.shots + [png(1080, 1920, b'extra')])
        _, lines = play.listing_diff(client(fake), self.local())
        self.assertIn('    4. live image 4: removed', lines)

    def test_publish_applies_only_changes_in_order_and_verifies(self):
        fake = self.store(text={**TEXT, 'shortDescription': 'Old'}, shots=self.shots[:2])
        lines, result = play.listing_publish(client(fake), self.local(), 'com.atvoptimizer.mobile')
        self.assertIn('fresh edit matches', result)
        calls = [(m, u.split('?')[0].split('/androidpublisher/v3/applications/')[-1]) for m, u in fake.calls()]
        writes = [c for c in calls if c[0] in ('PUT', 'DELETE', 'POST') and c[1] != 'edits']
        self.assertEqual(writes, [
            ('PUT', 'edits/e1/listings/en-US'),
            ('DELETE', 'edits/e1/listings/en-US/phoneScreenshots'),
            ('POST', 'edits/e1/listings/en-US/phoneScreenshots'),
            ('POST', 'edits/e1/listings/en-US/phoneScreenshots'),
            ('POST', 'edits/e1/listings/en-US/phoneScreenshots'),
            ('POST', 'edits/e1:validate'), ('POST', 'edits/e1:commit'),
            ('DELETE', 'edits/e2')])
        uploads = [b for m, u, _, b in fake.requests if m == 'POST' and 'uploadType=media' in u]
        self.assertEqual(uploads, self.shots)
        self.assertEqual(fake.live['listing'], {'language': 'en-US', **TEXT})
        self.assertEqual(fake.live['images']['phoneScreenshots'], [sha(s) for s in self.shots])
        self.assertEqual(fake.commits, 1)

    def test_publish_in_sync_commits_nothing(self):
        fake = self.store()
        _, result = play.listing_publish(client(fake), self.local(), 'com.atvoptimizer.mobile')
        self.assertEqual(result, 'nothing to publish')
        self.assertEqual(fake.commits, 0)
        self.assertEqual(fake.calls()[-1], ('DELETE', 'edits/e1'))

    def test_publish_preserves_video(self):
        fake = self.store(text={**TEXT, 'title': 'Old'})
        fake.live['listing']['video'] = 'https://www.youtube.com/watch?v=abc'
        play.listing_publish(client(fake), self.local(), 'com.atvoptimizer.mobile')
        put = next(json.loads(b) for m, u, _, b in fake.requests if m == 'PUT')
        self.assertEqual(put, {'language': 'en-US', **TEXT, 'video': 'https://www.youtube.com/watch?v=abc'})
        self.assertFalse(any('phoneScreenshots' in u and m != 'GET' for m, u in fake.calls()))

    def test_publish_refuses_without_the_package_as_confirm(self):
        for confirm in [None, '', 'yes', 'com.atvoptimizer']:
            fake = self.store(text={**TEXT, 'title': 'Old'})
            with self.assertRaisesRegex(play.PlayError, '--confirm'):
                play.listing_publish(client(fake), self.local(), confirm)
            self.assertEqual(fake.requests, [])
        original = play.Play.from_file
        play.Play.from_file = classmethod(lambda cls, path, **kw: self.fail('opened the service account'))
        self.addCleanup(setattr, play.Play, 'from_file', original)
        err = io.StringIO()
        with contextlib.redirect_stderr(err):
            code = play.main(['listing', 'publish', '--service-account', 'unused.json', '--listing-dir', str(self.dir)])
        self.assertEqual(code, 1)
        self.assertIn('needs --confirm com.atvoptimizer.mobile', err.getvalue())

    def test_failing_upload_discards_and_does_not_commit(self):
        fake = self.store(text={**TEXT, 'title': 'Old'}, shots=[],
                          fail=lambda m, u: m == 'POST' and 'uploadType=media' in u)
        with self.assertRaisesRegex(play.PlayError, 'HTTP 500.*backend error'):
            play.listing_publish(client(fake), self.local(), 'com.atvoptimizer.mobile')
        self.assertEqual(fake.commits, 0)
        self.assertFalse(any(u.endswith(':commit') for _, u in fake.calls()))
        self.assertEqual(fake.calls()[-1], ('DELETE', 'edits/e1'))
        self.assertEqual(fake.live['listing']['title'], 'Old')

    def test_verification_failure_is_reported(self):
        fake = self.store(text={**TEXT, 'title': 'Old'})
        original = fake.__call__

        def drop_title_on_commit(method, url, headers, body, timeout):
            status, payload = original(method, url, headers, body, timeout)
            if url.endswith(':commit'):
                fake.live['listing']['title'] = 'Old'
            return status, payload
        with self.assertRaisesRegex(play.VerifyError, 'commit succeeded, but re-reading.*title') as caught:
            play.listing_publish(play.Play(ACCOUNT, http=drop_title_on_commit, sign=lambda k, m: b's',
                                           clock=lambda: 1000), self.local(), 'com.atvoptimizer.mobile')
        self.assertIn('title: changed (3 -> 13 characters)', caught.exception.lines)
        original_from_file = play.Play.from_file
        play.Play.from_file = classmethod(lambda cls, path, **kw: play.Play(
            ACCOUNT, http=drop_title_on_commit, sign=lambda k, m: b's', clock=lambda: 1000))
        self.addCleanup(setattr, play.Play, 'from_file', original_from_file)
        fake.live['listing']['title'] = 'Old'
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            code = play.main(['listing', 'publish', '--service-account', 'unused.json', '--listing-dir', str(self.dir),
                              '--confirm', 'com.atvoptimizer.mobile'])
        self.assertEqual(code, 1)
        self.assertIn('title: changed', out.getvalue())
        self.assertIn('commit succeeded', err.getvalue())

    def test_commit_transport_error_reports_unknown_outcome(self):
        fake = self.store(text={**TEXT, 'title': 'Old'})
        original = fake.__call__

        def timeout_on_commit(method, url, headers, body, timeout):
            if url.endswith(':commit'):
                original(method, url, headers, body, timeout)
                raise TimeoutError('timed out')
            return original(method, url, headers, body, timeout)
        with self.assertRaisesRegex(play.PlayError, 'whether Play applied it is unknown.*listing diff'):
            play.listing_publish(play.Play(ACCOUNT, http=timeout_on_commit, sign=lambda k, m: b's',
                                           clock=lambda: 1000), self.local(), 'com.atvoptimizer.mobile')
        self.assertEqual(fake.calls()[-1], ('DELETE', 'edits/e1'))

    def test_discard_swallows_any_error_so_the_original_surfaces(self):
        fake = self.store(text={**TEXT, 'title': 'Old'}, shots=[],
                          fail=lambda m, u: m == 'POST' and 'uploadType=media' in u)
        original = fake.__call__

        def broken_delete(method, url, headers, body, timeout):
            if method == 'DELETE' and url.endswith('/edits/e1'):
                raise ConnectionResetError('reset')
            return original(method, url, headers, body, timeout)
        with self.assertRaisesRegex(play.PlayError, 'HTTP 500.*backend error'):
            play.listing_publish(play.Play(ACCOUNT, http=broken_delete, sign=lambda k, m: b's',
                                           clock=lambda: 1000), self.local(), 'com.atvoptimizer.mobile')
        self.assertEqual(fake.commits, 0)

    def test_missing_live_listing_counts_as_empty(self):
        fake = self.store()
        fake.live['listing'] = {}
        differences, _ = play.listing_diff(client(fake), self.local())
        self.assertEqual(set(differences['text']), set(TEXT))

    def test_validation(self):
        cases = [
            (dict(text={**TEXT, 'title': 'x' * 31}), 'title is 31 characters, over the 30 limit'),
            (dict(text={**TEXT, 'shortDescription': 'x' * 81}), 'shortDescription is 81'),
            (dict(text={**TEXT, 'fullDescription': 'x' * 4001}), 'fullDescription is 4001'),
            (dict(shots=self.shots[:1]), '1 phone screenshots; Play takes 2 to 8'),
            (dict(shots=self.shots * 3), '9 phone screenshots'),
            (dict(icon=png(512, 511)), 'icon must be 512x512'),
            (dict(feature=png(1024, 512)), 'feature graphic must be 1024x500'),
            (dict(icon=b'GIF89a not a png at all'), 'icon.png is not a PNG'),
            (dict(shots=[self.shots[0], png(1000, 2301)]), 'at most 2.3:1'),
        ]
        for kwargs, pattern in cases:
            for leftover in (self.dir / 'screenshots').glob('*.png'):
                leftover.unlink()
            self.write(**kwargs)
            with self.subTest(pattern), self.assertRaisesRegex(play.PlayError, pattern):
                play.load_listing(self.dir)
        self.write()
        (self.dir / 'screenshots/1.png').unlink()
        with self.assertRaisesRegex(play.PlayError, 'screenshots/1.png does not exist'):
            play.load_listing(self.dir)

    def test_bundle_upload_keeps_octet_stream(self):
        fake = FakePlay(uploaded_code=1002)
        aab = self.dir / 'app.aab'
        aab.write_bytes(b'bundle')
        play.upload(client(fake), aab, 'internal', 'draft', 1002, 'x')
        upload = next((u, h) for m, u, h, _ in fake.requests if 'uploadType=media' in u)
        self.assertTrue(upload[0].startswith(play.UPLOAD_API + '/com.atvoptimizer.mobile/edits/e1/bundles'))
        self.assertEqual(upload[1]['Content-Type'], 'application/octet-stream')


if __name__ == '__main__':
    unittest.main()
