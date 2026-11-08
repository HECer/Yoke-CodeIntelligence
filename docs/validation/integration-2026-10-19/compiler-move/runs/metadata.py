from pathlib import Path
import json,subprocess,copy,hashlib,re,sys
work=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');raw=work.parent/'Yoke-CodeIntelligence/validation/integration-20261018/compiler-move'
read=lambda p:json.loads(p.read_text(encoding='utf-8-sig'))
sha=lambda b:hashlib.sha256(b).hexdigest()
final=read(raw/'final-r2/result.json');assert final['allChecksPassed'] and final['sourceUnchanged']
library=read(raw/'final-r2/result.json');assert library['allChecksPassed'] and library['sourceUnchanged']
old=read(raw/'final-r2/source-after.json');current=read(raw/'final-r2/source-after.json')
fixed={p:h for p,h in old.items() if p.startswith('native/src/') or p in ['Cargo.toml','Cargo.lock','native/Cargo.toml']}
assert all(current[p]==h for p,h in fixed.items())
lib_groups=re.findall(r'^test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', (raw/'final-r2/native.log').read_text(encoding='utf-8'),re.M)
lib={k:int(lib_groups[0][i]) for i,k in enumerate(['passed','failed','ignored'])};totals=final['commands'][0]['totals'];integration={k:totals[k]-lib[k] for k in lib}
proof={'libraryPhase':'final-r2','libraryTotals':lib,'finalPhase':'final-r2','integrationTotals':integration,'combinedNonoverlappingTotals':totals,'combinedGroups':final['commands'][0]['groups'],'sameFinalInvocation':True,'nativeInputs':final['inputs'],'libraryRuntimeInputHashesMatched':len(fixed),'sourceHashes':fixed,'wholeNativeRerun':False,'allChecksPassed':True}
(raw/'combined-checks.json').write_text(json.dumps(proof,indent=2)+'\n',encoding='utf-8')
stdlib=read(raw/'live-stdlib-cli/result.json');assert stdlib['allChecksPassed'] and stdlib['sourceUnchanged'] and stdlib['frozenRuntimeInputsUnchanged']
tracker=work/'docs/parity/native-reconciliation-2026-10-15.json';before=read(tracker);j=copy.deepcopy(before)
def r1(x):
 if isinstance(x,dict):
  if x.get('id')=='R1':return x
  for v in x.values():
   found=r1(v)
   if found is not None:return found
 elif isinstance(x,list):
  for v in x:
   found=r1(v)
   if found is not None:return found
