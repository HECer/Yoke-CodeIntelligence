from pathlib import Path
import subprocess,json,hashlib,shutil,sys
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');root=w.parent/'Yoke-CodeIntelligence';raw=root/'validation/integration-20261018/function-body-inline';base='aac60b583386e277a358e612e882f0bb4b9af896'
read=lambda p:json.loads(p.read_text(encoding='utf-8-sig'));sha=lambda b:hashlib.sha256(b).hexdigest()
run=lambda *a,cwd=w:subprocess.check_output(a,cwd=cwd)
source=run('git','rev-parse','HEAD').decode().strip();assert source=='a0794a82afc06f5195b5226fc4055ae72a3f7aea'
assert run('git','rev-parse','HEAD',cwd=root).decode().strip()==source
assert not run('git','status','--porcelain').strip() and not run('git','status','--porcelain',cwd=root).strip()
final=read(raw/'final-r3/result.json');guard=read(raw/'guard-final-r1/result.json');broad=read(raw/'final-r2/result.json');ret=read(raw/'retained-r4/result.json');meta=read(raw/'metadata/proof.json');cli=read(raw/'eval-native-green/result.json')
assert all(r['allChecksPassed'] and r['sourceUnchanged'] for r in [final,guard,broad,ret])
assert cli['allChecksPassed'] and cli['binaryUnchanged'] and read(raw/'metadata/result.json')['allChecksPassed']
for n,h in read(raw/'final-r3/source-after.json').items():assert sha((w/n).read_bytes())==h,n
for n,h in read(raw/'retained-r4/source-after.json').items():
 if n=='docs/parity/native-reconciliation-2026-10-15.json':assert h==meta['trackerBeforeSha256'] and sha((w/n).read_bytes())==meta['trackerAfterSha256']
 else:assert sha((w/n).read_bytes())==h,n
pg='native/tests/fixtures/postgres-catalog-response.jsonl';assert len((w/pg).read_bytes())==2779 and sha((w/pg).read_bytes())=='f67a05349a6cedf55222afd3bb3231c169d382dd4b078d76544daac517530b47'
unchanged={}
for n in ['native/src/analysis.rs','native/src/model.rs','native/src/lsp.rs','native/src/lsp_refactor_intents.rs','native/src/semantic_move_intents.rs','native/src/semantic_refactor.rs','native/src/changes.rs','native/src/changes_plan.rs','native/src/agent_repl.rs','native/src/project_session.rs','native/src/compiler_inline_worker.cjs','native/src/arxiv_rate.rs','native/tests/session_refactor.rs','Cargo.toml','Cargo.lock','native/Cargo.toml',pg]:
 a=(w/n).read_bytes();b=run('git','show',base+':'+n);assert a.replace(b'\r\n',b'\n')==b.replace(b'\r\n',b'\n'),n;unchanged[n]={'currentSha256':sha(a),'baseGitSha256':sha(b),'match':'raw' if a==b else 'crlf-only'}
