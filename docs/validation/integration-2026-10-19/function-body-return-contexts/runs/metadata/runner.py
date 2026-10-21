from pathlib import Path
import copy,hashlib,json,subprocess,shutil
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');raw=w.parent/'Yoke-CodeIntelligence/validation/integration-20261018/function-body-return-contexts'
read=lambda p:json.loads(p.read_text(encoding='utf-8'));sha=lambda b:hashlib.sha256(b).hexdigest()
guard=read(raw/'guard-r2/result.json');ret=read(raw/'retained-r6/result.json')
assert all(r['allChecksPassed'] and r['sourceUnchanged'] for r in [guard,ret])
assert next(x for x in guard['commands'] if x['name']=='native')['totals']=={'passed':115,'failed':0,'ignored':2}
assert next(x for x in guard['commands'] if x['name']=='units')['totals']=={'passed':12,'failed':0,'ignored':0}
native=read(raw/'guard-r2/source-after.json');retained=read(raw/'retained-r6/source-after.json');assert len(native)==809 and len(retained)==803
assert all(sha((w/n).read_bytes())==h for n,h in native.items());assert all(sha((w/n).read_bytes())==h for n,h in retained.items())
out=raw/'metadata';out.mkdir(exist_ok=False)
tracker='docs/parity/native-reconciliation-2026-10-15.json';p=w/tracker;original=p.read_bytes();before=read(p);after=copy.deepcopy(before)
old=next(x for x in before['prioritizedRemainingScopes'] if x['id']=='R1');item=next(x for x in after['prioritizedRemainingScopes'] if x['id']=='R1')
assert 'functionBodyReturnContextDevelopment' not in item
item['missingBehavior'] += ' Source TypeScript return annotations additionally provide contextual typing for result storage with a fixed undefined alternative. Fixed void-comma barriers retain the original runtime names of anonymous returned arrows, functions and classes. Type shadows still refuse, and native original-byte reconstruction covers both constructs. Further predicates, parameter/annotation contexts and wider expression/invocation/declaration/language/consumer behavior remain open.'
item['suggestedScope']='Continue remaining predicates, argument/parameter/annotation contexts, diagnostic combinations and wider body/expression/invocation forms, remaining import attributes and sibling declaration bundles, export/import/destination/context/language/provider shapes. Preserve original byte/binding/type correspondence, legacy variable-inline and graph/plan/session identities, shared budgets and independent verification/application/recovery. Qualify dynamic consumers and full Windows/current-head hosted/upstream behavior separately.'
item['functionBodyReturnContextDevelopment']={'implemented':True,'fullContractCertified':False,'compiler':'TypeScript 5.8.3','newIntegrationFunctions':2,'newUnitFunctions':1,'nativePositiveScenarios':31,'consumerTypeShadowRefusalScenarios':1,'bodyCliGateScenarios':12,'newCliGateScenarios':3,'guardPhase':'guard-r2','guardUnitTotals':next(x for x in guard['commands'] if x['name']=='units')['totals'],'guardIntegrationTotals':next(x for x in guard['commands'] if x['name']=='native')['totals'],'guardGroups':8,'nativeFrozenInputs':len(native),'retainedPhase':'retained-r6','retainedFrozenInputs':len(retained),'countsAreNonadditive':True,'wholeNativeRerun':False,'platformEvidence':'Windows','otherPlatformsCertified':False,'evidence':['native/src/compiler_inline_body.rs','native/src/compiler_inline_body_worker.cjs','native/src/compiler_inline_body_admission_tests.rs','native/tests/compiler_inline_body.rs','docs/validation/integration-2026-10-19/function-body-return-contexts/index.json'],'remaining':['Predicate, argument/parameter/annotation contexts and diagnostic combinations','Wider expression/invocation/declaration/language/consumer forms','Full STATUS/R1-R9 and Windows/current-head hosted/upstream acceptance']}
restored=copy.deepcopy(after);r=next(x for x in restored['prioritizedRemainingScopes'] if x['id']=='R1')
for key in ['missingBehavior','suggestedScope']:r[key]=old[key]
del r['functionBodyReturnContextDevelopment'];assert restored==before;assert list(old)==[k for k in item if k!='functionBodyReturnContextDevelopment']
(out/'tracker-before.json').write_bytes(original);p.write_text(json.dumps(after,indent=2,ensure_ascii=False)+'\n',encoding='utf-8',newline='\n')
summary="""The return-context extension preserves original TypeScript return annotations as
contextual types for generated result storage, with a fixed undefined alternative
for normal completion. Function-valued properties, methods, tuples and callbacks
retain their original parameter types. Source identifiers keep the same binding
checks; consumer type shadows refuse. Source-derived void-comma barriers prevent anonymous
returned arrows, function expressions and classes from acquiring synthetic result
variable names. Original named expressions, evaluation order and finally behavior
remain intact. Native reconstruction checks the exact annotation and barrier.

Thirty-one new owned positive scenarios and one type-shadow refusal pass in two added
integration functions, plus nineteen source-wrapper classification cases in one new unit, with
Unicode/CRLF, retained exports and before-definition
calls. Three additional CLI cases pass independent tsc, verification, approval,
application and foreign-preserving recovery. Coherently forged annotations and
removed name barriers refuse despite matching forged result hashes.

The scoped regression passes **115 integrations / 0 failures / 2 existing ignored**
across eight targets and twelve body units on 809 unchanged native inputs.
Format/build/all-target Clippy pass. Python 17, typecheck and TypeScript 260 / 3
existing skips pass on 803 unchanged retained inputs. Counts overlap. The original
owned CLI rejection and the sixteen-case worker red/green comparison are retained:
eight original projected compiler errors and seven runtime name changes become
sixteen exact type/runtime matches. Synthetic probes claim no native admission;
runtime comparisons execute only owned literal fixtures.

Further predicate, argument/parameter/annotation and broader R1-R9 work, full
Windows and current-head hosted/upstream acceptance remain open. See the
[return-context evidence](validation/integration-2026-10-19/function-body-return-contexts/README.md).
The initial 114-pass/one-failure guard caught TS7027 from an overly broad comma
wrapper. The final source classifies anonymous values through erased wrappers and
keeps ordinary never-call flow; four never cases and the 31-case worker probe pass.
The following completion/type results belong to the preceding source checkpoint.

"""
changed_docs=[]
def update_doc(name,change):
 p=w/name;b=p.read_bytes();s=b.decode('utf-8');nl='\r\n' if b.count(b'\r\n')==b.count(b'\n') and b.count(b'\n') else '\n';s=s.replace('\r\n','\n');result=change(s);assert result!=s,name
 target=out/'before'/name;target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(b);p.write_bytes(result.replace('\n',nl).encode('utf-8'));changed_docs.append({'path':name,'beforeSha256':sha(b),'afterSha256':sha(p.read_bytes())})
