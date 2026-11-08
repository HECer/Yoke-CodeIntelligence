from pathlib import Path
import subprocess,json,hashlib,base64,datetime
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence')
base=root/'validation/integration-20261018/mixed-meaning-claims';temp=Path(__file__).parent
repo='HECer/Yoke-CodeIntelligence';branch='integrate/native-parity-github-app-20261018'
sha=lambda b:hashlib.sha256(b).hexdigest()
def run(args):return subprocess.check_output(args,cwd=root,stderr=subprocess.PIPE)
def api(route):return json.loads(run(['gh','api',route]))
head=run(['git','rev-parse','HEAD']).decode().strip();assert not run(['git','status','--porcelain'])
source=json.loads((base/'source-integration.json').read_bytes());t=source['nativeTotals']
index_path='docs/validation/integration-2026-10-19/mixed-meaning-claims/index.json';index_raw=(root/index_path).read_bytes();index=json.loads(index_raw)
assert index['sourceCommit']==source['sourceCommit'] and index['integrated']['allChecksPassed']
old=(root/'docs/validation/integration-2026-10-19/mixed-meaning-claims/prior-pr-body.md').read_bytes()
before=api('repos/'+repo+'/pulls/7');assert before['body'].encode()==old and before['state']=='open' and before['draft']
assert before['head']['ref']==branch and before['base']['ref']=='main'
link=lambda p:'https://github.com/'+repo+'/blob/'+head+'/'+p
text=old.decode();start=text.index('### Latest checked mixed-source inputs');end=text.index('### Validation',start)
latest=f'''### Latest checked mixed interpretation claims

`mixed-view --meaning-inputs HOST_JSON --meaning-claims HOST_FILE` checks
host-submitted concepts, directed parallel/self relations and ordered role-bearing
hyperedges against the exact prepared input hash and unit ranges. Every support
quote is reread through the original typed native/Office decoded/XML/PDF reader.
Original claim keys, aliases, order and participant slots remain unchanged.
The result nests the exact prepared inputs and original receipt/source-scope
metadata. Host interpretation remains unverified; checked quotations establish
their bytes without establishing semantic truth or canonical source authority.
No provider/model execution, invented receipt or persistence is added.

Source `{source['sourceCommit']}`; runtime merge `{source['runtimeCommit']}`.
Five new default cases include actual native PDF helpers and one actual CLI
case. They cover all four spaces, Unicode quotation, parallel directions/roles,
foreign support and coverage, exact128000/128001 quote and output boundaries,
deadline/no-write behavior, explicit no-claim coverage and strict preinitialization.
The first4groups43/0/0 and final14groups overlap and are not additive.
Mixed semantic provider extraction, durable graph replay and general consumers
remain open. Prior session/function-binding and TS/LSP qualifications remain
separately indexed with their own source/binary and application authority.

'''
text=text[:start]+latest+text[end:]
lines=[]
for line in text.splitlines(True):
    if line.startswith('|13 latest selected native groups|'):
        line=f"|14 latest selected native groups|{t['passed']} passed / {t['failed']} failed / {t['ignored']} ignored; {index['finalNative']['inputs']} inputs unchanged|\n"
    elif line.startswith('|Integrated source|'):
        line=f"|Integrated source|{index['integrated']['frozenInputs']} inputs unchanged; all {index['finalNative']['inputs']} native inputs match exactly or only by CRLF checkout materialization|\n"
    elif line.startswith('|Actual native execution|'):
        line='|Actual native execution|Native PDF helpers and one new CLI case are included in the latest default scope; earlier TS5.8.3/LSP5.1.3 qualifications stay separate|\n'
    lines.append(line)
text=''.join(lines);assert '|13 latest selected native groups|' not in text
start=text.index('- [Current STATUS');end=text.index('### Remaining boundaries',start)
links=f'''- [Current STATUS and remaining work]({link('docs/STATUS.md')})
- [Mixed interpretation claim contract]({link('native/docs/mixed-meaning-claims.md')})
- [Latest indexed raw evidence]({link(index_path)}) — SHA-256 `{sha(index_raw)}`.
- [Prior checked input preparation]({link('docs/validation/integration-2026-10-19/mixed-meaning-inputs/index.json')})
- [All integration evidence and retained failures]({link('docs/validation/integration-2026-10-19/README.md')})
- [Inventory and reconciliation boundaries]({link('docs/parity/native-reconciliation-2026-10-15.json')})
- [Archived prior PR description]({link('docs/validation/integration-2026-10-19/mixed-meaning-claims/prior-pr-body.md')})
  retains all {len(old)} corrected prior body bytes, SHA-256 `{sha(old)}`.

'''
body=(text[:start]+links+text[end:]).encode();assert len(body)<8000
body_file=temp/'ci-pr-7-body-mixed-meaning-claims-final.md';body_file.write_bytes(body)
run(['git','push','origin','HEAD:refs/heads/'+branch])
run(['gh','pr','edit','7','--repo',repo,'--body-file',str(body_file)])
after=api('repos/'+repo+'/pulls/7');assert after['head']['sha']==head and after['body'].encode()==body
assert after['state']=='open' and after['draft'] and after['base']['ref']=='main'
assert run(['git','ls-remote','--heads','origin','refs/heads/'+branch]).decode().split()[0]==head
remote=api('repos/'+repo+'/contents/'+index_path+'?ref='+head);assert base64.b64decode(remote['content'])==index_raw
prior=api('repos/'+repo+'/contents/docs/validation/integration-2026-10-19/mixed-meaning-claims/prior-pr-body.md?ref='+head)
assert base64.b64decode(prior['content'])==old
receipt={'verifiedAt':datetime.datetime.now(datetime.timezone.utc).isoformat(),'url':after['html_url'],'head':head,'branch':branch,
    'state':'open','draft':True,'base':'main','bodyBytes':len(body),'bodySha256':sha(body),'indexSha256':sha(index_raw),
    'remoteIndexRawEqual':True,'priorBodyArchivedRawEqual':True,'priorBodyBytes':len(old),'priorBodySha256':sha(old),
    'sourceCommit':source['sourceCommit'],'runtimeCommit':source['runtimeCommit'],'mainOrReleaseVerified':False}
(base/'remote-verification.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8')
print(json.dumps(receipt),flush=True)
