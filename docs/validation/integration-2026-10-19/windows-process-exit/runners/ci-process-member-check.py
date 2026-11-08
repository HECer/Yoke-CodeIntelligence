from pathlib import Path
import subprocess,json,hashlib,time,shutil,sys
work=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence')
phase=sys.argv[1];mode=sys.argv[2] if len(sys.argv)>2 else 'target';out=root/'validation/integration-20261018/windows-process-exit'/phase;out.mkdir(parents=True,exist_ok=False)
def inventory():
 names=sorted(set(subprocess.check_output(['git','ls-files','-c','-o','--exclude-standard','-z','native','Cargo.toml','Cargo.lock','.gitattributes'],cwd=work).decode().split('\0')))
 return {n:hashlib.sha256((work/n).read_bytes()).hexdigest() for n in names if n and (work/n).is_file()}
before=inventory();(out/'source-before.json').write_text(json.dumps(before,indent=2)+'\n',encoding='utf-8')
for name in before:
 if name in ['native/src/process_windows_job.rs','native/src/process_windows_job_tests.rs','native/src/github_app_digest_history_process_tests.rs','native/src/lib.rs','native/src/arxiv_rate.rs','native/src/source_acquisition_http.rs','native/src/source_acquisition_cli.rs','native/src/arxiv_source.rs'] or name.startswith('native/tests/arxiv_'):
  path=out/'source'/name;path.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(work/name,path)
cargo=str(Path.home()/'.cargo/bin/cargo.exe');common=['--workspace','--locked','-j','2','--target-dir',str(root/'target')]
if mode=='quality':commands=[[cargo,'clippy',*common,'--all-targets','--','-D','warnings'],[cargo,'fmt','--all','--','--check']]
elif mode=='boundary':commands=[[cargo,'test',*common,'--test','arxiv_source','--','--test-threads=1']]
else:
 tests=['arxiv_source','arxiv_source_http','arxiv_source_cli']
 if mode=='final':tests+=['source_acquisition','source_acquisition_http','source_acquisition_cli','postgres_catalog','postgres_catalog_cli','federation_http','media','model_http','model_http_cli','model_http_review','process']
 command=[cargo,'test',*common,'--no-fail-fast']
 if mode=='final':command+=['--lib']
 for test in tests:command+=['--test',test]
 commands=([[cargo,'test',*common,'--lib','arxiv_rate::tests','--','--test-threads=1']] if mode!='final' else [])+[command+['--','--test-threads=1']]
results=[]
for i,command in enumerate(commands):
 start=time.time();print('START',phase,i,flush=True)
 with (out/('native.log' if i==0 else f'check-{i}.log')).open('wb') as log:p=subprocess.run(command,cwd=work,stdout=log,stderr=subprocess.STDOUT)
 results.append({'command':command,'exitCode':p.returncode,'seconds':time.time()-start})
after=inventory();(out/'source-after.json').write_text(json.dumps(after,indent=2)+'\n',encoding='utf-8');result={'checks':results,'inputs':len(before),'sourceUnchanged':before==after};(out/'result.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8');print(json.dumps(result),flush=True)
