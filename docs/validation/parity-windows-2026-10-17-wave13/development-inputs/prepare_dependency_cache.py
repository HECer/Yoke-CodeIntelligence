"""Reuse public dependency products only; never an own-package acceptance result."""
import hashlib
import json
from pathlib import Path
import shutil

source = Path('E:/Yoke-Build-Cache/mixed-document-view-20261017/debug')
base = Path(__file__).resolve().parent
target = base/'debug'
assert source.is_dir() and not source.is_symlink() and not target.exists()
assert target.resolve().parent == base.resolve()
target.mkdir()
count = size = 0
for path in (source/'deps').iterdir():
    assert not path.is_symlink()
    if not path.is_file() or path.suffix not in {'.rlib','.rmeta','.dll','.lib','.exp','.d'}:
        continue
    if path.name.startswith(('libyoke-', 'yoke-')):
        continue
    destination = target/'deps'/path.name
    destination.parent.mkdir(exist_ok=True)
    shutil.copy2(path,destination)
    count += 1
    size += destination.stat().st_size
for path in (source/'.fingerprint').iterdir():
    assert not path.is_symlink()
    if path.name.startswith(('yoke-code-intelligence-', 'yoke-')):
        continue
    shutil.copytree(path,target/'.fingerprint'/path.name)
if (source/'build').exists():
    shutil.copytree(source/'build',target/'build')
for path in [base/'temp',base/'development']:
    path.mkdir(exist_ok=True)
receipt = dict(source=str(source),target=str(target),dependencyLeaves=count,
    dependencyBytes=size,ownPackageFingerprintsReused=False,ownBinariesReused=False,
    scriptSha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),buildExecuted=False)
(base/'dependency-cache.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8')
print(json.dumps(receipt))
