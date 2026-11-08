"""Archive exact existing evidence and verify final runtime source correspondence."""
from pathlib import Path
import hashlib,json,shutil,subprocess
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r7-google');p=w.parent/'Yoke-CodeIntelligence'
raw=w/'validation/google-workspace';base=p/'validation/integration-20261018/full-native-default-20261021-r1'
out=w/'docs/validation/integration-2026-10-21/google-workspace';out.mkdir(parents=True,exist_ok=False)
sha=lambda b:hashlib.sha256(b).hexdigest()
read=lambda path:json.loads(path.read_text(encoding='utf-8'))
r2=read(raw/'r2/result.json');r3=read(raw/'r3/result.json');assert r3['allChecksPassed']
before2=read(raw/'r2/source-before.json');before3=read(raw/'r3/source-before.json')
changed=[n for n in before2 if before2[n]!=before3[n]]
assert changed==['native/tests/google_workspace_process.rs'] and before2.keys()==before3.keys()
current=(w/changed[0]).read_bytes()
assert sha(current.replace(b'yoke::model::hash(fs::read(&program).unwrap())',b'yoke::model::hash(&fs::read(&program).unwrap())'))==before2[changed[0]]
metadata_only=[]
for n,h in before3.items():
    if sha((w/n).read_bytes())!=h:
        assert n=='native/docs/google-workspace.md';metadata_only.append(n)
selected=[c for c in r2['checks'] if c['name'] not in ['clippy','google_workspace_process']]+[c for c in r3['checks'] if c['name']!='fmt']
assert all(c['exitCode']==0 and c['sourceUnchanged'] for c in selected)
total={k:sum(c['totals'][k] for c in selected) for k in ['passed','failed','ignored']};assert total=={'passed':33,'failed':0,'ignored':0}
def copy(src,relative):
    dest=out/relative;dest.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(src,dest)
    assert dest.read_bytes()==src.read_bytes()
for phase in ['r1','r2','r3','gws-adc-guard-r1']:
    for f in sorted((raw/phase).iterdir()):
        if f.suffix in ['.json','.log','.py']:copy(f,Path('r7')/phase/f.name)
for name in ['result.json','manifest.json','source-before.json','tools.json','runner.py','examples.log']:
    if (base/name).is_file():copy(base/name,Path('baseline')/name)
baseline=read(base/'result.json');assert baseline['sourceUnchanged'] and baseline['completedGroups']==310
for group in baseline['groups']:
    if not group['allChecksPassed']:
        stage=base/(str(group['index']).zfill(3)+'-'+group['group'])
        for name in ['result.json','test.log'] if 'totals' in group else ['result.json','compile.log']:
            copy(stage/name,Path('baseline/failures')/stage.name/name)
for phase in ['full-native-digest-history-alternate-20261021-r1','full-native-digest-history-alternate-20261021-r2']:
    src=p/'validation/integration-20261018'/phase
    for f in sorted(src.iterdir()):
        if f.suffix in ['.json','.log','.py']:copy(f,Path('alternate')/phase/f.name)
assessment=p/'validation/integration-20261018/full-status-preflight/full-native-assessment-20261021-r1'
for f in assessment.glob('*.json'):copy(f,Path('baseline/assessment')/f.name)
npm=Path.home()/'AppData/Local/Temp/yoke-r7-npm-test-r2.log';data=npm.read_bytes()
text=data.decode('utf-16') if data.startswith((b'\xff\xfe',b'\xfe\xff')) else data.decode('utf-8')
assert 'pass 260' in text and 'fail 0' in text and 'skipped 3' in text
copy(npm,Path('retained/npm-test-r2.log'))
copy(Path(__file__),Path('archive-runner.py'))
summary={'baseCommit':'6951ff94aee42e867329a2a042204c13c670b788','nativeScopedTotals':total,
 'selectedPassingChecks':selected,'r2ToR3ChangedOnly':changed,'r2ToR3Correction':'Remove one needless borrow in the owned test fixture; exact byte reconstruction verified.',
 'runtimeInputsUnchangedAfterFinalValidation':True,'metadataOnlyAfterValidation':metadata_only,
 'retainedTypecheck':{'command':'npm.cmd run typecheck','observedExitCode':0,'evidence':'Actual tool transcript; no reconstructed stdout log.'},
 'retainedTests':{'command':'npm.cmd test','observedExitCode':0,'passed':260,'skipped':3,'log':'retained/npm-test-r2.log'},
 'initialSparseRetainedRun':'Failed because benchmarks/extensions were absent; checkout extended before complete successful rerun.',
 'baselineTotals':baseline['totals'],'baselineAll310GroupsAttempted':True,
 'alternateDigestHistory':{'passed':13,'failed':0,'standardCargoTest':False,'onlyOutputSuffixChanged':True},
 'cliSha256':r3['cliSha256'],'gwsAdcGuard':read(raw/'gws-adc-guard-r1/result.json'),
 'resourceKeyTransportAvailable':False,'liveGoogleExportValidated':False,'fullWindowsCertified':False,'fullStatusCertified':False}
(out/'checkpoint.json').write_text(json.dumps(summary,indent=2)+'\n',encoding='utf-8')
records=[{'path':str(f.relative_to(out)).replace('\\','/'),'bytes':f.stat().st_size,'sha256':sha(f.read_bytes())} for f in sorted(out.rglob('*')) if f.is_file()]
(out/'index.json').write_text(json.dumps({'files':records,'copiedRawFilesByteExact':True},indent=2)+'\n',encoding='utf-8')
print(json.dumps({'files':len(records)+1,'bytes':sum(x['bytes'] for x in records),'scopedTests':total,'runtimeUnchanged':True,'evidence':str(out)}))
