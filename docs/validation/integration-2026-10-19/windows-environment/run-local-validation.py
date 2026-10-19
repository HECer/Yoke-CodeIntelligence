from pathlib import Path
import os, subprocess, json, hashlib, time, datetime, re

root = Path.cwd()
out = root / 'validation/integration-20261018/windows-environment-r1'
out.mkdir(exist_ok=False)
env = os.environ.copy()
cargo = str(Path.home() / '.cargo/bin/cargo.exe')
env['PATH'] = str(Path(cargo).parent) + os.pathsep + env['PATH']

def save(name, value):
    (out / name).write_text(json.dumps(value, indent=2, ensure_ascii=False) + '\n', encoding='utf-8')

def manifest():
    names = subprocess.check_output(['git', 'ls-files', '-c', '-o', '--exclude-standard', '-z']).decode().split('\0')
    return {n: hashlib.sha256((root/n).read_bytes()).hexdigest() for n in sorted(set(names)) if n and (root/n).is_file() and (n.startswith(('native/src/', 'native/tests/', 'src/', 'test/', 'workers/', 'scripts/')) or n in ['Cargo.toml', 'Cargo.lock', 'native/Cargo.toml', 'package.json', 'package-lock.json', 'tsconfig.json'])}

before = manifest()
save('source-before.json', before)
receipts = []

def run(label, cmd, child_env=env):
    print(json.dumps({'gate': label, 'state': 'running'}), flush=True)
    start = time.monotonic()
    with (out/(label+'.log')).open('wb') as log:
        result = subprocess.run(cmd, env=child_env, stdout=log, stderr=subprocess.STDOUT, timeout=600)
    log = (out/(label+'.log')).read_text(encoding='utf-8', errors='replace')
    groups = [dict(zip(['passed', 'failed', 'ignored'], map(int, m))) for m in re.findall(r'test result: \w+\. (\d+) passed; (\d+) failed; (\d+) ignored;', log)]
    receipt = {'gate': label, 'command': cmd, 'exitCode': result.returncode, 'elapsedSeconds': round(time.monotonic()-start, 3), 'recordedAt': datetime.datetime.now(datetime.timezone.utc).isoformat(), 'resultGroups': groups}
    receipts.append(receipt)
    save('receipts.json', receipts)
    print(json.dumps(receipt), flush=True)

run('format', [cargo, 'fmt', '--all', '--check'])
run('process-and-repl', [cargo, 'test', '--locked', '-j2', '--test', 'process', '--test', 'agent_repl', '--', '--test-threads=2'])
alias = json.loads((root/'validation/integration-20261018/windows-ci-diagnosis/temp-alias.json').read_text(encoding='utf-8'))
assert alias['differs'] and Path(alias['short']).is_dir()
short_env = env.copy()
short_env['TEMP'] = alias['short']
short_env['TMP'] = alias['short']
save('short-temp.json', alias)
run('short-temp-process-and-repl', [cargo, 'test', '--locked', '-j2', '--test', 'process', '--test', 'agent_repl', '--', '--test-threads=2'], short_env)
run('affected-runtime-contracts', [cargo, 'test', '--locked', '-j2', '--lib', '--test', 'runtime', '--test', 'debug_adapter', '--test', 'lsp', '--test', 'lsp_lifecycle', '--test', 'lsp_source_execution', '--test', 'media', '--', '--test-threads=2'])
run('clippy', [cargo, 'clippy', '--workspace', '--all-targets', '--locked', '--keep-going', '-j2', '--', '-D', 'warnings'])
run('typescript-typecheck', ['npm.cmd', 'run', 'typecheck'])
run('typescript-tests', ['npm.cmd', 'test'])
after = manifest()
save('source-after.json', after)
summary = {'allChecksPassed': all(r['exitCode'] == 0 for r in receipts), 'sourceUnchanged': before == after, 'checks': receipts, 'scope': 'Local Windows environment repair; hosted shell deadline cause and platform acceptance require fresh remote validation. Short-TEMP groups overlap normal-TEMP groups.'}
save('summary.json', summary)
raise SystemExit(not (summary['allChecksPassed'] and summary['sourceUnchanged']))
