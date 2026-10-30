from pathlib import Path
import subprocess,json,hashlib,time,shutil
work=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');root=work.parent/'Yoke-CodeIntelligence';out=root/'validation/integration-20261018/neo4j-publish/final';out.mkdir(parents=True,exist_ok=False)
def inventory():
 names=sorted(set(subprocess.check_output(['git','ls-files','-c','-o','--exclude-standard','-z','native','Cargo.toml','Cargo.lock','.gitattributes'],cwd=work).decode().split('\0')))
 return {n:hashlib.sha256((work/n).read_bytes()).hexdigest() for n in names if n and (work/n).is_file()}
before=inventory();(out/'source-before.json').write_text(json.dumps(before,indent=2)+'\n',encoding='utf-8')
for name in before:
 if 'neo4j' in name or name in ['native/src/lib.rs','native/src/main.rs','native/src/host_credentials.rs']:
  target=out/'source'/name;target.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(work/name,target)
tests=['neo4j_publish','neo4j_publish_cli','arxiv_source','arxiv_source_http','arxiv_source_cli','source_acquisition','source_acquisition_http','source_acquisition_cli','postgres_catalog','postgres_catalog_cli','federation_http','media','model_http','model_http_cli','model_http_review','process']
command=[str(Path.home()/'.cargo/bin/cargo.exe'),'test','--workspace','--locked','-j','2','--target-dir',str(root/'target'),'--no-fail-fast','--lib']
for test in tests:command+=['--test',test]
command+=['--','--test-threads=1'];start=time.time();print('START FINAL NEO4J AND EXISTING BOUNDARIES',flush=True)
with (out/'native.log').open('wb') as log:p=subprocess.run(command,cwd=work,stdout=log,stderr=subprocess.STDOUT)
after=inventory();(out/'source-after.json').write_text(json.dumps(after,indent=2)+'\n',encoding='utf-8');result={'command':command,'exitCode':p.returncode,'seconds':time.time()-start,'inputs':len(before),'sourceUnchanged':before==after,'wholeNativeRerun':False};(out/'result.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8');print(json.dumps(result),flush=True)
