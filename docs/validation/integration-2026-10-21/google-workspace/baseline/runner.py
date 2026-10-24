"""Run the complete native default suite sequentially without retaining new test binaries.
Never removes a pre-existing artifact, source, directory, CLI binary or held crash file.
Start only after the scoped guard terminates and the exact source commit is clean.
"""
from pathlib import Path
import subprocess,json,hashlib,time,shutil,os,sys,re
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');root=w.parent/'Yoke-CodeIntelligence';target=(root/'target').resolve();deps=(target/'debug/deps').resolve();assert target.is_relative_to(root.resolve()) and deps.is_relative_to(target)
source,phase=sys.argv[1:];run=lambda *a:subprocess.check_output(a,cwd=w)
assert run('git','rev-parse','HEAD').decode().strip()==source
assert not run('git','status','--porcelain').strip()
assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=root).decode().strip()==source
assert not subprocess.check_output(['git','status','--porcelain'],cwd=root).strip()
out=root/'validation/integration-20261018'/phase;out.mkdir(exist_ok=False)
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
def inventory():
 names=run('git','ls-files','-c','-o','--exclude-standard','-z').decode().split('\0')
 include=lambda n:n.startswith(('native/','src/','test/','workers/','scripts/')) or n in ['Cargo.toml','Cargo.lock','package.json','package-lock.json','tsconfig.json','UPSTREAMS.json','docs/parity/native-reconciliation-2026-10-15.json','.github/workflows/rust.yml','.gitattributes','README.md','SECURITY.md','docs/ARCHITECTURE.md','docs/STATUS.md','docs/REMAINING_WORK.md']
 return {n:sha(w/n) for n in sorted(set(names)) if n and include(n) and (w/n).is_file()}
before=inventory();(out/'source-before.json').write_text(json.dumps(before,indent=2)+'\n',encoding='utf-8');shutil.copyfile(__file__,out/'runner.py')
pg='native/tests/fixtures/postgres-catalog-response.jsonl';assert (w/pg).stat().st_size==2779 and sha(w/pg)=='f67a05349a6cedf55222afd3bb3231c169d382dd4b078d76544daac517530b47'
cli=target/'debug/yoke.exe';cli_hash=sha(cli)
guard_dir=root/'validation/integration-20261018/function-body-caller-docs'
guard=json.loads((guard_dir/'guard-r2/result.json').read_text(encoding='utf-8'));assert guard['allChecksPassed'] and guard['sourceUnchanged']
native=json.loads((guard_dir/'guard-r2/source-after.json').read_text(encoding='utf-8'));assert all(before[n]==h for n,h in native.items())
cli_probe=json.loads((guard_dir/'native-current-r1/result.json').read_text(encoding='utf-8'));assert cli_probe['cliSha256']==cli_hash and cli_probe['exitCode']==0 and cli_probe['sourceUnchanged'] and not cli_probe['analyzedCodeExecuted']
assert cli_probe['response']['results'][0]['sourceExecuted'] is False and cli_probe['response']['results'][0]['actions'][0]['kind']=='native.inline.function.body'

