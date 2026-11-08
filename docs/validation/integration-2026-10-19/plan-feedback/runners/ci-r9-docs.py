from pathlib import Path
import json,hashlib
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence')
base=root/'validation/integration-20261018/plan-feedback'
source=json.loads((base/'source-integration.json').read_bytes())
sha=lambda b:hashlib.sha256(b).hexdigest()
tracker=root/'docs/parity/native-reconciliation-2026-10-15.json'
old_raw=tracker.read_bytes();(base/'frozen-tracker.json').write_bytes(old_raw)
old=json.loads(old_raw);value=json.loads(old_raw)
r9=next(s for s in value['prioritizedRemainingScopes'] if s['id']=='R9')
paths=[*source['paths'],'docs/validation/integration-2026-10-19/plan-feedback/index.json']
for path in paths:
    if path not in r9['evidence']:r9['evidence'].append(path)
r9['missingBehavior']='Native knowledge outcomes and separate authored historical saved-plan outcomes now have bounded Windows evidence. The plan path replays canonical saved-plan base/derived Views, exact changed-file side quotes and original receipt/source closures; it retains ordered immutable events, explicit integer ranking, zero execution/application/current-applicability authority and validated retention roots. Additional object targets and semantic transcript understanding beyond literal matching remain open. Independent corroboration and observed execution truth are not established by host labels, supplied outcomes, approval rows or mutable verification records.'
r9['suggestedScope']='Continue additional typed targets and explicit semantic transcript candidates with original source-space and receipt replay. Preserve the separate released knowledge schema and saved-plan format, immutable history, transparent arithmetic and independent write/approval/recovery gates. Planned result bytes never establish actual application/execution/current applicability; do not replace central AgentMemory or invent independent identity/corroboration.'
r9['planFeedbackCheckpoint']={'index':paths[-1],'newCases':14,'platform':'Windows',
    'recordKind':'native_plan_feedback_event_v1','legacyKnowledgeFormatUnchanged':True,
    'finalSelectedGroups':source['finalSelectedGroups'],'finalSelectedTotals':source['finalSelectedTotals'],
    'broadNativeGroups':source['broadNativeGroups'],'broadNativeTotals':source['broadNativeTotals'],
    'broadFailures':'Two unchanged Windows symlink fixture setup OS1314 failures and one corrected new test assumption about source-scope ordering; retained without count substitution.',
    'testOnlyCorrection':'Select the explicit primary_evidence role and snapshot identity, preserve receipt_replay_only scopes.',
    'productionBinaryUnchangedFromBroadNativeTests':True,'executionObserved':False,
    'workspaceApplicationObserved':False,'independentCorroborationEstablished':False,
    'currentApplicabilityEstablished':False,'wholeNativeRerun':False,'fullContractCertified':False}
cap='native_saved_plan_outcome_feedback'
assert not any(c['id']==cap for c in value['reconciledCapabilities'])
value['reconciledCapabilities'].append({'id':cap,
    'behavior':'Explicit feedback plan-record/plan-rank bind authored historical outcomes to canonical saved change plans, fully replayed native base/derived View proofs and exact changed-file origin/evaluated source quotes. Ordered immutable plan/actor/session label events are idempotent; retrospective ranking retains all selected events, visible future/superseded dispositions and overflow-safe integer hyperbolic decay. Separate event schema preserves released knowledge IDs. All input/source/pair/event/output quotas, immediate output-before-insert publication, late stdout loss and validated retention roots remain explicit. No model/provider/capture/application/execution or approval occurs.',
    'evidence':paths,'remainingReview':'Host labels and outcomes do not authenticate independent identity, correctness or execution. Derived results represent planned bytes; current applicability, actual application and observed execution remain false even with supplied verification/approval rows. Plan JSON reader is limited to128000bytes; additional object targets and semantic transcript interpretation remain open.14new cases pass; the broader failed attempt and final targeted63/1 Windows symlink prerequisite failure are retained, not relabeled as whole-native acceptance. Full upstream/platform/main/release and broaderR1-R9 remain open.',
    'fullContractCertified':False})
