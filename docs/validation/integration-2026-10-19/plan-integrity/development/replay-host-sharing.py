from pathlib import Path
import subprocess,json,re,hashlib,datetime,os,time
root=Path.cwd();out=root/'validation/integration-20261018/plan-integrity-development/host-sharing-serial';out.mkdir(exist_ok=False)
final=root/'validation/integration-20261018/plan-integrity-final-r2';assert (final/'summary.json').is_file(),'Wait for the complete original runner'
frozen=json.loads((final/'source-before.json').read_bytes())
def unchanged():return all(hashlib.sha256((root/n).read_bytes()).hexdigest()==h for n,h in frozen.items())
assert unchanged();raw=(final/'whole-native.log').read_text(encoding='utf-8',errors='replace');receipts=[]
env=os.environ.copy();pwsh=Path.home()/'AppData/Local/YokeValidation/tools/powershell-7.6.7-win-x64';env['PATH']=str(pwsh)+os.pathsep+env['PATH']
for name in ['host_install','host_install_cli']:
 m=re.search(r'Running tests[\\/]'+name+r'\.rs \(([^\r\n]+)\)',raw);assert m,name
 exe=(root/m[1]).resolve();assert exe.parent==root/'target/debug/deps';before=hashlib.sha256(exe.read_bytes()).hexdigest()
 command=[str(exe),'--test-threads=1'];started=time.monotonic()
 with (out/(name+'.log')).open('wb') as stream:r=subprocess.run(command,env=env,stdout=stream,stderr=subprocess.STDOUT,timeout=600)
 text=(out/(name+'.log')).read_text(encoding='utf-8',errors='replace');groups=[dict(zip(['passed','failed','ignored'],map(int,x))) for x in re.findall(r'^test result: \w+\. (\d+) passed; (\d+) failed; (\d+) ignored;',text,re.M)]
 assert before==hashlib.sha256(exe.read_bytes()).hexdigest()
 receipt={'target':name,'command':command,'executableSha256':before,'exitCode':r.returncode,'elapsedSeconds':round(time.monotonic()-started,3),'resultGroups':groups};receipts.append(receipt);print(json.dumps(receipt),flush=True)
assert unchanged()
report={'recordedAt':datetime.datetime.now(datetime.timezone.utc).isoformat(),'scope':'Separate serial replay of complete Windows host-install targets after actual error32 failures in the two-thread whole run. Same executable bytes, unchanged source, default TEMP/TMP and original test deadlines; only test concurrency differs. No namespace/sharing/permission policy or test assertion is weakened. Counts are nonadditive and original whole-run failures remain recorded.','allCasesPassed':all(r['exitCode']==0 for r in receipts),'sourceUnchanged':True,'receipts':receipts}
(out/'index.json').write_bytes((json.dumps(report,indent=2)+'\n').encode('utf-8'));raise SystemExit(not report['allCasesPassed'])
