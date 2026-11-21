import importlib.util
import json
import os
from pathlib import Path
import stat
import subprocess
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('kvm', Path(__file__).with_name('runtime-kvm.py'))
kvm = importlib.util.module_from_spec(spec)
spec.loader.exec_module(kvm)
BASIC = 'user::rw-\ngroup::rw-\nother::---'
GRANTED = 'user::rw-\nuser:1001:rw-\ngroup::rw-\nmask::rw-\nother::---'
IDENTITY = dict(st_dev=1, st_ino=2, st_rdev=3, st_uid=0, st_gid=108)


class KvmTests(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name)
        self.state = self.root / 'state.json'
        self.evidence = self.root / 'evidence'
        self.identity = self.mock('identity', return_value=IDENTITY)
        self.acl = self.mock('acl', return_value=BASIC)
        self.context = self.mock('hosted_context')
        self.command = self.mock('command')
        self.uid = self.mock('os.getuid', return_value=1001)
        self.access = self.mock('os.access', return_value=False)

    def mock(self, name, **kwargs):
        p = patch.object(kvm, name, **kwargs) if '.' not in name else patch('os.' + name.split('.')[1], **kwargs)
        result = p.start()
        self.addCleanup(p.stop)
        return result

    def prepare(self, value='false'):
        kvm.prepare(self.state, self.evidence, value)

    def test_default_denial_never_invokes_privileged_command(self):
        with self.assertRaisesRegex(ValueError, 'opt-in is false'):
            self.prepare()
        self.command.assert_not_called()
        self.assertFalse(self.state.exists())
        self.assertIn('access denied', (self.evidence / 'kvm-prerequisite.json').read_text())

    def test_existing_access_never_changes_permissions_even_opted_in(self):
        self.access.return_value = True
        for opt in ('false', 'true'):
            self.prepare(opt)
        self.command.assert_not_called()
        self.assertFalse(self.state.exists())

    def test_invalid_opt_in_fails(self):
        for opt in ('', 'yes', 'TRUE', '1', 'false; echo x'):
            with self.assertRaises(ValueError): self.prepare(opt)
        self.command.assert_not_called()

    def test_unknown_acl_denied_even_when_already_accessible(self):
        self.access.return_value = True
        for value in (GRANTED, BASIC + '\ndefault:user::rw-', BASIC.replace('other::---', 'other::rw-'),
                      BASIC.replace('user::rw-', 'user::rwx'), BASIC.replace('group::rw-', 'group::r--')):
            self.acl.return_value = value
            with self.assertRaises(ValueError): self.prepare('true')
        self.command.assert_not_called()

    def test_missing_device_is_recorded_and_never_modified(self):
        self.identity.side_effect = FileNotFoundError('/dev/kvm')
        with self.assertRaises(FileNotFoundError): self.prepare('true')
        self.command.assert_not_called()
        self.assertIn('/dev/kvm', (self.evidence / 'kvm-prerequisite.json').read_text())

    def grant(self):
        self.access.side_effect = [False, False, True, True]
        self.acl.side_effect = [BASIC, GRANTED]
        self.prepare('true')
        self.acl.side_effect = None
        self.access.side_effect = None

    def test_grant_is_single_uid_and_saved_before_sudo(self):
        self.command.side_effect = lambda args: self.assertTrue(self.state.exists())
        self.grant()
        self.command.assert_called_once_with(['sudo', '-n', 'setfacl', '--no-mask', '-m',
                                              'user:1001:rw-,mask::rw-', '/dev/kvm'])
        self.assertEqual(json.loads(self.state.read_text())['identity'], IDENTITY)

    def test_failed_sudo_still_arms_cleanup(self):
        self.command.side_effect = subprocess.CalledProcessError(1, 'sudo')
        with self.assertRaises(subprocess.CalledProcessError): self.prepare('true')
        self.assertTrue(self.state.exists())

    def test_existing_state_and_identity_race_prevent_grant(self):
        self.state.write_text('do not overwrite')
        with self.assertRaises(FileExistsError): self.prepare('true')
        self.command.assert_not_called()
        self.state.unlink()
        self.identity.side_effect = [IDENTITY, {**IDENTITY, 'st_ino': 9}]
        with self.assertRaisesRegex(ValueError, 'changed'): self.prepare('true')
        self.command.assert_not_called()

    def test_post_grant_identity_mismatch_keeps_cleanup_state(self):
        self.identity.side_effect = [IDENTITY, IDENTITY, {**IDENTITY, 'st_ino': 9}]
        with self.assertRaisesRegex(ValueError, 'identity changed after grant'):
            self.prepare('true')
        self.assertTrue(self.state.exists())
        self.command.assert_called_once()

    def test_post_grant_acl_mismatch_is_not_success(self):
        self.access.side_effect = [False, False, True, True]
        self.acl.side_effect = [BASIC, GRANTED + '\nuser:99:rw-']
        with self.assertRaisesRegex(ValueError, 'Unexpected ACL after grant'):
            self.prepare('true')
        self.assertTrue(self.state.exists())
        report = json.loads((self.evidence / 'kvm-prerequisite.json').read_text())
        self.assertEqual(report['status'], 'failed')

    def test_failed_restore_command_has_no_success_receipt(self):
        self.grant()
        self.acl.return_value = GRANTED
        self.command.side_effect = subprocess.CalledProcessError(1, 'sudo')
        with self.assertRaises(subprocess.CalledProcessError):
            kvm.restore(self.state, self.evidence)
        self.assertFalse((self.evidence / 'kvm-restored.json').exists())
        self.assertTrue(self.state.exists())

    def test_bad_host_prevents_grant(self):
        self.context.side_effect = ValueError('not hosted')
        with self.assertRaises(ValueError): self.prepare('true')
        self.command.assert_not_called()

    def test_restore_exact_basic_acl(self):
        self.grant()
        self.command.reset_mock()
        self.acl.side_effect = [GRANTED, BASIC]
        kvm.restore(self.state, self.evidence)
        self.command.assert_called_once_with(['sudo', '-n', 'setfacl', '--set=' + BASIC.replace('\n', ','), '/dev/kvm'])
        self.assertTrue((self.evidence / 'kvm-restored.json').exists())

    def test_failed_grant_original_acl_is_restorable(self):
        self.command.side_effect = subprocess.CalledProcessError(1, 'sudo')
        with self.assertRaises(subprocess.CalledProcessError): self.prepare('true')
        self.command.side_effect = None
        kvm.restore(self.state, self.evidence)
        self.assertTrue((self.evidence / 'kvm-restored.json').exists())

    def test_restore_no_state_is_noop(self):
        kvm.restore(self.state, self.evidence)
        self.command.assert_not_called()
        self.context.assert_not_called()

    def test_restore_rejects_changed_device_or_acl(self):
        self.grant()
        self.command.reset_mock()
        self.identity.return_value = {**IDENTITY, 'st_rdev': 99}
        with self.assertRaisesRegex(ValueError, 'device changed'): kvm.restore(self.state, self.evidence)
        self.command.assert_not_called()
        self.identity.return_value = IDENTITY
        self.acl.return_value = GRANTED + '\nuser:99:rw-'
        with self.assertRaisesRegex(ValueError, 'Unexpected ACL'): kvm.restore(self.state, self.evidence)
        self.command.assert_not_called()

    def test_restore_verification_failure_is_not_success(self):
        self.grant()
        self.acl.return_value = GRANTED
        with self.assertRaisesRegex(ValueError, 'not confirmed'): kvm.restore(self.state, self.evidence)
        self.assertFalse((self.evidence / 'kvm-restored.json').exists())


