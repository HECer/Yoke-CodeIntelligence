"""Apply only the reviewed, checked Meaning join delta to the accepted parent."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import uuid

base=Path(__file__).resolve().parent
root=Path('G:/NN-Developed/Dev-Tools/Yoke-CodeIntelligence/Code-Intelligence')
aux=Path('E:/Yoke-Worktrees/meaning-join-20261017')
wave=root/'docs/validation/parity-windows-2026-10-17-wave10'
expected_index='4de65fbe19220356f19fcdddb079542f9475490dcb790265d38a565a2b985ee3'
sha=lambda raw:hashlib.sha256(raw).hexdigest()
read=lambda path:json.loads(path.read_bytes())
def require(ok,message):
    if not ok:raise RuntimeError(message)
def encoded(value):return (json.dumps(value,indent=2,sort_keys=True)+'\n').encode()
def manifest(path):
    raw=subprocess.check_output(['git','ls-files','-c','-o','--exclude-standard','-z'],cwd=path)
    return {name:sha((path/name).read_bytes()) for name in sorted(set(raw.decode().split('\0')))
        if name and (path/name).is_file()}
names=['native/src/lib.rs','native/src/meaning_cli.rs','native/docs/meaning-cli.md',
    'native/src/meaning_join.rs','native/src/meaning_join_query.rs','native/src/meaning_join_query_tests.rs',
    'native/tests/meaning_join_cli.rs','native/docs/meaning-join.md']
require(sha((wave/'index.json').read_bytes())==expected_index,'Parent checkpoint index changed')
environment=os.environ.copy();environment['RTK_SUPPRESS_HOOK_WARNING']='1'
subprocess.run([sys.executable,'-B','-X','utf8',str(base.parent/'declaration-moves-20261017/verify_checkpoint.py'),expected_index],
    cwd=root,env=environment,check=True)
freeze=read(root/'target/parity-validation-20261015/declaration-moves-windows-20261017-r2/source-freeze.json')
candidate=manifest(aux)
changed=sorted(name for name in set(candidate)|set(freeze) if candidate.get(name)!=freeze.get(name))
require(changed==sorted(names),'Auxiliary delta exceeds the eight reviewed paths: '+repr(changed))
before=manifest(root)
updates={name:(aux/name).read_bytes() for name in names}
require(all(sha(raw)==candidate[name] for name,raw in updates.items()),'Candidate buffers changed after admission')
originals={name:(root/name).read_bytes() if (root/name).exists() else None for name in names}
proposal=dict(parentIndexSha256=expected_index,paths={name:dict(before=sha(originals[name]) if originals[name] is not None else None,
    after=sha(updates[name])) for name in names})
if '--validate-only' in sys.argv:
    print(json.dumps(dict(ready=True,**proposal)));sys.exit(0)
backup=base/'integration'
backup.mkdir(exist_ok=False)
(backup/'proposal.json').write_bytes(encoded(proposal))
require(manifest(aux)==candidate,'Auxiliary source inventory changed before integration writes')
for name,raw in originals.items():
    if raw is not None:
        path=backup/'original'/name;path.parent.mkdir(parents=True,exist_ok=True);path.write_bytes(raw)
for name,raw in updates.items():
    path=root/name
    require((path.read_bytes() if path.exists() else None)==originals[name],'Parent preimage changed: '+name)
    require((aux/name).read_bytes()==raw,'Reviewed source changed: '+name)
    stage=path.with_name('.'+path.name+'.'+uuid.uuid4().hex+'.tmp')
    with stage.open('xb') as stream:stream.write(raw)
    try:
        require((path.read_bytes() if path.exists() else None)==originals[name],'Parent preimage changed before replace: '+name)
        os.replace(stage,path)
    finally:
        if stage.exists():stage.unlink()
expected=dict(before)
expected.update({name:sha(raw) for name,raw in updates.items()})
require(manifest(root)==expected,'Unexpected parent changes during integration')
(backup/'receipt.json').write_bytes(encoded(dict(accepted=True,**proposal)))
print(json.dumps(dict(integrated=True,paths=len(names),sourceFiles=len(expected),
    sourceManifestSha256=sha(encoded(expected)),receipt=str(backup/'receipt.json'))))
