from pathlib import Path
import json,hashlib,subprocess,shutil
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach')
raw=w.parent/'Yoke-CodeIntelligence/validation/integration-20261018/meaning-feedback'
out=w/'docs/validation/integration-2026-10-19/meaning-feedback'
assert not out.exists()
sha=lambda b:hashlib.sha256(b).hexdigest()
head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=w).decode().strip()
base='92b090465dd70020155bd6b1be4951878849b0ca';assert head!=base
final=json.loads((raw/'final-r1/result.json').read_bytes());retained=json.loads((raw/'retained-r1/result.json').read_bytes());qual=json.loads((raw/'qualification.json').read_bytes())
assert final['sourceUnchanged'] and retained['sourceUnchanged'] and retained['allChecksPassed']
assert not final['allChecksPassed'] and qual['newCasesPassed']==13 and not qual['broaderNativeScopePassed']
assert final['commands'][0]['totals']=={'passed':74,'failed':1,'ignored':0}
assert all(r['exitCode']==0 for r in final['commands'] if r['name']!='native')
for phase in ['final-r1','retained-r1']:
    for name,h in json.loads((raw/phase/'source-before.json').read_bytes()).items():assert sha((w/name).read_bytes())==h,(phase,name)
out.mkdir(parents=True)
phases=[]
for name in ['feature-r1','final-r1','retained-r1']:
    result=json.loads((raw/name/'result.json').read_bytes());shutil.copytree(raw/name,out/'runs'/name)
    phases.append({'phase':name,'resultPath':f'runs/{name}/result.json','allChecksPassed':result['allChecksPassed'],'inputs':result.get('inputs',result.get('frozenInputs'))})
shutil.copytree(raw/'tracker',out/'tracker')
for name in ['qualification.json','host-privileges.txt','host-prerequisites.json']:shutil.copyfile(raw/name,out/name)
shutil.copyfile(__file__,out/'archive-recipe.py')
shutil.copyfile(Path.home()/'AppData/Local/Temp/ci-meaning-feedback-qualification.py',out/'qualification-recipe.py')
shutil.copyfile(Path.home()/'AppData/Local/Temp/ci-pr-7-body-interpretation-final.md',out/'prior-pr-body.md')
material=[]
for name,h in json.loads((raw/'final-r1/source-before.json').read_bytes()).items():
    data=(w/name).read_bytes();blob=subprocess.check_output(['git','show',head+':'+name],cwd=w)
    assert data==blob or data.replace(b'\r\n',b'\n')==blob.replace(b'\r\n',b'\n'),name
    material.append({'path':name,'checkedSha256':h,'gitBlobSha256':sha(blob),'materialization':'raw_equal' if data==blob else 'crlf_only'})
pg='native/tests/fixtures/postgres-catalog-response.jsonl';b=(w/pg).read_bytes()
assert len(b)==2779 and sha(b)=='f67a05349a6cedf55222afd3bb3231c169d382dd4b078d76544daac517530b47'
assert subprocess.check_output(['git','show',head+':'+pg],cwd=w)==b
for name in subprocess.check_output(['git','diff','--name-only',base,head],cwd=w).decode().splitlines():
    p=w/name
    if p.is_file():d=out/'final-source'/name;d.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(p,d)
