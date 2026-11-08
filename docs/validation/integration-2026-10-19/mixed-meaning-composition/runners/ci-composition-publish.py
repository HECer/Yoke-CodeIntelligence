from pathlib import Path
import subprocess,json,hashlib,base64,datetime
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');base=root/'validation/integration-20261018/mixed-meaning-composition';temp=Path(__file__).parent
repo='HECer/Yoke-CodeIntelligence';branch='integrate/native-parity-github-app-20261018';sha=lambda b:hashlib.sha256(b).hexdigest()
def run(args):return subprocess.check_output(args,cwd=root,stderr=subprocess.PIPE)
def api(route):return json.loads(run(['gh','api',route]))
head=run(['git','rev-parse','HEAD']).decode().strip();assert not run(['git','status','--porcelain'])
source=json.loads((base/'source-integration.json').read_bytes());t=source['nativeTotals']
index_path='docs/validation/integration-2026-10-19/mixed-meaning-composition/index.json';index_raw=(root/index_path).read_bytes();index=json.loads(index_raw)
assert index['sourceCommit']==source['sourceCommit'] and index['finalNative']['allChecksPassed'] and index['integrated']['allChecksPassed']
old=(root/'docs/validation/integration-2026-10-19/mixed-meaning-composition/prior-pr-body.md').read_bytes()
before=api('repos/'+repo+'/pulls/7');assert before['body'].encode()==old and before['state']=='open' and before['draft']
assert before['head']['ref']==branch and before['base']['ref']=='main'
link=lambda p:'https://github.com/'+repo+'/blob/'+head+'/'+p
text=old.decode();start=text.index('### Latest checked mixed interpretation graph queries');end=text.index('### Validation',start)
latest=f'''### Latest checked mixed graph composition

Opaque composition consumes two to four independently source/response-replayed
parents and an exact ordered host-authored link review. Original parent-qualified
concept/observation namespaces, parallel/reverse/self provider rows, ordered
hyperedge slots and full nested typed source/receipt/accounting remain intact.
Labels/aliases create no merge or semantic identity. Host links retain separate
indices, unverified provenance and degree counts; traces follow them only when
explicitly selected. Hyperedge roles create no flow.

Library parents may come from different checked workspaces/history. The CLI
selects explicit jobs from one root Store and applies PDF worker permission per
parent, including combined PDF/Office graphs. Frozen queries survive both Store
losses. No provider, persistence, source write, JSON capability construction or
default eight-tool MCP schema change is added. Broader analytics, semantic
qualification, cross-workspace CLI admission and full R4 remain open.

Source `{source['sourceCommit']}`; runtime merge `{source['runtimeCommit']}`.
Eight new default cases include owned Node/PDF setup, qualified identities,
original and authored multiedges/degrees, Unicode ambiguity, direction/depth0,
capacity/deadline/private input refusal, independent parent/endpoints, both Store
losses and actual PDF+Office CLI query without provider files. Initial test-only
Copy/Clone compilation failure executes no test groups; the next focused three
groups pass27/0/0. Scoped protocol/query evidence establishes no model-quality,
independent billing, semantic truth or current live workspace applicability.

'''
text=text[:start]+latest+text[end:];lines=[]
for line in text.splitlines(True):
    if line.startswith('|19 latest selected native groups|'):
        line=f"|20 latest selected native groups|{t['passed']} passed / {t['failed']} failed / {t['ignored']} ignored; {index['finalNative']['inputs']} inputs unchanged|\n"
    elif line.startswith('|Integrated source|'):
        line=f"|Integrated source|{index['integrated']['frozenInputs']} inputs unchanged; all {index['finalNative']['inputs']} native inputs match exactly or only by CRLF checkout materialization|\n"
    lines.append(line)
text=''.join(lines);assert '|19 latest selected native groups|' not in text
start=text.index('- [Current STATUS');end=text.index('### Remaining boundaries',start)
links=f'''- [Current STATUS and remaining work]({link('docs/STATUS.md')})
- [Checked mixed composition contract]({link('native/docs/mixed-meaning-composition.md')})
- [Latest indexed raw evidence]({link(index_path)}) — SHA-256 `{sha(index_raw)}`.
- [Prior checked single-graph query qualification]({link('docs/validation/integration-2026-10-19/mixed-meaning-consumers/index.json')})
- [All integration evidence and retained failures]({link('docs/validation/integration-2026-10-19/README.md')})
- [Inventory and reconciliation boundaries]({link('docs/parity/native-reconciliation-2026-10-15.json')})
- [Archived prior PR description]({link('docs/validation/integration-2026-10-19/mixed-meaning-composition/prior-pr-body.md')})
  retains all {len(old)} prior body bytes, SHA-256 `{sha(old)}`.

'''
body=(text[:start]+links+text[end:]).encode();assert len(body)<8000
body_file=temp/'ci-pr-7-body-mixed-meaning-composition-final.md';body_file.write_bytes(body)
run(['git','push','origin','HEAD:refs/heads/'+branch]);run(['gh','pr','edit','7','--repo',repo,'--body-file',str(body_file)])
after=api('repos/'+repo+'/pulls/7');assert after['head']['sha']==head and after['body'].encode()==body
assert after['state']=='open' and after['draft'] and after['base']['ref']=='main'
assert run(['git','ls-remote','--heads','origin','refs/heads/'+branch]).decode().split()[0]==head
remote=api('repos/'+repo+'/contents/'+index_path+'?ref='+head);assert base64.b64decode(remote['content'])==index_raw
prior=api('repos/'+repo+'/contents/docs/validation/integration-2026-10-19/mixed-meaning-composition/prior-pr-body.md?ref='+head);assert base64.b64decode(prior['content'])==old
receipt={'verifiedAt':datetime.datetime.now(datetime.timezone.utc).isoformat(),'url':after['html_url'],'head':head,'branch':branch,'state':'open','draft':True,'base':'main','bodyBytes':len(body),'bodySha256':sha(body),'indexSha256':sha(index_raw),'remoteIndexRawEqual':True,'priorBodyArchivedRawEqual':True,'priorBodyBytes':len(old),'priorBodySha256':sha(old),'sourceCommit':source['sourceCommit'],'runtimeCommit':source['runtimeCommit'],'mainOrReleaseVerified':False}
(base/'remote-verification.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8');print(json.dumps(receipt),flush=True)
