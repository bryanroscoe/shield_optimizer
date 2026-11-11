"""Route hosted-runner Ubuntu package downloads away from the Azure mirror."""

from pathlib import Path
import re


def configure(apt_root: Path) -> int:
    replacements = {
        'mirror+file:/etc/apt/apt-mirrors.txt': 'https://archive.ubuntu.com/ubuntu',
        'mirror+file:/etc/apt/apt-mirrors-security.txt': 'https://security.ubuntu.com/ubuntu',
        'http://azure.archive.ubuntu.com/ubuntu': 'https://archive.ubuntu.com/ubuntu',
        'https://azure.archive.ubuntu.com/ubuntu': 'https://archive.ubuntu.com/ubuntu',
    }
    paths = [apt_root / 'sources.list']
    source_dir = apt_root / 'sources.list.d'
    paths.extend(sorted(source_dir.glob('*.list')))
    paths.extend(sorted(source_dir.glob('*.sources')))
    changed = 0
    for path in paths:
        if not path.is_file():
            continue
        original = path.read_text()
        updated = original
        for source, destination in replacements.items():
            updated = re.sub(re.escape(source) + r'/?(?=\s|$)', destination, updated)
        if updated != original:
            path.write_text(updated)
            changed += 1
    return changed


if __name__ == '__main__':
    print(f'Updated {configure(Path("/etc/apt"))} Ubuntu source files')
