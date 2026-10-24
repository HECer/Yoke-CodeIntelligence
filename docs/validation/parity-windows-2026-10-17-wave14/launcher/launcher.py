"""Run the unchanged reviewed gates on the integrated PDF mixed-view source freeze."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path('G:/NN-Developed/Dev-Tools/Yoke-CodeIntelligence/Code-Intelligence')
BASE = Path(__file__).resolve().parent
RUNNER = ROOT/'target/parity-validation-20261015/acceptance_wave11_c_durable.py'
assert hashlib.sha256(RUNNER.read_bytes()).hexdigest() == '5b1dbf5179f274d57fc0217108867d90d9a62ae233ca551496e89d386df81a00'
TAG = 'pdf-mixed-view-windows-20261017-r1'
output = BASE/'acceptance'/TAG
output.mkdir(parents=True, exist_ok=False)
(output/'launcher.py').write_bytes(Path(__file__).read_bytes())
environment = os.environ.copy()
public_environment = dict(CARGO_HOME='E:/Yoke-Build-Cache/cargo-home-yoke-20261017',
    CARGO_NET_OFFLINE='true', RTK_SUPPRESS_HOOK_WARNING='1')
environment.update(public_environment)
(output/'environment.json').write_text(json.dumps(public_environment, indent=2)+'\n', encoding='utf-8')
for phase in ['format', 'clippy', 'native', 'python', 'typescript', 'finalize']:
    command = [sys.executable, '-B', '-X', 'utf8', str(RUNNER), phase, '--tag', TAG]
    if phase != 'finalize':
        command += ['--temp', 'E:/Yoke-Test-Scratch/'+TAG+'-'+phase]
        if phase == 'native':
            command += ['--acl-temp', 'C:/Users/HEC_e/AppData/Local/Temp/yoke-'+TAG+'-native']
    print(json.dumps(dict(running=phase, tag=TAG)), flush=True)
    with (output/(phase+'.log')).open('xb') as stream:
        status = subprocess.run(['rtk', 'proxy', *command], cwd=ROOT, env=environment,
            stdout=stream, stderr=subprocess.STDOUT).returncode
    if status != 0:
        print((output/(phase+'.log')).read_text(encoding='utf-8')[-6500:], flush=True)
        sys.exit(status)
    bundle = ROOT/'target/parity-validation-20261015'/TAG
    receipt = json.loads((bundle/'summary.json' if phase == 'finalize' else bundle/phase/'receipt.json').read_bytes())
    print(json.dumps({key: receipt[key] for key in ['accepted', 'sourceManifestSha256',
        'sourceFiles', 'passed', 'failed', 'ignored', 'groups', 'phaseReceipts'] if key in receipt}), flush=True)
