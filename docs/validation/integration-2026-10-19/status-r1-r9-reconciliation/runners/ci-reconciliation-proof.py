from pathlib import Path
import subprocess,json,hashlib,re,time
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');root=w.parent/'Yoke-CodeIntelligence';out=root/'validation/integration-20261018/status-r1-r9-reconciliation'
sha=lambda b:hashlib.sha256(b).hexdigest()
def git(cwd,*a):return subprocess.check_output(['git','-c','core.longpaths=true',*a],cwd=cwd,stderr=subprocess.PIPE)
assert git(root,'rev-parse','HEAD').decode().strip()=='6181fcb01522de2e1d82b7aa6c22d72a1ab65f1b' and not git(root,'status','--porcelain')
allowed={'docs/REMAINING_WORK.md','docs/STATUS.md','docs/parity/native-reconciliation-2026-10-15.json','native/docs/github-app-workers.md'}
changed=set(git(w,'diff','--name-only').decode().splitlines())|set(git(w,'ls-files','-o','--exclude-standard').decode().splitlines());assert changed==allowed,changed^allowed
before=json.loads((out/'tracker-before.json').read_bytes());after=json.loads((out/'tracker-after.json').read_bytes())
assert (w/'docs/parity/native-reconciliation-2026-10-15.json').read_bytes()==(out/'tracker-after.json').read_bytes()
for k in ['inventoryLedger','historicalInventory','ledgerSummary']:assert before[k]==after[k]
baseline=json.loads((root/'validation/integration-20261018/mixed-meaning-composition/integrated/source-after.json').read_bytes())
runtime={n:h for n,h in baseline.items() if n!='docs/parity/native-reconciliation-2026-10-15.json'}
def verify():
    rows=[]
    for n,h in runtime.items():
        a=(root/n).read_bytes();b=(w/n).read_bytes();assert sha(a)==h,n
        assert a==b or a.replace(b'\r\n',b'\n')==b.replace(b'\r\n',b'\n'),n
        rows.append({'path':n,'baselineSha256':h,'checkoutSha256':sha(b),'materialization':'raw_equal' if a==b else 'crlf_only_checkout_materialization'})
    return rows
rows=verify();(out/'runtime-source-proof.json').write_text(json.dumps(rows,indent=2)+'\n',encoding='utf-8')
doc_before={n:sha((w/n).read_bytes()) for n in sorted(allowed)}
command=[r'C:\Python314\python.exe','-m','unittest','discover','-s','scripts/tests','-v'];start=time.monotonic()
with (out/'python-contracts.log').open('wb') as log:p=subprocess.run(command,cwd=w,stdout=log,stderr=subprocess.STDOUT)
assert p.returncode==0 and 'Ran 17 tests' in (out/'python-contracts.log').read_text(encoding='utf-8')
assert verify()==rows and doc_before=={n:sha((w/n).read_bytes()) for n in sorted(allowed)}
result={'sourceParent':'6181fcb01522de2e1d82b7aa6c22d72a1ab65f1b','runtimeInputsMatched':len(rows),'rawEqual':sum(r['materialization']=='raw_equal' for r in rows),'crlfOnly':sum(r['materialization']!='raw_equal' for r in rows),'changedTrackerRows':['R2','R3','R8'],'inventoryLedgerUnchanged':True,'historicalInventoryUnchanged':True,'ledgerSummaryUnchanged':True,'noBehaviorChange':True,'nativeOrTypescriptTestsRerun':False,'pythonContracts':{'command':command,'exitCode':0,'passed':17,'seconds':round(time.monotonic()-start,2)},'documentHashesUnchangedDuringValidation':doc_before,'priorAttempt':'Initial recipe compared primary raw hashes with a differently materialized worktree and refused. No native/TypeScript test ran or failed; corrected proof compares raw bytes and explicitly records CRLF-only variants.','wholeGoalComplete':False}
(out/'reconciliation.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8');print(json.dumps({k:v for k,v in result.items() if k!='documentHashesUnchangedDuringValidation'}))
