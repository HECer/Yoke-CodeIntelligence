"""Independently verify Wave13 archive, raw totals, integration and current source."""
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path('G:/NN-Developed/Dev-Tools/Yoke-CodeIntelligence/Code-Intelligence')
BASE = Path(__file__).resolve().parent
DEST = ROOT/'docs/validation/parity-windows-2026-10-17-wave13'
sha = lambda raw: hashlib.sha256(raw).hexdigest()
read = lambda path: json.loads(path.read_bytes())
METADATA = {'docs/STATUS.md','docs/parity/native-reconciliation-2026-10-15.json','docs/parity/windows-development-gates-2026-10-15.json','native/docs/windows-parity-2026-10-15.md'}
def verified_parent_map(parent_index, parent, expected_index):
    def insist(ok, message):
        if not ok:
            raise RuntimeError(message)
    files = {}
    for path in sorted(parent_index.parent.rglob('*')):
        insist(not path.is_symlink() and path.resolve().is_relative_to(parent_index.parent.resolve()),
            'Parent archive path escaped')
        if path.is_file():
            files[path.relative_to(parent_index.parent).as_posix()] = path.read_bytes()
    value = json.loads(files['index.json'])
    insist(sha(files['index.json']) == expected_index == parent['indexSha256']
        == files['index.sha256'].decode().strip(), 'Parent index pin differs')
    insist(value['accepted'] is True and not any(value['claims'].values())
        and value['acceptedTag'] == parent['acceptedTag'] == 'mixed-view-windows-20261017-r1', 'Parent acceptance differs')
    special = {'index.json','index.sha256','post-freeze-metadata.json'}
    insist(not special.intersection(value['artifactPins']) and set(files) == set(value['artifactPins'])|special,
        'Parent archive file closure differs')
    insist(all(sha(files[name]) == digest for name,digest in value['artifactPins'].items()), 'Parent archive pin differs')
    prefix = 'evidence/'+value['acceptedTag']+'/'
    frozen_raw = files[prefix+'source-freeze.json']
    frozen = json.loads(frozen_raw)
    summary = json.loads(files[prefix+'summary.json'])
    insist(summary['accepted'] is True and sha(frozen_raw) == value['sourceManifestSha256']
        == parent['sourceManifestSha256'] == summary['sourceManifestSha256']
        and len(frozen) == value['sourceFiles'] == summary['sourceFiles'], 'Parent source freeze differs')
    runner = '5b1dbf5179f274d57fc0217108867d90d9a62ae233ca551496e89d386df81a00'
    insist(sha(files[prefix+'runner.py']) == summary['runnerSha256'] == runner, 'Parent runner differs')
    phases = ['format','clippy','native','python','typescript']
    insist(set(value['phaseReceipts']) == set(summary['phaseReceipts']) == set(phases), 'Parent phase closure differs')
    receipts = {}
    for phase in phases:
        raw = files[prefix+phase+'/receipt.json']
        receipt = json.loads(raw)
        receipts[phase] = receipt
        insist(sha(raw) == value['phaseReceipts'][phase]['receiptSha256']
            == summary['phaseReceipts'][phase]['receiptSha256'], 'Parent phase receipt pin differs')
        insist(receipt['accepted'] is True and receipt['phase'] == phase
            and not receipt['sourceChanges'] and not receipt['validationErrors']
            and receipt['sourceManifestSha256'] == sha(frozen_raw) and receipt['runnerSha256'] == runner,
            'Parent phase admission differs')
        insist(json.loads(files[prefix+phase+'/source-before.json'])
            == json.loads(files[prefix+phase+'/source-after.json']) == frozen, 'Parent phase source maps differ')
        insist(all(sha(files[prefix+phase+'/'+name]) == digest for name,digest in receipt['artifactSha256'].items()),
            'Parent phase artifact differs')
        for row in receipt['results']:
            insist(row['exitCode'] == row['failed'] == 0 and row['origin'] == 'fresh_execution'
                and row['sourceManifestSha256'] == sha(frozen_raw) and row['runnerSha256'] == runner
                and sha(files[prefix+phase+'/'+row['label']+'.log']) == row['logSha256'], 'Parent result differs')
        for key in ['passed','failed','ignored','groups']:
            insist(receipt[key] == sum(row[key] for row in receipt['results'])
                == value['phaseReceipts'][phase][key] == summary['phaseReceipts'][phase][key],
                'Parent phase aggregate differs')
    native = receipts['native']
    insist([native[k] for k in ['passed','failed','ignored','groups']] == [1898,0,33,244]
        and len(json.loads(files[prefix+'native/target-inventory.json'])) == 239,
        'Parent native inventory or total differs')
    raw_totals = [0,0,0,0]
    pattern = re.compile(rb'^test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;', re.M)
    for row in native['results']:
        if row['inventoryOnly']:
            continue
        groups = pattern.findall(files[prefix+'native/'+row['label']+'.log'])
        actual = [sum(int(g[n]) for g in groups) for n in range(3)]+[len(groups)]
        insist(actual == [row[k] for k in ['passed','failed','ignored','groups']], 'Parent raw native totals differ')
        raw_totals = [a+b for a,b in zip(raw_totals,actual)]
    insist(raw_totals == [1898,0,33,244] and receipts['python']['passed'] == 11
        and receipts['typescript']['passed'] == 254 and receipts['typescript']['ignored'] == 3,
        'Parent language/raw totals differ')
    post = json.loads(files['post-freeze-metadata.json'])
    metadata = set(METADATA)
    insist(post['indexSha256'] == expected_index and set(post['hashes']) == metadata
        and set(post['changedFrozenPaths']) == metadata and set(value['postFreezeMetadataFiles']) == metadata,
        'Parent post-freeze metadata differs')
    insist(all(name in frozen and re.fullmatch('[0-9a-f]{64}', digest) for name,digest in post['hashes'].items()),
        'Invalid parent metadata identity')
    result = dict(frozen)
    result.update(post['hashes'])
    archive_prefix = parent_index.parent.relative_to(ROOT).as_posix()+'/'
    insist(not any(name.startswith(archive_prefix) for name in frozen), 'Parent archive already in frozen map')
    included = set(subprocess.check_output(['git','ls-files','-c','-o','--exclude-standard','-z','--',
        parent_index.parent.relative_to(ROOT).as_posix()],cwd=ROOT).decode().split('\0'))-{''}
    insist(included <= {archive_prefix+name for name in files}, 'Unexpected Git-admitted parent proof path')
    result.update({archive_prefix+name:sha(raw) for name,raw in files.items() if archive_prefix+name in included})
    return result, frozen