row=r1(j);original=copy.deepcopy(row)
row['missingBehavior'] += ' Explicit moveCompiler now admits actual immutable TS5.8.3 moves of single top-level TS/TSX/JS/JSX declarations into existing captured ES modules, with native source/binding/import-export/result checks, ordered insertion/BOM normalization and guarded independent tsc/application/recovery. Graph-absent scalar variables/namespaces retain explicit AST origin and null entityId. Selected-symbol reexports and import-alias collisions that the compiler leaves invalid refuse before publication; independent repairs, new resources, symbol-parent destinations, wider context/language/consumer semantics and full acceptance remain open.'
row['suggestedScope']='Continue classic function-body inline, independent repair of compiler-selected-symbol reexports and import-alias collisions, and remaining move/declaration/language/provider contracts with exact source/binding/effect checks and independent verification/application/recovery. Preserve compiler inline/move and other-language LSP routes, complete=false limits and existing graph/plan/session identities; qualify wider consumer and runtime behavior separately.'
row['compilerMoveDevelopment']={'implemented':True,'fullContractCertified':False,'hostOptIn':'moveCompiler','selectedCompiler':'TypeScript 5.8.3','newIntegrationCasesPassed':13,'newUnitCasesPassed':3,'newCasesPassed':16,'declarationEpochScenarios':14,'encodingLanguageScenarios':4,'jsxComponentScenarios':2,'nativeTotals':totals,'nativeGroups':proof['combinedGroups'],'nativeFrozenInputs':final['inputs'],'libraryRuntimeInputHashesMatched':len(fixed),'wholeNativeRerun':False,'platformEvidence':'Windows','otherPlatformsCertified':False,'evidence':['native/src/compiler_move.rs','native/src/compiler_move_ast.rs','native/src/compiler_move_bindings.rs','native/src/compiler_move_worker.cjs','native/src/compiler_move_tests.rs','native/tests/compiler_move.rs','native/docs/compiler-move.md','docs/validation/integration-2026-10-19/compiler-move/index.json'],'remaining':['Classic function-body inline','Independent selected-symbol reexport and import-alias collision repair','Other destinations, contexts, languages and consumer shapes','Dynamic/omitted consumers, module evaluation order and observed execution','Full Windows and current-head hosted/upstream acceptance']}
row['compilerMoveDevelopment']['additionalStdlibCliProbePassed']=True
rest=copy.deepcopy(j);r=r1(rest)
for key in ['missingBehavior','suggestedScope']:r[key]=original[key]
r.pop('compilerMoveDevelopment');assert rest==before
tracker.write_text(json.dumps(j,indent=2,ensure_ascii=False)+'\n',encoding='utf-8')
(raw/'metadata-proof.json').write_text(json.dumps({'onlyR1DataFieldsChanged':['missingBehavior','suggestedScope','compilerMoveDevelopment'],'allOtherDecodedDataUnchanged':True,'existingObjectKeyOrderPreserved':True,'trackerSha256':sha(tracker.read_bytes())},indent=2)+'\n',encoding='utf-8')
paragraph=f'''Explicit [immutable compiler moves](../native/docs/compiler-move.md) add actual
host-selected TypeScript 5.8.3 declaration moves into existing captured ES modules.
Native syntax/binding/import-export/result checks, explicit graph-absent AST
selection and original BOM normalization precede ordinary guarded plans.
All **16 new cases pass** (13 integration / three unit), including fourteen
analyzer/declaration scenarios, four encoding/language scenarios, two JSX
component scenarios, inert repository plugins and actual CLI plus independent
`tsc`, approval/application and conditional recovery. An additional CLI Array/Promise standard-library move and independent tsc gate
pass separately from these Rust test counts. Invalid compiler reexports
and import-alias collisions refuse; their independent repair remains open.

The final scoped native invocation is **{totals['passed']} passed / {totals['failed']} failed /
{totals['ignored']} ignored**, across {proof['combinedGroups']} groups, including the library and selected
integrations. Format, build and all-target Clippy pass on {final['inputs']} fixed native
inputs. Existing ignored entries include separately qualified large-capacity,
Windows path and live/parent-worker fixtures; they are not counted as passed.
This is scoped Windows development evidence, not a whole-native or other-platform
rerun.
See [compiler move evidence](validation/integration-2026-10-19/compiler-move/README.md).
Classic body inline, broader move/consumer/runtime contracts, full Windows and
current-head hosted acceptance remain open. The existing Windows symlink-privilege
gate and separately identified macOS PDF/runtime/timing failures remain unresolved.

'''
p=work/'docs/STATUS.md';s=p.read_text(encoding='utf-8');marker='## Current integration work\n\n';assert marker in s;s=s.replace(marker,marker+paragraph,1);p.write_text(s,encoding='utf-8')
p=work/'docs/REMAINING_WORK.md';s=p.read_text(encoding='utf-8');s=s.replace('actual host-selected immutable TS5.8.3 function-binding inlining with compiler-bound consumers and independent source/effect/apply/recovery checks.', 'actual host-selected immutable TS5.8.3 function-binding inlining and moves into existing captured ES modules with compiler-bound consumers and independent source/effect/apply/recovery checks.',1);s=s.replace('Classic function-body inline, broader moves/declaration/file/language/context shapes,','Classic function-body inline, selected-symbol reexport/import-alias collision repair, broader moves/declaration/file/language/context shapes,',1);marker='## Acceptance still required\n\n';assert marker in s;s=s.replace(marker,marker+paragraph,1);p.write_text(s,encoding='utf-8')
retained=Path.home()/'AppData/Local/Temp/ci-compiler-inline-retained.py';s=retained.read_text(encoding='utf-8');s=s.replace('compiler-inline/retained-r1','compiler-move/retained-r1').replace('COMPILER INLINE','COMPILER MOVE');q=retained.with_name('ci-compiler-move-retained.py');q.write_text(s,encoding='utf-8')
(raw/'metadata.py').write_bytes(Path(__file__).read_bytes())
print(json.dumps({k:v for k,v in proof.items() if k!='sourceHashes'}))
