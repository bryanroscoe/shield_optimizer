#!/usr/bin/env python3
"""Diagnose KVM and optionally scope an approved ACL grant to one hosted job."""
import argparse
import json
import os
from pathlib import Path
import platform
import stat
import subprocess

DEVICE = Path('/dev/kvm')


def require(ok, message):
    if not ok:
        raise ValueError(message)


def command(args):
    return subprocess.run(args, check=True, capture_output=True, text=True, timeout=20).stdout.strip()


def identity():
    info = DEVICE.lstat()
    require(stat.S_ISCHR(info.st_mode), 'KVM is not a character device (symlinks rejected)')
    require(info.st_uid == 0, 'Unexpected KVM device owner')
    return {key: getattr(info, key) for key in ('st_dev', 'st_ino', 'st_rdev', 'st_uid', 'st_gid')}


def acl():
    return command(['getfacl', '--omit-header', '--numeric', '--absolute-names', str(DEVICE)])


def basic_acl(value):
    rows = value.splitlines()
    require(len(rows) == 3 and rows[0] == 'user::rw-' and rows[1] in ('group::rw-', 'group::---')
            and rows[2] == 'other::---', 'Unexpected KVM ACL; no permission change allowed')
    return ','.join(rows)


def hosted_context():
    require(platform.system() == 'Linux' and os.getuid() != 0, 'Require non-root Linux runner')
    for key, expected in [('RUNNER_ENVIRONMENT', 'github-hosted'),
                          ('GITHUB_EVENT_NAME', 'workflow_dispatch'),
                          ('GITHUB_REF', 'refs/heads/main'), ('REPOSITORY_VISIBILITY', 'public')]:
        require(os.environ.get(key) == expected, f'Unexpected {key}; no ACL change allowed')


def prepare(state, evidence, opt_in):
    require(opt_in in ('true', 'false'), 'Invalid KVM opt-in')
    evidence.mkdir(parents=True, exist_ok=True)
    report = {'opt_in': opt_in == 'true', 'status': 'failed', 'uid': os.getuid(),
              'groups': os.getgroups(), 'device': str(DEVICE)}
    try:
        report['identity'] = identity()
        report['acl'] = acl()
        original = basic_acl(report['acl'])
        report['readable'] = os.access(DEVICE, os.R_OK)
        report['writable'] = os.access(DEVICE, os.W_OK)
        if report['readable'] and report['writable']:
            report['status'] = 'already-accessible'
            return
        require(opt_in == 'true', 'KVM access denied; per-run ACL opt-in is false')
        hosted_context()
        uid = os.getuid()
        # Arm cleanup before sudo: a command failure does not prove no change happened.
        saved = {'identity': report['identity'], 'original_acl': original, 'uid': uid}
        with state.open('x') as output:
            json.dump(saved, output)
        require(identity() == saved['identity'], 'KVM identity changed before grant')
        command(['sudo', '-n', 'setfacl', '--no-mask', '-m', f'user:{uid}:rw-,mask::rw-', str(DEVICE)])
        require(identity() == saved['identity'], 'KVM identity changed after grant')
        require(os.access(DEVICE, os.R_OK) and os.access(DEVICE, os.W_OK), 'KVM remains inaccessible')
        report['granted_acl'] = acl()
        expected = set(original.split(',')) | {f'user:{uid}:rw-', 'mask::rw-'}
        require(set(report['granted_acl'].splitlines()) == expected, 'Unexpected ACL after grant')
        report['status'] = 'granted-for-this-job'
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        report['error'] = str(error)
        raise
    finally:
        (evidence / 'kvm-prerequisite.json').write_text(json.dumps(report, indent=2) + '\n')


def restore(state, evidence):
    if not state.exists():
        return
    hosted_context()
    saved = json.loads(state.read_text())
    require(saved['uid'] == os.getuid(), 'KVM restore UID differs')
    require(identity() == saved['identity'], 'KVM device changed; refusing restore')
    original = basic_acl(saved['original_acl'].replace(',', '\n'))
    current = set(acl().splitlines())
    allowed = set(original.split(',')) | {f'user:{saved["uid"]}:rw-', 'mask::rw-'}
    require(current == set(original.split(',')) or current == allowed, 'Unexpected ACL before restore')
    # Fixed device and validated basic entries only, never a privileged --restore file.
    command(['sudo', '-n', 'setfacl', '--set=' + original, str(DEVICE)])
    require(identity() == saved['identity'] and acl() == original.replace(',', '\n'),
            'KVM ACL restoration was not confirmed')
    (evidence / 'kvm-restored.json').write_text(json.dumps({'status': 'restored'}) + '\n')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', choices=['prepare', 'restore'])
    parser.add_argument('--state', type=Path, required=True)
    parser.add_argument('--evidence', type=Path, required=True)
    parser.add_argument('--allow-acl', default='false')
    args = parser.parse_args()
    try:
        if args.mode == 'prepare':
            prepare(args.state, args.evidence, args.allow_acl)
        else:
            restore(args.state, args.evidence)
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        parser.exit(1, f'KVM prerequisite rejected: {error}\n')


if __name__ == '__main__':
    main()
