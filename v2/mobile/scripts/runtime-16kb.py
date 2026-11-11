#!/usr/bin/env python3
"""Verify a retained release candidate, then smoke-test it on a disposable 16 KB emulator."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess
import time
import xml.etree.ElementTree as ET
import zipfile

PACKAGE = 'com.atvoptimizer.mobile'
APK_PATH = 'apk/universal/release/app-universal-release.apk'
AAB_PATH = 'bundle/universalRelease/app-universal-release.aab'
LIBRARY = 'libatv_optimizer_mobile_lib.so'
WORKFLOW = '.github/workflows/mobile-play-release.yml'


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(path):
    result = hashlib.sha256()
    with Path(path).open('rb') as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b''):
            result.update(chunk)
    return result.hexdigest()


def read_json(path):
    return json.loads(Path(path).read_text())


def select_artifact(run, artifacts, comparison, repository, run_id, source_sha):
    require(re.fullmatch(r'[0-9a-f]{40}', source_sha), 'Expected a full lowercase source SHA')
    require(str(run['id']) == run_id and run['head_sha'] == source_sha, 'Source run/SHA mismatch')
    require(run['repository']['full_name'] == repository
            and run['head_repository']['full_name'] == repository, 'Source run is not from this repository')
    require(run['path'] == WORKFLOW and run['event'] == 'workflow_dispatch'
            and run['head_branch'] == 'main', 'Require a manually dispatched main release build')
    require(run['status'] == 'completed' and run['conclusion'] == 'success', 'Source run did not succeed')
    require(comparison['status'] in ('ahead', 'identical')
            and comparison['merge_base_commit']['sha'] == source_sha, 'Source is not an ancestor of main')
    require(artifacts['total_count'] == len(artifacts['artifacts']), 'Artifact listing is incomplete')
    matches = [a for a in artifacts['artifacts'] if a['name'].startswith('atv-optimizer-android-')]
    require(len(matches) == 1, 'Require exactly one retained release artifact')
    artifact = matches[0]
    require(not artifact['expired'] and artifact['size_in_bytes'] > 0, 'Artifact is expired or empty')
    origin = artifact['workflow_run']
    require(origin['id'] == run['id'] and origin['head_sha'] == source_sha
            and origin['head_branch'] == 'main'
            and origin['repository_id'] == origin['head_repository_id'] == run['repository']['id'],
            'Artifact run provenance mismatch')
    require(re.fullmatch(r'sha256:[0-9a-f]{64}', artifact['digest']), 'Artifact digest is missing')
    return {'source_run_id': run['id'], 'source_run_attempt': run['run_attempt'],
            'source_sha': source_sha, 'repository': repository, 'artifact_id': artifact['id'],
            'artifact_name': artifact['name'], 'artifact_digest': artifact['digest']}


def verify_candidate(root, provenance, expected_apk_sha):
    require(re.fullmatch(r'[0-9a-f]{64}', expected_apk_sha), 'Expected a full lowercase APK SHA-256')
    root = Path(root).resolve()
    build = read_json(root / 'play-build-receipt.json')
    verification = read_json(root / 'play-verification.json')
    require(build['source_sha'] == provenance['source_sha'], 'Build receipt source mismatch')
    require(build['previous_version_code_source'] == 'play-api', 'Require the real signed release, not smoke')
    code = build['version_code']
    require(type(code) is int and code > 0, 'Invalid version code')
    certificate = build['upload_certificate_sha256']
    require(re.fullmatch(r'[0-9a-f]{64}', certificate), 'Invalid certificate digest')
    require(verification['package'] == PACKAGE and verification['version_code'] == code
            and verification['upload_certificate_sha256'] == certificate, 'Verification identity mismatch')
    require({'aab-jar-signature', 'upload-certificate', 'bundletool-validate',
             'manifest-package-and-versionCode', 'generated-universal-apk', 'apk-signer-certificate'}
            <= set(verification['verified']), 'Bundle verification is incomplete')
    hashes = {}
    for name in (APK_PATH, AAB_PATH):
        path = root / name
        require(path.is_file() and path.resolve().is_relative_to(root), 'Missing or unsafe candidate path')
        rows = [a for a in build['artifacts'] if a['path'] == name]
        require(len(rows) == 1, 'Missing or ambiguous build artifact receipt')
        hashes[name] = digest(path)
        require(path.stat().st_size == rows[0]['size_bytes'] and hashes[name] == rows[0]['sha256'],
                'Candidate size/hash differs from build receipt')
    require(hashes[APK_PATH] == expected_apk_sha, 'APK differs from the explicitly selected candidate')
    require(verification['aab_sha256'] == hashes[AAB_PATH], 'AAB verification hash mismatch')
    apks = [a for a in verification['apks'] if a['apk'] == Path(APK_PATH).name]
    require(len(apks) == 1 and apks[0]['sha256'] == expected_apk_sha
            and apks[0]['version_code'] == code and apks[0]['target_sdk'] == 36,
            'APK verification receipt mismatch')
    require({'signature', 'non-debuggable', 'application-id', 'zip-16kb', 'elf-load-alignment-by-abi'}
            <= set(apks[0]['verified']), 'APK verification is incomplete')
    with zipfile.ZipFile(root / APK_PATH) as apk, zipfile.ZipFile(root / AAB_PATH) as aab:
        member = f'lib/x86_64/{LIBRARY}'
        require(apk.namelist().count(member) == 1 and aab.namelist().count('base/' + member) == 1,
                'Missing or ambiguous x86_64 native library')
        native = apk.read(member)
        require(native == aab.read('base/' + member), 'APK/AAB x86_64 native library mismatch')
    return {**provenance, 'package': PACKAGE, 'version_code': code,
            'apk_sha256': expected_apk_sha, 'aab_sha256': hashes[AAB_PATH],
            'upload_certificate_sha256': certificate,
            'native_x86_64_sha256': hashlib.sha256(native).hexdigest()}


def verify_environment(page_size, api, abi):
    require(page_size.strip() == '16384', 'Runtime PAGE_SIZE must be exactly 16384; no 4 KB fallback')
    require(api.strip() == '36' and abi.strip() == 'x86_64', 'Require Android 16/API36 x86_64')


def verify_started(pid, activities, hierarchy, logs):
    require(re.fullmatch(r'[1-9][0-9]*', pid.strip()), 'App process is missing or ambiguous')
    require(re.search(r'topResumedActivity=.*com\.atvoptimizer\.mobile/\.MainActivity\b', activities),
            'MainActivity is not foreground')
    root = ET.fromstring(hierarchy)
    labels = {n.get('text') for n in root.iter('node') if n.get('package') == PACKAGE}
    require({'Find your TV', 'Enter IP address manually'} <= labels, 'Real onboarding did not render')
    require(re.search(r'nativeloader: Load .*lib/x86_64/' + re.escape(LIBRARY) + r'.*: ok\s*$', logs, re.M),
            'No successful x86_64 native-loader evidence')
    require(not re.search(r'FATAL EXCEPTION|Fatal signal|UnsatisfiedLinkError|dlopen failed|ANR in ' + PACKAGE,
                          logs), 'App crashed or failed native loading')


def verify_process(expected_pid, current_pid, logs, crash_logs):
    require(re.fullmatch(r'[1-9][0-9]*', expected_pid), 'Launch did not create one app process')
    require(current_pid == expected_pid, 'App process died or changed during this launch')
    require(not re.search(r'FATAL EXCEPTION|Fatal signal|UnsatisfiedLinkError|dlopen failed', logs),
            'App crashed or failed native loading during this launch')
    require('Process: ' + PACKAGE + ',' not in crash_logs
            and '>>> ' + PACKAGE + ' <<<' not in crash_logs,
            'Crash buffer contains an earlier app crash')


def command(args, timeout=120, binary=False):
    result = subprocess.run(args, check=True, capture_output=True, text=not binary, timeout=timeout)
    return result.stdout if binary else result.stdout.strip()


def runtime(root, evidence, provenance):
    evidence.mkdir(parents=True, exist_ok=True)
    receipt = {**provenance, 'status': 'failed', 'tested_abi': 'x86_64',
               'qa_workflow_sha': os.environ.get('GITHUB_SHA'),
               'qa_run_id': os.environ.get('GITHUB_RUN_ID'),
               'qa_run_attempt': os.environ.get('GITHUB_RUN_ATTEMPT'),
               'not_proven': ['ARM64 runtime', 'Play-generated splits', 'billing purchases',
                              'phone/TV acceptance', 'Play pre-launch report']}
    serial = 'emulator-5554'

    def adb(*args, **kwargs):
        return command(['adb', '-s', serial, *args], **kwargs)

    try:
        page, api, abi = (adb('shell', 'getconf', 'PAGE_SIZE'),
                          adb('shell', 'getprop', 'ro.build.version.sdk'),
                          adb('shell', 'getprop', 'ro.product.cpu.abi'))
        verify_environment(page, api, abi)
        receipt.update(page_size=int(page), api_level=int(api), system_abi=abi)
        apk = root / APK_PATH
        spec = importlib.util.spec_from_file_location('apk_gate', Path(__file__).with_name('verify-release-apk.py'))
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        checked = module.verify(apk, 'apksigner', 'zipalign', 'apkanalyzer')
        require(checked['sha256'] == receipt['apk_sha256']
                and checked['version_code'] == receipt['version_code'], 'Fresh APK verification mismatch')
        certs = command(['apksigner', 'verify', '--print-certs', str(apk)])
        signers = re.findall(r'^Signer #[0-9]+ certificate SHA-256 digest: ([0-9a-f]+)$', certs, re.M)
        require(signers == [receipt['upload_certificate_sha256']], 'Unexpected APK signing certificate')
        adb('logcat', '-c')
        adb('install', '--no-streaming', str(apk))
        package = adb('shell', 'dumpsys', 'package', PACKAGE)
        require('primaryCpuAbi=x86_64' in package, 'Installed APK did not select x86_64')
        paths = adb('shell', 'pm', 'path', PACKAGE).splitlines()
        require(len(paths) == 1 and paths[0].startswith('package:/data/app/')
                and paths[0].endswith('/base.apk'), 'Unexpected installed APK path')
        installed_hash = adb('shell', 'sha256sum', paths[0][8:]).split()[0]
        require(installed_hash == receipt['apk_sha256'], 'Installed APK hash mismatch')
        (evidence / 'installed-package.txt').write_text(package + '\n')
        for attempt in ('cold', 'restart'):
            adb('shell', 'am', 'force-stop', PACKAGE)
            launch = adb('shell', 'am', 'start', '-W', '-n', PACKAGE + '/.MainActivity')
            (evidence / f'{attempt}-launch.txt').write_text(launch + '\n')
            require('Status: ok' in launch, 'Activity launch failed')
            pid = adb('shell', 'pidof', PACKAGE)
            require(re.fullmatch(r'[1-9][0-9]*', pid), 'Launch did not create one app process')
            deadline = time.monotonic() + 90
            last_error = 'Onboarding not observed'
            while time.monotonic() < deadline:
                logs = adb('logcat', '-d', '--pid=' + pid)
                crash_logs = adb('logcat', '-b', 'crash', '-d')
                (evidence / f'{attempt}-app.log').write_text(logs[-2_000_000:])
                (evidence / 'crashes.log').write_text(crash_logs[-2_000_000:])
                verify_process(pid, adb('shell', 'pidof', PACKAGE), logs, crash_logs)
                try:
                    activities = adb('shell', 'dumpsys', 'activity', 'activities')
                    adb('shell', 'uiautomator', 'dump', '/sdcard/runtime-qa.xml', timeout=20)
                    hierarchy = adb('shell', 'cat', '/sdcard/runtime-qa.xml')
                    logs = adb('logcat', '-d', '--pid=' + pid)
                    (evidence / f'{attempt}-ui.xml').write_text(hierarchy)
                    (evidence / f'{attempt}-app.log').write_text(logs[-2_000_000:])
                    verify_started(pid, activities, hierarchy, logs)
                    break
                except (ValueError, ET.ParseError, subprocess.SubprocessError) as error:
                    last_error = str(error)
                    time.sleep(2)
            else:
                raise ValueError(last_error)
            time.sleep(10)
            verify_process(pid, adb('shell', 'pidof', PACKAGE), adb('logcat', '-d', '--pid=' + pid),
                           adb('logcat', '-b', 'crash', '-d'))
            adb('shell', 'uiautomator', 'dump', '/sdcard/runtime-qa.xml', timeout=20)
            hierarchy = adb('shell', 'cat', '/sdcard/runtime-qa.xml')
            logs = adb('logcat', '-d', '--pid=' + pid)
            verify_started(pid, adb('shell', 'dumpsys', 'activity', 'activities'), hierarchy, logs)
            system_logs = adb('logcat', '-d', '-t', '5000')
            require('ANR in ' + PACKAGE not in system_logs, 'System reported an app ANR')
            (evidence / f'{attempt}-ui.xml').write_text(hierarchy)
            (evidence / f'{attempt}-app.log').write_text(logs[-2_000_000:])
            screenshot = adb('exec-out', 'screencap', '-p', binary=True)
            require(len(screenshot) > 24 and screenshot.startswith(b'\x89PNG\r\n\x1a\n'), 'Screenshot capture failed')
            (evidence / f'{attempt}-onboarding.png').write_bytes(screenshot)
        receipt.update(status='passed', installed_apk_sha256=installed_hash,
                       passed=['signature/package/version/static APK checks', 'PAGE_SIZE=16384',
                               'installed APK hash', 'x86_64 native loader',
                               'foreground onboarding on cold start and restart', 'stable process'])
    except (ValueError, KeyError, OSError, subprocess.SubprocessError, zipfile.BadZipFile) as error:
        receipt['error'] = str(error)
        raise
    finally:
        for name, args, binary in [('logcat.txt', ['logcat', '-d', '-t', '5000'], False),
                                   ('final-screen.png', ['exec-out', 'screencap', '-p'], True)]:
            try:
                result = adb(*args, binary=binary, timeout=20)
                if binary:
                    (evidence / name).write_bytes(result)
                else:
                    (evidence / name).write_text(result[-2_000_000:])
            except (OSError, subprocess.SubprocessError):
                pass
        (evidence / 'runtime-receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='mode', required=True)
    select = sub.add_parser('select')
    for flag in ('run', 'artifacts', 'comparison', 'repository', 'run-id', 'source-sha', 'output'):
        select.add_argument('--' + flag, required=True)
    candidate = sub.add_parser('candidate')
    for flag in ('root', 'provenance', 'apk-sha256', 'output'):
        candidate.add_argument('--' + flag, required=True)
    smoke = sub.add_parser('runtime')
    for flag in ('root', 'evidence', 'provenance'):
        smoke.add_argument('--' + flag, required=True)
    args = parser.parse_args()
    try:
        if args.mode == 'select':
            result = select_artifact(read_json(args.run), read_json(args.artifacts), read_json(args.comparison),
                                     args.repository, args.run_id, args.source_sha)
            Path(args.output).write_text(json.dumps(result, indent=2) + '\n')
            with open(os.environ['GITHUB_OUTPUT'], 'a') as output:
                output.write(f"artifact_id={result['artifact_id']}\n")
        elif args.mode == 'candidate':
            result = verify_candidate(Path(args.root), read_json(args.provenance), args.apk_sha256)
            Path(args.output).write_text(json.dumps(result, indent=2) + '\n')
        else:
            runtime(Path(args.root), Path(args.evidence), read_json(args.provenance))
    except (ValueError, KeyError, OSError, subprocess.SubprocessError, zipfile.BadZipFile) as error:
        parser.exit(1, f'16 KB QA rejected: {error}\n')


if __name__ == '__main__':
    main()
