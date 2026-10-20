from pathlib import Path
import subprocess,json,hashlib
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence')
work=root.parent/'Yoke-CI-r1-attach'
base=root/'validation/integration-20261018/plan-feedback'
parent='afb37a3e46e7563ee9975de181b7be18733e5cad'
sha=lambda b:hashlib.sha256(b).hexdigest()
read=lambda n:json.loads((base/n).read_bytes())
def git(cwd,*args):return subprocess.check_output(['git','-c','core.longpaths=true',*args],cwd=cwd)
final=read('final-r2/result.json');broad=read('final/result.json')
assert final['sourceUnchanged'] and broad['sourceUnchanged']
assert final['commands'][0]['failures']==['windows_host_aliases_cannot_read_protected_files_or_alternate_streams']
assert all(c['exitCode']==0 for c in final['commands'][1:])
assert [c['name'] for c in final['commands']]==['native','format','build','clippy']
assert broad['commands'][0]['failures']==['windows_host_aliases_cannot_read_protected_files_or_alternate_streams','blob_links_are_refused_and_external_targets_are_preserved','planned_result_views_replay_exact_unicode_crlf_without_application_or_execution']
assert all(c['exitCode']==0 for c in broad['commands'][1:])
before=read('final-r2/source-before.json')
assert before==read('final-r2/source-after.json')
old=read('final/source-before.json');assert old==read('final/source-after.json')
assert [n for n in before if before[n]!=old.get(n)]==['native/tests/plan_feedback.rs']
probe=read('final/native-test-binary.json')
binary=sha((root/'target/debug/yoke.exe').read_bytes())
assert binary==probe['sha256']
assert git(root,'rev-parse','HEAD').decode().strip()==parent
assert git(work,'rev-parse','HEAD').decode().strip()==parent
assert git(root,'branch','--show-current').decode().strip()=='integrate/native-parity-github-app-20261018'
assert git(work,'branch','--show-current').decode().strip()=='work/r9-plan-feedback-20261020'
assert not git(root,'status','--porcelain') and not git(work,'diff','--cached','--name-only')
for name,h in before.items():assert sha((work/name).read_bytes())==h,name
fixture='native/tests/fixtures/postgres-catalog-response.jsonl'
fixture_sha='f67a05349a6cedf55222afd3bb3231c169d382dd4b078d76544daac517530b47'
for cwd in [root,work]:assert sha((cwd/fixture).read_bytes())==fixture_sha and (cwd/fixture).stat().st_size==2779
expected={'native/README.md','native/docs/outcome-feedback.md','native/src/feedback_cli.rs',
    'native/src/knowledge_feedback.rs','native/src/retention.rs','native/src/plan_feedback.rs',
    'native/src/plan_feedback_rank.rs','native/src/plan_feedback_store.rs','native/tests/plan_feedback.rs'}
names=set(filter(None,git(work,'ls-files','-o','--exclude-standard').decode().splitlines()))|set(filter(None,git(work,'diff','--name-only').decode().splitlines()))
assert names==expected,names^expected
paths=sorted(expected)
subprocess.run(['git','add','--',*paths],cwd=work,check=True,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
assert set(git(work,'diff','--cached','--name-only').decode().splitlines())==expected
subprocess.run(['git','-c','core.whitespace=cr-at-eol','diff','--cached','--check'],cwd=work,check=True)
subprocess.run(['git','commit','-m','feat(native): bind authored outcomes to saved change plans'],cwd=work,check=True,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
source=git(work,'rev-parse','HEAD').decode().strip()
(base/'source-commit.json').write_text(json.dumps({'sourceCommit':source,'parentCommit':parent,'paths':paths})+'\n',encoding='utf-8')
subprocess.run(['git','-c','core.longpaths=true','merge','--no-ff','-m','Merge source-bound saved-plan outcome checkpoint',source],cwd=root,check=True,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
runtime=git(root,'rev-parse','HEAD').decode().strip()
entries=[]
for name,h in before.items():
    old=(work/name).read_bytes();current=(root/name).read_bytes()
    assert sha(old)==h and (old==current or old.replace(b'\r\n',b'\n')==current.replace(b'\r\n',b'\n')),name
    if name==fixture:assert old==current and sha(current)==fixture_sha
    entries.append({'path':name,'scopedSha256':h,'integratedSha256':sha(current),'materialization':'raw_equal' if old==current else 'crlf_only_checkout_materialization'})
(base/'source-materialization.json').write_text(json.dumps(entries,indent=2)+'\n',encoding='utf-8')
result={'sourceCommit':source,'runtimeCommit':runtime,'parentCommit':parent,'paths':paths,'newCases':14,
    'broadNativeGroups':broad['commands'][0]['groups'],'broadNativeTotals':broad['commands'][0]['totals'],
    'finalSelectedGroups':final['commands'][0]['groups'],'finalSelectedTotals':final['commands'][0]['totals'],
    'nativeInputsMatched':len(entries),'nativeBinarySha256':binary,
    'productionBinaryUnchangedFromBroadNativeTests':True,'wholeNativeRerun':False,
    'testOnlyBridge':['native/tests/plan_feedback.rs']}
(base/'source-integration.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
print(json.dumps(result),flush=True)
