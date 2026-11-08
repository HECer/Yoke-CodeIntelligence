from pathlib import Path
import json,hashlib,subprocess,shutil,re
root=Path.cwd();out=root/'docs/validation/integration-2026-10-19/file-usage';index=json.loads((out/'index.json').read_bytes());counts=index['finalScopedTotals'];assert counts=={'passed':63,'failed':1,'ignored':0};before=json.loads((out/'integrated/source-before.json').read_bytes());tracker=root/'docs/parity/native-reconciliation-2026-10-15.json';original=tracker.read_bytes();assert hashlib.sha256(original).hexdigest()==before[tracker.relative_to(root).as_posix()];(out/'frozen-tracker.json').write_bytes(original);old=json.loads(original);v=json.loads(original)
boundary='Finite captured text directory moves retain their separately evidenced standard-LSP folder/resource contract. Default captured-file usage checks now support .ts/.js ES modules of at most eight named functions, using exact provider definition/reference bindings and conservative captured graph/path/name candidates before one guarded deletion plan. Unknown consumers remain explicit and remainingUsagesMayBreak stays true. Other file/declaration/language shapes, propagation, other symbol-parent declarations/languages, broader inline/rename/provider intents, exhaustive reference/IDE guarantees, binary or multiple-provider trees and host-scope/cross-filesystem migration remain open. Existing own_v1 attachment bundles, legacy plan identities and mandatory host permission/approval/verification/stale/recovery gates remain intact; multi-plan publication is not atomic.'
r1=next(r for r in v['prioritizedRemainingScopes'] if r['id']=='R1');r1['missingBehavior']=boundary;r1['suggestedScope']='Continue propagated deletion and other file/declaration/language cases with checked original operations and independent postconditions. Preserve finite directory/file selection, visible provider/unknown coverage, legacy identities and guarded application/recovery.'
paths=['native/src/semantic_file_delete.rs','native/tests/facade_file_usage.rs','native/tests/file_usage_live.rs','native/tests/fixtures/file-usage-lsp.mjs','native/docs/file-usage.md','docs/validation/integration-2026-10-19/file-usage/index.json']
for p in paths:
 if p not in r1['evidence']:r1['evidence'].append(p)
