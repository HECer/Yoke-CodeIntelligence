from pathlib import Path
import subprocess,os,json,time,hashlib,shutil,datetime
root=Path('C:/Users/Lomi/Documents/NN-Developed/Yoke-CodeIntelligence');work=Path.cwd();out=root/'validation/integration-20261018/declaration-attachments-development/red-r1';out.mkdir(parents=True,exist_ok=False)
files=sorted(p for p in work.rglob('*') if p.is_file() and (p.relative_to(work).as_posix().startswith(('native/src/','native/tests/')) or p.relative_to(work).as_posix() in ['Cargo.toml','Cargo.lock','native/Cargo.toml']))
def hashes():return {p.relative_to(work).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in files}
before=hashes()
for p in files:
 dst=out/'source'/p.relative_to(work);dst.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(p,dst)
cargo=str(Path.home()/'.cargo/bin/cargo.exe');env=os.environ.copy();env['PATH']=str(Path(cargo).parent)+os.pathsep+env['PATH'];env['YOKE_RUSTC']=str(Path.home()/'.cargo/bin/rustc.exe');env['YOKE_PYTHON']='C:/Python314/python.exe';versions={}
for name,cmd in [('rustc',[env['YOKE_RUSTC'],'--version']),('python',[env['YOKE_PYTHON'],'--version'])]:versions[name]={'command':cmd,'output':subprocess.check_output(cmd,env=env,stderr=subprocess.STDOUT).decode().strip()}
cmd=[cargo,'test','--locked','-j1','--target-dir',str(root/'target'),'--test','declaration_attachments_live','--','--ignored','--nocapture','--test-threads=1'];start=time.monotonic()
with (out/'native.log').open('wb') as log:r=subprocess.run(cmd,env=env,stdout=log,stderr=subprocess.STDOUT)
after=hashes();result={'recordedAt':datetime.datetime.now(datetime.timezone.utc).isoformat(),'exitCode':r.returncode,'elapsedSeconds':round(time.monotonic()-start,3),'command':cmd,'toolVersions':versions,'sourceBefore':before,'sourceAfter':after,'sourceUnchanged':before==after};(out/'result.json').write_bytes((json.dumps(result,indent=2)+'\n').encode());print(json.dumps({k:v for k,v in result.items() if k not in ['sourceBefore','sourceAfter']}));raise SystemExit(r.returncode)
