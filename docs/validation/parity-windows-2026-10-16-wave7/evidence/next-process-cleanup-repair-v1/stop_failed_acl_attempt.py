"""Stop only the exact owned native attempt whose three ACL fixtures failed."""
import datetime
import hashlib
import json
from pathlib import Path
import subprocess

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
inspection = json.loads((HERE / 'acl-attempt-inspection-v1.json').read_bytes())
assert len(inspection['Runner']) == 1
expected = inspection['Runner'][0]
assert expected['ProcessId'] == 86992
assert 'acceptance_wave4.py native --tag affected-r5b-process-20261016-full ' in expected['CommandLine']
query = "Get-CimInstance Win32_Process -Filter 'ProcessId=86992' | Select-Object ProcessId,ParentProcessId,ExecutablePath,CommandLine | ConvertTo-Json -Compress"
actual = json.loads(subprocess.check_output(['powershell.exe', '-NoProfile', '-Command', query]).decode('utf-8-sig'))
assert actual == expected, 'Owned runner process identity changed'
bundle = ROOT / 'target/parity-validation-20261015/affected-r5b-process-20261016-full'
logs = {f'library-c-acl-{n}.log': hashlib.sha256((bundle / 'native' / f'library-c-acl-{n}.log').read_bytes()).hexdigest() for n in range(3)}
assert all('test result: FAILED. 0 passed; 1 failed;' in (bundle / 'native' / name).read_text(encoding='utf-8') for name in logs)
result = subprocess.run(['C:/Windows/System32/taskkill.exe', '/PID', str(expected['ProcessId']), '/T', '/F'], capture_output=True)
record = dict(checkedAtUtc=datetime.datetime.now(datetime.timezone.utc).isoformat(),
              accepted=False, nativePhaseAborted=True, fullTag='affected-r5b-process-20261016-full',
              reason='The three explicit ACL fixtures failed setup under the chosen C root ACL; restart with a fresh user Temp child. No product test assertion or source changed.',
              ownedRunner=expected, taskkillExitCode=result.returncode,
              failedFixtureLogSha256=logs, nativeFinalReceiptExists=(bundle / 'native/receipt.json').exists(),
              sourceChanges=False, sourceManifestSha256='d9b2112398d06212d977212518c2377f6ed2d8df89c359a9925b1b7999bbfa31')
(HERE / 'aborted-native-control-v1.json').open('x', encoding='utf-8').write(json.dumps(record, indent=2) + '\n')
print(json.dumps(record))
assert result.returncode == 0
