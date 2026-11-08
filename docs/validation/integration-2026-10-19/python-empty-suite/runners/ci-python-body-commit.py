from pathlib import Path
import subprocess,json,hashlib
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence')
work=root.parent/'Yoke-CI-r1-attach';base=root/'validation/integration-20261018/python-empty-suite'
parent='87e7cfb276c481d3692b4e142dd39f9f22e81b5e'
sha=lambda b:hashlib.sha256(b).hexdigest();read=lambda n:json.loads((base/n).read_bytes())
def git(cwd,*args):return subprocess.check_output(['git','-c','core.longpaths=true',*args],cwd=cwd)
final=read('final-r2/result.json');prior=read('final/result.json');live=read('live-final-r2/result.json')
assert final['allChecksPassed'] and final['sourceUnchanged'] and live['allChecksPassed'] and live['sourceUnchanged']
assert prior['sourceUnchanged'] and prior['commands'][0]['failures']==['mcp_http::rejection_tests::idle_peer_cannot_extend_the_original_physical_deadline']
assert all(c['exitCode']==0 for c in prior['commands'][1:])
before=read('final-r2/source-before.json')
assert before==read('final-r2/source-after.json')==read('live-final-r2/source-before.json')==read('live-final-r2/source-after.json')
old=read('final/source-before.json');assert old==read('final/source-after.json')
assert [n for n in before if before[n]!=old.get(n)]==['native/src/mcp_http_rejection_tests.rs']
binary=sha((root/'target/debug/yoke.exe').read_bytes())
assert binary==read('final/native-test-binary.json')['sha256']
assert git(root,'rev-parse','HEAD').decode().strip()==parent and git(work,'rev-parse','HEAD').decode().strip()==parent
assert git(root,'branch','--show-current').decode().strip()=='integrate/native-parity-github-app-20261018'
assert git(work,'branch','--show-current').decode().strip()=='work/r1-python-empty-suite-20261020'
assert not git(root,'status','--porcelain') and not git(work,'diff','--cached','--name-only')
for name,h in before.items():assert sha((work/name).read_bytes())==h,name
fixture='native/tests/fixtures/postgres-catalog-response.jsonl';fixture_sha='f67a05349a6cedf55222afd3bb3231c169d382dd4b078d76544daac517530b47'
for cwd in [root,work]:assert sha((cwd/fixture).read_bytes())==fixture_sha and (cwd/fixture).stat().st_size==2779
expected={'native/README.md','native/docs/semantic-refactor-intents.md','native/docs/symbol-deletion.md',
    'native/src/changes.rs','native/src/changes_symbol_delete.rs','native/src/protocol.rs',
    'native/src/validation.rs','native/src/mcp_http_rejection_tests.rs','native/tests/python_empty_suite.rs'}
names=set(filter(None,git(work,'ls-files','-o','--exclude-standard').decode().splitlines()))|set(filter(None,git(work,'diff','--name-only').decode().splitlines()))
assert names==expected,names^expected
paths=sorted(expected)
subprocess.run(['git','add','--',*paths],cwd=work,check=True,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
assert set(git(work,'diff','--cached','--name-only').decode().splitlines())==expected
subprocess.run(['git','-c','core.whitespace=cr-at-eol','diff','--cached','--check'],cwd=work,check=True)
subprocess.run(['git','commit','-m','feat(native): add explicit Python empty-suite deletion policy'],cwd=work,check=True,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
source=git(work,'rev-parse','HEAD').decode().strip()
(base/'source-commit.json').write_text(json.dumps({'sourceCommit':source,'parentCommit':parent,'paths':paths})+'\n',encoding='utf-8')
subprocess.run(['git','-c','core.longpaths=true','merge','--no-ff','-m','Merge explicit Python empty-suite deletion checkpoint',source],cwd=root,check=True,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
runtime=git(root,'rev-parse','HEAD').decode().strip();entries=[]
for name,h in before.items():
    old=(work/name).read_bytes();current=(root/name).read_bytes()
    assert sha(old)==h and (old==current or old.replace(b'\r\n',b'\n')==current.replace(b'\r\n',b'\n')),name
    if name==fixture:assert old==current and sha(current)==fixture_sha
    entries.append({'path':name,'scopedSha256':h,'integratedSha256':sha(current),'materialization':'raw_equal' if old==current else 'crlf_only_checkout_materialization'})
(base/'source-materialization.json').write_text(json.dumps(entries,indent=2)+'\n',encoding='utf-8')
result={'sourceCommit':source,'runtimeCommit':runtime,'parentCommit':parent,'paths':paths,
    'newCases':{'unitAdmission':1,'defaultIntegration':8,'actualPythonAstOnly':1},
    'nativeGroups':final['commands'][0]['groups'],'nativeTotals':final['commands'][0]['totals'],
    'nativeInputsMatched':len(entries),'nativeBinarySha256':binary,
    'actualPython':read('live-final-r2/python-runtime.json'),
    'productionBinaryUnchangedFromPriorBroadNativeTests':True,
    'testOnlyCorrection':['native/src/mcp_http_rejection_tests.rs'],'wholeNativeRerun':False}
(base/'source-integration.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
print(json.dumps(result),flush=True)
