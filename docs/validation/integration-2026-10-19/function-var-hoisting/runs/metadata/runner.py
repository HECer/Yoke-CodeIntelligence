from pathlib import Path
import copy,hashlib,json,subprocess,shutil
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');raw=w.parent/'Yoke-CodeIntelligence/validation/integration-20261018/function-var-hoisting'
read=lambda p:json.loads(p.read_text(encoding='utf-8'));sha=lambda b:hashlib.sha256(b).hexdigest()
guard=read(raw/'guard-r3/result.json');review=read(raw/'review-r1/result.json');ret=read(raw/'retained-r3/result.json');repair=read(raw/'fixture-repair/proof.json')
assert all(r['allChecksPassed'] and r['sourceUnchanged'] for r in [guard,review,ret])
assert next(x for x in guard['commands'] if x['name']=='native')['totals']=={'passed':109,'failed':0,'ignored':2}
assert next(x for x in review['commands'] if x['name']=='native')['totals']=={'passed':7,'failed':0,'ignored':0}
assert next(x for x in review['commands'] if x['name']=='live')['totals']=={'passed':2,'failed':0,'ignored':0}
assert repair['productionPrefixUnchanged'] and repair['allOtherGuardInputsUnchanged']
native=read(raw/'review-r1/source-after.json');retained=read(raw/'retained-r3/source-after.json')
assert all(sha((w/n).read_bytes())==h for n,h in native.items())
assert all(sha((w/n).read_bytes())==h for n,h in retained.items())
out=raw/'metadata';out.mkdir(exist_ok=False)
tracker='docs/parity/native-reconciliation-2026-10-15.json';p=w/tracker;original=p.read_bytes();before=read(p);after=copy.deepcopy(before)
old=next(x for x in before['prioritizedRemainingScopes'] if x['id']=='R1');item=next(x for x in after['prioritizedRemainingScopes'] if x['id']=='R1')
assert 'functionVarHoistingDevelopment' not in item
needle='Var hoisting, async/generic/invocation-bound and wider expression/language/context/consumer semantics remain open.'
assert item['missingBehavior'].count(needle)==1
item['missingBehavior']=item['missingBehavior'].replace(needle,'The body route now independently reconstructs finite function-owned var hoisting, parameter-value copies, duplicate and Unicode bindings, shared loop storage, destructuring assignments and simple catch-parameter assignment targets. Exact original declaration/type/assignment witnesses and distinct property roles retain raw observations. Complex inferred types must pass projected diagnostics without arbitrary type insertion. Async/generic/invocation-bound and wider expression/language/context/consumer semantics remain open.')
item['suggestedScope']='Continue wider body/expression/invocation forms, inferred type shapes, remaining import attributes and sibling declaration bundles, export/import/destination/context/language/provider shapes. Preserve original byte/binding/type correspondence, legacy variable-inline and graph/plan/session identities, shared budgets and independent verification/application/recovery. Qualify dynamic consumers and full Windows/current-head hosted/upstream behavior separately.'
item['functionVarHoistingDevelopment']={'implemented':True,'fullContractCertified':False,'compiler':'TypeScript 5.8.3','hostOptIn':'inlineCompiler','actionKind':'native.inline.function.body','newIntegrationFunctions':2,'newUnitFunctions':3,'newVarIntegrationScenarios':21,'newOwnedJsUnitScenarios':11,'positiveBodyCliGateScenarios':4,'newPositiveVarCliGateScenarios':2,'guardPhase':'guard-r3','guardUnitTotals':next(x for x in guard['commands'] if x['name']=='units')['totals'],'guardIntegrationTotals':next(x for x in guard['commands'] if x['name']=='native')['totals'],'guardGroups':8,'reviewPhase':'review-r1','reviewUnitTotals':next(x for x in review['commands'] if x['name']=='native')['totals'],'separatelySelectedLiveSessionTotals':next(x for x in review['commands'] if x['name']=='live')['totals'],'nativeFrozenInputs':len(native),'retainedPhase':'retained-r3','retainedFrozenInputs':len(retained),'countsAreNonadditive':True,'fixtureRepair':repair,'wholeNativeRerun':False,'platformEvidence':'Windows','otherPlatformsCertified':False,'supersedesHistoricalVarHoistingLimitation':True,'evidence':['native/src/compiler_inline_body_vars.rs','native/src/compiler_inline_body_vars_worker.cjs','native/src/compiler_inline_body_admission.rs','native/tests/compiler_inline_body.rs','docs/validation/integration-2026-10-19/function-var-hoisting/index.json'],'remaining':['Complex inferred type preservation requiring further generated structures','Wider expression/invocation/declaration/language/consumer forms','General dynamic/reflective/runtime equivalence','Full STATUS/R1-R9 and Windows/current-head hosted/upstream acceptance']}
restored=copy.deepcopy(after);r=next(x for x in restored['prioritizedRemainingScopes'] if x['id']=='R1')
for key in ['missingBehavior','suggestedScope']:r[key]=old[key]
del r['functionVarHoistingDevelopment'];assert restored==before
assert list(old)==[k for k in item if k!='functionVarHoistingDevelopment']
(out/'tracker-before.json').write_bytes(original)
p.write_text(json.dumps(after,indent=2,ensure_ascii=False)+'\n',encoding='utf-8',newline='\n')
summary='''Classic function-body expansion under the optional `inlineCompiler` profile now
reconstructs function-owned `var` hoists, original-position initializers and shared
loop storage. Parameter copies preserve default-closure environments; duplicate,
escaped, destructured and simple catch bindings have separate exact source and
assignment correspondence. Nested scopes retain their own declarations. Original
annotations and fixed primitive hints preserve admitted compiler types. Arguments,
defaults/rest, labelled returns, finally cancellation and eval refusal retain their
existing contract. Preparation executes no analyzed source.

The broad scoped run passed **109 integrations / 0 failures / 2 existing ignored**
across eight targets, plus seven body units, on 807 unchanged native inputs.
After restoring two original Unicode test literals, all production text and the
other 806 inputs match that freeze. The current review passes seven body units and
separately executes both previously ignored live session tests with actual
TypeScript 5.8.3 and language-server 5.1.3. Format/build/all-target Clippy pass.
Python17, typecheck and TypeScript260/3skipped pass on 801 fixed retained inputs.
The distinct frozen scopes overlap; totals are not added.

Two new integration functions cover 21 var scenarios; three new unit functions
cover forged assignment/type/property/capture witnesses, Unicode and budgets.
Four body CLI verification/approval/apply/recovery scenarios include two new var
cases. Runtime comparisons execute only owned literal fixtures. Failed development
phases and the original invalid TypeScript fixture remain archived. Only documented
metadata changes follow the accepted checks, with exact runtime and tracker proofs.

Wider inferred types, expression/invocation/declaration/language/consumer forms and
full STATUS/R1-R9/Windows/current-head hosted/upstream acceptance remain open.
Existing OS1314/macOS failures and production App deployment remain separate. See
[the body contract](../native/docs/compiler-inline.md#classic-function-body-statements)
and [var evidence](validation/integration-2026-10-19/function-var-hoisting/README.md).

'''
changed_docs=[]
def update_doc(name,change):
 p=w/name;b=p.read_bytes();s=b.decode('utf-8');nl='\r\n' if b.count(b'\r\n')==b.count(b'\n') and b.count(b'\n') else '\n';s=s.replace('\r\n','\n');result=change(s);assert result!=s,name
 target=out/'before'/name;target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(b)
 p.write_bytes(result.replace('\n',nl).encode('utf-8'));changed_docs.append({'path':name,'beforeSha256':sha(b),'afterSha256':sha(p.read_bytes())})
