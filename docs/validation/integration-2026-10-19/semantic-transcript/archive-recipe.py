from pathlib import Path
import json,hashlib,subprocess,shutil,re
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach')
raw=w.parent/'Yoke-CodeIntelligence/validation/integration-20261018/semantic-transcript'
out=w/'docs/validation/integration-2026-10-19/semantic-transcript'
assert not out.exists()
sha=lambda b:hashlib.sha256(b).hexdigest()
head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=w).decode().strip()
base='dae57012519f013103541c1ee0ea21bb897a6409';assert head!=base
final=json.loads((raw/'final-r1/result.json').read_bytes());retained=json.loads((raw/'retained-r1/result.json').read_bytes());qual=json.loads((raw/'qualification.json').read_bytes())
assert final['sourceUnchanged'] and retained['sourceUnchanged'] and retained['allChecksPassed']
assert not final['allChecksPassed'] and qual['newCasesPassed']==15 and not qual['broaderNativeScopePassed']
assert final['commands'][0]['totals']=={'passed':93,'failed':1,'ignored':0}
assert all(r['exitCode']==0 for r in final['commands'] if r['name']!='native')
native=json.loads((raw/'final-r1/source-before.json').read_bytes());retained_inputs=json.loads((raw/'retained-r1/source-before.json').read_bytes())
for phase in ['final-r1','retained-r1']:
    for name,h in json.loads((raw/phase/'source-before.json').read_bytes()).items():assert sha((w/name).read_bytes())==h,(phase,name)
assert 'Ran 17 tests' in (raw/'retained-r1/python-contracts.log').read_text(encoding='utf-8')
log=(raw/'retained-r1/tests.log').read_text(encoding='utf-8');assert all(re.search(r'\b'+label+r' '+str(count)+r'\s*$',log,re.M) for label,count in [('pass',260),('skipped',3),('fail',0)])
out.mkdir(parents=True)
phases=[]
for name in ['feature-r1','feature-r2','final-r1','retained-r1']:
    result=json.loads((raw/name/'result.json').read_bytes());shutil.copytree(raw/name,out/'runs'/name)
    phases.append({'phase':name,'resultPath':f'runs/{name}/result.json','allChecksPassed':result['allChecksPassed'],'inputs':result.get('inputs',result.get('frozenInputs'))})
shutil.copytree(raw/'tracker',out/'tracker')
for name in ['qualification.json','host-privileges.txt','host-prerequisites.json']:shutil.copyfile(raw/name,out/name)
shutil.copyfile(__file__,out/'archive-recipe.py')
shutil.copyfile(Path.home()/'AppData/Local/Temp/ci-semantic-transcript-qualification.py',out/'qualification-recipe.py')
shutil.copyfile(Path.home()/'AppData/Local/Temp/ci-semantic-transcript-tracker.py',out/'tracker-recipe.py')
shutil.copyfile(Path.home()/'AppData/Local/Temp/ci-pr-7-body-meaning-feedback-final.md',out/'prior-pr-body.md')
material=[]
for name in sorted(set(native)|set(retained_inputs)):
    data=(w/name).read_bytes();blob=subprocess.check_output(['git','show',head+':'+name],cwd=w)
    assert data==blob or data.replace(b'\r\n',b'\n')==blob.replace(b'\r\n',b'\n'),name
    material.append({'path':name,'checkedSha256':sha(data),'gitBlobSha256':sha(blob),'materialization':'raw_equal' if data==blob else 'crlf_only'})
pg='native/tests/fixtures/postgres-catalog-response.jsonl';b=(w/pg).read_bytes()
assert len(b)==2779 and sha(b)=='f67a05349a6cedf55222afd3bb3231c169d382dd4b078d76544daac517530b47'
assert subprocess.check_output(['git','show',head+':'+pg],cwd=w)==b
for name in subprocess.check_output(['git','diff','--name-only',base,head],cwd=w).decode().splitlines():
    p=w/name
    if p.is_file():d=out/'final-source'/name;d.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(p,d)
