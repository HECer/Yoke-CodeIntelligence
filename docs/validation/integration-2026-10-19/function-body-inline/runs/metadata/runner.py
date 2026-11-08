from pathlib import Path
import json,subprocess,copy,hashlib,shutil
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach')
raw=w.parent/'Yoke-CodeIntelligence/validation/integration-20261018/function-body-inline'
read=lambda p:json.loads(p.read_text(encoding='utf-8-sig'))
sha=lambda b:hashlib.sha256(b).hexdigest()
final=read(raw/'final-r3/result.json');guard=read(raw/'guard-final-r1/result.json');broad=read(raw/'final-r2/result.json');ret=read(raw/'retained-r4/result.json');cli=read(raw/'eval-native-green/result.json')
assert all(r['allChecksPassed'] and r['sourceUnchanged'] for r in [final,guard,broad,ret])
assert cli['allChecksPassed'] and cli['binaryUnchanged']
assert final['commands'][0]['totals']=={'passed':14,'failed':0,'ignored':0}
p=w/'docs/parity/native-reconciliation-2026-10-15.json';original_bytes=p.read_bytes();before=read(p);after=copy.deepcopy(before)
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
old=find(before);item=find(after);assert item and 'compilerFunctionBodyDevelopment' not in item
needle='Classic function declarations, other languages/contexts, outside-file consumers, unowned trivia, broader moves/function-body inlining and runtime/IDE equivalence remain open.'
assert item['missingBehavior'].count(needle)==1
item['missingBehavior']=item['missingBehavior'].replace(needle,'A separate native body route under that profile now expands finite classic strict-module functions into caller statements, with original-byte argument/parameter/body frames, labelled returns and exact compiler binding/type coverage. Defaults/rest/patterns/optional parameters, nested returns and finally cancellation are checked. Direct eval, including wrappers, Unicode escapes and type instantiations, refuses before publication. Var hoisting, async/generic/invocation-bound and wider expression/language/context/consumer semantics remain open.')
item['suggestedScope']='Continue var hoisting/function-scope interactions, wider function/expression/caller forms and remaining import attributes, sibling declaration bundles, export/import/destination/context/language/provider shapes. Preserve original byte/binding/type correspondence, legacy variable-inline and graph/plan/session identities, shared budgets and independent verification/application/recovery; qualify dynamic consumers and full Windows/current-head hosted/upstream behavior separately.'
item['compilerFunctionBodyDevelopment']={
 'implemented':True,'fullContractCertified':False,'compiler':'TypeScript 5.8.3','hostOptIn':'inlineCompiler','actionKind':'native.inline.function.body',
 'scope':'Finite classic strict-module function-body statement expansion with trusted compiler observations and independent native source reconstruction',
 'newIntegrationFunctions':7,'newUnitFunctions':4,'parameterScenarios':12,'controlFlowScenarios':11,'epochAndKeepScenarios':4,'jsAndExportScenarios':2,'positiveCliGateScenarios':2,'evalRefusalScenarios':9,'actualCliEvalRefusalScenarios':4,
 'latestNativePhase':'final-r3','latestNativeTotals':final['commands'][0]['totals'],'latestNativeGroups':2,'nativeFrozenInputs':final['inputs'],
 'precedingGuardPhase':'guard-final-r1','precedingGuardUnits':guard['commands'][1]['totals'],'precedingGuardIntegrationTotals':guard['commands'][2]['totals'],
 'precedingBroadPhase':'final-r2','precedingBroadTotals':broad['commands'][0]['totals'],'precedingBroadGroups':14,'countsAreNonadditive':True,
 'retainedPhase':'retained-r4','retainedFrozenInputs':ret['frozenInputs'],'wholeNativeRerun':False,'platformEvidence':'Windows','otherPlatformsCertified':False,
 'evidence':['native/src/compiler_inline_body.rs','native/src/compiler_inline_body_parameters.rs','native/src/compiler_inline_body_admission.rs','native/src/compiler_inline_body_worker.cjs','native/tests/compiler_inline_body.rs','docs/validation/integration-2026-10-19/function-body-inline/index.json'],
 'remaining':['Var hoisting and wider function-scope interactions','Async/generic/invocation-bound and spread forms','Wider expression contexts, declaration shapes, languages and outside-file consumers','Dynamic/reflective consumers and general runtime equivalence','Full Windows/current-head hosted/upstream acceptance']}