tests=sorted(p.stem for p in (w/'native/tests').glob('*.rs'))
# Cargo.toml has default auto-discovery; preserve lib, bin and doctest groups too.
assert 'autotests = false' not in (w/'native/Cargo.toml').read_text(encoding='utf-8')
groups=[('lib',['--lib']),('bin-yoke',['--bin','yoke'])]+[(name,['--test',name]) for name in tests]+[('doc',['--doc'])]
manifest={'runnerPid':os.getpid(),'sourceCommit':source,'groups':[{'name':name,'selector':selector} for name,selector in groups],'integrationTargets':len(tests),'frozenInputs':len(before),'cliSha256':cli_hash,'existingArtifactsNeverDeleted':True,'ignorePolicy':'Cargo defaults; no ignored test promoted to passing','sourceCodeExecution':'Only repository test harnesses and their explicitly owned fixtures','fullStatusCertified':False}
(out/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n',encoding='utf-8')
cargo=str(Path.home()/'.cargo/bin/cargo.exe');common=['--workspace','--locked','-j','2','--target-dir',str(target)]
metadata_command=[cargo,'metadata','--no-deps','--locked','--format-version','1']
metadata_bytes=subprocess.check_output(metadata_command,cwd=w);(out/'cargo-metadata.json').write_bytes(metadata_bytes);metadata=json.loads(metadata_bytes)
package=next(p for p in metadata['packages'] if Path(p['manifest_path']).resolve()==(w/'native/Cargo.toml').resolve())
assert metadata['workspace_members']==[package['id']]
assert set(tests)=={t['name'] for t in package['targets'] if t['kind']==['test'] and t['test']}
assert sum(t['kind']==['lib'] and t['test'] for t in package['targets'])==1
assert sum(t['kind']==['bin'] and t['test'] and t['name']=='yoke' for t in package['targets'])==1
assert not any(t['test'] and t['kind']==['example'] for t in package['targets'])

env=os.environ.copy();env['PATH']=str(Path.home()/'AppData/Local/YokeValidation/tools/powershell-7.6.7-win-x64')+os.pathsep+env['PATH']
tools={}
for name,path in [('cargo',Path(cargo)),('rustc',Path.home()/'.cargo/bin/rustc.exe'),('node',Path(shutil.which('node'))),('powershell',Path.home()/'AppData/Local/YokeValidation/tools/powershell-7.6.7-win-x64/pwsh.exe')]:
 version=subprocess.check_output([str(path),'--version'],env=env).decode('utf-8',errors='replace').strip()
 tools[name]={'path':str(path.resolve()),'sha256':sha(path),'version':version}
compiler=(w/'node_modules/typescript/lib/typescript.js').resolve();tools['typescript']={'path':str(compiler),'sha256':sha(compiler),'version':json.loads((compiler.parent.parent/'package.json').read_text(encoding='utf-8'))['version']}
(out/'tools.json').write_text(json.dumps(tools,indent=2)+'\n',encoding='utf-8')
records=[];halted=None
for i,(name,selector) in enumerate(groups):
 assert inventory()==before,'Source changed before group '+name
 free=shutil.disk_usage(target).free
 if free<700*1024*1024:
  halted={'reason':'insufficient_free_bytes_before_link','freeBytes':free,'nextGroup':name};break
 stage=out/(str(i+1).zfill(3)+'-'+name);stage.mkdir();preexisting={p.name.casefold() for p in deps.iterdir() if p.is_file()};artifacts=[]
 record={'group':name,'index':i+1,'freeBytesBefore':free,'commands':[],'deletedNewTestArtifacts':[]};records.append(record)
 print('FULL NATIVE',i+1,'/',len(groups),name,flush=True)
 if name!='doc':
  command=[cargo,'test',*common,*selector,'--no-run','--message-format=json-render-diagnostics'];start=time.monotonic()
  with (stage/'compile.jsonl').open('wb') as stdout,(stage/'compile.log').open('wb') as stderr:p=subprocess.run(command,cwd=w,env=env,stdout=stdout,stderr=stderr)
  record['commands'].append({'name':'compile','command':command,'exitCode':p.returncode,'seconds':time.monotonic()-start})
  for line in (stage/'compile.jsonl').read_text(encoding='utf-8').splitlines():
   if not line.strip():continue
   msg=json.loads(line)
   if msg.get('reason')!='compiler-artifact' or not msg.get('profile',{}).get('test') or not msg.get('executable'):continue
   selected=msg['target']['name']==name or name=='lib' and 'lib' in msg['target']['kind'] or name=='bin-yoke' and 'bin' in msg['target']['kind'] and msg['target']['name']=='yoke'
   if not selected:continue
   original=Path(msg['executable']);assert not original.is_symlink();exe=original.resolve();assert exe.parent==deps and exe.suffix=='.exe'
   artifacts.append(exe)
  record['testExecutables']=[{'path':str(p),'sha256':sha(p),'existedBefore':p.name.casefold() in preexisting} for p in artifacts]
  if p.returncode:
   record['allChecksPassed']=False;record['sourceUnchanged']=inventory()==before;assert record['sourceUnchanged'];(stage/'result.json').write_text(json.dumps(record,indent=2)+'\n',encoding='utf-8');continue
  assert artifacts,'No selected Cargo test artifact for '+name
 command=[cargo,'test',*common,*selector,'--no-fail-fast','--','--test-threads=1'];start=time.monotonic()
 with (stage/'test.log').open('wb') as log:p=subprocess.run(command,cwd=w,env=env,stdout=log,stderr=subprocess.STDOUT)
 record['commands'].append({'name':'test','command':command,'exitCode':p.returncode,'seconds':time.monotonic()-start})
 log=(stage/'test.log').read_text(encoding='utf-8',errors='replace');totals=re.findall(r'^test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;',log,re.M)
 record['totals']={k:sum(int(g[j]) for g in totals) for j,k in enumerate(['passed','failed','ignored'])};record['resultGroups']=len(totals);record['failures']=re.findall(r'^test (\S+) \.\.\. FAILED$',log,re.M)
 record['allChecksPassed']=all(c['exitCode']==0 for c in record['commands']);record['sourceUnchanged']=inventory()==before;assert record['sourceUnchanged']
 # All child processes from this Cargo test have returned. Delete only exact
 # newly created test exe/PDB paths within the checked build-output directory.
 for exe in artifacts:
  for candidate in [exe,exe.with_suffix('.pdb')]:
   if candidate.name.casefold() in preexisting or not candidate.exists():continue
   resolved=candidate.resolve();assert resolved.parent==deps and resolved.is_relative_to(target) and not candidate.is_symlink() and resolved!=cli.resolve()
   item={'path':str(resolved),'bytes':resolved.stat().st_size,'sha256':sha(resolved)}
   try:resolved.unlink();item['deleted']=True
   except OSError as error:item.update({'deleted':False,'error':str(error)})
   record['deletedNewTestArtifacts'].append(item)
 record['freeBytesAfter']=shutil.disk_usage(target).free;(stage/'result.json').write_text(json.dumps(record,indent=2)+'\n',encoding='utf-8')
 print('DONE',name,json.dumps(record['totals']),'passed='+str(record['allChecksPassed']),flush=True)
# Cargo's default test selection also builds examples without running them.
examples=None
if halted is None:
 assert inventory()==before
 command=[cargo,'build',*common,'--examples'];start=time.monotonic()
 with (out/'examples-build.log').open('wb') as log:process=subprocess.run(command,cwd=w,env=env,stdout=log,stderr=subprocess.STDOUT)
 examples={'command':command,'exitCode':process.returncode,'seconds':time.monotonic()-start,'exampleCodeExecuted':False}
after=inventory();(out/'source-after.json').write_text(json.dumps(after,indent=2)+'\n',encoding='utf-8')
tools_unchanged=all(sha(Path(info['path']))==info['sha256'] for info in tools.values())
result={'sourceCommit':source,'sourceUnchanged':before==after,'frozenInputs':len(before),'registeredGroups':len(groups),'completedGroups':len(records),'allRegisteredGroupsAttempted':len(records)==len(groups) and halted is None,'allChecksPassed':len(records)==len(groups) and halted is None and examples is not None and examples['exitCode']==0 and all(r['allChecksPassed'] for r in records) and before==after and sha(cli)==cli_hash and tools_unchanged,'halted':halted,'examplesBuild':examples,'totals':{k:sum(r.get('totals',{}).get(k,0) for r in records) for k in ['passed','failed','ignored']},'cliUnchanged':sha(cli)==cli_hash,'existingArtifactsNeverDeleted':True,'toolsUnchanged':tools_unchanged,'attemptedScope':'complete_native_default_suite','fullWindowsCertified':False,'fullStatusCertified':False,'groups':records}
(out/'result.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8');print(json.dumps({k:v for k,v in result.items() if k!='groups'}),flush=True)

sys.exit(0 if result['allChecksPassed'] else 1)
