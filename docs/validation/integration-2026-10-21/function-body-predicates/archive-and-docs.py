from pathlib import Path
import hashlib,json,re,shutil,subprocess,zipfile
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');raw=w/'validation/integration-20261018/r1-predicate-contexts'
read=lambda p:json.loads(p.read_text(encoding='utf-8-sig'));sha=lambda b:hashlib.sha256(b).hexdigest()
n=read(raw/'green-contexts-r4/result.json');c=read(raw/'contracts-r2/result.json');p=read(raw/'primary-r1/result.json');a=read(raw/'primary-cli-r1/result.json');s=read(raw/'secondary-cli-r2/result.json')
assert all(r['allChecksPassed'] and r['sourceUnchanged'] for r in [n,c,p])
totals=lambda r:{k:sum(x['totals'][k] for x in r['checks']) for k in ['passed','failed','ignored']}
assert totals(n)=={'passed':29,'failed':0,'ignored':0} and totals(c)=={'passed':117,'failed':0,'ignored':2}
for r in [a,s]:
    assert r['completed'] and r['sourceInputsUnchanged'] and r['toolsUnchanged'] and r['allProcessJobsEmpty']
    assert len(r['cases'])==4 and len(r['commands'])==36
    assert all(x['activeAfterCleanup']==0 and x['assignedBeforeResume'] and x['startedSuspended'] for x in r['commands'])
    assert all(x['unverifiedApplyRefused'] and x['unapprovedApplyRefused'] and x['independentTscPassed'] and x['foreignRecoveryRefusedAndPreserved'] and x['conditionalInverseRestoredOriginalBytes'] and x['configurationUnchanged'] and x['ownedRuntimeObservation']['equal'] for x in r['cases'])
for phase in ['contracts-r2','primary-r1']:
    assert all(sha((w/name).read_bytes())==h for name,h in read(raw/phase/'source-after.json').items())
assert not subprocess.check_output(['git','status','--porcelain'],cwd=w).strip()
log=(raw/'primary-r1/retained.log').read_text(encoding='utf-8');assert all(re.search(r'^(?:#|\u2139) '+x+r'$',log,re.M) for x in ['pass 260','fail 0','skipped 3'])
build=next(x for x in p['checks'] if x['name']=='build');assert len(build['primaryPackageArtifacts'])==2 and all(not x['fresh'] for x in build['primaryPackageArtifacts']);assert p['validatedInputCount']==1002
out=w/'docs/validation/integration-2026-10-21/function-body-predicates';out.mkdir(parents=True,exist_ok=False);entries=[]
with zipfile.ZipFile(out/'raw-logs.zip','w',compression=zipfile.ZIP_DEFLATED,compresslevel=9) as archive:
    for f in sorted(raw.rglob('*')):
        if not f.is_file():continue
        assert not f.is_symlink();name=f.relative_to(raw).as_posix();data=f.read_bytes();info=zipfile.ZipInfo(name,date_time=(2026,10,21,0,0,0));info.compress_type=zipfile.ZIP_DEFLATED;info.external_attr=0o100644<<16;archive.writestr(info,data);entries.append({'path':name,'bytes':len(data),'sha256':sha(data)})
with zipfile.ZipFile(out/'raw-logs.zip') as archive:
    assert archive.testzip() is None and set(archive.namelist())=={e['path'] for e in entries}
    for e in entries:
        data=archive.read(e['path']);assert len(data)==e['bytes'] and sha(data)==e['sha256']
