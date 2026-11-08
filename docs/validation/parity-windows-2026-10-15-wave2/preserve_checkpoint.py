"""Preserve a new frozen checkpoint without rewriting earlier evidence."""
import argparse
import datetime
import hashlib
import json
from pathlib import Path
import shutil

from acceptance import ROOT, EVIDENCE, source_manifest


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--tags', nargs='+', required=True)
    parser.add_argument('--destination', required=True)
    parser.add_argument('--supplement', action='append', default=[])
    args = parser.parse_args()
    assert len(args.tags) == len(set(args.tags)), 'Duplicate checkpoint tags'
    destination = (ROOT / args.destination).resolve()
    validation_root = (ROOT / 'docs/validation').resolve()
    assert destination.is_relative_to(validation_root) and destination != validation_root
    assert not destination.exists(), 'Earlier evidence must never be overwritten'

    receipts = {}
    manifest = None
    copies = []
    phases = {}
    for tag in args.tags:
        assert Path(tag).name == tag and tag not in {'.', '..'}
        directory = EVIDENCE / tag
        receipt = json.loads((directory / 'receipt.json').read_text(encoding='utf-8'))
        assert receipt['accepted'] and not receipt['sourceChanges'], (tag, receipt)
        assert receipt['phase'] not in phases, 'One accepted run per phase is required'
        phases[receipt['phase']] = tag
        before = json.loads((directory / 'source-before.json').read_text(encoding='utf-8'))
        after = json.loads((directory / 'source-after.json').read_text(encoding='utf-8'))
        assert before == after
        assert manifest is None or manifest == before, 'Gates used different sources'
        manifest = before
        for result in receipt['results']:
            log = directory / (result['label'] + '.log')
            assert result['exitCode'] == 0 and digest(log) == result['logSha256'], log
        receipts[tag] = receipt
        copies.extend((path, Path(tag) / path.name) for path in sorted(directory.iterdir())
                      if path.is_file() and path.suffix in {'.log', '.json'})
    assert set(phases) == {'format', 'clippy', 'native', 'python', 'typescript'}, phases
    assert source_manifest() == manifest, 'Source changed after the accepted gates'
    native_tag = phases['native']
    native = receipts[native_tag]
    assert native['passed'] > 0 and native['failed'] == 0

    previous = ROOT / 'docs/validation/parity-windows-2026-10-15/index.json'
    old_index = json.loads(previous.read_text(encoding='utf-8'))
    for relative, expected in old_index['sha256'].items():
        assert digest(ROOT / relative) == expected, ('Earlier evidence changed', relative)

    for name in ['acceptance.py', 'progress.py', 'preserve_checkpoint.py', *args.supplement]:
        relative = Path(name)
        assert not relative.is_absolute() and '..' not in relative.parts
        source = (EVIDENCE / relative).resolve()
        assert source.is_relative_to(EVIDENCE.resolve()) and source.is_file(), source
        copies.append((source, relative))
    assert len({target.as_posix() for _, target in copies}) == len(copies), 'Duplicate copy path'
    if 'repairedFrom' in native:
        baseline_tag = native['repairedFrom']['tag']
        supplied = {target.as_posix() for _, target in copies}
        assert f'{baseline_tag}/receipt.json' in supplied
        assert f'{baseline_tag}/package.log' in supplied
        assert digest(EVIDENCE / baseline_tag / 'receipt.json') == native['repairedFrom']['receiptSha256']
        assert digest(EVIDENCE / baseline_tag / 'package.log') == native['repairedFrom']['originalFailedLogSha256']

    # All admission checks precede creation of the new metadata directory.
    destination.mkdir()
    hashes = {}
    for source, relative in copies:
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, target)
        assert digest(target) == digest(source), target
        hashes[target.relative_to(ROOT).as_posix()] = digest(target)
    manifest_path = destination / native_tag / 'source-before.json'
    inventory_path = destination / native_tag / 'target-inventory.json'
    inventory = json.loads(inventory_path.read_text(encoding='utf-8'))
    index = {
        'schemaVersion': 1,
        'preservedAtUtc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
        'scope': 'Unreleased Windows development checkpoint; universal parity remains open',
        'acceptedRun': native_tag,
        'acceptedPhases': phases,
        'sourceManifest': manifest_path.relative_to(ROOT).as_posix(),
        'sourceManifestSha256': digest(manifest_path),
        'sourceFiles': native['sourceFiles'],
        'cargoTargets': len(inventory),
        'groups': sum(result['groups'] for result in native['results']),
        'passed': native['passed'], 'failed': native['failed'], 'ignored': native['ignored'],
        'sourceChangesDuringEachGate': [],
        'metadataBoundary': 'These evidence copies and subsequent status, reconciliation and contract-documentation edits were added after the accepted source snapshot and are outside that manifest.',
        'previousEvidenceIndex': previous.relative_to(ROOT).as_posix(),
        'previousEvidenceIndexSha256': digest(previous),
        'supplementInterpretation': 'Supplementary focused logs retain their own results; inclusion does not turn failed baselines into accepted runs.',
        'sha256': hashes,
    }
    if 'repairedFrom' in native:
        index['nativeExecution'] = {
            'strategy': 'explicit_frozen_source_matrix_with_fixture_prerequisite_retries',
            'receipt': (destination / native_tag / 'receipt.json').relative_to(ROOT).as_posix(),
            'failedBaselineReceipt': (destination / native['repairedFrom']['tag'] / 'receipt.json').relative_to(ROOT).as_posix(),
            'successfulGroupsRetainedWithOriginalHashes': True,
            'originalFailedAttemptPreserved': True,
            'fixtureTempRoots': native['fixtureTempRoots'],
            'integrationTargets': native['matrixCoverage']['integrationTargets'],
            'registeredLibraryTests': native['matrixCoverage']['registeredLibraryTests'],
            'librarySkippedThenIndividuallyExecuted': native['matrixCoverage']['librarySkippedThenIndividuallyExecuted'],
            'libraryCoverageAccepted': native['matrixCoverage']['libraryCoverageAccepted'],
        }
    (destination / 'index.json').write_text(json.dumps(index, indent=2) + '\n', encoding='utf-8')
    print(json.dumps({'destination': str(destination), 'files': len(hashes),
                      'passed': native['passed'], 'failed': native['failed'],
                      'sourceManifestSha256': digest(manifest_path)}))


if __name__ == '__main__':
    main()
