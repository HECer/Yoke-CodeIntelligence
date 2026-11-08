from pathlib import Path
import subprocess,json,os,time
out=Path('validation/integration-20261018/git-areas-development');out.mkdir(parents=True,exist_ok=True)
env=os.environ.copy();env['PATH']=str(Path.home()/'.cargo/bin')+os.pathsep+str(Path.home()/'AppData/Local/YokeValidation/tools/powershell-7.6.7-win-x64')+os.pathsep+env['PATH'];receipts=[]
for label,cmd in [('fmt-r1',['cargo','fmt','--all']),('native-r1',['cargo','test','--locked','-j1','--test','git_areas','--','--test-threads=2'])]:
 start=time.monotonic()
 with (out/(label+'.log')).open('wb') as f:r=subprocess.run(cmd,env=env,stdout=f,stderr=subprocess.STDOUT)
 item={'gate':label,'exitCode':r.returncode,'elapsedSeconds':round(time.monotonic()-start,3)};receipts.append(item);(out/'receipts-r1.json').write_text(json.dumps(receipts,indent=2)+'\n',encoding='utf-8');print(json.dumps(item),flush=True)
 if r.returncode:print((out/(label+'.log')).read_text(errors='replace')[-6500:]);break
