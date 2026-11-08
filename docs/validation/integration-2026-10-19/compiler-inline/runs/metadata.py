from pathlib import Path
import json,copy,re,sys,hashlib
work=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');raw=work.parent/'Yoke-CodeIntelligence/validation/integration-20261018/compiler-inline'
read=lambda p:json.loads(p.read_text(encoding='utf-8-sig'))
final=read(raw/'final-r2/result.json');assert final['allChecksPassed'] and final['sourceUnchanged']
native=final['commands'][0];tot=native['totals'];counts=f"{tot['passed']} passed / {tot['failed']} failed / {tot['ignored']} ignored"
if sys.argv[1]=='tracker':
 p=work/'docs/parity/native-reconciliation-2026-10-15.json';before=read(p);after=copy.deepcopy(before);row=after['prioritizedRemainingScopes'][0];assert row['id']=='R1'
 row['missingBehavior']+=' Explicit host inlineCompiler now prepares actual TypeScript5.8.3 refactor.inline.variable edits for captured TS/TSX/JS/JSX function-valued bindings. Exact original declaration/reference spans, compiler free-value bindings, native copied initializer structure, complete statement/comma removal bundles, optional retained definitions and before/after source diagnostics precede one ordinary guarded plan. Classic function declarations, other languages/contexts, outside-file consumers, unowned trivia, broader moves/function-body inlining and runtime/IDE equivalence remain open. Existing LSP own_v1 and absent-profile identities remain unchanged.'
 row['suggestedScope']='Continue classic function-body inline and remaining move/declaration/language/provider contracts with actual selected providers, exact source/binding/effect checks and independent verification/application/recovery. Preserve the implemented immutable TypeScript variable-inline path, other-language LSP routes, complete=false limits and existing identities; qualify wider consumer and runtime behavior separately.'
 row['compilerInlineDevelopment']={'implemented':True,'hostOptIn':'inlineCompiler','selectedCompiler':'TypeScript 5.8.3','platformEvidence':'Windows','newIntegrationCasesPassed':16,'newUnitCasesPassed':2,'newCasesPassed':18,'languageSiblingScenarios':12,'jsxComponentBindingScenarios':2,'ownedNonTsLspFixtureSeparatelyLabeled':True,'nativeTotals':tot,'nativeGroups':native['groups'],'nativeFrozenInputs':final['inputs'],'formatBuildAllTargetClippyPassed':True,'wholeNativeRerun':False,'fullContractCertified':False,'otherPlatformsCertified':False,'evidence':['native/src/compiler_inline.rs','native/src/compiler_inline_worker.cjs','native/tests/compiler_inline.rs','native/docs/compiler-inline.md','docs/validation/integration-2026-10-19/compiler-inline/index.json'],'remaining':['Classic function declarations and wider function-body inline','Other contexts/languages and outside-file consumers','Broader symbol move contracts','Dynamic/omitted consumers, allocation identity and observed execution','Full Windows and current-head hosted/upstream acceptance']}
 allowed={'missingBehavior','suggestedScope','compilerInlineDevelopment'}
 a=copy.deepcopy(before);b=copy.deepcopy(after)
 for v in [a,b]:
  for key in allowed:v['prioritizedRemainingScopes'][0].pop(key,None)
 assert a==b
 p.write_text(json.dumps(after,indent=2,ensure_ascii=False,sort_keys=True)+'\n',encoding='utf-8')
 proof={'row':'R1','changedFields':sorted(allowed),'allOtherDataExact':a==b,'wholeNativeRerun':False,'fullContractCertified':False}
 (raw/'tracker-reconciliation.json').write_text(json.dumps(proof,indent=2)+'\n',encoding='utf-8');print(json.dumps(proof));sys.exit()
