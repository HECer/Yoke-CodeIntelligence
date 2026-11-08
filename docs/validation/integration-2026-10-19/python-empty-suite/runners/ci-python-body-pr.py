from pathlib import Path
import subprocess,json,hashlib,base64,time,datetime
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence')
base=root/'validation/integration-20261018/python-empty-suite'
archive=root/'docs/validation/integration-2026-10-19/python-empty-suite'
temp=Path(__file__).parent;repo='HECer/Yoke-CodeIntelligence';branch='integrate/native-parity-github-app-20261018'
sha=lambda b:hashlib.sha256(b).hexdigest()
def run(args):return subprocess.check_output(args,cwd=root)
def api(path):return json.loads(run(['gh','api',path]))
assert not run(['git','status','--porcelain'])
head=run(['git','rev-parse','HEAD']).decode().strip()
source=json.loads((base/'source-integration.json').read_bytes());index_raw=(archive/'index.json').read_bytes()
old=(temp/'ci-pr-7-body-plan-feedback-final.md').read_bytes()
assert len(old)==49791 and sha(old)=='2680b78997dd48e8828dea3a711bd9e699fee30ab403d841e1f1ad59a129de3e'
before=api('repos/'+repo+'/pulls/7')
assert before['head']['sha']==source['parentCommit'] and before['body'].encode()==old
assert before['state']=='open' and before['draft'] and before['base']['ref']=='main'
marker='<!-- python-empty-suite-20261020:start -->';assert marker.encode() not in old
addition=f'''\n\n{marker}
### Explicit native Python empty-suite deletion — native Code Intelligence0.5.0 development

Adds closed deleteSymbol.emptyPythonBody:reject|pass to native preparation.
Absent/reject keeps the released strict default. Explicitpass fills a required
Python block only when no other non-comment statement remains, after exact
AST/name/kind/range/decorator binding and before full-result syntax replay.
Original coordinates, BOM/Unicode/CRLF/indentation and neighbor comments stay
intact. Other languages, invalid policy data and mixed edits refuse. Existing
Plan identities, semantic-provider/facade defaults and independent write,
approval, executed verification, stale-source and recovery gates remain.

- Source `{source['sourceCommit']}`; runtime merge `{source['runtimeCommit']}`.
- **10 new cases:**1unit admission,8default integration,1explicit actual-Python gate.
- Final **10 selected native groups:559 passed / 0 failed / 8 ignored**.
  Exact **732 native inputs** remain unchanged. This is a finite selection;
  earlier OS1314 prerequisites are retained separately, with no whole-native pass.
- Actual **Python3.14.7** performs stdlib AST-only copied-result verification,
  then the fixture checks independent permission/approval/application/recovery.
  The analyzed module and unknown decorators never execute. A native MCP request
  prepares the exact plan without application or execution authority.
- Format/build/all-target Clippy-Dwarnings and mandatory typecheck, **260
  TypeScript tests / 3 skips**, **17 Python tests** pass with **738 integrated
  inputs** unchanged. Prior first-test error-envelope assumption and broad
  **558/1/8** attempt remain visible. Its sole idle-HTTP fixture failure required
  full20ms budget consumption; the test now checks the configured original20ms
  socket ceiling and unchanged250ms upper bound. HTTP runtime code and exact
  production binary are unchanged across that test-only correction.
- [Contract](https://github.com/{repo}/blob/{head}/native/docs/semantic-refactor-intents.md#explicit-native-python-empty-body-deletion)
  and [indexed evidence](https://github.com/{repo}/blob/{head}/docs/validation/integration-2026-10-19/python-empty-suite/index.json).
  Index SHA-256 `{sha(index_raw)}`.

The option is a finite singleton native source/syntax contract. It does not
establish consumer safety, general semantic/IDE equivalence, non-Windows or
whole-platform acceptance, main or release. Historical inventory/ledger and all
previous49791body bytes remain preserved; broaderR1-R9 work continues.
<!-- python-empty-suite-20261020:end -->
'''.encode()
body=old+addition;assert len(body)<65536
body_file=temp/'ci-pr-7-body-python-empty-suite-final.md';body_file.write_bytes(body)
subprocess.run(['git','push','origin',f'HEAD:refs/heads/{branch}'],cwd=root,check=True)
subprocess.run(['gh','pr','edit','7','--repo',repo,'--body-file',str(body_file)],cwd=root,check=True)
time.sleep(2)
after=api('repos/'+repo+'/pulls/7');actual=after['body'].encode()
assert after['head']['sha']==head and actual==body and actual.startswith(old)
assert after['state']=='open' and after['draft'] and after['base']['ref']=='main'
assert run(['git','ls-remote','--heads','origin',f'refs/heads/{branch}']).decode().split()[0]==head
remote=api('repos/'+repo+'/contents/docs/validation/integration-2026-10-19/python-empty-suite/index.json?ref='+branch)
assert base64.b64decode(remote['content'])==index_raw
receipt={'verifiedAt':datetime.datetime.now(datetime.timezone.utc).isoformat(),'url':after['html_url'],
    'head':head,'branch':branch,'state':'open','draft':True,'base':'main',
    'bodyBytes':len(actual),'bodySha256':sha(actual),'priorBodyBytes':len(old),'priorBodySha256':sha(old),
    'priorBodyPreserved':True,'indexSha256':sha(index_raw),'remoteIndexRawEqual':True,
    'sourceCommit':source['sourceCommit'],'runtimeCommit':source['runtimeCommit'],'mainOrReleaseVerified':False}
(base/'remote-verification.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8')
print(json.dumps(receipt),flush=True)
