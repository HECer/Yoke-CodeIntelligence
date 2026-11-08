from pathlib import Path
import json,hashlib,subprocess,collections,shutil
root=Path.cwd();out=root/'docs/validation/integration-2026-10-19/propagated-delete';index=json.loads((out/'index.json').read_bytes());totals=index['finalScopedTotals'];assert totals['failed']==0;before=json.loads((out/'integrated/source-before.json').read_bytes());tracker=root/'docs/parity/native-reconciliation-2026-10-15.json';old=json.loads((out/'frozen-tracker.json').read_bytes());assert tracker.read_bytes()==(out/'frozen-tracker.json').read_bytes();value=json.loads(tracker.read_bytes());r1=next(r for r in value['prioritizedRemainingScopes'] if r['id']=='R1')
boundary=r1['missingBehavior']+' Explicit captured_es_function_propagation_v1 now removes checked standalone target-call statements and newly unused private function dependencies in one bounded TS/JS ES module, including private cycles. Surviving reported/possible consumers and exported dependencies remain protected. Other expression consumers, module/declaration/language/file shapes, general propagation, future consumer changes and exhaustive compiler/IDE guarantees remain outside this finite policy.'
paths=['native/src/semantic_refactor.rs','native/src/semantic_propagated_delete.rs','native/tests/propagated_delete_contract.rs','native/tests/propagated_delete_live.rs','native/tests/fixtures/propagated-delete-lsp.mjs','native/docs/propagated-delete.md','docs/validation/integration-2026-10-19/propagated-delete/index.json'];r1['missingBehavior']=boundary;r1['suggestedScope']='Continue broader propagation and remaining declaration/file/language/provider intents with exact original source operations, directed dependency/reference semantics and independent postconditions. Preserve finite source/proof/unknown-consumer boundaries, immutable anchors, legacy identities and guarded application/recovery.'
for path in paths:
 if path not in r1['evidence']:r1['evidence'].append(path)
