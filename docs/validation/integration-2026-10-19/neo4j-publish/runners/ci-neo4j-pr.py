from pathlib import Path
import subprocess,json,hashlib,base64,time,datetime
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');base=root/'validation/integration-20261018/neo4j-publish';archive=root/'docs/validation/integration-2026-10-19/neo4j-publish';temp=Path(__file__).parent;repo='HECer/Yoke-CodeIntelligence';branch='integrate/native-parity-github-app-20261018';sha=lambda b:hashlib.sha256(b).hexdigest()
def run(args):return subprocess.check_output(args,cwd=root)
def api(path):return json.loads(run(['gh','api',path]))
assert not run(['git','status','--porcelain']);head=run(['git','rev-parse','HEAD']).decode().strip();source=json.loads((base/'source-integration.json').read_bytes());index_raw=(archive/'index.json').read_bytes();index=json.loads(index_raw)
old=(temp/'ci-pr-7-body-held-members-final.md').read_bytes();assert len(old)==39383 and sha(old)=='71ce71dfdbdaf4d1a9df03ca5a3133ec98532a74952318ab0ea174739a34c14d'
before=api('repos/'+repo+'/pulls/7');assert before['head']['sha']=='58be008555e9d24df1af346245fe8249d8d4d998' and before['body'].encode()==old;assert before['state']=='open' and before['draft'] and before['base']['ref']=='main'
marker='<!-- neo4j-publish-20261019:start -->';assert marker.encode() not in old
addition=f'''\n\n{marker}
### Reviewed native Neo4j snapshot publication

Adds an explicit Windows host workflow for publishing saved native base graphs:
closed offline plans, separate approval/schema/network/OS-credential/write gates,
fixed parameterized QueryAPIv2/Cypher5, immutable scoped revisions and durable
pre-send intents. Original rows, byte ranges, directions, parallel observations
and unresolved targets are retained. Unknown writes require positive fixed-read
recovery and cannot be automatically resent.

Actual owned Neo4j Community2026.09.0 tests pass HTTPS/static roots/hostname
refusals, Unicode private-pipe credentials, exact original row roundtrips, forced
two-writer CAS with exactly one commit, historical recovery, count/schema
conflicts, response loss after commit and lost stdout. All5fixturecredentials
and the Java server are removed. Source and production-binary hashes are frozen.

- Source `{source['sourceCommit']}`; runtime merge `{source['runtimeCommit']}`.
- **21 new cases**. Final17selectednativegroups: **555 passed / 1 failed / 8 ignored**.
  The failure remains the unchanged media symlink setupOS1314. No whole-native pass.
- All-target Clippy/format, mandatory typecheck, **260 TypeScript tests / 3 skips**
  and **17 Python tests** pass; **714integrated / 708native inputs** are checked.
  A test-only cfg-import correction retains the exact production binary.
- [Workflow](https://github.com/{repo}/blob/{head}/docs/native-neo4j-publish.md)
  and [indexed evidence](https://github.com/{repo}/blob/{head}/docs/validation/integration-2026-10-19/neo4j-publish/index.json).
  Index SHA-256 `{sha(index_raw)}`.

JBR25.0.3 functional evidence does not qualify the JVM vendor. Live publication
and credentials remain Windows-only; other server/platform/EnterpriseRBAC and
full upstream/mixed-source equivalence remain open. Journal hashes are integrity
metadata, not authentication/global coordination, and client cancellation cannot
guarantee server cancellation. BroaderR1-R9, FalkorDB/Google/auth/social/video/
semantic connectors, unattended production, main and release remain open.
Previous39383PR-bodybytes and the historical inventory/ledger are preserved.
<!-- neo4j-publish-20261019:end -->
'''.encode();body=old+addition;assert len(body)<65536;body_file=temp/'ci-pr-7-body-neo4j-final.md';body_file.write_bytes(body)
subprocess.run(['git','push','origin',f'HEAD:refs/heads/{branch}'],cwd=root,check=True)
subprocess.run(['gh','pr','edit','7','--repo',repo,'--body-file',str(body_file)],cwd=root,check=True)
time.sleep(2)
after=api('repos/'+repo+'/pulls/7');actual_body=after['body'].encode();assert after['head']['sha']==head and actual_body==body and actual_body.startswith(old);assert after['state']=='open' and after['draft'] and after['base']['ref']=='main'
remote_head=run(['git','ls-remote','--heads','origin',f'refs/heads/{branch}']).decode().split()[0];assert remote_head==head
remote_index=api('repos/'+repo+'/contents/docs/validation/integration-2026-10-19/neo4j-publish/index.json?ref='+branch);assert base64.b64decode(remote_index['content'])==index_raw
receipt={'verifiedAt':datetime.datetime.now(datetime.timezone.utc).isoformat(),'url':after['html_url'],'head':head,'branch':branch,'state':'open','draft':True,'base':'main','bodyBytes':len(actual_body),'bodySha256':sha(actual_body),'priorBodyBytes':len(old),'priorBodySha256':sha(old),'priorBodyPreserved':True,'indexSha256':sha(index_raw),'remoteIndexRawEqual':True,'sourceCommit':source['sourceCommit'],'runtimeCommit':source['runtimeCommit'],'mainOrReleaseVerified':False};(base/'remote-verification.json').write_text(json.dumps(receipt,indent=2)+'\n');print(json.dumps(receipt),flush=True)
