from pathlib import Path
import subprocess,json,hashlib,shutil
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach')
raw=w.parent/'Yoke-CodeIntelligence/validation/integration-20261018/received-source-interpretation'
out=w/'docs/validation/integration-2026-10-19/received-source-interpretation'
assert not out.exists()
sha=lambda b:hashlib.sha256(b).hexdigest()
head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=w).decode().strip()
base='4456fab78d4f42fea843ff2659313288bf396bef'
assert head!=base
phases=sorted(p.name for p in raw.iterdir() if p.is_dir() and (p/'result.json').is_file())
finals=[n for n in phases if n.startswith('final')]
retaineds=[n for n in phases if n.startswith('retained')]
final_name=finals[-1];retained_name=retaineds[-1]
final=json.loads((raw/final_name/'result.json').read_bytes());retained=json.loads((raw/retained_name/'result.json').read_bytes())
assert final['allChecksPassed'] and final['sourceUnchanged'] and retained['allChecksPassed'] and retained['sourceUnchanged']
native=next(r for r in final['commands'] if r['name']=='native')
assert native['groups']==5 and native['totals']['failed']==0
for phase in [final_name,retained_name]:
    for name,h in json.loads((raw/phase/'source-before.json').read_bytes()).items():assert sha((w/name).read_bytes())==h,(phase,name)
out.mkdir(parents=True)
records=[]
for name in phases:
    r=json.loads((raw/name/'result.json').read_bytes());shutil.copytree(raw/name,out/'runs'/name)
    records.append({'phase':name,'resultPath':f'runs/{name}/result.json','allChecksPassed':r['allChecksPassed'],'inputs':r.get('inputs',r.get('frozenInputs'))})
shutil.copytree(raw/'tracker',out/'tracker');shutil.copyfile(__file__,out/'archive-recipe.py')
prior=Path.home()/'AppData/Local/Temp/ci-pr-7-body-received-final.md'
shutil.copyfile(prior,out/'prior-pr-body.md')
manifest=json.loads((raw/final_name/'source-before.json').read_bytes());material=[]
for name,h in manifest.items():
    data=(w/name).read_bytes();blob=subprocess.check_output(['git','show',head+':'+name],cwd=w)
    assert blob==data or blob.replace(b'\r\n',b'\n')==data.replace(b'\r\n',b'\n'),name
    material.append({'path':name,'checkedSha256':h,'gitBlobSha256':sha(blob),'materialization':'raw_equal' if blob==data else 'crlf_only'})
pg='native/tests/fixtures/postgres-catalog-response.jsonl';b=(w/pg).read_bytes()
assert len(b)==2779 and sha(b)=='f67a05349a6cedf55222afd3bb3231c169d382dd4b078d76544daac517530b47'
assert subprocess.check_output(['git','show',head+':'+pg],cwd=w)==b
for name in subprocess.check_output(['git','diff','--name-only',base,head],cwd=w).decode().splitlines():
    p=w/name
    if p.is_file():
        d=out/'final-source'/name;d.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(p,d)
def write(name,value):(out/name).write_text(json.dumps(value,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
write('source-materialization.json',{'sourceCommit':head,'nativeInputs':len(manifest),'allNativeAndRetainedInputsStillMatch':True,'rawEqual':sum(x['materialization']=='raw_equal' for x in material),'crlfOnly':sum(x['materialization']=='crlf_only' for x in material),'protectedFixture':{'path':pg,'bytes':len(b),'sha256':sha(b)},'files':material})
total=native['totals']
(out/'README.md').write_text(f'''# Source-bound receiver interpretation evidence

Source `{head}`, based on integration `{base}`.
The separate host-local interpretation API/CLI binds existing explicitly built
native Meaning inferences to fresh received-declaration verification. Every complete
submitted unit, including context and no-claims units, must fit verified local
path/hash/declaration bounds. All containing declaration rows remain distinct.
The entire independently source/receipt/cache/accounting-replayed Meaning layer
retains its namespaces, quotations, directed parallel/reverse/self relations,
ordered roles, coverage and original receipt/usage/cost provenance. It adds no
provider dispatch, model/network request, project execution or derived record.

Final selected native scope: **{total['passed']} passed / {total['failed']} failed /
{total['ignored']} ignored**, five groups. Format, build and all-target Clippy with
warnings denied pass on **{final['inputs']} unchanged native inputs**. Python17,
typecheck and TypeScript260/3skipped pass on **{retained['frozenInputs']} unchanged
retained inputs**. Every final input still matches current source bytes. This is
scoped Windows/owned-process evidence, not a whole-native or hosted Unix pass.

Thirteen new cases use real owned receiver intake, explicit owned Node protocol
provider builds, local Store/Git lineage and actual CLI processes. Receiver reads
work after removing the provider script and do not add attempts or records.
Cases cover private/oversized submitted units despite smaller quotes/no-claims,
unknown/duplicate protocol fields, permission ordering/current grants, tampering,
exact Unicode/CRLF quotes, original parallel directions and roles, overlapping
module/function correspondence, multiple graphs, explicit zero-call cache rebound,
Store loss, deadlines and independent input/mapping/match/output limits.

The initial owned fixture supplied an exactly duplicate role/concept pair; the
existing Meaning contract correctly refused it after accounting. Seven tests
failed during fixture construction; 39 passed and one was ignored. The corrected
fixture uses repeated concepts/roles with distinct pairs. No source or protocol
guard was weakened. Earlier source variants, including quotation-presence and
Store-loss-test refinements, remain archived with their actual results. The first
expanded run passed all 114 behavior tests but Clippy refused a cloned test
selection; the final test uses a borrowed slice without weakening any assertion.
Frozen
export checks the deadline after cloning, and actual no-claims output reports
`sourceQuotesPresent:false` rather than inventing quotations.

Model interpretations remain unverified inferences. Correspondence grants no
semantic identity, compiler binding, observed execution, remote ownership, sender
snapshot/CAS identity, source-digest verification or whole-source equivalence.
Discussions/history claims, broader receiver reasoning/model quality, configured
production deployment and full STATUS/R1–R9 acceptance remain open. No model
quality or remote billing qualification was performed by these protocol fixtures.

Raw manifests and captured source variants accompany every phase. Final changed
files are separately archived; source-materialization records raw/CRLF-only Git
blobs for the entire native scope. The protected PostgreSQL fixture is byte-exact.
Tracker proof changes only three R8 development fields, leaving all inventory,
other rows and certification states unchanged. `index.json` hashes every artifact
except itself. Prior PR metadata is historical; post-push remote verification is
retained separately in the ignored local receipt. Recipes refer to completed
phases and must not be blindly rerun. No release or main merge is claimed.
''',encoding='utf-8')
files=[{'path':p.relative_to(out).as_posix(),'bytes':p.stat().st_size,'sha256':sha(p.read_bytes())} for p in sorted(out.rglob('*')) if p.is_file()]
write('index.json',{'schemaVersion':1,'kind':'received-source-bound-interpretation','sourceCommit':head,'baseIntegration':base,'scope':{'nativeGroups':5,'nativePassed':total['passed'],'nativeFailed':total['failed'],'nativeIgnored':total['ignored'],'nativeInputs':final['inputs'],'retainedInputs':retained['frozenInputs'],'newCases':13,'wholeNativeRerun':False,'semanticQualityVerified':False,'configuredProductionAcceptance':False},'finalPhase':final_name,'retainedPhase':retained_name,'phases':records,'files':files})
print(json.dumps({'sourceCommit':head,'artifacts':len(files),'indexSha256':sha((out/'index.json').read_bytes()),'nativeTotals':total}))
