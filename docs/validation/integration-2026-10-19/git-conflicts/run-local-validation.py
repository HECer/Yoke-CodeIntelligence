from pathlib import Path
import os,subprocess,time,json,hashlib,datetime,re,collections
root=Path.cwd();out=root/'validation/integration-20261018/git-conflicts-final-r3';out.mkdir(parents=True,exist_ok=False)
env=os.environ.copy();cargo=str(Path.home()/'.cargo/bin/cargo.exe');pwsh=Path.home()/'AppData/Local/YokeValidation/tools/powershell-7.6.7-win-x64';assert (pwsh/'pwsh.exe').is_file();env['PATH']=str(Path(cargo).parent)+os.pathsep+str(pwsh)+os.pathsep+env['PATH']
def save(n,v):(out/n).write_text(json.dumps(v,indent=2,ensure_ascii=False)+'\n',encoding='utf-8')
def manifest():
 names=subprocess.check_output(['git','ls-files','-c','-o','--exclude-standard','-z']).decode().split('\0')
 return {n:hashlib.sha256((root/n).read_bytes()).hexdigest() for n in sorted(set(names)) if n and (root/n).is_file() and (n.startswith(('native/src/','native/tests/','native/examples/','src/','test/','workers/','scripts/')) or n in ['Cargo.toml','Cargo.lock','native/Cargo.toml','package.json','package-lock.json','tsconfig.json','UPSTREAMS.json','docs/parity/native-reconciliation-2026-10-15.json'])}
metadata=json.loads(subprocess.check_output([cargo,'metadata','--no-deps','--format-version','1'],env=env));package=next(p for p in metadata['packages'] if p['name']=='yoke-code-intelligence');save('native-target-inventory.json',package['targets'])
before=manifest();save('source-before.json',before);receipts=[]
def run(label,cmd,timeout=900):
 print(json.dumps({'gate':label,'state':'running'}),flush=True);start=time.monotonic();timed_out=False
 with (out/(label+'.log')).open('wb') as log:
  process=subprocess.Popen(cmd,env=env,stdout=log,stderr=subprocess.STDOUT)
  try:code=process.wait(timeout=timeout)
  except subprocess.TimeoutExpired:
   timed_out=True
   if process.poll() is None:
    if os.name=='nt':subprocess.run(['taskkill','/PID',str(process.pid),'/T','/F'],stdout=log,stderr=subprocess.STDOUT)
    else:process.kill()
   process.wait();code=-1
 log=(out/(label+'.log')).read_text(encoding='utf-8',errors='replace');groups=[dict(zip(['passed','failed','ignored'],map(int,m))) for m in re.findall(r'^test result: \w+\. (\d+) passed; (\d+) failed; (\d+) ignored;',log,re.M)]
 receipt={'gate':label,'command':cmd,'exitCode':code,'timedOut':timed_out,'elapsedSeconds':round(time.monotonic()-start,3),'recordedAt':datetime.datetime.now(datetime.timezone.utc).isoformat(),'resultGroups':groups};receipts.append(receipt);save('receipts.json',receipts);print(json.dumps(receipt),flush=True);return log
run('format',[cargo,'fmt','--all','--check'])
run('clippy',[cargo,'clippy','--workspace','--all-targets','--locked','--keep-going','-j1','--','-D','warnings'],timeout=1800)
if any(r['exitCode'] for r in receipts):
 save('source-after.json',manifest());save('preflight-failure.json',{'checks':receipts});raise SystemExit(1)
run('library',[cargo,'test','--locked','-j1','--lib','--','--test-threads=2'],timeout=1800)
selected=['git_conflicts','git_workflows','git_cli','git_epochs','git_epoch_selection','git_review','git_affected_cli','git_affected_details','git_affected_qualification','git_impact_details','git_impact_details_cli','git_impact_details_no_git','git_impact_details_qualification','git_impact_details_sqlite_limits','capture_options','capture_policy','ignored_capture','editor','process','deadline_scope']
args=[cargo,'test','--locked','--no-fail-fast','-j1']
for name in selected:args+=['--test',name]
args+=['--','--test-threads=2']
run('native-selected',args,timeout=3600)
run('doctests',[cargo,'test','--workspace','--doc','--locked','-j1'])
run('python-contracts',['python','-m','unittest','discover','-s','scripts/tests','-v'])
run('typescript-typecheck',['npm.cmd','run','typecheck']);run('typescript-tests',['npm.cmd','test'])
save('tested-native-binary.json',{'path':'target/debug/yoke.exe','sha256':hashlib.sha256((root/'target/debug/yoke.exe').read_bytes()).hexdigest()})
after=manifest();save('source-after.json',after)
summary={'allChecksPassed':all(r['exitCode']==0 for r in receipts),'sourceUnchanged':before==after,'wholeNativeRun':False,'selectedIntegrationTargets':selected,'checks':receipts,'scope':'Scoped Windows validation for new opt-in actual index conflict inspection and early readonly CLI dispatch: full library plus20 Git/capture/scope/editor/process integration targets, all-target Clippy, Python and mandatory TypeScript checks. No HostGit/shared Store/source-scope implementation changes. Whole-native, hosted/platform, wider semantic/upstream and full STATUS acceptance remain separate.'};save('summary.json',summary)
print(json.dumps({'finished':True,'allChecksPassed':summary['allChecksPassed'],'sourceUnchanged':summary['sourceUnchanged']}),flush=True)
raise SystemExit(not(summary['allChecksPassed'] and summary['sourceUnchanged']))