assert sys.argv[1]=='docs'
retained=read(raw/'retained-r1/result.json');assert retained['allChecksPassed'] and retained['sourceUnchanged']
log=(raw/'retained-r1/tests.log').read_text(encoding='utf-8');ts_pass=int(re.findall(r'^(?:#|\u2139) pass (\d+)',log,re.M)[-1]);ts_fail=int(re.findall(r'^(?:#|\u2139) fail (\d+)',log,re.M)[-1]);ts_skip=int(re.findall(r'^(?:#|\u2139) skipped (\d+)',log,re.M)[-1]);assert ts_fail==0
python_log=(raw/'retained-r1/python-contracts.log').read_text(encoding='utf-8');python=int(re.findall(r'Ran (\d+) tests?',python_log)[-1]);assert '\nOK\n' in python_log
paragraph=f'''Explicit [immutable compiler inlining](../native/docs/compiler-inline.md) adds
host-selected TypeScript 5.8.3 function-binding inline plans for TS/TSX/JS/JSX,
including sibling declarations, JSX/component bindings, object shorthand and
retained definitions. Compiler source/binding/diagnostic checks and independent
native initializer/removal/result checks precede ordinary guarded plans. Existing
other-language LSP routes and absent-profile session identities remain intact.

All **18 new cases pass** (16 integration / two unit), including twelve actual
language/sibling scenarios, component-shadowing checks, inert repository plugins,
actual CLI plus independent `tsc` verification, approval/application and conditional
recovery. The owned non-TS LSP preservation fixture is separately labeled.
The final scoped run is **{counts}**, across {native['groups']} groups;
format/build/all-target Clippy, Python{python}, typecheck and
TypeScript{ts_pass}/{ts_skip}skipped pass on **{final['inputs']} fixed native / {retained['frozenInputs']} fixed
retained inputs**. Earlier failed development/link runs remain archived, including
the disk-exhausted link that ran no tests and verified build-cache cleanup.
Classic function declarations, broader moves, other contexts/languages, runtime
equivalence and full native/current-head hosted acceptance remain open. The earlier
Windows symlink-privilege gate remains unresolved; its original test and source
reader were not weakened. See [compiler inline evidence](validation/integration-2026-10-19/compiler-inline/README.md).

'''
status=work/'docs/STATUS.md';text=status.read_text(encoding='utf-8');marker='## Current integration work\n\n';assert marker in text and 'All **18 new cases pass**' not in text;text=text.replace(marker,marker+paragraph,1);status.write_text(text,encoding='utf-8')
p=work/'docs/REMAINING_WORK.md';text=p.read_text(encoding='utf-8');old='| R1: semantic edits | Guarded native/provider planning, exact declaration ownership, directory/file usage checks, explicit TS/JS propagation and session routing. | Broader declaration, file and language shapes and provider intents, especially inline/move and compiler-bound consumer behavior; qualify each extension with its actual selected provider and independent apply/recovery. |';assert old in text
new='| R1: semantic edits | Guarded native/provider planning, declaration ownership, directory/file usage, TS/JS propagation/session routing and actual host-selected immutable TS5.8.3 function-binding inlining with compiler-bound consumers and independent source/effect/apply/recovery checks. | Classic function-body inline, broader moves/declaration/file/language/context shapes, dynamic or outside-file consumers and wider runtime/upstream acceptance. The compiler variable-inline action and existing LSP own_v1 move contract retain distinct finite scopes. |'
text=text.replace(old,new,1);text=text.replace('## Acceptance still required\n\n','## Acceptance still required\n\n'+paragraph,1);p.write_text(text,encoding='utf-8')
p=work/'README.md';text=p.read_text(encoding='utf-8');text+='\nExplicit [immutable compiler inlining](native/docs/compiler-inline.md) adds\nhost-selected TypeScript 5.8.3 function-binding inline plans for TS/TSX/JS/JSX\nthrough native sessions and the existing facade, with exact source/effect checks\nand separate verification, approval, application and recovery.\n';p.write_text(text,encoding='utf-8')
print(json.dumps({'native':tot,'groups':native['groups'],'nativeInputs':final['inputs'],'retainedInputs':retained['frozenInputs'],'python':python,'typeScriptPassed':ts_pass,'typeScriptSkipped':ts_skip}))
