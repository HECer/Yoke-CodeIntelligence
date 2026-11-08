"""Seal the current isolated PDF candidate without claiming validation."""
import hashlib
import json
import argparse
import re
from pathlib import Path
import subprocess

BASE=Path(__file__).resolve().parent
ROOT=Path('E:/Yoke-Worktrees/pdf-mixed-view-20261017')
sha=lambda raw:hashlib.sha256(raw).hexdigest()
args=argparse.ArgumentParser()
args.add_argument('--label',default='r1')
label=args.parse_args().label
assert re.fullmatch(r'r[0-9]+',label)
baseline_raw=(BASE/'baseline-source-freeze.json').read_bytes()
assert sha(baseline_raw)=='d6510d06d4cc8d3f53a0f375f4801e2529e24b16c7a5a960e47bafcfb4c4f5ec'
baseline=json.loads(baseline_raw)
names=subprocess.check_output(['git','ls-files','-c','-o','--exclude-standard','-z'],cwd=ROOT).decode().split('\0')
current={name:sha((ROOT/name).read_bytes()) for name in sorted(set(names)) if name and (ROOT/name).is_file()}
delta=sorted(name for name in set(current)|set(baseline) if current.get(name)!=baseline.get(name))
expected=['native/src/media.rs','native/src/mixed_view.rs','native/src/mixed_view_query.rs',
    'native/src/mixed_view_cli.rs','native/src/mixed_pdf_input.rs','native/tests/mixed_pdf_view.rs',
    'native/tests/support/mixed_pdf_fixture.rs','native/docs/mixed-pdf-views.md',
    'native/docs/mixed-views.md']
assert delta==sorted(expected),delta
output=BASE/'development'
output.mkdir(exist_ok=True)
raw=(json.dumps(current,indent=2,sort_keys=True)+'\n').encode()
with (output/f'candidate-{label}-source-freeze.json').open('xb') as stream:stream.write(raw)
receipt=dict(sourceManifestSha256=sha(raw),sourceFiles=len(current),rustFiles=sum(n.endswith('.rs') for n in current),
    changedPaths={name:dict(before=baseline.get(name),after=current[name]) for name in delta},
    actualBehavioralRed='cli-red-r3-receipt.json',rustTestsWritten=len(re.findall(r'#\[test\]\s*fn\s+', (ROOT/'native/tests/mixed_pdf_view.rs').read_text(encoding='utf-8'))),
    exactCandidateValidationNotReasserted=True,
    interpretation='Isolated candidate source seal only. Validation remains in separate immutable receipts; no primary integration or PDF whole acceptance.')
with (output/f'candidate-{label}.json').open('x',encoding='utf-8') as stream:stream.write(json.dumps(receipt,indent=2)+'\n')
print(json.dumps(receipt))
