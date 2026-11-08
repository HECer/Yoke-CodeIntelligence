from pathlib import Path
import copy,hashlib,json,subprocess,shutil
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');raw=w.parent/'Yoke-CodeIntelligence/validation/integration-20261018/function-body-types'
read=lambda p:json.loads(p.read_text(encoding='utf-8'));sha=lambda b:hashlib.sha256(b).hexdigest()
guard=read(raw/'guard-r1/result.json');ret=read(raw/'retained-r1/result.json')
assert all(r['allChecksPassed'] and r['sourceUnchanged'] for r in [guard,ret])
assert next(x for x in guard['commands'] if x['name']=='native')['totals']=={'passed':111,'failed':0,'ignored':2}
native=read(raw/'guard-r1/source-after.json');retained=read(raw/'retained-r1/source-after.json')
assert len(native)==809 and len(retained)==803
assert all(sha((w/n).read_bytes())==h for n,h in native.items())
assert all(sha((w/n).read_bytes())==h for n,h in retained.items())
out=raw/'metadata';out.mkdir(exist_ok=False)
tracker='docs/parity/native-reconciliation-2026-10-15.json';p=w/tracker;original=p.read_bytes();before=read(p);after=copy.deepcopy(before)
old=next(x for x in before['prioritizedRemainingScopes'] if x['id']=='R1');item=next(x for x in after['prioritizedRemainingScopes'] if x['id']=='R1')
needle='Complex inferred types must pass projected diagnostics without arbitrary type insertion.'
assert item['missingBehavior'].count(needle)==1 and 'functionBodyTypeInferenceDevelopment' not in item
item['missingBehavior']=item['missingBehavior'].replace(needle,'Nonprimitive inferred var storage now uses fixed typeof syntax with fresh carriers derived from original declarations or loop heads at their lexical sites. Exact original-byte reconstruction and raw binding/type coverage include destructured names. The fixed pure-false void guard prevents copied initializers, iterable expressions and reads from executing, while preserving enabled diagnostic options. Native carriers and composed insertion/return edits retain source order, labels and unbraced control scope. Other inferred return/parameter/annotation contexts and strict-option returning bodies still require further work; projected errors visibly refuse.')
item['suggestedScope']='Continue remaining body/expression/invocation forms, inferred return/parameter/annotation contexts and diagnostic-option combinations, remaining import attributes and sibling declaration bundles, export/import/destination/context/language/provider shapes. Preserve original byte/binding/type correspondence, legacy variable-inline and graph/plan/session identities, shared budgets and independent verification/application/recovery. Qualify dynamic consumers and full Windows/current-head hosted/upstream behavior separately.'
item['functionBodyTypeInferenceDevelopment']={'implemented':True,'fullContractCertified':False,'compiler':'TypeScript 5.8.3','hostOptIn':'inlineCompiler','actionKind':'native.inline.function.body','newIntegrationFunctions':2,'newUnitFunctions':2,'inferredIntegrationScenarios':22,'strictDiagnosticIntegrationScenarios':2,'positiveBodyCliGateScenarios':5,'newPositiveInferredCliGateScenarios':1,'guardPhase':'guard-r1','guardUnitTotals':next(x for x in guard['commands'] if x['name']=='units')['totals'],'guardIntegrationTotals':next(x for x in guard['commands'] if x['name']=='native')['totals'],'guardGroups':8,'nativeFrozenInputs':len(native),'retainedPhase':'retained-r1','retainedFrozenInputs':len(retained),'countsAreNonadditive':True,'wholeNativeRerun':False,'platformEvidence':'Windows','otherPlatformsCertified':False,'evidence':['native/src/compiler_inline_body_types.rs','native/src/compiler_inline_body_types_worker.cjs','native/src/compiler_inline_body_admission.rs','native/tests/compiler_inline_body.rs','docs/validation/integration-2026-10-19/function-body-types/index.json'],'remaining':['Inferred return/parameter/annotation contexts and other diagnostic-option combinations','Returning body fixture with allowUnreachableCode:false still visibly refuses; precise cause not localized','Wider expression/invocation/declaration/language/consumer forms','General dynamic/reflective/runtime equivalence','Full STATUS/R1-R9 and Windows/current-head hosted/upstream acceptance']}
restored=copy.deepcopy(after);r=next(x for x in restored['prioritizedRemainingScopes'] if x['id']=='R1')
for key in ['missingBehavior','suggestedScope']:r[key]=old[key]
del r['functionBodyTypeInferenceDevelopment'];assert restored==before
assert list(old)==[k for k in item if k!='functionBodyTypeInferenceDevelopment']
(out/'tracker-before.json').write_bytes(original)
p.write_text(json.dumps(after,indent=2,ensure_ascii=False)+'\n',encoding='utf-8',newline='\n')
summary='''Classic function-body expansion under the optional `inlineCompiler` profile now
preserves nonprimitive inferred `var` storage using fresh `typeof` carriers from
original declarations or loop heads at their lexical sites. Native reconstruction
checks original source, destructured declaration types and every copied binding.
The fixed pure-false guard `void 0 !== void 0` keeps copied initializers, iterables
and reads inert; carrier bindings are function/module scoped. Actual body storage
remains lexical and fresh per expansion. Quoted shorthand keys, labels, unbraced
control slots and composed insertion/return edits retain source correspondence.
Arguments, parameter/default environments, shared var storage, labelled returns,
finally cancellation and eval refusal retain their existing guarded contract.
Preparation executes no analyzed source or arbitrary compiler type expressions.

The final scoped run passed **111 integrations / 0 failures / 2 existing ignored**
across eight targets, plus nine body units, on 809 unchanged native inputs.
Format/build/all-target Clippy with warnings denied pass. Python 17, typecheck and
TypeScript 260 / 3 existing skips pass on 803 fixed retained inputs. These frozen
scopes overlap; totals are not added. Only documented metadata changes follow,
with exact runtime and tracker preservation checks.

Two added integration functions cover 22 inferred object/array/tuple/union/function/
loop/destructuring scenarios and two cases with enabled strict diagnostic options.
Two new unit functions and extended admission tests cover original UTF-8/CRLF bytes,
quoted property keys, deadlines and coherent forged carrier/type/guard witnesses.
Five body CLI cases include one new inferred loop case with independent tsc,
approval, application and foreign-preserving recovery. Runtime comparisons execute
only owned literal fixtures. Failed development phases remain archived.

Inferred return/parameter/annotation contexts, other diagnostic-option combinations,
wider expression/invocation/declaration/language/consumer forms and full STATUS/
R1-R9/Windows/current-head hosted/upstream acceptance remain open. A returning-body
fixture with `allowUnreachableCode:false` still visibly refuses; its precise cause
has not been localized. Existing OS1314/macOS failures and production App deployment
remain separate. See [the body contract](../native/docs/compiler-inline.md#classic-function-body-statements)
and [inferred-type evidence](validation/integration-2026-10-19/function-body-types/README.md).

'''
changed_docs=[]
def update_doc(name,change):
 p=w/name;b=p.read_bytes();s=b.decode('utf-8');nl='\r\n' if b.count(b'\r\n')==b.count(b'\n') and b.count(b'\n') else '\n';s=s.replace('\r\n','\n');result=change(s);assert result!=s,name
 target=out/'before'/name;target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(b)
 p.write_bytes(result.replace('\n',nl).encode('utf-8'));changed_docs.append({'path':name,'beforeSha256':sha(b),'afterSha256':sha(p.read_bytes())})
