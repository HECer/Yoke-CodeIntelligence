from pathlib import Path
import json,hashlib,subprocess,collections,shutil
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');out=root/'docs/validation/integration-2026-10-19/portable-projections';index=json.loads((out/'index.json').read_bytes());totals=index['finalScopedTotals'];tracker=root/'docs/parity/native-reconciliation-2026-10-15.json';old=json.loads((out/'frozen-tracker.json').read_bytes());assert tracker.read_bytes()==(out/'frozen-tracker.json').read_bytes();value=json.loads(tracker.read_bytes());r2=next(x for x in value['prioritizedRemainingScopes'] if x['id']=='R2')
boundary='Semantic alias identity/de-dup, mixed Office/PDF source spaces, cross-history/workspace joins, broader serialized schemas and outcome learning remain open. Checked same-snapshot native joins and lookup/degree/directed-trace consumers retain complete native source/receipt replay. Separate portable native group/join consumers now check supplied presentation structure without inheriting source authority; coherent forgeries/omissions and semantic/model quality are not verified.'
r2['missingBehavior']=boundary;r2['suggestedScope']='Preserve checked native replay and the separate data-only portable projection boundary. Define mixed-document/cross-history/workspace contracts and reviewed semantic alias identity/outcome learning without granting source authority or semantic truth to labels, host assertions or serialized JSON.'
r2['hostGroupingAddition']=r2['hostGroupingAddition'].replace('mixed-document spaces and generic serialized projection consumers remain open.','mixed-document spaces remain open. Separate bounded data-only native group/join imports now support structural inspection and queries, without source replay or checked JoinedGraph authority.')
paths=['native/src/meaning_projection.rs','native/src/meaning_projection_cli.rs','native/tests/meaning_projection_portable.rs','native/docs/meaning-projection.md','docs/validation/integration-2026-10-19/portable-projections/index.json']
for p in paths:
    if p not in r2['evidence']:r2['evidence'].append(p)