expected = sys.argv[1]
assert re.fullmatch('[0-9a-f]{64}', expected)
raw = (DEST/'index.json').read_bytes()
assert sha(raw) == expected == (DEST/'index.sha256').read_text().strip()
index = json.loads(raw)
assert index['accepted'] is True and not any(index['claims'].values())
for name, digest in index['artifactPins'].items():
    path = DEST/name
    assert path.resolve().is_relative_to(DEST.resolve()) and not path.is_symlink()
    assert sha(path.read_bytes()) == digest
bundle = DEST/'evidence'/index['acceptedTag']
summary = read(bundle/'summary.json')
assert summary['accepted'] is True
frozen_raw = (bundle/'source-freeze.json').read_bytes()
frozen = json.loads(frozen_raw)
assert sha(frozen_raw) == index['sourceManifestSha256'] == summary['sourceManifestSha256']
assert len(frozen) == index['sourceFiles'] == summary['sourceFiles']
assert sum(name.endswith('.rs') for name in frozen) == index['rustFiles']
assert sha((bundle/'runner.py').read_bytes()) == summary['runnerSha256'] == '5b1dbf5179f274d57fc0217108867d90d9a62ae233ca551496e89d386df81a00'
assert len(index['nonadditiveFeedbackCliTests']) == 21
phases = {}
for phase in ['format', 'clippy', 'native', 'python', 'typescript']:
    path = bundle/phase/'receipt.json'
    receipt = read(path)
    phases[phase] = receipt
    assert sha(path.read_bytes()) == summary['phaseReceipts'][phase]['receiptSha256'] == index['phaseReceipts'][phase]['receiptSha256']
    assert receipt['accepted'] is True and not receipt['sourceChanges'] and not receipt['validationErrors']
    assert receipt['sourceManifestSha256'] == index['sourceManifestSha256']
    assert read(bundle/phase/'source-before.json') == read(bundle/phase/'source-after.json') == frozen
    for name, digest in receipt['artifactSha256'].items():
        assert sha((bundle/phase/name).read_bytes()) == digest
    for row in receipt['results']:
        assert row['exitCode'] == row['failed'] == 0 and row['origin'] == 'fresh_execution'
        assert sha((bundle/phase/(row['label']+'.log')).read_bytes()) == row['logSha256']
        assert row['sourceManifestSha256'] == index['sourceManifestSha256'] and row['runnerSha256'] == summary['runnerSha256']
