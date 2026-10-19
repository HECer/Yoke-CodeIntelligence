from pathlib import Path
import subprocess,os,json,time,hashlib,shutil
root=Path('C:/Users/Lomi/Documents/NN-Developed/Yoke-CodeIntelligence');work=Path.cwd();out=root/'validation/integration-20261018/propagated-delete-development/binding-red-r7';out.mkdir(parents=True,exist_ok=False)
files=sorted(p for p in work.rglob('*') if p.is_file() and (p.relative_to(work).as_posix().startswith(('native/src/','native/tests/')) or p.relative_to(work).as_posix() in ['Cargo.toml','Cargo.lock','native/Cargo.toml']))
def hashes():return {p.relative_to(work).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in files}
before=hashes()
for p in files:
 dst=out/'source'/p.relative_to(work);dst.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(p,dst)
cargo=str(Path.home()/'.cargo/bin/cargo.exe');env=os.environ.copy();env['PATH']=str(Path(cargo).parent)+os.pathsep+env['PATH'];cmd=[cargo,'test','--locked','-j1','--target-dir',str(root/'target'),'--test','propagated_delete_contract','reported_non_reference_tokens_cannot_seed_deletion_of_an_unrelated_unused_function','--','--exact','--nocapture','--test-threads=1'];start=time.monotonic()
with (out/'native.log').open('wb') as log:r=subprocess.run(cmd,env=env,stdout=log,stderr=subprocess.STDOUT)
after=hashes();result={'exitCode':r.returncode,'elapsedSeconds':round(time.monotonic()-start,3),'command':cmd,'sourceBefore':before,'sourceAfter':after,'sourceUnchanged':before==after};(out/'result.json').write_bytes((json.dumps(result,indent=2)+'\n').encode());print(json.dumps({k:v for k,v in result.items() if k not in ['sourceBefore','sourceAfter']}));raise SystemExit(r.returncode)
