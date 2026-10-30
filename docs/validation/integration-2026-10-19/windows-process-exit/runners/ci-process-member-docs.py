from pathlib import Path
import json,re,hashlib
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');out=root/'docs/validation/integration-2026-10-19/windows-process-exit';index=json.loads((out/'index.json').read_bytes());tracker=root/'docs/parity/native-reconciliation-2026-10-15.json';assert tracker.read_bytes()==(out/'frozen-tracker.json').read_bytes();old=json.loads(tracker.read_bytes());value=json.loads(tracker.read_bytes());t=index['nativeTotals']
assert re.search(r'\bpass\s+260\b',(out/'integrated/tests.log').read_text(encoding='utf-8'));assert 'Ran 17 tests' in (out/'integrated/python-contracts.log').read_text(encoding='utf-8')
paths=[*index['paths'],'docs/validation/integration-2026-10-19/windows-process-exit/index.json'];r8=next(x for x in value['prioritizedRemainingScopes'] if x['id']=='R8')
for path in paths:
 if path not in r8['evidence']:r8['evidence'].append(path)
r8['ownedProcessCheckpoint']={'index':paths[-1],'newContractCases':2,'targetedCases':17,'finalSelectedTotals':t,'actualWindowsPostgresTlsAndFrozenPdfWorker':True,'sharedExistingCleanupBudget':True,'maxObservedMembersPerCleanup':4096,'pidBasedTermination':False,'kernelCauseCertified':False,'wholeNativeRerun':False,'fullContractCertified':False}
assert not any(x['id']=='native_windows_owned_member_exit_proof' for x in value['reconciledCapabilities'])
value['reconciledCapabilities'].append({'id':'native_windows_owned_member_exit_proof','behavior':'Windows native process cleanup captures up to4096 nested-job member synchronization handles before requesting termination, and waits for their signals alongside the separately held root and empty job accounting within the existing cleanup budget. Incomplete/unavailable/over-budget member admission refuses proof while still requesting owned termination. No PID-based kill; recycled PIDs can cause only conservative waiting/unproven cleanup. Tests retain original liveness/time/SQLite/source restoration requirements and report original cleanup outcomes on failure.','evidence':paths,'remainingReview':'This bounded observed-member/OS proof is not an OS sandbox, a claim about every historical transient process, kernel API hard preemption or full upstream/non-Windows/Windows-suite acceptance. Earlier full-native failures remain; unattended App deployment/main/release and wider R1-R9 stay open.','fullContractCertified':False})
pg=next(x for x in value['reconciledCapabilities'] if x['id']=='native_postgres_received_catalog');pg['remainingReview']='Bounded PostgreSQL selected-catalog capture/frozen queries have actual Windows PostgreSQL18.6/psql18.6 evidence, now revalidated with the held-member process guard. Public HTTP and dedicated arxiv received-source routes are separately checkpointed. Full pinned upstream schema/definition equivalence, other server/client versions, non-Windows qualification and mixed semantic/source integration remain unverified. Google shortcut export, authenticated/social/video and Neo4j/FalkorDB upsert remain open; local readers/generated Cypher are not those connectors.'
assert all(value[k]==old[k] for k in ['inventoryLedger','historicalInventory','ledgerSummary']);tracker.write_bytes((json.dumps(value,indent=2,ensure_ascii=False)+'\n').encode())
paragraph=f'''[Windows owned-member exit proof](validation/integration-2026-10-19/windows-process-exit/index.json)
now strengthens explicit process cleanup. It retains up to 4,096 observed nested
job members' synchronization handles before termination and requires their final
signals, the held root's exit and empty job accounting within the same existing
cleanup budget. Incomplete or unavailable member admission still requests owned
termination and reports missing proof. No PID-based termination is introduced.

The diagnostic RED reproduces a nonsignaled owned process at normal deadline
return with `cleanupUnproven=false`. The new guard passes **17 targeted cases**,
including two new OS permission/ownership contracts. Final selected native
results are **{t['passed']} passed / {t['failed']} failed / {t['ignored']} existing ignored gates**;
the sole unchanged media symlink setup failure remains OS1314. All-target Clippy,
format, mandatory typecheck, **260 TypeScript tests / 3 skips**, and **17 Python
tests** pass with **{index['integratedInputs']} integrated inputs unchanged**. Actual Windows
PostgreSQL TLS/SCRAM/Unicode capture, cleanup of all three fixture credentials,
lost-stdout identity and frozen native W3PDF worker replay pass on the new binary.

The earlier eight same-binary and four temporary-trace passes are nonadditive.
Temporary tracing is absent from final code. This observed-member/OS proof does
not certify a kernel scheduling cause, every historical transient process,
arbitrary kernel API preemption, full Windows/non-Windows/upstream acceptance,
unattended App deployment, main or release. Wider STATUS/R1–R9 remains open.

'''
status=root/'docs/STATUS.md';raw=status.read_text(encoding='utf-8');marker='Windows ZIP and VSIX development packages';assert raw.count(marker)==1;raw=raw.replace(marker,paragraph+marker);status.write_text(raw,encoding='utf-8',newline='\r\n')
architecture=root/'docs/ARCHITECTURE.md';raw=architecture.read_text(encoding='utf-8');raw+='\nWindows explicit process cleanup holds bounded OS-reported member synchronization\nhandles before termination and checks their final signals alongside the held root\nand empty job accounting in the existing cleanup budget. Missing proof remains\nan explicit failure; no PID-based termination is authorized. See the\n[owned-member checkpoint](validation/integration-2026-10-19/windows-process-exit/index.json).\n';architecture.write_text(raw,encoding='utf-8',newline='\r\n')
readme=root/'docs/validation/integration-2026-10-19/README.md';prefix=readme.read_bytes();p=index['priorEvidenceReadme'];assert len(prefix)==p['bytes'] and hashlib.sha256(prefix).hexdigest()==p['sha256'];append=f'''\n## Windows held-member cleanup checkpoint\n\nSource `{index['sourceCommit']}`, runtime merge `{index['runtimeCommit']}`.\n[Indexed evidence](windows-process-exit/index.json) retains the diagnostic3/1RED,\nnonadditive eight baseline/four trace repetitions, bounded member-handle proof,\n17 targeted cases/two new OS contracts, selected{t['passed']}/{t['failed']}/{t['ignored']}, actual\nPostgreSQL TLS/frozen-PDF-worker receipts and mandatory integrated checks.\nOriginal failures remain; no kernel-cause/full-native/platform/main/release\ncertification follows. All preceding README bytes are preserved.\n'''.encode();readme.write_bytes(prefix+append);assert readme.read_bytes().startswith(prefix)
print(json.dumps({'historicalLedgerUnchanged':True,'nativeTotals':t,'readmePrefixPreserved':p}))
