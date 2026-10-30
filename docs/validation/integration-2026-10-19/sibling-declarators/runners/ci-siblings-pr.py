from pathlib import Path
import subprocess,json,hashlib,base64,time,datetime
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence')
base=root/'validation/integration-20261018/sibling-declarators'
archive=root/'docs/validation/integration-2026-10-19/sibling-declarators'
temp=Path(__file__).parent;repo='HECer/Yoke-CodeIntelligence';branch='integrate/native-parity-github-app-20261018'
sha=lambda b:hashlib.sha256(b).hexdigest()
def run(args):return subprocess.check_output(args,cwd=root)
def api(path):return json.loads(run(['gh','api',path]))
assert not run(['git','status','--porcelain'])
head=run(['git','rev-parse','HEAD']).decode().strip();source=json.loads((base/'source-integration.json').read_bytes())
index_raw=(archive/'index.json').read_bytes();old=(temp/'ci-pr-7-body-python-empty-suite-final.md').read_bytes()
assert len(old)==52623 and sha(old)=='7b572e5cf7cf724755bdcff17db0a4ba0a90a796635c4fdf21bb597f29875e70'
before=api('repos/'+repo+'/pulls/7')
assert before['head']['sha']==source['parentCommit'] and before['body'].encode()==old
assert before['state']=='open' and before['draft'] and before['base']['ref']=='main'
marker='<!-- sibling-declarators-20261020:start -->';assert marker.encode() not in old
addition=f'''\n\n{marker}
### Explicit native TS/JS sibling-declarator deletion — Code Intelligence native0.5.0 development

Adds closed native deleteSymbol.siblingDeclarators:reject|preserve. The strict
absent/reject and semantic-provider/facade defaults remain. Explicit preservation
binds one simple TS/TSX/JS/JSX function-valued variable in a const/let/var statement,
then removes that original AST node and one direct parent comma as separate byte
edits. Surviving declarators, wrappers, semicolon, whitespace and original entity/
evidence coordinates remain. Direct ambiguous comments, unsupported targets,
mixed edits and combined policies refuse; complete-result syntax is rechecked
under original source/AST/deadline limits. Ordinary immutable-plan/application
authority and identity stay unchanged.

- Source `{source['sourceCommit']}`; runtime merge `{source['runtimeCommit']}`.
- **10 new cases:**1unit admission,8default integration,1actual compiler gate.
- Final **11 selected native groups:568 passed / 0 failed / 9 ignored**,
  including **467/0/7 library**. **734 native inputs** and the exact binary
  remain frozen. Earlier 15/0/1 and26/0/1 runs are nonadditive.
- Actual **Node24.21.0 / TypeScript5.8.3** compile copied TS,JS,TSX,JSX results,
  followed by independent verification/permission/approval/application/recovery
  checks. No analyzed module executes. The existing Python AST-only application/
  recovery gate also passes on the identical final734-input freeze.
- Format/build/all-target Clippy-Dwarnings, mandatory typecheck, **260 TypeScript
  tests / 3 skips** and **17 Python tests** pass with **740 integrated inputs**
  unchanged. Native MCP prepares the exact planned effect with no model/write/
  execution grants.
- [Contract](https://github.com/{repo}/blob/{head}/native/docs/symbol-deletion.md)
  and [indexed evidence](https://github.com/{repo}/blob/{head}/docs/validation/integration-2026-10-19/sibling-declarators/index.json).
  Index SHA-256 `{sha(index_raw)}`.

This finite source/syntax policy does not prove unusedness, consumer safety,
compiler reference completeness, runtime behavior or general semantic/IDE
equivalence. Earlier OS1314 prerequisites remain outside this selection; no
whole-native/platform/non-Windows/main/release acceptance follows. All previous
52623body bytes and historical inventory/ledger remain; broaderR1-R9 work continues.
<!-- sibling-declarators-20261020:end -->
'''.encode()
body=old+addition;assert len(body)<65536
body_file=temp/'ci-pr-7-body-sibling-declarators-final.md';body_file.write_bytes(body)
subprocess.run(['git','push','origin',f'HEAD:refs/heads/{branch}'],cwd=root,check=True)
subprocess.run(['gh','pr','edit','7','--repo',repo,'--body-file',str(body_file)],cwd=root,check=True)
time.sleep(2)
after=api('repos/'+repo+'/pulls/7');actual=after['body'].encode()
assert after['head']['sha']==head and actual==body and actual.startswith(old)
assert after['state']=='open' and after['draft'] and after['base']['ref']=='main'
assert run(['git','ls-remote','--heads','origin',f'refs/heads/{branch}']).decode().split()[0]==head
remote=api('repos/'+repo+'/contents/docs/validation/integration-2026-10-19/sibling-declarators/index.json?ref='+branch)
assert base64.b64decode(remote['content'])==index_raw
receipt={'verifiedAt':datetime.datetime.now(datetime.timezone.utc).isoformat(),'url':after['html_url'],
    'head':head,'branch':branch,'state':'open','draft':True,'base':'main',
    'bodyBytes':len(actual),'bodySha256':sha(actual),'priorBodyBytes':len(old),'priorBodySha256':sha(old),
    'priorBodyPreserved':True,'indexSha256':sha(index_raw),'remoteIndexRawEqual':True,
    'sourceCommit':source['sourceCommit'],'runtimeCommit':source['runtimeCommit'],'mainOrReleaseVerified':False}
(base/'remote-verification.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8')
print(json.dumps(receipt),flush=True)