(out/'raw-index.json').write_text(json.dumps({'files':entries,'rawMembersByteExact':True,'archiveSha256':sha((out/'raw-logs.zip').read_bytes())},indent=2)+'\n',encoding='utf-8',newline='\n')
checkpoint={'sourceCommit':subprocess.check_output(['git','rev-parse','HEAD'],cwd=w).decode().strip(),'newTargetTotals':totals(n),'regressionTotals':totals(c),'distinctRustCases':146,'newRustFunctionsIncluded':13,'frozenAuxiliaryInputs':1002,'sourceBaselineCopiedInputs':999,'allPrimaryInputsByteExact':True,'primaryBuildArtifacts':build['primaryPackageArtifacts'],'primaryFmtBuildTypecheckPassed':True,'auxiliaryFmtAllTargetClippyPassed':True,'retainedPassed':260,'retainedSkipped':3,'primaryActualCli':a,'secondaryActualCli':s,'rustTestsRepeatedInPrimary':False,'fullCurrentNativeSuiteExecuted':False,'fullR1Certified':False,'fullWindowsCertified':False,'fullStatusCertified':False}
(out/'checkpoint.json').write_text(json.dumps(checkpoint,indent=2)+'\n',encoding='utf-8',newline='\n');shutil.copyfile(__file__,out/'archive-and-docs.py')
(out/'README.md').write_text('''# Classic function predicate, assertion and dense-argument checkpoint

Explicit TypeScript predicates/assertions and compiler-inferred predicates retain
their original signature through an inert source copy and fixed result relay.
The copied source function/defaults never execute. Original arguments run once
before parameter/body initialization. A generated caller reference requires one
captured ordinary lexical binding; imports, ambient/global, namespace and exported
bindings gain no repeatable-read privilege. Separate export aliases are checked
by declaration identity, preserving same-name local parameter shadowing. Getters,
calls and members are never repeated by the relay. Complete surviving compiler
type/binding/diagnostic checks remain. Known-present dense collector indices keep
tuple element types under noUncheckedIndexedAccess, including genuine undefined.

Ten new-target integrations and nineteen body units plus117 regressions across
eight targets pass:146 distinct Rust functions and two existing ignored functions.
Thirteen added functions are already included. Format, worker syntax and all-target
Clippy with warnings denied pass. All1002 validated inputs match primary exactly.
Its fresh library/CLI build records primary source paths and fresh:false; format,
typecheck and TypeScript260/3 existing skips pass. Identical Rust inputs are not
retested solely because the checkout changed.

Each actual binary passes four owned CLI scenarios: explicit predicate, inferred
predicate, assertion and effectful getter/rest arguments. Unverified/unapproved
applies refuse without source changes. Independent tsc, approval and workspace
apply succeed; owned literal before/after observations match, including caught
assertion failure and argument effects. Recovery preserves foreign bytes, then
restores exact original bytes only after the owned postimage is restored. Each
control executes36 commands created suspended and assigned to Windows jobs before
resume; every job ends empty. Inputs/tools/configuration stay unchanged. Only
owned literal runtime oracles execute; preparation executes no analyzed code.

Failing phases remain: annotation-as-value rejection, corrected test fixture
lifetime, unchecked-access uncertainty, and actual native acceptance of a
separately exported predicate target. That last counterexample fails after a
same-name local-shadow positive, then passes with declaration-bound export checks.
Worker-only getter diagnostics claim no native admission. Earlier passing phases
do not qualify final source or add counts. Raw CRLF/trailing bytes stay intact.
An additional actual read-only CLI probe rejects a script-global predicate target
at the existing strict-ES-module gate before publishing a plan; no apply or
runtime oracle is executed for that refused fixture.

[checkpoint.json](checkpoint.json) pins commits, binaries, results and procedures.
[raw-index.json](raw-index.json) indexes exact byte lengths/SHA-256 hashes of every
decompressed [raw-logs.zip](raw-logs.zip) member, verified individually. Only five
documents change after validation; their hashes are reconciled separately.

JSDoc predicates, generic/this/wider forms, unsafe property-flow escape, wider
runtime/upstream equivalence and fullR1/R1-R9 stay open. Result complete=false
remains. The historical bcf442b full baseline predates this addition:2951 passed,
10 original OS1314 symlink-setup failures and53 ignored. No current-head complete
native/hosted/main/release acceptance is claimed. No Developer Mode, privilege or
system setting changes. No approved Google document; live export stays unvalidated.
''',encoding='utf-8',newline='\n')
def replace(name,old,new):
    f=w/name;s=f.read_text(encoding='utf-8-sig');assert s.count(old)==1,(name,old[:70]);f.write_text(s.replace(old,new),encoding='utf-8',newline='\n')
