from pathlib import Path
import json,hashlib,subprocess,shutil,re
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach')
raw=w.parent/'Yoke-CodeIntelligence/validation/integration-20261018/semantic-identity'
out=w/'docs/validation/integration-2026-10-19/semantic-identity'
assert not out.exists()
sha=lambda b:hashlib.sha256(b).hexdigest()
head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=w).decode().strip()
base='7a61aa782462614843fcd879dfffa31e9dacd382';assert head!=base
final=json.loads((raw/'final-r1/result.json').read_bytes());retained=json.loads((raw/'retained-r1/result.json').read_bytes());qual=json.loads((raw/'qualification.json').read_bytes())
assert final['sourceUnchanged'] and retained['sourceUnchanged'] and retained['allChecksPassed']
assert not final['allChecksPassed'] and qual['newCasesPassed']==22 and not qual['broaderNativeScopePassed']
assert final['commands'][0]['totals']=={'passed':574,'failed':1,'ignored':7}
assert all(r['exitCode']==0 for r in final['commands'] if r['name']!='native')
native=json.loads((raw/'final-r1/source-before.json').read_bytes());retained_inputs=json.loads((raw/'retained-r1/source-before.json').read_bytes())
for phase in ['final-r1','retained-r1']:
    for name,h in json.loads((raw/phase/'source-before.json').read_bytes()).items():assert sha((w/name).read_bytes())==h,(phase,name)
assert 'Ran 17 tests' in (raw/'retained-r1/python-contracts.log').read_text(encoding='utf-8')
log=(raw/'retained-r1/tests.log').read_text(encoding='utf-8');assert all(re.search(r'\b'+label+r' '+str(count)+r'\s*$',log,re.M) for label,count in [('pass',260),('skipped',3),('fail',0)])
out.mkdir(parents=True)
phases=[]
for name in ['compile-r1','feature-r1','feature-r2','feature-r3','feature-r4','feature-r5','feature-r6','final-r1','retained-r1']:
    result=json.loads((raw/name/'result.json').read_bytes());shutil.copytree(raw/name,out/'runs'/name)
    phases.append({'phase':name,'resultPath':f'runs/{name}/result.json','allChecksPassed':result['allChecksPassed'],'inputs':result.get('inputs',result.get('frozenInputs'))})
