from pathlib import Path
import subprocess,hashlib,json,time,os
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');primary=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');out=primary/'validation/integration-20261018/captured-owners/r2';out.mkdir(parents=True,exist_ok=False)
names=sorted(set(subprocess.check_output(['git','ls-files','-c','-o','--exclude-standard','-z','native','Cargo.toml','Cargo.lock'],cwd=root).decode().split('\0')))
def inventory():return {n:hashlib.sha256((root/n).read_bytes()).hexdigest() for n in names if n and (root/n).is_file()}
before=inventory();(out/'source-before.json').write_text(json.dumps(before,indent=2)+'\n')
for name in ['native/src/git_owners.rs','native/src/git_areas.rs','native/src/git_owners_cli.rs','native/src/main.rs','native/src/git_workflows.rs','native/tests/git_owners.rs','native/tests/support/git_impact_fixture.rs']:
    dest=out/'source'/name;dest.parent.mkdir(parents=True,exist_ok=True);dest.write_bytes((root/name).read_bytes())
cmd=[str(Path.home()/'.cargo/bin/cargo.exe'),'test','--workspace','--locked','-j','2','--target-dir',str(primary/'target'),'--no-fail-fast','--test','git_owners','--test','git_areas','--','--test-threads=1'];env=os.environ.copy();env['PATH']=str(Path.home()/'.cargo/bin')+os.pathsep+env['PATH'];start=time.time();print('START R2',flush=True)
with (out/'native.log').open('wb') as log:p=subprocess.run(cmd,cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT)
after=inventory();(out/'source-after.json').write_text(json.dumps(after,indent=2)+'\n');result={'command':cmd,'exitCode':p.returncode,'seconds':time.time()-start,'sourceUnchanged':before==after,'inputs':len(before)};(out/'result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result),flush=True)
