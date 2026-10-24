from pathlib import Path
import os,subprocess,time,json,hashlib,datetime,re,collections
root=Path.cwd();out=root/'validation/integration-20261018/plan-integrity-final-r2';out.mkdir(parents=True,exist_ok=False)
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
run('clippy',[cargo,'clippy','--workspace','--all-targets','--locked','--keep-going','-j1','--','-D','warnings'])
if receipts[-1]['exitCode'] != 0:
 save('source-after.json',manifest());save('preflight-failure.json',{'checks':receipts,'wholeNativeStarted':False});raise SystemExit(1)
native=run('whole-native',[cargo,'test','--workspace','--all-targets','--locked','--no-fail-fast','-j1','--','--test-threads=2'],timeout=7200)
expected=collections.Counter(t['name'].replace('-','_') for t in package['targets'] if t['test']);seen=collections.Counter()
for path in re.findall(r'^\s*Running .*?\(([^\r\n]+)\)\s*$',native,re.M):
 name=Path(path).stem;name=re.sub(r'-[0-9a-f]{16}$','',name).replace('-','_');seen[name]+=1
coverage={'expectedTestableTargets':dict(expected),'observedExecutableNames':dict(seen),'expectedTestableTargetCount':sum(expected.values()),'observedInvocations':sum(seen.values()),'missing':{n:c-seen[n] for n,c in expected.items() if seen[n]<c},'failures':re.findall(r'^test (.+?) \.\.\. FAILED\s*$',native,re.M)};coverage['allTestableTargetsRan']=not coverage['missing'];save('native-target-coverage.json',coverage)
run('doctests',[cargo,'test','--workspace','--doc','--locked','--no-fail-fast','-j2'])
run('python-contracts',['python','-m','unittest','discover','-s','scripts/tests','-v'])
run('typescript-typecheck',['npm.cmd','run','typecheck']);run('typescript-tests',['npm.cmd','test'])
run('actual-os-credential-cli',[cargo,'test','--locked','-j2','--test','github_app_digest_handoff','actual_cli_send_status_and_lost_response_recovery_use_os_key_without_retry','--','--exact','--ignored','--test-threads=1'])
run('actual-tls-receiver-cli',[cargo,'test','--locked','-j2','--test','digest_receiver','actual_tls_receiver_and_sender_cli_recover_using_os_credentials_and_selected_ca','--','--exact','--ignored','--test-threads=1'])
save('tested-native-binary.json',{'path':'target/debug/yoke.exe','sha256':hashlib.sha256((root/'target/debug/yoke.exe').read_bytes()).hexdigest()})
after=manifest();save('source-after.json',after);summary={'allChecksPassed':all(r['exitCode']==0 for r in receipts),'sourceUnchanged':before==after,'wholeNativeRun':True,'allTestableNativeTargetsRan':coverage['allTestableTargetsRan'],'nativeTargetCoverage':coverage,'checks':receipts,'scope':'Full default Windows native all-target run after saved-plan identity, complete captured delta, exact review and warning replay across approval/application/verification/recovery, with scoped inherited deadline policy. Separate actual OS/TLS CLI gates, Python contracts and mandatory TypeScript checks remain nonadditive. This is an owned Windows development validation; host-gated ignored cases, broader STATUS/R1-R9 behavior, platform/live upstream certification and release remain open. Plan fault injection assumes access to owned saved state; it does not demonstrate an untrusted-repository or cross-principal exploit.'};save('summary.json',summary)
print(json.dumps({'finished':True,'allChecksPassed':summary['allChecksPassed'],'sourceUnchanged':summary['sourceUnchanged'],'allTestableNativeTargetsRan':coverage['allTestableTargetsRan']}),flush=True)
raise SystemExit(not(summary['allChecksPassed'] and summary['sourceUnchanged'] and coverage['allTestableTargetsRan']))
