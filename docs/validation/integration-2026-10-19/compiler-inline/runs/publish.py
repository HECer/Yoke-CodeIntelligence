from pathlib import Path
import subprocess,json,hashlib,base64,datetime
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');work=root.parent/'Yoke-CI-r1-attach';raw=root/'validation/integration-20261018/compiler-inline'
repo='HECer/Yoke-CodeIntelligence';branch='integrate/native-parity-github-app-20261018';index_path='docs/validation/integration-2026-10-19/compiler-inline/index.json'
sha=lambda b:hashlib.sha256(b).hexdigest()
def run(*args,cwd=root):return subprocess.check_output(args,cwd=cwd)
def read(p):return json.loads(p.read_text(encoding='utf-8-sig'))
final=read(raw/'final-r2/result.json');retained=read(raw/'retained-r1/result.json');assert final['allChecksPassed'] and retained['allChecksPassed'] and final['sourceUnchanged'] and retained['sourceUnchanged']
head=run('git','rev-parse','HEAD').decode().strip();assert head==run('git','rev-parse','HEAD',cwd=work).decode().strip();assert not run('git','status','--porcelain')
assert run('git','branch','--show-current').decode().strip()==branch
index=(root/index_path).read_bytes();assert index==(work/index_path).read_bytes();metadata=json.loads(index)
prior=json.loads(run('gh','pr','view','7','--repo',repo,'--json','body,headRefOid,headRefName,state,isDraft,url'))
assert prior['state']=='OPEN' and prior['headRefName']==branch
subprocess.run(['git','merge-base','--is-ancestor',prior['headRefOid'],head],cwd=root,check=True)
tot=final['commands'][0]['totals'];groups=final['commands'][0]['groups']
ts=(raw/'retained-r1/tests.log').read_text(encoding='utf-8')
import re
passed=int(re.findall(r'^(?:#|\u2139) pass (\d+)',ts,re.M)[-1]);skipped=int(re.findall(r'^(?:#|\u2139) skipped (\d+)',ts,re.M)[-1])
python_count=int(re.findall(r'Ran (\d+) tests?',(raw/'retained-r1/python-contracts.log').read_text(encoding='utf-8'))[-1])
block=f'''<!-- compiler-inline-checkpoint:start -->
## Native 0.5 immutable compiler inline checkpoint

Host-selected TypeScript 5.8.3 now prepares TS/TSX/JS/JSX function-binding inline plans through native sessions and `jb.inline_symbol`. Immutable compiler inputs, actual value/reference bindings, copied initializer structure, exact declaration/comma bundles and result diagnostics are checked before guarded preparation. `keepDefinition`, shorthand keys, JSX component shadowing and the existing non-TS LSP route have dedicated checks. Default sessions retain their previous identities and execution behavior.

- Source: `{metadata['sourceCommit']}`; current integrated head: `{head}`.
- All 18 new cases pass (16 integration / two unit), including twelve language/sibling scenarios and actual CLI, independent `tsc`, approval/application and conditional recovery. The owned non-TS LSP fixture is separately labeled.
- Final scoped native run: **{tot['passed']} passed / {tot['failed']} failed / {tot['ignored']} ignored**, {groups} groups, {final['inputs']} fixed inputs. Format, native build and all-target Clippy with warnings denied pass.
- Separate retained checks: Python{python_count}, TypeScript typecheck and **{passed} passed / {skipped} skipped**, {retained['frozenInputs']} fixed inputs.
- [Contract](native/docs/compiler-inline.md) and [raw evidence index]({index_path}); index SHA-256 `{sha(index)}`.

Earlier failed development/link attempts and the hash-qualified, nonrecursive disposable-cache cleanup remain archived. Counts are scoped and nonadditive. Classic function declarations, broader move/context/language contracts, dynamic consumers and runtime equivalence remain open. The earlier Windows symlink-privilege gate is unresolved; full current-head native/hosted/upstream acceptance and the rest of STATUS/R1-R9 are not complete. This remains an integration checkpoint, not a release or main merge.
<!-- compiler-inline-checkpoint:end -->

'''
old=prior['body'];start='<!-- compiler-inline-checkpoint:start -->';end='<!-- compiler-inline-checkpoint:end -->'
if start in old:
 a=old.index(start);b=old.index(end,a)+len(end);body=old[:a]+block+old[b:].lstrip('\r\n')
else:body=block+old
assert len(body.encode())<60_000
bodyfile=Path.home()/'AppData/Local/Temp/ci-pr-7-body-compiler-inline-final.md';bodyfile.write_bytes(body.encode('utf-8'))
(raw/'pr-before-publish.json').write_text(json.dumps(prior,indent=2)+'\n',encoding='utf-8')
subprocess.run(['git','push','origin',f'HEAD:refs/heads/{branch}'],cwd=root,check=True)
subprocess.run(['gh','pr','edit','7','--repo',repo,'--body-file',str(bodyfile)],cwd=root,check=True)
actual=json.loads(run('gh','pr','view','7','--repo',repo,'--json','body,headRefOid,headRefName,state,isDraft,url'))
remote=run('git','ls-remote','origin','refs/heads/'+branch).decode().split()[0]
assert remote==head==actual['headRefOid'] and actual['body']==bodyfile.read_bytes().decode('utf-8')
blob=json.loads(run('gh','api',f'repos/{repo}/contents/{index_path}?ref={head}'));server_index=base64.b64decode(blob['content']);assert server_index==index
proof={'verifiedAt':datetime.datetime.now(datetime.timezone.utc).isoformat(),'url':actual['url'],'head':head,'remoteBranchHead':remote,'sourceCommit':metadata['sourceCommit'],'state':actual['state'],'isDraft':actual['isDraft'],'bodyBytes':len(actual['body'].encode()),'bodySha256':sha(actual['body'].encode()),'indexPath':index_path,'indexBytes':len(index),'indexSha256':sha(index),'rawArtifactCount':len(metadata['files']),'mainMerged':False,'releasePerformed':False,'ciPolled':False}
(raw/'remote-verification.json').write_text(json.dumps(proof,indent=2)+'\n',encoding='utf-8');print(json.dumps(proof))
