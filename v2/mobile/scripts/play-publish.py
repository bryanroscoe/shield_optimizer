#!/usr/bin/env python3
"""Resolve the release versionCode and talk to the Google Play Developer API.

Subcommands:
  version   Read tauri.conf.json, check a mobile-v* tag against it, print the versionCode.
  highest   Print the highest versionCode Play already holds (every bundle, APK and track).
  upload    Upload a verified AAB once and release it on one or more tracks (comma-separated),
            as a draft unless told otherwise, in a single edit. Refuses a track whose country
            list leaves out an EU state.
  geo-check Read-only: fail when a track or an active one-time product leaves out an EU state.

EU Geo-blocking Regulation (EU) 2018/302: Play asks that an app distributed in the EU be
accessible EU-wide, so every track with a country list and every active Pro product must
cover all 27 member states (v2/mobile/PLAY-CONSOLE-WALKTHROUGH.md, requirements table).

Standard library plus the `openssl` binary only, so the release path has no third-party Python
dependency. Credentials are read from files and never printed; API errors print Google's
message, which never contains them.
"""
import argparse
import base64
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
MOBILE_DIR = Path(__file__).resolve().parents[1]
EU_COUNTRIES = frozenset('AT BE BG CY CZ DE DK EE ES FI FR GR HR HU IE IT LT LU LV MT NL PL PT RO SE SI SK'.split())


class PlayError(Exception):
    pass


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
        source = 'derived from version'
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

    def call(self, method, path, body=None, *, upload=None, timeout=120):
        if self.token is None:
            self.authorize()
        headers = {'Authorization': f'Bearer {self.token}'}
        if upload is not None:
            url = f'{UPLOAD_API}/{self.package}/{path}?uploadType=media'
            headers['Content-Type'] = 'application/octet-stream'
            data = upload
        else:
            url = f'{API}/{self.package}/{path}'
            data = None if body is None else json.dumps(body).encode()
            if data is not None:
                headers['Content-Type'] = 'application/json'
        status, payload = self.http(method, url, headers, data, timeout)
        if not 200 <= status < 300:
            raise PlayError(f'{method} {path.split("?")[0]} failed (HTTP {status}): {error_message(payload)}')
        return json.loads(payload) if payload else {}

    def open_edit(self):
        return self.call('POST', 'edits', {})['id']

    def discard_edit(self, edit):
        try:
            self.call('DELETE', f'edits/{edit}')
        except PlayError:
            pass

    def highest(self, edit):
        codes = [0]
        codes += [int(b['versionCode']) for b in self.call('GET', f'edits/{edit}/bundles').get('bundles', [])]
        codes += [int(a['versionCode']) for a in self.call('GET', f'edits/{edit}/apks').get('apks', [])]
        for track in self.call('GET', f'edits/{edit}/tracks').get('tracks', []):
            for release in track.get('releases', []):
                codes += [int(code) for code in release.get('versionCodes', [])]
        return max(codes)

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


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest='command', required=True)
    version = commands.add_parser('version')
    version.add_argument('--config', type=Path, default=MOBILE_DIR / 'src-tauri/tauri.conf.json')
    version.add_argument('--tag')
    version.add_argument('--version-code')
    for name in ['highest', 'upload', 'geo-check']:
        sub = commands.add_parser(name)
        sub.add_argument('--service-account', type=Path, required=True)
        sub.add_argument('--package', default=PACKAGE)
    up = commands.choices['upload']
    up.add_argument('--aab', type=Path, required=True)
    up.add_argument('--track', required=True)
    up.add_argument('--status', choices=['draft', 'completed'], default='draft')
    up.add_argument('--version-code', type=int, required=True)
    up.add_argument('--release-name', required=True)
    up.add_argument('--notes-file', type=Path)
    up.add_argument('--expected-aab-sha256', help='Reject changed bytes before opening a Play edit')
    args = parser.parse_args(argv)
    try:
        if args.command == 'version':
            print(json.dumps(resolve_version(args.config, args.tag, args.version_code)))
        elif args.command == 'highest':
            print(highest(Play.from_file(args.service_account, package=args.package)))
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