for key in ['inventoryLedger','historicalInventory','ledgerSummary']:assert value[key]==old[key]
tracker.write_text(json.dumps(value,indent=2,ensure_ascii=False)+'\n',encoding='utf-8')
status=root/'docs/STATUS.md';text=status.read_text(encoding='utf-8')
marker='Windows ZIP and VSIX development packages';assert text.count(marker)==1
paragraph='''[Saved-plan outcome feedback](validation/integration-2026-10-19/plan-feedback/index.json)
adds explicit `feedback plan-record` and `plan-rank` over existing native Stores.
Canonical saved-plan replay, full base/derived View proofs and exact changed-file
side quotes bind authored historical outcomes to planned source bytes. A separate
event schema preserves released knowledge-event identities. Ordered immutable
history, visible integer decay/session selection, output-before-insert SQL
publication and validated retention roots retain their explicit budgets.
See the [contract](../native/docs/outcome-feedback.md#authored-outcomes-for-saved-change-plans).

All **14 new cases pass**. The broad **14-group** run records **585 passed /
3 failed / 7 ignored**: two unchanged Windows symlink fixture setup failures
(OS1314), plus a new test that assumed primary source-scope ordering. The test
now selects the explicit role and snapshot ID. The corrected **4-group** run
records **63 passed / 1 failed / 0 ignored**, with the existing feedback symlink
prerequisite still failing. Format, build and all-target Clippy with warnings
denied pass. The library's **465 passed / 0 failed / 7 ignored** remains bound
to identical production sources; only the test changes across the recorded
bridge, and the exact production binary is unchanged. The scoped runs are
nonadditive; mandatory integrated checks are indexed separately.

Planned results never establish actual application, execution, independent
corroboration or current applicability. Mutable verification/approval records
are not consumed as proof. No model, provider, capture or project execution
occurs in these operations. Additional targets, semantic transcript learning,
broaderR1-R9, whole-native/platform acceptance, main and release remain open.

'''
status.write_text(text.replace(marker,paragraph+marker),encoding='utf-8',newline='\r\n')
architecture=root/'docs/ARCHITECTURE.md';text=architecture.read_text(encoding='utf-8')
text+='''
Saved-plan feedback uses a separate immutable event kind beneath the existing
knowledge feedback module. It admits bounded stored plan JSON, replays the
canonical plan and both native View/source closures, and checks exact quotes
against changed-file hashes on their explicitly selected original/planned side.
Immediate SQL publication follows sequence, source, quota and whole-output
admission; reads retain the pinned transaction. Historical integer ranking and
retention replay preserve original receipt/source roles. Plan results remain
planned bytes and no approval, execution/application receipt or current-workspace
authority follows. See the [contract](../native/docs/outcome-feedback.md) and
[indexed evidence](validation/integration-2026-10-19/plan-feedback/index.json).
'''
architecture.write_text(text,encoding='utf-8',newline='\r\n')
readme=root/'README.md';text=readme.read_text(encoding='utf-8');needle='## TypeScript advanced runtime'
assert text.count(needle)==1
text=text.replace(needle,'''Explicit [saved-plan feedback](native/docs/outcome-feedback.md#authored-outcomes-for-saved-change-plans)
records source-bound authored historical outcomes and exposes transparent
ranking. Planned result bytes and labels grant no application or execution authority.

'''+needle)
readme.write_text(text,encoding='utf-8',newline='\r\n')
(base/'documentation-proof.json').write_text(json.dumps({'historicalInventoryLedgerAndSummaryUnchanged':True,
    'oldTrackerSha256':sha(old_raw),'currentTrackerSha256':sha(tracker.read_bytes()),
    'mainOrReleaseVerified':False},indent=2)+'\n',encoding='utf-8')
print(json.dumps({'historicalLedgerUnchanged':True,'newCases':14,'runtimeCommit':source['runtimeCommit']}),flush=True)
