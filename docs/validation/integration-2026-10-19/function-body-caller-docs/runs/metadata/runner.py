from pathlib import Path
import copy,hashlib,json,subprocess,shutil
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');raw=w.parent/'Yoke-CodeIntelligence/validation/integration-20261018/function-body-caller-docs'
read=lambda p:json.loads(p.read_text(encoding='utf-8'));sha=lambda b:hashlib.sha256(b).hexdigest()
g=read(raw/'guard-r2/result.json');r=read(raw/'retained-r2/result.json')
assert all(x['allChecksPassed'] and x['sourceUnchanged'] for x in [g,r])
assert next(x['totals'] for x in g['commands'] if x['name']=='native')=={'passed':117,'failed':0,'ignored':2}
assert next(x['totals'] for x in g['commands'] if x['name']=='units')=={'passed':16,'failed':0,'ignored':0}
native=read(raw/'guard-r2/source-after.json');ret=read(raw/'retained-r2/source-after.json');assert len(native)==811 and len(ret)==805
assert all(sha((w/n).read_bytes())==h for n,h in native.items());assert all(sha((w/n).read_bytes())==h for n,h in ret.items())
out=raw/'metadata';out.mkdir(exist_ok=False)
tracker='docs/parity/native-reconciliation-2026-10-15.json';p=w/tracker;original=p.read_bytes();before=read(p);after=copy.deepcopy(before)
old=next(x for x in before['prioritizedRemainingScopes'] if x['id']=='R1');item=next(x for x in after['prioritizedRemainingScopes'] if x['id']=='R1')
field='functionBodyCallerDocumentationDevelopment';assert field not in item
item['missingBehavior']+=' Attached caller JSDoc now moves once with its original lexical declaration after generated statements; native derives edit extents from original syntax, retaining exact comment/trivia bytes and typedef identity in the same scope. Source-derived callee function tags stay in one contract comment so template binders cover copied parameters and returns. The caller @type{3}/generic-factory counterexample is fixed; broader generic/expected-result contexts still require qualification.'
item['suggestedScope']='Continue TypeScript generic frames, broader generic/expected-result/predicate/parameter/annotation and diagnostic contexts, wider invocation/expression/declaration/language/consumer forms, import attributes and sibling bundles. Preserve original byte/binding/type correspondence, environments, graph/plan/session identity and budgets with independent application/recovery. Complete full Windows/current-head hosted/upstream acceptance separately.'
item[field]={'implemented':True,'fullContractCertified':False,'compiler':'TypeScript 5.8.3','newIntegrationFunctions':1,'nativePositiveScenarios':13,'newUnitFunctions':2,'bodyCliGateScenarios':17,'newCliGateScenarios':2,'callerEditExtent':'derived independently from original syntax; no provider override','copiedFunctionTagContract':'one comment preserves template/parameter/return binder','guardPhase':'guard-r2','guardUnitTotals':next(x['totals'] for x in g['commands'] if x['name']=='units'),'guardIntegrationTotals':next(x['totals'] for x in g['commands'] if x['name']=='native'),'guardGroups':8,'nativeFrozenInputs':811,'retainedPhase':'retained-r2','retainedFrozenInputs':805,'countsAreNonadditive':True,'wholeNativeRerun':False,'platformEvidence':'Windows','otherPlatformsCertified':False,'evidence':['native/src/compiler_inline_body.rs','native/src/compiler_inline_body_admission.rs','native/src/compiler_inline_body_admission_tests.rs','native/tests/compiler_inline_body.rs','docs/validation/integration-2026-10-19/function-body-caller-docs/index.json'],'remaining':['Actual TypeScript generic frames and broader expected-result/predicate/parameter/annotation/diagnostic contexts','Wider invocation/expression/declaration/language/consumer forms and full STATUS/R1-R9/Windows/hosted/upstream acceptance']}
restored=copy.deepcopy(after);rr=next(x for x in restored['prioritizedRemainingScopes'] if x['id']=='R1')
for key in ['missingBehavior','suggestedScope']:rr[key]=old[key]
del rr[field];assert restored==before and list(old)==[k for k in item if k!=field]
(out/'tracker-before.json').write_bytes(original);p.write_text(json.dumps(after,indent=2,ensure_ascii=False)+'\n',encoding='utf-8',newline='\n')
summary='''Attached caller JSDoc now stays with its original lexical declaration after the
inserted statements. Native derives the widened edit extent from original syntax;
there is no provider range override. Every removed definition leaves an empty
statement to stop trailing comments from acquiring a new annotation owner.
Comments/trivia move once in the same scope,
retaining typedef identity, exports, UTF-8/CRLF bytes and surviving types. Copied
callee function tags form one contract comment so template, parameter and return
tags keep their common binder. The original caller @type{3}/generic-factory
counterexample and a nested typedef/template counterexample now pass.

One new integration function covers thirteen positive cases. Two new units cover
original attachment/budget rules and coherent forged comment extents/types/output;
two additional CLI cases pass independent tsc, approval, apply and conditional
foreign-preserving recovery (seventeen CLI scenarios total). The scoped regression
passes 117 integrations / 0 failures / 2 existing ignored across eight targets and
sixteen body units on 811 unchanged native inputs; fmt/build/all-target Clippy pass.
Python 17, typecheck and TypeScript 260 / 3 existing skips pass on 805 unchanged
retained inputs. Counts overlap. Preparation executes no analyzed source.

Broader generic/expected-result contexts, actual TypeScript generic frames,
predicate/parameter/annotation/diagnostic and wider invocation/expression/
declaration/language/consumer semantics, other R1-R9 work and complete Windows/
current-head hosted/upstream acceptance remain open. See the
[caller-documentation evidence](validation/integration-2026-10-19/function-body-caller-docs/README.md).
The following argument-context results belong to the preceding source checkpoint.

'''
documents=[]
def update_doc(name,old,new):
 p=w/name;b=p.read_bytes();s=b.decode('utf-8');nl='\r\n' if b.count(b'\r\n')==b.count(b'\n') and b.count(b'\n') else '\n';s=s.replace('\r\n','\n');assert s.count(old)==1,name
 target=out/'before'/name;target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(b);p.write_bytes(s.replace(old,new,1).replace('\n',nl).encode());documents.append({'path':name,'beforeSha256':sha(b),'afterSha256':sha(p.read_bytes())})
