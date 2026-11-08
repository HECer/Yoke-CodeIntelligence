from pathlib import Path
import subprocess,json,hashlib,time,shutil,os,sys
work=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');root=work.parent/'Yoke-CodeIntelligence';out=root/'validation/integration-20261018/neo4j-publish/quality';out.mkdir(parents=True,exist_ok=False)
def inventory():
 names=sorted(set(subprocess.check_output(['git','ls-files','-c','-o','--exclude-standard','-z'],cwd=work).decode().split('\0')))
 return {n:hashlib.sha256((work/n).read_bytes()).hexdigest() for n in names if n and (work/n).is_file() and (n.startswith(('native/','src/','test/','workers/','scripts/')) or n in ['Cargo.toml','Cargo.lock','.gitattributes','package.json','package-lock.json','tsconfig.json','UPSTREAMS.json','docs/parity/native-reconciliation-2026-10-15.json','.github/workflows/rust.yml'])}
before=inventory();(out/'source-before.json').write_text(json.dumps(before,indent=2)+'\n',encoding='utf-8')
for name in before:
 if 'neo4j' in name or name in ['native/src/lib.rs','native/src/host_credentials.rs','native/src/main.rs']:
  target=out/'source'/name;target.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(work/name,target)
cargo=str(Path.home()/'.cargo/bin/cargo.exe');common=['--workspace','--locked','-j','2','--target-dir',str(root/'target')]
commands=[('clippy',[cargo,'clippy',*common,'--all-targets','--','-D','warnings']),('format',[cargo,'fmt','--all','--','--check']),('build',[cargo,'build',*common,'--bin','yoke','--example','neo4j_owned_probe']),('neo4j-unit',[cargo,'test',*common,'--lib','neo4j_publish','--','--test-threads=1']),('neo4j-contracts',[cargo,'test',*common,'--test','neo4j_publish','--test','neo4j_publish_cli','--','--test-threads=1'])]
results=[];env=os.environ.copy();env['PATH']=str(Path.home()/'AppData/Local/YokeValidation/tools/powershell-7.6.7-win-x64')+os.pathsep+env['PATH']
for name,command in commands:
 start=time.time();print('START NEO4J QUALITY',name,flush=True)
 with (out/(name+'.log')).open('wb') as log:p=subprocess.run(command,cwd=work,env=env,stdout=log,stderr=subprocess.STDOUT)
 results.append({'name':name,'command':command,'exitCode':p.returncode,'seconds':time.time()-start})
 if p.returncode:break
after=inventory();(out/'source-after.json').write_text(json.dumps(after,indent=2)+'\n',encoding='utf-8');result={'runs':results,'inputs':len(before),'sourceUnchanged':before==after,'allChecksPassed':len(results)==len(commands) and all(r['exitCode']==0 for r in results)};(out/'result.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8');print(json.dumps(result),flush=True)
