"""Qualify the three exact ACL fixtures in a fresh user-owned C: Temp child."""
import hashlib
import json
import os
from pathlib import Path
import runpy
import subprocess

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
runner = ROOT / 'target/parity-validation-20261015/acceptance_wave4.py'
assert hashlib.sha256(runner.read_bytes()).hexdigest() == 'b51bbffbddd07be0aa6ef019fc950d772c628d77b3567f646d7a837726aa3d15'
contract = runpy.run_path(str(runner))
cache = ROOT / 'target/parity-validation-20261015/cache-origin-admission-v4/cache_origin.py'
assert hashlib.sha256(cache.read_bytes()).hexdigest() == 'a24fd7ab1c40c0c815117878f16be97760076a6efffd56479386cc363d4865db'
runpy.run_path(str(cache))['no_compiler_processes']()
before = contract['source_manifest']()
assert hashlib.sha256(contract['encoded'](before)).hexdigest() == 'd9b2112398d06212d977212518c2377f6ed2d8df89c359a9925b1b7999bbfa31'
temp = Path('C:/Users/HEC_e/AppData/Local/Temp/yoke-r5b-acl-qualification-20261016')
assert not temp.exists() and temp.parent.resolve(strict=True) == temp.parent
temp.mkdir()
output = HERE / 'user-temp-acl-qualification-v1'
output.mkdir(exist_ok=False)
results = []
for index, name in enumerate(contract['ACL_TESTS']):
    command = ['cargo', 'test', '--workspace', '--locked', '-j2', '--lib', name, '--', '--exact', '--test-threads=4']
    log = output / f'acl-{index}.log'
    with log.open('xb') as stream:
        status = subprocess.run(['rtk', 'proxy', *command], cwd=ROOT, env=dict(os.environ, TEMP=str(temp), TMP=str(temp)), stdout=stream, stderr=subprocess.STDOUT).returncode
    raw = log.read_bytes()
    results.append(dict(command=command, exitCode=status, logSha256=hashlib.sha256(raw).hexdigest(),
                        exactPass=b'test result: ok. 1 passed; 0 failed; 0 ignored;' in raw))
after = contract['source_manifest']()
assert after == before
record = dict(scope='three_exact_acl_fixture_qualification_not_full_acceptance', tempRoot=str(temp),
              sourceManifestSha256=hashlib.sha256(contract['encoded'](before)).hexdigest(),
              sourceChanges=[], results=results, accepted=all(row['exitCode'] == 0 and row['exactPass'] for row in results))
(output / 'receipt.json').write_text(json.dumps(record, indent=2) + '\n', encoding='utf-8')
print(json.dumps(record))
if not record['accepted']:
    print('\n'.join(path.read_text(encoding='utf-8', errors='replace') for path in output.glob('*.log')))
raise SystemExit(not record['accepted'])
