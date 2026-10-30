"""Owned Node checks for the exact diagnostic source commit; no native image."""
from pathlib import Path
import hashlib
import json
import os
import re
import sys
import types

ROOT = Path('C:/Users/Lomi/Documents/NN-Developed/Yoke-CodeIntelligence')
BASE = ROOT / 'validation/integration-20261025/platform-repairs'
CLI = BASE / 'control-cli/cli_controller.py'
CLI_SHA = 'd6e306cabbeffaf47b51904758dd0d7c0c4c0ed4dd65f077f1ef07a01ec47582'
NODE = Path('C:/Program Files/nodejs/node.exe')
NPM = NODE.parent / 'node_modules/npm/bin/npm-cli.js'


def text_bytes(path, maximum=64_000_000):
    assert path.suffix.lower() not in ('.exe', '.pdb', '.dll', '.rlib', '.lib', '.obj', '.o')
    before = path.lstat()
    assert before.st_size <= maximum and before.st_nlink == 1
    assert not getattr(before, 'st_file_attributes', 0) & 0x400
    data = path.read_bytes()
    after = path.lstat()
    assert (before.st_dev, before.st_ino, before.st_size, before.st_mtime_ns) == (
        after.st_dev, after.st_ino, after.st_size, after.st_mtime_ns)
    return data


def source_map(names):
    return {name: {'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()}
            for name in names for data in (text_bytes(ROOT / name),)}


def main():
    assert os.name == 'nt' and len(sys.argv) == 1
    data = text_bytes(CLI)
    assert hashlib.sha256(data).hexdigest() == CLI_SHA
    module = types.ModuleType('reviewed_capacity_check_runtime')
    module.__file__ = str(CLI)
    exec(compile(data, str(CLI), 'exec'), module.__dict__)
    g = module.load_base()
    assert g['ROOT'] == ROOT
    g.update(RUNS=BASE / 'runs-ci-boundary-node', OWNER='ci-boundary-node-checks-v1')
    api, extended = g['load_windows_api']()
    out = g['new_output']('node-ci-boundary-r3')
    assert NODE.is_file() and NPM.is_file()
    result = {'completed': False, 'scope': 'exact-clean-test-boundary-diagnostic-source',
              'parentHead': '771833bb5180f3d51e092e64de5fd22c783c920a',
              'nativeImageRead': False, 'nativeImageExecuted': False,
              'releaseQualification': False, 'checks': []}
    mutex = None
    owned = False
    try:
        import _winapi
        mutex_name = 'Local\\YokeValidation-' + hashlib.sha256(str(ROOT).encode()).hexdigest()[:24]
        mutex = api.CreateMutexW(None, False, mutex_name)
        assert mutex
        wait = _winapi.WaitForSingleObject(mutex, 0)
        assert wait in (0, 128)
        owned = True
        result['mutexAbandoned'] = wait == 128
        environment = g['environment_for']('test')
        head = g['git_capture'](api, extended, out, environment, 'git-head-before', ['rev-parse', 'HEAD']).decode('ascii').strip()
        status = g['git_capture'](api, extended, out, environment, 'git-status-before', ['status', '--porcelain=v1', '-z', '--untracked-files=no'])
        assert head == result['parentHead'] and not status
        listing = g['git_capture'](api, extended, out, environment, 'git-files-before', ['ls-files', '-z']).decode('utf-8').split('\0')
        names = sorted(name for name in listing if name and (
            name.startswith(('native/', 'scripts/', '.cargo/', 'src/', 'test/', 'tests/', 'bin/', 'extension/'))
            or name == '.github/workflows/rust.yml'
            or Path(name).suffix in ('.json', '.jsonc', '.ts', '.js', '.mjs', '.cjs') and '/' not in name
            or Path(name).name in ('Cargo.toml', 'Cargo.lock', 'rust-toolchain', 'rust-toolchain.toml')))
        sources = source_map(names)
        controls = g['controls_map']()
        driver_hash = hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
        g['write_json'](out, 'source-before.json', sources)
        g['write_json'](out, 'control-before.json', controls)
        environment = g['environment_for']('test')
        commands = (
            ('typecheck', [str(NODE), str(NPM), 'run', 'typecheck']),
            ('npm-test', [str(NODE), str(NPM), 'test']),
        )
        for label, argv in commands:
            job = g['run_owned'](api, extended, out, label, argv, environment, 180)
            assert job['completed'] and job['exitCode'] == 0, label
            assert job['activeBeforeCleanup'] == 0 and job['activeAfterCleanup'] == 0
            assert job['treeExitedBeforeCleanup'] and job['rootExitedAfterCleanup']
            assert job['streamEOF'] == [True, True] and job['readerErrors'] == [False, False]
            assert job['cleanupUnproven'] is False
            if label == 'npm-test':
                stdout = text_bytes(out / (label + '.stdout'), 16_000_000)
                report = stdout.decode('utf-8', 'strict')
                pairs = re.findall(r'(?m)^(?:# |\u2139 )(tests|suites|pass|fail|cancelled|skipped|todo) ([0-9]+)\r?$', report)
                assert len(pairs) == len(dict(pairs)) == 7
                assert {key: int(value) for key, value in pairs} == {
                    'tests': 263, 'suites': 0, 'pass': 260, 'fail': 0,
                    'cancelled': 0, 'skipped': 3, 'todo': 0}
            result['checks'].append({'name': label, 'exitCode': 0, 'naturalTreeExit': True})
        assert g['environment_for']('test') == environment
        final_head = g['git_capture'](api, extended, out, environment, 'git-head-final', ['rev-parse', 'HEAD']).decode('ascii').strip()
        final_status = g['git_capture'](api, extended, out, environment, 'git-status-final', ['status', '--porcelain=v1', '-z', '--untracked-files=no'])
        assert final_head == head and not final_status
        after = source_map(names)
        after_controls = g['controls_map']()
        assert sources == after and controls == after_controls
        assert hashlib.sha256(Path(__file__).read_bytes()).hexdigest() == driver_hash
        g['write_json'](out, 'source-after.json', after)
        g['write_json'](out, 'control-after.json', after_controls)
        result.update(completed=True, sourceFiles=len(names), sourcesUnchanged=True,
                      controlsUnchanged=True, driverSha256=driver_hash,
                      npmTestsPassed=260, npmTestsSkipped=3, cleanHeadVerified=True)
    except BaseException as error:
        result.update(completed=False, errorType=type(error).__name__)
    finally:
        if owned and not api.ReleaseMutex(mutex):
            result.update(completed=False, mutexReleaseFailed=True)
        if mutex and not api.CloseHandle(mutex):
            result.update(completed=False, mutexCloseFailed=True)
        g['write_json'](out, 'result.json', result)
    print(json.dumps(result), flush=True)
    return 0 if result['completed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
