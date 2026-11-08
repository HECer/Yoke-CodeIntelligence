from pathlib import Path
import subprocess,json,hashlib,time,shutil,re
work=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');root=work.parent/'Yoke-CodeIntelligence';out=root/'validation/integration-20261018/falkor-publish/quality-r5';out.mkdir(exist_ok=False)
def inventory():
 names=sorted(set(subprocess.check_output(['git','ls-files','-c','-o','--exclude-standard','-z','native','Cargo.toml','Cargo.lock','.gitattributes'],cwd=work).decode().split('\0')))
 return {n:hashlib.sha256((work/n).read_bytes()).hexdigest() for n in names if n and (work/n).is_file()}
before=inventory();(out/'source-before.json').write_text(json.dumps(before,indent=2)+'\n',encoding='utf-8')
for name in before:
 if 'falkor' in name or name in ['native/src/lib.rs','native/src/neo4j_publish.rs','native/src/main.rs','native/src/host_credentials.rs']:
  path=out/'source'/name;path.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(work/name,path)
cargo=str(Path.home()/'.cargo/bin/cargo.exe');common=['--workspace','--locked','-j','2','--target-dir',str(root/'target')]
selected=['falkor_wire','falkor_publish','falkor_publish_cli','neo4j_publish','neo4j_publish_cli','arxiv_source','arxiv_source_http','arxiv_source_cli','source_acquisition','source_acquisition_http','source_acquisition_cli','postgres_catalog','postgres_catalog_cli','federation_http','media','model_http','model_http_cli','model_http_review','process']
native=[cargo,'test',*common,'--no-fail-fast','--lib']
for test in selected:native+=['--test',test]
native+=['--','--test-threads=1'];results=[]
for name,command in [('native',native),('example',[cargo,'build',*common,'--example','falkor_owned_probe']),('clippy',[cargo,'clippy',*common,'--all-targets','--','-D','warnings']),('format',[cargo,'fmt','--all','--check'])]:
 print('START FALKOR FINAL',name,flush=True);start=time.monotonic()
 with (out/(name+'.log')).open('wb') as log:p=subprocess.run(command,cwd=work,stdout=log,stderr=subprocess.STDOUT)
 result={'name':name,'command':command,'exitCode':p.returncode,'seconds':time.monotonic()-start};results.append(result)
 if name=='native':
  log=(out/'native.log').read_text(encoding='utf-8',errors='replace');groups=re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;',log);failures=re.findall(r'^test (\S+) \.\.\. FAILED$',log,re.M)
  result['totals']={k:sum(int(g[i]) for g in groups) for i,k in enumerate(['passed','failed','ignored'])};result['groups']=len(groups);result['failures']=failures
  result['onlyKnownOs1314Failure']=failures==['media_cannot_bypass_source_scope_links_ignore_or_protected_paths'] and '1314' in log
  if p.returncode and not result['onlyKnownOs1314Failure']:break
 elif p.returncode:break
after=inventory();(out/'source-after.json').write_text(json.dumps(after,indent=2)+'\n',encoding='utf-8');result={'runs':results,'sourceUnchanged':before==after,'inputs':len(before),'wholeNativeRerun':False};(out/'result.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8');print(json.dumps(result),flush=True)
