from pathlib import Path
import subprocess,json,hashlib,shutil
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach')
raw=w.parent/'Yoke-CodeIntelligence/validation/integration-20261018/received-source-verification'
out=w/'docs/validation/integration-2026-10-19/received-source-verification'
assert not out.exists()
sha=lambda b:hashlib.sha256(b).hexdigest()
head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=w).decode().strip()
base='5d806c2b1b176206c7e2535379f5d264fdfc4176'
final=json.loads((raw/'final-r2/result.json').read_bytes())
retained=json.loads((raw/'retained-r1/result.json').read_bytes())
assert final['allChecksPassed'] and final['sourceUnchanged']
assert retained['allChecksPassed'] and retained['sourceUnchanged']
assert head!=base
out.mkdir(parents=True)
phases=[]
for name in ['feature-r1','feature-r2','feature-r3','final-r1','final-r2','retained-r1']:
    r=json.loads((raw/name/'result.json').read_bytes())
    shutil.copytree(raw/name,out/'runs'/name)
    phases.append({'phase':name,'resultPath':f'runs/{name}/result.json','allChecksPassed':r['allChecksPassed'],'inputs':r.get('inputs',r.get('frozenInputs'))})
shutil.copytree(raw/'tracker',out/'tracker')
shutil.copyfile(__file__,out/'archive-recipe.py')
prior=Path.home()/'AppData/Local/Temp/ci-pr-7-body-app-history-final.md'
if prior.is_file():shutil.copyfile(prior,out/'prior-pr-body.md')
manifest=json.loads((raw/'final-r2/source-before.json').read_bytes())
material=[];docs=[]
for name,expected in manifest.items():
    data=(w/name).read_bytes()
    if sha(data)!=expected:
        assert name in ['native/docs/digest-query.md','native/docs/digest-receiver.md'],name
        docs.append({'path':name,'testedSha256':expected,'currentSha256':sha(data),'change':'Documentation-only reconciliation of separate available verification/App-history contracts; runtime/test source unchanged.'})
    blob=subprocess.check_output(['git','show',head+':'+name],cwd=w)
    assert blob==data or blob.replace(b'\r\n',b'\n')==data.replace(b'\r\n',b'\n'),name
    material.append({'path':name,'checkedSha256':expected,'workingSha256':sha(data),'gitBlobSha256':sha(blob),'materialization':'raw_equal' if data==blob else 'crlf_only'})
pg='native/tests/fixtures/postgres-catalog-response.jsonl';b=(w/pg).read_bytes()
assert len(b)==2779 and sha(b)=='f67a05349a6cedf55222afd3bb3231c169d382dd4b078d76544daac517530b47'
assert subprocess.check_output(['git','show',head+':'+pg],cwd=w)==b
for name in subprocess.check_output(['git','diff','--name-only',base,head],cwd=w).decode().splitlines():
    p=w/name
    if p.is_file():
        dest=out/'final-source'/name;dest.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(p,dest)
def write(name,obj):
    (out/name).write_text(json.dumps(obj,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
write('source-materialization.json',{'sourceCommit':head,'inputs':len(manifest),'rawEqual':sum(x['materialization']=='raw_equal' for x in material),'crlfOnly':sum(x['materialization']=='crlf_only' for x in material),'documentationOnlyAfterChecks':docs,'runtimeAndTestSourceUnchangedAfterChecks':True,'protectedFixture':{'path':pg,'bytes':len(b),'sha256':sha(b)},'files':material})
readme=f'''# Independent received-declaration verification evidence

Source `{head}`, based on integration `{base}`.
The separate host-local library/CLI compares accepted Rust projection claims
with an explicitly selected independently replayed local capture. Current receiver
and source grants, exact receipt pins, native source/CAS/analyzer/Git replay and
finite work/output admission are mandatory. Foreign IDs never select local state.

The final five-target scope passes **142 Rust tests / 0 failed / 2 ignored**.
Format, build and all-target Clippy with warnings denied pass on the same
**761 unchanged native inputs**. Retained Python contracts, TypeScript typecheck
and TypeScript tests pass on **{retained['frozenInputs']} unchanged runtime inputs**;
their logs retain exact totals. This is not a whole-native or hosted Unix pass.

Twelve new default cases exercise real owned receiver intake, independent
workspaces, actual local Git objects and actual CLI processes. They cover
CRLF/Unicode, exact original row order, rehashed forged claims, changed bodies,
tampered native entities, forged capacity-error text, permissions/revocation,
stale receipts, optional whole-file hashes, missing Stores/no creation, same-Store
transaction sequencing, frozen Store-loss queries, deadlines and work/output limits.
No production service, account signing key or model was used.

Matching a reported full-declaration fingerprint is distinguished from a direct
comparison of unavailable sender raw bytes. Optional whole-file hashes are never
inferred; a private declaration outside the advertised projection can differ.
Matched local Git IDs do not attest a remote repository, sender snapshot identity
or current remote head. Sender source-digest/CAS identity, discussions, history
claims, semantic reasoning, whole-source equivalence and production deployment
remain open. Empty claim sets refuse instead of returning a complete source result.

The earlier test-only HostProfile serialization compilation failure is preserved
with zero executed groups; the fixture now supplies existing closed profile JSON,
without adding a profile serialization capability. Both successful intermediate
source variants and the earlier overly strong output-label variant are retained.
Final output uses `reportedDeclarationFingerprintMatched` and explicitly rejects
the old label in its tests.

All phase manifests are raw before/after hashes. Frozen source copies preserve
the changed variants captured by each runner. Final changed files are archived
separately. Two existing contract documents were reconciled after checks, with
no runtime/test changes; `source-materialization.json` records exact differences
and raw/CRLF-only Git materialization for all native inputs. The protected
PostgreSQL fixture remains byte-exact. Tracker proof changes only three R8
development fields; inventory, other rows and certification states remain exact.

`index.json` hashes every artifact except itself. Prior PR metadata is historical;
post-push remote verification is retained separately in the ignored local receipt.
Recipes refer to completed phase directories and must not be blindly rerun.
No release, main merge or full-platform certification is claimed.
'''
(out/'README.md').write_text(readme,encoding='utf-8')
files=[]
for p in sorted(out.rglob('*')):
    if p.is_file():files.append({'path':p.relative_to(out).as_posix(),'bytes':p.stat().st_size,'sha256':sha(p.read_bytes())})
write('index.json',{'schemaVersion':1,'kind':'received-rust-source-verification','sourceCommit':head,'baseIntegration':base,'scope':{'nativeGroups':5,'nativePassed':142,'nativeFailed':0,'nativeIgnored':2,'nativeInputs':761,'retainedInputs':retained['frozenInputs'],'wholeNativeRerun':False,'configuredProductionAcceptance':False,'semanticReasoningAvailable':False},'phases':phases,'files':files})
print(json.dumps({'sourceCommit':head,'artifacts':len(files),'indexSha256':sha((out/'index.json').read_bytes()),'documentationChangesAfterChecks':len(docs)}))
