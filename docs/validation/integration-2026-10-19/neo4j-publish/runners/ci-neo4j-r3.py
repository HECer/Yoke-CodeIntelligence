from pathlib import Path
import subprocess,json,hashlib,time,shutil
work=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');root=work.parent/'Yoke-CodeIntelligence';out=root/'validation/integration-20261018/neo4j-publish/r3';out.mkdir(parents=True,exist_ok=False)
def inventory():
 names=sorted(set(subprocess.check_output(['git','ls-files','-c','-o','--exclude-standard','-z','native','Cargo.toml','Cargo.lock','.gitattributes'],cwd=work).decode().split('\0')))
 return {n:hashlib.sha256((work/n).read_bytes()).hexdigest() for n in names if n and (work/n).is_file()}
before=inventory();(out/'source-before.json').write_text(json.dumps(before,indent=2)+'\n',encoding='utf-8')
for name in ['native/src/neo4j_publish_plan.rs','native/src/neo4j_publish_program.rs','native/src/lib.rs','native/src/neo4j_publish.rs','native/tests/neo4j_publish.rs']:
 target=out/'source'/name;target.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(work/name,target)
command=[str(Path.home()/'.cargo/bin/cargo.exe'),'test','--workspace','--locked','-j','2','--target-dir',str(root/'target'),'--test','neo4j_publish','--','--test-threads=1'];start=time.time();print('START NEO4J PLAN R3',flush=True)
with (out/'native.log').open('wb') as log:p=subprocess.run(command,cwd=work,stdout=log,stderr=subprocess.STDOUT)
after=inventory();(out/'source-after.json').write_text(json.dumps(after,indent=2)+'\n',encoding='utf-8');result={'command':command,'exitCode':p.returncode,'seconds':time.time()-start,'inputs':len(before),'sourceUnchanged':before==after};(out/'result.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8');print(json.dumps(result),flush=True)
