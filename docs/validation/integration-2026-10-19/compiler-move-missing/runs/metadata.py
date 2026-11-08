from pathlib import Path
import json,subprocess,copy,sys,shutil
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');raw=w.parent/'Yoke-CodeIntelligence/validation/integration-20261018/compiler-move-missing';phase=sys.argv[1]
read=lambda p:json.loads(p.read_text(encoding='utf-8-sig'))
r=read(raw/phase/'result.json');assert r['allChecksPassed'] and r['sourceUnchanged'];native=r['commands'][0];t=native['totals'];assert t['failed']==0
p=w/'docs/parity/native-reconciliation-2026-10-15.json';before=read(p);after=copy.deepcopy(before)
def find(v):
 if isinstance(v,dict):
  if v.get('id')=='R1':return v
  for child in v.values():
   hit=find(child)
   if hit is not None:return hit
 if isinstance(v,list):
  for child in v:
   hit=find(child)
   if hit is not None:return hit
old=find(before);item=find(after);assert item and 'compilerMoveMissingImportsDevelopment' not in item
needle='Diagnostic-free omitted shorthand dependencies still refuse; missing-import synthesis,';assert needle in item['missingBehavior']
item['missingBehavior']=item['missingBehavior'].replace(needle,'Captured omitted import and source-owned free dependencies now have independently reconstructed import/export repairs with exact relative module linkage and prototype-safe shorthand keys. Unsupported import attributes, sibling declaration bundles,')
item['suggestedScope']='Continue classic function-body inline and remaining import attributes, sibling declaration bundles, export/import/destination/context/language/provider shapes. Preserve native compiler-move reconstruction, property semantics, original graph/plan/session identities, shared budgets and independent verification/application/recovery; qualify dynamic consumers and full Windows/current-head hosted/upstream behavior separately.'
item['compilerMoveMissingImportsDevelopment']={'implemented':True,'fullContractCertified':False,'compiler':'TypeScript 5.8.3','hostOptIn':'moveCompiler','newIntegrationCasesPassed':6,'updatedExistingIntegrationCases':['omitted_shorthand_import_is_reconstructed_without_rebinding_the_target','actual_cli_move_requires_independent_tsc_approval_and_conditional_recovery'],'updatedExistingUnitCases':2,'importScenarios':12,'localDependencyScenarios':12,'cliGateScenarios':3,'prototypeRuntimeOracleCases':3,'nativeTotals':t,'nativeGroups':native['groups'],'nativeFrozenInputs':r['inputs'],'wholeNativeRerun':False,'platformEvidence':'Windows','otherPlatformsCertified':False,'evidence':['native/src/compiler_move_dependency_import.rs','native/src/compiler_move_repairs.rs','native/src/compiler_move_repairs_worker.cjs','native/src/compiler_move_ast.rs','native/tests/compiler_move.rs','docs/validation/integration-2026-10-19/compiler-move-missing/index.json'],'remaining':['Classic function-body inline','Unsupported import attributes and sibling declaration bundles','Broader reexport/import/context/destination/language shapes','Dynamic consumers, evaluation order and general runtime equivalence','Full Windows/current-head hosted/upstream acceptance']}
assert list(old)==[k for k in item if k!='compilerMoveMissingImportsDevelopment']
proof=copy.deepcopy(after);proof_item=find(proof)
for k in ['missingBehavior','suggestedScope']:proof_item[k]=old[k]
del proof_item['compilerMoveMissingImportsDevelopment'];assert proof==before
p.write_text(json.dumps(after,indent=2,ensure_ascii=False)+'\n',encoding='utf-8',newline='\n')
(raw/'metadata-proof.json').write_text(json.dumps({'baseCommit':subprocess.check_output(['git','rev-parse','HEAD'],cwd=w).decode().strip(),'changedR1Fields':['missingBehavior','suggestedScope','compilerMoveMissingImportsDevelopment'],'allOtherDecodedDataPreserved':True,'existingKeyOrderPreserved':True},indent=2)+'\n',encoding='utf-8',newline='\n')
text=f'''Captured missing compiler-move imports now reconstruct original named/default/
namespace bindings and source-owned top-level free dependencies. Native checks
bind original/current definitions and references, exact relative module targets
and declaration-boundary import/export insertions before complete-result
syntax/binding/API and ordinary guarded-plan gates. Original target declarations
retain their names. Shorthand `__proto__` uses a computed own-property key;
prototype-setter substitutions have a distinct native syntax signature.

Six additional integration cases pass, with twelve imported-binding scenarios,
twelve private/public/default/aliased source-dependency scenarios, combined repairs,
BOM/CRLF/directive/path coverage and three explicit owned JavaScript property
oracles. Two existing integrations and two unit cases now cover omitted imports,
three actual CLI/independent tsc/approval/apply/conditional recovery scenarios and
coherent forged origins, module paths, source exports, references and budgets.
The final scoped native run is **{t['passed']} passed / {t['failed']} failed /
{t['ignored']} existing ignored**, across {native['groups']} groups on {r['inputs']} fixed native inputs.
Format/build/all-target Clippy pass. Development phases are not added to totals.

Classic body inline, unsupported attributes and sibling bundles, wider shapes,
runtime/upstream qualification and full Windows/current-head hosted acceptance
remain open. Existing OS1314 and macOS PDF/runtime/timing failures remain
unresolved. See [missing-import evidence](validation/integration-2026-10-19/compiler-move-missing/README.md).

'''
for path,marker in [('docs/STATUS.md','## Current integration work\n\n'),('docs/REMAINING_WORK.md','## Acceptance still required\n\n')]:
 p=w/path;s=p.read_text(encoding='utf-8');assert marker in s;s=s.replace(marker,marker+text,1)
 s=s.replace('Missing shorthand-only dependency imports still refuse when native bindings\ndetect a diagnostic-free substitution. Their synthesis, classic body inline,','At the earlier repair checkpoint, missing shorthand-only imports still refused.\nThe missing-import checkpoint above supersedes that limitation for captured\nimports and supported source declarations. Classic body inline,')
 if path.endswith('REMAINING_WORK.md'):
  s=s.replace('with checked direct reexport/local-alias repairs,','with checked direct reexport/local-alias repairs, captured omitted imports and source dependency exports,')
  s=s.replace('Classic function-body inline, shorthand-only dependency import synthesis and remaining reexport/import shapes,','Classic function-body inline, unsupported import attributes and sibling declaration bundles, remaining reexport/import shapes,')
 p.write_text(s,encoding='utf-8',newline='\n')
p=w/'docs/ARCHITECTURE.md';s=p.read_text(encoding='utf-8');s=s.replace('source/binding/API/verification gates; diagnostic-free missing-import substitutions\nremain refused.','source/binding/API/verification gates. Captured omitted imports and source-owned\nfree dependencies have independent origin, module and insertion checks. Required\nsource exports are disclosed; computed `__proto__` keys preserve own-property\nsemantics. Unsupported substitutions still refuse.')
s=s.replace('invalid compiler reexports/import collisions refuse.','unsupported compiler effects refuse.')
p.write_text(s,encoding='utf-8',newline='\n');shutil.copyfile(__file__,raw/'metadata.py')
print(json.dumps({'nativeTotals':t,'groups':native['groups'],'inputs':r['inputs'],'trackerOutsideR1Preserved':True}))
