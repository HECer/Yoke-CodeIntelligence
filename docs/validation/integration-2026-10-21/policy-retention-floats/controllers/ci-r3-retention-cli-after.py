from pathlib import Path
import json,hashlib,subprocess
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');raw=w/'validation/integration-20261018/r3-policy-retention';before=json.loads((raw/'before-r1/result.json').read_text());guard=json.loads((raw/'green-r4/result.json').read_text());assert guard['allChecksPassed'] and guard['sourceUnchanged']
exe=w/'target/debug/yoke.exe';sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest();binary=sha(exe);assert binary==guard['cliSha256'];owned=Path(before['ownedRoot']);assert all(sha(owned/n)==h for n,h in before['sourceFiles'].items())
out=raw/'after-r1';out.mkdir(exist_ok=False);rows=[];reports=[]
def run(name,argv):
    p=subprocess.run(argv,cwd=w,stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=45,creationflags=0x08000000);(out/(name+'.stdout')).write_bytes(p.stdout);(out/(name+'.stderr')).write_bytes(p.stderr);rows.append({'name':name,'command':argv,'exitCode':p.returncode});assert p.returncode==0,(name,p.returncode);return p.stdout
for row in before['commands']:
    name=row['name'];reply=run(name,row['command']);assert reply==(raw/'before-r1'/(name+'.stdout')).read_bytes(),name+' v1 changed';reports.append(json.loads(reply))
for name,args in [('retention-plan',['plan']),('retention-purge-plan',['purge-plan'])]:
    argv=[str(exe),'--root',str(owned),'--timeout','30000','--max-bytes','1000000','retention',*args,'--minimum-age-ms','0'];reply=json.loads(run(name,argv));assert reply['unknownDependencyKinds']==[]
    if name=='retention-plan':assert reply['snapshotIds']==[]
    else:
        for item in reply['items']:
            for report in reports:assert item['blobId'] not in [report['artifactId'],report['markdownArtifactId']]
assert sha(exe)==binary and all(sha(owned/n)==h for n,h in before['sourceFiles'].items())
result={'cliSha256':binary,'legacyAndCategoryV1RawStdoutExact':True,'savedRootRetentionPlansPassed':True,'sourceUnchanged':True,'analyzedSourceExecuted':False,'sourceFiles':before['sourceFiles'],'commands':rows,'oldProducersAndArtifactsPreserved':True}
(out/'result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps({k:v for k,v in result.items() if k not in ['commands','sourceFiles']}))
