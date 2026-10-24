from pathlib import Path
import copy,hashlib,json,subprocess,shutil
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');raw=w.parent/'Yoke-CodeIntelligence/validation/integration-20261018/function-body-argument-contexts'
read=lambda p:json.loads(p.read_text(encoding='utf-8'));sha=lambda b:hashlib.sha256(b).hexdigest()
guard=read(raw/'guard-r2/result.json');ret=read(raw/'retained-r2/result.json')
assert all(r['allChecksPassed'] and r['sourceUnchanged'] for r in [guard,ret])
assert next(x for x in guard['commands'] if x['name']=='native')['totals']=={'passed':116,'failed':0,'ignored':2}
assert next(x for x in guard['commands'] if x['name']=='units')['totals']=={'passed':14,'failed':0,'ignored':0}
native=read(raw/'guard-r2/source-after.json');retained=read(raw/'retained-r2/source-after.json');assert len(native)==811 and len(retained)==805
assert all(sha((w/n).read_bytes())==h for n,h in native.items());assert all(sha((w/n).read_bytes())==h for n,h in retained.items())
out=raw/'metadata';out.mkdir(exist_ok=False)
tracker='docs/parity/native-reconciliation-2026-10-15.json';p=w/tracker;original=p.read_bytes();before=read(p);after=copy.deepcopy(before)
old=next(x for x in before['prioritizedRemainingScopes'] if x['id']=='R1');item=next(x for x in after['prioritizedRemainingScopes'] if x['id']=='R1')
assert 'functionBodyArgumentCaptureDevelopment' not in item
item['missingBehavior'] += ' Context-sensitive arguments now retain source-derived callable types through a fixed pure rest collector. Original argument lists run once before defaults, rest and pattern initialization; only own dense raw-array positions are read, never an iterator. Inert original/signature carriers, bounded original JSDoc parameter/template pieces and closed conditional tuple syntax preserve callback/property/tuple contexts and generic function-value literals. Simple literal/identifier arguments retain direct fresh bindings to preserve existing generic scalar inference. Native independently reconstructs both recipes and checks source copies, names, bindings and surviving caller/argument types. Mixed generic/expected-result contexts, TS generic frames, other parameter/annotation forms and wider invocation/expression/declaration/language/consumer semantics remain open.'
item['suggestedScope']='Continue mixed generic argument/result contexts, TypeScript generic frames, predicate/parameter/annotation and diagnostic combinations; wider invocation/expression/declaration/language/consumer forms, import attributes and sibling bundles. Preserve original byte/binding/type correspondence, default/body environments, graph/plan/session identity and shared budgets with independent verification/application/recovery. Qualify full Windows/current-head hosted/upstream acceptance separately.'
item['functionBodyArgumentCaptureDevelopment']={'implemented':True,'fullContractCertified':False,'compiler':'TypeScript 5.8.3','newIntegrationFunctions':1,'newUnitFunctions':2,'nativePositiveScenarios':20,'bodyCliGateScenarios':15,'newCliGateScenarios':3,'argumentRecipes':['direct_original_bindings','source_typed_pure_rest_collector'],'argumentCaptureNamesPerFrame':7,'documentationTypeAuthority':'host TypeScript enumerates bounded original comment pieces; native checks source bounds and reconstruction','guardPhase':'guard-r2','guardUnitTotals':next(x for x in guard['commands'] if x['name']=='units')['totals'],'guardIntegrationTotals':next(x for x in guard['commands'] if x['name']=='native')['totals'],'guardGroups':8,'nativeFrozenInputs':len(native),'retainedPhase':'retained-r2','retainedFrozenInputs':len(retained),'countsAreNonadditive':True,'wholeNativeRerun':False,'platformEvidence':'Windows','otherPlatformsCertified':False,'evidence':['native/src/compiler_inline_body_arguments.rs','native/src/compiler_inline_body_arguments_worker.cjs','native/src/compiler_inline_body_admission_tests.rs','native/tests/compiler_inline_body.rs','docs/validation/integration-2026-10-19/function-body-argument-contexts/index.json'],'remaining':['Caller JSDoc annotation ownership and expected-result argument contexts; TypeScript generic frames','Other predicate/parameter/annotation and wider invocation/expression/declaration/language/consumer semantics','Full STATUS/R1-R9 and Windows/current-head hosted/upstream acceptance']}
restored=copy.deepcopy(after);r=next(x for x in restored['prioritizedRemainingScopes'] if x['id']=='R1')
for key in ['missingBehavior','suggestedScope']:r[key]=old[key]
del r['functionBodyArgumentCaptureDevelopment'];assert restored==before;assert list(old)==[k for k in item if k!='functionBodyArgumentCaptureDevelopment']
(out/'tracker-before.json').write_bytes(original);p.write_text(json.dumps(after,indent=2,ensure_ascii=False)+'\n',encoding='utf-8',newline='\n')
summary="""Context-sensitive arguments now keep original callable typing through a fixed
pure rest collector. The original argument list executes once before parameter
defaults, rest/pattern initialization and the original body. Fresh raw arrays are
read only at their own dense indices, preserving behavior when Array iterators
are replaced. Inert original/signature carriers provide source-derived types;
original JSDoc parameter/template pieces have bounded native byte checks and
compiler free-binding checks. Simple literal/identifier arguments retain direct
fresh bindings and existing generic scalar inference. Both finite recipes have
independent native reconstruction and unchanged caller/argument type comparisons.
Preparation executes no analyzed source. Actual parameter/body environments,
finally behavior and ordinary verification/approval/application/recovery remain.

One added integration function covers twenty owned positive cases: callback,
property, tuple, rest and pattern contexts; generic function-value literal types;
anonymous arrow/function/class names; never-typed argument values; default order;
hostile iterators and scalar/var compatibility. Three additional CLI cases pass
independent tsc, approval, application and foreign-preserving recovery. Two added
units and extended actual-host tests reject forged documentation/source ranges,
UTF-8 boundaries, names, recipes, raw destructuring and extra executable code.

The scoped regression passes **116 integrations / 0 failures / 2 existing ignored**
across eight targets and fourteen body units on 811 unchanged native inputs.
Format/build/all-target Clippy pass. Python 17, typecheck and TypeScript 260 / 3
existing skips pass on 805 unchanged retained inputs. Counts overlap. Failed
development evidence retains a stricter old var assertion over an inert clone,
exact owned native/runtime controls, and generic scalar widening (3 to number).
The corrected direct scalar recipe preserves the old var test without weakening
it. Synthetic worker/signature probes are distinct from native admission.

Caller JSDoc annotation ownership, mixed generic and expected-result contexts,
TypeScript generic frames, other
parameter/annotation and broader R1-R9 work, complete Windows and current-head
hosted/upstream acceptance remain open. See the
[argument evidence](validation/integration-2026-10-19/function-body-argument-contexts/README.md).
The following return-context results belong to the preceding source checkpoint.

"""
changed_docs=[]
def update_doc(name,change):
 p=w/name;b=p.read_bytes();s=b.decode('utf-8');nl='\r\n' if b.count(b'\r\n')==b.count(b'\n') and b.count(b'\n') else '\n';s=s.replace('\r\n','\n');result=change(s);assert result!=s,name
 target=out/'before'/name;target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(b);p.write_bytes(result.replace('\n',nl).encode('utf-8'));changed_docs.append({'path':name,'beforeSha256':sha(b),'afterSha256':sha(p.read_bytes())})
