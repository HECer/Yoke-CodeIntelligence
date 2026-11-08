"""Check actual isolated receipts and seal their source; no whole-run claim."""
import hashlib
import json
from pathlib import Path
import re
import subprocess

BASE = Path(__file__).resolve().parent
ROOT = Path('E:/Yoke-Worktrees/pdf-mixed-view-20261017')
DEVELOPMENT = BASE/'development'
FINAL = 'a3e0210e314936ffe007661111e2780d02081dbd41c9ca21467a85129646f247'
EARLIER = 'f23616f840be3d746859b469f1473b221747e1085aa2f85342fcfcfb05a8ddab'
BASELINE = 'd6510d06d4cc8d3f53a0f375f4801e2529e24b16c7a5a960e47bafcfb4c4f5ec'
RUNNER = 'dbf6e95ee624c6f05ec0edf336ae6c0d4ec302d48120e6211e13ede1bb833062'
sha = lambda raw: hashlib.sha256(raw).hexdigest()
read = lambda path: json.loads(path.read_bytes())
pins = {}

def pinned(path):
    raw = path.read_bytes()
    pins[path.relative_to(BASE).as_posix()] = sha(raw)
    return raw

final_raw = pinned(DEVELOPMENT/'candidate-r3-source-freeze.json')
assert sha(final_raw) == FINAL
final = json.loads(final_raw)
baseline_raw = pinned(BASE/'baseline-source-freeze.json')
assert sha(baseline_raw) == BASELINE
baseline = json.loads(baseline_raw)
earlier_raw = pinned(DEVELOPMENT/'candidate-r2-source-freeze.json')
assert sha(earlier_raw) == EARLIER
earlier = json.loads(earlier_raw)
assert set(final) == set(earlier)
assert [name for name in sorted(final) if final[name] != earlier[name]] == ['native/docs/mixed-views.md']
assert sha(pinned(BASE/'run_scoped.py')) == RUNNER

receipts = {}
for stem in ['pdf-r1', 'regressions-r1', 'pdf-r2', 'format-r1', 'clippy-r1',
             'typecheck-r1', 'typescript-r1']:
    receipt = json.loads(pinned(DEVELOPMENT/(stem+'.json')))
    expected = EARLIER if stem in {'pdf-r1', 'regressions-r1'} else FINAL
    assert receipt['exitCode'] == 0 and receipt['sourceChanges'] == []
    assert receipt['runnerSha256'] == RUNNER
    assert receipt['sourceBefore'] == receipt['sourceAfter'] == expected
    assert sha(pinned(DEVELOPMENT/receipt['log'])) == receipt['logSha256']
    for suffix in ['before', 'after']:
        raw = pinned(DEVELOPMENT/(stem+'-'+suffix+'.json'))
        assert sha(raw) == expected
        assert json.loads(raw) == (earlier if expected == EARLIER else final)
    receipts[stem] = receipt

def native_totals(stem):
    log = (DEVELOPMENT/(stem+'.log')).read_text(encoding='utf-8')
    rows = re.findall(r'^test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', log, re.M)
    return [sum(int(row[i]) for row in rows) for i in range(3)]+[len(rows)]

assert native_totals('pdf-r1') == native_totals('pdf-r2') == [9, 0, 0, 1]
assert native_totals('regressions-r1') == [103, 0, 0, 14]
typescript = (DEVELOPMENT/'typescript-r1.log').read_text(encoding='utf-8')
rows = re.findall(r'^ℹ (tests|pass|fail|cancelled|skipped) (\d+)\r?$', typescript, re.M)
assert len(rows) == 5
assert {key: int(value) for key, value in rows} == {
    'tests': 257, 'pass': 254, 'fail': 0, 'cancelled': 0, 'skipped': 3}

red = json.loads(pinned(BASE/'cli-red-r3-receipt.json'))
green = json.loads(pinned(DEVELOPMENT/'contract-green.json'))
contract_sha = sha(pinned(BASE/'contract_red.py'))
assert red['contractSha256'] == green['contractSha256'] == contract_sha
assert red['passed'] == 0 and red['failed'] == 2
assert sha(pinned(BASE/red['log'])) == red['logSha256']
assert sha(pinned(BASE/'original-pdf-worker-oracle.json')) == red['originalPdfWorkerOracleSha256']
assert green['ran'] == 2 and green['failures'] == green['errors'] == green['skipped'] == 0
assert sha(Path(green['executable']).read_bytes()) == green['executableSha256']
pinned(DEVELOPMENT/'contract-green.log')
pinned(BASE/'run_contract_green.py')
pinned(Path(__file__).resolve())
names = subprocess.check_output(['git', 'ls-files', '-c', '-o', '--exclude-standard', '-z'], cwd=ROOT).decode().split('\0')
current = {name: sha((ROOT/name).read_bytes()) for name in sorted(set(names))
           if name and (ROOT/name).is_file()}
assert current == final
changed = {name: dict(before=baseline.get(name), after=final.get(name))
           for name in sorted(set(baseline)|set(final)) if baseline.get(name) != final.get(name)}
assert len(changed) == 9
for name in ['validated-candidate.json', 'review-candidate-source-freeze.json']:
    assert not (DEVELOPMENT/name).exists()
with (DEVELOPMENT/'review-candidate-source-freeze.json').open('xb') as stream:
    stream.write(final_raw)
receipt = dict(sourceManifestSha256=FINAL, sourceFiles=len(final),
    rustFiles=sum(name.endswith('.rs') for name in final), changedPaths=changed,
    artifactPins=pins, scopedUniqueRustPassed=112, newPdfTests=9,
    existingRegressionTests=103, regressionTargets=14,
    unchangedCliContractGreen=2, typescriptPassed=254, typescriptSkipped=3,
    sourceUnchanged=True, format='passed', allTargetClippy='passed', typecheck='passed',
    earlierSourceContinuity='Only the mixed-views.md schema2 link differs; all code/test bytes are identical. Earlier scoped counts remain nonadditive.',
    primaryIntegrated=False, wholeAcceptance=False, universalParity=False,
    interpretation='Actual isolated development checks only. Nine PDF tests were repeated, never added twice; final primary integration requires accepted Wave13.')
with (DEVELOPMENT/'validated-candidate.json').open('x', encoding='utf-8') as stream:
    stream.write(json.dumps(receipt, indent=2)+'\n')
print(json.dumps({key: receipt[key] for key in ['sourceManifestSha256', 'sourceFiles',
    'rustFiles', 'scopedUniqueRustPassed', 'typescriptPassed', 'typescriptSkipped',
    'primaryIntegrated', 'wholeAcceptance']}))