new_tests=['actual_classic_body_expansion_removes_the_call_and_retains_the_original_definition_only_when_requested','actual_body_frames_preserve_argument_order_local_scope_closures_and_finally_control','actual_jsdoc_body_and_retained_export_keep_their_captured_bindings','parameter_initialization_preserves_defaults_rest_patterns_optional_types_and_inferred_names','unsafe_or_unowned_body_forms_refuse_before_any_plan_or_derived_source','wrapped_eval_in_the_body_or_caller_cannot_publish_a_plan','body_cli_plans_require_independent_verification_approval_and_foreign_preserving_recovery']
log=(raw/'final-r3/native.log').read_text(encoding='utf-8');assert all('test '+n+' ... ok' in log for n in new_tests)
q={'baseIntegration':base,'sourceCommit':source,'finalPhase':'final-r3','latestNativeTotals':final['commands'][0]['totals'],'latestNativeGroups':2,'nativeFrozenInputs':final['inputs'],'guardPhase':'guard-final-r1','guardUnits':guard['commands'][1]['totals'],'guardIntegrations':guard['commands'][2]['totals'],'guardGroups':7,'precedingBroadPhase':'final-r2','precedingBroadTotals':broad['commands'][0]['totals'],'precedingBroadGroups':14,'countsNonadditive':True,'retainedPhase':'retained-r4','retainedFrozenInputs':ret['frozenInputs'],'newIntegrationFunctions':new_tests,'newUnitFunctions':4,'parameterScenarios':12,'controlFlowScenarios':11,'epochAndKeepScenarios':4,'jsAndExportScenarios':2,'positiveCliGateScenarios':2,'evalRefusalScenarios':9,'actualCliEvalRefusalScenarios':4,'metadataOnlyAfterFreeze':meta,'wholeNativeRerun':False,'otherPlatformsCertified':False,'fullParityCertified':False,'mainMerged':False,'releasePerformed':False,'unchangedSources':unchanged}
(raw/'qualification.json').write_text(json.dumps(q,indent=2)+'\n',encoding='utf-8');shutil.copyfile(__file__,raw/'evidence.py')
node=Path(shutil.which('node')).resolve();compiler=(w/'node_modules/typescript/lib/typescript.js').resolve()
(raw/'tools.json').write_text(json.dumps({'node':{'path':str(node),'version':run(str(node),'--version').decode().strip(),'sha256':sha(node.read_bytes())},'typescript':{'path':str(compiler),'version':json.loads((compiler.parent.parent/'package.json').read_text())['version'],'sha256':sha(compiler.read_bytes())},'python':sys.version,'rustc':run(str(Path.home()/'.cargo/bin/rustc.exe'),'--version').decode().strip()},indent=2)+'\n',encoding='utf-8')
dest=w/'docs/validation/integration-2026-10-19/function-body-inline';dest.mkdir(exist_ok=False);shutil.copytree(raw,dest/'runs')
files=[{'path':p.relative_to(dest).as_posix(),'bytes':p.stat().st_size,'sha256':sha(p.read_bytes())} for p in sorted((dest/'runs').rglob('*')) if p.is_file()]
phases=[]
for p in sorted((dest/'runs').glob('*/result.json')):
 r=read(p);phases.append({'phase':p.parent.name,'resultPath':p.relative_to(dest).as_posix(),'allChecksPassed':r.get('allChecksPassed'),'exitCode':r.get('exitCode'),'status':r.get('status'),'nativeAdmission':r.get('nativeAdmission'),'sourceUnchanged':r.get('sourceUnchanged'),'inputs':r.get('inputs',r.get('frozenInputs'))})
