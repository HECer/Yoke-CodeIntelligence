from pathlib import Path
import json,hashlib,shutil,subprocess
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence')
work=root.parent/'Yoke-CI-r1-attach'
raw=root/'validation/integration-20261018/platform-d819'
out=work/'docs/validation/integration-2026-10-19/host-install-platform'
assert not out.exists()
out.mkdir(parents=True)
files=['ubuntu-job.log','hosted-summary.json','original-test-bodies.json','prior-pr.json']
for phase in ['final-r1','library-r1','retained-checks']:
 result=json.loads((raw/phase/'result.json').read_text())
 assert result['sourceUnchanged'] and result['allChecksPassed'],phase
 files.extend(str(p.relative_to(raw)).replace('\\','/') for p in (raw/phase).rglob('*') if p.is_file())
for name in files:
 target=out/name; target.parent.mkdir(parents=True,exist_ok=True)
 shutil.copyfile(raw/name,target)
for name in ['ci-d819-check.py','ci-d819-integrated.py','ci-d819-proof.py','ci-d819-archive.py']:
 shutil.copyfile(Path(__file__).parent/name,out/name)
summary=json.loads((raw/'hosted-summary.json').read_text())
assert summary['totals']=={'passed':2381,'failed':36,'ignored':36}
assert len(summary['failedTests'])==36
new=json.loads((raw/'final-r1/result.json').read_text())
lib=json.loads((raw/'library-r1/result.json').read_text())
assert new['commands'][0]['totals']=={'passed':20,'failed':0,'ignored':0}
assert lib['commands'][0]['totals']['passed']>0 and lib['commands'][0]['totals']['failed']==0
frozen=json.loads((raw/'final-r1/source-before.json').read_text())
for name,digest in frozen.items():
 assert hashlib.sha256((work/name).read_bytes()).hexdigest()==digest,name
records=[]
for p in sorted(out.rglob('*')):
 if p.is_file():
  b=p.read_bytes();records.append({'path':str(p.relative_to(out)).replace('\\','/'),'bytes':len(b),'sha256':hashlib.sha256(b).hexdigest()})
index={'schemaVersion':1,'baseline':'d819d7100b06f239fae291083908e17e18d3ae35','kind':'platform-test-scope-correction','artifacts':records,'scope':'Windows-only success/recovery fixtures retained unchanged; Unix refusal API/CLI tests added. Windows local checks only. Hosted Unix execution pending. Cargo interleaving prevents associating summaries with neighboring Running lines.'}
(out/'index.json').write_text(json.dumps(index,indent=2)+'\n',encoding='utf-8')
print(json.dumps({'artifacts':len(records),'indexSha256':hashlib.sha256((out/'index.json').read_bytes()).hexdigest(),'library':lib['commands'][0]['totals']}))
