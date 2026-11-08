from pathlib import Path
import subprocess,os,json,time,hashlib,shutil
root=Path('C:/Users/Lomi/Documents/NN-Developed/Yoke-CodeIntelligence');work=Path.cwd();out=root/'validation/integration-20261018/file-usage-development/quota-red-r2';out.mkdir(exist_ok=False);cargo=str(Path.home()/'.cargo/bin/cargo.exe');env=os.environ.copy();env['PATH']=str(Path(cargo).parent)+os.pathsep+env['PATH'];files=['native/src/semantic_file_delete.rs','native/tests/facade_file_usage.rs']
before={n:hashlib.sha256((work/n).read_bytes()).hexdigest() for n in files}
for n in files:
 p=out/'source'/n;p.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(work/n,p)
cmd=[cargo,'test','--locked','-j1','--target-dir',str(root/'target'),'--test','facade_file_usage','actual_file_inventory_quota_admits2000_and_refuses2001_before_provider_start','--','--exact','--nocapture','--test-threads=1'];start=time.monotonic()
with (out/'native.log').open('wb') as log:r=subprocess.run(cmd,env=env,stdout=log,stderr=subprocess.STDOUT)
after={n:hashlib.sha256((work/n).read_bytes()).hexdigest() for n in files};result={'exitCode':r.returncode,'elapsedSeconds':round(time.monotonic()-start,3),'command':cmd,'sourceBefore':before,'sourceAfter':after,'sourceUnchanged':before==after};(out/'result.json').write_bytes((json.dumps(result,indent=2)+'\n').encode());print(json.dumps(result));raise SystemExit(r.returncode)
