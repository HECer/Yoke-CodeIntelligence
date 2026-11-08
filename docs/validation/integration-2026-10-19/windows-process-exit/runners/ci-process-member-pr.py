from pathlib import Path
import subprocess,json,hashlib,base64,datetime,time
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');repo='HECer/Yoke-CodeIntelligence';branch='integrate/native-parity-github-app-20261018';sha=lambda b:hashlib.sha256(b).hexdigest()
def gh(*args):return subprocess.check_output(['gh',*args],cwd=root)
head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root).decode().strip();path='docs/validation/integration-2026-10-19/windows-process-exit/index.json';raw=(root/path).read_bytes();index=json.loads(raw);time.sleep(2)
pr=json.loads(gh('pr','view','7','--repo',repo,'--json','headRefOid,isDraft,state,baseRefName,body'));assert pr['state']=='OPEN' and pr['isDraft'] and pr['baseRefName']=='main' and pr['headRefOid']==head
old=pr['body'].encode();assert len(old)==36867 and sha(old)=='9983fe1541001369adb22f5991811330a729ff0dc82ae48ea2aeee89d6bcdd69';assert b'<!-- held-members-20261019:start -->' not in old
t=index['nativeTotals'];addition=f'''

<!-- held-members-20261019:start -->
### Windows held-member cleanup checkpoint

Source `{index['sourceCommit']}` / runtime merge `{index['runtimeCommit']}`
strengthen explicit Windows process cleanup. Before asynchronous termination,
the native owner captures up to4,096 job-member synchronization handles. Success
requires their final signals, the independently held root's exit and empty job
accounting within the same existing cleanup budget. Missing/incomplete/over-budget
member admission still requests owned termination and reports missing proof.
There is no PID-based kill; PID reuse can cause conservative waiting/unproven
cleanup only. Original permissions, time/liveness and SQLite/source restoration
assertions remain. Failure diagnostics now retain the original cleanup outcome.

Diagnostic R2 reproduces **3 passed / 1 failed**: a held owned process remained
nonsignaled at a normal deadline return with cleanupUnproven=false. Earlier8
same-binary and4temporary-trace passes are nonadditive; they do not certify a
kernel scheduling cause. Temporary tracing is absent from final source.

All **17 targeted cases** pass, including2new real OS permission/ownership
contracts. The final15selectednativegroups record **{t['passed']} passed /
{t['failed']} failed / {t['ignored']} existing ignored gates**; the sole unchanged media
symlink setupOS1314 remains. AlltargetClippy-Dwarnings, format, Python17,
mandatory typecheck and TypeScript260/3skip pass with **{index['nativeInputs']} native /
{index['integratedInputs']} integrated inputs unchanged**. Actual Windows PostgreSQL18.6
TLS/SCRAM/Unicode capture, all3owned credential removals, lost-stdout identity and
offline traversal after server stop pass on the new binary. Frozen W3PDF native
worker text/replay passes without reacquisition. Provider protocols/dependencies
are unchanged. Cache cleanup now durably records each planned hash before deletion.

[Frozen evidence](https://github.com/{repo}/blob/{head}/{path}):
**{len(index['artifacts'])} raw hash-checked artifacts**, index SHA-256 `{sha(raw)}`.
Historical inventory and preceding PR/README bytes remain. This finite observed
member/OS proof is not a sandbox, a statement about every historical transient
process, hard kernel-API preemption or full native/upstream/non-Windows/main/release
qualification. Remaining STATUS/R1–R9 work stays open.
<!-- held-members-20261019:end -->
'''.encode();body=old+addition;file=Path.home()/'AppData/Local/Temp/ci-pr-7-body-held-members-final.md';file.write_bytes(body)
subprocess.run(['gh','pr','edit','7','--repo',repo,'--body-file',str(file)],cwd=root,check=True,stdout=subprocess.PIPE)
after=json.loads(gh('pr','view','7','--repo',repo,'--json','headRefOid,isDraft,state,baseRefName,body'));assert after['headRefOid']==head and after['isDraft'] and after['state']=='OPEN' and after['body'].encode()==body and body.startswith(old)
ref=json.loads(gh('api','repos/'+repo+'/git/ref/heads/'+branch));assert ref['object']['sha']==head
content=json.loads(gh('api','repos/'+repo+'/contents/'+path+'?ref='+head));assert base64.b64decode(content['content'])==raw
proof={'verifiedAt':datetime.datetime.now(datetime.timezone.utc).isoformat(),'url':'https://github.com/'+repo+'/pull/7','head':head,'state':after['state'],'isDraft':after['isDraft'],'base':after['baseRefName'],'bodyBytes':len(body),'bodySha256':sha(body),'preservedPrefixBytes':len(old),'preservedPrefixSha256':sha(old),'indexSha256':sha(raw),'remoteBranchVerified':True,'remoteIndexRawBytesVerified':True,'ciPolled':False};(root/'validation/integration-20261018/windows-process-exit/remote-verification.json').write_bytes((json.dumps(proof,indent=2)+'\n').encode());print(json.dumps(proof))
