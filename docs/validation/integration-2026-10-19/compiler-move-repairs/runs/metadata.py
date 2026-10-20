from pathlib import Path
import copy,json,hashlib,subprocess,re,shutil
work=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');raw=work.parent/'Yoke-CodeIntelligence/validation/integration-20261018/compiler-move-repairs'
read=lambda p:json.loads(p.read_text(encoding='utf-8-sig'));sha=lambda b:hashlib.sha256(b).hexdigest()
final=read(raw/'final-r1/result.json');assert final['allChecksPassed'] and final['sourceUnchanged'];totals=final['commands'][0]['totals'];groups=final['commands'][0]['groups']
tracker=work/'docs/parity/native-reconciliation-2026-10-15.json';before=read(tracker);assert before==json.loads(subprocess.check_output(['git','show','99febf08db9016f4382c2425c80eedc6dcdd8630:docs/parity/native-reconciliation-2026-10-15.json'],cwd=work));j=copy.deepcopy(before)
def r1(x):
 if isinstance(x,dict):
  if x.get('id')=='R1':return x
  for v in x.values():
   result=r1(v)
   if result is not None:return result
 elif isinstance(x,list):
  for v in x:
   result=r1(v)
   if result is not None:return result
row=r1(j);original=copy.deepcopy(row)
old='Selected-symbol reexports and import-alias collisions that the compiler leaves invalid refuse before publication; independent repairs, '
assert old in row['missingBehavior'];row['missingBehavior']=row['missingBehavior'].replace(old,'Direct named selected-symbol reexports and generated import-alias collisions now have independently reconstructed source-only repairs, original-byte piece composition and final binding/API/diagnostic checks. Diagnostic-free omitted shorthand dependencies still refuse; missing-import synthesis, ',1)
row['suggestedScope']='Continue missing-import synthesis for shorthand-only dependencies, classic function-body inline and remaining reexport/import/destination/context/language/provider shapes. Preserve the checked compiler-move repair pipeline, original graph/plan/session identities, shared budgets and independent verification/application/recovery; qualify dynamic consumers and full Windows/current-head hosted/upstream behavior separately.'
row['compilerMoveRepairsDevelopment']={'implemented':True,'fullContractCertified':False,'hostOptIn':'moveCompiler','compiler':'TypeScript 5.8.3','newIntegrationCasesPassed':6,'newUnitCasesPassed':2,'newCasesPassed':8,'updatedExistingIntegrationCases':['actual_compiler_selected_reexports_and_import_alias_collisions_are_repaired','actual_cli_move_requires_independent_tsc_approval_and_conditional_recovery'],'importFormScenarios':4,'reexportStyleScenarios':3,'cliGateScenarios':2,'utf8CompositionSequences':128,'editsPerSequence':8,'nativeTotals':totals,'nativeGroups':groups,'nativeFrozenInputs':final['inputs'],'wholeNativeRerun':False,'platformEvidence':'Windows','otherPlatformsCertified':False,'evidence':['native/src/compiler_move_repairs.rs','native/src/compiler_move_repairs_worker.cjs','native/src/compiler_move_repair_tests.rs','native/tests/compiler_move.rs','native/docs/compiler-move.md','docs/validation/integration-2026-10-19/compiler-move-repairs/index.json'],'remaining':['Missing import synthesis for shorthand-only dependencies','Classic function-body inline','Broader reexport/import/context/destination/language shapes','Dynamic consumers, evaluation order and observed execution','Full Windows/current-head hosted/upstream acceptance']}
rest=copy.deepcopy(j);r=r1(rest)
for key in ['missingBehavior','suggestedScope']:r[key]=original[key]
r.pop('compilerMoveRepairsDevelopment');assert rest==before
tracker.write_text(json.dumps(j,indent=2,ensure_ascii=False)+'\n',encoding='utf-8',newline='\n')
(raw/'metadata-proof.json').write_text(json.dumps({'onlyR1DataFieldsChanged':['missingBehavior','suggestedScope','compilerMoveRepairsDevelopment'],'allOtherDecodedDataUnchanged':True,'existingKeyOrderPreserved':True,'trackerSha256':sha(tracker.read_bytes())},indent=2)+'\n',encoding='utf-8',newline='\n')
paragraph=f'''Explicit [compiler move repairs](../native/docs/compiler-move.md#source-only-compiler-repairs)
now split direct named reexports and give generated imports fresh local aliases
without renaming original target/dependency declarations. Native original/current
source-anchor checks, captured free-binding checks and original-byte piece
composition precede the existing complete-result syntax/binding/API gates.
Shorthand property keys retain their independent definition anchors.

All **eight additional Rust cases pass** (six integration / two unit); two
existing integration cases now also exercise repaired outputs. Coverage includes
four import forms, three reexport styles, 128 UTF-8 composition sequences with
eight edits each, coherent forged effects, a shared operation budget and actual
CLI plus independent tsc/approval/apply/conditional recovery for both original
and repaired plans. The final scoped native run is **{totals['passed']} passed /
{totals['failed']} failed / {totals['ignored']} existing ignored**, across {groups} groups on {final['inputs']} fixed
native inputs. Format/build/all-target Clippy pass. Counts include the new cases;
development phases are not added to these totals.

Missing shorthand-only dependency imports still refuse when native bindings
detect a diagnostic-free substitution. Their synthesis, classic body inline,
wider shapes/runtime/upstream acceptance and full Windows/current-head hosted
acceptance remain open. Existing OS1314 and macOS PDF/runtime/timing failures
remain unresolved. See [repair evidence](validation/integration-2026-10-19/compiler-move-repairs/README.md).

'''
p=work/'docs/STATUS.md';s=p.read_text(encoding='utf-8');marker='## Current integration work\n\n';assert marker in s;p.write_text(s.replace(marker,marker+paragraph,1),encoding='utf-8',newline='\n')
p=work/'docs/REMAINING_WORK.md';s=p.read_text(encoding='utf-8');s=s.replace('selected-symbol reexport/import-alias collision repair,','shorthand-only dependency import synthesis and remaining reexport/import shapes,',1);s=s.replace('moves into existing captured ES modules with compiler-bound consumers','moves into existing captured ES modules with checked direct reexport/local-alias repairs, compiler-bound consumers',1);marker='## Acceptance still required\n\n';assert marker in s;p.write_text(s.replace(marker,marker+paragraph,1),encoding='utf-8',newline='\n')
p=Path.home()/'AppData/Local/Temp/ci-compiler-inline-retained.py';s=p.read_text(encoding='utf-8-sig').replace('compiler-inline/retained-r1','compiler-move-repairs/retained-r1').replace('COMPILER INLINE','COMPILER MOVE REPAIRS');p.with_name('ci-compiler-move-repairs-retained.py').write_text(s,encoding='utf-8',newline='\n');shutil.copyfile(__file__,raw/'metadata.py')
print(json.dumps({'nativeTotals':totals,'groups':groups,'nativeFrozenInputs':final['inputs'],'newCases':8,'updatedCases':2}))
