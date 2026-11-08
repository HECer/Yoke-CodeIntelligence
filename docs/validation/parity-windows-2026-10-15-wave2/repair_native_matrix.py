"""Retry a package with explicit fixture prerequisites on identical frozen sources.

Successful integration/doc groups are retained byte-for-byte. Failed attempts
stay failures; this writes a separate, explicit matrix receipt.
"""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import time

from acceptance import ROOT, EVIDENCE, source_manifest

ACL_TESTS = [
    'host_install::io::tests::explicit_low_integrity_label_is_preserved_or_refused_before_human_bytes',
    'host_install::io::tests::prepared_output_preserves_owner_and_nondefault_group_before_human_bytes',
]
SUMMARY = rb'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;'


def inspect(data):
    groups = re.findall(SUMMARY, data)
    return dict(groups=len(groups), passed=sum(int(g[0]) for g in groups),
                failed=sum(int(g[1]) for g in groups), ignored=sum(int(g[2]) for g in groups))


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--baseline', required=True)
    parser.add_argument('--tag', required=True)
    parser.add_argument('--ordinary-temp', required=True)
    parser.add_argument('--acl-temp', required=True)
    args = parser.parse_args()
    for name in [args.baseline, args.tag]:
        assert Path(name).name == name and name not in {'.', '..'}
    baseline_dir = EVIDENCE / args.baseline
    baseline = json.loads((baseline_dir / 'receipt.json').read_text(encoding='utf-8'))
    assert baseline['phase'] == 'native' and baseline['accepted'] is False
    assert baseline['sourceChanges'] == [] and baseline['failed'] == 2
    original = json.loads((baseline_dir / 'source-before.json').read_text(encoding='utf-8'))
    assert original == json.loads((baseline_dir / 'source-after.json').read_text(encoding='utf-8'))
    assert source_manifest() == original, 'Code changed; successful baseline groups cannot be reused'
    failed = [r for r in baseline['results'] if r['exitCode'] != 0]
    assert len(failed) == 1 and failed[0]['label'] == 'package'
    assert failed[0]['failed'] == 2 and failed[0]['ignored'] == 0
    failed_log = (baseline_dir / 'package.log').read_bytes()
    failed_names = re.findall(rb'^test (\S+) \.\.\. FAILED\r?$', failed_log, re.MULTILINE)
    assert sorted(n.decode('utf-8') for n in failed_names) == sorted(ACL_TESTS)
    assert b'Fixture low label setup requires WRITE_OWNER' in failed_log
    assert b'Fixture group setup failed' in failed_log

    inventory = json.loads((baseline_dir / 'target-inventory.json').read_text(encoding='utf-8'))
    expected_targets = sorted(t['name'] for t in inventory if t['kind'] == ['test'])
    selected_targets = []
    retained = []
    for result in baseline['results']:
        log = baseline_dir / (result['label'] + '.log')
        assert sha(log) == result['logSha256']
        assert all(result[k] == v for k, v in inspect(log.read_bytes()).items())
        if result['label'] == 'package':
            continue
        assert result['exitCode'] == 0 and result['failed'] == 0
        command = result['command']
        names = [command[i + 1] for i, token in enumerate(command[:-1]) if token == '--test']
        if result['label'].startswith('integration-'):
            assert result['groups'] == len(names)
            selected_targets.extend(names)
        retained.append(result)
    assert sorted(selected_targets) == expected_targets
    assert len(selected_targets) == len(set(selected_targets))
    assert any(r['label'] == 'doctests' and '--doc' in r['command'] for r in retained)

    output = EVIDENCE / args.tag
    assert not output.exists()
    ordinary = Path(args.ordinary_temp).resolve()
    acl = Path(args.acl_temp).resolve()
    assert ordinary.is_absolute() and acl.is_absolute() and ordinary != acl
    assert not ordinary.exists() and not acl.exists()
    output.mkdir()
    ordinary.mkdir(parents=True)
    acl.mkdir(parents=True)
    shutil.copy2(baseline_dir / 'source-before.json', output / 'source-before.json')
    shutil.copy2(baseline_dir / 'target-inventory.json', output / 'target-inventory.json')
    shutil.copy2(baseline_dir / 'receipt.json', output / 'baseline-receipt.json')
    shutil.copy2(baseline_dir / 'package.log', output / 'baseline-failed-package.log')
    results = []
    for result in retained:
        shutil.copy2(baseline_dir / (result['label'] + '.log'), output / (result['label'] + '.log'))
        results.append(dict(result, origin='retained_success_on_identical_frozen_source',
                            originalRun=args.baseline, tempRoot=baseline['tempRoot']))

    def run(label, command, temp):
        assert source_manifest() == original
        log = output / (label + '.log')
        print(json.dumps({'running': label, 'tempRoot': str(temp), 'log': str(log)}), flush=True)
        started = time.monotonic()
        with log.open('wb') as stream:
            code = subprocess.run(['rtk', 'proxy', *command], cwd=ROOT,
                                  env=dict(os.environ, TEMP=str(temp), TMP=str(temp)),
                                  stdout=stream, stderr=subprocess.STDOUT).returncode
        result = dict(label=label, command=command, exitCode=code,
                      seconds=round(time.monotonic() - started, 3), logSha256=sha(log),
                      origin='executed_fixture_prerequisite_retry', tempRoot=str(temp),
                      **inspect(log.read_bytes()))
        results.append(result)
        print(json.dumps(result), flush=True)
        return result

    base = ['cargo', 'test', '--workspace', '--locked', '-j', '2', '--no-fail-fast']
    listing = run('lib-test-inventory', base + ['--lib', '--', '--list'], ordinary)
    assert listing['exitCode'] == 0
    registered = re.findall(r'^(.+): test\r?$', (output / 'lib-test-inventory.log').read_text(encoding='utf-8'), re.MULTILINE)
    skipped = [name for name in registered if any(test in name for test in ACL_TESTS)]
    assert sorted(skipped) == sorted(ACL_TESTS)
    assert len(registered) == len(set(registered)) == failed[0]['passed'] + failed[0]['failed']
    package_command = base + ['--lib', '--bins', '--examples', '--benches', '--', '--test-threads=4']
    for name in ACL_TESTS:
        package_command += ['--skip', name]
    package = run('package', package_command, ordinary)
    exact = []
    for index, name in enumerate(ACL_TESTS):
        exact.append(run(f'package-acl-{index}', base + ['--lib', name, '--', '--exact', '--test-threads=1'], acl))
    coverage_ok = (package['passed'] == len(registered) - len(ACL_TESTS)
                   and package['failed'] == 0 and package['ignored'] == 0
                   and all(r['passed'] == 1 and r['failed'] == 0 and r['ignored'] == 0 for r in exact))
    after = source_manifest()
    changes = sorted(n for n in original.keys() | after.keys() if original.get(n) != after.get(n))
    (output / 'source-after.json').write_text(json.dumps(after, indent=2), encoding='utf-8')
    receipt = dict(
        checkedAtUtc=datetime.datetime.now(datetime.timezone.utc).isoformat(), phase='native',
        sourceFiles=len(original), sourceChanges=changes, results=results,
        passed=sum(r['passed'] for r in results), failed=sum(r['failed'] for r in results),
        ignored=sum(r['ignored'] for r in results), tempRoot=str(ordinary),
        fixtureTempRoots=dict(retained=baseline['tempRoot'], ordinary=str(ordinary), acl=str(acl)),
        repairedFrom=dict(tag=args.baseline, accepted=False, failed=2, failureStage='ACL fixture setup before product invocation',
                          receiptSha256=sha(baseline_dir / 'receipt.json'),
                          originalFailedTests=ACL_TESTS, originalFailedLogSha256=sha(baseline_dir / 'package.log')),
        matrixCoverage=dict(integrationTargets=len(expected_targets), integrationTargetNames=expected_targets,
                            registeredLibraryTests=len(registered), librarySkippedThenIndividuallyExecuted=ACL_TESTS,
                            libraryCoverageAccepted=coverage_ok),
        interpretation='Explicit frozen-source matrix: successful integration/doc groups are retained with original hashes; package retries ordinary tests on D and two ACL setup fixtures on C. Original failed attempts remain failed and are preserved.',
        accepted=not changes and coverage_ok and all(r['exitCode'] == 0 for r in results))
    (output / 'receipt.json').write_text(json.dumps(receipt, indent=2), encoding='utf-8')
    print(json.dumps(receipt), flush=True)
    return 0 if receipt['accepted'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
