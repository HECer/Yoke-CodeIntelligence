from pathlib import Path
import json,hashlib,shutil
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');raw=w/'validation/integration-20261018/r3-source-categories'
out=w/'docs/validation/integration-2026-10-21/source-category-policy';out.mkdir(parents=True,exist_ok=False)
read=lambda p:json.loads(p.read_text(encoding='utf-8'));sha=lambda b:hashlib.sha256(b).hexdigest()
r2=read(raw/'guard-r2/result.json');r3=read(raw/'guard-r3/result.json');after=read(raw/'legacy-after-r1/result.json')
assert r3['allChecksPassed'] and after['legacyRawStdoutExact'] and after['originalSourceEvidenceAndGraphPreserved']
a=read(raw/'guard-r2/source-before.json');b=read(raw/'guard-r3/source-before.json')
changed=[n for n in a if a[n]!=b[n]];assert a.keys()==b.keys() and changed==['native/tests/graph_analysis_policy.rs']
current=(w/changed[0]).read_bytes();old=current.replace(b'    let p = SourceCategoryPolicy {\n        schema_version: 2,\n        ..Default::default()\n    };',b'    let mut p = SourceCategoryPolicy::default();\n    p.schema_version = 2;')
assert sha(old)==a[changed[0]]
metadata=[]
for n,h in b.items():
    if sha((w/n).read_bytes())!=h:
        assert n=='native/docs/graph-extensions.md';metadata.append(n)
checks=[r for r in r2['checks'] if r['name'] not in ['fmt','clippy','graph_analysis_policy']]+r3['checks']
assert all(c['exitCode']==0 and c['sourceUnchanged'] for c in checks)
totals={k:sum(c['totals'][k] for c in checks) for k in ['passed','failed','ignored']};assert totals=={'passed':55,'failed':0,'ignored':0}
def copy(src,relative):
    dest=out/relative;dest.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(src,dest);assert dest.read_bytes()==src.read_bytes()
for phase in ['guard-r1','guard-r2','guard-r3','legacy-before-r1','legacy-after-r1']:
    for f in sorted((raw/phase).iterdir()):
        if f.is_file():copy(f,Path(phase)/f.name)
for name in ['ci-r3-legacy-before.py','ci-r3-legacy-after.py']:
    copy(Path.home()/'AppData/Local/Temp'/name,Path('controllers')/name)
copy(Path(__file__),Path('archive-runner.py'))
log=(raw/'guard-r3/retained.log').read_text(encoding='utf-8');assert 'pass 260' in log and 'fail 0' in log and 'skipped 3' in log
summary={'baseCommit':'52f60c2097356f41f0573f99d47977b204cfa39f','nativeScopedTotals':totals,'selectedChecks':checks,
 'runtimeInputsUnchangedAfterValidation':True,'r2ToR3ChangedOnly':changed,
 'fixtureCorrection':'Equivalent initializer for invalid schemaVersion=2 fixture; exact previous bytes reconstructed.',
 'existingMetadataDocsChangedAfterValidation':metadata,'newMetadataDoc':'native/docs/source-category-policy.md',
 'legacyCliControl':after,'retainedPassed':260,'retainedSkipped':3,'fullStatusCertified':False,
 'remaining':'Other clustering/splitting/hub reattachment and upstream qualification; R1-R9 and Windows/provider gates remain.'}
(out/'checkpoint.json').write_text(json.dumps(summary,indent=2)+'\n')
index=[{'path':str(f.relative_to(out)).replace('\\','/'),'bytes':f.stat().st_size,'sha256':sha(f.read_bytes())} for f in sorted(out.rglob('*')) if f.is_file()]
(out/'index.json').write_text(json.dumps({'files':index,'copiedRawFilesByteExact':True},indent=2)+'\n')
print(json.dumps({'files':len(index)+1,'bytes':sum(r['bytes'] for r in index),'nativeScopedTotals':totals,'runtimeSourceExact':True}))
