from pathlib import Path
import subprocess,json,hashlib,base64,datetime
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');base=root/'validation/integration-20261018/mixed-meaning-provider';temp=Path(__file__).parent
repo='HECer/Yoke-CodeIntelligence';branch='integrate/native-parity-github-app-20261018';sha=lambda b:hashlib.sha256(b).hexdigest()
def run(args):return subprocess.check_output(args,cwd=root,stderr=subprocess.PIPE)
def api(route):return json.loads(run(['gh','api',route]))
head=run(['git','rev-parse','HEAD']).decode().strip();assert not run(['git','status','--porcelain'])
source=json.loads((base/'source-integration.json').read_bytes());t=source['nativeTotals']
index_path='docs/validation/integration-2026-10-19/mixed-meaning-provider/index.json';index_raw=(root/index_path).read_bytes();index=json.loads(index_raw)
assert index['sourceCommit']==source['sourceCommit'] and index['finalNative']['allChecksPassed'] and index['integrated']['allChecksPassed']
old=(root/'docs/validation/integration-2026-10-19/mixed-meaning-provider/prior-pr-body.md').read_bytes()
before=api('repos/'+repo+'/pulls/7');assert before['body'].encode()==old and before['state']=='open' and before['draft']
assert before['head']['ref']==branch and before['base']['ref']=='main'
link=lambda p:'https://github.com/'+repo+'/blob/'+head+'/'+p
text=old.decode();start=text.index('### Latest checked mixed interpretation claims');end=text.index('### Validation',start)
latest=f'''### Latest explicit mixed-source meaning builds

`mixed-meaning build` admits one fixed typed input set only with separate
trusted-provider permission. Original native/Office decoded/XML/PDF pins and
exact text/context order bind the request. Atomic ledger/job registration and a
single-use reservation precede the send. Completely bound valid costs survive
bad claims, usage, provider/process errors and overruns; unknown sends retain
their reservation without automatic retry. Original raw replies and source-bound
graph artifacts publish with exact row preconditions. Interpretations remain
unverified, without decoded-to-native authority promotion or default model calls.

Provider-free status/jobs recover IDs and distinguish original response reports
from later ledger reconciliation. Load replays actual original source/receipts
and quoted byte spans; PDF replay retains its separate own-worker permission.
Typed retention verifies saved quotation/graph consistency and real parent
dependencies without a PDF process. Original claim order, aliases, directed
parallel/self relations and ordered hyperedge roles remain intact.

Source `{source['sourceCommit']}`; runtime merge `{source['runtimeCommit']}`.
Eleven new default cases use actual owned Node/PDF protocol processes and one
CLI workflow. Coverage includes Unicode/all4spaces, invalid/unbound/duplicate
replies, bound billing failures/overruns, unknown-cost reconciliation, immutable
replay after provider/live-file removal, corrupt quote retention, transaction/
SQLite limits, metadata4096/4097 admission and private malformed host config.
These are protocol/host-process checks; no semantic model, real service billing
or model quality was evaluated. Trusted provider OS/network effects remain
unverified; sourceWritten=false is scoped to native parent source operations.
General mixed-graph consumers, cross-workspace/fullR4 and wider STATUS remain open.

'''
text=text[:start]+latest+text[end:];lines=[]
for line in text.splitlines(True):
    if line.startswith('|14 latest selected native groups|'):
        line=f"|18 latest selected native groups|{t['passed']} passed / {t['failed']} failed / {t['ignored']} ignored; {index['finalNative']['inputs']} inputs unchanged|\n"
    elif line.startswith('|Integrated source|'):
        line=f"|Integrated source|{index['integrated']['frozenInputs']} inputs unchanged; all {index['finalNative']['inputs']} native inputs match exactly or only by CRLF checkout materialization|\n"
    elif line.startswith('|Actual native execution|'):
        line='|Actual native execution|Owned Node/PDF protocol processes and CLI included in default scope; no real semantic model/paid-service billing qualification|\n'
    lines.append(line)
text=''.join(lines);assert '|14 latest selected native groups|' not in text
old_sentence='The latest default PDF-worker case is covered by its\nunchanged source freeze; no separate before/after call-hash attestation is claimed.'
assert old_sentence in text
text=text.replace(old_sentence,'The latest owned Node/PDF protocol processes are\ncovered by their unchanged source freeze; no separate before/after call-hash\nor real-model quality attestation is claimed.')
start=text.index('- [Current STATUS');end=text.index('### Remaining boundaries',start)
links=f'''- [Current STATUS and remaining work]({link('docs/STATUS.md')})
- [Mixed provider build contract]({link('native/docs/mixed-meaning-provider.md')})
- [Latest indexed raw evidence]({link(index_path)}) — SHA-256 `{sha(index_raw)}`.
- [Prior typed interpretation checks]({link('docs/validation/integration-2026-10-19/mixed-meaning-claims/index.json')})
- [All integration evidence and retained failures]({link('docs/validation/integration-2026-10-19/README.md')})
- [Inventory and reconciliation boundaries]({link('docs/parity/native-reconciliation-2026-10-15.json')})
- [Archived prior PR description]({link('docs/validation/integration-2026-10-19/mixed-meaning-provider/prior-pr-body.md')})
  retains all {len(old)} prior body bytes, SHA-256 `{sha(old)}`.

'''
body=(text[:start]+links+text[end:]).encode();assert len(body)<8000
body_file=temp/'ci-pr-7-body-mixed-meaning-provider-final.md';body_file.write_bytes(body)
run(['git','push','origin','HEAD:refs/heads/'+branch]);run(['gh','pr','edit','7','--repo',repo,'--body-file',str(body_file)])
after=api('repos/'+repo+'/pulls/7');assert after['head']['sha']==head and after['body'].encode()==body
assert after['state']=='open' and after['draft'] and after['base']['ref']=='main'
assert run(['git','ls-remote','--heads','origin','refs/heads/'+branch]).decode().split()[0]==head
remote=api('repos/'+repo+'/contents/'+index_path+'?ref='+head);assert base64.b64decode(remote['content'])==index_raw
prior=api('repos/'+repo+'/contents/docs/validation/integration-2026-10-19/mixed-meaning-provider/prior-pr-body.md?ref='+head)
assert base64.b64decode(prior['content'])==old
receipt={'verifiedAt':datetime.datetime.now(datetime.timezone.utc).isoformat(),'url':after['html_url'],'head':head,'branch':branch,
    'state':'open','draft':True,'base':'main','bodyBytes':len(body),'bodySha256':sha(body),'indexSha256':sha(index_raw),
    'remoteIndexRawEqual':True,'priorBodyArchivedRawEqual':True,'priorBodyBytes':len(old),'priorBodySha256':sha(old),
    'sourceCommit':source['sourceCommit'],'runtimeCommit':source['runtimeCommit'],'mainOrReleaseVerified':False}
(base/'remote-verification.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8');print(json.dumps(receipt),flush=True)
