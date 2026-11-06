#!/usr/bin/env python3
"""Build a signed Play candidate from local credentials; never uploads or installs it."""
import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys


def run(args, **kwargs):
    return subprocess.run(args, check=True, **kwargs)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    config_dir = Path.home() / '.config/atv-optimizer'
    parser.add_argument('--signing-config', type=Path, default=config_dir / 'play-upload.json')
    parser.add_argument('--license-key-file', type=Path, default=config_dir / 'play-license-key')
    parser.add_argument('--version-code', type=int, required=True)
    parser.add_argument('--previous-version-code', type=int, required=True,
                        help='Highest code on every Play track; use 0 only for a new app with no uploads')
    parser.add_argument('--target', nargs='+', choices=['aarch64', 'armv7', 'i686', 'x86_64'],
                        default=['aarch64', 'armv7', 'i686', 'x86_64'])
    parser.add_argument('--preflight-only', action='store_true')
    parser.add_argument('--previous-version-source', default='operator', choices=['operator', 'play-api', 'smoke'],
                        help='Who confirmed --previous-version-code; recorded in the receipt')
    parser.add_argument('--min-free-gib', type=int, default=40,
                        help='Refuse to build below this much free disk (CI runners pass less)')
    parser.add_argument('--build-jobs', type=int, default=1,
                        help='Cargo/CMake/Gradle parallelism; stays 1 on workstations')
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    env = os.environ.copy()
    if env.get('VITE_DEMO') == '1':
        raise ValueError('VITE_DEMO=1 is fixture data and must not be used for a release')
    if not 0 <= args.previous_version_code < args.version_code <= 2100000000:
        raise ValueError('Choose a versionCode newer than every Play upload and at most 2100000000')
    if subprocess.check_output(['git', 'status', '--porcelain'], cwd=root, text=True).strip():
        raise ValueError('Commit the complete candidate before building; the receipt needs an exact clean revision')
    for path in [args.signing_config, args.license_key_file]:
        if not path.is_file():
            raise ValueError(f'Missing local release input: {path}')
    signing = json.loads(args.signing_config.read_text())
    mapping = {
        'storeFile': 'ATVOPT_KEYSTORE_PATH',
        'storePassword': 'ATVOPT_KEYSTORE_PASSWORD',
        'keyAlias': 'ATVOPT_KEYSTORE_ALIAS',
        'keyPassword': 'ATVOPT_KEYSTORE_KEY_PASSWORD',
    }
    for key, variable in mapping.items():
        value = signing.get(key)
        if not isinstance(value, str) or not value:
            raise ValueError(f'Signing config is missing {key}; no credential values are printed')
        env[variable] = value
    if not Path(signing['storeFile']).is_absolute() or not Path(signing['storeFile']).is_file():
        raise ValueError('storeFile must be an existing absolute keystore path')
    public_key = ''.join(args.license_key_file.read_text().split())
    der = base64.b64decode(public_key, validate=True)
    fixtures = json.loads((root / 'src-tauri/test-fixtures/play-billing-vectors.json').read_text())
    if public_key in [fixtures['play_public_key'], fixtures['other_public_key']]:
        raise ValueError('A billing test-fixture public key must never be used in a Play release')
    # rsa works with both the Mac's LibreSSL and OpenSSL; pkey -pubcheck does not.
    parsed = run(['openssl', 'rsa', '-pubin', '-inform', 'DER', '-text', '-noout'],
                 input=der, capture_output=True, timeout=30).stdout.decode()
    bits = re.search(r'Public-Key: \((\d+) bit', parsed)
    if not bits or int(bits.group(1)) < 2048:
        raise ValueError('The Play Console license public key must be RSA with at least 2048 bits')
    jdk = Path(env.get('JAVA_HOME', ''))
    keytool = str(jdk / 'bin/keytool') if (jdk / 'bin/keytool').is_file() else shutil.which('keytool')
    if not keytool:
        raise ValueError('Set JAVA_HOME to the installed Android build JDK')
    cert = run([keytool, '-exportcert', '-keystore', signing['storeFile'], '-alias', signing['keyAlias'],
                '-storepass:env', 'ATVOPT_KEYSTORE_PASSWORD'], env=env, capture_output=True).stdout
    sdk = Path(env.get('ANDROID_HOME', ''))
    ndk = Path(env.get('NDK_HOME', ''))
    if not (sdk / 'platforms/android-36/android.jar').is_file() or not (ndk / 'source.properties').is_file():
        raise ValueError('Set ANDROID_HOME and NDK_HOME to the installed SDK 36 and NDK')
    free = shutil.disk_usage(root).free
    if free < args.min_free_gib * 1024 ** 3:
        raise ValueError(f'Less than {args.min_free_gib} GiB free: inspect owned output and expected build growth before continuing')
    if args.build_jobs < 1:
        raise ValueError('--build-jobs must be at least 1')
    listing = json.loads((root / 'store-listing/listing.json').read_text())
    policy = run(['curl', '--fail', '--silent', '--show-error', '--max-time', '30',
                  listing['privacyPolicyUrl']], capture_output=True).stdout
    if listing['supportEmail'].encode() not in policy or b'ATV Optimizer Privacy Policy' not in policy:
        raise ValueError('Live privacy policy does not match the approved app/contact')
    env.update({
        'CARGO_BUILD_JOBS': str(args.build_jobs),
        'CMAKE_BUILD_PARALLEL_LEVEL': str(args.build_jobs),
        'ATVOPT_DISTRIBUTION': 'play',
        'ATVOPT_PLAY_LICENSE_KEY': public_key,
        'ATVOPT_TELEMETRY_URL': 'https://atv-optimizer-telemetry.bryanroscoe.workers.dev',
    })
    env.pop('ATVOPT_ALLOW_UNSIGNED_RELEASE', None)
    env.pop('VITE_DEMO', None)
    env['GRADLE_OPTS'] = (env.get('GRADLE_OPTS', '') +
                          f' -Dorg.gradle.workers.max={args.build_jobs} -Dorg.gradle.daemon=false')
    receipt = {
        'source_sha': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip(),
        'version_code': args.version_code,
        'previous_version_code': args.previous_version_code,
        'previous_version_code_source': args.previous_version_source,
        'targets': args.target,
        'upload_certificate_sha256': hashlib.sha256(cert).hexdigest(),
        'play_license_key_sha256': hashlib.sha256(der).hexdigest(),
        'privacy_policy_sha256': hashlib.sha256(policy).hexdigest(),
    }
    print('Play release preflight passed. No private credentials were printed.', flush=True)
    if args.preflight_only:
        return
    config = json.dumps({'bundle': {'android': {'versionCode': args.version_code}}})
    run(['npm', 'run', 'tauri', '--', 'android', 'build', '--aab', '--apk', '--ci',
         '--config', config, '--target', *args.target], cwd=root, env=env, timeout=7200)
    output = root / 'src-tauri/gen/android/app/build/outputs'
    receipt['artifacts'] = []
    for artifact in sorted(output.rglob('*')):
        if artifact.suffix not in ['.aab', '.apk'] or 'release' not in artifact.name:
            continue
        receipt['artifacts'].append({
            'path': str(artifact.relative_to(output)),
            'size_bytes': artifact.stat().st_size,
            'sha256': hashlib.sha256(artifact.read_bytes()).hexdigest(),
        })
    if not any(a['path'].endswith('.aab') for a in receipt['artifacts']):
        raise ValueError('Build completed without a release AAB')
    receipt['remaining_gates'] = [
        'AAB signature and expected upload certificate', 'bundletool validation and generated APK verification',
        'APK signature, non-debuggable manifest, package, SDK, versionCode and 16 KiB checks',
        'actual 16 KiB runtime validation', 'Play pre-launch report', 'phone/TV and purchase matrix',
    ]
    path = output / 'play-build-receipt.json'
    path.write_text(json.dumps(receipt, indent=2) + '\n')
    print(f'Build receipt: {path}. Complete its remaining gates before upload.')


if __name__ == '__main__':
    try:
        main()
    except (ValueError, OSError, subprocess.SubprocessError) as error:
        # Child output is deliberately not dumped: signing inputs stay out of logs.
        print(f'Play release preparation failed: {error}', file=sys.stderr)
        sys.exit(1)