assert list(old)==[k for k in item if k!='compilerFunctionBodyDevelopment']
proof=copy.deepcopy(after);proof_item=find(proof)
for key in ['missingBehavior','suggestedScope']:proof_item[key]=old[key]
del proof_item['compilerFunctionBodyDevelopment'];assert proof==before
meta=raw/'metadata';meta.mkdir(exist_ok=False);(meta/'tracker-before.json').write_bytes(original_bytes)
p.write_text(json.dumps(after,indent=2,ensure_ascii=False)+'\n',encoding='utf-8',newline='\n')
proof={'baseCommit':subprocess.check_output(['git','rev-parse','HEAD'],cwd=w).decode().strip(),'changedR1Fields':['missingBehavior','suggestedScope','compilerFunctionBodyDevelopment'],'allOtherDecodedDataPreserved':True,'existingKeyOrderPreserved':True,'trackerBeforeSha256':sha(original_bytes),'trackerAfterSha256':sha(p.read_bytes()),'runtimeCodeChangedAfterFinalChecks':False}
(meta/'proof.json').write_text(json.dumps(proof,indent=2)+'\n',encoding='utf-8')
summary='''Classic function-body expansion now has a separate native statement-frame route
under the existing optional `inlineCompiler` profile. Arguments evaluate before
defaults/rest/destructuring parameters. Original body/caller bytes, nested returns,
finally cancellation, static targets and declaration type coverage are checked
before ordinary guarded planning. Anonymous default names and contextual types
retain their source meaning. Direct eval refuses even through parentheses, Unicode
escapes and TypeScript erasures/instantiations. Preparation executes no analyzed code.

The final targeted compiler/body run is **14 passed / 0 failed**, across two
groups on 805 unchanged native inputs, with format/build/all-target Clippy passed.
Before the final wrapper extension, the guard phase passed four units plus
82 integration tests with two existing gates ignored. The preceding broader
snapshot passed **668 / 0 / 9 existing ignored**, across 14 groups. Those are
different frozen source sets and overlapping scopes; counts are not added.
Python17, typecheck and TypeScript260/3skipped pass on 799 fixed retained inputs.
Only documentation/tracker metadata changes after these checks, with a separate
proof preserving all runtime bytes and all other decoded tracker data.

Seven new integration functions cover twelve parameter and eleven control-flow
scenarios, both analyzer epochs, retained definitions, JS/export forms, two real
CLI verification/approval/apply/recovery scenarios and nine eval refusals. Four
additional real CLI counterprobes leave no plan/report/derived snapshot and retain
all input bytes. Runtime comparisons execute only owned literal fixtures. Failed
development phases, the finally regression and earlier incorrect eval admissions
remain indexed beside their corrected checks.

Var hoisting, wider expression/invocation/declaration/language/consumer forms and
full R1-R9/Windows/current-head hosted/upstream acceptance remain open. Existing
OS1314/macOS failures and production App deployment remain separate. See
[the body contract](../native/docs/compiler-inline.md#classic-function-body-statements)
and [owned evidence](validation/integration-2026-10-19/function-body-inline/README.md).

'''
p=w/'docs/STATUS.md';s=p.read_text(encoding='utf-8');start=s.index('Classic function-body expansion now');end=s.index('Captured missing compiler-move imports',start);s=s[:start]+summary+s[end:];p.write_text(s,encoding='utf-8',newline='\n')
p=w/'docs/REMAINING_WORK.md';s=p.read_text(encoding='utf-8');s=s.replace('function-binding inlining and moves into existing captured ES modules','function-binding inlining, finite classic function-body statement expansion and moves into existing captured ES modules',1);s=s.replace('| Classic function-body inline, unsupported import attributes','| Var hoisting, wider body/expression/invocation forms, unsupported import attributes',1);marker='## Acceptance still required\n\n';assert marker in s;s=s.replace(marker,marker+summary,1);p.write_text(s,encoding='utf-8',newline='\n')
p=w/'README.md';s=p.read_text(encoding='utf-8');s=s.replace('host-selected TypeScript 5.8.3 function-binding inline plans for TS/TSX/JS/JSX','host-selected TypeScript 5.8.3 function-binding and finite classic function-body\ninline plans for TS/TSX/JS/JSX');p.write_text(s,encoding='utf-8',newline='\n')
shutil.copyfile(__file__,meta/'runner.py')
print(json.dumps({'latestNative':final['commands'][0]['totals'],'trackerOutsideR1Preserved':True,'runtimeCodeChangedAfterFinalChecks':False}))
