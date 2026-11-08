from pathlib import Path
import subprocess,os,hashlib,json,time
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');primary=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');out=primary/'validation/integration-20261018/postgres-catalog/quality-r2';out.mkdir(parents=True,exist_ok=False)
names=sorted(set(subprocess.check_output(['git','ls-files','-c','-o','--exclude-standard','-z','native','Cargo.toml','Cargo.lock','.gitattributes'],cwd=root).decode().split('\0')))
def inventory():return {n:hashlib.sha256((root/n).read_bytes()).hexdigest() for n in names if n and (root/n).is_file()}
before=inventory();(out/'source-before.json').write_text(json.dumps(before,indent=2)+'\n',encoding='utf-8');cargo=str(Path.home()/'.cargo/bin/cargo.exe');env=os.environ.copy();env['PATH']=str(Path.home()/'.cargo/bin')+os.pathsep+env['PATH'];results=[]
for name,cmd in [('clippy',[cargo,'clippy','--workspace','--all-targets','--locked','-j','2','--target-dir',str(primary/'target'),'--','-D','warnings']),('format',[cargo,'fmt','--all','--','--check'])]:
    print('START '+name,flush=True);start=time.time()
    with (out/(name+'.log')).open('wb') as log:p=subprocess.run(cmd,cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT)
    results.append({'name':name,'command':cmd,'exitCode':p.returncode,'seconds':round(time.time()-start,2)});print(json.dumps(results[-1]),flush=True)
after=inventory();(out/'source-after.json').write_text(json.dumps(after,indent=2)+'\n',encoding='utf-8');result={'sourceUnchanged':before==after,'inputs':len(before),'results':results,'allChecksPassed':all(r['exitCode']==0 for r in results)};(out/'result.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8');print(json.dumps(result),flush=True)
