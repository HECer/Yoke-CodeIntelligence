from pathlib import Path
import subprocess,json,hashlib,base64,time,datetime
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence')
base=root/'validation/integration-20261018/member-decorators';archive=root/'docs/validation/integration-2026-10-19/member-decorators'
temp=Path(__file__).parent;repo='HECer/Yoke-CodeIntelligence';branch='integrate/native-parity-github-app-20261018'
sha=lambda b:hashlib.sha256(b).hexdigest()
def run(args):return subprocess.check_output(args,cwd=root)
def api(path):return json.loads(run(['gh','api',path]))
assert not run(['git','status','--porcelain'])
head=run(['git','rev-parse','HEAD']).decode().strip();source=json.loads((base/'source-integration.json').read_bytes())
index_raw=(archive/'index.json').read_bytes();old=(temp/'ci-pr-7-body-sibling-declarators-final.md').read_bytes()
assert len(old)==55276 and sha(old)=='9363bc62d7259ee6c5c2224e9c894d451caea8f4ca05496690abf08a054cc109'
before=api('repos/'+repo+'/pulls/7')
assert before['head']['sha']==source['parentCommit'] and before['body'].encode()==old
assert before['state']=='open' and before['draft'] and before['base']['ref']=='main'
marker='<!-- member-decorators-20261020:start -->';assert marker.encode() not in old
addition=f'''\n\n{marker}
### Concrete class-method decorator ownership — Code Intelligence native0.5.0 development

Adds a checked owning deletion range for concrete plain-named TS/TSX/JS/JSX
methods. Up to64 preceding direct sibling decorators and intervening comment/
whitespace gaps bind to the selected method after exact original AST/name/kind/
range checks. Parent/neighbor decorations, earlier comments and original entity/
evidence coordinates stay intact. Unsupported gaps, constructors, abstract/
signature and computed/quoted/private decorated names refuse. Full-result syntax
replay uses original source/AST/deadline bounds. Native and provider safeDelete
final preparation share this boundary; no new input or host authority is added.

- Source `{source['sourceCommit']}`; runtime merge `{source['runtimeCommit']}`.
- **11 new cases:**1unit,8default integration,2explicit actual gates.
- Final **12 selected native groups:577 passed / 0 failed / 11 ignored**,
  including **468/0/7 library**. **736 native inputs** and exact production
  binary remain unchanged within their frozen runs.
- Actual **TypeScript5.8.3** compiler/AST verifies TS/TSX/JS/JSX and class/neighbor
  decoration; **language-server5.1.3** safeDelete produces the same checked plan.
  Permission/approval/application/recovery pass in owned experimentalDecorators
  fixtures. Analyzed modules/decorator expressions never execute. Existing
  sibling and Python gates also pass on the identical final source.
- Format/build/all-target Clippy-Dwarnings, mandatory typecheck, **260 TypeScript
  tests / 3 skips** and **17 Python tests** pass with **742 integrated inputs**
  unchanged. Initial invalid formatter-overlap baseline and missing-entity test
  assumption remain recorded with corrected sequential proofs; runs are nonadditive.
- [Contract](https://github.com/{repo}/blob/{head}/native/docs/symbol-deletion.md)
  and [indexed evidence](https://github.com/{repo}/blob/{head}/docs/validation/integration-2026-10-19/member-decorators/index.json).
  Index SHA-256 `{sha(index_raw)}`.

Saved-plan identity, insert/replace ranges and independent usage/write/approval/
verification/stale/recovery gates remain. Other member/name/language forms,
decorator modes/versions/transforms and complete consumer/compiler/dispatch/
runtime/IDE guarantees remain open. Earlier OS1314 prerequisites lie outside
this selection; no whole-native/platform/non-Windows/main/release acceptance
follows. All previous55276body bytes and historical inventory/ledger remain;
broaderR1-R9 work continues.
<!-- member-decorators-20261020:end -->
'''.encode()
body=old+addition;assert len(body)<65536
body_file=temp/'ci-pr-7-body-member-decorators-final.md';body_file.write_bytes(body)
subprocess.run(['git','push','origin',f'HEAD:refs/heads/{branch}'],cwd=root,check=True)
subprocess.run(['gh','pr','edit','7','--repo',repo,'--body-file',str(body_file)],cwd=root,check=True)
time.sleep(2)
after=api('repos/'+repo+'/pulls/7');actual=after['body'].encode()
assert after['head']['sha']==head and actual==body and actual.startswith(old)
assert after['state']=='open' and after['draft'] and after['base']['ref']=='main'
assert run(['git','ls-remote','--heads','origin',f'refs/heads/{branch}']).decode().split()[0]==head
remote=api('repos/'+repo+'/contents/docs/validation/integration-2026-10-19/member-decorators/index.json?ref='+branch)
assert base64.b64decode(remote['content'])==index_raw
receipt={'verifiedAt':datetime.datetime.now(datetime.timezone.utc).isoformat(),'url':after['html_url'],
    'head':head,'branch':branch,'state':'open','draft':True,'base':'main',
    'bodyBytes':len(actual),'bodySha256':sha(actual),'priorBodyBytes':len(old),'priorBodySha256':sha(old),
    'priorBodyPreserved':True,'indexSha256':sha(index_raw),'remoteIndexRawEqual':True,
    'sourceCommit':source['sourceCommit'],'runtimeCommit':source['runtimeCommit'],'mainOrReleaseVerified':False}
(base/'remote-verification.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8')
print(json.dumps(receipt),flush=True)
