"""CI-only Unix memory API observations, never native product acceptance.

Compile only the fixed owned C fixture; upload JSON/logs, never its binary.
OS failures are explicit data. No limit is weakened or inferred from a setting.
"""
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
MEMORY = 256 * 1024 * 1024
MAX_LOG = 65_536
MIN_FREE = 500_000_000
KEYS = set('schema mode requestedBytes vmObservationAvailable virtualBytesBefore setLimitAttempted setLimitSucceeded setLimitErrno limitObservationAvailable observedSoftBytes observedHardBytes exactLimitInstalled allocationProbeAttempted mallocDenied mallocErrno mmapDenied mmapErrno nativeAcceptance'.split())


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def unique(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, 'Duplicate diagnostic field')
        result[key] = value
    return result


def bounded(path, limit):
    require(path.is_file() and not path.is_symlink() and not path.is_junction(),
            'Ordinary diagnostic file required')
    with path.open('rb') as source:
        raw = source.read(limit + 1)
    require(len(raw) <= limit, 'Diagnostic file capacity exceeded')
    return raw


def decode(raw, mode):
    require(len(raw) <= 4096, 'Diagnostic JSON capacity exceeded')
    value = json.loads(raw, object_pairs_hook=unique)
    require(type(value) is dict and set(value) == KEYS, 'Unexpected diagnostic schema')
    require(type(value['schema']) is int and value['schema'] == 1
            and value['mode'] == mode and value['requestedBytes'] == MEMORY,
            'Diagnostic identity differs')
    for key in ['requestedBytes', 'virtualBytesBefore', 'observedSoftBytes', 'observedHardBytes']:
        require(type(value[key]) is int and 0 <= value[key] <= 2**64 - 1, 'Invalid diagnostic integer')
    for key in ['setLimitErrno', 'mallocErrno', 'mmapErrno']:
        require(type(value[key]) is int and 0 <= value[key] <= 65535, 'Invalid diagnostic errno')
    for key in ['vmObservationAvailable', 'setLimitAttempted', 'limitObservationAvailable',
                'exactLimitInstalled', 'allocationProbeAttempted', 'nativeAcceptance']:
        require(type(value[key]) is bool, 'Invalid diagnostic boolean')
    require(value['nativeAcceptance'] is False, 'An OS observation cannot certify native acceptance')
    require(value['setLimitAttempted'] == (mode == 'self'), 'Wrong diagnostic operation')
    require(type(value['setLimitSucceeded']) is bool if mode == 'self' else value['setLimitSucceeded'] is None,
            'Invalid install observation')
    exact = value['limitObservationAvailable'] and value['observedSoftBytes'] == MEMORY and value['observedHardBytes'] == MEMORY
    require(value['exactLimitInstalled'] == exact and value['allocationProbeAttempted'] == exact,
            'Unproved allocation-probe admission')
    for key in ['mallocDenied', 'mmapDenied']:
        require(type(value[key]) is bool if exact else value[key] is None, 'Invalid allocation observation')
    return value


def plan(binary, platform):
    require(binary.is_absolute() and platform in ('linux', 'darwin'), 'Unsupported diagnostic plan')
    direct = [{'name': 'direct', 'argv': [str(binary), 'self'], 'mode': 'self', 'nanoDisabled': False}]
    if platform == 'darwin':
        direct.append({'name': 'direct-no-nano', 'argv': [str(binary), 'self'], 'mode': 'self', 'nanoDisabled': True})
    direct.append({'name': 'shell-inherited', 'argv': ['/bin/sh', '-c',
        'ulimit -S -v 262144 && ulimit -H -v 262144 && exec "$@"', 'owned-memory-probe', str(binary), 'inherited'],
        'mode': 'inherited', 'nanoDisabled': platform == 'darwin'})
    return direct


