from pathlib import Path
import json,hashlib,shutil,re,datetime
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence')
base=root/'validation/integration-20261018/plan-feedback'
out=root/'docs/validation/integration-2026-10-19/plan-feedback'
read=lambda n:json.loads((base/n).read_bytes())
sha=lambda b:hashlib.sha256(b).hexdigest()
source=read('source-integration.json');broad=read('final/result.json');final=read('final-r2/result.json');integrated=read('integrated/result.json')
assert integrated['allChecksPassed'] and integrated['sourceUnchanged']
assert final['sourceUnchanged'] and broad['sourceUnchanged']
assert final['commands'][0]['totals']=={'passed':63,'failed':1,'ignored':0}
assert broad['commands'][0]['totals']=={'passed':585,'failed':3,'ignored':7}
assert final['commands'][0]['failures']==['windows_host_aliases_cannot_read_protected_files_or_alternate_streams']
assert all(c['exitCode']==0 for c in final['commands'][1:])
log=(base/'final-r2/native.log').read_text(encoding='utf-8')
assert 'running 14 tests' in log and 'test result: ok. 14 passed; 0 failed; 0 ignored' in log
assert 'code: 1314' in log
ts=(base/'integrated/tests.log').read_text(encoding='utf-8')
assert re.search(r'\bpass\s+260\b',ts) and re.search(r'\bskipped\s+3\b',ts)
assert 'Ran 17 tests' in (base/'integrated/python-contracts.log').read_text(encoding='utf-8')
assert sha((root/'target/debug/yoke.exe').read_bytes())==source['nativeBinarySha256']==read('final/native-test-binary.json')['sha256']
assert next(e for e in read('source-materialization.json') if e['path']=='native/tests/fixtures/postgres-catalog-response.jsonl')['materialization']=='raw_equal'
old=read('frozen-tracker.json');current=json.loads((root/'docs/parity/native-reconciliation-2026-10-15.json').read_bytes())
for key in ['inventoryLedger','historicalInventory','ledgerSummary']:assert old[key]==current[key]
out.mkdir(exist_ok=False)
for file in sorted(base.rglob('*')):
    if file.is_file():
        target=out/file.relative_to(base);target.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(file,target)
runners=out/'runners';runners.mkdir()
for file in sorted(Path(__file__).parent.glob('ci-r9-*')):
    if file.is_file() and file.suffix in ['.py','.rs','.ps1','.md']:shutil.copyfile(file,runners/file.name)
