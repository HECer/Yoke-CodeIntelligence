from pathlib import Path
import subprocess,json,hashlib,datetime
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence')
work=root.parent/'Yoke-CI-r1-attach'
base=root/'validation/integration-20261018/mixed-meaning-composition'
parent='87be1a19ae1556099352fc7400034fdd109acae6'
sha=lambda b:hashlib.sha256(b).hexdigest()
def git(cwd,*args):return subprocess.check_output(['git','-c','core.longpaths=true',*args],cwd=cwd,stderr=subprocess.PIPE)
read=lambda n:json.loads((base/n).read_bytes())
final=read('final/result.json')
assert final['allChecksPassed'] and final['sourceUnchanged']
assert final['commands'][0]['groups']==20 and final['commands'][0]['totals']['failed']==0
assert git(root,'rev-parse','HEAD').decode().strip()==parent and git(work,'rev-parse','HEAD').decode().strip()==parent
assert git(work,'branch','--show-current').decode().strip()=='work/r4-mixed-meaning-composition-20261020'
assert git(root,'branch','--show-current').decode().strip()=='integrate/native-parity-github-app-20261018'
assert not git(root,'status','--porcelain') and not git(work,'diff','--cached','--name-only')
paths=sorted(['native/README.md','native/docs/mixed-meaning-queries.md','native/docs/mixed-meaning-composition.md','native/src/mixed_meaning_cli.rs','native/src/mixed_meaning_provider.rs','native/src/mixed_meaning_query.rs','native/src/mixed_meaning_composition.rs','native/tests/mixed_meaning_composition.rs'])
changed=set(git(work,'diff','--name-only').decode().splitlines())|set(git(work,'ls-files','-o','--exclude-standard').decode().splitlines())
assert changed==set(paths),changed^set(paths)
before=read('final/source-before.json');assert before==read('final/source-after.json')
for name,h in before.items():assert sha((work/name).read_bytes())==h,name
fixture='native/tests/fixtures/postgres-catalog-response.jsonl'
expected='f67a05349a6cedf55222afd3bb3231c169d382dd4b078d76544daac517530b47'
for cwd in [root,work]:assert sha((cwd/fixture).read_bytes())==expected and (cwd/fixture).stat().st_size==2779
binary=root/'target/debug/yoke.exe';binary_hash=sha(binary.read_bytes())
git(work,'add','--',*paths)
assert set(git(work,'diff','--cached','--name-only').decode().splitlines())==set(paths)
git(work,'-c','core.whitespace=cr-at-eol','diff','--cached','--check')
git(work,'commit','-m','feat(native): compose checked mixed interpretation graph selections')
source=git(work,'rev-parse','HEAD').decode().strip()
git(root,'merge','--no-ff','-m','Merge checked mixed interpretation graph composition',source)
runtime=git(root,'rev-parse','HEAD').decode().strip()
materialization=[]
for name,h in before.items():
    old=(work/name).read_bytes();new=(root/name).read_bytes()
    assert sha(old)==h and old.replace(b'\r\n',b'\n')==new.replace(b'\r\n',b'\n'),name
    if name==fixture:assert old==new and sha(new)==expected
    materialization.append({'path':name,'scopedSha256':h,'integratedSha256':sha(new),'materialization':'raw_equal' if old==new else 'crlf_only_checkout_materialization'})
(base/'source-materialization.json').write_text(json.dumps(materialization,indent=2)+'\n',encoding='utf-8')
result={'sourceCommit':source,'runtimeCommit':runtime,'parentCommit':parent,'paths':paths,'nativeInputsMatched':len(before),
    'nativeGroups':final['commands'][0]['groups'],'nativeTotals':final['commands'][0]['totals'],
    'newCases':{'defaultIntegration':8,'actualNativeCliIncludedInDefault':1},
    'nativeBinarySha256':binary_hash,'nativeBinaryBytes':binary.stat().st_size,
    'binaryHashObservedAt':datetime.datetime.now(datetime.timezone.utc).isoformat(),
    'wholeNativeRerun':False,'fullR4Complete':False}
(base/'source-integration.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
assert not git(root,'status','--porcelain') and not git(work,'status','--porcelain')
print(json.dumps(result),flush=True)
