from pathlib import Path
import json,hashlib,subprocess,collections,shutil
root=Path.cwd();out=root/'docs/validation/integration-2026-10-19/declaration-attachments';index=json.loads((out/'index.json').read_bytes());totals=index['finalScopedTotals'];assert totals['failed']==0;before=json.loads((out/'integrated/source-before.json').read_bytes());tracker=root/'docs/parity/native-reconciliation-2026-10-15.json';old=json.loads((out/'frozen-tracker.json').read_bytes());assert tracker.read_bytes()==(out/'frozen-tracker.json').read_bytes();value=json.loads(tracker.read_bytes());r1=next(r for r in value['prioritizedRemainingScopes'] if r['id']=='R1')
boundary=r1['missingBehavior'].split(' Exact TypeScript/TSX/JavaScript/JSX symbol deletion',1)[0]+' Checked TypeScript/TSX/JavaScript/JSX declaration/export/ambient deletion and Rust/Python owning attribute/documentation/decorator deletion now keep original entity/evidence coordinates. Contextual method binding uses the nearest declared owner; nested functions stay functions. Multi-declarators, expression consumers, separate TS member decorators, otherwise empty required Python bodies and other-language attachment ownership remain outside these finite symbol-deletion contracts.'
paths=['native/src/analysis.rs','native/src/changes.rs','native/src/changes_symbol_delete.rs','native/src/refactor_contract.rs','native/tests/declaration_attachment_ranges.rs','native/tests/declaration_attachments_live.rs','native/docs/symbol-deletion.md','docs/validation/integration-2026-10-19/declaration-attachments/index.json'];r1['missingBehavior']=boundary;r1['suggestedScope']='Continue propagated deletion and remaining declaration/file/language/provider cases using checked original operations and independent postconditions. Preserve finite body/ownership refusals, source-bound contextual kinds, visible unknown consumers, legacy identities and guarded application/recovery.'
for path in paths:
 if path not in r1['evidence']:r1['evidence'].append(path)
