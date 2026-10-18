"""Frozen-source Windows acceptance with bounded on-disk test executables."""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import time

ROOT = Path(__file__).resolve().parents[2]
EVIDENCE = Path(__file__).resolve().parent

def source_manifest():
    paths = subprocess.check_output(['rtk', 'proxy', 'git', 'ls-files', '-c', '-o', '--exclude-standard', '-z'], cwd=ROOT)
    return {name: hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
            for name in sorted(set(paths.decode('utf-8').split('\0'))) if name and (ROOT / name).is_file()}

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('phase', choices=['format', 'clippy', 'native', 'python', 'typescript'])
    parser.add_argument('--tag', required=True)
    parser.add_argument('--temp', required=True)
    args = parser.parse_args()
    assert ROOT.name == 'Code-Intelligence' and (ROOT / 'native/Cargo.toml').is_file()
    output = EVIDENCE / args.tag
    output.mkdir(exist_ok=False)
    temp = Path(args.temp).resolve()
    assert temp.is_absolute() and not temp.exists()
    temp.mkdir(parents=True)
    before = source_manifest()
    (output / 'source-before.json').write_text(json.dumps(before, indent=2), encoding='utf-8')
    environment = dict(os.environ, TEMP=str(temp), TMP=str(temp))
    results = []

    def run(label, command):
        log = output / (label + '.log')
        started = time.monotonic()
        print(json.dumps({'running': label, 'sourceFiles': len(before), 'log': str(log)}), flush=True)
        with log.open('wb') as handle:
            status = subprocess.run(['rtk', 'proxy', *command], cwd=ROOT, env=environment, stdout=handle, stderr=subprocess.STDOUT).returncode
        data = log.read_bytes()
        groups = re.findall(rb'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;', data)
        result = {'label': label, 'command': command, 'exitCode': status,
                  'seconds': round(time.monotonic() - started, 3),
                  'logSha256': hashlib.sha256(data).hexdigest(),
                  'groups': len(groups), 'passed': sum(int(g[0]) for g in groups),
                  'failed': sum(int(g[1]) for g in groups), 'ignored': sum(int(g[2]) for g in groups)}
        results.append(result)
        print(json.dumps(result), flush=True)

    base = ['cargo', 'test', '--workspace', '--locked', '-j', '2', '--no-fail-fast']
    if args.phase == 'format':
        run('format', ['cargo', 'fmt', '--all', '--', '--check'])
    elif args.phase == 'clippy':
        run('clippy', ['cargo', 'clippy', '--workspace', '--all-targets', '--locked', '-j', '2', '--', '-D', 'warnings'])
    elif args.phase == 'python':
        run('python', ['python', '-m', 'unittest', 'discover', '-s', 'scripts/tests', '-v'])
    elif args.phase == 'typescript':
        run('typecheck', ['npm.cmd', 'run', 'typecheck'])
        run('typescript', ['npm.cmd', 'test'])
    else:
        metadata = json.loads(subprocess.check_output(['rtk', 'proxy', 'cargo', 'metadata', '--no-deps', '--format-version', '1'], cwd=ROOT))
        package = next(p for p in metadata['packages'] if p['name'] == 'yoke-code-intelligence')
        targets = package['targets']
        unsupported = [t for t in targets if not set(t['kind']).issubset({'lib', 'bin', 'test', 'example', 'bench'})]
        assert not unsupported, unsupported
        (output / 'target-inventory.json').write_text(json.dumps(targets, indent=2), encoding='utf-8')
        tests = sorted(t['name'] for t in targets if t['kind'] == ['test'])
        deps = (Path(metadata['target_directory']) / 'debug/deps').resolve()
        intended_target = (ROOT / 'target').resolve()
        assert deps.is_relative_to(intended_target), (deps, intended_target)
        old_test_outputs = []
        for path in deps.iterdir():
            if not any(re.fullmatch(re.escape(name) + r'-[0-9a-f]+\.(?:exe|pdb)', path.name) for name in tests):
                continue
            resolved = path.resolve()
            assert resolved.is_relative_to(deps) and resolved.parent == deps
            resolved.unlink()
            old_test_outputs.append(path.name)
        (output / 'cleaned-before.json').write_text(json.dumps(old_test_outputs), encoding='utf-8')
        run('package', base + ['--lib', '--bins', '--examples', '--benches', '--', '--test-threads=4'])
        run('doctests', base + ['--doc', '--', '--test-threads=4'])
        for offset in range(0, len(tests), 6):
            names = tests[offset:offset + 6]
            selectors = [item for name in names for item in ['--test', name]]
            run(f'integration-{offset // 6:02d}', base + selectors + ['--', '--test-threads=4'])
            # Only disposable Cargo test executable/PDB outputs, after their
            # process has ended; sources, logs and dependency libraries remain.
            removed = []
            for path in deps.iterdir():
                if not any(re.fullmatch(re.escape(name) + r'-[0-9a-f]+\.(?:exe|pdb)', path.name) for name in names):
                    continue
                resolved = path.resolve()
                assert resolved.is_relative_to(deps) and resolved.parent == deps
                resolved.unlink()
                removed.append(path.name)
            (output / f'cleaned-{offset // 6:02d}.json').write_text(json.dumps(removed), encoding='utf-8')
    after = source_manifest()
    (output / 'source-after.json').write_text(json.dumps(after, indent=2), encoding='utf-8')
    changed = sorted(name for name in before.keys() | after.keys() if before.get(name) != after.get(name))
    receipt = {'checkedAtUtc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
               'phase': args.phase, 'sourceFiles': len(before), 'sourceChanges': changed,
               'results': results, 'passed': sum(r['passed'] for r in results),
               'failed': sum(r['failed'] for r in results), 'ignored': sum(r['ignored'] for r in results),
               'accepted': not changed and all(r['exitCode'] == 0 for r in results),
               'tempRoot': str(temp)}
    (output / 'receipt.json').write_text(json.dumps(receipt, indent=2), encoding='utf-8')
    print(json.dumps(receipt), flush=True)
    return 0 if receipt['accepted'] else 1

if __name__ == '__main__':
    raise SystemExit(main())
