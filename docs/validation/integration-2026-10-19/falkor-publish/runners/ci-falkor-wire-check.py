from pathlib import Path
import subprocess,json,hashlib,time,shutil
work=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');root=work.parent/'Yoke-CodeIntelligence';out=root/'validation/integration-20261018/falkor-publish/wire-r1';out.mkdir(exist_ok=False)
def inventory():
 names=sorted(set(subprocess.check_output(['git','ls-files','-c','-o','--exclude-standard','-z','native','Cargo.toml','Cargo.lock','.gitattributes'],cwd=work).decode().split('\0')))
 return {n:hashlib.sha256((work/n).read_bytes()).hexdigest() for n in names if n and (work/n).is_file()}
before=inventory();(out/'source-before.json').write_text(json.dumps(before,indent=2)+'\n')
for name in ['native/src/lib.rs','native/src/falkor_wire.rs','native/tests/falkor_wire.rs']:
 path=out/'source'/name;path.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(work/name,path)
cargo=str(Path.home()/'.cargo/bin/cargo.exe');common=['--workspace','--locked','-j','2','--target-dir',str(root/'target')];results=[]
for name,command in [('test',[cargo,'test',*common,'--test','falkor_wire','--','--test-threads=1']),('clippy',[cargo,'clippy',*common,'--all-targets','--','-D','warnings'])]:
 print('START FALKOR WIRE',name,flush=True);start=time.time()
 with (out/(name+'.log')).open('wb') as log:p=subprocess.run(command,cwd=work,stdout=log,stderr=subprocess.STDOUT)
 results.append({'name':name,'command':command,'exitCode':p.returncode,'seconds':time.time()-start})
 if p.returncode:break
after=inventory();(out/'source-after.json').write_text(json.dumps(after,indent=2)+'\n');result={'runs':results,'sourceUnchanged':before==after,'inputs':len(before)};(out/'result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result),flush=True)
