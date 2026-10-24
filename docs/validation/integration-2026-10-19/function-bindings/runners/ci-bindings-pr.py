from pathlib import Path
import subprocess,json,hashlib,base64,time,datetime
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');base=root/'validation/integration-20261018/function-bindings'
archive=root/'docs/validation/integration-2026-10-19/function-bindings';temp=Path(__file__).parent
repo='HECer/Yoke-CodeIntelligence';branch='integrate/native-parity-github-app-20261018';sha=lambda b:hashlib.sha256(b).hexdigest()
def run(args):return subprocess.check_output(args,cwd=root)
def api(path):return json.loads(run(['gh','api',path]))
assert not run(['git','status','--porcelain'])
head=run(['git','rev-parse','HEAD']).decode().strip();source=json.loads((base/'source-integration.json').read_bytes());totals=source['nativeTotals']
index_raw=(archive/'index.json').read_bytes();old=(archive/'prior-pr-body.md').read_bytes()
assert len(old)==6151 and sha(old)=='39882b2bd084116b61c9a5065f0414666f91cd5c4efd475ebcbc65c069350f91'
before=api('repos/'+repo+'/pulls/7');assert before['head']['sha']==source['parentCommit'] and before['body'].encode()==old
assert before['state']=='open' and before['draft'] and before['base']['ref']=='main'
link=lambda p:f'https://github.com/{repo}/blob/{head}/{p}'
text=old.decode().replace('/blob/'+source['parentCommit']+'/', '/blob/'+head+'/')
start=text.index('### Latest addition: native session semantic plans');end=text.index('### Validation',start)
latest=f'''### Latest native semantic planning

The native session `refactor.prepare` route exposes existing safeDelete/rename/
inline/move plans through batch CLI and native session MCP, with mode/project/
current-record admission, closed arguments and separate application authority.
Its native arguments/results, normal eight-tool and published facade schemas
remain unchanged.

Explicit `captured_es_function_propagation_v3` adds single plain const/let/var
bindings initialized directly by arrows or anonymous function expressions.
Existing captured function entities bind exact variable-declarator/name ranges;
only their owning statement/export supplies the wider removal range. Original
entity/evidence/analyzer identities remain. V1/default and V2 retain their
previous module boundary. V2 call wrappers, surviving-AST correspondence, exact
provider/source proofs, dependency/cycle/export protection and resource limits
remain; surviving assignment/value consumers cannot be forced away.

Source `{source['sourceCommit']}`; runtime merge `{source['runtimeCommit']}`.
The new v3 contract has 10 default cases and 1 actual CLI gate. Coverage includes
both analyzer epochs/three keywords, immutable base Graph and byte ranges,
unsupported declarations/consumers, retained dependencies, exact declaration/
usage quotas, missing/tampered proofs and automatic-semicolon boundaries.
Actual native CLI/TypeScript 5.8.3/language-server 5.1.3 qualify TS/JS const/let/var,
async arrows and Greek names, copied compilation and exact apply/recovery.
Unsupported shapes and dynamic/external consumers remain explicit.

'''
text=text[:start]+latest+text[end:]
lines=[]
for line in text.splitlines(True):
    if line.startswith('|15 selected native groups|'):
        line=f"|15 selected native groups|{totals['passed']} passed / {totals['failed']} failed / {totals['ignored']} ignored; 738 inputs unchanged|\n"
    elif line.startswith('|Actual native CLI|'):
        line='|Actual native CLI|TypeScript 5.8.3 / language-server 5.1.3, v2/v3 TS/JS; copied compilation and guarded apply/recovery pass|\n'
    lines.append(line)
text=''.join(lines)
start=text.index('- [Current STATUS');end=text.index('### Remaining boundaries',start)
links=f'''- [Current STATUS and remaining work]({link('docs/STATUS.md')})
- [Function binding contract]({link('native/docs/propagated-delete.md')}#explicit-function-binding-policy)
- [Native session contract]({link('native/docs/agent-sessions.md')}#native-semantic-planning)
- [Latest indexed raw evidence]({link('docs/validation/integration-2026-10-19/function-bindings/index.json')})
  — SHA-256 `{sha(index_raw)}`.
- [All integration evidence and retained failures]({link('docs/validation/integration-2026-10-19/README.md')})
- [Inventory and reconciliation boundaries]({link('docs/parity/native-reconciliation-2026-10-15.json')})
- [Archived prior PR description]({link('docs/validation/integration-2026-10-19/function-bindings/prior-pr-body.md')})
  retains all 6151 prior body bytes, SHA-256 `{sha(old)}`.

'''
body=(text[:start]+links+text[end:]).encode();assert len(body)<65536
body_file=temp/'ci-pr-7-body-function-bindings-final.md';body_file.write_bytes(body)
subprocess.run(['git','push','origin',f'HEAD:refs/heads/{branch}'],cwd=root,check=True)
subprocess.run(['gh','pr','edit','7','--repo',repo,'--body-file',str(body_file)],cwd=root,check=True)
time.sleep(2);after=api('repos/'+repo+'/pulls/7');actual=after['body'].encode()
assert after['head']['sha']==head and actual==body
assert after['state']=='open' and after['draft'] and after['base']['ref']=='main'
assert run(['git','ls-remote','--heads','origin',f'refs/heads/{branch}']).decode().split()[0]==head
remote=api('repos/'+repo+'/contents/docs/validation/integration-2026-10-19/function-bindings/index.json?ref='+branch)
assert base64.b64decode(remote['content'])==index_raw
prior=api('repos/'+repo+'/contents/docs/validation/integration-2026-10-19/function-bindings/prior-pr-body.md?ref='+branch)
assert base64.b64decode(prior['content'])==old
receipt={'verifiedAt':datetime.datetime.now(datetime.timezone.utc).isoformat(),'url':after['html_url'],'head':head,'branch':branch,
    'state':'open','draft':True,'base':'main','bodyBytes':len(actual),'bodySha256':sha(actual),
    'priorBodyBytes':len(old),'priorBodySha256':sha(old),'priorBodyArchivedRawEqual':True,'priorBodyPreservedInCurrentBody':False,
    'descriptionConsolidatedAroundIntegratedBehavior':True,'indexSha256':sha(index_raw),'remoteIndexRawEqual':True,
    'sourceCommit':source['sourceCommit'],'runtimeCommit':source['runtimeCommit'],'mainOrReleaseVerified':False}
(base/'remote-verification.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8');print(json.dumps(receipt),flush=True)
