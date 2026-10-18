"""Execute all five unchanged reviewed gates on the integrated Meaning join."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

root=Path('G:/NN-Developed/Dev-Tools/Yoke-CodeIntelligence/Code-Intelligence')
base=Path(__file__).resolve().parent
runner=root/'target/parity-validation-20261015/acceptance_wave8.py'
assert hashlib.sha256(runner.read_bytes()).hexdigest()=='412b796dff54e94a5ba1076204c28646f85dc1a769a4e7afa0428416b07f6a6b'
tag='meaning-join-windows-20261017-r1'
output=base/'acceptance'/tag
output.mkdir(parents=True,exist_ok=False)
(output/'launcher.py').write_bytes(Path(__file__).read_bytes())
environment=os.environ.copy()
public_environment=dict(CARGO_HOME='E:/Yoke-Build-Cache/cargo-home-yoke-20261017',CARGO_NET_OFFLINE='true',RTK_SUPPRESS_HOOK_WARNING='1')
environment.update(public_environment)
(output/'environment.json').write_text(json.dumps(public_environment,indent=2)+'\n',encoding='utf-8')
for phase in ['format','clippy','native','python','typescript','finalize']:
    command=[sys.executable,'-B','-X','utf8',str(runner),phase,'--tag',tag]
    if phase!='finalize':
        command+=['--temp','E:/Yoke-Test-Scratch/'+tag+'-'+phase]
        if phase=='native':command+=['--acl-temp','C:/Users/HEC_e/AppData/Local/Temp/yoke-'+tag+'-native']
    print(json.dumps(dict(running=phase,tag=tag)),flush=True)
    with (output/(phase+'.log')).open('xb') as stream:
        status=subprocess.run(['rtk','proxy',*command],cwd=root,env=environment,stdout=stream,stderr=subprocess.STDOUT).returncode
    if status!=0:
        print((output/(phase+'.log')).read_text(encoding='utf-8')[-6500:],flush=True);sys.exit(status)
    proof=root/'target/parity-validation-20261015'/tag
    value=json.loads((proof/'summary.json' if phase=='finalize' else proof/phase/'receipt.json').read_bytes())
    print(json.dumps({key:value[key] for key in ['accepted','sourceManifestSha256','sourceFiles','passed','failed','ignored','groups','phaseReceipts'] if key in value}),flush=True)
