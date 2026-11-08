from pathlib import Path
import subprocess,json,hashlib,time,shutil,re,os
work=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');root=work.parent/'Yoke-CodeIntelligence';out=root/'validation/integration-20261018/codex-host/final';out.mkdir(exist_ok=False);sha=lambda b:hashlib.sha256(b).hexdigest();binary=sha((root/'target/debug/yoke.exe').read_bytes())
def inventory():
 names=sorted(set(subprocess.check_output(['git','ls-files','-c','-o','--exclude-standard','-z','native','Cargo.toml','Cargo.lock','.gitattributes'],cwd=work).decode().split('\0')))
 return {n:sha((work/n).read_bytes()) for n in names if n and (work/n).is_file()}
before=inventory();(out/'source-before.json').write_text(json.dumps(before,indent=2)+'\n',encoding='utf-8')
for name in before:
 if 'host_install' in name or 'host_output' in name or 'codex_host' in name or name in ['native/src/host_config.rs','native/docs/host-install.md']:
  target=out/'source'/name;target.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(work/name,target)
cargo=str(Path.home()/'.cargo/bin/cargo.exe');common=['--workspace','--locked','-j','2','--target-dir',str(root/'target')];tests=['codex_host_cli','host_install','host_install_cli','host_config','host_output_disposition','host_cli','process','falkor_wire','falkor_publish','falkor_publish_cli','neo4j_publish','neo4j_publish_cli'];native=[cargo,'test',*common,'--no-fail-fast','--lib']
for name in tests:native+=['--test',name]
native+=['--','--test-threads=1'];env=os.environ.copy();env['PATH']=str(Path.home()/'AppData/Local/YokeValidation/tools/powershell-7.6.7-win-x64')+os.pathsep+env['PATH'];runs=[]
for name,command in [('native',native),('format',[cargo,'fmt','--all','--check']),('build',[cargo,'build',*common,'--bin','yoke'])]:
 print('START FINAL CODEX',name,flush=True);started=time.monotonic()
 with (out/(name+'.log')).open('wb') as log:p=subprocess.run(command,cwd=work,env=env,stdout=log,stderr=subprocess.STDOUT)
 record={'name':name,'command':command,'exitCode':p.returncode,'seconds':time.monotonic()-started};runs.append(record)
 if name=='native':
  log=(out/'native.log').read_text(encoding='utf-8',errors='replace');groups=re.findall(r'^test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;',log,re.M);record['totals']={k:sum(int(g[i]) for g in groups) for i,k in enumerate(['passed','failed','ignored'])};record['groups']=len(groups);record['failures']=re.findall(r'^test (\S+) \.\.\. FAILED$',log,re.M)
 if p.returncode:break
after=inventory();(out/'source-after.json').write_text(json.dumps(after,indent=2)+'\n',encoding='utf-8');result={'sourceUnchanged':before==after,'inputs':len(before),'commands':runs,'allChecksPassed':all(x['exitCode']==0 for x in runs),'nativeBinaryBeforeSha256':binary,'nativeBinaryAfterSha256':sha((root/'target/debug/yoke.exe').read_bytes()),'productionBinaryUnchanged':binary==sha((root/'target/debug/yoke.exe').read_bytes()),'wholeNativeRerun':False};(out/'result.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8');print(json.dumps(result),flush=True)
