#!/usr/bin/env python3
"""Render notices from Gradle's resolved release modules and their cached upstream archives."""
import argparse
import hashlib
import io
import json
from pathlib import Path
import re
import xml.etree.ElementTree as ET
import zipfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('inventory', type=Path)
    parser.add_argument('--parent-pom', type=Path, action='append', default=[])
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    rows = json.loads(args.inventory.read_text())
    texts = {}
    parents = {}

    def read_pom(path):
        xml = ET.parse(path).getroot()
        for node in xml.iter():
            node.tag = node.tag.rsplit('}', 1)[-1]
        return xml

    for path in args.parent_pom:
        xml = read_pom(path)
        parents[(xml.findtext('groupId'), xml.findtext('artifactId'), xml.findtext('version'))] = xml

    def record(data, origin):
        text = '\n'.join(line.rstrip() for line in data.decode('utf-8').splitlines()).strip()
        if not text:
            return
        key = hashlib.sha256(text.encode()).hexdigest()
        entry = texts.setdefault(key, {'text': text, 'origins': set()})
        entry['origins'].add(origin)

    def notices(archive, coordinate, prefix=''):
        for name in sorted(archive.namelist()):
            leaf = name.rsplit('/', 1)[-1]
            if leaf.endswith('.class'):
                continue
            if re.search(r'(^|[-_])(LICENSE|NOTICE|COPYING)([._-].*)?$', leaf, re.I):
                record(archive.read(name), f'{coordinate} / {prefix}{name}')
        if 'third_party_licenses.json' in archive.namelist():
            index = json.loads(archive.read('third_party_licenses.json'))
            data = archive.read('third_party_licenses.txt')
            for name, position in sorted(index.items()):
                start, length = position['start'], position['length']
                if start < 0 or length <= 0 or start + length > len(data):
                    raise ValueError(f'Invalid upstream notice offsets in {coordinate}')
                record(data[start:start + length], f'{coordinate} / {name}')
        if 'classes.jar' in archive.namelist():
            with zipfile.ZipFile(io.BytesIO(archive.read('classes.jar'))) as classes:
                notices(classes, coordinate, prefix + 'classes.jar!/')

    modules = []
    for row in sorted(rows, key=lambda r: r['coordinate']):
        coordinate = row['coordinate']
        if not row['pom']:
            raise ValueError(f'Missing resolved POM for {coordinate}')
        pom_path = Path(row['pom'])
        xml = read_pom(pom_path)
        license_xml = xml
        if not xml.findall('licenses/license'):
            parent = xml.find('parent')
            key = tuple(parent.findtext(tag) for tag in ['groupId', 'artifactId', 'version']) if parent is not None else None
            license_xml = parents.get(key)
        licenses = [{'name': node.findtext('name'), 'url': node.findtext('url')}
                    for node in license_xml.findall('licenses/license')] if license_xml is not None else []
        if not licenses or any(not entry['name'] or not entry['url'] for entry in licenses):
            raise ValueError(f'Unresolved license metadata for {coordinate}; supply its parent POM if inherited')
        if any(re.search(r'\b(?:AGPL|LGPL|GPL)\b', entry['name'], re.I) for entry in licenses):
            raise ValueError(f'GPL-family module needs review before distribution: {coordinate}')
        files = [Path(path) for path in row['artifacts']]
        artifacts = []
        for path in files:
            artifacts.append({'file': path.name, 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()})
            with zipfile.ZipFile(path) as archive:
                notices(archive, coordinate)
        modules.append({'coordinate': coordinate, 'licenses': licenses, 'artifacts': artifacts, 'metadata_only': not files,
                        'pom_sha256': hashlib.sha256(pom_path.read_bytes()).hexdigest()})
    inventory = {'schema': 1, 'configuration': 'universalReleaseRuntimeClasspath', 'modules': modules}
    (root / 'android-runtime-inventory.json').write_text(json.dumps(inventory, indent=2) + '\n')
    body = ['## Resolved Android runtime dependencies', '',
            'Resolved from the Android release runtime classpath. Metadata-only modules are included; '
            'their platform-specific artifacts appear separately. The archive hashes and license metadata '
            'are recorded in `android-runtime-inventory.json`.', '',
            '| Module | License |', '| --- | --- |']
    for module in modules:
        body.append(f"| {module['coordinate']} | " + '; '.join(f"[{entry['name']}]({entry['url']})" for entry in module['licenses']) + ' |')
    body += ['', '### Android archive redistribution notices', '',
             'The following texts are retained from the upstream JAR/AAR license files and the Google '
             'SDKs’ bundled third-party notice indexes. An upstream SDK notice may describe embedded '
             'components that are not separate Gradle modules. Text identical after normalizing line '
             'endings and trailing spaces is shown once with '
             'all of its origins; distinct copyright notices are preserved.', '']
    for key, entry in sorted(texts.items()):
        body += ['<details>', '<summary>Notice ' + key[:12] + '</summary>', '',
                 'From: ' + '; '.join(sorted(entry['origins'])), '', '````', entry['text'], '````', '', '</details>', '']
    (root / 'android-runtime-notices.md').write_text('\n'.join(body).rstrip() + '\n')
    print(f'Resolved {len(modules)} modules; retained {len(texts)} distinct upstream notice texts.')


if __name__ == '__main__':
    main()
