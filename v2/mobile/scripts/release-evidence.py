#!/usr/bin/env python3
"""Fail closed when release summaries or test claims lack successful evidence."""
import argparse
import json
from pathlib import Path
import re
import xml.etree.ElementTree as ET


def junit_summary(directory):
    reports = sorted(Path(directory).glob('TEST-*.xml'))
    if not reports:
        raise ValueError('No Kotlin JUnit reports were produced')
    tests = skipped = 0
    for report in reports:
        suite = ET.parse(report).getroot()
        if suite.tag != 'testsuite':
            raise ValueError(f'Unexpected JUnit report: {report.name}')
        count = int(suite.attrib['tests'])
        skip = int(suite.attrib.get('skipped', '0'))
        failures = int(suite.attrib['failures'])
        errors = int(suite.attrib['errors'])
        if min(count, skip, failures, errors) < 0 or skip > count:
            raise ValueError(f'Invalid JUnit counts in {report.name}')
        if failures or errors or suite.findall('.//failure') or suite.findall('.//error'):
            raise ValueError(f'Kotlin tests failed in {report.name}')
        tests += count
        skipped += skip
    if tests <= skipped:
        raise ValueError('No Kotlin tests actually ran')
    return f'Kotlin JVM tests: {tests - skipped} passed, {skipped} skipped in {len(reports)} reports.'


def publish_summary(outcome, receipt, track, status, version_code):
    if outcome != 'success':
        if outcome in ('', 'skipped'):
            return '- Play upload did not run.'
        return f'- Play upload step was {outcome}; publication is not confirmed. Check Play before retrying.'
    result = json.loads(Path(receipt).read_text())
    expected = {'package': 'com.atvoptimizer.mobile', 'track': track, 'status': status,
                'version_code': int(version_code)}
    if any(result.get(key) != value for key, value in expected.items()):
        raise ValueError('Play upload receipt does not match the requested release')
    if not re.fullmatch(r'[0-9a-f]{64}', result.get('aab_sha256', '')):
        raise ValueError('Play upload receipt is missing the verified AAB SHA-256')
    summary = f'- Uploaded versionCode {version_code} to track `{track}` as a **{status}** release.'
    if status == 'draft':
        summary += '\n  The draft is not a rollout. Review it in Play Console before publishing.'
    return summary


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    junit = commands.add_parser('junit')
    junit.add_argument('directory', type=Path)
    publish = commands.add_parser('publish')
    publish.add_argument('--outcome', required=True)
    publish.add_argument('--receipt', required=True, type=Path)
    publish.add_argument('--track', required=True)
    publish.add_argument('--status', required=True)
    publish.add_argument('--version-code', required=True)
    args = parser.parse_args()
    try:
        if args.command == 'junit':
            print(junit_summary(args.directory))
        else:
            print(publish_summary(args.outcome, args.receipt, args.track, args.status, args.version_code))
    except (OSError, ValueError, KeyError, TypeError, ET.ParseError) as error:
        parser.exit(1, f'Release evidence rejected: {error}\n')


if __name__ == '__main__':
    main()
