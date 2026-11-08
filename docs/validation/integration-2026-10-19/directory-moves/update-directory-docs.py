from pathlib import Path
import json,hashlib,re,subprocess,shutil
root=Path.cwd();out=root/'docs/validation/integration-2026-10-19/directory-moves';index=json.loads((out/'index.json').read_bytes());summary=index['summary'];counts=index['wholeNativeTotals'];groups=index['nativeResultGroups'];frozen=index['frozenInputCount'];assert summary['sourceUnchanged'] and summary['allTestableNativeTargetsRan'];assert all(c['exitCode']==0 for c in summary['checks'] if c['gate']!='whole-native')
ts=(out/'final/typescript-tests.log').read_text(encoding='utf-8');passed=int(re.search(r'\bpass (\d+)\s*$',ts,re.M)[1]);skipped=int(re.search(r'\bskipped (\d+)\s*$',ts,re.M)[1]);python=int(re.search(r'Ran (\d+) tests', (out/'final/python-contracts.log').read_text(encoding='utf-8'))[1]);failures=json.loads((out/'failure-classification.json').read_bytes())['failures'];symlinks=sum(f['windowsError1314'] for f in failures);other=len(failures)-symlinks
repair=index['postFreezeRepair'];assert repair['allProductCasesPassed'];repair_groups=next(g for g in repair['summary']['gates'] if g['gate']=='scoped-native')['groups'];repair_totals={k:sum(g[k] for g in repair_groups) for k in ['passed','failed','ignored']}
boundary='Finite complete captured text directory moves now have standard-LSP folder repair, one guarded resource plan, empty-directory/mode/physical-identity binding, copied-shape verification and conditional actual-rename recovery. Multiple provider languages, uncaptured or binary trees, host capture/workspace-root migration, cross-filesystem copy and broader directory/IDE reference guarantees remain open. Other symbol-parent declarations/languages, default-safe captured-file usage proof, propagated deletion, broader inline/rename/provider intents and exhaustive compiler reference binding remain open. The explicit own_v1 Rust named-declaration/negotiated-attachment slice and legacy file parent/basename behavior retain their separate evidence. Saved-plan identity/review replay, host permissions, approval, executed verification and stale/recovery gates remain mandatory; multi-plan publication is not atomic.'
tracker=root/'docs/parity/native-reconciliation-2026-10-15.json';original=tracker.read_bytes();assert hashlib.sha256(original).hexdigest()==json.loads((out/'final/source-before.json').read_bytes())['docs/parity/native-reconciliation-2026-10-15.json'];(out/'frozen-tracker.json').write_bytes(original);prior=json.loads(original);v=json.loads(original);r1=next(r for r in v['prioritizedRemainingScopes'] if r['id']=='R1');r1['missingBehavior']=boundary;r1['suggestedScope']='Continue default-safe captured-file usage checks, propagated deletion, other symbol-parent/language cases and broader inline/rename intents under checked source operations. Preserve finite directory completeness, explicit provider coverage, legacy identities and guarded application/recovery.'
evidence=['native/src/changes_directory.rs','native/src/semantic_directory_move.rs','native/src/changes_directory_tests.rs','native/tests/facade_directory_move.rs','native/tests/directory_move_live.rs','native/docs/directory-moves.md','docs/validation/integration-2026-10-19/directory-moves/index.json']
for path in evidence:
 if path not in r1['evidence']:r1['evidence'].append(path)