r1['declarationAttachmentsCheckpoint']={'index':paths[-1],'finalScopedTotals':totals,'newDefaultCases':7,'actualCompositeRustTests':3,'actualOwnedFixtureCount':6,'actualOwnedLanguages':index['actualOwnedLanguages'],'wholeNativeRerun':False,'countsAreNonadditive':True,'fullParityCertified':False}
for key,id_,field in [('residualRegister','refactor_and_provider_intent','remainingBoundary'),('reconciledCapabilities','bounded_semantic_refactor_equivalents','remainingReview'),('reconciledCapabilities','checked_symbol_declaration_deletion_ranges','remainingReview')]:next(r for r in value[key] if r['id']==id_)[field]=boundary
assert not any(r['id']=='owned_declaration_attachment_deletion' for r in value['reconciledCapabilities']);value['reconciledCapabilities'].append({'id':'owned_declaration_attachment_deletion','behavior':'Rust symbol deletion reuses checked outer attributes/documentation/attachment comments; Python includes the exact owning decorated_definition and refuses otherwise empty required bodies. Exact AST/entity binding applies the released nearest-owner function/method classification, checks forged kinds/names/ranges and preserves BOM/Unicode/CRLF. Original entities and evidence stay unchanged; no project module execution, provider protocol changes or new durable authority schema.','evidence':paths,'remainingReview':boundary,'fullContractCertified':False});assert all(value[k]==old[k] for k in ['inventoryLedger','historicalInventory','ledgerSummary']);tracker.write_bytes((json.dumps(value,indent=2,ensure_ascii=False)+'\n').encode())
paragraph=f'''The subsequent [declaration attachment repair](../native/docs/symbol-deletion.md)
extends checked symbol deletion to Rust outer attributes/documentation and
Python decorators. Actual REDs show Rust moving `#[inline]` onto a following
constant and Python silently attaching a decorator to the retained function.
The Python check compiles and inspects the stdlib AST without executing project
modules. Rust reuses the existing owned attachment grammar. Exact kind binding
includes the nearest declared owner; nested functions remain functions. Python
refuses deletion that would otherwise leave a required body empty.

The final frozen source passes **{totals['passed']} selected native tests / 0 failed /
{totals['ignored']} existing ignored gates** across eleven groups, including seven new
default cases and existing symbol-move contracts. Separate explicit gates pass
three Rust harness tests covering six owned Rust/Python/TS/JS fixtures: actual
compiler/AST/provider checks, approved application and exact recovery. Counts
across attempts/gates are nonadditive. The initial repair's two method-binding
refusals remain preserved with the original compiler/AST REDs.

Format, all-target Clippy, **17 Python tests**, mandatory typecheck and **260
TypeScript tests / 3 existing skips** pass with all **{index['frozenIntegratedInputCount']} integrated inputs
unchanged**. [Evidence](validation/integration-2026-10-19/declaration-attachments/index.json)
records raw attempts, source hashes and Git/CRLF materialization. No whole-native
rerun is claimed. Other declaration/language shapes, general propagation, wider
R1–R9, hosted/platform acceptance, main merge and release remain open.

'''
status=root/'docs/STATUS.md';raw=status.read_bytes();nl=b'\r\n' if b'\r\n' in raw else b'\n';marker=b'Windows ZIP and VSIX development packages were built at `9f48353`:';assert raw.count(marker)==1;status.write_bytes(raw.replace(marker,paragraph.replace('\n',nl.decode()).encode()+marker))
readme=root/'docs/validation/integration-2026-10-19/README.md';prefix=readme.read_bytes();readme.write_bytes(prefix+(f'\nRust/Python declaration attachments: [declaration-attachments/index.json](declaration-attachments/index.json) preserves two actual compiler/AST REDs, contextual method-binding refusals, final {totals["passed"]}/0/{totals["ignored"]} selected tests and six actual owned language fixtures. Full native/platform/upstream acceptance remains open.\n').encode());assert readme.read_bytes().startswith(prefix)
records=[];counts=collections.Counter()
for name,h in before.items():
 frozen=(out/'frozen-tracker.json').read_bytes() if name==tracker.relative_to(root).as_posix() else (root/name).read_bytes();assert hashlib.sha256(frozen).hexdigest()==h,name;blob=subprocess.check_output(['git','show','HEAD:'+name]);kind='raw_equal' if blob==frozen else 'crlf_only_checkout_materialization';assert blob==frozen or blob.replace(b'\r\n',b'\n')==frozen.replace(b'\r\n',b'\n'),name
 if name==tracker.relative_to(root).as_posix():kind='thematic_status_metadata_after_freeze'
 counts[kind]+=1;records.append({'path':name,'frozenSha256':h,'gitSha256':hashlib.sha256(blob).hexdigest(),'currentSha256':hashlib.sha256((root/name).read_bytes()).hexdigest(),'materialization':kind})
(out/'source-materialization.json').write_bytes((json.dumps({'inputCount':len(before),'counts':dict(counts),'entries':records},indent=2)+'\n').encode());shutil.copyfile(Path(__file__),out/'update-declaration-attachments-docs.py');index['inventoryPreserved']={k:old[k] for k in ['historicalInventory','ledgerSummary']};index['sourceMaterialization']={'path':'source-materialization.json','counts':dict(counts),'inputCount':len(before)};index['metadataAfterFreeze']={'path':tracker.relative_to(root).as_posix(),'beforeSha256':before[tracker.relative_to(root).as_posix()],'afterSha256':hashlib.sha256(tracker.read_bytes()).hexdigest(),'scope':'Thematic R1 status/evidence only; original inventoryLedger, historicalInventory and ledgerSummary preserved.'};index['artifacts']=[{'path':p.relative_to(out).as_posix(),'bytes':len(p.read_bytes()),'sha256':hashlib.sha256(p.read_bytes()).hexdigest()} for p in sorted(out.rglob('*')) if p.is_file() and p.name!='index.json'];(out/'index.json').write_bytes((json.dumps(index,indent=2,ensure_ascii=False)+'\n').encode());print(json.dumps({'artifacts':len(index['artifacts']),'totals':totals,'materializationCounts':dict(counts),'indexSha256':hashlib.sha256((out/'index.json').read_bytes()).hexdigest()}))
