from pathlib import Path
import subprocess,json,hashlib,time
work=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');root=work.parent/'Yoke-CodeIntelligence';base=root/'validation/integration-20261018/neo4j-publish';out=base/'final-source-r2';out.mkdir(exist_ok=False);sha=lambda b:hashlib.sha256(b).hexdigest()
def inventory():
 names=sorted(set(subprocess.check_output(['git','ls-files','-c','-o','--exclude-standard','-z','native','Cargo.toml','Cargo.lock','.gitattributes'],cwd=work).decode().split('\0')))
 return {n:sha((work/n).read_bytes()) for n in names if n and (work/n).is_file()}
old=json.loads((base/'final/source-before.json').read_bytes());before=inventory();changed=[n for n in before if before[n]!=old.get(n)];path='native/tests/neo4j_publish_cli.rs';assert changed==[path] and old.keys()==before.keys()
previous=(base/'final/source'/path).read_bytes();expected=previous.replace(b'process::{Command, Output, Stdio},',b'process::{Command, Output},').replace(b'use std::{',b'#[cfg(windows)]\nuse std::process::Stdio;\nuse std::{',1);assert (work/path).read_bytes()==expected
binary=root/'target/debug/yoke.exe';binary_before=sha(binary.read_bytes());assert binary_before==json.loads((base/'native-final-r1/frozen-inputs.json').read_bytes())['binarySha256']
(out/'source-before.json').write_text(json.dumps(before,indent=2)+'\n');(out/'neo4j_publish_cli.rs').write_bytes(expected)
cargo=str(Path.home()/'.cargo/bin/cargo.exe');common=['--workspace','--locked','-j','2','--target-dir',str(root/'target')];commands=[('clippy',[cargo,'clippy',*common,'--all-targets','--','-D','warnings']),('format',[cargo,'fmt','--all','--','--check']),('cli',[cargo,'test',*common,'--test','neo4j_publish_cli','--','--test-threads=1'])];results=[]
for name,command in commands:
 start=time.time();print('START FINAL SOURCE',name,flush=True)
 with (out/(name+'.log')).open('wb') as log:p=subprocess.run(command,cwd=work,stdout=log,stderr=subprocess.STDOUT)
 results.append({'name':name,'command':command,'exitCode':p.returncode,'seconds':time.time()-start})
after=inventory();(out/'source-after.json').write_text(json.dumps(after,indent=2)+'\n');result={'runs':results,'allChecksPassed':all(r['exitCode']==0 for r in results),'sourceUnchanged':before==after,'inputs':len(before),'bridge':{'path':path,'priorSha256':old[path],'currentSha256':before[path],'reason':'Windows-only Stdio import is cfg-gated for non-Windows compiler hygiene; no runtime or test behavior change on Windows'},'productionBinarySha256':binary_before,'productionBinaryUnchanged':sha(binary.read_bytes())==binary_before};(out/'result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result),flush=True);assert result['allChecksPassed'] and result['sourceUnchanged'] and result['productionBinaryUnchanged']