def status(s):
 start=s.index('Classic function-body expansion under');end=s.index('Captured missing compiler-move imports',start);return s[:start]+summary+s[end:]
update_doc('docs/STATUS.md',status)
update_doc('docs/REMAINING_WORK.md',lambda s:status(s).replace('| Wider inferred types and body/expression/invocation forms','| Remaining inferred return/parameter/annotation contexts, strict diagnostic combinations and wider body/expression/invocation forms',1))
def architecture(s):
 needle='Explicit annotations and fixed primitive hints retain admitted compiler types.\nWider inferred types and invocation/expression forms remain open.';assert needle in s
 return s.replace(needle,'Explicit annotations and fixed primitive hints retain admitted compiler types.\nNonprimitive var storage additionally uses fixed typeof syntax with fresh carriers\ninferred from original declarations or loop heads at their lexical sites. The fixed\npure-false void guard prevents copied expressions and reads from executing without\nsuppressing enabled compiler diagnostics. Native byte reconstruction, raw binding/\ntype coverage and source-ordered composition preserve keys, labels and control slots.\nRemaining inferred contexts and invocation/expression forms remain open.',1)
update_doc('docs/ARCHITECTURE.md',architecture)
assert all(sha((w/n).read_bytes())==h for n,h in native.items())
assert all(sha((w/n).read_bytes())==h for n,h in retained.items() if n!=tracker)
proof={'baseCommit':subprocess.check_output(['git','rev-parse','HEAD'],cwd=w).decode().strip(),'changedR1Fields':['missingBehavior','suggestedScope','functionBodyTypeInferenceDevelopment'],'allOtherDecodedDataPreserved':True,'existingKeyOrderPreserved':True,'trackerBeforeSha256':sha(original),'trackerAfterSha256':sha(p.read_bytes()),'runtimeCodeChangedAfterFinalChecks':False,'nativeInputsExact':len(native),'otherRetainedInputsExact':len(retained)-1,'documents':changed_docs}
(out/'proof.json').write_text(json.dumps(proof,indent=2)+'\n',encoding='utf-8');shutil.copyfile(__file__,out/'runner.py');print(json.dumps(proof))
