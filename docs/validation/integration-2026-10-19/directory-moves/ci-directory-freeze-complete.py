from pathlib import Path
import json,hashlib,shutil
root=Path.cwd();folder=root/'validation/integration-20261018/directory-moves-final';summary=json.loads((folder/'summary.json').read_bytes());assert summary['sourceUnchanged'] and summary['allTestableNativeTargetsRan'];frozen=json.loads((folder/'source-before.json').read_bytes());assert frozen==json.loads((folder/'source-after.json').read_bytes())
for n,h in frozen.items():assert hashlib.sha256((root/n).read_bytes()).hexdigest()==h,n
expected=json.loads((folder/'tested-native-binary.json').read_bytes())['sha256'];binary=root/'target/debug/yoke.exe';assert hashlib.sha256(binary.read_bytes()).hexdigest()==expected;target=root/'validation/integration-20261018/directory-eaf9d59-yoke.exe'
if target.exists():assert hashlib.sha256(target.read_bytes()).hexdigest()==expected
else:shutil.copyfile(binary,target)
out=root/'validation/integration-20261018/directory-moves-development/pre-repair-full-source'
for n in ['native/src/agent_repl.rs','native/tests/facade_directory_move.rs']:
 p=out/n;p.parent.mkdir(parents=True,exist_ok=True)
 if p.exists():assert p.read_bytes()==(root/n).read_bytes()
 else:shutil.copyfile(root/n,p)
(out/'manifest.json').write_bytes((json.dumps({'fullInputCommit':'eaf9d5995c2370b4873bbd752f1479b0923637a0','sourceSha256':{n:frozen[n] for n in ['native/src/agent_repl.rs','native/tests/facade_directory_move.rs']},'testedNativeBinarySha256':expected,'ignoredBinaryCopy':str(target.relative_to(root))},indent=2)+'\n').encode());print(json.dumps({'frozenInputsVerified':len(frozen),'testedNativeBinarySha256':expected,'backupPreserved':True,'wholeNativeTotals':summary['wholeNativeTotals']}))