def write(name,value):(out/name).write_text(json.dumps(value,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
write('source-materialization.json',{'sourceCommit':head,'nativeInputs':len(material),'allNativeAndRetainedInputsStillMatch':True,'rawEqual':sum(x['materialization']=='raw_equal' for x in material),'crlfOnly':sum(x['materialization']=='crlf_only' for x in material),'protectedFixture':{'path':pg,'bytes':len(b),'sha256':sha(b)},'files':material})
(out/'README.md').write_text(f'''# Native Meaning outcome evidence

Source `{head}`, based on integration `{base}`.
The separate immutable feedback contract supports packet-qualified native Meaning
concepts, directed relations and hyperedges with complete original/evaluated
View/source/overlay/Meaning receipt replay. Original quotes lie in actual selected
claim support; evaluated quotes establish exact bytes in the same support path
only. Subject-wide actor/session sequences span evaluated Views while every
event retains its original pins. Whole-output admission precedes SQL insertion;
retention replays original event roots. No provider, source capture/edit, automatic
promotion, semantic truth or execution authority is added.

All **13 new cases pass**. The final broader five-target native run is
**74 passed / 1 failed / 0 ignored**, so **the broader run failed**. The unchanged
`windows_host_aliases_cannot_read_protected_files_or_alternate_streams` fixture
fails at `native/tests/feedback_cli.rs:1100:59`, `symlink_file` setup with OS1314,
before an application alias read. Host token inspection shows no
SeCreateSymbolicLinkPrivilege, and Developer Mode is not configured. The original
test file and original host-file reader function are unchanged; hashes and exact
comparison are in `qualification.json`. No fixture, permission guard or assertion
was weakened, no gate was skipped and no privilege/system mutation was made.
Full local Windows/security-alias and whole-native acceptance remain open.

Format, build and all-target Clippy with warnings denied pass on
**{final['inputs']} unchanged native inputs**. Python17, typecheck and
TypeScript260/3skipped pass on **{retained['frozenInputs']} unchanged retained
inputs**. Every final input still matches source. This does not replace the
failed broader native result or establish current-head hosted qualification.

Owned explicit Node protocol builds provide original/rebound Meaning receipts;
the worker is removed before feedback operations. Tests retain exact Unicode/CRLF
bytes, complete packets, parallel/reverse relations, roles, unknown original
costs, idempotence after lost stdout, typed references, evaluated source changes,
sequence/time conflicts, future/latest votes, independently expected integer decay,
all resource limits, source tampering, closed/duplicate inputs, SQL rollback,
read transaction/SQLite-limit preservation, actual CLI permission ordering and
retention refusal on corrupt/missing roots. No model quality or remote billing
is qualified by these protocol fixtures.

The initial12-case source passed. Final source adds independent relation-target
scores and strengthens the consistent-forgery case: immutable IDs are recomputed
from the typed draft, preserving the released identity serialization order. The
bad source hash refuses and repair-only replay succeeds. Earlier variants remain
archived; an earlier Value-based hash assertion alone was weaker evidence.

`tracker/proof.json` changes only three R9 development fields; all inventory,
other rows and certification states remain exact. Source materialization records
raw/CRLF-only Git blobs; the protected PostgreSQL fixture stays byte-exact.
Original response/provider truth, independently authenticated corroboration,
current applicability, observed execution, further targets, semantic transcript
candidates, full STATUS/R1–R9 and platform/upstream acceptance remain open.

`index.json` hashes every artifact except itself. Prior PR metadata is historical;
post-push remote verification is retained separately in the ignored local receipt.
Recipes reference completed phases and must not be blindly rerun. No main merge,
release or separate publisher action is claimed.
''',encoding='utf-8')
files=[{'path':p.relative_to(out).as_posix(),'bytes':p.stat().st_size,'sha256':sha(p.read_bytes())} for p in sorted(out.rglob('*')) if p.is_file()]
write('index.json',{'schemaVersion':1,'kind':'native-meaning-outcome-feedback','sourceCommit':head,'baseIntegration':base,'scope':{'nativeGroups':5,'nativePassed':74,'nativeFailed':1,'nativeIgnored':0,'nativeInputs':final['inputs'],'retainedInputs':retained['frozenInputs'],'newCasesPassed':13,'newCasesFailed':0,'broaderNativeScopePassed':False,'failedPrerequisiteTest':qual['failedTest'],'formatBuildClippyPassed':True,'allRetainedChecksPassed':True,'wholeNativeRerun':False,'fullContractCertified':False},'finalPhase':'final-r1','retainedPhase':'retained-r1','phases':phases,'files':files})
print(json.dumps({'sourceCommit':head,'artifacts':len(files),'indexSha256':sha((out/'index.json').read_bytes()),'broaderNativeScopePassed':False}))
