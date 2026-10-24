from pathlib import Path
import subprocess,json,hashlib,time,shutil
work=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');root=work.parent/'Yoke-CodeIntelligence';out=root/'validation/integration-20261018/falkor-publish/plan-r1';out.mkdir(exist_ok=False)
def inventory():
 names=sorted(set(subprocess.check_output(['git','ls-files','-c','-o','--exclude-standard','-z','native','Cargo.toml','Cargo.lock','.gitattributes'],cwd=work).decode().split('\0')))
 return {n:hashlib.sha256((work/n).read_bytes()).hexdigest() for n in names if n and (work/n).is_file()}
before=inventory();(out/'source-before.json').write_text(json.dumps(before,indent=2)+'\n')
for name in before:
 if 'falkor' in name or name in ['native/src/lib.rs','native/src/neo4j_publish.rs']:
  path=out/'source'/name;path.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(work/name,path)
cargo=str(Path.home()/'.cargo/bin/cargo.exe');common=['--workspace','--locked','-j','2','--target-dir',str(root/'target')];results=[]
commands=[('test',[cargo,'test',*common,'--test','falkor_wire','--test','falkor_publish','--test','neo4j_publish','--','--test-threads=1']),('example',[cargo,'build',*common,'--example','falkor_owned_probe']),('clippy',[cargo,'clippy',*common,'--all-targets','--','-D','warnings'])]
for name,command in commands:
 print('START FALKOR PLAN',name,flush=True);start=time.time()
 with (out/(name+'.log')).open('wb') as log:p=subprocess.run(command,cwd=work,stdout=log,stderr=subprocess.STDOUT)
 results.append({'name':name,'command':command,'exitCode':p.returncode,'seconds':time.time()-start})
 if p.returncode:break
after=inventory();(out/'source-after.json').write_text(json.dumps(after,indent=2)+'\n');result={'runs':results,'sourceUnchanged':before==after,'inputs':len(before)};(out/'result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result),flush=True)
