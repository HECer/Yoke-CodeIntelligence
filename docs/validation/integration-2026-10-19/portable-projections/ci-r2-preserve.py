from pathlib import Path
import subprocess,json,hashlib,datetime,re,shutil
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');work=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');base=root/'validation/integration-20261018/portable-projections';out=root/'docs/validation/integration-2026-10-19/portable-projections';out.mkdir(exist_ok=False)
def sha(b):return hashlib.sha256(b).hexdigest()
final=json.loads((base/'r4/freeze.json').read_bytes());integ=json.loads((base/'integrated/summary.json').read_bytes());assert final['allNewCapabilityChecksPassed'] and not final['changed'] and integ['sourceUnchanged'] and integ['allChecksPassed']
for stage in ['red','r2','r3','r4','integrated']:
    dest=out/stage;shutil.copytree(base/stage,dest)
temp=Path.home()/'AppData/Local/Temp'
for p in list(temp.glob('ci-r2-*.py'))+[temp/'ci-r2-additional-tests.rs',temp/'ci-meaning-projection.md']:
    shutil.copyfile(p,out/p.name)
def groups(path):return [dict(zip(['passed','failed','ignored'],map(int,m))) for m in re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;',path.read_text(encoding='utf-8'))]
allgroups=groups(base/'r4/portable.log')+groups(base/'r3/library.log')+groups(base/'r4/regressions.log')
assert len(allgroups)==6 and sum(g['failed'] for g in allgroups)==1
assert groups(base/'r4/portable.log')==[{'passed':9,'failed':0,'ignored':0}]
regression=(base/'r4/regressions.log').read_text(encoding='utf-8');assert 'code: 1314' in regression and regression.count('test result: FAILED.')==1
assert 'existing_output_file_directory_and_hardlink_are_never_overwritten' in regression
shutil.copyfile(base/'r3-to-r4-test-only.json',out/'r3-to-r4-test-only.json')
r3=json.loads((base/'r3/source-before.json').read_bytes());r4=json.loads((base/'r4/source-before.json').read_bytes());assert set(r3)==set(r4)
changed=[name for name in r3 if r3[name]!=r4[name]];assert changed==['native/tests/meaning_projection_portable.rs']
totals={k:sum(g[k] for g in allgroups) for k in ['passed','failed','ignored']}
assert groups(base/'red/portable.log')==[{'passed':0,'failed':1,'ignored':0}]
assert groups(base/'r2/portable.log')==[{'passed':6,'failed':1,'ignored':0}]
assert 'meaning_projection_import_unavailable' in (base/'red/portable.log').read_text(encoding='utf-8')
ts=(base/'integrated/tests.log').read_text(encoding='utf-8');assert re.search(r'\bpass 260\b',ts) and re.search(r'\bfail 0\b',ts) and re.search(r'\bskipped 3\b',ts)
assert 'Ran 17 tests' in (base/'integrated/python-contracts.log').read_text(encoding='utf-8')
tracker='docs/parity/native-reconciliation-2026-10-15.json';frozen=json.loads((base/'integrated/source-before.json').read_bytes());assert sha((root/tracker).read_bytes())==frozen[tracker];shutil.copyfile(root/tracker,out/'frozen-tracker.json')
index={'schemaVersion':1,'recordedAt':datetime.datetime.now(datetime.timezone.utc).isoformat(),'parentCommit':'5d369e04f30bf45e2271a3ddaa0f4bac3998811c','implementationCommit':subprocess.check_output(['git','rev-parse','HEAD'],cwd=work).decode().strip(),'integratedRuntimeCommit':integ['runtimeCommit'],
    'accepted':False,'wholeNativeRerun':False,'fullParityCertified':False,'allScopedChecksPassed':False,'allNewCapabilityChecksPassed':True,'finalStage':'r4','finalScopedTotals':totals,'finalScopedResultGroups':allgroups,'newDefaultTestsPassed':9,'countsAreNonadditive':True,'frozenNativeInputs':final['inputs'],'frozenIntegratedInputs':integ['frozenInputs'],'integrationSummary':integ,'libraryReplaySourceLineage':{'stage':'r3','onlyFinalSourceDifference':changed,'runtimeAndLibraryInputsUnchanged':True},
    'findings':['Initial API RED refuses actual source/receipt-produced native exports after the source Store and provider are removed: 0 passed/1 failed, no source changes. Only the first export was reached. Initial raw source hashes and log are retained; the original stub source itself was not separately frozen as an artifact.',
        'R2 implementation passes actual import/CLI/malformed/authority/budget/wire cases: 6 passed/1 failed. The remaining traversal fixture mistakenly chose an equal-label concept from a different packet. The implementation correctly returned no relations for that distinct original reference. Final fixture selects the packet-qualified relation endpoint, explicitly selects Alpha/Beta for host grouping and adds asymmetric/two-parent cases. R2 sources/logs are preserved; this failure is not called a production defect.',
        'Final data-only consumers accept native meaning_group_projection_v1 and meaning_join_projection_v1 exports. Review/node/policy/projection identity recipes, graph/packet membership, original directed endpoint mappings, every ordered hyperedge slot, local evidence references and finite independent capacities are checked without a Store or provider.',
        'Imported claims are nested under exportMetadata. All results retain bindingValidation not_replayed, sourceAuthority false, providerAttestation unverified, semanticIdentityEstablished false, modelCalls zero and complete false. Imports cannot construct meaning_join::JoinedGraph or perform captured-source/receipt replay.',
        'Actual fixtures build published native graphs with explicitly owned local Node protocol workers; these are not live model calls or model-quality validation. Exports are consumed only after source Store/provider deletion. Same-label records in distinct packets and two parent graphs remain separate; parallel observations, selfloops, original evidence and repeated ordered role slots remain unchanged.',
        'A coherent omitted-record subset is deliberately admitted as untrusted data, and remains incomplete. Structural consistency and transport hashes cannot detect coherent forgeries, establish complete parent closure or source/model/receipt/accounting truth.',
        'Final nine contracts include CLI exact newline-inclusive output/host-flag admission and no initialization, malformed/duplicate-key/Unicode inputs, byte/count/output/deadline boundaries, authority laundering, corrupted membership/endpoints/roles/evidence/policy, asymmetric directed trace and two actual parent graphs. Fixed maximum constants are not claimed as fully populated maximum-size execution tests.',
        'R4 strengthens only the portable test to close the engine before removing its TempDir and asserts actual absence of the entire original source/Store/provider directory. All runtime/library inputs remain byte-identical to R3, whose429 library tests/7 ignored are retained without claiming a second library run. All final nine portable tests pass.',
        'Expanded archive CLI regressions retain one unchanged Windows fixture setup failure: existing_output_file_directory_and_hardlink_are_never_overwritten cannot create its real symlink (OS1314). R3 Cargo stopped after this target, so the other queued targets were not counted. R4 uses no-fail-fast, executes all four selected regression targets and retains the same one failure. No security fixture was weakened or skipped.',
        'All-target Clippy, format, 17 Python tests, mandatory typecheck and TypeScript 260 passed/3 skipped pass with frozen inputs. The selected native tally retains the known archive OS1314 failure. No whole-native acceptance, full R1-R9 parity, main merge or release is claimed. Semantic alias identity/dedup, mixed document source spaces, cross-history/workspace joins, broader schemas and outcome learning remain open.'],
    'inventoryPreserved':{},'artifacts':[]}
(out/'index.json').write_bytes((json.dumps(index,indent=2,ensure_ascii=False)+'\n').encode());print(json.dumps({'totals':totals,'nativeInputs':final['inputs'],'integratedInputs':integ['frozenInputs']}))
