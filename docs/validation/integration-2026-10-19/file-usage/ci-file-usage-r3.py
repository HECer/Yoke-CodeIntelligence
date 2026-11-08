from pathlib import Path
import subprocess,os,json,time,hashlib,shutil,re
root=Path('C:/Users/Lomi/Documents/NN-Developed/Yoke-CodeIntelligence');work=Path.cwd();out=root/'validation/integration-20261018/file-usage-development/r3';out.mkdir(exist_ok=False);cargo=str(Path.home()/'.cargo/bin/cargo.exe');env=os.environ.copy();env['PATH']=str(Path(cargo).parent)+os.pathsep+str(Path.home()/'AppData/Local/YokeValidation/tools/powershell-7.6.7-win-x64')+os.pathsep+env['PATH'];tools=Path.home()/'AppData/Local/YokeValidation/typescript-5.8.3-lsp-5.1.3/node_modules';env['YOKE_TS_LSP']=str(tools/'typescript-language-server/lib/cli.mjs');env['YOKE_TSC']=str(tools/'typescript/lib/tsc.js')
with (out/'format.log').open('wb') as log:r=subprocess.run([cargo,'fmt','--all'],env=env,stdout=log,stderr=subprocess.STDOUT)
assert r.returncode==0
names=subprocess.check_output(['git','ls-files','-c','-o','--exclude-standard','-z']).decode().split('\0');before={n:hashlib.sha256((work/n).read_bytes()).hexdigest() for n in sorted(set(names)) if n and n.startswith(('native/src/','native/tests/')) and (work/n).is_file()};(out/'source-before.json').write_bytes((json.dumps(before,indent=2)+'\n').encode())
changed=subprocess.check_output(['git','diff','--name-only','-z'],stderr=subprocess.DEVNULL).decode().split('\0');new=subprocess.check_output(['git','ls-files','-o','--exclude-standard','-z']).decode().split('\0')
for n in sorted(set(changed+new)):
 if n and n.startswith(('native/src/','native/tests/','native/docs/')):
  p=out/'source'/n;p.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(work/n,p)
selections=[]
for name in ['facade_file_usage','facade_file_delete','semantic_refactor','refactor_intents','facade_directory_move','plan_integrity']:selections+=['--test',name]
commands=[('clippy',[cargo,'clippy','--locked','--all-targets','--keep-going','-j1','--target-dir',str(root/'target'),'--','-D','warnings']),('scoped-native',[cargo,'test','--locked','--no-fail-fast','-j1','--target-dir',str(root/'target')]+selections+['--','--test-threads=1']),('actual-provider',[cargo,'test','--locked','-j1','--target-dir',str(root/'target'),'--test','file_usage_live','--','--ignored','--nocapture','--test-threads=1'])];results=[]
for name,cmd in commands:
 start=time.monotonic();print(json.dumps({'gate':name,'state':'running'}),flush=True)
 with (out/(name+'.log')).open('wb') as log:r=subprocess.run(cmd,env=env,stdout=log,stderr=subprocess.STDOUT)
 raw=(out/(name+'.log')).read_text(encoding='utf-8',errors='replace');groups=[dict(zip(['passed','failed','ignored'],map(int,m))) for m in re.findall(r'^test result: \w+\. (\d+) passed; (\d+) failed; (\d+) ignored;',raw,re.M)];value={'gate':name,'exitCode':r.returncode,'elapsedSeconds':round(time.monotonic()-start,3),'command':cmd,'groups':groups};results.append(value);(out/'receipts.json').write_bytes((json.dumps(results,indent=2)+'\n').encode());print(json.dumps({k:v for k,v in value.items() if k!='groups'}),flush=True)
 if name=='clippy' and r.returncode:break
after={n:hashlib.sha256((work/n).read_bytes()).hexdigest() for n in before};(out/'source-after.json').write_bytes((json.dumps(after,indent=2)+'\n').encode());(out/'summary.json').write_bytes((json.dumps({'checks':results,'sourceUnchanged':before==after,'libraryReplay':'Initial R1 library425/0/7 remains a separate earlier source observation; only the new private bulk-read loop changed, and its boundary/refusal/approval/recovery integration cases are replayed here.'},indent=2)+'\n').encode());assert before==after
raise SystemExit(1 if len(results)!=3 or any(x['exitCode'] for x in results) else 0)
