from pathlib import Path
import subprocess,os,json,time,hashlib,shutil,re
root=Path('C:/Users/Lomi/Documents/NN-Developed/Yoke-CodeIntelligence');work=Path.cwd();out=root/'validation/integration-20261018/file-usage-development';out.mkdir(exist_ok=False);cargo=str(Path.home()/'.cargo/bin/cargo.exe');env=os.environ.copy();env['PATH']=str(Path(cargo).parent)+os.pathsep+str(Path.home()/'AppData/Local/YokeValidation/tools/powershell-7.6.7-win-x64')+os.pathsep+env['PATH'];tools=Path.home()/'AppData/Local/YokeValidation/typescript-5.8.3-lsp-5.1.3/node_modules';env['YOKE_TS_LSP']=str(tools/'typescript-language-server/lib/cli.mjs');env['YOKE_TSC']=str(tools/'typescript/lib/tsc.js');assert Path(env['YOKE_TS_LSP']).is_file() and Path(env['YOKE_TSC']).is_file()
with (out/'format-r1.log').open('wb') as log:r=subprocess.run([cargo,'fmt','--all'],env=env,stdout=log,stderr=subprocess.STDOUT)
assert r.returncode==0
names=subprocess.check_output(['git','ls-files','-c','-o','--exclude-standard','-z']).decode().split('\0');before={n:hashlib.sha256((work/n).read_bytes()).hexdigest() for n in sorted(set(names)) if n and n.startswith(('native/src/','native/tests/')) and (work/n).is_file()};(out/'source-before-r1.json').write_bytes((json.dumps(before,indent=2)+'\n').encode())
changed=subprocess.check_output(['git','diff','--name-only','-z'],stderr=subprocess.DEVNULL).decode().split('\0');new=subprocess.check_output(['git','ls-files','-o','--exclude-standard','-z']).decode().split('\0')
for n in sorted(set(changed+new)):
 if n and n.startswith(('native/src/','native/tests/','native/docs/')):
  p=out/'r1-source'/n;p.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(work/n,p)
results=[]
selections=['--lib']
for name in ['facade_file_usage','facade_file_delete','semantic_refactor','refactor_intents','facade_directory_move','plan_integrity','file_usage_live']:selections+=['--test',name]
for name,cmd in [('scoped-r1',[cargo,'test','--locked','--no-fail-fast','-j1','--target-dir',str(root/'target')]+selections+['--','--test-threads=1']),('actual-provider-r1',[cargo,'test','--locked','-j1','--target-dir',str(root/'target'),'--test','file_usage_live','--','--ignored','--nocapture','--test-threads=1'])]:
 start=time.monotonic()
 with (out/(name+'.log')).open('wb') as log:r=subprocess.run(cmd,env=env,stdout=log,stderr=subprocess.STDOUT)
 raw=(out/(name+'.log')).read_text(encoding='utf-8',errors='replace');groups=[dict(zip(['passed','failed','ignored'],map(int,m))) for m in re.findall(r'^test result: \w+\. (\d+) passed; (\d+) failed; (\d+) ignored;',raw,re.M)];result={'gate':name,'exitCode':r.returncode,'elapsedSeconds':round(time.monotonic()-start,3),'command':cmd,'groups':groups};results.append(result);print(json.dumps(result),flush=True)
after={n:hashlib.sha256((work/n).read_bytes()).hexdigest() for n in before};(out/'source-after-r1.json').write_bytes((json.dumps(after,indent=2)+'\n').encode());(out/'r1-results.json').write_bytes((json.dumps({'gates':results,'sourceUnchanged':before==after},indent=2)+'\n').encode());assert before==after
raise SystemExit(1 if any(x['exitCode'] for x in results) else 0)
