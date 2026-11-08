"""Copy external dependency products only; own artifacts are always rebuilt."""
import hashlib
import json
from pathlib import Path
import shutil

SOURCE = Path('E:/Yoke-Build-Cache/outcome-learning-20261017/debug')
ROOT = Path('E:/Yoke-Worktrees/pdf-mixed-view-20261017')
BASE = Path(__file__).resolve().parent
TARGET = BASE/'debug'
assert SOURCE.is_dir() and not SOURCE.is_symlink() and not TARGET.exists()
assert TARGET.resolve().parent == BASE.resolve()
assert not (BASE/'dependency-cache.json').exists()
TARGET.mkdir()
count = size = 0
own_stems = {'yoke', 'yoke_code_intelligence'}
for folder in [ROOT/'native/tests', ROOT/'native/examples']:
    if folder.exists():
        own_stems.update(path.stem.replace('-', '_') for path in folder.glob('*.rs'))
own_prefixes = tuple(prefix+stem+'-' for stem in sorted(own_stems) for prefix in ['', 'lib'])
for path in (SOURCE/'deps').iterdir():
    assert not path.is_symlink()
    if not path.is_file() or path.suffix not in {'.rlib', '.rmeta', '.dll', '.lib', '.exp', '.d'}:
        continue
    if path.name.startswith(own_prefixes):
        continue
    destination = TARGET/'deps'/path.name
    destination.parent.mkdir(exist_ok=True)
    shutil.copy2(path, destination)
    count += 1
    size += destination.stat().st_size
for path in (SOURCE/'.fingerprint').iterdir():
    assert not path.is_symlink()
    if path.name.startswith(('yoke-code-intelligence-', 'yoke-')):
        continue
    shutil.copytree(path, TARGET/'.fingerprint'/path.name)
if (SOURCE/'build').exists():
    for path in (SOURCE/'build').iterdir():
        assert not path.is_symlink()
        if path.name.startswith(('yoke-code-intelligence-', 'yoke-')):
            continue
        shutil.copytree(path, TARGET/'build'/path.name)
for path in [BASE/'temp', BASE/'development']:
    path.mkdir(exist_ok=True)
receipt = dict(source=str(SOURCE), target=str(TARGET), dependencyLeaves=count,
    dependencyBytes=size, ownPackageFingerprintsReused=False, ownBinariesReused=False,
    scriptSha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(), buildExecuted=False)
with (BASE/'dependency-cache.json').open('x', encoding='utf-8') as stream:
    stream.write(json.dumps(receipt, indent=2)+'\n')
print(json.dumps(receipt))