native = phases['native']
durable = 'host_install::output_recovery_tests::sixteen_root_v2_history_reserves_only_remaining_output_work'
assert native['matrixCoverage']['libraryDurableTests'] == [durable]
special = [r for r in native['results'] if r['label'] == 'library-c-durable-0']
assert len(special) == 1 and special[0]['testGroups'][0]['rows'] == {durable:'ok'}
assert special[0]['tempRoot'] == native['aclTempRoot'] and special[0]['command'][-2:] == ['--test-threads=1','--exact']
totals = [0, 0, 0, 0]
pattern = re.compile(r'^test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;', re.M)
for row in native['results']:
    if row['inventoryOnly']:
        continue
    text = (bundle/'native'/(row['label']+'.log')).read_text(encoding='utf-8')
    groups = pattern.findall(text)
    counters = [sum(int(g[n]) for g in groups) for n in range(3)]+[len(groups)]
    assert counters == [row[k] for k in ['passed', 'failed', 'ignored', 'groups']]
    totals = [a+b for a, b in zip(totals, counters)]
assert totals == [native[k] for k in ['passed', 'failed', 'ignored', 'groups']] == [1919, 0, 33, 245]
assert len(read(bundle/'native/target-inventory.json')) == 240
assert phases['python']['passed'] == 11 and phases['typescript']['passed'] == 254 and phases['typescript']['ignored'] == 3
names = set(index['nonadditiveFeedbackCliTests'])
executed = []
for row in native['results']:
    if not row['inventoryOnly']:
        executed += [g for g in row['testGroups'] if g.get('target') == 'feedback_cli']
assert len(executed) == 1 and set(executed[0]['rows']) == names and set(executed[0]['rows'].values()) == {'ok'}
integration = read(DEST/'integration/receipt.json')
assert integration == dict(accepted=True, **read(DEST/'integration/proposal.json'))
assert len(integration['paths']) == 14
for name, proof in integration['paths'].items():
    original = DEST/'integration/original'/name
    assert proof['before'] == (sha(original.read_bytes()) if original.exists() else None)
    assert proof['after'] == frozen[name]
assert sha((DEST/'integration/operation-executed.py').read_bytes()) == '7e65528091d28639b8b27d18b6b473c8cbdf92c2a095467dce34be879936e863'

baseline_raw = (DEST/'development-inputs/baseline-source-freeze.json').read_bytes()
candidate_raw = (DEST/'development-probes/review-candidate-source-freeze.json').read_bytes()
assert sha(baseline_raw) == integration['reviewedBaselineSourceManifestSha256'] == '1ef41d7564050c370f7db36fd539e0db1fa081248fe282db4997c46572d09e26'
assert sha(candidate_raw) == integration['candidateSourceManifestSha256'] == 'd6510d06d4cc8d3f53a0f375f4801e2529e24b16c7a5a960e47bafcfb4c4f5ec'
baseline, candidate = json.loads(baseline_raw), json.loads(candidate_raw)
assert {name for name in set(baseline)|set(candidate) if baseline.get(name) != candidate.get(name)} == set(integration['paths'])
for name, proof in integration['paths'].items():
    assert proof['before'] == baseline.get(name) and proof['after'] == candidate[name]

