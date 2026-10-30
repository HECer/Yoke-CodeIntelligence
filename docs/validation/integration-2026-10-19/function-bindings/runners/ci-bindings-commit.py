from pathlib import Path
import subprocess,json,hashlib,re,datetime
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');work=root.parent/'Yoke-CI-r1-attach'
base=root/'validation/integration-20261018/function-bindings';parent='2bc4df49423332497a14118cbd45d72c32ecb352'
sha=lambda b:hashlib.sha256(b).hexdigest();read=lambda n:json.loads((base/n).read_bytes())
def git(cwd,*args):return subprocess.check_output(['git','-c','core.longpaths=true',*args],cwd=cwd)
final=read('final/result.json');actual=read('live-final/result.json')
assert all(r['allChecksPassed'] and r['sourceUnchanged'] for r in [final,actual])
assert final['commands'][0]['groups']==15 and final['commands'][0]['totals']['failed']==0
assert actual['commands'][0]['totals']=={'passed':2,'failed':0,'ignored':0}
before=read('final/source-before.json');assert len(before)==738
for name in ['final/source-after.json','live-final/source-before.json','live-final/source-after.json']:assert before==read(name),name
assert git(root,'rev-parse','HEAD').decode().strip()==parent and git(work,'rev-parse','HEAD').decode().strip()==parent
assert git(root,'branch','--show-current').decode().strip()=='integrate/native-parity-github-app-20261018'
assert git(work,'branch','--show-current').decode().strip()=='work/r1-function-bindings-20261020'
assert not git(root,'status','--porcelain') and not git(work,'diff','--cached','--name-only')
for name,h in before.items():assert sha((work/name).read_bytes())==h,name
fixture='native/tests/fixtures/postgres-catalog-response.jsonl';fixture_sha='f67a05349a6cedf55222afd3bb3231c169d382dd4b078d76544daac517530b47'
for cwd in [root,work]:assert sha((cwd/fixture).read_bytes())==fixture_sha and (cwd/fixture).stat().st_size==2779
expected={'native/README.md','native/docs/propagated-delete.md','native/src/semantic_propagated_delete.rs','native/tests/fixtures/propagated-delete-lsp.mjs','native/tests/propagated_delete_contract.rs','native/tests/session_refactor.rs'}
names=set(filter(None,git(work,'ls-files','-o','--exclude-standard').decode().splitlines()))|set(filter(None,git(work,'diff','--name-only').decode().splitlines()))
assert names==expected,names^expected;paths=sorted(expected)
binary=root/'target/debug/yoke.exe';binary_raw=binary.read_bytes();binary_sha=sha(binary_raw)
actual_hashes=re.findall(r'actual-native-session-cli-executable=.* sha256=([a-f0-9]{64})',(base/'live-final/native.log').read_text(encoding='utf-8'))
assert actual_hashes and set(actual_hashes)=={binary_sha}
subprocess.run(['git','add','--',*paths],cwd=work,check=True,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
assert set(git(work,'diff','--cached','--name-only').decode().splitlines())==expected
subprocess.run(['git','-c','core.whitespace=cr-at-eol','diff','--cached','--check'],cwd=work,check=True)
subprocess.run(['git','commit','-m','feat(native): propagate checked function-valued binding deletion'],cwd=work,check=True,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
source=git(work,'rev-parse','HEAD').decode().strip()
(base/'source-commit.json').write_text(json.dumps({'sourceCommit':source,'parentCommit':parent,'paths':paths})+'\n',encoding='utf-8')
subprocess.run(['git','-c','core.longpaths=true','merge','--no-ff','-m','Merge explicit function binding propagation policy',source],cwd=root,check=True,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
runtime=git(root,'rev-parse','HEAD').decode().strip();entries=[]
for name,h in before.items():
    old=(work/name).read_bytes();new=(root/name).read_bytes()
    assert sha(old)==h and (old==new or old.replace(b'\r\n',b'\n')==new.replace(b'\r\n',b'\n')),name
    if name==fixture:assert old==new and sha(new)==fixture_sha
    entries.append({'path':name,'scopedSha256':h,'integratedSha256':sha(new),'materialization':'raw_equal' if old==new else 'crlf_only_checkout_materialization'})
(base/'source-materialization.json').write_text(json.dumps(entries,indent=2)+'\n',encoding='utf-8')
result={'sourceCommit':source,'runtimeCommit':runtime,'parentCommit':parent,'paths':paths,
    'newCases':{'defaultIntegration':10,'actualFunctionBindingsCli':1},
    'nativeGroups':final['commands'][0]['groups'],'nativeTotals':final['commands'][0]['totals'],
    'nativeInputsMatched':len(entries),'nativeBinarySha256':binary_sha,'nativeBinaryBytes':len(binary_raw),
    'actualCliExecutableHashMatchesFinalBinary':True,'binaryHashObservedAt':datetime.datetime.now(datetime.timezone.utc).isoformat(),
    'actualTools':read('live-final/tool-runtimes.json'),'wholeNativeRerun':False}
(base/'source-integration.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
print(json.dumps(result),flush=True)
