import importlib.util
from pathlib import Path
import struct
import subprocess
import tempfile
import unittest
import zipfile

spec = importlib.util.spec_from_file_location('gate', Path(__file__).with_name('verify-release-apk.py'))
gate = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gate)


def elf(alignment=16384, elf_class=2, machine=None):
    data = bytearray(128)
    data[:6] = b'\x7fELF' + bytes([elf_class, 1])
    struct.pack_into('<H', data, 18, machine if machine is not None else (183 if elf_class == 2 else 40))
    if elf_class == 2:
        struct.pack_into('<Q', data, 32, 64)
        struct.pack_into('<HH', data, 54, 56, 1)
        struct.pack_into('<Q', data, 112, alignment)
    else:
        struct.pack_into('<I', data, 28, 64)
        struct.pack_into('<HH', data, 42, 32, 1)
        struct.pack_into('<I', data, 92, alignment)
    struct.pack_into('<I', data, 64, 1)
    return bytes(data)


class ReleaseGate(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.apk = Path(self.temp.name) / 'candidate.apk'
        self.write_apk(elf())
        self.calls = []
        self.values = {'debuggable': 'false', 'application-id': 'com.atvoptimizer.mobile',
                       'target-sdk': '36', 'version-code': '1001'}

    def write_apk(self, data):
        with zipfile.ZipFile(self.apk, 'w') as archive:
            if data is not None:
                archive.writestr('lib/arm64-v8a/libapp.so', data)

    def run_tool(self, args, **kwargs):
        self.calls.append(args)
        return subprocess.CompletedProcess(args, 0, self.values.get(args[2], '') if args[0] == 'analyzer' else 'verified')

    def verify(self):
        return gate.verify(self.apk, 'signer', 'align', 'analyzer', run=self.run_tool)

    def test_valid_archive_and_both_elf_classes(self):
        gate.check_elf(elf(elf_class=1))
        result = self.verify()
        self.assertEqual(result['version_code'], 1001)
        self.assertIn('elf-load-alignment-by-abi', result['verified'])
        self.assertIn('GNU_RELRO layout/overlap compatibility', result['manual_gates'])
        self.assertIn('actual 16 KB runtime validation', result['manual_gates'])
        self.assertEqual(len(result['sha256']), 64)
        self.assertEqual(self.calls[0][:2], ['signer', 'verify'])

    def test_universal_apk_accepts_4kb_32bit_libraries(self):
        with zipfile.ZipFile(self.apk, 'a') as archive:
            archive.writestr('lib/armeabi-v7a/arm.so', elf(4096, 1, 40))
            archive.writestr('lib/x86/x86.so', elf(4096, 1, 3))
            archive.writestr('lib/x86_64/x64.so', elf(16384, 2, 62))
        self.assertEqual(len(self.verify()['native_libraries']), 4)

    def test_each_64bit_architecture_still_rejects_4kb(self):
        for abi, machine in [('arm64-v8a', 183), ('x86_64', 62)]:
            with self.subTest(abi=abi):
                self.write_apk(elf())
                with zipfile.ZipFile(self.apk, 'a') as archive:
                    archive.writestr(f'lib/{abi}/bad.so', elf(4096, 2, machine))
                with self.assertRaises(ValueError):
                    self.verify()

    def test_abi_header_mismatch_is_rejected(self):
        self.write_apk(elf(4096, 1, 40))
        with self.assertRaises(ValueError):
            self.verify()

    def test_unsigned_failure_stops_before_other_checks(self):
        def reject(args, **kwargs):
            self.calls.append(args)
            raise subprocess.CalledProcessError(1, args)
        with self.assertRaises(subprocess.CalledProcessError):
            gate.verify(self.apk, 'signer', 'align', 'analyzer', run=reject)
        self.assertEqual(len(self.calls), 1)

    def test_missing_tool_is_not_success(self):
        def missing(*args, **kwargs):
            raise FileNotFoundError('SDK tool missing')
        with self.assertRaises(FileNotFoundError):
            gate.verify(self.apk, 'signer', 'align', 'analyzer', run=missing)

    def test_rejects_manifest_mismatches(self):
        for field, value in [('debuggable', 'true'), ('application-id', 'other.app'),
                             ('target-sdk', '35'), ('version-code', '0')]:
            with self.subTest(field=field):
                old = self.values[field]
                self.values[field] = value
                with self.assertRaises(ValueError):
                    self.verify()
                self.values[field] = old

    def test_all_libraries_must_pass(self):
        with zipfile.ZipFile(self.apk, 'a') as archive:
            archive.writestr('lib/x86_64/other.so', elf(4096))
        with self.assertRaisesRegex(ValueError, 'other.so'):
            self.verify()

    def test_rejects_missing_or_malformed_native_code(self):
        for data in [None, b'not ELF', elf(4096)]:
            with self.subTest(data=data):
                self.write_apk(data)
                with self.assertRaises(ValueError):
                    self.verify()

    def test_rejects_debug_certificate(self):
        def debug(*args, **kwargs):
            return subprocess.CompletedProcess(args, 0, 'Signer #1 certificate DN: CN=Android Debug')
        with self.assertRaisesRegex(ValueError, 'debug signing'):
            gate.verify(self.apk, 'signer', 'align', 'analyzer', run=debug)

    def test_malformed_headers_and_noncongruent_segments(self):
        truncated = bytearray(elf())
        struct.pack_into('<Q', truncated, 32, 120)
        with self.assertRaises(ValueError):
            gate.check_elf(truncated)
        misaligned = bytearray(elf())
        struct.pack_into('<Q', misaligned, 72, 4096)
        with self.assertRaises(ValueError):
            gate.check_elf(misaligned)

    def test_cli_missing_tool_fails_without_success_receipt(self):
        import sys
        result = subprocess.run([sys.executable, str(Path(gate.__file__)), str(self.apk),
                                 '--apksigner', str(Path(self.temp.name) / 'missing-sdk-tool')],
                                capture_output=True, text=True)
        self.assertEqual(result.returncode, 1)
        self.assertEqual(result.stdout, '')
        self.assertIn('Release APK rejected', result.stderr)

    def test_zip_alignment_failure_is_fatal(self):
        def misaligned(args, **kwargs):
            if args[0] == 'align':
                raise subprocess.CalledProcessError(1, args)
            return self.run_tool(args, **kwargs)
        with self.assertRaises(subprocess.CalledProcessError):
            gate.verify(self.apk, 'signer', 'align', 'analyzer', run=misaligned)


if __name__ == '__main__':
    unittest.main()
