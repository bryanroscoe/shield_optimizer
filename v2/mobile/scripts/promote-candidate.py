#!/usr/bin/env python3
"""Fail-closed provenance gates for manually promoting retained Internal candidates."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import re

spec = importlib.util.spec_from_file_location('runtime_gate', Path(__file__).with_name('runtime-16kb.py'))
runtime = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runtime)
require = runtime.require
read_json = runtime.read_json
CI_JOBS = {'Rust tests on ubuntu-22.04', 'Rust tests on macos-latest',
           'Rust tests on windows-latest', 'Frontend type check',
           'Mobile frontend type check', 'End-to-end (simulated devices)', 'v2 Tests OK'}


def verified_run(run, repository, run_id, workflow, events):
    require(str(run['id']) == str(run_id), 'Run id differs')
    require(run['repository']['full_name'] == repository
            and run['head_repository']['full_name'] == repository, 'Foreign repository run')
    require(run['path'] == workflow and run['event'] in events
            and run['head_branch'] == 'main', 'Unexpected workflow, event or branch')
    require(run['status'] == 'completed' and run['conclusion'] == 'success', 'Run has not succeeded')


def ancestor(comparison, sha):
    require(comparison['status'] in ('ahead', 'identical')
            and comparison['merge_base_commit']['sha'] == sha, 'Revision is no longer on main')


def plan(data, repository, build_id, source_sha, qa_id, ci_id):
    source = runtime.select_artifact(data['build_run'], data['build_artifacts'],
                                     data['source_comparison'], repository, str(build_id), source_sha)
    qa = data['qa_run']
    verified_run(qa, repository, qa_id, '.github/workflows/mobile-16kb-runtime.yml', {'workflow_dispatch'})
    ancestor(data['qa_comparison'], qa['head_sha'])
    ci = data['ci_run']
    verified_run(ci, repository, ci_id, '.github/workflows/v2-tests.yml', {'push', 'workflow_dispatch'})
    require(ci['head_sha'] == source_sha, 'Source CI tested another revision')
    jobs = data['ci_jobs']
    require(jobs['total_count'] == len(jobs['jobs']), 'Incomplete CI jobs')
    for name in CI_JOBS:
        matches = [j for j in jobs['jobs'] if j['name'] == name]
        require(len(matches) == 1 and matches[0]['conclusion'] == 'success'
                and matches[0]['status'] == 'completed'
                and matches[0]['run_id'] == ci['id']
                and matches[0]['head_sha'] == source_sha
                and matches[0]['run_attempt'] == ci['run_attempt'], 'Missing/failed/wrong-attempt CI job: ' + name)
    listing = data['qa_artifacts']
    require(listing['total_count'] == len(listing['artifacts']), 'Incomplete QA artifact listing')
    matches = [a for a in listing['artifacts'] if a['name'] == f"mobile-16kb-runtime-{qa['id']}"]
    require(len(matches) == 1, 'Missing or ambiguous runtime evidence')
    artifact = matches[0]
    require(not artifact['expired'] and artifact['size_in_bytes'] > 0, 'Expired/empty runtime evidence')
    origin = artifact['workflow_run']
    require(origin['id'] == qa['id'] and origin['head_sha'] == qa['head_sha']
            and origin['head_branch'] == 'main'
            and origin['repository_id'] == origin['head_repository_id'] == qa['repository']['id'],
            'Runtime artifact provenance differs')
    require(re.fullmatch(r'sha256:[0-9a-f]{64}', artifact['digest']), 'Missing runtime artifact digest')
    return {**source, 'qa_run_id': qa['id'], 'qa_run_attempt': qa['run_attempt'],
            'qa_workflow_sha': qa['head_sha'], 'qa_artifact_id': artifact['id'],
            'qa_artifact_digest': artifact['digest'], 'ci_run_id': ci['id'],
            'ci_run_attempt': ci['run_attempt']}


def verify(root, evidence, selected, apk_sha, aab_sha, certificate):
    require(re.fullmatch(r'[0-9a-f]{64}', aab_sha), 'Expected explicit AAB SHA-256')
    require(re.fullmatch(r'[0-9a-f]{64}', certificate), 'Expected configured upload certificate')
    candidate = runtime.verify_candidate(root, selected, apk_sha)
    require(candidate['aab_sha256'] == aab_sha, 'AAB differs from explicit selection')
    require(candidate['upload_certificate_sha256'] == certificate, 'Configured upload certificate differs')
    receipt = read_json(Path(evidence) / 'runtime-receipt.json')
    for field in ('source_run_id', 'source_run_attempt', 'source_sha', 'repository', 'artifact_id',
                  'artifact_name', 'artifact_digest', 'package', 'version_code', 'apk_sha256',
                  'aab_sha256', 'upload_certificate_sha256', 'native_x86_64_sha256',
                  'qa_workflow_sha'):
        require(receipt[field] == candidate[field], 'Runtime candidate identity differs: ' + field)
    for field in ('qa_run_id', 'qa_run_attempt'):
        require(str(receipt[field]) == str(selected[field]), 'Runtime run/attempt differs')
    require(receipt['status'] == 'passed' and receipt['page_size'] == 16384
            and receipt['api_level'] == 36 and receipt['system_abi'] == 'x86_64'
            and receipt['tested_abi'] == 'x86_64'
            and receipt['installed_apk_sha256'] == apk_sha, 'Runtime did not qualify selected bytes')
    require({'signature/package/version/static APK checks', 'PAGE_SIZE=16384', 'installed APK hash',
             'x86_64 native loader', 'foreground onboarding on cold start and restart', 'stable process'}
            <= set(receipt['passed']), 'Runtime gates incomplete')
    return {**candidate, 'track': 'internal', 'qualification': 'automated candidate gates only',
            'not_proven': ['ARM64 runtime', 'Play-generated splits', 'billing purchases',
                           'phone/TV acceptance', 'Play pre-launch report']}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='mode', required=True)
    first = sub.add_parser('plan')
    for flag in ('metadata', 'repository', 'build-run-id', 'source-sha', 'qa-run-id', 'ci-run-id', 'output'):
        first.add_argument('--' + flag, required=True)
    second = sub.add_parser('verify')
    for flag in ('root', 'evidence', 'plan', 'apk-sha256', 'aab-sha256', 'certificate', 'output'):
        second.add_argument('--' + flag, required=True)
    args = parser.parse_args()
    try:
        if args.mode == 'plan':
            directory = Path(args.metadata)
            names = ('build_run', 'build_artifacts', 'source_comparison', 'qa_run', 'qa_comparison',
                     'qa_artifacts', 'ci_run', 'ci_jobs')
            result = plan({n: read_json(directory / (n + '.json')) for n in names}, args.repository,
                          args.build_run_id, args.source_sha, args.qa_run_id, args.ci_run_id)
            outputs = {'build_artifact_id': result['artifact_id'], 'qa_artifact_id': result['qa_artifact_id']}
        else:
            result = verify(args.root, args.evidence, read_json(args.plan), args.apk_sha256,
                            args.aab_sha256, args.certificate)
            outputs = {'version_code': result['version_code']}
        Path(args.output).write_text(json.dumps(result, indent=2) + '\n')
        with open(os.environ['GITHUB_OUTPUT'], 'a') as output:
            for key, value in outputs.items():
                require(type(value) is int and value > 0, 'Unexpected nonnumeric workflow output')
                output.write(f'{key}={value}\n')
    except (ValueError, KeyError, TypeError, OSError) as error:
        parser.exit(1, f'Candidate promotion rejected: {error}\n')


if __name__ == '__main__':
    main()
