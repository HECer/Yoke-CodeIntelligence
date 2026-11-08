from pathlib import Path
import subprocess,json,hashlib,time,shutil
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');primary=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');out=primary/'validation/integration-20261018/postgres-catalog/red';out.mkdir(parents=True,exist_ok=False)
files=sorted(set(subprocess.check_output(['git','ls-files','-c','-o','--exclude-standard','-z','native','Cargo.toml','Cargo.lock','.gitattributes'],cwd=root).decode().split('\0')))
def inventory():return {n:hashlib.sha256((root/n).read_bytes()).hexdigest() for n in files if n and (root/n).is_file()}
before=inventory();(out/'source-before.json').write_text(json.dumps(before,indent=2)+'\n')
for name in ['native/src/postgres_catalog.rs','native/src/postgres_catalog_query.sql','native/src/lib.rs','native/tests/postgres_catalog.rs','native/tests/fixtures/postgres-catalog-response.jsonl','.gitattributes']:
    target=out/'source'/name;target.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(root/name,target)
cmd=[str(Path.home()/'.cargo/bin/cargo.exe'),'test','--workspace','--locked','-j','2','--target-dir',str(primary/'target'),'--test','postgres_catalog','--','--test-threads=1'];start=time.time();print('START RED',flush=True)
with (out/'native.log').open('wb') as log:p=subprocess.run(cmd,cwd=root,stdout=log,stderr=subprocess.STDOUT)
after=inventory();(out/'source-after.json').write_text(json.dumps(after,indent=2)+'\n');result={'command':cmd,'exitCode':p.returncode,'seconds':time.time()-start,'sourceUnchanged':before==after,'inputs':len(before)};(out/'result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result),flush=True)
