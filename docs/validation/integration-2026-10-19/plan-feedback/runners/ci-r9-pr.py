from pathlib import Path
import subprocess,json,hashlib,base64,time,datetime
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence')
base=root/'validation/integration-20261018/plan-feedback'
archive=root/'docs/validation/integration-2026-10-19/plan-feedback'
temp=Path(__file__).parent;repo='HECer/Yoke-CodeIntelligence'
branch='integrate/native-parity-github-app-20261018'
sha=lambda b:hashlib.sha256(b).hexdigest()
def run(args):return subprocess.check_output(args,cwd=root)
def api(path):return json.loads(run(['gh','api',path]))
assert not run(['git','status','--porcelain'])
head=run(['git','rev-parse','HEAD']).decode().strip()
source=json.loads((base/'source-integration.json').read_bytes())
index_raw=(archive/'index.json').read_bytes()
old=(temp/'ci-pr-7-body-codex-final.md').read_bytes()
assert len(old)==47115 and sha(old)=='0ab74125d54e4fbf4ac3fae61625be96d59e5aaea39fa510f4ed6ee53cb3a211'
before=api('repos/'+repo+'/pulls/7')
assert before['head']['sha']==source['parentCommit'] and before['body'].encode()==old
assert before['state']=='open' and before['draft'] and before['base']['ref']=='main'
marker='<!-- plan-feedback-20261020:start -->';assert marker.encode() not in old
addition=f'''\n\n{marker}
### Saved-plan outcome feedback — Code Intelligence native0.5.0 development

Adds explicit feedback plan-record/plan-rank over existing native Stores, using
a separate immutable event schema. Canonical saved-plan and full native View
replay bind exact changed-file origin/evaluated quotes to base/planned-result
bytes. Released knowledge-event identities stay unchanged. Ordered history,
one latest eligible actor/session-label vote, visible integer age decay,
independent write authority and output-before-insert SQL admission remain
bounded. Lost stdout retries recover the same event ID. Retention replays
original plan/source closures before recognizing roots.

- Source `{source['sourceCommit']}`; runtime merge `{source['runtimeCommit']}`.
- **14 new plan-feedback cases pass.** The broad14-group run records
  **585 passed / 3 failed / 7 ignored**: two unchanged Windows symlink setup
  OS1314 failures and one new test's incorrect source-scope ordering assumption.
  Its correction selects the explicit primary role and snapshot identity.
- Corrected **4-group run:63 passed / 1 failed / 0 ignored**. The unchanged
  feedback symlink setupOS1314 failure remains. The library's **465/0/7** has
  identical production sources; the test-only bridge and exact unchanged
  binary are recorded. Runs are nonadditive; no whole-native pass is claimed.
- Format/build/all-target Clippy-Dwarnings, mandatory typecheck, **260
  TypeScript tests / 3 skips** and **17 Python tests** pass. **731 native /
  737 integrated inputs** remain unchanged within their recorded runs.
- [Contract](https://github.com/{repo}/blob/{head}/native/docs/outcome-feedback.md#authored-outcomes-for-saved-change-plans)
  and [indexed evidence](https://github.com/{repo}/blob/{head}/docs/validation/integration-2026-10-19/plan-feedback/index.json).
  Index SHA-256 `{sha(index_raw)}`.

Authored outcomes and planned result bytes establish no observed application,
execution, current applicability, independent corroboration or source authority.
Mutable verification/approval rows are not consumed as execution proof. There
is no model/provider call, source capture, project execution or automatic
promotion. Additional targets, semantic transcript learning, broaderR1-R9,
whole-native/platform acceptance, main and release remain open. All previous
47115PR-body bytes and historical inventory/ledger remain preserved; work continues.
<!-- plan-feedback-20261020:end -->
'''.encode()
body=old+addition;assert len(body)<65536
body_file=temp/'ci-pr-7-body-plan-feedback-final.md';body_file.write_bytes(body)
subprocess.run(['git','push','origin',f'HEAD:refs/heads/{branch}'],cwd=root,check=True)
subprocess.run(['gh','pr','edit','7','--repo',repo,'--body-file',str(body_file)],cwd=root,check=True)
time.sleep(2)
after=api('repos/'+repo+'/pulls/7');actual=after['body'].encode()
assert after['head']['sha']==head and actual==body and actual.startswith(old)
assert after['state']=='open' and after['draft'] and after['base']['ref']=='main'
assert run(['git','ls-remote','--heads','origin',f'refs/heads/{branch}']).decode().split()[0]==head
remote=api('repos/'+repo+'/contents/docs/validation/integration-2026-10-19/plan-feedback/index.json?ref='+branch)
assert base64.b64decode(remote['content'])==index_raw
receipt={'verifiedAt':datetime.datetime.now(datetime.timezone.utc).isoformat(),'url':after['html_url'],
    'head':head,'branch':branch,'state':'open','draft':True,'base':'main',
    'bodyBytes':len(actual),'bodySha256':sha(actual),'priorBodyBytes':len(old),'priorBodySha256':sha(old),
    'priorBodyPreserved':True,'indexSha256':sha(index_raw),'remoteIndexRawEqual':True,
    'sourceCommit':source['sourceCommit'],'runtimeCommit':source['runtimeCommit'],'mainOrReleaseVerified':False}
(base/'remote-verification.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8')
print(json.dumps(receipt),flush=True)
