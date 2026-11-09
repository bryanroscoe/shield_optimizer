#!/usr/bin/env python3
"""Resolve the release versionCode and talk to the Google Play Developer API.

Subcommands:
  version   Read tauri.conf.json, check a mobile-v* tag against it, print the versionCode.
  highest   Print the highest versionCode Play already holds (every bundle, APK and track).
  upload    Upload a verified AAB to one track, as a draft release unless told otherwise.

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


def upload(play, aab, track, status, version_code, release_name, notes=None):
    if status not in ['draft', 'completed']:
        raise PlayError('Release status must be draft or completed')
    data = Path(aab).read_bytes()
    digest = hashlib.sha256(data).hexdigest()
    edit = play.open_edit()
    committed = False
    try:
        current = play.highest(edit)
        if version_code <= current:
            raise PlayError(f'versionCode {version_code} is not above the highest code on Play ({current})')
        bundle = play.call('POST', f'edits/{edit}/bundles', upload=data, timeout=900)
        if int(bundle.get('versionCode', -1)) != version_code:
            raise PlayError(f'Play read versionCode {bundle.get("versionCode")} from the bundle, expected {version_code}')
        if bundle.get('sha256', '').lower() != digest:
            raise PlayError('Play reports a different SHA-256 for the uploaded bundle')
        release = {'name': release_name, 'versionCodes': [str(version_code)], 'status': status}
        if notes:
            release['releaseNotes'] = [{'language': 'en-US', 'text': notes}]
        play.call('PUT', f'edits/{edit}/tracks/{track}', {'track': track, 'releases': [release]})
        play.call('POST', f'edits/{edit}:validate')
        play.call('POST', f'edits/{edit}:commit')
        committed = True
    finally:
        if not committed:
            play.discard_edit(edit)
    return {'package': play.package, 'track': track, 'status': status, 'version_code': version_code,
            'release_name': release_name, 'aab_sha256': digest, 'previous_highest_version_code': current}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest='command', required=True)
    version = commands.add_parser('version')
    version.add_argument('--config', type=Path, default=MOBILE_DIR / 'src-tauri/tauri.conf.json')
    version.add_argument('--tag')
    version.add_argument('--version-code')
    for name in ['highest', 'upload']:
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
    args = parser.parse_args(argv)
    try:
        if args.command == 'version':
            print(json.dumps(resolve_version(args.config, args.tag, args.version_code)))
        elif args.command == 'highest':
            print(highest(Play.from_file(args.service_account, package=args.package)))
        else:
            notes = args.notes_file.read_text().strip() if args.notes_file else None
            result = upload(Play.from_file(args.service_account, package=args.package), args.aab,
                            args.track, args.status, args.version_code, args.release_name, notes)
            print(json.dumps(result, indent=2))
    except (PlayError, OSError, ValueError, subprocess.SubprocessError) as error:
        print(f'Play publish failed: {error}', file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
