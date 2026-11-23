#!/usr/bin/env python3
"""Resolve the release versionCode and talk to the Google Play Developer API.

Subcommands:
  version   Read tauri.conf.json, check a mobile-v* tag against it, print the versionCode.
  highest   Print the highest versionCode Play already holds (every bundle, APK and track).
  next-code Choose the versionCode a release uploads with, read against Play (see choose_version_code).
  upload    Upload a verified AAB once and release it on one or more tracks (comma-separated),
            as a draft unless told otherwise, in a single edit. Refuses a track whose country
            list leaves out an EU state.
  geo-check Read-only: fail when a track or an active one-time product leaves out an EU state.
  listing   Compare the store listing in v2/mobile/store-listing with Play (`listing diff`, read-only),
            or apply only what differs and commit it (`listing publish --confirm PACKAGE`).

EU Geo-blocking Regulation (EU) 2018/302: Play asks that an app distributed in the EU be
accessible EU-wide, so every track with a country list and every active Pro product must
cover all 27 member states (v2/mobile/PLAY-CONSOLE-WALKTHROUGH.md, requirements table).

Standard library plus the `openssl` binary only, so the release path has no third-party Python
dependency. Credentials are read from files and never printed; API errors print Google's
message, which never contains them.
"""
import argparse
import base64
import difflib
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request

PACKAGE = 'com.atvoptimizer.mobile'
API = 'https://androidpublisher.googleapis.com/androidpublisher/v3/applications'
UPLOAD_API = 'https://androidpublisher.googleapis.com/upload/androidpublisher/v3/applications'
TOKEN_URL = 'https://oauth2.googleapis.com/token'
SCOPE = 'https://www.googleapis.com/auth/androidpublisher'
MAX_VERSION_CODE = 2100000000
DERIVED = 'derived from version'
MOBILE_DIR = Path(__file__).resolve().parents[1]
EU_COUNTRIES = frozenset('AT BE BG CY CZ DE DK EE ES FI FR GR HR HU IE IT LT LU LV MT NL PL PT RO SE SI SK'.split())


class PlayError(Exception):
    def __init__(self, message, status=None):
        super().__init__(message)
        self.status = status


class VerifyError(PlayError):
    """The commit succeeded but a fresh edit disagrees; `lines` is what was applied."""

    def __init__(self, message, lines):
        super().__init__(message)
        self.lines = lines


def resolve_version(config_path, tag=None, override=None):
    config = json.loads(Path(config_path).read_text())
    version = config.get('version', '')
    match = re.fullmatch(r'(\d+)\.(\d+)\.(\d+)(-[0-9A-Za-z.-]+)?', version)
    if not match:
        raise PlayError(f'tauri.conf.json version {version!r} is not semver')
    if tag is not None and tag != f'mobile-v{version}':
        raise PlayError(f'Tag {tag} does not match tauri.conf.json version {version}; expected mobile-v{version}')
    if override:
        code = int(override)
        source = 'override'
    elif config.get('bundle', {}).get('android', {}).get('versionCode') is not None:
        code = int(config['bundle']['android']['versionCode'])
        source = 'tauri.conf.json bundle.android.versionCode'
    else:
        major, minor, patch = (int(x) for x in match.groups()[:3])
        code = major * 1000000 + minor * 1000 + patch
        source = DERIVED
    if not 0 < code <= MAX_VERSION_CODE:
        raise PlayError(f'versionCode {code} is outside 1..{MAX_VERSION_CODE}')
    return {'version': version, 'version_code': code, 'version_code_source': source}


def b64url(data):
    return base64.urlsafe_b64encode(data).rstrip(b'=').decode()


