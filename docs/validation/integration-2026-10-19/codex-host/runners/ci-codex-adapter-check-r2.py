from pathlib import Path
import subprocess,json,hashlib,time,shutil
work=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');root=work.parent/'Yoke-CodeIntelligence';out=root/'validation/integration-20261018/codex-host/r2';out.mkdir(parents=True,exist_ok=False)
def inventory():
 names=sorted(set(subprocess.check_output(['git','ls-files','-c','-o','--exclude-standard','-z','native','Cargo.toml','Cargo.lock','.gitattributes'],cwd=work).decode().split('\0')))
 return {n:hashlib.sha256((work/n).read_bytes()).hexdigest() for n in names if n and (work/n).is_file()}
before=inventory();(out/'source-before.json').write_text(json.dumps(before,indent=2)+'\n',encoding='utf-8')
for name in before:
 if 'host_install' in name or 'host_output' in name or 'codex_host' in name or name=='native/src/host_config.rs':
  path=out/'source'/name;path.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(work/name,path)
cargo=str(Path.home()/'.cargo/bin/cargo.exe');common=['--workspace','--locked','-j','2','--target-dir',str(root/'target')];results=[]
for name,command in [('tests',[cargo,'test',*common,'--lib','host_install','--','--test-threads=1']),('contracts',[cargo,'test',*common,'--no-fail-fast','--test','codex_host_cli','--test','host_install','--test','host_install_cli','--test','host_config','--test','host_output_disposition','--','--test-threads=1']),('cli',[cargo,'build',*common,'--bin','yoke']),('clippy',[cargo,'clippy',*common,'--all-targets','--','-D','warnings'])]:
 print('START CODEX ADAPTER',name,flush=True);start=time.monotonic()
 with (out/(name+'.log')).open('wb') as log:p=subprocess.run(command,cwd=work,stdout=log,stderr=subprocess.STDOUT)
 results.append({'name':name,'command':command,'exitCode':p.returncode,'seconds':time.monotonic()-start})
 if p.returncode:break
after=inventory();(out/'source-after.json').write_text(json.dumps(after,indent=2)+'\n',encoding='utf-8');result={'sourceUnchanged':before==after,'inputs':len(before),'commands':results};(out/'result.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8');print(json.dumps(result),flush=True)
