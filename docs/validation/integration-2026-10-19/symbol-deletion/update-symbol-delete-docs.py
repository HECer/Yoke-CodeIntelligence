from pathlib import Path
import json,hashlib,subprocess,collections,shutil
root=Path.cwd();out=root/'docs/validation/integration-2026-10-19/symbol-deletion';index=json.loads((out/'index.json').read_bytes());assert index['finalScopedTotals']=={'passed':466,'failed':0,'ignored':7};before=json.loads((out/'integrated/source-before.json').read_bytes());tracker=root/'docs/parity/native-reconciliation-2026-10-15.json';old=json.loads((out/'frozen-tracker.json').read_bytes());assert tracker.read_bytes()==(out/'frozen-tracker.json').read_bytes();value=json.loads(tracker.read_bytes());r1=next(r for r in value['prioritizedRemainingScopes'] if r['id']=='R1')
boundary=r1['missingBehavior']+' Exact TypeScript/TSX/JavaScript/JSX symbol deletion now includes only checked owning declaration/export/ambient syntax while keeping original entity/evidence ranges. Multi-declarators, expression consumers, separate member decorators and other-language attachment ownership remain outside this finite deletion contract.'
paths=['native/src/changes.rs','native/src/changes_symbol_delete.rs','native/tests/symbol_delete_ranges.rs','native/tests/symbol_delete_live.rs','native/docs/symbol-deletion.md','docs/validation/integration-2026-10-19/symbol-deletion/index.json'];r1['missingBehavior']=boundary;r1['suggestedScope']='Continue propagated deletion, remaining language attachment ownership and broader file/symbol/provider cases with exact source operations, explicit unsupported coverage and independent compiler postconditions. Preserve original entity/evidence coordinates, legacy identities and guarded application/recovery.'
for path in paths:
 if path not in r1['evidence']:r1['evidence'].append(path)