def default_sign(private_key_pem, message):
    # The key only ever touches a 0600 file in a private temp directory, removed on exit.
    with tempfile.TemporaryDirectory() as directory:
        key_path = Path(directory) / 'key.pem'
        fd = os.open(key_path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        with os.fdopen(fd, 'w') as handle:
            handle.write(private_key_pem)
        result = subprocess.run(['openssl', 'dgst', '-sha256', '-sign', str(key_path)],
                                input=message, capture_output=True, check=False, timeout=30)
    if result.returncode != 0:
        raise PlayError('openssl could not sign the service-account token request')
    return result.stdout


def default_http(method, url, headers, body, timeout):
    request = urllib.request.Request(url, data=body, method=method, headers=headers)
    try:
        with urllib.request.urlopen(request, timeout=timeout) as response:
            return response.status, response.read()
    except urllib.error.HTTPError as error:
        return error.code, error.read()


class Play:
    def __init__(self, service_account, package=PACKAGE, http=default_http, sign=default_sign, clock=time.time):
        self.account = service_account
        self.package = package
        self.http = http
        self.sign = sign
        self.clock = clock
        self.token = None

    @classmethod
    def from_file(cls, path, **kwargs):
        try:
            account = json.loads(Path(path).read_text())
        except (OSError, ValueError) as error:
            raise PlayError(f'Cannot read the service-account file: {type(error).__name__}') from None
        for key in ['client_email', 'private_key']:
            if not isinstance(account.get(key), str) or not account[key]:
                raise PlayError(f'Service-account file is missing {key}')
        return cls(account, **kwargs)

    def authorize(self):
        now = int(self.clock())
        header = b64url(json.dumps({'alg': 'RS256', 'typ': 'JWT'}).encode())
        claims = b64url(json.dumps({'iss': self.account['client_email'], 'scope': SCOPE,
                                    'aud': self.account.get('token_uri', TOKEN_URL),
                                    'iat': now, 'exp': now + 3600}).encode())
        unsigned = f'{header}.{claims}'.encode()
        assertion = unsigned.decode() + '.' + b64url(self.sign(self.account['private_key'], unsigned))
        body = urllib.parse.urlencode({'grant_type': 'urn:ietf:params:oauth:grant-type:jwt-bearer',
                                       'assertion': assertion}).encode()
        status, payload = self.http('POST', self.account.get('token_uri', TOKEN_URL),
                                    {'Content-Type': 'application/x-www-form-urlencoded'}, body, 60)
        if status != 200:
            raise PlayError(f'Google refused the service-account token request (HTTP {status}): '
                            f'{error_message(payload)}')
        self.token = json.loads(payload)['access_token']

    def call(self, method, path, body=None, *, upload=None, content_type='application/octet-stream', timeout=120):
        if self.token is None:
            self.authorize()
        headers = {'Authorization': f'Bearer {self.token}'}
        if upload is not None:
            url = f'{UPLOAD_API}/{self.package}/{path}?uploadType=media'
            headers['Content-Type'] = content_type
            data = upload
        else:
            url = f'{API}/{self.package}/{path}'
            data = None if body is None else json.dumps(body).encode()
            if data is not None:
                headers['Content-Type'] = 'application/json'
        status, payload = self.http(method, url, headers, data, timeout)
        if not 200 <= status < 300:
            raise PlayError(f'{method} {path.split("?")[0]} failed (HTTP {status}): {error_message(payload)}', status)
        return json.loads(payload) if payload else {}

    def open_edit(self):
        return self.call('POST', 'edits', {})['id']

    def discard_edit(self, edit):
        # Never let a second failure here mask the error that made us discard.
        try:
            self.call('DELETE', f'edits/{edit}')
        except Exception:
            pass

    def highest(self, edit):
        codes = [0]
        codes += [int(b['versionCode']) for b in self.call('GET', f'edits/{edit}/bundles').get('bundles', [])]
        codes += [int(a['versionCode']) for a in self.call('GET', f'edits/{edit}/apks').get('apks', [])]
        for track in self.call('GET', f'edits/{edit}/tracks').get('tracks', []):
            for release in track.get('releases', []):
                codes += [int(code) for code in release.get('versionCodes', [])]
        return max(codes)

    def release_names(self, edit):
        return [(track['track'], release['name'])
                for track in self.call('GET', f'edits/{edit}/tracks').get('tracks', [])
                for release in track.get('releases', []) if release.get('name')]

    def eu_gaps(self, edit, track):
        """EU states missing from a track's countries, or None if the track has no country list.

        Internal testing has none, and a track Play has never released to answers with an empty
        list; neither restricts anyone, so neither counts as a gap here.
        """
        if track == 'internal':
            return None
        try:
            available = self.call('GET', f'edits/{edit}/countryAvailability/{track}')
        except PlayError as error:
            if 'does not support country availability' in str(error):
                return None
            raise
        countries = {c.get('countryCode') for c in available.get('countries', [])}
        if not countries:
            return None
        return sorted(EU_COUNTRIES - countries)

    def one_time_products(self):
        products, token = [], None
        while True:
            page = self.call('GET', 'oneTimeProducts' + (f'?pageToken={urllib.parse.quote(token)}' if token else ''))
            products += page.get('oneTimeProducts', [])
            token = page.get('nextPageToken')
            if not token:
                return products


def error_message(payload):
    try:
        return json.loads(payload)['error']['message']
    except (ValueError, KeyError, TypeError):
        return (payload or b'').decode(errors='replace')[:300] or 'no detail'


def highest(play):
    edit = play.open_edit()
    try:
        return play.highest(edit)
    finally:
        play.discard_edit(edit)


def choose_version_code(play, version, code, source, refuse_released=False):
    """Return the versionCode to build with, checked against Play in a throwaway edit.

    Builds pushed to the mobile-internal branch take versionCodes between releases, so a code
    derived from the version can already be taken; it then moves to the next free code. A code
    someone set (bundle.android.versionCode or a dispatch override) never moves and must already
    be above Play. With refuse_released (tag pushes), a tester track that already carries a
    release of this version stops the run, so re-running a tag can't ship the version twice.
    Internal testing doesn't count: an internal build of the version doesn't block its release.
    """
    edit = play.open_edit()
    try:
        current = play.highest(edit)
        if refuse_released:
            for track, name in play.release_names(edit):
                if track != 'internal' and name.startswith(f'{version} ('):
                    raise PlayError(f'Track {track} already has release {name!r}. Bump the version '
                                    '(mobile/scripts/bump-version.sh) for a new release')
    finally:
        play.discard_edit(edit)
    if code > current:
        chosen = code
    elif source == DERIVED:
        chosen, source = current + 1, 'next free code on Play'
    else:
        raise PlayError(f'versionCode {code} ({source}) is not above {current}, the highest on Play. '
                        'Raise it, or unset it to take the next free code')
    if chosen > MAX_VERSION_CODE:
        raise PlayError(f'versionCode {chosen} is above {MAX_VERSION_CODE}')
    return {'highest': current, 'version_code': chosen, 'version_code_source': source}


def parse_tracks(track):
    tracks = str(track).split(',')
    for name in tracks:
        if not re.fullmatch(r'[a-z0-9:_-]+', name):
            raise PlayError(f'Unexpected track id {name!r}: give track ids separated by commas, no spaces')
        if name == 'production' or name.endswith(':production'):
            raise PlayError('This tool does not release to production; promote a tested release in Play Console')
    if len(set(tracks)) != len(tracks):
        raise PlayError('A Play track is listed twice')
    return tracks


def upload(play, aab, track, status, version_code, release_name, notes=None, expected_sha256=None):
    if status not in ['draft', 'completed']:
        raise PlayError('Release status must be draft or completed')
    tracks = parse_tracks(track)
    data = Path(aab).read_bytes()
    digest = hashlib.sha256(data).hexdigest()
    if expected_sha256 is not None and (not re.fullmatch(r'[0-9a-f]{64}', expected_sha256)
                                         or digest != expected_sha256):
        raise PlayError('AAB differs from the explicitly verified SHA-256; no Play edit opened')
    edit = play.open_edit()
    committed = False
    try:
        current = play.highest(edit)
        if version_code <= current:
            raise PlayError(f'versionCode {version_code} is not above the highest code on Play ({current})')
        for name in tracks:
            gaps = play.eu_gaps(edit, name)
            if gaps:
                raise PlayError(f'Track {name} is not available in {", ".join(gaps)}. The EU geo-blocking rule '
                                'needs every EU state: add them in Play Console > Test and release > '
                                f'{name} > Countries / regions, then publish again')
        bundle = play.call('POST', f'edits/{edit}/bundles', upload=data, timeout=900)
        if int(bundle.get('versionCode', -1)) != version_code:
            raise PlayError(f'Play read versionCode {bundle.get("versionCode")} from the bundle, expected {version_code}')
        if bundle.get('sha256', '').lower() != digest:
            raise PlayError('Play reports a different SHA-256 for the uploaded bundle')
        release = {'name': release_name, 'versionCodes': [str(version_code)], 'status': status}
        if notes:
            release['releaseNotes'] = [{'language': 'en-US', 'text': notes}]
        for name in tracks:
            play.call('PUT', f'edits/{edit}/tracks/{name}', {'track': name, 'releases': [release]})
        play.call('POST', f'edits/{edit}:validate')
        play.call('POST', f'edits/{edit}:commit')
        committed = True
    finally:
        if not committed:
            play.discard_edit(edit)
    return {'package': play.package, 'track': ','.join(tracks), 'status': status, 'version_code': version_code,
            'release_name': release_name, 'aab_sha256': digest, 'previous_highest_version_code': current}


def geo_check(play):
    """Return (report lines, problems) for every track and every active one-time purchase option."""
    lines, problems = [], []
    edit = play.open_edit()
    try:
        for track in play.call('GET', f'edits/{edit}/tracks').get('tracks', []):
            name = track['track']
            released = any(r.get('versionCodes') for r in track.get('releases', []))
            gaps = play.eu_gaps(edit, name)
            if gaps is None:
                if released and name != 'internal':
                    problems.append(f'track {name} has releases but no country list')
                lines.append(f'track {name}: no country list' + (' (internal testing has none)' if name == 'internal' else ''))
            elif gaps:
                problems.append(f'track {name} is missing {", ".join(gaps)}')
                lines.append(f'track {name}: missing {", ".join(gaps)}')
            else:
                lines.append(f'track {name}: all EU states')
    finally:
        play.discard_edit(edit)
    for product in play.one_time_products():
        restricted = sorted(EU_COUNTRIES & set(product.get('restrictedPaymentCountries', {}).get('regionCodes', [])))
        if restricted:
            problems.append(f'product {product.get("productId")} restricts payment methods in {", ".join(restricted)}')
            lines.append(f'product {product.get("productId")}: payment restricted in {", ".join(restricted)}')
        for option in product.get('purchaseOptions', []):
            label = f'{product.get("productId")}/{option.get("purchaseOptionId")}'
            if option.get('state') != 'ACTIVE':
                lines.append(f'product {label}: {option.get("state", "not active")}, skipped')
                continue
            available = {c.get('regionCode') for c in option.get('regionalPricingAndAvailabilityConfigs', [])
                         if c.get('availability') == 'AVAILABLE'}
            gaps = sorted(EU_COUNTRIES - available)
            if gaps:
                problems.append(f'product {label} is not available in {", ".join(gaps)}')
            lines.append(f'product {label}: ' + (f'missing {", ".join(gaps)}' if gaps else 'all EU states'))
    return lines, problems


LISTING_DIR = MOBILE_DIR / 'store-listing'
TEXT_LIMITS = {'title': 30, 'shortDescription': 80, 'fullDescription': 4000}
IMAGE_TYPES = ['icon', 'featureGraphic', 'phoneScreenshots']
PNG_SIGNATURE = b'\x89PNG\r\n\x1a\n'


def png_size(data):
    """(width, height) from a PNG's IHDR chunk, or None when the bytes aren't a PNG."""
    if len(data) < 24 or data[:8] != PNG_SIGNATURE or data[12:16] != b'IHDR':
        return None
    return int.from_bytes(data[16:20], 'big'), int.from_bytes(data[20:24], 'big')


def load_listing(listing_dir=LISTING_DIR, language=None):
    """Read and validate listing.json and its images. Raises PlayError listing every problem."""
    listing_dir = Path(listing_dir)
    try:
        data = json.loads((listing_dir / 'listing.json').read_text())
    except (OSError, ValueError) as error:
        raise PlayError(f'Cannot read {listing_dir / "listing.json"}: {error}') from None
    problems = []
    text = {}
    for field, limit in TEXT_LIMITS.items():
        value = data.get(field)
        if not isinstance(value, str) or not value.strip():
            problems.append(f'{field} is missing')
            continue
        if len(value) > limit:
            problems.append(f'{field} is {len(value)} characters, over the {limit} limit')
        text[field] = value
    shots = data.get('screenshots')
    if not isinstance(shots, list):
        shots = []
    if not 2 <= len(shots) <= 8:
        problems.append(f'{len(shots)} phone screenshots; Play takes 2 to 8')
    wanted = [('icon', 'icon.png'), ('featureGraphic', 'feature-graphic.png')]
    wanted += [('phoneScreenshots', shot.get('file') if isinstance(shot, dict) else None) for shot in shots]
    images = {kind: [] for kind in IMAGE_TYPES}
    for kind, name in wanted:
        if not isinstance(name, str) or not name:
            problems.append(f'a {kind} entry has no file')
            continue
        path = listing_dir / name
        try:
            content = path.read_bytes()
        except OSError:
            problems.append(f'{name} does not exist')
            continue
        size = png_size(content)
        if size is None:
            problems.append(f'{name} is not a PNG')
            continue
        width, height = size
        if kind == 'icon' and size != (512, 512):
            problems.append(f'{name} is {width}x{height}; the icon must be 512x512')
        elif kind == 'featureGraphic' and size != (1024, 500):
            problems.append(f'{name} is {width}x{height}; the feature graphic must be 1024x500')
        elif kind == 'phoneScreenshots' and not (320 <= min(size) and max(size) <= 3840 and max(size) <= 2.3 * min(size)):
            problems.append(f'{name} is {width}x{height}; screenshots need sides of 320-3840 px and at most 2.3:1')
        images[kind].append({'file': name, 'data': content, 'sha256': hashlib.sha256(content).hexdigest()})
    if problems:
        raise PlayError('Store listing is not valid:\n  ' + '\n  '.join(problems))
    return {'language': language or data.get('language') or 'en-US', 'text': text, 'images': images}


def read_live_listing(play, edit, language):
    try:
        listing = play.call('GET', f'edits/{edit}/listings/{language}')
    except PlayError as error:
        if error.status != 404:
            raise
        listing = {}
    images = {kind: play.call('GET', f'edits/{edit}/listings/{language}/{kind}').get('images', [])
              for kind in IMAGE_TYPES}
    return {'listing': listing, 'images': images}


def compare_listing(local, live):
    """Fields and image types that differ, each with what a person needs to read about it."""
    text = {}
    for field in TEXT_LIMITS:
        old, new = live['listing'].get(field, ''), local['text'][field]
        if old != new:
            text[field] = (old, new)
    images = {}
    for kind in IMAGE_TYPES:
        remote = [(image.get('sha256') or '').lower() for image in live['images'].get(kind, [])]
        mine = local['images'][kind]
        rows = []
        for index in range(max(len(remote), len(mine))):
            name = mine[index]['file'] if index < len(mine) else f'live image {index + 1}'
            if index >= len(remote):
                rows.append((name, 'added'))
            elif index >= len(mine):
                rows.append((name, 'removed'))
            else:
                rows.append((name, 'same' if remote[index] == mine[index]['sha256'] else 'changed'))
        if any(state != 'same' for _, state in rows):
            images[kind] = rows
        else:
            images[kind] = None
    return {'text': text, 'images': images}


def describe_differences(play, local, differences, max_diff_lines=24):
    lines = [f'Store listing {local["language"]} for {play.package} (live on Play vs local):']
    for field in TEXT_LIMITS:
        if field not in differences['text']:
            lines.append(f'{field}: same')
            continue
        old, new = differences['text'][field]
        lines.append(f'{field}: changed ({len(old)} -> {len(new)} characters)')
        diff = list(difflib.unified_diff(old.splitlines(), new.splitlines(), 'live', 'local', n=0, lineterm=''))
        shown = [line for line in diff if not line.startswith('@@')]
        for line in shown[:max_diff_lines]:
            lines.append('    ' + (line if len(line) <= 160 else line[:157] + '...'))
        if len(shown) > max_diff_lines:
            lines.append(f'    ... {len(shown) - max_diff_lines} more diff lines')
    for kind in IMAGE_TYPES:
        rows = differences['images'][kind]
        if rows is None:
            lines.append(f'{kind}: same ({len(local["images"][kind])})')
            continue
        lines.append(f'{kind}: differs')
        for index, (name, state) in enumerate(rows, 1):
            lines.append(f'    {index}. {name}: {state}')
    if not differences['text'] and not any(differences['images'].values()):
        lines.append('in sync')
    return lines


def listing_diff(play, local):
    edit = play.open_edit()
    try:
        live = read_live_listing(play, edit, local['language'])
    finally:
        play.discard_edit(edit)
    differences = compare_listing(local, live)
    return differences, describe_differences(play, local, differences)


def listing_publish(play, local, confirm):
    """Apply only what differs in one edit, commit it, then verify through a fresh edit."""
    if confirm != play.package:
        raise PlayError(f'--confirm must be the package name ({play.package}); nothing was sent to Play')
    language = local['language']
    edit = play.open_edit()
    committed = False
    try:
        live = read_live_listing(play, edit, language)
        differences = compare_listing(local, live)
        lines = describe_differences(play, local, differences)
        changed_images = [kind for kind in IMAGE_TYPES if differences['images'][kind] is not None]
        if not differences['text'] and not changed_images:
            return lines, 'nothing to publish'
        if differences['text']:
            body = {'language': language, **local['text']}
            if live['listing'].get('video'):
                body['video'] = live['listing']['video']
            play.call('PUT', f'edits/{edit}/listings/{language}', body)
        for kind in changed_images:
            play.call('DELETE', f'edits/{edit}/listings/{language}/{kind}')
            for image in local['images'][kind]:
                uploaded = play.call('POST', f'edits/{edit}/listings/{language}/{kind}', upload=image['data'],
                                     content_type='image/png', timeout=300)
                sha = (uploaded.get('image', {}).get('sha256') or '').lower()
                if sha and sha != image['sha256']:
                    raise PlayError(f'Play reports a different SHA-256 for {image["file"]}')
        play.call('POST', f'edits/{edit}:validate')
        try:
            play.call('POST', f'edits/{edit}:commit')
        except Exception as error:
            # A timeout or dropped connection can arrive after Play applied the commit.
            raise PlayError(f'The commit request failed ({error}); whether Play applied it is unknown. '
                            'Run `listing diff` to see the live state before publishing again') from None
        committed = True
    finally:
        if not committed:
            play.discard_edit(edit)
    after, _ = listing_diff(play, local)
    if after['text'] or any(after['images'].values()):
        left = list(after['text']) + [kind for kind, rows in after['images'].items() if rows]
        raise VerifyError('The commit succeeded, but re-reading Play through a fresh edit still differs from '
                          'local in: ' + ', '.join(left) + '. Run `listing diff` to compare again', lines)
    return lines, 'committed; a fresh edit matches local text and image SHA-256s'


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest='command', required=True)
    version = commands.add_parser('version')
    version.add_argument('--config', type=Path, default=MOBILE_DIR / 'src-tauri/tauri.conf.json')
    version.add_argument('--tag')
    version.add_argument('--version-code')
    for name in ['highest', 'next-code', 'upload', 'geo-check']:
        sub = commands.add_parser(name)
        sub.add_argument('--service-account', type=Path, required=True)
        sub.add_argument('--package', default=PACKAGE)
    nxt = commands.choices['next-code']
    nxt.add_argument('--version', required=True)
    nxt.add_argument('--version-code', type=int, required=True)
    nxt.add_argument('--version-code-source', required=True)
    nxt.add_argument('--refuse-released', action='store_true',
                     help='Fail when a tester track already has a release of --version')
    up = commands.choices['upload']
    up.add_argument('--aab', type=Path, required=True)
    up.add_argument('--track', required=True)
    up.add_argument('--status', choices=['draft', 'completed'], default='draft')
    up.add_argument('--version-code', type=int, required=True)
    up.add_argument('--release-name', required=True)
    up.add_argument('--notes-file', type=Path)
    up.add_argument('--expected-aab-sha256', help='Reject changed bytes before opening a Play edit')
    store = commands.add_parser('listing', help='diff (default, read-only) or publish the store listing')
    store.add_argument('action', nargs='?', choices=['diff', 'publish'], default='diff')
    store.add_argument('--service-account', type=Path, required=True)
    store.add_argument('--package', default=PACKAGE)
    store.add_argument('--listing-dir', type=Path, default=LISTING_DIR)
    store.add_argument('--language', help="Default: listing.json's language, else en-US")
    store.add_argument('--confirm', help='publish only: the package name, to show you mean the live store page')
    args = parser.parse_args(argv)
    try:
        if args.command == 'listing':
            if args.action == 'publish' and args.confirm != args.package:
                raise PlayError(f'listing publish needs --confirm {args.package}; nothing was sent to Play')
            local = load_listing(args.listing_dir, args.language)
            store_play = Play.from_file(args.service_account, package=args.package)
            if args.action == 'diff':
                print('\n'.join(listing_diff(store_play, local)[1]))
            else:
                try:
                    lines, result = listing_publish(store_play, local, args.confirm)
                except VerifyError as error:
                    print('\n'.join(error.lines))
                    raise
                print('\n'.join(lines))
                print(f'Publish: {result}')
            return 0
        if args.command == 'version':
            print(json.dumps(resolve_version(args.config, args.tag, args.version_code)))
        elif args.command == 'highest':
            print(highest(Play.from_file(args.service_account, package=args.package)))
        elif args.command == 'next-code':
            print(json.dumps(choose_version_code(Play.from_file(args.service_account, package=args.package),
                                                 args.version, args.version_code, args.version_code_source,
                                                 args.refuse_released)))
        elif args.command == 'geo-check':
            lines, problems = geo_check(Play.from_file(args.service_account, package=args.package))
            print('\n'.join(lines))
            if problems:
                for problem in problems:
                    print(f'EU geo-blocking: {problem}', file=sys.stderr)
                return 1
        else:
            notes = args.notes_file.read_text().strip() if args.notes_file else None
            result = upload(Play.from_file(args.service_account, package=args.package), args.aab,
                            args.track, args.status, args.version_code, args.release_name, notes, args.expected_aab_sha256)
            print(json.dumps(result, indent=2))
    except (PlayError, OSError, ValueError, subprocess.SubprocessError) as error:
        print(f'Play {args.command} failed: {error}', file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