r1['directoryMoveCheckpoint']={'index':'docs/validation/integration-2026-10-19/directory-moves/index.json','initialFullNativeTotals':counts,'groups':groups,'frozenInputs':frozen,'allLocalChecksPassed':index['allLocalChecksPassed'],'postFreezeRepairTotals':repair_totals,'twoRuntimeInputsRepaired':repair['normalizedRuntimeDifferences'],'wholeNativeRerunAfterRepair':False,'actualRustAnalyzerAndRustcGatePassed':True,'countsAreNonadditive':True,'fullParityCertified':False}
next(r for r in v['residualRegister'] if r['id']=='refactor_and_provider_intent')['remainingBoundary']=boundary
cap=next(r for r in v['reconciledCapabilities'] if r['id']=='bounded_semantic_refactor_equivalents');cap['remainingReview']=boundary
assert not any(r['id']=='captured_directory_moves' for r in v['reconciledCapabilities']);v['reconciledCapabilities'].append({'id':'captured_directory_moves','behavior':'One complete captured writable text directory uses a matching host-selected standard-LSP folder request, checked original-coordinate repairs and one native guarded resource effect. Empty directories, modes and physical directory/parent identities enter directory-move-v1 identity; legacy file-only plan IDs remain unchanged. Copied verification checks the declared shape; actual rename application and conditional recovery preserve foreign writes.','evidence':evidence,'remainingReview':boundary,'fullContractCertified':False});assert all(v[k]==prior[k] for k in ['inventoryLedger','historicalInventory','ledgerSummary']);tracker.write_bytes((json.dumps(v,indent=2,ensure_ascii=False)+'\n').encode())
problem=f'{symlinks} failures occur while creating actual Windows symlink fixtures (OS 1314), before their safety assertions.' if symlinks else 'No OS 1314 fixture failure was recorded.'
assert other==1
problem+=' One additional failure exposed file-only jb.rename entering the directory-enabled shared helper and starting a provider. The initial full run remains failed.'
paragraph=f'''Complete [captured text directory moves](../native/docs/directory-moves.md) now
bind empty directories, modes and physical parent/tree identities into one plan.
A matching host-selected standard-LSP folder request contributes checked text
repairs. Copied verification checks directory shape; approved workspace apply
and conditional recovery use one actual filesystem rename. File-only plan IDs,
host permissions, stale-source checks and recovery journals remain compatible.

The initial frozen full Windows serial-fixture run records **{counts['passed']:,} passed /
{counts['failed']} failed / {counts['ignored']} ignored** across **{groups} groups**, with every testable
target executed and **{frozen} inputs unchanged**. {problem}
All ten new library directory cases and the folder-facade cases are included in
these totals. Scoped development counts are nonadditive. A separate actual
`rust-analyzer`/`rustc` gate repairs Rust module declarations and uses, compiles
the copied result, and passes approved application and exact recovery.
Format, all-target Clippy with warnings denied, doctests, **{python} Python tests**,
typecheck and **{passed} TypeScript tests / {skipped} existing skips** pass.

A separate two-input repair confines directory selection to `jb.move` and retains
file-only `jb.rename` refusal before provider startup, including a folder-capable
provider. Its scoped run records **{repair_totals['passed']} passed / {repair_totals['failed']} failed /
{repair_totals['ignored']} ignored**; the retained failure is the unchanged OS 1314 file-delete
setup. The original failing rename case and the new analyzer 8/9 omitted/null
cases pass. These counts are nonadditive. Format, all-target Clippy, the actual
Rust provider/compiler gate and the post-repair TypeScript checks pass. Exact
source bindings identify both repaired inputs; no full native rerun after this
repair is claimed.

The [directory evidence](validation/integration-2026-10-19/directory-moves/index.json)
retains the Windows alias RED, earlier fixture assertion, Clippy diagnostic,
raw logs, exact source/binary hashes and the real junction/empty-shape cases.
The selected tree admits 1–1,000 captured text files, 2,000 directories and
16 MB counted per physical file. Multi-provider/binary trees, host-scope migration,
cross-filesystem fallback, exhaustive reference/IDE guarantees, default-safe file
usage checks, propagated deletion and the wider STATUS/R1–R9 work remain open.
The isolated file-usage follow-up is outside this checkpoint. Full local/hosted
platform acceptance, packaging, main merge and release remain open.

'''
p=root/'docs/STATUS.md';s=p.read_text(encoding='utf-8');anchor='Windows ZIP and VSIX development packages were built at `9f48353`:';assert s.count(anchor)==1 and 'Complete [captured text directory moves]' not in s;p.write_bytes(s.replace(anchor,paragraph+anchor,1).encode())
p=root/'docs/validation/integration-2026-10-19/README.md';raw=p.read_bytes();assert raw==subprocess.check_output(['git','show','HEAD:docs/validation/integration-2026-10-19/README.md']);newline=b'\r\n' if b'\r\n' in raw else b'\n';text=f'''[Captured directory moves](directory-moves/index.json) bind a complete text tree,
empty directories, modes and physical identities through standard-LSP repair,
copied-shape verification, approved rename application and conditional recovery.
The initial complete serial-fixture run records **{counts['passed']} passed / {counts['failed']} failed /
{counts['ignored']} ignored** across {groups} groups with {frozen} unchanged inputs. {problem}
Ten new library cases and folder-facade cases are included; earlier scoped counts
and the separate actual rust-analyzer/rustc gate remain nonadditive. Format,
all-target Clippy, Python{python}, TypeScript{passed}/{skipped} skipped and typecheck pass.
The file-only rename regression has a separate two-input repair and a nonadditive
scoped run of {repair_totals['passed']}/{repair_totals['failed']}/{repair_totals['ignored']}; the unchanged symlink setup failure remains.
Both original/new rename refusals pass. Exact post-repair source, Clippy,
TypeScript and actual Rust provider/compiler evidence are retained; the initial
whole run stays failed and no whole-native rerun after repair is claimed.
Exact raw attempts, source and binary hashes remain preserved. Wider STATUS/R1-R9,
upstream/platform, packaging, main merge and release acceptance remain open.
''';p.write_bytes(raw+text.encode().replace(b'\n',newline)+newline)
p=root/'README.md';s=p.read_text(encoding='utf-8');anchor='## TypeScript advanced runtime';assert s.count(anchor)==1;s=s.replace(anchor,'[Captured directory moves](native/docs/directory-moves.md) retain complete text\ntrees, empty directories and physical identities through configured folder\nrepair, reviewed plans, copied verification and conditional recovery.\n\n'+anchor,1);p.write_bytes(s.encode())
metadata={'trackerBeforeSha256':hashlib.sha256(original).hexdigest(),'trackerAfterSha256':hashlib.sha256(tracker.read_bytes()).hexdigest(),'runtimeChangedByThisMetadataUpdate':False,'postFreezeRuntimeRepairs':repair['normalizedRuntimeDifferences'],'inventoryLedgerPreserved':True,'historicalInventoryPreserved':True,'ledgerSummaryPreserved':True,'scope':'This step updates thematic R1 capability/evidence and remaining-boundary documentation only; the separate two-input runtime repair retains its own source/validation. All original inventory identities/statuses/certifications remain unchanged.'};(out/'metadata-after-freeze.json').write_bytes((json.dumps(metadata,indent=2)+'\n').encode())
for name in ['update-directory-docs.py','verify-directory-staged.py']:shutil.copyfile(root/'validation/integration-20261018'/name,out/name)
index['metadataAfterFreeze']='metadata-after-freeze.json';index['artifacts']=[]
for p in sorted(out.rglob('*')):
 if p.is_file() and p!=out/'index.json':
  b=p.read_bytes();index['artifacts'].append({'path':p.relative_to(out).as_posix(),'bytes':len(b),'sha256':hashlib.sha256(b).hexdigest()})
(out/'index.json').write_bytes((json.dumps(index,indent=2,ensure_ascii=False)+'\n').encode())
print(json.dumps({'wholeNativeTotals':counts,'groups':groups,'frozenInputs':frozen,'pythonPassed':python,'typescriptPassed':passed,'typescriptSkipped':skipped,'symlinkPrerequisiteFailures':symlinks,'otherFailures':other,'inventoryPreserved':True}))