shutil.copytree(raw/'tracker',out/'tracker')
for name in ['qualification.json','host-privileges.txt','host-prerequisites.json','tool-runtimes.json','minhash-oracle-result.json']:shutil.copyfile(raw/name,out/name)
shutil.copyfile(__file__,out/'archive-recipe.py')
shutil.copyfile(Path.home()/'AppData/Local/Temp/ci-semantic-identity-minhash-oracle.py',out/'minhash-oracle.py')
shutil.copyfile(Path.home()/'AppData/Local/Temp/ci-semantic-identity-qualification.py',out/'qualification-recipe.py')
shutil.copyfile(Path.home()/'AppData/Local/Temp/ci-semantic-identity-tracker.py',out/'tracker-recipe.py')
shutil.copyfile(Path.home()/'AppData/Local/Temp/ci-pr-7-body-semantic-transcript-final.md',out/'prior-pr-body.md')
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
(out/'README.md').write_text(f'''# Source-bound semantic identity evidence

Source `{head}`, based on integration `{base}`.
The independent native identity policy consumes real joint source/receipt-checked
Meaning graphs, computes bounded normalization/MinHash64/LSH16x4/Jaro candidates,
and admits one explicit host-provider adjudication job. Complete decisions require
actual original concept-support IDs. Only fully positive cliques group nodes;
unknown, negative, absent or inconsistent intra-component decisions refuse.
The quotient retains every original claim, parallel/reverse/self observation,
ordered repeated role slot, source evidence and original receipt. No parent record
is changed. Private frozen node/degree/directed-trace queries start no provider.

All **22 new cases pass**: 18 integration and four unit cases. Owned Node24.21
processes provide actual native extraction/adjudication, CLI success/lost stdout
and provider-free recovery after config/worker removal. Tests cover independent
SHA-256 MinHash vectors and band/score values, Unicode/alias/short-name boundaries,
all selected candidates, complete positive cliques, negative/uncertain outcomes,
non-clique refusal, exact support/coverage, raw decision order, full collapse with
all original roles, independent degree counts and directed traces, Store-loss
exports/queries, trust/source/all capacities, source/payload/reply/rehashed-result/
ledger corruption, original unknown billing after both identity and parent-ledger
reconciliation, bound provider errors/invalid usage/nonzero exits/overrun, incomplete/
unbound replies, cancellation, SQL registration/send/publication failure and recovery.
The allocation observer is thread-local: a linked 16-MB SQL cell refuses before a
Rust allocation of 1 MB, and SQL limits are restored. Model-rationale hashes do not
retain unrelated history; typed new job/artifact roots alone are followed.

The final broader ten-target run **failed: 574 passed / 1 failed / 7 ignored**.
Its 473-pass/seven-ignored library group includes the four new unit cases. The
unchanged `windows_host_aliases_cannot_read_protected_files_or_alternate_streams`
fixture fails at `native/tests/feedback_cli.rs:1100:59`, `symlink_file` setup,
OS1314, before application reads. Fresh token/registry inspection confirms no
SeCreateSymbolicLinkPrivilege and unconfigured Developer Mode. The original
security test and host-file reader are unchanged. No assertion, fixture,
permission guard or host privilege was weakened, skipped or changed.
This remains an open Windows/security-alias and whole-native acceptance gate.

Format, build and all-target Clippy with warnings denied pass on
**{final['inputs']} fixed native inputs**. Python17, typecheck and
TypeScript260/3skipped pass on **{retained['frozenInputs']} fixed retained inputs**.
Every final input still matches the source commit. Raw/CRLF-only materialization
is recorded explicitly. Existing alias normalization/Jaro bodies are unchanged;
all thirteen legacy join CLI cases pass, including independent hash/output/limit
oracles. The internal shared query adapter preserves the released join schema and
4-MB limit while exposing a separate checked identity-query kind/64-MB ceiling.

The initial core compiled. First integration source recorded 12 passed / two
failed: one assertion used `quote` rather than the real evidence `text` field;
the unrelated-history fixture changed only latest while leaving the actual
Freshness baseline on the new capture. Correct field access and actual recapture
repair the fixtures without relaxing a guard. The second source passed 488/0/7
across library and 15 integration cases. Two later test-only compile failures
(a borrowed temporary deadline and a shadowed helper name) executed no tests and
remain archived. Intermediate 17-case and final-query 31-case scopes passed.
Final source also isolates the allocation observer per thread. All variants and
their unchanged-input manifests remain visible; no failed run is relabelled.

`tracker/proof.json` changes only four R2 development fields. Original inventory,
other priority rows and all certification states remain exact. The protected
PostgreSQL fixture stays byte-exact. Node/Rust/Python executable/version records
and the independent Python MinHash oracle are included. No configured live
semantic model was used: owned protocol processes qualify processing, not actual
semantic accuracy, external billing or provider filesystem isolation. Original
claims and model interpretations remain unverified; source-write claims apply
only to the native parent. Mixed/history/workspace identity, wider upstream rules,
independent corroboration/execution, full STATUS/R1–R9 and platform/upstream
acceptance remain open.

`index.json` hashes every artifact except itself. Prior PR metadata is historical;
post-push remote verification is retained separately in the ignored local receipt.
Recipes reference completed phases and must not be blindly rerun. No main merge,
release or separate publisher action is claimed.
''',encoding='utf-8')
files=[{'path':p.relative_to(out).as_posix(),'bytes':p.stat().st_size,'sha256':sha(p.read_bytes())} for p in sorted(out.rglob('*')) if p.is_file()]
write('index.json',{'schemaVersion':1,'kind':'native-semantic-identity','sourceCommit':head,'baseIntegration':base,'scope':{'nativeGroups':10,'nativePassed':574,'nativeFailed':1,'nativeIgnored':7,'nativeInputs':final['inputs'],'retainedInputs':retained['frozenInputs'],'newCasesPassed':22,'newIntegrationCasesPassed':18,'newUnitCasesPassed':4,'newCasesFailed':0,'broaderNativeScopePassed':False,'failedPrerequisiteTest':qual['failedTest'],'formatBuildClippyPassed':True,'allRetainedChecksPassed':True,'wholeNativeRerun':False,'fullContractCertified':False,'liveModelQualityQualified':False},'finalPhase':'final-r1','retainedPhase':'retained-r1','phases':phases,'files':files})
print(json.dumps({'sourceCommit':head,'artifacts':len(files),'indexSha256':sha((out/'index.json').read_bytes()),'broaderNativeScopePassed':False}))
