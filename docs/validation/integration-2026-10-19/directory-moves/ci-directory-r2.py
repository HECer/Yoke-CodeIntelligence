from pathlib import Path
import subprocess,os,json,time,hashlib,shutil
root=Path('C:/Users/Lomi/Documents/NN-Developed/Yoke-CodeIntelligence');work=Path.cwd();out=root/'validation/integration-20261018/directory-moves-development';out.mkdir(parents=True,exist_ok=True)
exe=root/'target/debug/yoke.exe';saved=root/'validation/integration-20261018/plan-integrity-f17c8b0-yoke.exe'
if not saved.exists():
 assert hashlib.sha256(exe.read_bytes()).hexdigest()=='13c469ad8d2f10ebf5f301698292c007c25610104100a25055d0e8454afe2e1d';shutil.copyfile(exe,saved)
cargo=str(Path.home()/'.cargo/bin/cargo.exe');env=os.environ.copy();env['PATH']=str(Path(cargo).parent)+os.pathsep+str(Path.home()/'AppData/Local/YokeValidation/tools/powershell-7.6.7-win-x64')+os.pathsep+env['PATH']
names=subprocess.check_output(['git','ls-files','-c','-o','--exclude-standard','-z']).decode().split('\0');before={n:hashlib.sha256((work/n).read_bytes()).hexdigest() for n in sorted(set(names)) if n and n.startswith(('native/src/','native/tests/')) and (work/n).is_file()};(out/'source-before-r2.json').write_text(json.dumps(before,indent=2)+'\n',encoding='utf-8')
commands=[('fmt-r2',[cargo,'fmt','--all']),('native-r2',[cargo,'test','--locked','--no-fail-fast','-j1','--target-dir',str(root/'target'),'--lib','--test','facade_directory_move','--test','facade_file_move','--test','plan_integrity','--','--test-threads=2'])]
for label,cmd in commands:
 start=time.monotonic()
 with (out/(label+'.log')).open('wb') as log:r=subprocess.run(cmd,env=env,stdout=log,stderr=subprocess.STDOUT)
 print(json.dumps({'gate':label,'exitCode':r.returncode,'elapsedSeconds':round(time.monotonic()-start,3)}),flush=True)
 if r.returncode:raise SystemExit(r.returncode)