for name in ['docs/STATUS.md','docs/REMAINING_WORK.md']:
 update_doc(name,lambda s:s.replace('Classic function-body expansion under',summary+'Classic function-body expansion under',1))
update_doc('docs/ARCHITECTURE.md',lambda s:s.replace('Original/fixed result assertions use a fixed unknown intermediate. Native','Original TypeScript return annotations also contextually type result storage, with\na fixed undefined alternative for normal completion. Fixed void-comma return\nbarriers retain anonymous function/class runtime names. Exact source annotation\nand barrier reconstruction precede binding/type checks. Original/fixed result\nassertions use a fixed unknown intermediate. Native',1))
assert all(sha((w/n).read_bytes())==h for n,h in native.items());assert all(sha((w/n).read_bytes())==h for n,h in retained.items() if n!=tracker)
proof={'baseCommit':subprocess.check_output(['git','rev-parse','HEAD'],cwd=w).decode().strip(),'changedR1Fields':['missingBehavior','suggestedScope','functionBodyReturnContextDevelopment'],'allOtherDecodedDataPreserved':True,'existingKeyOrderPreserved':True,'trackerBeforeSha256':sha(original),'trackerAfterSha256':sha(p.read_bytes()),'runtimeCodeChangedAfterFinalChecks':False,'nativeInputsExact':len(native),'otherRetainedInputsExact':len(retained)-1,'documents':changed_docs}
(out/'proof.json').write_text(json.dumps(proof,indent=2)+'\n',encoding='utf-8');shutil.copyfile(__file__,out/'runner.py');print(json.dumps(proof))
