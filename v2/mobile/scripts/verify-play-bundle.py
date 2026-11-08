#!/usr/bin/env python3
"""Read-only Play AAB gate: signature, upload certificate, bundletool and generated-APK checks.

Closes the automatable gates the build receipt leaves open: the AAB is JAR-signed by the expected
upload certificate, bundletool validates it, its manifest carries the expected package and
versionCode, and the universal APK bundletool generates from it (plus any Gradle APKs given) passes
verify-release-apk.py and is signed by the same certificate. It never uploads or installs anything.
Keystore passwords are read from environment variables and handed to bundletool through 0600 files.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import zipfile

PACKAGE = 'com.atvoptimizer.mobile'
spec = importlib.util.spec_from_file_location('apk_gate', Path(__file__).with_name('verify-release-apk.py'))
apk_gate = importlib.util.module_from_spec(spec)
spec.loader.exec_module(apk_gate)


def fingerprint(value):
    cleaned = re.sub(r'[^0-9a-f]', '', value.lower())
    if len(cleaned) != 64:
        raise ValueError('Expected a SHA-256 certificate fingerprint (64 hex digits)')
    return cleaned


def secret_file(directory, name, value):
    path = Path(directory) / name
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(fd, 'w') as handle:
        handle.write(value)
    return path


def verify(args, run=subprocess.run):
    def command(argv, timeout=600):
        return run(argv, check=True, capture_output=True, text=True, timeout=timeout).stdout

    expected = fingerprint(args.expected_cert_sha256)
    aab = Path(args.aab)
    if aab.suffix != '.aab' or not aab.is_file():
        raise ValueError('Provide an existing .aab')
    if 'jar verified.' not in command([args.jarsigner, '-verify', str(aab)]):
        raise ValueError('jarsigner did not verify the AAB signature')
    certs = re.findall(r'SHA256:\s*([0-9A-Fa-f:]{95})', command([args.keytool, '-printcert', '-jarfile', str(aab)]))
    if not certs or {fingerprint(c) for c in certs} != {expected}:
        raise ValueError('AAB is not signed by the expected upload certificate')
    bundletool = [args.java, '-jar', str(args.bundletool)]
    command([*bundletool, 'validate', f'--bundle={aab}'])

    def manifest(xpath):
        return command([*bundletool, 'dump', 'manifest', f'--bundle={aab}', f'--xpath={xpath}']).strip()
    if manifest('/manifest/@package') != PACKAGE:
        raise ValueError('AAB package is not ' + PACKAGE)
    if int(manifest('/manifest/@android:versionCode')) != args.version_code:
        raise ValueError(f'AAB versionCode is not {args.version_code}')

    passwords = {name: os.environ.get(name, '') for name in [args.ks_pass_env, args.key_pass_env]}
    if not all(passwords.values()):
        raise ValueError('Keystore password environment variables are not set')
    with tempfile.TemporaryDirectory() as directory:
        ks_pass = secret_file(directory, 'ks-pass', passwords[args.ks_pass_env])
        key_pass = secret_file(directory, 'key-pass', passwords[args.key_pass_env])
        apks = Path(directory) / 'universal.apks'
        command([*bundletool, 'build-apks', f'--bundle={aab}', f'--output={apks}', '--mode=universal',
                 f'--ks={args.keystore}', f'--ks-key-alias={args.key_alias}',
                 f'--ks-pass=file:{ks_pass}', f'--key-pass=file:{key_pass}'], timeout=1200)
        universal = Path(args.output_dir) / 'bundletool-universal.apk'
        with zipfile.ZipFile(apks) as archive:
            universal.write_bytes(archive.read('universal.apk'))

    results = []
    for apk in [universal, *map(Path, args.apk)]:
        result = apk_gate.verify(apk, args.apksigner, args.zipalign, args.apkanalyzer, run=run)
        if result['version_code'] != args.version_code:
            raise ValueError(f'{apk.name} has versionCode {result["version_code"]}, expected {args.version_code}')
        signers = re.findall(r'certificate SHA-256 digest:\s*([0-9a-fA-F]{64})',
                             command([args.apksigner, 'verify', '--print-certs', str(apk)]))
        if not signers or {fingerprint(s) for s in signers} != {expected}:
            raise ValueError(f'{apk.name} is not signed by the expected upload certificate')
        results.append(result)
    return {'aab': aab.name, 'aab_sha256': hashlib.sha256(aab.read_bytes()).hexdigest(),
            'package': PACKAGE, 'version_code': args.version_code, 'upload_certificate_sha256': expected,
            'verified': ['aab-jar-signature', 'upload-certificate', 'bundletool-validate',
                         'manifest-package-and-versionCode', 'generated-universal-apk', 'apk-signer-certificate'],
            'apks': results}


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--aab', required=True)
    parser.add_argument('--apk', action='append', default=[], help='Gradle-built APKs to check as well')
    parser.add_argument('--version-code', type=int, required=True)
    parser.add_argument('--expected-cert-sha256', required=True)
    parser.add_argument('--bundletool', type=Path, required=True)
    parser.add_argument('--keystore', type=Path, required=True)
    parser.add_argument('--key-alias', required=True)
    parser.add_argument('--ks-pass-env', default='ATVOPT_KEYSTORE_PASSWORD')
    parser.add_argument('--key-pass-env', default='ATVOPT_KEYSTORE_KEY_PASSWORD')
    parser.add_argument('--output-dir', type=Path, required=True)
    parser.add_argument('--evidence', type=Path, help='Write the JSON result here too')
    for tool in ['java', 'jarsigner', 'keytool', 'apksigner', 'zipalign', 'apkanalyzer']:
        parser.add_argument(f'--{tool}', default=tool)
    args = parser.parse_args()
    try:
        result = json.dumps(verify(args), indent=2)
    except (ValueError, OSError, KeyError, subprocess.SubprocessError, zipfile.BadZipFile) as error:
        # Passwords only ever reach tools as file paths, so a failing tool's stderr is safe to show.
        detail = getattr(error, 'stderr', None) or ''
        tail = '\n'.join(detail.strip().splitlines()[-20:])
        parser.exit(1, f'Play bundle rejected: {error}\n' + (f'{tail}\n' if tail else ''))
    if args.evidence:
        args.evidence.write_text(result + '\n')
    print(result)


if __name__ == '__main__':
    sys.exit(main())