readme=root/'docs/validation/integration-2026-10-19/README.md'
prefix=readme.read_bytes();prior={'bytes':len(prefix),'sha256':sha(prefix)}
index={'schemaVersion':1,'kind':'native_saved_plan_outcome_feedback',
    'asOf':datetime.datetime.now(datetime.timezone.utc).isoformat(),**source,
    'platform':'Windows','nativeInputs':final['inputs'],'integratedInputs':integrated['frozenInputs'],
    'newContractCases':14,'broadAttempt':broad,'finalSelectedRun':final,
    'integrated':integrated,'priorEvidenceReadme':prior,
    'legacyKnowledgeEventSchemaAndIdentityUnchanged':True,
    'executionObserved':False,'workspaceApplicationObserved':False,
    'independentCorroborationEstablished':False,'currentApplicabilityEstablished':False,
    'mainOrReleaseVerified':False,'fullUpstreamContractCertified':False,'wholeNativeRerun':False,
    'notes':[
        'Own MIT separate native_plan_feedback_event_v1 namespace under knowledge::feedback::plans. No dependencies or model/provider calls added. Released knowledge-event/transcript schemas, IDs and existing ranking semantics remain unchanged. CLIplan-record requires independent global--allow-writes, and plan-rank refuses that write flag. Both require an existing Store; host JSON/permission admission precedes source state access.',
        'Bounded128000byte raw stored plan admission precedes full native base/derived View replay and canonical saved-plan replay. Exact plan identity, base/result snapshots, review text, modes and fixed expiry relation are checked. Historical loading supplies no approval or expired-plan application authority. Every source quote identifies original/evaluated side, exact changed-file hash, UTF8 byte boundaries and original CRLF/Unicode bytes. Unchanged and receipt-only source files cannot become plan quote authority.',
        'Opaque checked native pairs carry original overlays, Meaning selections/packets, provider-unverified provenance, coverage and source-scope roles into the bounded result. The evaluated primary scope is a derived planned result; historical disk scopes may also exist as receipt_replay_only. No array position establishes primary authority. Planned bytes and host outcomes/actor/session/time labels prove neither correctness nor authenticated identity, independent corroboration, current applicability, actual application or observed execution. Mutable verification and approval rows are not consumed as execution proof.',
        'Separate immutable event IDs canonicalize ordered quote sets. Perplan/actor/session, sequence slots are unique and timestamps nondecreasing even on out-of-order arrival. Retrospective ranking selects highest eligible sequence once perselectedtarget/actor/session, exposes all older/future events and their dispositions, and uses overflow-safe integer hyperbolic age decay. Explicit targets retain exact View representations; outside events are counted. Selected-plan limit omissions are visible, while budget overflow refuses whole output. No automatic promotion, source edit, capture, project execution or overflow artifact occurs.',
        'Immediate SQLite transactions reject caller transactions on writes and pre-admit source proofs, sequences, quotas and complete output before insertion. Reads preserve pinned caller transactions and SQLite limits. Lost stdout leaves exactly one durable event; retry returns original ID/insertedfalse. Default separateplan quotas10000events/32MBJSON, draft64KB/32quotes/32KBquoteaggregate, source/replay64MB/32pairs and output4MB are independently lowerable. Retention validates all registered plan events and replays their canonicalplan/quote/proof closure before recognizing original snapshots/artifacts/sourceblobs as roots; corrupt records refuse maintenance.',
        'R1 has real compile errors from the local Context name shadowing the anyhow trait import; explicit trait admission fixes compilation. R2 first behavior case passes. R3 has12newcases passing and selected61/1/0; the existing feedback symlink fixture fails at creation withOS1314. R4 has14newcases passing after two additional target/capacity cases. These runs are nonadditive and preserved.',
        'The broad14group finalattempt records585/3/7 with731inputs unchanged:465/0/7library, two unchanged feedback/maintenance symlink setupOS1314 failures, and a new test incorrectly assuming sourceScopes[0] must be primary. The code already uses opaque primary_snapshot binding. Only that test changes to select the explicit primary_evidence role and snapshot ID while preserving historical receipt-only roles. The final4group corrective run records63/1/0 with all14newcases passing; the existing feedbackOS1314failure remains. Both attempts and exact source manifests remain, with no inferred merged totals or whole-native pass.',
        'Format/build/alltargetClippy-Dwarnings pass for both broad and corrected source freezes. Runtime bytes are unchanged across the test-only bridge, as is the exact159080960byte native binary d6c11e8ee3f7d3a5967c84bc94428e0cda5c08a908be390bb1b458256ffab046 observed after native test compilation and again after final build/integrated checks. The731native inputs materialize exactly or CRLF-only in integration, with the2779byte PostgreSQL captured response exactly f67a05349a6cedf55222afd3bb3231c169d382dd4b078d76544daac517530b47. Integrated format/Python17/typecheck/TS260+3skip pass with737inputs unchanged.',
        'An earlier commentary transiently stated13newcases forR3; the actual retained log has12. The final new suite has14 and is reported from the log. No raw counts are rewritten. Historical inventory353/73unreconciled/77backendunknown, false full-contract flags and prior evidence README bytes remain. Additional object targets, semantic transcript learning, independent execution/corroboration, broaderR1-R9, whole-native/platform acceptance, main and release remain open. The full task and coordination actions remain active.'
    ],'artifacts':[]}
assert integrated['frozenInputs']==737
append=f'''\n## Saved-plan outcome feedback checkpoint\n\nSource `{source['sourceCommit']}`, runtime merge `{source['runtimeCommit']}`.\n[Indexed evidence](plan-feedback/index.json) retains14newcases, the broad\n14group585/3/7attempt and corrected4group63/1/0run, including unchangedOS1314\nprerequisite failures and the exact test-only primary-role correction. Native\nsource731/binary proofs, canonical planned-byte/source binding, immutable\nhistory, explicit integer ranking, lost-stdout idempotency, validated retention\nand mandatory integrated format/Python17/typecheck/TS260+3skip with737inputs\nunchanged are recorded. No observed application/execution, independent\ncorroboration, current applicability, whole-native/platform/main/release\ncertification follows. Earlier README bytes remain; broaderR1-R9 continues.\n'''.encode()
readme.write_bytes(prefix+append);assert readme.read_bytes().startswith(prefix)
for file in sorted(out.rglob('*')):
    if file.is_file():
        raw=file.read_bytes();index['artifacts'].append({'path':file.relative_to(out).as_posix(),'bytes':len(raw),'sha256':sha(raw)})
(out/'index.json').write_text(json.dumps(index,indent=2,ensure_ascii=False)+'\n',encoding='utf-8')
print(json.dumps({'artifacts':len(index['artifacts']),'indexSha256':sha((out/'index.json').read_bytes()),'priorEvidenceReadme':prior}),flush=True)