def write(name,value):(out/name).write_text(json.dumps(value,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
write('source-materialization.json',{'sourceCommit':head,'nativeInputs':len(native),'retainedInputs':len(retained_inputs),'unionInputs':len(material),'allNativeAndRetainedInputsStillMatch':True,'rawEqual':sum(x['materialization']=='raw_equal' for x in material),'crlfOnly':sum(x['materialization']=='crlf_only' for x in material),'protectedFixture':{'path':pg,'bytes':len(b),'sha256':sha(b)},'files':material})
(out/'README.md').write_text(f'''# Explicit semantic transcript candidate evidence

Source `{head}`, based on integration `{base}`.
The separate `transcriptUtf8` protocol admits one explicit host-provider attempt
over selected original imported bytes. It retains exact source binding, full
selected context, immutable payload/reply/result artifacts and original billing.
Provider-free status/load/jobs and private checked exports grant no event, native
code identity, semantic-truth or execution authority. Duplicate observations and
ordered repeated quotes remain intact. Typed retention validates all job states.

All **15 new cases pass**, including actual CLI permission ordering, successful
publication followed by lost stdout and provider-free recovery after config/worker
removal. Further cases cover exact Unicode/CRLF/context, parallel/opposing
candidates, explicit no-candidates coverage, source/quote/header/payload/response/
rehashed-result/ledger/artifact tampering, whole-result capacity refusals, SQL
registration rollback, never-sent cancellation, interrupted/truncated/oversized/
duplicate/unbound responses, publication failure, original unknown costs after
later ledger reconciliation, immutable export after Store loss, independent new
attempts and fully checked job-header pagination. Bound bills survive invalid
domain/usage, provider errors, nonzero exits and overruns; unknown sends keep
reservations and no read can retry a send.

The final broader nine-target run **failed: 93 passed / 1 failed / 0 ignored**.
The unchanged `windows_host_aliases_cannot_read_protected_files_or_alternate_streams`
fixture fails at `native/tests/feedback_cli.rs:1100:59`, `symlink_file` setup,
OS1314, before application reads. Current token inspection confirms no
SeCreateSymbolicLinkPrivilege and unconfigured Developer Mode. The original test
file and original host-file reader function remain unchanged. No fixture,
assertion, permission guard or host privilege was weakened, skipped or changed.
This remains open local Windows/security-alias and whole-native acceptance.

Format, build and all-target Clippy with warnings denied pass on
**{final['inputs']} fixed native inputs**. Python17, typecheck and
TypeScript260/3skipped pass on **{retained['frozenInputs']} fixed retained inputs**.
All final inputs still match committed source; materialization distinguishes raw
equality from checkout CRLF only. This is not current-head hosted acceptance.

The first feature variant recorded 12 passed / 1 failed: its deliberately created
SQL failure trigger remained present during subsequent retention. Retention
correctly refused the unknown database schema. The test now removes that owned
trigger after proving publication failure, before checking normal retention.
Two compiler warnings were removed. Later source closes a post-reservation error
window, preserves a job ID through Store cleanup failures, and records bound money
before the independent usage write. The 15-case second variant passed; final
source further strengthens actual CLI lost-stdout recovery. All variants remain
archived; no guard was weakened to obtain a pass.

`tracker/proof.json` changes only three R9 development fields. Original inventory,
other rows, the Meaning-outcome checkpoint and every certification state remain
exact. The protected PostgreSQL response fixture remains byte-exact. Owned Node
processes qualify protocol/accounting/replay, not actual model quality, external
billing verification or provider filesystem isolation. Source-write claims apply
to the native parent only; an explicitly trusted provider has host-account access.
Further object surfaces, independent corroboration, observed execution, complete
STATUS/R1–R9 and upstream/platform acceptance remain open.

`index.json` hashes every artifact except itself. Prior PR metadata is historical;
post-push remote verification is stored separately in the ignored local receipt.
Recipes reference completed phases and must not be blindly rerun. No main merge,
release or separate publisher action is claimed.
''',encoding='utf-8')
files=[{'path':p.relative_to(out).as_posix(),'bytes':p.stat().st_size,'sha256':sha(p.read_bytes())} for p in sorted(out.rglob('*')) if p.is_file()]
write('index.json',{'schemaVersion':1,'kind':'native-semantic-transcript-candidates','sourceCommit':head,'baseIntegration':base,'scope':{'nativeGroups':9,'nativePassed':93,'nativeFailed':1,'nativeIgnored':0,'nativeInputs':final['inputs'],'retainedInputs':retained['frozenInputs'],'newCasesPassed':15,'newCasesFailed':0,'broaderNativeScopePassed':False,'failedPrerequisiteTest':qual['failedTest'],'formatBuildClippyPassed':True,'allRetainedChecksPassed':True,'wholeNativeRerun':False,'fullContractCertified':False,'liveModelQualityQualified':False},'finalPhase':'final-r1','retainedPhase':'retained-r1','phases':phases,'files':files})
print(json.dumps({'sourceCommit':head,'artifacts':len(files),'indexSha256':sha((out/'index.json').read_bytes()),'broaderNativeScopePassed':False}))
