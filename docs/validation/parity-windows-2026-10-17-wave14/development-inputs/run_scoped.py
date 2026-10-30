"""Execute isolated PDF checks with immutable logs and complete source maps."""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path('E:/Yoke-Worktrees/pdf-mixed-view-20261017')
BASE = Path(__file__).resolve().parent

def digest(raw):
    return hashlib.sha256(raw).hexdigest()

def manifest():
    names = subprocess.check_output(
        ['git', 'ls-files', '-c', '-o', '--exclude-standard', '-z'], cwd=ROOT
    ).decode('utf-8').split('\0')
    return {name: digest((ROOT/name).read_bytes()) for name in sorted(set(names))
            if name and (ROOT/name).is_file()}

def encode(value):
    return (json.dumps(value, indent=2, sort_keys=True)+'\n').encode('utf-8')

label, attempt = sys.argv[1:]
assert re.fullmatch(r'r[0-9]+', attempt)
commands = {
    'pdf': ['cargo', 'test', '--workspace', '--locked', '-j', '1', '--test',
            'mixed_pdf_view', '--', '--test-threads=2'],
    'regressions': ['cargo', 'test', '--workspace', '--locked', '-j', '1',
        *[argument for name in ['media', 'mixed_view_cli', 'document_cli',
            'document_explorer', 'document_graph', 'document_graph_review',
            'office', 'office_bounds', 'view_comparison_cli', 'feedback_cli',
            'process', 'deadlines', 'store_cancellation', 'retention']
          for argument in ['--test', name]], '--', '--test-threads=2'],
    'binary': ['cargo', 'build', '--workspace', '--locked', '-j', '1', '--bin', 'yoke'],
    'format': ['cargo', 'fmt', '--all', '--', '--check'],
    'clippy': ['cargo', 'clippy', '--workspace', '--all-targets', '--locked',
               '-j', '1', '--', '-D', 'warnings'],
    'typecheck': ['npm.cmd', 'run', 'typecheck'],
    'typescript': ['npm.cmd', 'test'],
}
command = commands[label]
public = dict(CARGO_TARGET_DIR=str(BASE),
              CARGO_HOME='E:/Yoke-Build-Cache/cargo-home-yoke-20261017',
              CARGO_NET_OFFLINE='true', TEMP=str(BASE/'temp'),
              TMP=str(BASE/'temp'), RTK_SUPPRESS_HOOK_WARNING='1')
environment = os.environ.copy()
environment.update(public)
assert (BASE/'temp').is_dir()
directory = BASE/'development'
directory.mkdir(exist_ok=True)
stem = directory/(label+'-'+attempt)
assert not any(path.exists() for path in [stem.with_suffix('.log'),
    stem.with_suffix('.json'), directory/(stem.name+'-before.json'),
    directory/(stem.name+'-after.json')])
before = manifest()
before_raw = encode(before)
with (directory/(stem.name+'-before.json')).open('xb') as stream:
    stream.write(before_raw)
with stem.with_suffix('.log').open('xb') as stream:
    result = subprocess.run(['rtk', 'proxy', *command], cwd=ROOT, env=environment,
                            stdout=stream, stderr=subprocess.STDOUT)
after = manifest()
after_raw = encode(after)
with (directory/(stem.name+'-after.json')).open('xb') as stream:
    stream.write(after_raw)
changes = sorted(name for name in set(before)|set(after)
                 if before.get(name) != after.get(name))
receipt = dict(label=label, command=command, exitCode=result.returncode,
    environment=public, runnerSha256=digest(Path(__file__).read_bytes()),
    sourceBefore=digest(before_raw), sourceAfter=digest(after_raw),
    sourceChanges=changes, log=stem.with_suffix('.log').name,
    logSha256=digest(stem.with_suffix('.log').read_bytes()),
    interpretation='Isolated scoped development only; no integrated acceptance or universal parity claim.')
with stem.with_suffix('.json').open('x', encoding='utf-8') as stream:
    stream.write(json.dumps(receipt, indent=2)+'\n')
print(json.dumps(receipt), flush=True)
if result.returncode:
    print(stem.with_suffix('.log').read_text(encoding='utf-8')[-12000:], flush=True)
sys.exit(result.returncode or (1 if changes else 0))
