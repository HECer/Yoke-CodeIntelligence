from pathlib import Path
import json,hashlib,subprocess,collections,shutil
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');out=root/'docs/validation/integration-2026-10-19/captured-owners';index=json.loads((out/'index.json').read_bytes());totals=index['finalScopedTotals'];tracker=root/'docs/parity/native-reconciliation-2026-10-15.json';old=json.loads((out/'frozen-tracker.json').read_bytes());assert tracker.read_bytes()==(out/'frozen-tracker.json').read_bytes();value=json.loads(tracker.read_bytes());r5=next(x for x in value['prioritizedRemainingScopes'] if x['id']=='R5')
boundary=r5['missingBehavior']+' Separate captured per-path CODEOWNERS interpretation now retains exact rule/token byte evidence, base/head and all supported matches over an unchanged impactAreas parent in the same pinned read transaction. Empty-owner winners are explicit; unsupported active rules make winners unknown, and uncaptured higher-precedence paths stay visible. The finite pattern policy, labels and captured selection do not establish full GitHub parser equivalence, actual identity, permissions, ownership or review requirements. Broader ownership and the remaining R5 interpretations stay open.'
r5['missingBehavior']=boundary;r5['suggestedScope']='Extend broader source-bound ownership/report and compiler interpretations while preserving the separate finite captured rule policy, unchanged legacy publications, source/receipt provenance and visible ambiguity/unknowns. No inferred user/team/review authority, automatic Git/current-source capture, provider, merge, resolution, fetch, hook/filter/project execution or remote freshness follows.'
paths=['native/src/git_owners.rs','native/src/git_owners_cli.rs','native/src/git_owners_tests.rs','native/src/git_areas.rs','native/tests/git_owners.rs','native/docs/git-owners.md','docs/validation/integration-2026-10-19/captured-owners/index.json']
for p in paths:
    if p not in r5['evidence']:r5['evidence'].append(p)