replace('native/docs/compiler-inline-predicates.md','Current scoped native/CLI qualification is pending. JSDoc predicates, generics,','The scoped guard passes146 distinct Rust functions/2 existing ignored, including\nten new-target integrations and nineteen body units. Format/all-target Clippy,\nfresh primary library/CLI build, typecheck and TypeScript260/3 existing skips\npass; all1002 validated inputs match primary byte-exactly. Each actual binary\npasses four owned CLI verification/approval/apply/conditional-recovery scenarios\nand before/after literal runtime comparisons. See the\n[indexed evidence](../../docs/validation/integration-2026-10-21/function-body-predicates/README.md).\nJSDoc predicates, generics,')
replace('native/docs/compiler-inline.md','An explicit return annotation is copied\nfrom source; inferred primitive return hints have a fixed keyword whitelist.','An explicit ordinary value return annotation is copied\nfrom source; inferred primitive return hints have a fixed keyword whitelist.\n[Predicate/assertion signatures](compiler-inline-predicates.md) use an inert\noriginal signature and fixed result relay instead of treating a predicate as a\nvalue type. Eligible captured lexical targets retain a separately checked pure\ncaller reference for narrowing; calls/getters are never repeated. Dense collector\nindices retain original tuple element types under noUncheckedIndexedAccess,\nincluding genuine undefined alternatives.')
replace('docs/ARCHITECTURE.md','## Native Rust ownership\n','''## Native Rust ownership

Classic compiler-selected body expansion preserves predicate/assertion caller
flow through an inert original signature and fixed relay of the computed result.
Native derives explicit source targets, restricts generated references to ordinary
captured lexical bindings and matches separate exports by declaration identity.
Inferred predicates retain the trusted compiler boundary; complete surviving
observations still agree. Known dense argument indices retain element types and
genuine undefined. Original evaluation and independent write gates remain. See
the [predicate contract](../native/docs/compiler-inline-predicates.md).
''')
replace('docs/REMAINING_WORK.md','finite classic function-body statement expansion with checked var hoisting','finite classic function-body statement expansion with checked var hoisting, explicit/inferred TypeScript predicate and assertion flow and dense argument typing')
replace('docs/REMAINING_WORK.md','Further return/predicate/parameter/annotation contexts, diagnostic combinations and wider body/expression/invocation forms','Further return/parameter/annotation contexts, JSDoc/generic/this and wider predicate-target forms, diagnostic combinations and wider body/expression/invocation forms')
replace('docs/REMAINING_WORK.md','## Acceptance still required\n','''## Acceptance still required

Explicit/inferred TypeScript predicates and assertions now preserve caller flow
through an inert source signature and fixed computed-result relay. Eligible
ordinary lexical bindings have independently checked pure caller references;
imports, ambient/global/namespace/exported bindings gain no repeated-read right.
Separate export aliases are declaration-bound, including local-shadow controls.
Arguments/getters run once; dense noUncheckedIndexedAccess types retain genuine
undefined. Full diagnostics and surviving type/binding observations still agree.
The scoped guard passes146 Rust functions/2 existing ignored; format/all-target
Clippy,1002 same-byte primary inputs, fresh primary build, typecheck and
TypeScript260/3 skips pass. Both actual binaries pass four owned CLI controls
with independent verification, approval, application, foreign-preserving recovery
and equal literal runtime observations. Failed causal phases remain separate.
See the [predicate evidence](validation/integration-2026-10-21/function-body-predicates/README.md).
JSDoc predicates, generic/this/wider forms and complete R1-R9 acceptance stay open.
The following caller-documentation evidence belongs to its preceding checkpoint.
''')
replace('docs/STATUS.md','## Current integration work\n','''## Current integration work

The optional TypeScript5.8.3 classic body route now preserves explicit/inferred
predicate and assertion caller flow with an inert original signature and fixed
computed-result relay. Pure generated caller references require one captured
ordinary lexical binding; separate exports/aliases are declaration-bound.
Calls/getters are never replayed. Known dense indices retain element types under
noUncheckedIndexedAccess, including genuine undefined. Complete surviving
compiler observations and independent write gates remain.

The scoped guard passes146 Rust functions/2 existing ignored across ten new-target
integrations, nineteen body units and117 regression integrations. Format/all-target
Clippy,1002 byte-exact primary inputs, fresh primary library/CLI build, typecheck
and TypeScript260/3 existing skips pass. Each actual binary passes four owned CLI
controls with independent tsc, verification, approval, application, foreign-byte
refusal/conditional recovery and equal literal runtime observations. All process
jobs end empty; preparation executes no analyzed source. Failed causal phases
and diagnostic-only probes stay separate. See the
[predicate evidence](validation/integration-2026-10-21/function-body-predicates/README.md)
and [contract](../native/docs/compiler-inline-predicates.md).
JSDoc predicates, generic/this/wider forms, full R1-R9 and current-head whole-native/
hosted/platform/upstream/main/release acceptance stay open. The full bcf442b
baseline and its ten OS1314 failures remain historical and explicit.
''')
changed=[]
for name,h in read(raw/'primary-r1/source-after.json').items():
    now=sha((w/name).read_bytes())
    if now!=h:changed.append({'path':name,'validatedSha256':h,'documentedSha256':now})
assert {x['path'] for x in changed}=={'native/docs/compiler-inline-predicates.md','native/docs/compiler-inline.md','docs/ARCHITECTURE.md','docs/REMAINING_WORK.md','docs/STATUS.md'}
(out/'post-validation-docs.json').write_text(json.dumps({'postValidationChangesOnlyDocumentation':True,'allRuntimeInputsByteExact':True,'changedDocuments':changed},indent=2)+'\n',encoding='utf-8',newline='\n')
index=[{'path':f.name,'bytes':f.stat().st_size,'sha256':sha(f.read_bytes())} for f in sorted(out.iterdir()) if f.is_file()]
(out/'index.json').write_text(json.dumps({'files':index,'rawLogsCompressedWithoutModification':True},indent=2)+'\n',encoding='utf-8',newline='\n')
print(json.dumps({'archive':str(out),'rawFiles':len(entries),'rustScopedPassed':146,'rustIgnored':2,'retainedPassed':260,'retainedSkipped':3,'allInputsMatched':1002,'cliSha256':a['cliSha256'],'fullStatusCertified':False}))
