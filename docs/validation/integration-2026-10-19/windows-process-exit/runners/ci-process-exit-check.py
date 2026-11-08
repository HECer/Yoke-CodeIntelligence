from pathlib import Path
import subprocess,json,hashlib,time,shutil
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');work=root.parent/'Yoke-CI-r1-attach';out=root/'validation/integration-20261018/windows-process-exit/r2';out.mkdir(parents=True,exist_ok=False);sha=lambda b:hashlib.sha256(b).hexdigest()
def inventory():
 files=sorted(set(subprocess.check_output(['git','ls-files','-c','-o','--exclude-standard','-z','native','Cargo.toml','Cargo.lock','.gitattributes'],cwd=work).decode().split('\0')))
 return {n:sha((work/n).read_bytes()) for n in files if n and (work/n).is_file()}
before=inventory();(out/'source-before.json').write_text(json.dumps(before,indent=2)+'\n',encoding='utf-8');target=out/'source/native/src/github_app_digest_history_process_tests.rs';target.parent.mkdir(parents=True);shutil.copyfile(work/'native/src/github_app_digest_history_process_tests.rs',target)
cargo=str(Path.home()/'.cargo/bin/cargo.exe');common=['--workspace','--locked','-j','2','--target-dir',str(root/'target')];commands=[[cargo,'test',*common,'--lib','history_owned_process_tests','--','--test-threads=1'],[cargo,'test',*common,'--lib','process::windows_job','--','--test-threads=1'],[cargo,'test',*common,'--test','process','--','--test-threads=1'],[cargo,'clippy',*common,'--all-targets','--','-D','warnings'],[cargo,'fmt','--all','--','--check']];results=[]
for i,command in enumerate(commands):
 print('START PROCESS CHECK',i,flush=True);start=time.time()
 with (out/f'check-{i}.log').open('wb') as log:p=subprocess.run(command,cwd=work,stdout=log,stderr=subprocess.STDOUT)
 results.append({'command':command,'exitCode':p.returncode,'seconds':time.time()-start});print(json.dumps(results[-1]),flush=True)
 if p.returncode:break
after=inventory();(out/'source-after.json').write_text(json.dumps(after,indent=2)+'\n',encoding='utf-8');result={'checks':results,'inputs':len(before),'sourceUnchanged':before==after,'allChecksPassed':len(results)==len(commands) and all(r['exitCode']==0 for r in results)};(out/'result.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8');print(json.dumps(result),flush=True)