r1['functionPropagationCheckpoint']={'index':paths[-1],'finalScopedTotals':totals,'newDefaultCases':20,'actualCompositeRustTests':5,'actualOwnedFixtureCount':10,'actualOwnedLanguages':index['actualOwnedLanguages'],'wholeNativeRerun':False,'countsAreNonadditive':True,'fullParityCertified':False}
for key,id_,field in [('residualRegister','refactor_and_provider_intent','remainingBoundary'),('reconciledCapabilities','bounded_semantic_refactor_equivalents','remainingReview')]:next(r for r in value[key] if r['id']==id_)[field]=boundary
assert not any(r['id']=='captured_function_deletion_propagation' for r in value['reconciledCapabilities']);value['reconciledCapabilities'].append({'id':'captured_function_deletion_propagation','behavior':'Explicit TS/JS symbol propagate=true checks exact captured AST/entities, provider definitions/references and graph/lexical candidates. It removes only whole standalone target-call statements and newly unused private dependency declarations, handles private cycles and retains candidates with surviving reported/possible consumers or exports. Captured physical source is checked before/after provider work; one guarded original-coordinate plan preserves legacy IDs and all permission/approval/verification/stale/recovery gates. Force cannot bypass missing proofs. Future/dynamic/external/omitted consumers remain unknown and remainingUsagesMayBreak stays true.','evidence':paths,'remainingReview':boundary,'fullContractCertified':False});assert all(value[k]==old[k] for k in ['inventoryLedger','historicalInventory','ledgerSummary']);tracker.write_bytes((json.dumps(value,indent=2,ensure_ascii=False)+'\n').encode())
paragraph=f'''[Captured function propagation](../native/docs/propagated-delete.md) now handles
explicit symbol `propagate=true` in a finite TS/JS ES function module. Exact
provider-bound standalone call statements are removed, then newly unused
private dependencies can be pruned together, including private cycles.
Surviving reported/possible consumers and exported dependencies stay protected;
unrelated already-unused functions are preserved. Other expression consumers
refuse before publication. One ordinary guarded plan retains original byte
coordinates, permissions, approval, verification, stale-source and recovery
requirements. Unknown future/dynamic/external consumers remain explicit with
`complete=false` and `remainingUsagesMayBreak=true`.

All **20 new default contracts** pass, covering cycles/liveness, argument
dependencies, duplicate/nested calls, both epochs/force, BOM/UTF16, malformed or
omitted proofs, authority/paths, provider-time source changes/deadline and exact
declaration/statement/module/file/physical-byte quotas. The frozen selected run
passes **{totals['passed']} native tests / 0 failed / {totals['ignored']} existing ignored gates** across
fourteen groups. Five separate actual Rust harness tests cover ten owned
TS/JS/Rust/Python compiler/AST/provider/apply/recovery fixtures. Counts across
attempts/gates are nonadditive. The original capability refusal and initial
borrow-checker build failure remain preserved. A subsequent adversarial RED
showed an invalid provider comment-reference could select an unrelated unused
function. The first role predicate also admitted an Arrow parameter in a second
adversarial RED. Positive expression-role admission now refuses comments, strings,
bindings and unknown roles; all four role variants remain asserted.

Format, all-target Clippy, **17 Python tests**, mandatory typecheck and **260
TypeScript tests / 3 existing skips** pass with **{index['frozenIntegratedInputCount']} integrated inputs unchanged**.
[Evidence](validation/integration-2026-10-19/propagated-delete/index.json) binds
source, raw attempts and Git/CRLF materialization. Maximum-inventory quota
probes intentionally stop at unavailable definition proof; they do not claim
a complete maximum-size provider workflow. No whole-native rerun is claimed.
General propagation beyond this finite policy, other R1–R9 work, hosted/platform
acceptance, main merge and release remain open.

'''
status=root/'docs/STATUS.md';raw=status.read_bytes();nl=b'\r\n' if b'\r\n' in raw else b'\n';marker=b'Windows ZIP and VSIX development packages were built at `9f48353`:';assert raw.count(marker)==1;status.write_bytes(raw.replace(marker,paragraph.replace('\n',nl.decode()).encode()+marker))
readme=root/'docs/validation/integration-2026-10-19/README.md';prefix=readme.read_bytes();readme.write_bytes(prefix+(f'\nCaptured function propagation: [propagated-delete/index.json](propagated-delete/index.json) preserves the capability RED, initial compiler failure,20 new contracts, final {totals["passed"]}/0/{totals["ignored"]} selected native result and ten actual owned language fixtures. General propagation and full native/platform/upstream acceptance remain open.\n').encode());assert readme.read_bytes().startswith(prefix)
records=[];counts=collections.Counter()
for name,h in before.items():
 frozen=(out/'frozen-tracker.json').read_bytes() if name==tracker.relative_to(root).as_posix() else (root/name).read_bytes();assert hashlib.sha256(frozen).hexdigest()==h,name;blob=subprocess.check_output(['git','show','HEAD:'+name]);kind='raw_equal' if blob==frozen else 'crlf_only_checkout_materialization';assert blob==frozen or blob.replace(b'\r\n',b'\n')==frozen.replace(b'\r\n',b'\n'),name
 if name==tracker.relative_to(root).as_posix():kind='thematic_status_metadata_after_freeze'
 counts[kind]+=1;records.append({'path':name,'frozenSha256':h,'gitSha256':hashlib.sha256(blob).hexdigest(),'currentSha256':hashlib.sha256((root/name).read_bytes()).hexdigest(),'materialization':kind})
(out/'source-materialization.json').write_bytes((json.dumps({'inputCount':len(before),'counts':dict(counts),'entries':records},indent=2)+'\n').encode());shutil.copyfile(Path(__file__),out/'update-propagated-delete-docs.py');index['inventoryPreserved']={k:old[k] for k in ['historicalInventory','ledgerSummary']};index['sourceMaterialization']={'path':'source-materialization.json','counts':dict(counts),'inputCount':len(before)};index['metadataAfterFreeze']={'path':tracker.relative_to(root).as_posix(),'beforeSha256':before[tracker.relative_to(root).as_posix()],'afterSha256':hashlib.sha256(tracker.read_bytes()).hexdigest(),'scope':'Thematic R1 status/evidence only; original inventoryLedger, historicalInventory and ledgerSummary preserved.'};index['artifacts']=[{'path':p.relative_to(out).as_posix(),'bytes':len(p.read_bytes()),'sha256':hashlib.sha256(p.read_bytes()).hexdigest()} for p in sorted(out.rglob('*')) if p.is_file() and p.name!='index.json'];(out/'index.json').write_bytes((json.dumps(index,indent=2,ensure_ascii=False)+'\n').encode());print(json.dumps({'artifacts':len(index['artifacts']),'totals':totals,'materializationCounts':dict(counts),'indexSha256':hashlib.sha256((out/'index.json').read_bytes()).hexdigest()}))