r1['fileUsageCheckpoint']={'index':'docs/validation/integration-2026-10-19/file-usage/index.json','finalScopedTotals':counts,'integratedInputs':len(before),'scopedRuntimeInputsMatched':550,'actualOwnedLanguages':['typescript','javascript'],'actualCompositeRustTests':1,'initialLibraryObservationOnly':{'passed':425,'failed':0,'ignored':7},'wholeNativeRerun':False,'countsAreNonadditive':True,'fullParityCertified':False}
next(r for r in v['residualRegister'] if r['id']=='refactor_and_provider_intent')['remainingBoundary']=boundary;next(r for r in v['reconciledCapabilities'] if r['id']=='bounded_semantic_refactor_equivalents')['remainingReview']=boundary
assert not any(r['id']=='captured_file_usage_checks' for r in v['reconciledCapabilities']);v['reconciledCapabilities'].append({'id':'captured_file_usage_checks','behavior':'Default omitted/null captured-file selection checks a bounded TS/JS function ES module, conservative captured usages and exact self-bound provider definitions/references before one ordinary guarded deletion plan. Missing/empty/truncated/foreign proof refuses; source hashes, provider policy/omissions and unknown consumers remain explicit. One admitted bulk scope retains every per-file policy/hash/cancellation check. Explicit forced deletion and approval/verification/application/recovery keep their existing gates.','evidence':paths,'remainingReview':boundary,'fullContractCertified':False});assert all(v[k]==old[k] for k in ['inventoryLedger','historicalInventory','ledgerSummary']);tracker.write_bytes((json.dumps(v,indent=2,ensure_ascii=False)+'\n').encode())
paragraph='''Default [captured file usage checks](../native/docs/file-usage.md) now prepare
deletion plans for bounded TypeScript/JavaScript ES modules of named functions.
Each declaration needs an exact source-bound provider definition and references
including itself, with no reported usage outside the selected file. Captured
graph/path/name candidates independently refuse possible usage. Missing, empty,
foreign or truncated responses refuse before plan publication. Provider policy,
source omissions and unknown consumers remain visible; `complete=false`,
`exhaustiveCompilerProof=false` and `remainingUsagesMayBreak=true` remain.
Explicit forced deletion, host edit permission, exact approval, executed
verification, stale-source checks and recovery retain their separate gates.

All **nine new cases** pass, including omitted/null selectors at both analyzer
epochs, 2,000/2,001 files, 16 MB/16 MB+1 byte, eight/nine declarations,
permissions, malformed/absent provider results and actual guarded recovery.
The final scoped run records **63 passed / 1 failed / 0 ignored** across six
groups; the sole failure remains real OS 1314 symlink setup in the unchanged
forced-delete fixture. A separate actual TypeScript language-server/compiler
gate exercises both TypeScript and JavaScript: copied compilation, approved
application, exact recovery and used-module refusal pass. It is one Rust test
with two owned fixtures and is nonadditive.

The initial 2,000-file test exceeded its unchanged 30-second deadline while
rebuilding source scope per read. An exact diagnostic RED is preserved.
Bulk reads now reuse one checked captured scope while retaining all per-file
policy, size, hash and cancellation checks; the original boundary assertion
and deadline pass. Final all-target Clippy passes. **550 scoped runtime inputs**
match the integrated checkout with recorded CRLF materialization; formatting,
**17 Python tests**, mandatory typecheck and **260 TypeScript tests / 3 existing
skips** pass with **665 integrated inputs unchanged**. The earlier 425/0/7 library
observation uses the previous private loop and remains separately labeled.
No whole-native rerun after this addition is claimed. The
[file-usage evidence](validation/integration-2026-10-19/file-usage/index.json)
retains exact attempts, source hashes and real provider/compiler outputs.

Other file/declaration/language shapes, propagated deletion, broader R1–R9,
hosted/platform acceptance, packaging, main merge and release remain open.

'''
p=root/'docs/STATUS.md';s=p.read_text(encoding='utf-8');anchor='Windows ZIP and VSIX development packages were built at `9f48353`:';assert s.count(anchor)==1 and 'Default [captured file usage checks]' not in s;p.write_bytes(s.replace(anchor,paragraph+anchor,1).encode())
p=root/'README.md';s=p.read_text(encoding='utf-8');anchor='## TypeScript advanced runtime';assert s.count(anchor)==1;s=s.replace(anchor,'[Captured file usage checks](native/docs/file-usage.md) add bounded default\ndeletion planning for TypeScript/JavaScript function modules with exact\nprovider bindings, conservative captured usages and visible unknown consumers.\n\n'+anchor,1);p.write_bytes(s.encode())
p=root/'docs/validation/integration-2026-10-19/README.md';raw=p.read_bytes();assert raw==subprocess.check_output(['git','show','HEAD:docs/validation/integration-2026-10-19/README.md']);newline=b'\r\n' if b'\r\n' in raw else b'\n';text='''[Captured file usage checks](file-usage/index.json) bind finite TS/JS function
modules, conservative captured usages and exact provider definitions/references
before one guarded deletion plan. All nine new cases pass; the final scoped run
records **63/1/0** with one unchanged OS1314 prerequisite. A separate actual
TypeScript/JavaScript provider/compiler/apply/recovery gate passes one Rust test
containing two fixtures. Counts are nonadditive. The original 2,000-file deadline
RED is retained; one checked bulk scope fixes repeated admission without changing
the deadline or per-file checks. Final all-target Clippy and integrated format,
Python17, typecheck and TypeScript260/3 skipped pass; 550 scoped source bindings
and 665 unchanged integrated inputs are retained. The initial library425/0/7
observation uses an earlier private loop. No whole-native rerun, general absence
proof, full R1-R9, upstream/platform, packaging, main merge or release is claimed.
''';p.write_bytes(raw+text.encode().replace(b'\n',newline)+newline)
(out/'metadata-after-freeze.json').write_bytes((json.dumps({'trackerBeforeSha256':hashlib.sha256(original).hexdigest(),'trackerAfterSha256':hashlib.sha256(tracker.read_bytes()).hexdigest(),'runtimeChangedByThisStep':False,'inventoryPreserved':True,'scope':'Post-validation R1 thematic capability/evidence/boundary documentation only.'},indent=2)+'\n').encode())
for name in ['update-file-usage-docs.py','verify-file-usage-staged.py']:shutil.copyfile(root/'validation/integration-20261018'/name,out/name)
index['metadataAfterFreeze']='metadata-after-freeze.json';index['artifacts']=[]
for p in sorted(out.rglob('*')):
 if p.is_file() and p!=out/'index.json':
  b=p.read_bytes();index['artifacts'].append({'path':p.relative_to(out).as_posix(),'bytes':len(b),'sha256':hashlib.sha256(b).hexdigest()})
(out/'index.json').write_bytes((json.dumps(index,indent=2,ensure_ascii=False)+'\n').encode());print(json.dumps({'artifacts':len(index['artifacts']),'inventoryPreserved':True,'scopedTotals':counts,'integratedInputs':len(before)}))