index={'schemaVersion':1,'kind':'native-immutable-classic-function-body-inline','sourceCommit':source,'baseIntegration':base,'scope':['Finite strict-module body statement frames with explicit inlineCompiler host trust','Independent native original-byte, binding, type-coverage, control-flow and budget admission','Defaults/rest/patterns, inferred default names and finally cancellation','Direct eval refusals across transparent TypeScript wrappers, escapes and instantiations','Owned runtime/CLI/guarded application evidence; full STATUS remains incomplete'],'finalPhase':'final-r3','guardPhase':'guard-final-r1','precedingBroadPhase':'final-r2','retainedPhase':'retained-r4','countsNonadditive':True,'phases':phases,'files':files}
(dest/'index.json').write_text(json.dumps(index,indent=2)+'\n',encoding='utf-8')
readme='''# Classic function-body inline development evidence

Source `SOURCE` extends the existing optional `inlineCompiler` host profile with
independently reconstructed classic function-body statements. It preserves
original snapshot/entity bytes and the separate variable-inline action.

| Scope | Actual result |
| --- | --- |
| Latest body/compiler groups, final-r3 | 14 passed / 0 failed / 0 ignored |
| Latest frozen native inputs | 805 unchanged |
| Before final wrapper extension, guard-final-r1 | Four units and 82 integration passes; two existing gates ignored |
| Preceding broader snapshot, final-r2 | 668 passed / 0 failed / 9 existing ignored, across 14 groups |
| Latest format/build/all-target Clippy with warnings denied | Passed |
| Retained-r4 on 799 unchanged inputs | Python17, typecheck, TypeScript260 passed / 3 existing skips |
| Real eval CLI counterprobes | Four refused; zero plan/report/derived snapshots; inputs unchanged |
| Metadata after accepted freeze | Python17; all 805 native and 798 other retained inputs exact; tracker-only decoded-data proof |

These are distinct frozen source sets and overlapping scopes. Counts are never
added. This is not a whole-native, current-head hosted or full Windows acceptance
claim. The two existing session gates require explicitly selected installed tools;
the new compiler CLI gates independently execute their own installed tsc checks.

Seven new integration functions exercise twelve parameter and eleven control-flow
scenarios, epoch/keep-definition variants, JS/export cases and two real CLI
verification/approval/apply/foreign-preserving recovery scenarios. Nine direct-eval
forms refuse before publication, including generic instantiation with an actual
captured ambient overload. Four unit functions independently check frame source
copies, owned execution, escaped invocation identifiers and coherent forged reports.
Missing type observations cannot be omitted from both witnesses to bypass checks.

The labelled frame lowers outer returns while retaining nested returns. Normal
completion clears a return cancelled by finally break/continue. Argument evaluation
precedes parameter initialization; computed own-property defaults retain inferred
anonymous names without invoking the __proto__ prototype setter. Original types
provide contextual typing. Preparation executes no analyzed source. Runtime
comparisons explicitly execute owned literal fixtures only.

Earlier failures remain raw: missing-argument default typing, one wrong test import,
typed-arrow assertion precedence, the reproduced finally regression, an intentionally
aborted final-r1, and incorrect direct-eval worker/native admissions. The final SDK
wrapper extension uses actual pinned TypeScript5.8.3 isOuterExpression; its four
actual CLI green checks retain exact executable hashes before/after use. Synthetic
worker prototypes explicitly claim no native admission. Their success cannot
substitute for the subsequent real native/CLI tests.

Only documentation/tracker metadata follows the runtime freeze. The proof preserves
all other decoded tracker data and existing key order; inventory identities and
false full-contract certifications remain. Materialization compares 916 paths:
667 raw matches and 249 CRLF-only text variants, with no runtime rerun in the main
integration checkout. The protected PostgreSQL fixture remains 2779 bytes with
SHA256 f67a05349a6cedf55222afd3bb3231c169d382dd4b078d76544daac517530b47.

Var hoisting, wider invocation/expression/declaration/language/consumer forms,
reflection/runtime/upstream equivalence, existing platform failures and configured
App deployment remain open. No main merge or release is performed.

The [index](index.json) binds ARTIFACTS raw artifacts by size and SHA256.
This explanatory README is metadata outside that raw-artifact index.
'''.replace('SOURCE',source).replace('ARTIFACTS',str(len(files)))
(dest/'README.md').write_text(readme,encoding='utf-8')
p=w/'docs/validation/integration-2026-10-19/README.md';s=p.read_text(encoding='utf-8');marker='`index.json` binds the selected raw artifacts and their precise scopes.\n';assert marker in s;s=s.replace(marker,marker+'\nThe [classic function-body checkpoint](function-body-inline/README.md) adds finite\noriginal-byte statement expansion, checked parameter/finally semantics and direct-\neval exclusions. Its latest14, preceding guard86 and broader668 passes have separate\nfrozen inputs and are nonadditive. Full STATUS/R1-R9 and platform acceptance remain\nopen. Failed development phases and corrected native CLI counterprobes are retained.\n',1);p.write_text(s,encoding='utf-8',newline='\n')
print(json.dumps({'sourceCommit':source,'artifacts':len(files),'indexBytes':(dest/'index.json').stat().st_size,'indexSha256':sha((dest/'index.json').read_bytes())}))