def retire(process):
    end = time.monotonic() + 2
    refused = False
    try:
        os.killpg(process.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    except OSError:
        refused = True
        try:
            process.kill()
        except OSError:
            pass
    process.wait(timeout=max(0.001, end - time.monotonic()))
    require(not refused, 'Diagnostic process-group termination refused')
    while True:
        try:
            os.killpg(process.pid, 0)
        except ProcessLookupError:
            return
        require(time.monotonic() < end, 'Diagnostic process-group cleanup unproven')
        time.sleep(0.01)


def capture(argv, cwd, env, directory, label, seconds):
    require(re.fullmatch('[a-z][a-z-]{0,31}', label) and 0 < seconds <= 20,
            'Invalid diagnostic capture budget')
    require(min(shutil.disk_usage(cwd).free, shutil.disk_usage(directory).free) >= MIN_FREE,
            'Diagnostic capture disk reserve unavailable')
    paths = [directory / (label + '.stdout'), directory / (label + '.stderr')]
    deadline = time.monotonic() + seconds
    refusal = None
    with paths[0].open('xb') as out, paths[1].open('xb') as err:
        process = subprocess.Popen(argv, cwd=cwd, env=env, stdin=subprocess.DEVNULL,
                                   stdout=out, stderr=err, start_new_session=True)
        try:
            while process.poll() is None:
                if time.monotonic() >= deadline:
                    refusal = 'deadline'
                    break
                if min(shutil.disk_usage(cwd).free, shutil.disk_usage(directory).free) < MIN_FREE:
                    refusal = 'disk-reserve'
                    break
                if any(path.stat().st_size > MAX_LOG for path in paths):
                    refusal = 'output-capacity'
                    break
                time.sleep(0.02)
        finally:
            retire(process)
    raw = []
    for path in paths:
        with path.open('rb') as source:
            raw.append(source.read(MAX_LOG + 1))
    require(all(len(data) <= MAX_LOG for data in raw), 'Diagnostic retained-log capacity exceeded')
    return {'exitCode': process.returncode, 'refusal': refusal, 'groupAbsenceObserved': True,
            'stdout': raw[0], 'stderr': raw[1]}


def main():
    require(os.environ.get('GITHUB_ACTIONS') == 'true' and sys.platform in ('linux', 'darwin'),
            'Host memory diagnostic requires Unix CI')
    revision = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, timeout=5).decode().strip()
    require(re.fullmatch('[0-9a-f]{40}', revision)
            and revision == os.environ.get('GITHUB_SHA'), 'Exact CI checkout required')
    require(shutil.disk_usage(ROOT).free >= MIN_FREE, 'Diagnostic disk reserve unavailable')
    source = ROOT / 'scripts/fixtures/host-memory-probe.c'
    require(source.is_file() and not source.is_symlink() and not source.is_junction()
            and source.stat().st_size <= 16384, 'Owned C fixture unavailable')
    source_bytes = bounded(source, 16384)
    source_hash = hashlib.sha256(source_bytes).hexdigest()
    output = ROOT / 'validation/ci-host-memory'
    require(not output.exists(), 'Diagnostic output must be fresh')
    require(all(not path.is_symlink() and not path.is_junction()
                for path in (output.parent, *output.parent.parents) if path.is_relative_to(ROOT)),
            'Linked diagnostic output ancestor refused')
    output.mkdir(parents=True)
    report = {'schema': 1, 'diagnosticOnly': True, 'nativeAcceptance': False,
              'sourceRevision': revision, 'platform': sys.platform, 'fixtureSha256': source_hash,
              'kernelRelease': os.uname().release, 'architecture': os.uname().machine,
              'requestedBytes': MEMORY, 'trials': [], 'diagnosticComplete': False,
              'nativeYokeImageReadOrExecuted': False,
              'scope': 'Owned C process/API observations, not Yoke or full memory qualification'}
    compiler = next((path for path in [Path('/usr/bin/clang'), Path('/usr/bin/cc')] if path.is_file()), None)
    if compiler is None:
        report['unavailable'] = 'system-compiler'
    else:
        with tempfile.TemporaryDirectory(prefix='yoke-owned-memory-probe-') as temporary:
            private = Path(temporary).resolve(strict=True)
            binary = private / 'probe'
            env = {'PATH': '/usr/bin:/bin', 'HOME': str(private), 'TMPDIR': str(private), 'LC_ALL': 'C'}
            compiled = capture([str(compiler), '-std=c11', '-Wall', '-Wextra', '-Werror', '-O0',
                                str(source), '-o', str(binary)], private, env, output, 'compile', 20)
            report['compiler'] = str(compiler)
            report['compileExit'] = compiled['exitCode']
            if compiled['exitCode'] != 0 or compiled['refusal']:
                report['unavailable'] = 'fixture-compilation'
            else:
                require(binary.is_file() and not binary.is_symlink(), 'Owned probe binary unavailable')
                image = bounded(binary, 1_000_000)
                report['ownedProbeSha256'] = hashlib.sha256(image).hexdigest()
                for trial in plan(binary, sys.platform):
                    selected = dict(env)
                    if trial['nanoDisabled']:
                        selected['MallocNanoZone'] = '0'
                    result = capture(trial['argv'], private, selected, output, trial['name'], 6)
                    row = {key: trial[key] for key in ('name', 'mode', 'nanoDisabled')}
                    row.update({key: result[key] for key in ('exitCode', 'refusal', 'groupAbsenceObserved')})
                    # An inherited shell limit can refuse before exec. Preserve
                    # that exit/log, never manufacture a worker observation.
                    if result['exitCode'] == 0 and result['refusal'] is None:
                        row['observation'] = decode(result['stdout'], trial['mode'])
                    else:
                        row['observation'] = None
                    report['trials'].append(row)
                require(hashlib.sha256(bounded(binary, 1_000_000)).hexdigest() == report['ownedProbeSha256'],
                        'Owned diagnostic binary changed')
                report['diagnosticComplete'] = True
            require(hashlib.sha256(bounded(source, 16384)).hexdigest() == source_hash, 'Diagnostic source changed')
    logs = []
    paths = sorted(output.iterdir())
    require(len(paths) <= 8, 'Diagnostic log cardinality exceeded')
    for path in paths:
        require(re.fullmatch(r'(compile|direct|direct-no-nano|shell-inherited)\.(stdout|stderr)', path.name),
                'Unexpected diagnostic output')
        raw = bounded(path, MAX_LOG)
        logs.append({'name': path.name, 'bytes': len(raw), 'sha256': hashlib.sha256(raw).hexdigest()})
    report['logs'] = logs
    (output / 'result.json').write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')
    print(json.dumps({key: report[key] for key in ('diagnosticOnly', 'nativeAcceptance', 'diagnosticComplete', 'platform')}))
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
