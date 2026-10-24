"""Apply the sealed nine-path PDF delta after checked Wave13 acceptance."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import uuid

BASE = Path(__file__).resolve().parent
ROOT = Path('G:/NN-Developed/Dev-Tools/Yoke-CodeIntelligence/Code-Intelligence')
AUX = Path('E:/Yoke-Worktrees/pdf-mixed-view-20261017')
PARENT = ROOT / 'docs/validation/parity-windows-2026-10-17-wave13'
BASELINE_SHA = 'd6510d06d4cc8d3f53a0f375f4801e2529e24b16c7a5a960e47bafcfb4c4f5ec'
CANDIDATE_SHA = 'a3e0210e314936ffe007661111e2780d02081dbd41c9ca21467a85129646f247'
METADATA = {'docs/STATUS.md', 'docs/parity/native-reconciliation-2026-10-15.json',
    'docs/parity/windows-development-gates-2026-10-15.json', 'native/docs/windows-parity-2026-10-15.md'}
NAMES = [
    'native/src/media.rs',
    'native/src/mixed_view.rs',
    'native/src/mixed_view_query.rs',
    'native/src/mixed_view_cli.rs',
    'native/src/mixed_pdf_input.rs',
    'native/tests/mixed_pdf_view.rs',
    'native/tests/support/mixed_pdf_fixture.rs',
    'native/docs/mixed-pdf-views.md',
    'native/docs/mixed-views.md',
]


def require(ok, message):
    if not ok:
        raise RuntimeError(message)


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def encoded(value):
    return (json.dumps(value, indent=2, sort_keys=True)+'\n').encode()


def manifest(folder):
    raw = subprocess.check_output(['git', 'ls-files', '-c', '-o', '--exclude-standard', '-z'], cwd=folder)
    result = {}
    for name in sorted(set(raw.decode().split('\0'))):
        if not name:
            continue
        path = folder / name
        require(path.resolve().is_relative_to(folder.resolve()) and not path.is_symlink(),
            'Source path escaped: '+name)
        if path.is_file():
            result[name] = sha(path.read_bytes())
    return result


def checked_candidate():
    baseline_raw = (BASE/'baseline-source-freeze.json').read_bytes()
    candidate_raw = (BASE/'development/review-candidate-source-freeze.json').read_bytes()
    require(sha(baseline_raw) == BASELINE_SHA, 'Reviewed baseline map changed')
    require(sha(candidate_raw) == CANDIDATE_SHA, 'Reviewed candidate map changed')
    baseline, candidate = json.loads(baseline_raw), json.loads(candidate_raw)
    require(manifest(AUX) == candidate, 'Auxiliary inventory differs from reviewed candidate')
    changed = sorted(name for name in set(candidate)|set(baseline)
        if candidate.get(name) != baseline.get(name))
    require(changed == sorted(NAMES), 'Auxiliary delta exceeds nine reviewed paths: '+repr(changed))
    return baseline, candidate


def parent_current_map(index, frozen, expected_index):
    files = {}
    for path in sorted(PARENT.rglob('*')):
        require(not path.is_symlink() and path.resolve().is_relative_to(PARENT.resolve()), 'Parent archive path escaped')
        if path.is_file():
            files[path.relative_to(PARENT).as_posix()] = path.read_bytes()
    special = {'index.json','index.sha256','post-freeze-metadata.json'}
    require(not special.intersection(index['artifactPins']) and set(files) == set(index['artifactPins'])|special,
        'Parent archive file closure differs')
    require(sha(files['index.json']) == expected_index and files['index.sha256'].decode().strip() == expected_index,
        'Parent index changed')
    for name, digest in index['artifactPins'].items():
        require(sha(files[name]) == digest, 'Parent archive pin differs')
    post = json.loads(files['post-freeze-metadata.json'])
    require(post['indexSha256'] == expected_index and set(post['hashes']) == METADATA
        and set(post['changedFrozenPaths']) == METADATA and set(index['postFreezeMetadataFiles']) == METADATA,
        'Parent post-freeze metadata set differs')
    require(all(name in frozen and re.fullmatch('[0-9a-f]{64}', digest) for name,digest in post['hashes'].items()),
        'Invalid parent metadata identity')
    result = dict(frozen)
    result.update(post['hashes'])
    prefix = PARENT.relative_to(ROOT).as_posix()+'/'
    require(not any(name.startswith(prefix) for name in frozen), 'Parent archive already present in frozen map')
    included = set(subprocess.check_output(['git','ls-files','-c','-o','--exclude-standard','-z','--',
        PARENT.relative_to(ROOT).as_posix()],cwd=ROOT).decode().split('\0'))-{''}
    require(included <= {prefix+name for name in files}, 'Unexpected Git-admitted parent proof path')
    result.update({prefix+name:sha(raw) for name,raw in files.items() if prefix+name in included})
    return result


def main():
    arguments = argparse.ArgumentParser()
    arguments.add_argument('parent_index_sha256')
    arguments.add_argument('--validate-only', action='store_true')
    args = arguments.parse_args()
    expected_index = args.parent_index_sha256
    require(re.fullmatch('[0-9a-f]{64}', expected_index), 'Invalid parent index pin')
    require(sha((PARENT/'index.json').read_bytes()) == expected_index, 'Parent checkpoint index changed')
    environment = os.environ.copy()
    environment['RTK_SUPPRESS_HOOK_WARNING'] = '1'
    verifier = BASE.parent/'outcome-learning-20261017/verify_checkpoint.py'
    require(sha(verifier.read_bytes()) == '7cabba3e438926a5667aa5e41f6e22bf443d9fb0e9459b3fe2dd1d97c12fbe2e',
        'Reviewed parent verifier changed')
    subprocess.run([sys.executable, '-B', '-X', 'utf8',
        str(verifier), expected_index],
        cwd=ROOT, env=environment, check=True)
    index = json.loads((PARENT/'index.json').read_bytes())
    require(index['accepted'] is True and not any(index['claims'].values()), 'Unaccepted parent checkpoint')
    frozen_raw = (PARENT/'evidence'/index['acceptedTag']/'source-freeze.json').read_bytes()
    require(sha(frozen_raw) == index['sourceManifestSha256'], 'Parent source freeze changed')
    frozen = json.loads(frozen_raw)
    baseline, candidate = checked_candidate()
    # The sealed auxiliary baseline predates the two accepted parent archives.
    # Product bytes must match; only the four status files and those newly
    # admitted archive paths may differ. The parent verifier replays its
    # complete integration maps and closed Wave12 proof before this check.
    prior_prefixes = ('docs/validation/parity-windows-2026-10-17-wave11/',
        'docs/validation/parity-windows-2026-10-17-wave12/')
    allowed = {name for name in set(frozen)|set(baseline)
        if name in METADATA or (name.startswith(prior_prefixes) and name not in baseline)}
    changes = {name for name in set(frozen)|set(baseline) if frozen.get(name) != baseline.get(name)}
    require(changes <= allowed, 'Accepted parent differs from the reviewed product baseline')
    expected_before = parent_current_map(index, frozen, expected_index)
    before = manifest(ROOT)
    require(before == expected_before, 'Whole parent state changed after verification')
    require(all(before.get(name) == frozen.get(name) == baseline.get(name) for name in NAMES),
        'Parent integration preimage differs')
    updates = {name: (AUX/name).read_bytes() for name in NAMES}
    require(all(sha(raw) == candidate[name] for name, raw in updates.items()), 'Candidate buffer changed after admission')
    originals = {}
    for name in NAMES:
        path = ROOT/name
        require(path.resolve().is_relative_to(ROOT.resolve()) and not path.is_symlink(),
            'Integration target escaped: '+name)
        if frozen.get(name) is None:
            require(not os.path.lexists(path), 'Unregistered target already exists: '+name)
            originals[name] = None
        else:
            require(path.is_file(), 'Tracked integration target is not a file: '+name)
            originals[name] = path.read_bytes()
        actual = sha(originals[name]) if originals[name] is not None else None
        require(actual == before.get(name) == frozen.get(name), 'Original byte buffer differs: '+name)
    expected_after = dict(before)
    expected_after.update({name:sha(raw) for name,raw in updates.items()})
    before_raw, after_raw = encoded(before), encoded(expected_after)
    proposal = dict(parentIndexSha256=expected_index, parentSourceManifestSha256=sha(frozen_raw),
        reviewedBaselineSourceManifestSha256=BASELINE_SHA, beforeSourceManifestSha256=sha(before_raw), afterSourceManifestSha256=sha(after_raw),
        candidateSourceManifestSha256=CANDIDATE_SHA,
        paths={name: dict(before=sha(originals[name]) if originals[name] is not None else None,
            after=sha(updates[name])) for name in NAMES})
    if args.validate_only:
        print(json.dumps(dict(ready=True, **proposal)))
        return
    backup = BASE/'integration'
    backup.mkdir(exist_ok=False)
    (backup/'proposal.json').write_bytes(encoded(proposal))
    (backup/'source-before.json').write_bytes(before_raw)
    (backup/'source-after.json').write_bytes(after_raw)
    (backup/'operation-executed.py').write_bytes(Path(__file__).read_bytes())
    require(manifest(AUX) == candidate, 'Auxiliary inventory changed before primary writes')
    require(manifest(ROOT) == before, 'Primary inventory changed before integration writes')
    for name, raw in originals.items():
        if raw is not None:
            path = backup/'original'/name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(raw)
    for name, raw in updates.items():
        path = ROOT/name
        require((path.read_bytes() if path.exists() else None) == originals[name], 'Parent preimage changed: '+name)
        require((AUX/name).read_bytes() == raw, 'Reviewed source changed: '+name)
        stage = path.with_name('.'+path.name+'.'+uuid.uuid4().hex+'.tmp')
        with stage.open('xb') as stream:
            stream.write(raw)
            stream.flush()
            os.fsync(stream.fileno())
        try:
            require((path.read_bytes() if path.exists() else None) == originals[name],
                'Parent preimage changed before replace: '+name)
            os.replace(stage, path)
        finally:
            if stage.exists():
                stage.unlink()
    expected = dict(before)
    expected.update({name: sha(raw) for name, raw in updates.items()})
    require(expected == expected_after and manifest(ROOT) == expected_after, 'Unexpected primary changes during integration')
    (backup/'receipt.json').write_bytes(encoded(dict(accepted=True, **proposal)))
    print(json.dumps(dict(integrated=True, paths=len(NAMES), sourceFiles=len(expected),
        sourceManifestSha256=sha(encoded(expected)), receipt=str(backup/'receipt.json'))))


if __name__ == '__main__':
    main()