r2['portableProjectionCheckpoint']={'index':paths[-1],'newDefaultTestsPassed':9,'finalScopedTotals':totals,'sourceAuthority':False,'sourceReplayPerformed':False,'semanticIdentityEstablished':False,'wholeNativeRerun':False,'fullParityCertified':False}
next(x for x in value['residualRegister'] if x['id']=='meaning_dedup_feedback')['remainingBoundary']=boundary+' Broader R9 outcome/corroboration/decay/transcript feedback remains separate.'
assert all(value[k]==old[k] for k in ['inventoryLedger','historicalInventory','ledgerSummary'])
assert not any(x['id']=='portable_meaning_presentation_consumers' for x in value['reconciledCapabilities'])
value['reconciledCapabilities'].append({'id':'portable_meaning_presentation_consumers','behavior':'Storeless bounded import of native group/join presentation exports checks policy/node identity, graph/packet membership, original directed endpoint mappings, evidence references and ordered hyperedge slots. Inspection, lookup, degree and directed trace preserve parallel observations, selfloops and roles. Imported metadata stays untrusted; no checked JoinedGraph, source replay, provider, model, permission or write authority is created. Coherent omissions/forgeries remain undetectable from the supplied data alone.','evidence':paths,'remainingReview':boundary,'fullContractCertified':False})
tracker.write_bytes((json.dumps(value,indent=2,ensure_ascii=False)+'\n').encode())
paragraph=f'''[Portable Meaning projections](../native/docs/meaning-projection.md) now read
native group/join exports without a Store or provider and support inspection,
node lookup, degree and bounded directed traces. Policy/node identities,
graph/packet membership, original directed endpoints, evidence references and
every ordered hyperedge slot are structurally checked. Parallel observations,
selfloops, original quotes and repeated roles remain distinct. Imported replay,
attestation and completeness claims stay nested untrusted metadata; results
report `bindingValidation=not_replayed`, `sourceAuthority=false` and
`complete=false`. Import cannot create a checked `JoinedGraph` capability or
detect coherent omitted records/forgeries from the export alone.

All **nine new contracts** pass, including actual exports after Store/provider
removal, grouped and two-parent records, asymmetric traversal, exact independent
byte/count/output boundaries, malformed/Unicode/protocol/authority/path cases,
deadline and CLI no-initialization checks. The frozen selected run records
**{totals['passed']} native tests / {totals['failed']} failed / {totals['ignored']} existing ignored gates** across six
groups. The failure is an unchanged archive security fixture that cannot create
a real Windows symlink (OS 1314); it is neither skipped nor weakened. Final
regressions use `--no-fail-fast`. The final test-only strengthening explicitly
closes the engine and asserts removal of the original source/Store/provider
directory. Runtime/library inputs remain identical to the preceding 429-test
library run; it is not claimed as a second run. These are local owned protocol
fixtures, not live-model or semantic
quality validation. Clippy, format, **17 Python tests**, mandatory typecheck and
**260 TypeScript tests / 3 existing skips** pass with
**{index['frozenIntegratedInputs']} integrated inputs unchanged**. The initial API RED and an
intermediate packet-selection error in the traversal test are retained with
raw logs and source hashes. [Evidence](validation/integration-2026-10-19/portable-projections/index.json)
records the finite scope. Semantic alias identity/de-dup, mixed-document spaces,
cross-history/workspace joins, outcome learning, other R1-R9 work, whole-native
acceptance, main merge and release remain open.

'''
status=root/'docs/STATUS.md';raw=status.read_bytes();nl=b'\r\n' if b'\r\n' in raw else b'\n';marker=b'Windows ZIP and VSIX development packages were built at `9f48353`:';assert raw.count(marker)==1;status.write_bytes(raw.replace(marker,paragraph.replace('\n',nl.decode()).encode()+marker))
readme=root/'docs/validation/integration-2026-10-19/README.md';prefix=readme.read_bytes();readme.write_bytes(prefix+(f'\nPortable Meaning projections: [portable-projections/index.json](portable-projections/index.json) preserves the initial API RED, intermediate packet-selection fixture failure and final nine contracts with {totals["passed"]}/{totals["failed"]}/{totals["ignored"]} selected native results. The unchanged archive fixture retains its OS1314 symlink-creation failure. Imported data never provides source replay or semantic identity; whole-project acceptance remains open.\n').encode());assert readme.read_bytes().startswith(prefix)
before=json.loads((out/'integrated/source-before.json').read_bytes());records=[];counts=collections.Counter()
for name,h in before.items():
    frozen=(out/'frozen-tracker.json').read_bytes() if name==tracker.relative_to(root).as_posix() else (root/name).read_bytes();assert hashlib.sha256(frozen).hexdigest()==h,name
    blob=subprocess.check_output(['git','show','HEAD:'+name],cwd=root);kind='raw_equal' if blob==frozen else 'crlf_only_checkout_materialization';assert blob==frozen or blob.replace(b'\r\n',b'\n')==frozen.replace(b'\r\n',b'\n'),name
    if name==tracker.relative_to(root).as_posix():kind='thematic_status_metadata_after_freeze'
    counts[kind]+=1;records.append({'path':name,'frozenSha256':h,'gitSha256':hashlib.sha256(blob).hexdigest(),'currentSha256':hashlib.sha256((root/name).read_bytes()).hexdigest(),'materialization':kind})
(out/'source-materialization.json').write_bytes((json.dumps({'inputCount':len(before),'counts':dict(counts),'entries':records},indent=2)+'\n').encode());shutil.copyfile(__file__,out/'ci-r2-update-docs.py')
index['inventoryPreserved']={k:old[k] for k in ['historicalInventory','ledgerSummary']};index['sourceMaterialization']={'path':'source-materialization.json','counts':dict(counts),'inputCount':len(before)};index['metadataAfterFreeze']={'path':tracker.relative_to(root).as_posix(),'beforeSha256':before[tracker.relative_to(root).as_posix()],'afterSha256':hashlib.sha256(tracker.read_bytes()).hexdigest(),'scope':'Thematic R2 status/evidence only; inventoryLedger, historicalInventory and ledgerSummary unchanged.'}
index['artifacts']=[{'path':p.relative_to(out).as_posix(),'bytes':len(p.read_bytes()),'sha256':hashlib.sha256(p.read_bytes()).hexdigest()} for p in sorted(out.rglob('*')) if p.is_file() and p.relative_to(out).as_posix()!='index.json'];(out/'index.json').write_bytes((json.dumps(index,indent=2,ensure_ascii=False)+'\n').encode());print(json.dumps({'artifacts':len(index['artifacts']),'totals':totals,'materializationCounts':dict(counts),'indexSha256':hashlib.sha256((out/'index.json').read_bytes()).hexdigest()}))
