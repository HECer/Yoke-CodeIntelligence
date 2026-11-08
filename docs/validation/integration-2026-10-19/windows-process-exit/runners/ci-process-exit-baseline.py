from pathlib import Path
import subprocess,json,hashlib,time,os
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');work=root.parent/'Yoke-CI-r1-attach';out=root/'validation/integration-20261018/windows-process-exit/baseline';out.mkdir(parents=True,exist_ok=False)
binary=root/'target/debug/deps/yoke-f14d0af26d19713e.exe';name='github_app::digest::history_boundary_tests::history_owned_process_tests::cancellation_and_deadline_stop_owned_git_and_restore_the_same_store';sha=lambda b:hashlib.sha256(b).hexdigest()
def inventory():
 files=sorted(set(subprocess.check_output(['git','ls-files','-c','-o','--exclude-standard','-z','native','Cargo.toml','Cargo.lock','.gitattributes'],cwd=work).decode().split('\0')))
 return {n:sha((work/n).read_bytes()) for n in files if n and (work/n).is_file()}
before=inventory();prior=json.loads((root/'validation/integration-20261018/arxiv-source/final/source-before.json').read_bytes());assert before==prior
(out/'source-before.json').write_text(json.dumps(before,indent=2)+'\n',encoding='utf-8');binary_hash=sha(binary.read_bytes());results=[];env=os.environ.copy();env['PATH']=str(Path.home()/'.cargo/bin')+os.pathsep+env['PATH']
for i in range(8):
 command=[str(binary),'--exact',name,'--test-threads=1'];start=time.time();print('START SAME-BINARY SERIAL',i+1,flush=True)
 with (out/f'iteration-{i+1}.log').open('wb') as log:p=subprocess.run(command,cwd=work,env=env,stdout=log,stderr=subprocess.STDOUT,timeout=35,creationflags=subprocess.CREATE_NO_WINDOW if os.name=='nt' else 0)
 results.append({'iteration':i+1,'command':command,'exitCode':p.returncode,'seconds':time.time()-start});print(json.dumps(results[-1]),flush=True)
 if p.returncode:break
after=inventory();(out/'source-after.json').write_text(json.dumps(after,indent=2)+'\n',encoding='utf-8');result={'policy':'up_to8_serial_same_binary_stop_first_failure_nonadditive','binarySha256':binary_hash,'binaryUnchanged':sha(binary.read_bytes())==binary_hash,'sourceUnchanged':before==after,'inputs':len(before),'iterations':results,'parentCheckpoint':'7237a6c54b217df7be61afa8044786a1c7e75406','causalConclusion':None};(out/'result.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8');print(json.dumps(result),flush=True)
