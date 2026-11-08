from pathlib import Path
import copy,hashlib,json,subprocess,shutil
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');raw=w.parent/'Yoke-CodeIntelligence/validation/integration-20261018/function-body-completion'
read=lambda p:json.loads(p.read_text(encoding='utf-8'));sha=lambda b:hashlib.sha256(b).hexdigest()
guard=read(raw/'guard-r4/result.json');ret=read(raw/'retained-r5/result.json')
assert all(r['allChecksPassed'] and r['sourceUnchanged'] for r in [guard,ret])
assert next(x for x in guard['commands'] if x['name']=='native')['totals']=={'passed':113,'failed':0,'ignored':2}
assert next(x for x in guard['commands'] if x['name']=='units')['totals']=={'passed':11,'failed':0,'ignored':0}
native=read(raw/'guard-r4/source-after.json');retained=read(raw/'retained-r5/source-after.json')
assert len(native)==809 and len(retained)==803
assert all(sha((w/n).read_bytes())==h for n,h in native.items())
assert all(sha((w/n).read_bytes())==h for n,h in retained.items())
out=raw/'metadata';out.mkdir(exist_ok=False)
tracker='docs/parity/native-reconciliation-2026-10-15.json';p=w/tracker;original=p.read_bytes();before=read(p);after=copy.deepcopy(before)
old=next(x for x in before['prioritizedRemainingScopes'] if x['id']=='R1');item=next(x for x in after['prioritizedRemainingScopes'] if x['id']=='R1')
needle='Other inferred return/parameter/annotation contexts and strict-option returning bodies still require further work; projected errors visibly refuse.'
assert item['missingBehavior'].count(needle)==1 and 'functionBodyCompletionDevelopment' not in item
item['missingBehavior']=item['missingBehavior'].replace(needle,'The original body now runs under a fixed runtime-true void guard inside an independently reconstructed completion block. Normal completion clears cancelled returns outside that original body; an exit label is emitted only for actual outer returns. Enabled unreachable-code and unused-label options remain active. Fixed unknown intermediate assertions preserve original/fixed result annotations on generated never/undefined paths. Nonprimitive inferred result types additionally derive from exact original parameters/body and attached leading JSDoc in an inert function carrier, using fixed typeof syntax over a never-executed call with the already evaluated argument temporaries. JSDoc template/literal instantiation is preserved without reevaluating arguments. Fixed empty statements retain documentation ownership when a definition is removed, including typedefs and neighboring async/generator declarations. The compiler checks documentation type-expression/template-default free bindings against the consumer scope. Native literal wrappers retain fixed AST-checked const assertions; bounded original function-tag pieces keep module type declarations at their original site and preserve aliases. Trailing-comment ownership, BOM, original bindings and surviving caller types remain checked. Further return/predicate/parameter/annotation contexts and wider body/expression/invocation/declaration/language/consumer semantics remain open; projected errors visibly refuse.')
item['suggestedScope']='Continue remaining return/predicate/parameter/annotation contexts, diagnostic combinations and wider body/expression/invocation forms, remaining import attributes and sibling declaration bundles, export/import/destination/context/language/provider shapes. Preserve original byte/binding/type correspondence, legacy variable-inline and graph/plan/session identities, shared budgets and independent verification/application/recovery. Qualify dynamic consumers and full Windows/current-head hosted/upstream behavior separately.'
item['functionBodyCompletionDevelopment']={'implemented':True,'fullContractCertified':False,'compiler':'TypeScript 5.8.3','hostOptIn':'inlineCompiler','actionKind':'native.inline.function.body','newIntegrationFunctions':2,'newUnitFunctions':2,'strictCompletionIntegrationScenarios':26,'genericDocumentationPositiveScenarios':13,'documentationShadowingRefusalScenarios':1,'positiveBodyCliGateScenarios':9,'newPositiveStrictCliGateScenarios':4,'guardPhase':'guard-r4','guardUnitTotals':next(x for x in guard['commands'] if x['name']=='units')['totals'],'guardIntegrationTotals':next(x for x in guard['commands'] if x['name']=='native')['totals'],'guardGroups':8,'nativeFrozenInputs':len(native),'retainedPhase':'retained-r5','retainedFrozenInputs':len(retained),'countsAreNonadditive':True,'wholeNativeRerun':False,'platformEvidence':'Windows','otherPlatformsCertified':False,'supersedesHistoricalReturningDiagnosticAndInferredResultLimitation':True,'evidence':['native/src/compiler_inline_body.rs','native/src/compiler_inline_body_worker.cjs','native/src/compiler_inline_body_admission.rs','native/tests/compiler_inline_body.rs','docs/validation/integration-2026-10-19/function-body-completion/index.json'],'remaining':['Further return/predicate/parameter/annotation contexts and diagnostic-option combinations','Wider expression/invocation/declaration/language/consumer forms','General dynamic/reflective/runtime equivalence','Full STATUS/R1-R9 and Windows/current-head hosted/upstream acceptance']}
restored=copy.deepcopy(after);r=next(x for x in restored['prioritizedRemainingScopes'] if x['id']=='R1')
for key in ['missingBehavior','suggestedScope']:r[key]=old[key]
del r['functionBodyCompletionDevelopment'];assert restored==before
assert list(old)==[k for k in item if k!='functionBodyCompletionDevelopment']
(out/'tracker-before.json').write_bytes(original)
p.write_text(json.dumps(after,indent=2,ensure_ascii=False)+'\n',encoding='utf-8',newline='\n')
summary='''Classic function-body expansion under the optional `inlineCompiler` profile now
retains enabled unreachable-code and unused-label diagnostics. The original body
block runs under the fixed runtime-true guard `void 0 === void 0`; only actual outer
returns need an exit label. Normal completion clears a return cancelled by finally
control flow outside the original body. Throwing and never-typed return paths keep
their runtime behavior, including a declared-never call that actually returns.

Nonprimitive inferred result types use fixed `typeof` syntax over a fresh result
carrier. A separate original-parameter/body function carrier, with attached leading
JSDoc, is called only with the already evaluated argument temporaries. Literal const/contextual values use a fixed readonly wrapper and indexed typeof
query; other values retain ordinary inference. Original module type declarations
stay at their original site, with only bounded source function-tag pieces copied
when needed, preserving typedef identity. Both
initializers and the read remain under `void 0 !== void 0`; neither clone nor call
executes. This preserves JSDoc template/literal instantiation without reevaluating
arguments. Fixed empty statements protect neighboring declarations from orphaned
documentation while retaining original comment/typedef bytes. The compiler checks
free documentation type/value bindings against the consumer scope. Original/fixed annotations use a fixed intermediate
unknown assertion
for generated undefined paths. Native reconstruction checks source bytes, comment
ownership, fresh names, copied bindings and surviving caller types. Original var
storage carriers, argument/default environments, labelled returns, finally
cancellation and eval refusal retain their existing guarded contract. Preparation
executes no analyzed source or arbitrary compiler type expressions.

The final scoped run passed **113 integrations / 0 failures / 2 existing ignored**
across eight targets, plus eleven body units, on 809 unchanged native inputs.
Format/build/all-target Clippy with warnings denied pass. Python 17, typecheck and
TypeScript 260 / 3 existing skips pass on 803 fixed retained inputs. These frozen
scopes overlap; totals are not added. Only documented metadata changes follow,
with exact runtime and tracker preservation checks.

Two added integration functions cover 26 strict completion/type scenarios plus
thirteen generic/documentation positives and one explicit shadowing refusal. Two new
unit functions cover attached/trailing comments, BOM/Unicode, original bytes,
deadlines and bounds; extended admission tests reject forged guards, names and
type queries, carrier argument maps and documentation neutralization. Nine body
CLI cases include four new strict TS/JS cases with independent tsc, approval, application and foreign-preserving recovery. Runtime
comparisons execute only owned literal fixtures. Failed development phases retain
the original invalid test fixtures and the BOM regression caught by the new unit.

Further return/predicate/parameter/annotation contexts, diagnostic combinations,
wider expression/invocation/declaration/language/consumer forms and full STATUS/
R1-R9/Windows/current-head hosted/upstream acceptance remain open. Existing
OS1314/macOS failures and production App deployment remain separate. See
[the body contract](../native/docs/compiler-inline.md#classic-function-body-statements)
and [completion/type evidence](validation/integration-2026-10-19/function-body-completion/README.md).

'''
changed_docs=[]
def update_doc(name,change):
 p=w/name;b=p.read_bytes();s=b.decode('utf-8');nl='\r\n' if b.count(b'\r\n')==b.count(b'\n') and b.count(b'\n') else '\n';s=s.replace('\r\n','\n');result=change(s);assert result!=s,name
 target=out/'before'/name;target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(b)
 p.write_bytes(result.replace('\n',nl).encode('utf-8'));changed_docs.append({'path':name,'beforeSha256':sha(b),'afterSha256':sha(p.read_bytes())})
