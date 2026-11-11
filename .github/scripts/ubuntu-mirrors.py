"""Route hosted-runner Ubuntu package downloads away from the Azure mirror."""

from pathlib import Path
import re
import platform
import subprocess


ARCHIVE = 'https://archive.ubuntu.com/ubuntu'
SECURITY = 'https://security.ubuntu.com/ubuntu'
FALLBACK = 'https://mirrors.edge.kernel.org/ubuntu'


def reachable(url: str) -> bool:
    try:
        result = subprocess.run(
            ['curl', '--fail', '--silent', '--show-error', '--location',
             '--max-time', '10', '--range', '0-33', url],
            capture_output=True, timeout=15,
        )
        if result.returncode == 0 and result.stdout.startswith(b'-----BEGIN PGP SIGNED MESSAGE-----'):
            return True
        print(f'Ubuntu mirror probe failed for {url} (curl exit {result.returncode})', flush=True)
    except (OSError, subprocess.TimeoutExpired) as error:
        print(f'Ubuntu mirror probe failed for {url}: {error}', flush=True)
    return False


def select_mirrors(codename: str, probe=reachable) -> tuple[str, str]:
    if codename not in ('jammy', 'noble'):
        raise RuntimeError(f'Unsupported CI Ubuntu codename: {codename}')
    for archive, security in [(ARCHIVE, SECURITY), (FALLBACK, FALLBACK)]:
        urls = [f'{archive}/dists/{codename}{suffix}/InRelease'
                for suffix in ('', '-updates', '-backports')]
        urls.append(f'{security}/dists/{codename}-security/InRelease')
        if all(probe(url) for url in urls):
            print(f'Using Ubuntu mirrors: {archive}, {security}', flush=True)
            return archive, security
    raise RuntimeError('No reachable Ubuntu mirror for all required suites; dependencies cannot be validated')


def configure(apt_root: Path, archive: str = ARCHIVE, security: str = SECURITY) -> int:
    replacements = {
        'mirror+file:/etc/apt/apt-mirrors.txt': archive,
        'mirror+file:/etc/apt/apt-mirrors-security.txt': security,
        'http://azure.archive.ubuntu.com/ubuntu': archive,
        'https://azure.archive.ubuntu.com/ubuntu': archive,
        'http://archive.ubuntu.com/ubuntu': archive,
        ARCHIVE: archive,
        'http://security.ubuntu.com/ubuntu': security,
        SECURITY: security,
    }
    pattern = re.compile('(' + '|'.join(re.escape(uri) for uri in replacements) + r')/?(?=\s|$)')
    paths = [apt_root / 'sources.list']
    source_dir = apt_root / 'sources.list.d'
    paths.extend(sorted(source_dir.glob('*.list')))
    paths.extend(sorted(source_dir.glob('*.sources')))
    changed = 0
    for path in paths:
        if not path.is_file():
            continue
        original = path.read_text()
        updated = pattern.sub(lambda match: replacements[match.group(1)], original)
        if updated != original:
            path.write_text(updated)
            changed += 1
    return changed


if __name__ == '__main__':
    archive, security = select_mirrors(platform.freedesktop_os_release()['VERSION_CODENAME'])
    print(f'Updated {configure(Path("/etc/apt"), archive, security)} Ubuntu source files')
