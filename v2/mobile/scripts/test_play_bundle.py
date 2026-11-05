import argparse
import importlib.util
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest import mock
import zipfile

spec = importlib.util.spec_from_file_location('bundle_gate', Path(__file__).with_name('verify-play-bundle.py'))
gate = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gate)

CERT = 'AB:' * 31 + 'AB'
HEX = 'ab' * 32


class BundleGate(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.dir = Path(directory.name)
        self.aab = self.dir / 'app.aab'
        self.aab.write_bytes(b'aab')
        self.values = {'jar': 'jar verified.', 'cert': f'Certificate fingerprints:\n\t SHA256: {CERT}\n',
                       'package': 'com.atvoptimizer.mobile', 'code': '1002',
                       'apk_cert': f'Signer #1 certificate SHA-256 digest: {HEX}\n'}
        self.calls = []
        self.apk_results = []
        patcher = mock.patch.object(gate.apk_gate, 'verify', side_effect=self.fake_apk_verify)
        patcher.start()
        self.addCleanup(patcher.stop)
        env = mock.patch.dict(os.environ, {'KS': 'store-secret', 'KP': 'key-secret'})
        env.start()
        self.addCleanup(env.stop)

    def fake_apk_verify(self, apk, *tools, run):
        return {'apk': Path(apk).name, 'version_code': int(self.apk_results.pop(0) if self.apk_results else 1002)}

    def fake_run(self, args, **kwargs):
        self.calls.append(args)
        joined = ' '.join(map(str, args))
        if args[0] == 'jarsigner':
            out = self.values['jar']
        elif args[0] == 'keytool':
            out = self.values['cert']
        elif '/manifest/@package' in joined:
            out = self.values['package'] + '\n'
        elif 'versionCode' in joined:
            out = self.values['code'] + '\n'
        elif 'build-apks' in joined:
            ks_pass = next(a for a in args if a.startswith('--ks-pass=file:')).split(':', 1)[1]
            self.assertEqual(Path(ks_pass).read_text(), 'store-secret')
            self.assertEqual(Path(ks_pass).stat().st_mode & 0o777, 0o600)
            output = next(a for a in args if a.startswith('--output=')).split('=', 1)[1]
            with zipfile.ZipFile(output, 'w') as archive:
                archive.writestr('universal.apk', b'apk')
            out = ''
        elif args[0] == 'apksigner':
            out = self.values['apk_cert']
        else:
            out = ''
        return subprocess.CompletedProcess(args, 0, stdout=out, stderr='')

    def args(self, **overrides):
        values = dict(aab=str(self.aab), apk=[], version_code=1002, expected_cert_sha256=CERT,
                      bundletool=Path('bt.jar'), keystore=Path('upload.jks'), key_alias='upload',
                      ks_pass_env='KS', key_pass_env='KP', output_dir=self.dir, java='java',
                      jarsigner='jarsigner', keytool='keytool', apksigner='apksigner', zipalign='zipalign',
                      apkanalyzer='apkanalyzer')
        values.update(overrides)
        return argparse.Namespace(**values)

    def test_accepts_expected_bundle(self):
        result = gate.verify(self.args(), run=self.fake_run)
        self.assertEqual(result['upload_certificate_sha256'], HEX)
        self.assertEqual([a['apk'] for a in result['apks']], ['bundletool-universal.apk'])
        self.assertTrue(any('validate' in a for a in self.calls))
        self.assertFalse(any('secret' in str(a) for call in self.calls for a in call))

    def test_rejections(self):
        cases = [('jar', 'jar is unsigned.', 'jarsigner'),
                 ('cert', 'SHA256: ' + 'CD:' * 31 + 'CD', 'upload certificate'),
                 ('package', 'com.example', 'package'),
                 ('code', '1001', 'versionCode'),
                 ('apk_cert', 'Signer #1 certificate SHA-256 digest: ' + 'cd' * 32, 'upload certificate')]
        for key, value, pattern in cases:
            with self.subTest(key=key):
                saved = self.values[key]
                self.values[key] = value
                with self.assertRaisesRegex(ValueError, pattern):
                    gate.verify(self.args(), run=self.fake_run)
                self.values[key] = saved

    def test_gradle_apk_version_mismatch(self):
        self.apk_results = [1002, 1001]
        with self.assertRaisesRegex(ValueError, 'gradle.apk has versionCode 1001'):
            gate.verify(self.args(apk=['gradle.apk']), run=self.fake_run)

    def test_missing_password_and_bad_fingerprint(self):
        with mock.patch.dict(os.environ, {'KS': ''}):
            with self.assertRaisesRegex(ValueError, 'password'):
                gate.verify(self.args(), run=self.fake_run)
        with self.assertRaisesRegex(ValueError, '64 hex'):
            gate.verify(self.args(expected_cert_sha256='abc'), run=self.fake_run)


if __name__ == '__main__':
    unittest.main()
