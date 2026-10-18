"""Execute only the original recovery regression, without altering source or deadline."""
import datetime
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

base=Path(__file__).resolve().parent
root=Path('G:/NN-Developed/Dev-Tools/Yoke-CodeIntelligence/Code-Intelligence')
bundle=root/'target/parity-validation-20261015/meaning-join-windows-20261017-serial-hosts'
sha=lambda raw:hashlib.sha256(raw).hexdigest()
frozen_raw=(bundle/'source-freeze.json').read_bytes(); frozen=json.loads(frozen_raw)
def manifest():
    names=subprocess.check_output(['git','ls-files','-c','-o','--exclude-standard','-z'],cwd=root).decode().split('\0')
    return {name:sha((root/name).read_bytes()) for name in sorted(set(names)) if name and (root/name).is_file()}
assert manifest()==frozen
inventory_raw=(bundle/'native/library-inventory.json').read_bytes()
inventory=json.loads(inventory_raw)['yoke']; executable=Path(inventory['executable'])
name='host_install::output_recovery_tests::sixteen_root_v2_history_reserves_only_remaining_output_work'
assert name in inventory['registered'] and name not in inventory['ignored']
binary_sha=sha(executable.read_bytes())
temp=Path('E:/Yoke-Test-Scratch/recovery-diagnostic-20261017');temp.mkdir(exist_ok=False)
command=[str(executable),'--exact',name,'--test-threads=1']
log=base/'recovery-exact-verified.log'; started=time.monotonic()
with log.open('xb') as stream:
    status=subprocess.run(command,cwd=root,env=dict(os.environ,TEMP=str(temp),TMP=str(temp)),
        stdout=stream,stderr=subprocess.STDOUT,timeout=180).returncode
assert sha(executable.read_bytes())==binary_sha and manifest()==frozen
receipt=dict(schemaVersion=1,kind='separate_nonadditive_recovery_diagnostic',
    checkedAtUtc=datetime.datetime.now(datetime.timezone.utc).isoformat(),
    runnerSha256=sha(Path(__file__).read_bytes()),command=command,exitCode=status,
    executable=str(executable),executableSha256=binary_sha,libraryInventorySha256=sha(inventory_raw),
    sourceManifestSha256=sha(frozen_raw),sourceChanges=[],tempRoot=str(temp),
    seconds=round(time.monotonic()-started,3),log=log.name,logSha256=sha(log.read_bytes()),
    contributesToAcceptedTotals=False)
(base/'recovery-verified-receipt.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8')
print(json.dumps(receipt),flush=True)
raise SystemExit(status)
