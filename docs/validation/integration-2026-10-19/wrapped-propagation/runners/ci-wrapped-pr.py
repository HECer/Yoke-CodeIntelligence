from pathlib import Path
import subprocess,json,hashlib,base64,time,datetime
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence')
base=root/'validation/integration-20261018/wrapped-propagation';archive=root/'docs/validation/integration-2026-10-19/wrapped-propagation'
temp=Path(__file__).parent;repo='HECer/Yoke-CodeIntelligence';branch='integrate/native-parity-github-app-20261018'
sha=lambda b:hashlib.sha256(b).hexdigest()
def run(args):return subprocess.check_output(args,cwd=root)
def api(path):return json.loads(run(['gh','api',path]))
assert not run(['git','status','--porcelain'])
head=run(['git','rev-parse','HEAD']).decode().strip();source=json.loads((base/'source-integration.json').read_bytes())
index_raw=(archive/'index.json').read_bytes();old=(temp/'ci-pr-7-body-member-decorators-final.md').read_bytes()
assert len(old)==58090 and sha(old)=='5bd9a684f8f92e49aba56d17de18a96745ae285325eff1ff89d173f25c3124e5'
before=api('repos/'+repo+'/pulls/7')
assert before['head']['sha']==source['parentCommit'] and before['body'].encode()==old
assert before['state']=='open' and before['draft'] and before['base']['ref']=='main'
marker='<!-- wrapped-propagation-20261020:start -->';assert marker.encode() not in old
addition=f'''\n\n{marker}
### Explicit wrapped-call propagation — Code Intelligence native0.5.0 development

Adds opt-in Rust API `safeDelete.propagationPolicy` v2 for complete plain call
statements within32 parentheses/await/void wrappers in a program or block.
Exact original provider/source/dependency evidence remains; full-result grammar
and bounded surviving-node kind/nesting/field/order/mapped-byte comparison refuse unintended
statement merging at automatic-semicolon boundaries. V1/facade defaults,
canonical Plan identity formula and independent application gates remain.
Optional/type/assertion/value/unclassified forms and lone control slots refuse.

- Source `{source['sourceCommit']}`; runtime merge `{source['runtimeCommit']}`.
- Eleven new cases:1unit+9default integration+1explicit actual gate.
- Final13selectednativegroups:587 passed / 0 failed / 13 ignored,
  including469/0/7library.737nativeinputs unchanged.
- Actual TypeScript5.8.3/language-server5.1.3 validate copied TS/JS results for
  v1/v2, execution permission, approval, write, exact apply/recovery. The2actual
  cases overlap ignored gates; source modules never execute.
- Format/build/alltargetClippy-Dwarnings, mandatory typecheck,
  TypeScript260/3skip and Python17 pass with743integratedinputs unchanged.
  Initial refusal, corrected timestamp-test assumption, intermediate passing
  run and reproduced statement-merging risk before the guard are preserved.
- [Contract](https://github.com/{repo}/blob/{head}/native/docs/propagated-delete.md)
  and [indexed evidence](https://github.com/{repo}/blob/{head}/docs/validation/integration-2026-10-19/wrapped-propagation/index.json).
  Index SHA-256 `{sha(index_raw)}`.

Syntax correspondence does not establish semantic/runtime/dispatch/consumer
completeness. Other propagation/declaration/file/language/provider forms and
broaderR1-R9 remain open. Historical inventory, old OS1314 prerequisites and
all prior58090bodybytes remain. No whole-native/platform/upstream/IDE/nonWindows/
main/release acceptance follows; counts are nonadditive.
<!-- wrapped-propagation-20261020:end -->
'''.encode()
body=old+addition;assert len(body)<65536
body_file=temp/'ci-pr-7-body-wrapped-propagation-final.md';body_file.write_bytes(body)
subprocess.run(['git','push','origin',f'HEAD:refs/heads/{branch}'],cwd=root,check=True)
subprocess.run(['gh','pr','edit','7','--repo',repo,'--body-file',str(body_file)],cwd=root,check=True)
time.sleep(2)
after=api('repos/'+repo+'/pulls/7');actual=after['body'].encode()
assert after['head']['sha']==head and actual==body and actual.startswith(old)
assert after['state']=='open' and after['draft'] and after['base']['ref']=='main'
assert run(['git','ls-remote','--heads','origin',f'refs/heads/{branch}']).decode().split()[0]==head
remote=api('repos/'+repo+'/contents/docs/validation/integration-2026-10-19/wrapped-propagation/index.json?ref='+branch)
assert base64.b64decode(remote['content'])==index_raw
receipt={'verifiedAt':datetime.datetime.now(datetime.timezone.utc).isoformat(),'url':after['html_url'],'head':head,'branch':branch,
    'state':'open','draft':True,'base':'main','bodyBytes':len(actual),'bodySha256':sha(actual),
    'priorBodyBytes':len(old),'priorBodySha256':sha(old),'priorBodyPreserved':True,
    'indexSha256':sha(index_raw),'remoteIndexRawEqual':True,'sourceCommit':source['sourceCommit'],
    'runtimeCommit':source['runtimeCommit'],'mainOrReleaseVerified':False}
(base/'remote-verification.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8')
print(json.dumps(receipt),flush=True)
