from pathlib import Path
import subprocess,json,hashlib,base64,datetime
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');base=root/'validation/integration-20261018/mixed-meaning-consumers';temp=Path(__file__).parent
repo='HECer/Yoke-CodeIntelligence';branch='integrate/native-parity-github-app-20261018';sha=lambda b:hashlib.sha256(b).hexdigest()
def run(args):return subprocess.check_output(args,cwd=root,stderr=subprocess.PIPE)
def api(route):return json.loads(run(['gh','api',route]))
head=run(['git','rev-parse','HEAD']).decode().strip();assert not run(['git','status','--porcelain'])
source=json.loads((base/'source-integration.json').read_bytes());t=source['nativeTotals']
index_path='docs/validation/integration-2026-10-19/mixed-meaning-consumers/index.json';index_raw=(root/index_path).read_bytes();index=json.loads(index_raw)
assert index['sourceCommit']==source['sourceCommit'] and index['finalNative']['allChecksPassed'] and index['integrated']['allChecksPassed']
old=(root/'docs/validation/integration-2026-10-19/mixed-meaning-consumers/prior-pr-body.md').read_bytes()
before=api('repos/'+repo+'/pulls/7');assert before['body'].encode()==old and before['state']=='open' and before['draft']
assert before['head']['ref']==branch and before['base']['ref']=='main'
link=lambda p:'https://github.com/'+repo+'/blob/'+head+'/'+p
text=old.decode();start=text.index('### Latest explicit mixed-source meaning builds');end=text.index('### Validation',start)
latest=f'''### Latest checked mixed interpretation graph queries

Opaque `CheckedGraph` owns actual source/response replay and exact original graph
data. Graph-qualified concept lookup, case-sensitive label/alias search,
multiedge degrees and bounded directed frontier traces preserve original
parallel observation positions, self-loops and ordered repeated hyperedge roles.
Roles create no flow and aliases merge no concepts. Every answer retains exact
nested typed source/receipt/accounting provenance. Existing graph/load bytes and
IDs remain unchanged; JSON cannot construct the capability.

Loaded queries remain frozen after metadata corruption or total Store/CAS loss.
CLI query retains separate PDF permission during load and starts no provider;
malformed shapes refuse before Store initialization. Top-level complete and
canonical-source authority remain false. Cross-graph/workspace composition,
broader analytics, external semantic qualification and full R4 remain open.

Source `{source['sourceCommit']}`; runtime merge `{source['runtimeCommit']}`.
Eight new default cases include actual owned Node/PDF setup and end-to-end CLI,
Unicode/ambiguous labels, graph identity, direction/depth0/self/parallel/roles,
Store loss, closed JSON/private failures and output/deadline refusal. Initial
17/1 retains a wrong expected refusal code; the first final build retains its
disk-full linker failure before any test group executed. Inactive owned target
cache cleanup records hashes/bytes before deletion; sources/evidence remain.
These are scoped protocol/query checks, without model-quality or independent
billing attestation. Trusted provider OS/network effects remain unverified.

'''
text=text[:start]+latest+text[end:];lines=[]
for line in text.splitlines(True):
    if line.startswith('|18 latest selected native groups|'):
        line=f"|19 latest selected native groups|{t['passed']} passed / {t['failed']} failed / {t['ignored']} ignored; {index['finalNative']['inputs']} inputs unchanged|\n"
    elif line.startswith('|Integrated source|'):
        line=f"|Integrated source|{index['integrated']['frozenInputs']} inputs unchanged; all {index['finalNative']['inputs']} native inputs match exactly or only by CRLF checkout materialization|\n"
    lines.append(line)
text=''.join(lines);assert '|18 latest selected native groups|' not in text
start=text.index('- [Current STATUS');end=text.index('### Remaining boundaries',start)
links=f'''- [Current STATUS and remaining work]({link('docs/STATUS.md')})
- [Checked mixed graph query contract]({link('native/docs/mixed-meaning-queries.md')})
- [Latest indexed raw evidence]({link(index_path)}) — SHA-256 `{sha(index_raw)}`.
- [Prior explicit provider build qualification]({link('docs/validation/integration-2026-10-19/mixed-meaning-provider/index.json')})
- [All integration evidence and retained failures]({link('docs/validation/integration-2026-10-19/README.md')})
- [Inventory and reconciliation boundaries]({link('docs/parity/native-reconciliation-2026-10-15.json')})
- [Archived prior PR description]({link('docs/validation/integration-2026-10-19/mixed-meaning-consumers/prior-pr-body.md')})
  retains all {len(old)} prior body bytes, SHA-256 `{sha(old)}`.

'''
body=(text[:start]+links+text[end:]).encode();assert len(body)<8000
body_file=temp/'ci-pr-7-body-mixed-meaning-consumers-final.md';body_file.write_bytes(body)
run(['git','push','origin','HEAD:refs/heads/'+branch]);run(['gh','pr','edit','7','--repo',repo,'--body-file',str(body_file)])
after=api('repos/'+repo+'/pulls/7');assert after['head']['sha']==head and after['body'].encode()==body
assert after['state']=='open' and after['draft'] and after['base']['ref']=='main'
assert run(['git','ls-remote','--heads','origin','refs/heads/'+branch]).decode().split()[0]==head
remote=api('repos/'+repo+'/contents/'+index_path+'?ref='+head);assert base64.b64decode(remote['content'])==index_raw
prior=api('repos/'+repo+'/contents/docs/validation/integration-2026-10-19/mixed-meaning-consumers/prior-pr-body.md?ref='+head);assert base64.b64decode(prior['content'])==old
receipt={'verifiedAt':datetime.datetime.now(datetime.timezone.utc).isoformat(),'url':after['html_url'],'head':head,'branch':branch,'state':'open','draft':True,'base':'main','bodyBytes':len(body),'bodySha256':sha(body),'indexSha256':sha(index_raw),'remoteIndexRawEqual':True,'priorBodyArchivedRawEqual':True,'priorBodyBytes':len(old),'priorBodySha256':sha(old),'sourceCommit':source['sourceCommit'],'runtimeCommit':source['runtimeCommit'],'mainOrReleaseVerified':False}
(base/'remote-verification.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8');print(json.dumps(receipt),flush=True)