r5['capturedOwnersCheckpoint']={'index':paths[-1],'newPublicContractTestsPassed':10,'newPrivatePatternTestsPassed':3,'newTestsIncludedInScopedTotals':True,'finalScopedTotals':totals,'legacyOwnerParserUnchanged':True,'ownerIdentityVerified':False,'githubPermissionVerified':False,'wholeNativeRerun':False,'fullParityCertified':False}
next(x for x in value['residualRegister'] if x['id']=='git_pr_affected_client_extras')['remainingBoundary'] += ' Separate captured per-path authored owner interpretations preserve individual rule/token quotes and explicit empty/unknown winners without changing legacy reports. Actual owner/team/review authority and full GitHub pattern/schema equivalence remain unverified.'
assert all(value[k]==old[k] for k in ['inventoryLedger','historicalInventory','ledgerSummary']);assert not any(x['id']=='captured_codeowners_path_candidates' for x in value['reconciledCapabilities'])
value['reconciledCapabilities'].append({'id':'captured_codeowners_path_candidates','behavior':'Checked impact/area replay stays pinned through captured rule interpretation. New per-path base/head rows retain the unchanged parent, every supported matching rule and exact winning rule/owner-token bytes, including explicit empty owner lists. Unsupported active syntax makes winners indeterminate; uncaptured higher-priority files remain unknown. Legacy owner parser/report IDs stay unchanged. No user/team/permission/review/ownership truth, Git/network/model/current-source fallback or state publication is supplied.','evidence':paths,'remainingReview':boundary,'fullContractCertified':False})
tracker.write_bytes((json.dumps(value,indent=2,ensure_ascii=False)+'\n').encode())
paragraph=f'''[Captured owner candidates](../native/docs/git-owners.md) now add `git owners`
over an unchanged saved impact/area parent. Base/head stay separate, each rule
and owner token retains its exact byte quote, all supported matches stay visible
and explicit empty-owner winners are preserved. The original owner parser and
report/replay identities remain unchanged. An internal callback keeps the same
checked read transaction through additional captured rule reads. Unsupported
active syntax makes winners indeterminate; uncaptured higher-precedence files
remain visible. The finite independent pattern policy and authored labels do
not establish actual ownership, identity, GitHub permission/review requirements
or full upstream parser/schema equivalence. No Git/provider/network/model,
current-source fallback, capture or Store/source publication occurs.

All **10 new saved-Git/CLI contracts** and **three included library parser cases**
pass: exact empty/rule/token quotes, same-label distinct positions, base/head,
live-source isolation, unsupported/uncaptured cases, independent exact quotas,
output, tampered CAS, SQL/caller transaction/cancel/deadline and offline CLI
admission/no initialization. The frozen selected run records **{totals['passed']} passed /
{totals['failed']} failed / {totals['ignored']} existing ignored gates** across four native groups. Clippy,
format, **17 Python tests**, mandatory typecheck and **260 TypeScript tests /
3 existing skips** pass with **{index['frozenIntegratedInputs']} integrated inputs unchanged**.
[Evidence](validation/integration-2026-10-19/captured-owners/index.json) retains
the initial API RED, implementation attempts, source hashes and unchanged
legacy-parser proof. Hard maxima are not claimed as fully populated qualification.
These scoped results do not replace earlier whole-native failures, including
Windows OS 1314 symlink prerequisites. Wider R5/R1-R9 work, whole-native/platform
acceptance, main merge and release remain open.

'''
status=root/'docs/STATUS.md';raw=status.read_bytes();nl=b'\r\n' if b'\r\n' in raw else b'\n';marker=b'Windows ZIP and VSIX development packages were built at `9f48353`:';assert raw.count(marker)==1;status.write_bytes(raw.replace(marker,paragraph.replace('\n',nl.decode()).encode()+marker))
readme=root/'docs/validation/integration-2026-10-19/README.md';prefix=readme.read_bytes();readme.write_bytes(prefix+(f'\nCaptured per-path owner candidates: [captured-owners/index.json](captured-owners/index.json) retains the API RED, ten new contracts, three included parser cases and {totals["passed"]}/{totals["failed"]}/{totals["ignored"]} selected native results. Legacy owner parser/report IDs and original evidence remain unchanged. Actual ownership/GitHub permission/review authority and whole-project acceptance remain open.\n').encode());assert readme.read_bytes().startswith(prefix)
before=json.loads((out/'integrated/source-before.json').read_bytes());records=[];counts=collections.Counter()
for name,h in before.items():
    frozen=(out/'frozen-tracker.json').read_bytes() if name==tracker.relative_to(root).as_posix() else (root/name).read_bytes();assert hashlib.sha256(frozen).hexdigest()==h,name
    blob=subprocess.check_output(['git','show','HEAD:'+name],cwd=root);kind='raw_equal' if blob==frozen else 'crlf_only_checkout_materialization';assert blob==frozen or blob.replace(b'\r\n',b'\n')==frozen.replace(b'\r\n',b'\n'),name
    if name==tracker.relative_to(root).as_posix():kind='thematic_status_metadata_after_freeze'
    counts[kind]+=1;records.append({'path':name,'frozenSha256':h,'gitSha256':hashlib.sha256(blob).hexdigest(),'currentSha256':hashlib.sha256((root/name).read_bytes()).hexdigest(),'materialization':kind})
(out/'source-materialization.json').write_bytes((json.dumps({'inputCount':len(before),'counts':dict(counts),'entries':records},indent=2)+'\n').encode());shutil.copyfile(__file__,out/'ci-r5-update-docs.py')
index['inventoryPreserved']={k:old[k] for k in ['historicalInventory','ledgerSummary']};index['sourceMaterialization']={'path':'source-materialization.json','counts':dict(counts),'inputCount':len(before)};index['metadataAfterFreeze']={'path':tracker.relative_to(root).as_posix(),'beforeSha256':before[tracker.relative_to(root).as_posix()],'afterSha256':hashlib.sha256(tracker.read_bytes()).hexdigest(),'scope':'Thematic R5 status/evidence only; inventoryLedger, historicalInventory and ledgerSummary unchanged.'}
index['artifacts']=[{'path':p.relative_to(out).as_posix(),'bytes':len(p.read_bytes()),'sha256':hashlib.sha256(p.read_bytes()).hexdigest()} for p in sorted(out.rglob('*')) if p.is_file() and p.relative_to(out).as_posix()!='index.json'];(out/'index.json').write_bytes((json.dumps(index,indent=2,ensure_ascii=False)+'\n').encode());print(json.dumps({'artifacts':len(index['artifacts']),'totals':totals,'materializationCounts':dict(counts),'indexSha256':hashlib.sha256((out/'index.json').read_bytes()).hexdigest()}))
