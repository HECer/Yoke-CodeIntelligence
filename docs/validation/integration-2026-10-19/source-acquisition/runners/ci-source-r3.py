from pathlib import Path
import subprocess,json,hashlib,time,shutil
work=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');out=root/'validation/integration-20261018/source-acquisition/r3';out.mkdir(parents=True,exist_ok=False)
files=sorted(set(subprocess.check_output(['git','ls-files','-c','-o','--exclude-standard','-z','native','Cargo.toml','Cargo.lock','.gitattributes'],cwd=work).decode().split('\0')))
def inventory():return {n:hashlib.sha256((work/n).read_bytes()).hexdigest() for n in files if n and (work/n).is_file()}
before=inventory();(out/'source-before.json').write_text(json.dumps(before,indent=2)+'\n',encoding='utf-8')
for name in ['native/src/lib.rs','native/src/source_acquisition_cli.rs','native/src/main.rs','native/Cargo.toml','Cargo.lock','native/tests/source_acquisition_http.rs','native/tests/source_acquisition_cli.rs','native/docs/source-acquisition.md','native/src/source_acquisition_http.rs','native/src/source_acquisition.rs','native/tests/source_acquisition.rs']:
    path=out/'source'/name;path.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(work/name,path)
command=[str(Path.home()/'.cargo/bin/cargo.exe'),'test','--workspace','--locked','-j','2','--target-dir',str(root/'target'),'--no-fail-fast','--test','source_acquisition','--test','source_acquisition_http','--test','source_acquisition_cli','--','--test-threads=1'];start=time.time();print('START SOURCE R3',flush=True)
with (out/'native.log').open('wb') as log:p=subprocess.run(command,cwd=work,stdout=log,stderr=subprocess.STDOUT)
after=inventory();(out/'source-after.json').write_text(json.dumps(after,indent=2)+'\n',encoding='utf-8');result={'command':command,'exitCode':p.returncode,'seconds':time.time()-start,'inputs':len(before),'sourceUnchanged':before==after};(out/'result.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8');print(json.dumps(result))