r1['symbolDeletionCheckpoint']={'index':paths[-1],'finalScopedTotals':index['finalScopedTotals'],'newDefaultCases':8,'actualCompositeRustTests':1,'actualOwnedFixtureCount':4,'actualOwnedLanguages':['typescript','javascript'],'wholeNativeRerun':False,'countsAreNonadditive':True,'fullParityCertified':False}
for key,id_,field in [('residualRegister','refactor_and_provider_intent','remainingBoundary'),('reconciledCapabilities','bounded_semantic_refactor_equivalents','remainingReview')]:next(r for r in value[key] if r['id']==id_)[field]=boundary
assert not any(r['id']=='checked_symbol_declaration_deletion_ranges' for r in value['reconciledCapabilities']);value['reconciledCapabilities'].append({'id':'checked_symbol_declaration_deletion_ranges','behavior':'TypeScript/TSX/JavaScript/JSX deleteSymbol derives one independently removable declaration range from an exact captured AST/entity binding, including checked owning export/ambient or single-declarator syntax. Stored entity and usage anchors remain unchanged. Unsupported consumers/siblings, malformed syntax, forged bindings and capacity/cancellation refuse. Existing usage, permission, approval, verification, stale and recovery gates remain separate.','evidence':paths,'remainingReview':boundary,'fullContractCertified':False})
assert all(value[k]==old[k] for k in ['inventoryLedger','historicalInventory','ledgerSummary']);tracker.write_bytes((json.dumps(value,indent=2,ensure_ascii=False)+'\n').encode())
paragraph='''[Symbol deletion boundaries](../native/docs/symbol-deletion.md) now derive
TypeScript/TSX/JavaScript/JSX declaration ranges from exact captured AST/entity
bindings. Export and ambient wrappers belong to the deletion only with their
owning declaration; stored symbol and usage-evidence coordinates stay intact.
Unsupported sibling/consumer ownership refuses before plan publication.

The original actual TypeScript RED left `export` behind and failed copied
compilation with TS1030; its frozen source and output are preserved. The repair
passes **466 selected native tests / 0 failed / 7 existing ignored gates** across
seven groups, including eight new default cases. One separate actual provider/
compiler test covers four owned TypeScript/JavaScript named/default-export
fixtures: copied compilation, approved application and exact CRLF/Unicode
recovery all pass. Counts are nonadditive. Format, all-target Clippy, **17 Python
tests**, mandatory typecheck and **260 TypeScript tests / 3 existing skips** pass
with unchanged frozen inputs. [Evidence](validation/integration-2026-10-19/symbol-deletion/index.json)
binds every attempt, integrated source and Git materialization. No whole-native
rerun is claimed. Other-language attachments, general propagation, wider R1–R9,
hosted/platform acceptance, packaging, main merge and release remain open.

'''
status=root/'docs/STATUS.md';raw=status.read_bytes();nl=b'\r\n' if b'\r\n' in raw else b'\n';marker=b'Windows ZIP and VSIX development packages were built at `9f48353`:';assert raw.count(marker)==1;status.write_bytes(raw.replace(marker,paragraph.replace('\n',nl.decode()).encode()+marker))
readme=root/'docs/validation/integration-2026-10-19/README.md';prefix=readme.read_bytes();readme.write_bytes(prefix+b'\nSymbol declaration deletion: [symbol-deletion/index.json](symbol-deletion/index.json) preserves the actual TS1030 RED, 466/0/7 scoped native result, four actual TS/JS compiler/apply/recovery fixtures and unchanged integrated inputs. This finite checkpoint does not replace historical full-native failures or certify wider parity.\n');assert readme.read_bytes().startswith(prefix)
records=[];counts=collections.Counter()
for name,h in before.items():
 frozen=(out/'frozen-tracker.json').read_bytes() if name==tracker.relative_to(root).as_posix() else (root/name).read_bytes();assert hashlib.sha256(frozen).hexdigest()==h,name;blob=subprocess.check_output(['git','show','HEAD:'+name]);kind='raw_equal' if blob==frozen else 'crlf_only_checkout_materialization';assert blob==frozen or blob.replace(b'\r\n',b'\n')==frozen.replace(b'\r\n',b'\n'),name
 if name==tracker.relative_to(root).as_posix():kind='thematic_status_metadata_after_freeze'
 counts[kind]+=1;records.append({'path':name,'frozenSha256':h,'gitSha256':hashlib.sha256(blob).hexdigest(),'currentSha256':hashlib.sha256((root/name).read_bytes()).hexdigest(),'materialization':kind})
(out/'source-materialization.json').write_bytes((json.dumps({'inputCount':len(before),'counts':dict(counts),'entries':records},indent=2)+'\n').encode());shutil.copyfile(Path(__file__),out/'update-symbol-delete-docs.py')
index['inventoryPreserved']={k:old[k] for k in ['historicalInventory','ledgerSummary']};index['sourceMaterialization']={'path':'source-materialization.json','counts':dict(counts),'inputCount':len(before)};index['metadataAfterFreeze']={'path':tracker.relative_to(root).as_posix(),'beforeSha256':before[tracker.relative_to(root).as_posix()],'afterSha256':hashlib.sha256(tracker.read_bytes()).hexdigest(),'scope':'Thematic R1 status/evidence only; original inventoryLedger, historicalInventory and ledgerSummary preserved.'};index['artifacts']=[{'path':p.relative_to(out).as_posix(),'bytes':len(p.read_bytes()),'sha256':hashlib.sha256(p.read_bytes()).hexdigest()} for p in sorted(out.rglob('*')) if p.is_file() and p.name!='index.json'];(out/'index.json').write_bytes((json.dumps(index,indent=2,ensure_ascii=False)+'\n').encode());print(json.dumps({'artifacts':len(index['artifacts']),'materializationCounts':dict(counts),'indexSha256':hashlib.sha256((out/'index.json').read_bytes()).hexdigest()}))