def status(s):
 start=s.index('Classic function-body expansion under');end=s.index('Captured missing compiler-move imports',start);return s[:start]+summary+s[end:]
update_doc('docs/STATUS.md',status)
update_doc('docs/REMAINING_WORK.md',lambda s:status(s).replace('| Remaining inferred return/parameter/annotation contexts, strict diagnostic combinations and wider body/expression/invocation forms','| Further return/predicate/parameter/annotation contexts, diagnostic combinations and wider body/expression/invocation forms',1))
def architecture(s):
 needle='Remaining inferred contexts and invocation/expression forms remain open.';assert needle in s
 return s.replace(needle,'A fixed runtime-true guard retains the original body block and enabled reachability\nchecks; normal completion clears cancelled returns outside it, with a label only\nfor actual outer returns. Inferred result types use fixed typeof syntax over an inert\nresult carrier from a\nnever-executed source-function call with already evaluated fresh argument temps.\nAttached JSDoc templates, literal inference and free bindings retain checked scope.\nEmpty declaration statements protect neighboring annotation/typedef ownership.\nOriginal/fixed result assertions use a fixed unknown intermediate. Native\ncomment/BOM ownership, fresh names and copied binding/type checks remain mandatory.\nFurther return/parameter/annotation contexts and invocation/expression forms remain open.',1)
update_doc('docs/ARCHITECTURE.md',architecture)
assert all(sha((w/n).read_bytes())==h for n,h in native.items())
assert all(sha((w/n).read_bytes())==h for n,h in retained.items() if n!=tracker)
proof={'baseCommit':subprocess.check_output(['git','rev-parse','HEAD'],cwd=w).decode().strip(),'changedR1Fields':['missingBehavior','suggestedScope','functionBodyCompletionDevelopment'],'allOtherDecodedDataPreserved':True,'existingKeyOrderPreserved':True,'trackerBeforeSha256':sha(original),'trackerAfterSha256':sha(p.read_bytes()),'runtimeCodeChangedAfterFinalChecks':False,'nativeInputsExact':len(native),'otherRetainedInputsExact':len(retained)-1,'documents':changed_docs}
(out/'proof.json').write_text(json.dumps(proof,indent=2)+'\n',encoding='utf-8');shutil.copyfile(__file__,out/'runner.py');print(json.dumps(proof))