meta = index['postFreezeMetadataFiles']
originals = {}
for name in meta:
    path = BASE/'metadata-backups'/frozen[name]/name
    assert sha(path.read_bytes()) == frozen[name]
    originals[name] = path.read_bytes()
old = json.loads(originals['docs/parity/windows-development-gates-2026-10-15.json'])
gates = read(ROOT/'docs/parity/windows-development-gates-2026-10-15.json')
for key, value in old.items():
    if key.endswith('WaveAcceptance') or key == 'meaningGroupProjectionScopedChecks':
        assert gates[key] == value
assert gates['latestWholeNativeRun'] == gates['wholeNativeRun'] == gates['thirteenthWaveAcceptance']
assert gates['thirteenthWaveAcceptance']['indexSha256'] == expected
assert gates['twelfthWaveAcceptance']['indexSha256'] == integration['parentIndexSha256']
before_raw=(DEST/'integration/source-before.json').read_bytes()
after_raw=(DEST/'integration/source-after.json').read_bytes()
assert sha(before_raw)==integration['beforeSourceManifestSha256'] and sha(after_raw)==integration['afterSourceManifestSha256']
before_map,after_map=json.loads(before_raw),json.loads(after_raw)
parent=gates['twelfthWaveAcceptance']
assert parent['index']=='docs/validation/parity-windows-2026-10-17-wave12/index.json'
expected_before,parent_map=verified_parent_map(ROOT/parent['index'],parent,integration['parentIndexSha256'])
assert before_map==expected_before and sha((ROOT/parent['sourceManifest']).read_bytes())==integration['parentSourceManifestSha256']
for name,proof in integration['paths'].items():
    assert proof['before']==parent_map.get(name)
expected_after=dict(before_map)
expected_after.update({name:proof['after'] for name,proof in integration['paths'].items()})
assert after_map==expected_after==frozen
parent_changes={name for name in set(parent_map)|set(baseline) if parent_map.get(name)!=baseline.get(name)}
assert all(name in METADATA or (name.startswith('docs/validation/parity-windows-2026-10-17-wave11/') and name not in baseline) for name in parent_changes)

previous = json.loads(originals['docs/parity/native-reconciliation-2026-10-15.json'])
reconciliation = read(ROOT/'docs/parity/native-reconciliation-2026-10-15.json')
for key in ['inventoryLedger', 'ledgerSummary']:
    assert reconciliation[key] == previous[key]
for key, value in previous.items():
    if key.endswith('WaveAcceptance'):
        assert reconciliation[key] == value
mapping = reconciliation['currentBehaviorMapping']
assert mapping == previous['currentBehaviorMapping']
assert sha((ROOT/mapping['path']).read_bytes()) == mapping['sha256']
inventory = subprocess.check_output(['git', 'ls-files', '-c', '-o', '--exclude-standard', '-z'], cwd=ROOT).decode().split('\0')
current = {name: sha((ROOT/name).read_bytes()) for name in sorted(set(inventory))
    if name and not name.startswith(DEST.relative_to(ROOT).as_posix()+'/') and (ROOT/name).is_file()}
assert set(current) == set(frozen)
assert sorted(name for name in frozen if current[name] != frozen[name]) == sorted(meta)
post = read(DEST/'post-freeze-metadata.json')
assert post['indexSha256'] == expected and post['hashes'] == {name: current[name] for name in meta}
print(json.dumps(dict(verified=True, indexSha256=expected, artifactPins=len(index['artifactPins']),
    nativePassed=totals[0], nativeIgnored=totals[2], groups=totals[3], typescriptPassed=254,
    typescriptSkipped=3, pythonPassed=11, newFeedbackCliTests=21, originalLedgerUnchanged=True,
    historicalAcceptanceUnchanged=True, currentFacadeMappingPinVerified=True)))
