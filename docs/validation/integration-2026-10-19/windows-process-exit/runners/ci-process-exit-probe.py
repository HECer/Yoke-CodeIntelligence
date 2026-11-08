from pathlib import Path
import subprocess,json,hashlib,time,os
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');work=root.parent/'Yoke-CI-r1-attach';out=root/'validation/integration-20261018/windows-process-exit/probe';out.mkdir(parents=True,exist_ok=False);binary=root/'target/debug/deps/yoke-f14d0af26d19713e.exe';sha=lambda b:hashlib.sha256(b).hexdigest();before=json.loads((out.parent/'r3/source-before.json').read_bytes());assert all(sha((work/n).read_bytes())==h for n,h in before.items());binary_hash=sha(binary.read_bytes());results=[]
env=os.environ.copy();env['PATH']=str(Path.home()/'.cargo/bin')+os.pathsep+env['PATH']
for i in range(4):
 command=[str(binary),'--exact','github_app::digest::history_boundary_tests::history_owned_process_tests::cancellation_and_deadline_stop_owned_git_and_restore_the_same_store','--nocapture','--test-threads=1'];start=time.time()
 with (out/f'iteration-{i+1}.log').open('wb') as log:p=subprocess.run(command,cwd=work,env=env,stdout=log,stderr=subprocess.STDOUT,timeout=35,creationflags=subprocess.CREATE_NO_WINDOW if os.name=='nt' else 0)
 results.append({'iteration':i+1,'exitCode':p.returncode,'seconds':time.time()-start});print(json.dumps(results[-1]),flush=True)
 if p.returncode:break
result={'policy':'up_to4_serial_trace_stop_first_failure','iterations':results,'inputs':len(before),'sourceUnchanged':all(sha((work/n).read_bytes())==h for n,h in before.items()),'binarySha256':binary_hash,'binaryUnchanged':sha(binary.read_bytes())==binary_hash,'concurrentOtherBuild':'r3 command2 builds distinct non-test process executable; this immutable unit executable is not rewritten; helper Cargo uses its private target directory'};(out/'result.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8');print(json.dumps(result),flush=True)
