"""Record bounded same-artifact diagnostic executions without changing any gate."""
import datetime
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

BASE = Path(__file__).resolve().parent
ROOT = Path('G:/NN-Developed/Dev-Tools/Yoke-CodeIntelligence/Code-Intelligence')
BUNDLE = ROOT/'target/parity-validation-20261015/meaning-join-windows-20261017-r1'
sha = lambda raw: hashlib.sha256(raw).hexdigest()
frozen_raw = (BUNDLE/'source-freeze.json').read_bytes()
assert sha(frozen_raw) == '47ecb688e6a3beb16f7a638a93b31f3725b05a948440bbcf44715081b2dee4ec'
frozen = json.loads(frozen_raw)


def manifest():
    names = subprocess.check_output(['git', 'ls-files', '-c', '-o', '--exclude-standard', '-z'], cwd=ROOT).decode().split('\0')
    return {name: sha((ROOT/name).read_bytes()) for name in sorted(set(names)) if name and (ROOT/name).is_file()}


assert manifest() == frozen
inventory_raw = (BUNDLE/'native/integration-inventory.json').read_bytes()
inventory = json.loads(inventory_raw)['github']
executable = Path(inventory['executable'])
binary_sha = sha(executable.read_bytes())
assert not inventory['ignored'] and len(inventory['registered']) == 12
results = []
for label, arguments in [
    ('exact', ['--exact', 'explicit_loopback_http_reads_observed_ci_review_and_scope_without_commands', '--test-threads=1']),
    ('suite', ['--test-threads=4']),
]:
    temp = Path('E:/Yoke-Test-Scratch/github-verified-20261017-'+label)
    temp.mkdir(exist_ok=False)
    environment = dict(os.environ, TEMP=str(temp), TMP=str(temp))
    command = [str(executable), *arguments]
    log = BASE/('github-'+label+'-verified.log')
    assert sha(executable.read_bytes()) == binary_sha
    started = time.monotonic()
    with log.open('xb') as stream:
        status = subprocess.run(command, cwd=ROOT, env=environment, timeout=60,
            stdout=stream, stderr=subprocess.STDOUT).returncode
    assert sha(executable.read_bytes()) == binary_sha
    results.append(dict(label=label, command=command, exitCode=status, log=log.name,
        logSha256=sha(log.read_bytes()), seconds=round(time.monotonic()-started, 3),
        tempRoot=str(temp), sourceManifestSha256=sha(frozen_raw), executableSha256=binary_sha))
assert manifest() == frozen
receipt = dict(schemaVersion=1, kind='separate_nonadditive_github_diagnostics',
    checkedAtUtc=datetime.datetime.now(datetime.timezone.utc).isoformat(),
    runnerSha256=sha(Path(__file__).read_bytes()), executable=str(executable), executableSha256=binary_sha,
    integrationInventorySha256=sha(inventory_raw), sourceManifestSha256=sha(frozen_raw), sourceChanges=[],
    contributesToAcceptedTotals=False, results=results)
with (BASE/'github-verified-receipt.json').open('x', encoding='utf-8') as stream:
    json.dump(receipt, stream, indent=2)
    stream.write('\n')
print(json.dumps(receipt))
