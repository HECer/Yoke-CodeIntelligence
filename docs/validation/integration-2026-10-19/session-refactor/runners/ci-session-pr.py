from pathlib import Path
import subprocess,json,hashlib,base64,time,datetime
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');base=root/'validation/integration-20261018/session-refactor'
archive=root/'docs/validation/integration-2026-10-19/session-refactor';temp=Path(__file__).parent
repo='HECer/Yoke-CodeIntelligence';branch='integrate/native-parity-github-app-20261018';sha=lambda b:hashlib.sha256(b).hexdigest()
def run(args):return subprocess.check_output(args,cwd=root)
def api(path):return json.loads(run(['gh','api',path]))
assert not run(['git','status','--porcelain'])
head=run(['git','rev-parse','HEAD']).decode().strip();source=json.loads((base/'source-integration.json').read_bytes());totals=source['nativeTotals']
index_raw=(archive/'index.json').read_bytes();old=(archive/'prior-pr-body.md').read_bytes()
assert len(old)==60414 and sha(old)=='adb7b9ec4e5fbeaa1bbb2048aed82c105b38d82ec4a62157d18d30e8c90693ce'
before=api('repos/'+repo+'/pulls/7')
assert before['head']['sha']==source['parentCommit'] and before['body'].encode()==old
assert before['state']=='open' and before['draft'] and before['base']['ref']=='main'
link=lambda p:f'https://github.com/{repo}/blob/{head}/{p}'
body=f'''Combines native Code Intelligence 0.5 checkpoints `c5178b2` and `7b8eabb`
with guarded GitHub App workflows and the subsequent source-bound native
extensions. The retained TypeScript runtime stays at 0.3. This remains an active
draft integration; full STATUS/R1–R9 acceptance and release are open.

### Implemented behavior

- **Git and GitHub workflows:** bounded verified history capture and persistent
  jobs, closed-PR discussion observations, actual unmerged-index conflict pages,
  captured impact areas and authored owner candidates. Approved digest submission,
  authenticated native intake and receipt-qualified metadata queries retain
  explicit endpoint, source and principal permissions with unknown-send recovery.
- **Guarded changes:** checked saved-plan replay, captured directory moves,
  function/file usage checks and bounded dependency-aware deletion propagation.
  Python empty-suite replacement, function-valued sibling declarators and concrete
  class-method decorator ownership keep original byte evidence and separate
  verification, approval, application and conditional recovery.
- **Source and graph adapters:** explicit PostgreSQL catalog capture, received
  HTTP/arXiv source bodies and exact evidence; reviewed Neo4j/FalkorDB publication
  preserves directed parallel rows, immutable revisions and unknown-write recovery.
  Saved-plan feedback distinguishes planned results from execution and source truth.
- **Host and runtime integration:** receipt-owned Codex MCP configuration,
  portable projections, checked registry coordination, root/query-scope repairs,
  process-environment and cleanup fixes, bounded HTTP refusals and SQLite deadlines.
  Optional retained-provider compatibility keeps separately indexed qualifications.

Repository content remains data. Immutable snapshots, original UTF-8 bytes,
directed/parallel observations and explicit host authority remain fundamental.
Default retrieval starts no model or analyzed project code. Unsupported or
incomplete capabilities are reported explicitly.

### Latest addition: native session semantic plans

`refactor.prepare` exposes existing `safeDelete`, `rename`, `inline` and `move`
planning through `session --requests` and native `ci_session`. Its closed
`operation`/`arguments` envelope retains the native 64,000-byte input bound,
mode/read-only project restrictions, original deadline and source/provider proofs.
A comparison against the stored session record refuses a client that another
client has steered before provider startup. This admission check creates no
lease over future state/source changes.

Native arguments/results and canonical plan identity remain unchanged.
Preparation does not steer the session, write original source or grant approval,
verification, execution or application authority. The native session description
advertises the operation; the normal native eight-tool and published facade
argument schemas remain unchanged.

Source `{source['sourceCommit']}`; runtime merge `{source['runtimeCommit']}`.
The new contract has 9 default integration cases and 1 explicit actual gate. Tests
cover both analyzer epochs, BOM/Unicode/CRLF, closed authority fields, exact input
budget, mode/project/current-session/snapshot/path denial, MCP discovery/results,
CLI host trust, duplicate JSON and independent application/recovery guards.

### Validation

| Scope | Result |
| --- | --- |
|15 selected native groups|{totals['passed']} passed / {totals['failed']} failed / {totals['ignored']} ignored; 738 inputs unchanged|
|Native quality|Format, build and all-target Clippy with warnings denied pass|
|Actual native CLI|TypeScript 5.8.3 / language-server 5.1.3 safeDeletev2 for TS/JS; copied compilation and guarded apply/recovery pass|
|Mandatory retained checks|Typecheck; 260 TypeScript tests / 3 existing skips; 17 Python tests pass|
|Integrated source|744 inputs unchanged; all 738 native inputs match exactly or only by CRLF checkout materialization|

The actual CLI executable is hashed before/after use and matches the final build.
Counts from scoped, actual and historical runs overlap and are not additive.
Earlier provider/platform qualifications retain their own exact source/binary
boundaries; this is not a new whole-native or current-head hosted acceptance claim.

- [Current STATUS and remaining work]({link('docs/STATUS.md')})
- [Native session contract]({link('native/docs/agent-sessions.md')}#native-semantic-planning)
- [Latest indexed raw evidence]({link('docs/validation/integration-2026-10-19/session-refactor/index.json')})
  — SHA-256 `{sha(index_raw)}`.
- [All integration evidence and retained failures]({link('docs/validation/integration-2026-10-19/README.md')})
- [Original inventory and reconciliation boundaries]({link('docs/parity/native-reconciliation-2026-10-15.json')})
- [Archived prior PR description]({link('docs/validation/integration-2026-10-19/session-refactor/prior-pr-body.md')})
  retains all 60414 prior body bytes, SHA-256 `{sha(old)}`.

### Remaining boundaries

All 353 inventory identities, 73 unreconciled rows, 77 backend runtime unknowns and
false full-contract certifications remain. Other provider intents, clients,
language/file shapes, dynamic/external consumers, exhaustive compiler/runtime
guarantees and full upstream/IDE equivalence remain open. Historical OS1314
fixtures and hosted-platform failures remain indexed. Unattended production App
installation, wider authenticated/media services, full platform acceptance,
`main` integration and release are not certified by these scoped checks.
'''.encode()
assert len(body)<65536;body_file=temp/'ci-pr-7-body-session-refactor-final.md';body_file.write_bytes(body)
subprocess.run(['git','push','origin',f'HEAD:refs/heads/{branch}'],cwd=root,check=True)
subprocess.run(['gh','pr','edit','7','--repo',repo,'--body-file',str(body_file)],cwd=root,check=True)
time.sleep(2)
after=api('repos/'+repo+'/pulls/7');actual=after['body'].encode()
assert after['head']['sha']==head and actual==body
assert after['state']=='open' and after['draft'] and after['base']['ref']=='main'
assert run(['git','ls-remote','--heads','origin',f'refs/heads/{branch}']).decode().split()[0]==head
remote=api('repos/'+repo+'/contents/docs/validation/integration-2026-10-19/session-refactor/index.json?ref='+branch)
assert base64.b64decode(remote['content'])==index_raw
prior=api('repos/'+repo+'/contents/docs/validation/integration-2026-10-19/session-refactor/prior-pr-body.md?ref='+branch)
assert base64.b64decode(prior['content'])==old
receipt={'verifiedAt':datetime.datetime.now(datetime.timezone.utc).isoformat(),'url':after['html_url'],'head':head,'branch':branch,
    'state':'open','draft':True,'base':'main','bodyBytes':len(actual),'bodySha256':sha(actual),
    'priorBodyBytes':len(old),'priorBodySha256':sha(old),'priorBodyArchivedRawEqual':True,'priorBodyPreservedInCurrentBody':False,
    'descriptionConsolidatedAroundIntegratedBehavior':True,'indexSha256':sha(index_raw),'remoteIndexRawEqual':True,
    'sourceCommit':source['sourceCommit'],'runtimeCommit':source['runtimeCommit'],'mainOrReleaseVerified':False}
(base/'remote-verification.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8')
print(json.dumps(receipt),flush=True)