for name in ['docs/STATUS.md','docs/REMAINING_WORK.md']:
 update_doc(name,lambda s:s.replace('The return-context extension preserves original TypeScript return annotations as',summary+'The return-context extension preserves original TypeScript return annotations as',1))
update_doc('docs/ARCHITECTURE.md',lambda s:s.replace('Original TypeScript return annotations also contextually type result storage, with','Context-sensitive arguments use a fixed pure rest collector with an inert\nsource-derived callable signature. Original arguments run once before existing\nparameter/default/body lowering; raw dense-array indexing avoids iterator hooks.\nBounded original documentation pieces retain compiler type authority and native\nbyte/binding checks. Simple literal/identifier arguments keep direct bindings and\ngeneric scalar inference. Both recipes retain exact surviving caller/argument\ntypes and independent application gates. Original TypeScript return annotations\nalso contextually type result storage, with',1))
assert all(sha((w/n).read_bytes())==h for n,h in native.items());assert all(sha((w/n).read_bytes())==h for n,h in retained.items() if n!=tracker)
proof={'baseCommit':subprocess.check_output(['git','rev-parse','HEAD'],cwd=w).decode().strip(),'changedR1Fields':['missingBehavior','suggestedScope','functionBodyArgumentCaptureDevelopment'],'allOtherDecodedDataPreserved':True,'existingKeyOrderPreserved':True,'trackerBeforeSha256':sha(original),'trackerAfterSha256':sha(p.read_bytes()),'runtimeCodeChangedAfterFinalChecks':False,'nativeInputsExact':len(native),'otherRetainedInputsExact':len(retained)-1,'documents':changed_docs}
(out/'proof.json').write_text(json.dumps(proof,indent=2)+'\n',encoding='utf-8');shutil.copyfile(__file__,out/'runner.py');print(json.dumps(proof))