class PrimitiveTests(unittest.TestCase):
    def test_identity_rejects_symlink_regular_file_and_nonroot_owner(self):
        for mode, uid in [(stat.S_IFLNK, 0), (stat.S_IFREG, 0), (stat.S_IFCHR, 1001)]:
            with patch.object(Path, 'lstat', return_value=SimpleNamespace(st_mode=mode, st_uid=uid)):
                with self.assertRaises(ValueError): kvm.identity()

    def test_context_requires_each_scope_condition(self):
        env = {'RUNNER_ENVIRONMENT': 'github-hosted', 'GITHUB_EVENT_NAME': 'workflow_dispatch',
               'GITHUB_REF': 'refs/heads/main', 'REPOSITORY_VISIBILITY': 'public'}
        with patch.object(kvm.platform, 'system', return_value='Linux'), patch.object(kvm.os, 'getuid', return_value=1001):
            with patch.dict(os.environ, env, clear=True): kvm.hosted_context()
            for key in env:
                with patch.dict(os.environ, {**env, key: 'wrong'}, clear=True):
                    with self.assertRaises(ValueError): kvm.hosted_context()

    def test_workflow_default_cleanup_and_runtime_gates(self):
        workflow = (Path(__file__).resolve().parents[3] / '.github/workflows/mobile-16kb-runtime.yml').read_text()
        self.assertIn('type: boolean\n        default: false', workflow)
        self.assertIn('ALLOW_KVM_ACL: ${{ inputs.allow_kvm_acl }}', workflow)
        self.assertIn('\"$KVM_CLEANUP\" == success', workflow)
        self.assertIn('--allow-acl "$ALLOW_KVM_ACL"', workflow)
        self.assertIn('Restore original KVM ACL if this run changed it\n        id: kvm_cleanup\n        if: always()', workflow)
        self.assertLess(workflow.index('runtime-kvm.py restore'), workflow.index('Keep bounded runtime evidence'))
        for value in ('-accel on', '"$ANDROID_HOME/emulator/emulator" -accel-check', 'timeout-minutes: 35',
                      'retention-days: 7', 'runtime-16kb.py select', 'runtime-16kb.py candidate', 'runtime-16kb.py runtime'):
            self.assertIn(value, workflow)
        for value in ('sudo ', 'chmod', 'udev', '--licenses', 'secrets.', 'continue-on-error:'):
            self.assertNotIn(value, workflow)


if __name__ == '__main__':
    unittest.main()