anchor='Context-sensitive arguments now keep original callable typing through a fixed'
for name in ['docs/STATUS.md','docs/REMAINING_WORK.md']:update_doc(name,anchor,summary+anchor)
anchor='Context-sensitive arguments use a fixed pure rest collector with an inert'
update_doc('docs/ARCHITECTURE.md',anchor,'Caller JSDoc moves once with its original lexical declaration in the same\nscope, after generated statements; the native parser derives the widened edit\nextent and exact source correspondence. Copied callee function tags stay in one\ncontract comment so template/parameter/return binders remain together. '+anchor)
assert all(sha((w/n).read_bytes())==h for n,h in native.items());assert all(sha((w/n).read_bytes())==h for n,h in ret.items() if n!=tracker)
proof={'baseCommit':subprocess.check_output(['git','rev-parse','HEAD'],cwd=w).decode().strip(),'changedR1Fields':['missingBehavior','suggestedScope',field],'allOtherDecodedDataPreserved':True,'existingKeyOrderPreserved':True,'trackerBeforeSha256':sha(original),'trackerAfterSha256':sha(p.read_bytes()),'runtimeCodeChangedAfterFinalChecks':False,'nativeInputsExact':811,'otherRetainedInputsExact':804,'documents':documents}
(out/'proof.json').write_text(json.dumps(proof,indent=2)+'\n',encoding='utf-8');shutil.copyfile(__file__,out/'runner.py');print(json.dumps(proof))
