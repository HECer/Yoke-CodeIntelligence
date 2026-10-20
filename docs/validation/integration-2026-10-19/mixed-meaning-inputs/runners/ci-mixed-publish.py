from pathlib import Path
import subprocess,json,hashlib,base64,datetime
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence')
base=root/'validation/integration-20261018/mixed-meaning-inputs';temp=Path(__file__).parent
repo='HECer/Yoke-CodeIntelligence';branch='integrate/native-parity-github-app-20261018'
sha=lambda b:hashlib.sha256(b).hexdigest()
def run(args):return subprocess.check_output(args,cwd=root,stderr=subprocess.PIPE)
def api(route):return json.loads(run(['gh','api',route]))
head=run(['git','rev-parse','HEAD']).decode().strip();assert not run(['git','status','--porcelain'])
source=json.loads((base/'source-integration.json').read_bytes());t=source['nativeTotals']
index_path='docs/validation/integration-2026-10-19/mixed-meaning-inputs/index.json'
index_raw=(root/index_path).read_bytes();index=json.loads(index_raw)
assert index['sourceCommit']==source['sourceCommit'] and index['integrated']['allChecksPassed']
old=(root/'docs/validation/integration-2026-10-19/mixed-meaning-inputs/prior-pr-body.md').read_bytes()
before=api('repos/'+repo+'/pulls/7');assert before['body'].encode()==old and before['state']=='open' and before['draft']
assert before['head']['ref']==branch and before['base']['ref']=='main'
link=lambda p:'https://github.com/'+repo+'/blob/'+head+'/'+p
text=old.decode();start=text.index('### Latest native semantic planning');end=text.index('### Validation',start)
latest=f'''### Latest checked mixed-source inputs

`mixed-view --meaning-inputs HOST_JSON` and the opaque library capability prepare
exact native UTF-8, Office decoded/XML and explicitly permitted PDF page reads.
Every unit retains its original typed coordinates and evidence; nested mixed
metadata preserves original native View/Meaning receipts and source-scope roles.
Ordered contexts retain repeated occurrences and count repeated bytes. Labels
assert no semantic identity. Closed requests, unit/item/context/output budgets,
UTF-8 boundaries and the existing PDF permission remain enforced before output.
No provider/model call, acquisition, input persistence or graph creation is added.
Mixed semantic execution and general cross-source graph links remain open.

Source `{source['sourceCommit']}`; runtime merge `{source['runtimeCommit']}`.
Seven new default cases include the actual native PDF worker/CLI case. Exact
legacy-read equality, original receipt retention, live-file removal, label/order
identity, 16/17 selection limits, repeated context-byte capacity, strict JSON,
preinitialization refusal, output boundary and deadline checks are covered.

The prior native session semantic-planning route and explicit function-binding
propagation v3 remain; their separate indexed TS5.8.3/LSP5.1.3 qualifications
retain guarded application/recovery and the original default/v1/v2 boundaries.

'''
text=text[:start]+latest+text[end:]
text=text.replace('|15 selected native groups|592 passed / 0 failed / 13 ignored; 738 inputs unchanged|',
    f"|13 latest selected native groups|{t['passed']} passed / {t['failed']} failed / {t['ignored']} ignored; {index['finalNative']['inputs']} inputs unchanged|")
text=text.replace('|Actual native CLI|TypeScript 5.8.3 / language-server 5.1.3, v2/v3 TS/JS; copied compilation and guarded apply/recovery pass|',
    '|Actual native execution|Native PDF worker/CLI is included in the latest default scope; prior TS5.8.3/LSP5.1.3 v2/v3 CLI qualifications stay separate|')
start=text.index('- [Current STATUS');end=text.index('### Remaining boundaries',start)
links=f'''- [Current STATUS and remaining work]({link('docs/STATUS.md')})
- [Mixed-source input contract]({link('native/docs/mixed-views.md')})
- [Latest indexed raw evidence]({link(index_path)}) — SHA-256 `{sha(index_raw)}`.
- [Prior function-binding evidence]({link('docs/validation/integration-2026-10-19/function-bindings/index.json')})
- [All integration evidence and retained failures]({link('docs/validation/integration-2026-10-19/README.md')})
- [Inventory and reconciliation boundaries]({link('docs/parity/native-reconciliation-2026-10-15.json')})
- [Archived prior PR description]({link('docs/validation/integration-2026-10-19/mixed-meaning-inputs/prior-pr-body.md')})
  retains all {len(old)} prior body bytes, SHA-256 `{sha(old)}`.

'''
body=(text[:start]+links+text[end:]).encode();assert len(body)<8000
body_file=temp/'ci-pr-7-body-mixed-meaning-inputs-final.md';body_file.write_bytes(body)
run(['git','push','origin','HEAD:refs/heads/'+branch])
run(['gh','pr','edit','7','--repo',repo,'--body-file',str(body_file)])
after=api('repos/'+repo+'/pulls/7');assert after['head']['sha']==head and after['body'].encode()==body
assert after['state']=='open' and after['draft'] and after['base']['ref']=='main'
assert run(['git','ls-remote','--heads','origin','refs/heads/'+branch]).decode().split()[0]==head
remote=api('repos/'+repo+'/contents/'+index_path+'?ref='+head)
assert base64.b64decode(remote['content'])==index_raw
prior=api('repos/'+repo+'/contents/docs/validation/integration-2026-10-19/mixed-meaning-inputs/prior-pr-body.md?ref='+head)
assert base64.b64decode(prior['content'])==old
receipt={'verifiedAt':datetime.datetime.now(datetime.timezone.utc).isoformat(),'url':after['html_url'],'head':head,'branch':branch,
    'state':'open','draft':True,'base':'main','bodyBytes':len(body),'bodySha256':sha(body),'indexSha256':sha(index_raw),
    'remoteIndexRawEqual':True,'priorBodyArchivedRawEqual':True,'priorBodyBytes':len(old),'priorBodySha256':sha(old),
    'sourceCommit':source['sourceCommit'],'runtimeCommit':source['runtimeCommit'],'mainOrReleaseVerified':False}
(base/'remote-verification.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8')
print(json.dumps(receipt),flush=True)
