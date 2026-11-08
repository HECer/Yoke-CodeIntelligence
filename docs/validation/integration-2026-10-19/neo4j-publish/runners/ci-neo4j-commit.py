from pathlib import Path
import subprocess,json,hashlib,re
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');work=root.parent/'Yoke-CI-r1-attach';base=root/'validation/integration-20261018/neo4j-publish';parent='58be008555e9d24df1af346245fe8249d8d4d998';sha=lambda b:hashlib.sha256(b).hexdigest()
def git(cwd,*args):return subprocess.check_output(['git',*args],cwd=cwd)
read=lambda n:json.loads((base/n).read_bytes())
assert read('quality-r2/result.json')['allChecksPassed'] and read('quality-r2/result.json')['sourceUnchanged']
native=read('final/result.json');assert native['sourceUnchanged'];prior=read('final/source-before.json');assert prior==read('final/source-after.json')
final_source=read('final-source-r2/result.json');assert final_source['allChecksPassed'] and final_source['sourceUnchanged'] and final_source['productionBinaryUnchanged'];before=read('final-source-r2/source-before.json');assert before==read('final-source-r2/source-after.json')
assert [n for n in before if before[n]!=prior.get(n)]==[final_source['bridge']['path']]
assert read('native-final-r1/frozen-inputs.json')['sourceUnchanged'] and read('native-final-r1/frozen-inputs.json')['binaryUnchanged'];assert read('native-final-r1/result.json')['ownedServerStopped'] and read('native-final-r1/result.json')['ownedCredentialsRemoved']==5
log=(base/'final/native.log').read_text(encoding='utf-8');groups=[list(map(int,m)) for m in re.findall(r'^test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;',log,re.M)];totals=dict(zip(['passed','failed','ignored'],map(sum,zip(*groups))))
failed=re.findall(r'^test (.*?) \.\.\. FAILED$',log,re.M);old=json.loads((root/'docs/validation/integration-2026-10-19/windows-process-exit/index.json').read_bytes());assert failed==old['failedCases'],failed;assert len(groups)==17 and totals['failed']==1 and '1314' in log
assert git(root,'rev-parse','HEAD').decode().strip()==parent and git(work,'rev-parse','HEAD').decode().strip()==parent
assert git(root,'branch','--show-current').decode().strip()=='integrate/native-parity-github-app-20261018'
assert git(work,'branch','--show-current').decode().strip()=='work/r7-neo4j-upsert-20261019'
assert not git(root,'status','--porcelain') and not git(work,'diff','--cached','--name-only')
for name,h in before.items():assert sha((work/name).read_bytes())==h,name
fixture='native/tests/fixtures/postgres-catalog-response.jsonl';fixture_sha='f67a05349a6cedf55222afd3bb3231c169d382dd4b078d76544daac517530b47';assert sha((work/fixture).read_bytes())==fixture_sha and sha((root/fixture).read_bytes())==fixture_sha
names=set(filter(None,git(work,'ls-files','-o','--exclude-standard').decode().splitlines()))|set(filter(None,git(work,'diff','--name-only').decode().splitlines()))
paths=sorted(n for n in names if (n.startswith('native/') and ('neo4j' in n or n in ['native/src/main.rs','native/src/lib.rs','native/src/host_credentials.rs'])) or n=='docs/native-neo4j-publish.md');assert set(paths)==names,names-set(paths)
subprocess.run(['git','add','--',*paths],cwd=work,check=True)
subprocess.run(['git','-c','core.whitespace=cr-at-eol','diff','--cached','--check'],cwd=work,check=True)
subprocess.run(['git','commit','-m','feat(native): add reviewed Neo4j snapshot publication'],cwd=work,check=True)
source=git(work,'rev-parse','HEAD').decode().strip();(base/'source-commit.json').write_text(json.dumps({'sourceCommit':source,'parentCommit':parent,'paths':paths})+'\n')
subprocess.run(['git','merge','--no-ff','-m','Merge reviewed native Neo4j publication checkpoint',source],cwd=root,check=True)
runtime=git(root,'rev-parse','HEAD').decode().strip();assert sha((root/fixture).read_bytes())==fixture_sha
result={'sourceCommit':source,'runtimeCommit':runtime,'parentCommit':parent,'paths':paths,'runtimeCodeChanged':True,'newCases':21,'nativeGroups':groups,'nativeTotals':totals,'failedCases':failed,'testOnlySourceBridge':final_source['bridge'],'productionBinaryUnchangedAcrossTestImportBridge':True,'wholeNativeRerun':False};(base/'source-integration.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result),flush=True)
