from pathlib import Path
import subprocess,json,hashlib,base64,time,datetime
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');base=root/'validation/integration-20261018/falkor-publish';archive=root/'docs/validation/integration-2026-10-19/falkor-publish';temp=Path(__file__).parent;repo='HECer/Yoke-CodeIntelligence';branch='integrate/native-parity-github-app-20261018';sha=lambda b:hashlib.sha256(b).hexdigest()
def run(args):return subprocess.check_output(args,cwd=root)
def api(path):return json.loads(run(['gh','api',path]))
assert not run(['git','status','--porcelain']);head=run(['git','rev-parse','HEAD']).decode().strip();source=json.loads((base/'source-integration.json').read_bytes());index_raw=(archive/'index.json').read_bytes();index=json.loads(index_raw)
old=(temp/'ci-pr-7-body-neo4j-final.md').read_bytes();assert len(old)==41826 and sha(old)=='7e10db5e07773ea403f60c8efa50582682624c46f593ec391b3eff6c3e759c04'
before=api('repos/'+repo+'/pulls/7');assert before['head']['sha']=='dd69e53b9218f75018b879108fe13a2883b4366f' and before['body'].encode()==old;assert before['state']=='open' and before['draft'] and before['base']['ref']=='main'
marker='<!-- falkor-publish-20261020:start -->';assert marker.encode() not in old
addition=f'''\n\n{marker}
### Reviewed native FalkorDB snapshot publication

Adds an explicit Windows host workflow for saved native base graphs with bounded
native RESP2/TCP/TLS, pinned sockets/static roots/verified hostnames, private-pipe
OS credentials, separate host grants and closed offline plans. Fresh-graph
WATCH/EXISTS/MULTI-one-initialize/EXEC precedes separately journaled operational
schema. Immutable revisions retain every original row, byte range, directed
parallel observation and unresolved target, with atomic expected-revision/count
guards and durable pre-execution intents. Unknown results require positive
fixed-read recovery and cannot be automatically resent.

Actual owned FalkorDB6.0.2/module60002 tests pass native TLS/staticCA/wrong-host/
wrong-root/private Unicode credentials, exact row roundtrips, two forced updates
with exactly one commit and loser zero rows, response loss after commit,
historical recovery, count/schema conflicts, foreign graph preservation, actual
native WATCH/EXEC interference and lost stdout. All5credentials and own VM are
removed. The same final binary revalidates the complete owned Neo4j connector
fixture; its5credentials and Java server are also removed.

- Source `{source['sourceCommit']}`; runtime merge `{source['runtimeCommit']}`.
- **25 new cases**. Final20selectedgroups: **580 passed / 1 failed / 8 ignored**.
  The failure remains unchanged media symlink setupOS1314; no whole-native pass.
- All-target Clippy with warnings denied, format, mandatory typecheck,
  **260 TypeScript tests / 3 skips**, **17 Python tests** pass.
  **730 integrated / 724 native inputs** and exact final live binaries are frozen.
- [Workflow](https://github.com/{repo}/blob/{head}/docs/native-falkor-publish.md)
  and [indexed evidence](https://github.com/{repo}/blob/{head}/docs/validation/integration-2026-10-19/falkor-publish/index.json).
  Index SHA-256 `{sha(index_raw)}`.

Only module60002 is admitted; other versions refuse. Live credentials/journals
remain Windows-only. Hash chains are integrity metadata, not authentication or
global coordination; client cancellation cannot guarantee server cancellation.
Production/RBAC, other servers/platforms/JVM vendors, full upstream/mixed-source
equivalence, Google/auth/social/video/article/semantic connectors and widerR1-R9
remain open. Main/release are unverified and the broad STATUS task continues.
All previous41826PR-bodybytes and historical inventory/ledger remain preserved.
<!-- falkor-publish-20261020:end -->
'''.encode();body=old+addition;assert len(body)<65536;body_file=temp/'ci-pr-7-body-falkor-final.md';body_file.write_bytes(body)
subprocess.run(['git','push','origin',f'HEAD:refs/heads/{branch}'],cwd=root,check=True)
subprocess.run(['gh','pr','edit','7','--repo',repo,'--body-file',str(body_file)],cwd=root,check=True);time.sleep(2)
after=api('repos/'+repo+'/pulls/7');actual_body=after['body'].encode();assert after['head']['sha']==head and actual_body==body and actual_body.startswith(old);assert after['state']=='open' and after['draft'] and after['base']['ref']=='main'
remote_head=run(['git','ls-remote','--heads','origin',f'refs/heads/{branch}']).decode().split()[0];assert remote_head==head
remote_index=api('repos/'+repo+'/contents/docs/validation/integration-2026-10-19/falkor-publish/index.json?ref='+branch);assert base64.b64decode(remote_index['content'])==index_raw
receipt={'verifiedAt':datetime.datetime.now(datetime.timezone.utc).isoformat(),'url':after['html_url'],'head':head,'branch':branch,'state':'open','draft':True,'base':'main','bodyBytes':len(actual_body),'bodySha256':sha(actual_body),'priorBodyBytes':len(old),'priorBodySha256':sha(old),'priorBodyPreserved':True,'indexSha256':sha(index_raw),'remoteIndexRawEqual':True,'sourceCommit':source['sourceCommit'],'runtimeCommit':source['runtimeCommit'],'mainOrReleaseVerified':False};(base/'remote-verification.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8');print(json.dumps(receipt),flush=True)
