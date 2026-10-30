from pathlib import Path
import subprocess,json,hashlib,re
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');work=root.parent/'Yoke-CI-r1-attach';base=root/'validation/integration-20261018/falkor-publish';parent='dd69e53b9218f75018b879108fe13a2883b4366f';sha=lambda b:hashlib.sha256(b).hexdigest();read=lambda n:json.loads((base/n).read_bytes())
def git(cwd,*args):return subprocess.check_output(['git',*args],cwd=cwd)
quality=read('quality-r5/result.json');assert quality['sourceUnchanged'] and len(quality['runs'])==4;native=quality['runs'][0];assert native['onlyKnownOs1314Failure'] and native['groups']==20 and native['totals']=={'passed':580,'failed':1,'ignored':8};assert all(r['exitCode']==0 for r in quality['runs'][1:])
before=read('quality-r5/source-before.json');assert before==read('quality-r5/source-after.json')
for phase,stopped in [('native-final-r2','ownedQemuStopped'),('neo4j-compatibility-r1','ownedServerStopped')]:
 proof=read(phase+'/frozen-inputs.json');actual=read(phase+'/result.json');assert proof['sourceUnchanged'] and proof['binaryUnchanged'] and actual[stopped] and actual['ownedCredentialsRemoved']==5;assert read(phase+'/source-before.json')==before==read(phase+'/source-after.json');assert actual['nativeBinarySha256']==proof['binarySha256']=='5e5240250fc94c705d8d2c9db08b0f6b522f641901489d0bb2a4e00595c67f2f'
assert git(root,'rev-parse','HEAD').decode().strip()==parent and git(work,'rev-parse','HEAD').decode().strip()==parent
assert git(root,'branch','--show-current').decode().strip()=='integrate/native-parity-github-app-20261018' and git(work,'branch','--show-current').decode().strip()=='work/r7-falkor-upsert-20261019'
assert not git(root,'status','--porcelain') and not git(work,'diff','--cached','--name-only')
for name,h in before.items():assert sha((work/name).read_bytes())==h,name
fixture='native/tests/fixtures/postgres-catalog-response.jsonl';fixture_sha='f67a05349a6cedf55222afd3bb3231c169d382dd4b078d76544daac517530b47';assert sha((work/fixture).read_bytes())==fixture_sha and sha((root/fixture).read_bytes())==fixture_sha
names=set(filter(None,git(work,'ls-files','-o','--exclude-standard').decode().splitlines()))|set(filter(None,git(work,'diff','--name-only').decode().splitlines()))
paths=sorted(n for n in names if n.startswith('native/') and ('falkor' in n or n in ['native/src/main.rs','native/src/lib.rs','native/src/host_credentials.rs','native/src/neo4j_publish.rs']));assert set(paths)==names and len(paths)==20,names-set(paths)
subprocess.run(['git','add','--',*paths],cwd=work,check=True);subprocess.run(['git','-c','core.whitespace=cr-at-eol','diff','--cached','--check'],cwd=work,check=True);subprocess.run(['git','commit','-m','feat(native): add reviewed FalkorDB snapshot publication'],cwd=work,check=True)
source=git(work,'rev-parse','HEAD').decode().strip();(base/'source-commit.json').write_text(json.dumps({'sourceCommit':source,'parentCommit':parent,'paths':paths})+'\n',encoding='utf-8')
subprocess.run(['git','merge','--no-ff','-m','Merge reviewed native FalkorDB publication checkpoint',source],cwd=root,check=True);runtime=git(root,'rev-parse','HEAD').decode().strip();entries=[]
for name,h in before.items():
 old=(work/name).read_bytes();current=(root/name).read_bytes();assert sha(old)==h and (old==current or old.replace(b'\r\n',b'\n')==current.replace(b'\r\n',b'\n')),name
 if name==fixture:assert old==current and sha(current)==fixture_sha
 entries.append({'path':name,'scopedSha256':h,'integratedSha256':sha(current),'materialization':'raw_equal' if old==current else 'crlf_only_checkout_materialization'})
(base/'source-materialization.json').write_text(json.dumps(entries,indent=2)+'\n',encoding='utf-8');result={'sourceCommit':source,'runtimeCommit':runtime,'parentCommit':parent,'paths':paths,'runtimeCodeChanged':True,'newCases':25,'nativeGroups':20,'nativeTotals':native['totals'],'failedCases':native['failures'],'nativeInputsMatched':len(entries),'nativeBinarySha256':'5e5240250fc94c705d8d2c9db08b0f6b522f641901489d0bb2a4e00595c67f2f','wholeNativeRerun':False};(base/'source-integration.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8');print(json.dumps(result),flush=True)
