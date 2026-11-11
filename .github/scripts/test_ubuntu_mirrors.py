import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('ubuntu_mirrors', Path(__file__).with_name('ubuntu-mirrors.py'))
mirrors = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mirrors)


class UbuntuMirrorsTests(unittest.TestCase):
    def test_runner_formats_preserve_suites_components_and_signing(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'sources.list.d').mkdir()
            legacy = root / 'sources.list'
            legacy.write_text('deb [signed-by=/usr/share/keyrings/ubuntu-archive-keyring.gpg] http://azure.archive.ubuntu.com/ubuntu/ jammy main universe\n')
            deb822 = root / 'sources.list.d/ubuntu.sources'
            deb822.write_text('Types: deb\nURIs: mirror+file:/etc/apt/apt-mirrors.txt\nSuites: noble noble-updates noble-backports\nComponents: main restricted universe multiverse\nSigned-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg\n\nTypes: deb\nURIs: mirror+file:/etc/apt/apt-mirrors-security.txt\nSuites: noble-security\nComponents: main\nSigned-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg\n')
            before = deb822.read_text()
            third_party = root / 'sources.list.d/microsoft.list'
            third_party.write_text('deb [arch=amd64] https://packages.microsoft.com/ubuntu/24.04/prod noble main\n')
            third_before = third_party.read_bytes()
            self.assertEqual(mirrors.configure(root), 2)
            self.assertEqual(legacy.read_text(), 'deb [signed-by=/usr/share/keyrings/ubuntu-archive-keyring.gpg] https://archive.ubuntu.com/ubuntu jammy main universe\n')
            self.assertEqual(deb822.read_text(), before.replace('mirror+file:/etc/apt/apt-mirrors.txt', 'https://archive.ubuntu.com/ubuntu').replace('mirror+file:/etc/apt/apt-mirrors-security.txt', 'https://security.ubuntu.com/ubuntu'))
            self.assertEqual(third_party.read_bytes(), third_before)
            self.assertEqual(mirrors.configure(root), 0)

    def test_absent_sources_and_unrelated_uri_are_not_changed(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.assertEqual(mirrors.configure(root), 0)
            source = root / 'sources.list'
            text = 'deb https://archive.ubuntu.com/ubuntu jammy main\ndeb https://azure.archive.ubuntu.com/ubuntu-other custom main\n'
            source.write_text(text)
            self.assertEqual(mirrors.configure(root), 0)
            self.assertEqual(source.read_text(), text)


if __name__ == '__main__':
    unittest.main()
