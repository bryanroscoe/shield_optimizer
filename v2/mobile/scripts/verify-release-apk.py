#!/usr/bin/env python3
"""Read-only release APK gate. Requires Android SDK tools; never signs or installs."""
import argparse
import hashlib
import json
from pathlib import Path
import struct
import subprocess
import zipfile


def check_elf(data, abi=None):
    if data[:4] != b'\x7fELF' or len(data) < 64 or data[5] != 1:
        raise ValueError('Unsupported or malformed native ELF')
    if data[4] == 2:
        offset = struct.unpack_from('<Q', data, 32)[0]
        size, count = struct.unpack_from('<HH', data, 54)
        minimum = 56
    elif data[4] == 1:
        offset = struct.unpack_from('<I', data, 28)[0]
        size, count = struct.unpack_from('<HH', data, 42)
        minimum = 32
    else:
        raise ValueError('Unsupported ELF class')
    architectures = {'arm64-v8a': (2, 183), 'x86_64': (2, 62),
                     'armeabi-v7a': (1, 40), 'x86': (1, 3)}
    header = (data[4], struct.unpack_from('<H', data, 18)[0])
    if (abi is not None and architectures.get(abi) != header) or header not in architectures.values():
        raise ValueError('ELF architecture does not match a supported Android ABI')
    page_size = 16384 if data[4] == 2 else 4096
    if not count or size < minimum or offset + size * count > len(data):
        raise ValueError('Malformed ELF program headers')
    loads = 0
    for i in range(count):
        start = offset + i * size
        if struct.unpack_from('<I', data, start)[0] != 1:
            continue
        loads += 1
        if data[4] == 2:
            file_offset, address = struct.unpack_from('<QQ', data, start + 8)
            alignment = struct.unpack_from('<Q', data, start + 48)[0]
        else:
            file_offset, address = struct.unpack_from('<II', data, start + 4)
            alignment = struct.unpack_from('<I', data, start + 28)[0]
        if alignment < page_size or alignment & (alignment - 1) or file_offset % page_size != address % page_size:
            raise ValueError(f'Native LOAD segment is not {page_size // 1024} KB aligned')
    if not loads:
        raise ValueError('Native ELF has no LOAD segments')


def verify(apk, apksigner, zipalign, apkanalyzer, run=subprocess.run):
    apk = Path(apk)
    if apk.suffix != '.apk' or not apk.is_file():
        raise ValueError('Provide an existing APK; AAB verification is a separate release gate')

    def command(args):
        result = run(args, check=True, capture_output=True, text=True, timeout=120)
        return result.stdout.strip()

    certificate = command([apksigner, 'verify', '--verbose', '--print-certs', str(apk)])
    if 'CN=Android Debug' in certificate:
        raise ValueError('Android debug signing certificate is not a release identity')
    command([zipalign, '-c', '-P', '16', '-v', '4', str(apk)])
    def manifest(field):
        return command([apkanalyzer, 'manifest', field, str(apk)])
    if manifest('debuggable').lower() != 'false':
        raise ValueError('APK must be non-debuggable')
    if manifest('application-id') != 'com.atvoptimizer.mobile':
        raise ValueError('Unexpected application id')
    target = int(manifest('target-sdk'))
    version = int(manifest('version-code'))
    if target < 36 or version <= 0:
        raise ValueError('Require target SDK >= 36 and a positive version code')
    with zipfile.ZipFile(apk) as archive:
        native = [x for x in archive.namelist() if x.startswith('lib/') and x.endswith('.so')]
        if not native:
            raise ValueError('No packaged native libraries found')
        for name in native:
            try:
                check_elf(archive.read(name), name.split("/")[1])
            except (ValueError, struct.error) as error:
                raise ValueError(f'{name}: {error}') from error
    return {'apk': apk.name, 'sha256': hashlib.sha256(apk.read_bytes()).hexdigest(),
            'target_sdk': target, 'version_code': version, 'native_libraries': native,
            'verified': ['signature', 'non-debuggable', 'application-id', 'zip-16kb', 'elf-load-alignment-by-abi'],
            'manual_gates': ['trusted signing fingerprint', 'versionCode newer than every uploaded track',
                             'AAB/bundletool and Play checks', 'GNU_RELRO layout/overlap compatibility',
                             'actual 16 KB runtime validation', 'physical device matrix', 'store disclosures']}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('apk')
    parser.add_argument('--apksigner', default='apksigner')
    parser.add_argument('--zipalign', default='zipalign')
    parser.add_argument('--apkanalyzer', default='apkanalyzer')
    args = parser.parse_args()
    try:
        print(json.dumps(verify(args.apk, args.apksigner, args.zipalign, args.apkanalyzer), indent=2))
    except (ValueError, OSError, subprocess.SubprocessError, zipfile.BadZipFile, struct.error) as error:
        parser.exit(1, f'Release APK rejected: {error}\n')


if __name__ == '__main__':
    main()
