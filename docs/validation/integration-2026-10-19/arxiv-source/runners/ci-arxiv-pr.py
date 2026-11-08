from pathlib import Path
import subprocess,json,hashlib,base64,datetime,time
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');repo='HECer/Yoke-CodeIntelligence';branch='integrate/native-parity-github-app-20261018';sha=lambda b:hashlib.sha256(b).hexdigest()
def gh(*args):return subprocess.check_output(['gh',*args],cwd=root)
head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root).decode().strip();path='docs/validation/integration-2026-10-19/arxiv-source/index.json';raw=(root/path).read_bytes();index=json.loads(raw)
# Allow the observed GraphQL head cache to settle; never poll CI.
time.sleep(2)
pr=json.loads(gh('pr','view','7','--repo',repo,'--json','headRefOid,isDraft,state,baseRefName,body'));assert pr['state']=='OPEN' and pr['isDraft'] and pr['baseRefName']=='main' and pr['headRefOid']==head
old=pr['body'].encode();assert len(old)==34193 and sha(old)=='e2350cd1184e9fdbe1aa64807b450c85b02459671d5f55bdf0f14a3af78602c8';assert b'<!-- received-arxiv-20261019:start -->' not in old
t=index['nativeTotals'];addition=f'''

<!-- received-arxiv-20261019:start -->
### Native coordinated arXiv source checkpoint

Source `{index['sourceCommit']}` / runtime merge `{index['runtimeCommit']}` add
host-only arxiv.org abstract/PDF capture with literal SocketAddr, verified TLS
name and separate network/output/rate-state grants. One exclusive shared journal
spans the owned request; completion starts a three-second cooldown. Pending or
corrupt state refuses. Explicit exact-hash pending recovery sends no request.
Operators must share the same coherent-lock file and UTC clocks; other clients,
different files and clock skew remain outside this mechanism's guarantee.

Frozen finite head-META inspection preserves distinct citation occurrences,
repeated author labels and exact received whole-tag body-byte ranges. XML/numeric
decoding and matching base-ID labels do not establish paper version, publication,
author identity, server truth, a browser DOM or canonical code source authority.
Generic HTTP continues to refuse arxiv domains. No new dependency, default model
call, project execution or query-time acquisition is added.

All **16 new contracts** pass. The final frozen selected native run records
**{t['passed']} passed / {t['failed']} failed / {t['ignored']} existing ignored gates** across
{index['nativeResultGroups']} groups. The evidence retains every failure and source hashes.
All-target Clippy with warnings denied, format, Python17, mandatory typecheck and
TypeScript260/3skip pass with **{index['frozenNativeInputs']} native /
{index['frozenIntegratedInputs']} integrated inputs unchanged**. Actual Windows native
metadata/PDF captures share journal completion sequences2/4, matching body/archive
hashes and six distinct author occurrences. Arxiv PDF text extraction is not claimed.

The initial unavailable API RED, genuine comment-boundary panic and correction,
two test-fixture mistakes, first Clippy warning and an earlier unchanged owned-Git
termination test failure remain nonadditive. Later passes do not erase failures
or establish their cause. Public page/PDF archives and full abstract output stay
ignored/local; published artifacts use hashes, receipts and project-owned fixtures.
Remaining Google/social/video/database and semantic connectors, wider R1–R9,
full native/platform/upstream qualification, main merge and release remain open.

[Frozen evidence](https://github.com/{repo}/blob/{head}/{path}):
**{len(index['artifacts'])} raw hash-checked artifacts**. Index SHA-256 `{sha(raw)}`.
Historical inventory/ledger identities and the complete preceding PR body remain.
<!-- received-arxiv-20261019:end -->
'''.encode();body=old+addition;file=Path.home()/'AppData/Local/Temp/ci-pr-7-body-arxiv-final.md';file.write_bytes(body)
subprocess.run(['gh','pr','edit','7','--repo',repo,'--body-file',str(file)],cwd=root,check=True,stdout=subprocess.PIPE)
after=json.loads(gh('pr','view','7','--repo',repo,'--json','headRefOid,isDraft,state,baseRefName,body'));assert after['headRefOid']==head and after['isDraft'] and after['state']=='OPEN' and after['body'].encode()==body and body.startswith(old)
ref=json.loads(gh('api','repos/'+repo+'/git/ref/heads/'+branch));assert ref['object']['sha']==head
content=json.loads(gh('api','repos/'+repo+'/contents/'+path+'?ref='+head));assert base64.b64decode(content['content'])==raw
proof={'verifiedAt':datetime.datetime.now(datetime.timezone.utc).isoformat(),'url':'https://github.com/'+repo+'/pull/7','head':head,'state':after['state'],'isDraft':after['isDraft'],'base':after['baseRefName'],'bodyBytes':len(body),'bodySha256':sha(body),'preservedPrefixBytes':len(old),'preservedPrefixSha256':sha(old),'indexSha256':sha(raw),'remoteBranchVerified':True,'remoteIndexRawBytesVerified':True,'ciPolled':False};(root/'validation/integration-20261018/arxiv-source/remote-verification.json').write_bytes((json.dumps(proof,indent=2)+'\n').encode());print(json.dumps(proof))
