from pathlib import Path
import subprocess,os,json,time
root=Path('C:/Users/Lomi/Documents/NN-Developed/Yoke-CodeIntelligence');out=root/'validation/integration-20261018/directory-moves-development';out.mkdir(parents=True,exist_ok=True)
cargo=str(Path.home()/'.cargo/bin/cargo.exe');env=os.environ.copy();env['PATH']=str(Path(cargo).parent)+os.pathsep+env['PATH']
commands=[('fmt-r1',[cargo,'fmt','--all']),('backend-r1',[cargo,'test','--locked','-j1','--target-dir',str(root/'target'),'--lib','changes::directory::tests','--','--test-threads=2'])]
for label,cmd in commands:
 start=time.monotonic()
 with (out/(label+'.log')).open('wb') as log:r=subprocess.run(cmd,env=env,stdout=log,stderr=subprocess.STDOUT)
 print(json.dumps({'gate':label,'exitCode':r.returncode,'elapsedSeconds':round(time.monotonic()-start,3)}),flush=True)
 if r.returncode:raise SystemExit(r.returncode)