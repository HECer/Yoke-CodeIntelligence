from pathlib import Path
import os,subprocess,time,json,hashlib,datetime,re,collections
root=Path.cwd();out=root/'validation/integration-20261018/directory-moves-final';out.mkdir(parents=True,exist_ok=False)
env=os.environ.copy();cargo=str(Path.home()/'.cargo/bin/cargo.exe');pwsh=Path.home()/'AppData/Local/YokeValidation/tools/powershell-7.6.7-win-x64';assert (pwsh/'pwsh.exe').is_file();env['PATH']=str(Path(cargo).parent)+os.pathsep+str(pwsh)+os.pathsep+env['PATH'];env['YOKE_RUST_ANALYZER']=str(Path(cargo).with_name('rust-analyzer.exe'));env['YOKE_RUSTC']=str(Path(cargo).with_name('rustc.exe'))
def save(n,v):(out/n).write_text(json.dumps(v,indent=2,ensure_ascii=False)+'\n',encoding='utf-8')
def manifest():
 names=subprocess.check_output(['git','ls-files','-c','-o','--exclude-standard','-z']).decode().split('\0')
 return {n:hashlib.sha256((root/n).read_bytes()).hexdigest() for n in sorted(set(names)) if n and (root/n).is_file() and (n.startswith(('native/src/','native/tests/','native/examples/','src/','test/','workers/','scripts/')) or n in ['Cargo.toml','Cargo.lock','native/Cargo.toml','package.json','package-lock.json','tsconfig.json','UPSTREAMS.json','docs/parity/native-reconciliation-2026-10-15.json','.github/workflows/rust.yml'])}
metadata=json.loads(subprocess.check_output([cargo,'metadata','--no-deps','--format-version','1'],env=env));package=next(p for p in metadata['packages'] if p['name']=='yoke-code-intelligence');save('native-target-inventory.json',package['targets']);before=manifest();save('source-before.json',before);receipts=[]
def run(label,cmd,timeout=900):
 print(json.dumps({'gate':label,'state':'running'}),flush=True);start=time.monotonic();timed_out=False
 with (out/(label+'.log')).open('wb') as log:
  process=subprocess.Popen(cmd,env=env,stdout=log,stderr=subprocess.STDOUT)
  try:code=process.wait(timeout=timeout)
  except subprocess.TimeoutExpired:
   timed_out=True
   if process.poll() is None:subprocess.run(['taskkill','/PID',str(process.pid),'/T','/F'],stdout=log,stderr=subprocess.STDOUT)
   process.wait();code=-1
 raw=(out/(label+'.log')).read_text(encoding='utf-8',errors='replace');groups=[dict(zip(['passed','failed','ignored'],map(int,m))) for m in re.findall(r'^test result: \w+\. (\d+) passed; (\d+) failed; (\d+) ignored;',raw,re.M)];receipt={'gate':label,'command':cmd,'exitCode':code,'timedOut':timed_out,'elapsedSeconds':round(time.monotonic()-start,3),'recordedAt':datetime.datetime.now(datetime.timezone.utc).isoformat(),'resultGroups':groups};receipts.append(receipt);save('receipts.json',receipts);print(json.dumps({k:v for k,v in receipt.items() if k!='resultGroups'}),flush=True);return raw
run('format',[cargo,'fmt','--all','--check'])
run('clippy',[cargo,'clippy','--workspace','--all-targets','--locked','--keep-going','-j1','--','-D','warnings'])
if any(r['exitCode'] for r in receipts):
 save('source-after.json',manifest());save('preflight-failure.json',{'checks':receipts,'wholeNativeStarted':False});raise SystemExit(1)
native=run('whole-native',[cargo,'test','--workspace','--all-targets','--locked','--no-fail-fast','-j1','--','--test-threads=1'],timeout=7200)
expected=collections.Counter(t['name'].replace('-','_') for t in package['targets'] if t['test']);seen=collections.Counter()
for path in re.findall(r'^\s*Running .*?\(([^\r\n]+)\)\s*$',native,re.M):
 name=re.sub(r'-[0-9a-f]{16}$','',Path(path).stem).replace('-','_');seen[name]+=1
coverage={'expectedTestableTargets':dict(expected),'observedExecutableNames':dict(seen),'expectedTestableTargetCount':sum(expected.values()),'observedInvocations':sum(seen.values()),'missing':{n:c-seen[n] for n,c in expected.items() if seen[n]<c},'failures':re.findall(r'^test (.+?) \.\.\. FAILED\s*$',native,re.M)};coverage['allTestableTargetsRan']=not coverage['missing'];save('native-target-coverage.json',coverage)
run('doctests',[cargo,'test','--workspace','--doc','--locked','--no-fail-fast','-j1'])
run('python-contracts',['python','-m','unittest','discover','-s','scripts/tests','-v'])
run('typescript-typecheck',['npm.cmd','run','typecheck']);run('typescript-tests',['npm.cmd','test'])
run('actual-directory-provider',[cargo,'test','--locked','-j1','--test','directory_move_live','--','--ignored','--nocapture','--test-threads=1'])
save('tested-native-binary.json',{'path':'target/debug/yoke.exe','sha256':hashlib.sha256((root/'target/debug/yoke.exe').read_bytes()).hexdigest()});after=manifest();save('source-after.json',after);totals={k:sum(g[k] for g in next(r for r in receipts if r['gate']=='whole-native')['resultGroups']) for k in ['passed','failed','ignored']};summary={'allChecksPassed':all(r['exitCode']==0 for r in receipts),'sourceUnchanged':before==after,'wholeNativeRun':True,'allTestableNativeTargetsRan':coverage['allTestableTargetsRan'],'nativeTargetCoverage':coverage,'wholeNativeTotals':totals,'checks':receipts,'scope':'Full default Windows native all-target run after finite captured directory selection, source-bound standard-LSP folder repair, guarded physical rename, copied directory-shape verification and conditional recovery. Test fixtures run serially under the current Windows CI profile while in-test concurrency adversaries remain intact. The actual installed rust-analyzer/rustc gate is separate and nonadditive. Ignored host gates, OS prerequisites, broader STATUS/R1-R9 work, upstream/platform certification, packaging, main merge and release remain open.'};save('summary.json',summary);print(json.dumps({'finished':True,'allChecksPassed':summary['allChecksPassed'],'sourceUnchanged':summary['sourceUnchanged'],'allTestableNativeTargetsRan':coverage['allTestableTargetsRan'],'wholeNativeTotals':totals}),flush=True)
raise SystemExit(not(summary['allChecksPassed'] and summary['sourceUnchanged'] and coverage['allTestableTargetsRan']))
