from pathlib import Path
import os, subprocess, json, hashlib, time, datetime, re

root=Path.cwd()
prior=root/'validation/integration-20261018/windows-environment-r1'
assert (prior/'summary.json').is_file(), 'The first validation process must finish before follow-up'
before=json.loads((prior/'source-before.json').read_text(encoding='utf-8'))
for n,h in before.items():
    assert hashlib.sha256((root/n).read_bytes()).hexdigest()==h,n
out=root/'validation/integration-20261018/windows-environment-r2'
out.mkdir(exist_ok=False)
env=os.environ.copy()
cargo=str(Path.home()/'.cargo/bin/cargo.exe')
env['PATH']=str(Path(cargo).parent)+os.pathsep+env['PATH']
env['YOKE_TS_LSP']=str(Path.home()/'AppData/Local/YokeValidation/typescript-5.8.3-lsp-5.1.3/node_modules/typescript-language-server/lib/cli.mjs')
env['YOKE_PYTHON_LSP']=str(Path.home()/'AppData/Local/YokeValidation/jedi-0.47.0/Scripts/jedi-language-server.exe')
assert Path(env['YOKE_TS_LSP']).is_file() and Path(env['YOKE_PYTHON_LSP']).is_file()
receipts=[]
checks=[('remaining-runtime',[cargo,'test','--locked','-j2','--test','runtime','--','--test-threads=2']),('actual-installed-language-servers',[cargo,'test','--locked','-j2','--test','live','--','--ignored','--test-threads=2'])]
for label,cmd in checks:
    print(json.dumps({'gate':label,'state':'running'}),flush=True)
    start=time.monotonic()
    with (out/(label+'.log')).open('wb') as log:
        result=subprocess.run(cmd,env=env,stdout=log,stderr=subprocess.STDOUT,timeout=300)
    log=(out/(label+'.log')).read_text(encoding='utf-8',errors='replace')
    groups=[dict(zip(['passed','failed','ignored'],map(int,m))) for m in re.findall(r'test result: \w+\. (\d+) passed; (\d+) failed; (\d+) ignored;',log)]
    receipt={'gate':label,'command':cmd,'exitCode':result.returncode,'elapsedSeconds':round(time.monotonic()-start,3),'recordedAt':datetime.datetime.now(datetime.timezone.utc).isoformat(),'resultGroups':groups}
    receipts.append(receipt)
    (out/'receipts.json').write_text(json.dumps(receipts,indent=2)+'\n',encoding='utf-8')
    print(json.dumps(receipt),flush=True)
for n,h in before.items():
    assert hashlib.sha256((root/n).read_bytes()).hexdigest()==h,n
summary={'allChecksPassed':all(r['exitCode']==0 for r in receipts),'sourceUnchanged':True,'sourceReference':'../windows-environment-r1/source-before.json','checks':receipts,'scope':'Separate tests after the first command stopped at the original Windows OS1314 media fixture setup failure. No security fixture is suppressed.'}
(out/'summary.json').write_text(json.dumps(summary,indent=2)+'\n',encoding='utf-8')
raise SystemExit(not summary['allChecksPassed'])