def status(s):
 start=s.index('Classic function-body expansion now');end=s.index('Captured missing compiler-move imports',start);return s[:start]+summary+s[end:]
update_doc('docs/STATUS.md',status)
def remaining(s):
 s=status(s);return s.replace('finite classic function-body statement expansion and moves','finite classic function-body statement expansion with checked var hoisting and moves',1).replace('| Var hoisting, wider body/expression/invocation forms','| Wider inferred types and body/expression/invocation forms',1)
update_doc('docs/REMAINING_WORK.md',remaining)
def architecture(s):
 needle='runtime equivalence. Var-hoisting and wider invocation/expression forms remain\nopen.';assert needle in s
 return s.replace(needle,'runtime equivalence. Function-owned var hoists retain original-position\ninitializers, shared loop storage and separate parameter/catch assignment roles.\nExplicit annotations and fixed primitive hints retain admitted compiler types.\nWider inferred types and invocation/expression forms remain open.',1)
update_doc('docs/ARCHITECTURE.md',architecture)
assert all(sha((w/n).read_bytes())==h for n,h in native.items())
assert all(sha((w/n).read_bytes())==h for n,h in retained.items() if n!=tracker)
proof={'baseCommit':subprocess.check_output(['git','rev-parse','HEAD'],cwd=w).decode().strip(),'changedR1Fields':['missingBehavior','suggestedScope','functionVarHoistingDevelopment'],'allOtherDecodedDataPreserved':True,'existingKeyOrderPreserved':True,'trackerBeforeSha256':sha(original),'trackerAfterSha256':sha(p.read_bytes()),'runtimeCodeChangedAfterFinalChecks':False,'nativeInputsExact':len(native),'otherRetainedInputsExact':len(retained)-1,'documents':changed_docs}
(out/'proof.json').write_text(json.dumps(proof,indent=2)+'\n',encoding='utf-8');shutil.copyfile(__file__,out/'runner.py');print(json.dumps(proof))
