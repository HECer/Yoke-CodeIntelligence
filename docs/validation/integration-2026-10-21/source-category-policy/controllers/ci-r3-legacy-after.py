"""Actual CLI old/new identity control, after the selected guard is terminal."""
from pathlib import Path
import hashlib,json,subprocess,sys,tempfile
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');raw=w/'validation/integration-20261018/r3-source-categories'
guard=json.loads((raw/sys.argv[1]/'result.json').read_text());assert guard['allChecksPassed'] and guard['sourceUnchanged']
before=json.loads((raw/'legacy-before-r1/result.json').read_text());owned=Path(before['ownedRoot'])
assert owned.name=='yoke-owned-r3-policy-xk79eno_' and owned.resolve().is_relative_to(Path(tempfile.gettempdir()).resolve())
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
assert all(sha(owned/n)==h for n,h in before['sourceFiles'].items())
exe=w/'target/debug/yoke.exe';digest=sha(exe);assert digest==guard['cliSha256']
out=raw/'legacy-after-r1';out.mkdir(exist_ok=False)
policy={'schemaVersion':1,'resolutionCategories':{'syntactic':'extracted','heuristic':'inferred','inferred_concept':'inferred','ambiguous':'ambiguous'},
    'relationWeights':{},'excludedKinds':['file','concept','semantic_concept'],'excludedLabels':[],
    'excludeStubLabels':False,'extensionlessAsConcepts':False}
options=out/'options.json';options.write_text(json.dumps({'sourceCategoryPolicy':policy})+'\n')
reports=[];commands=[]
for label,args in [('legacy',[]),('policy',['--options',str(options)])]:
    command=[str(exe),'--root',str(owned),'--snapshot',before['defaultReport']['snapshotId'],'--timeout','30000','--max-bytes','1000000','graph','analyze',*args]
    p=subprocess.run(command,cwd=w,stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=45,creationflags=0x08000000)
    (out/(label+'.stdout')).write_bytes(p.stdout);(out/(label+'.stderr')).write_bytes(p.stderr)
    commands.append({'label':label,'command':command,'exitCode':p.returncode});assert p.returncode==0
    reports.append(json.loads(p.stdout))
assert reports[0]==before['defaultReport'], 'Legacy report or artifact identities changed'
assert reports[1]['producer']=='yoke-source-category-report-1'
assert reports[1]['artifactId']!=reports[0]['artifactId']
for key in ['sourceBindings','evidenceAnchors','graphHash','snapshotId']:
    assert reports[0][key]==reports[1][key]
assert reports[0]['audit']['byResolution']==reports[1]['audit']['byResolution']
assert sha(exe)==digest and all(sha(owned/n)==h for n,h in before['sourceFiles'].items())
result={'guard':sys.argv[1],'commands':commands,'cliSha256':digest,'legacyReportByteValueAndArtifactIdsExact':True,
    'legacyRawStdoutExact':(out/'legacy.stdout').read_bytes()==(raw/'legacy-before-r1/report-1.stdout').read_bytes(),
    'policyProducer':reports[1]['producer'],'policyArtifactId':reports[1]['artifactId'],
    'originalSourceEvidenceAndGraphPreserved':True,'sourceUnchanged':True,'modelCalls':0,'analyzedSourceExecuted':False}
(out/'result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result))
