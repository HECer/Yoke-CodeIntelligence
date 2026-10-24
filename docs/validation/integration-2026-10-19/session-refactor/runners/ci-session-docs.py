from pathlib import Path
import json,hashlib
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');base=root/'validation/integration-20261018/session-refactor'
source=json.loads((base/'source-integration.json').read_bytes());totals=source['nativeTotals']
assert totals['failed']==0 and source['nativeGroups']==15
tracker=root/'docs/parity/native-reconciliation-2026-10-15.json';raw=tracker.read_bytes();old=json.loads(raw);value=json.loads(raw)
(base/'frozen-tracker.json').write_bytes(raw)
evidence=[*source['paths'],'docs/validation/integration-2026-10-19/session-refactor/index.json']
addition=' Native Session refactor.prepare now exposes existing safeDelete/rename/inline/move planning through the session CLI and native ci_session. A closed operation/arguments envelope,64000byte inner input bound, editing/planning and host-project restrictions, exact persisted-session admission check and original deadline precede provider work. Delegated native arguments/results, source/provider/snapshot preconditions and guarded application remain intact; no session steering, source writes, model/runtime grant or auto-approval is added. Native eight-tool and published facade argument schemas remain; nativeSession description advertises the route. Actual native CLI with TS5.8.3/LSP5.1.3 validates TS/JS safeDeletev2 plus compiler-only verification and separate approval/write/stale/recovery. Other provider intents, clients/platforms and broader R1-R9 remain open.'
for scope_id in ['R1','R6']:
    scope=next(s for s in value['prioritizedRemainingScopes'] if s['id']==scope_id)
    assert addition not in scope['missingBehavior'];scope['missingBehavior']+=addition
    for name in evidence:
        if name not in scope['evidence']:scope['evidence'].append(name)
    scope['nativeSessionRefactorCheckpoint']={'index':evidence[-1],'newCases':source['newCases'],'nativeGroups':15,
        'nativeTotals':totals,'platform':'Windows','actualTools':source['actualTools'],
        'existingNativeArgumentsAndPlanIdentityChanged':False,'publishedFacadeArgumentSchemasChanged':False,
        'sessionSteeredOrOriginalSourceWritten':False,'approvalOrExecutionAuthorityAdded':False,'wholeNativeRerun':False,'fullContractCertified':False}
cap='native_session_semantic_refactor_preparation';assert not any(c['id']==cap for c in value['reconciledCapabilities'])
value['reconciledCapabilities'].append({'id':cap,'behavior':addition.strip(),'evidence':evidence,
    'remainingReview':'The stored session is checked at admission; there is no lease over future mode/source/consumer changes. Native publication retains its existing nontransactional boundary. Actual qualification exercises safeDeletev2 via native CLI; other provider intents and client installations retain separate evidence. No exhaustive compiler/runtime/consumer, full upstream/IDE/nonWindows/whole-native/platform/main/release acceptance follows.',
    'fullContractCertified':False})
for key in ['inventoryLedger','historicalInventory','ledgerSummary']:assert old[key]==value[key]
tracker.write_text(json.dumps(value,indent=2,ensure_ascii=False)+'\n',encoding='utf-8')
paragraph=f'''[Native session refactoring](validation/integration-2026-10-19/session-refactor/index.json)
makes existing `safeDelete`, `rename`, `inline` and `move` plans available through
`session --requests` and native `ci_session` with `refactor.prepare`.
The closed envelope accepts native arguments up to64,000 serialized UTF-8 bytes.
Mode, host-project and exact stored-session admission checks precede provider
work. Another client's steering requires resuming the session. The original
deadline, source/provider/snapshot bindings and native results remain; preparation
does not steer the session or write original source. Approval, executed
verification, workspace permission, stale-source checks and recovery remain
separate. See the [contract](../native/docs/agent-sessions.md#native-semantic-planning).

Ten new cases include nine default integration tests and one explicit actual
gate. Final **15 selected native groups** record **{totals['passed']} passed /
{totals['failed']} failed / {totals['ignored']} ignored**, with **738 inputs** unchanged.
Actual native CLI bytes are hashed before/after use and match the final binary.
Actual **TypeScript5.8.3** / **language-server5.1.3** validate TS/JS safeDeletev2,
copied compiler-only results and independent approval/verification/write/stale/
recovery gates. No analyzed module runs. Format/build/all-target Clippy with
warnings denied pass; mandatory integrated checks are indexed separately.
The initial unsupported-operation regression, all source variants and scoped
results remain; counts overlap and are not additive.

The normal native eight-tool and published facade argument schemas remain;
the native session tool description advertises the added operation. Current
session validation is an admission check, with no lease over future state or
source changes. Native plan identity/publication behavior remains unchanged.
Other provider intents, client installations, exhaustive compiler/consumer/
runtime guarantees and broaderR1-R9 remain open. Prior OS1314 prerequisites lie
outside this selection; no whole-native/platform/upstream/main/release acceptance
follows.

'''
p=root/'docs/STATUS.md';s=p.read_text(encoding='utf-8');marker='Windows ZIP and VSIX development packages';assert s.count(marker)==1
p.write_text(s.replace(marker,paragraph+marker),encoding='utf-8',newline='\r\n')
p=root/'docs/ARCHITECTURE.md';s=p.read_text(encoding='utf-8');s+='''
Native `Session::invoke` admits a closed `refactor.prepare` envelope before
delegating unchanged arguments to the semantic planning core. A bounded checked
read compares the persisted session record with the opened record before provider
work; no transaction is held over provider execution. Existing session CLI and
native session MCP dispatch share this route, with the same mode/project/source
guards and original deadline. Native plan publication and independent approval,
verification, write and recovery remain. See the
[contract](../native/docs/agent-sessions.md#native-semantic-planning) and
[evidence](validation/integration-2026-10-19/session-refactor/index.json).
''';p.write_text(s,encoding='utf-8',newline='\r\n')
p=root/'README.md';s=p.read_text(encoding='utf-8');needle='## TypeScript advanced runtime';assert s.count(needle)==1
s=s.replace(needle,'''Native [session refactoring](native/docs/agent-sessions.md#native-semantic-planning)
prepares existing semantic plans through the session CLI and MCP interface, with
mode, project and current-session checks and separate application authority.

'''+needle);p.write_text(s,encoding='utf-8',newline='\r\n')
print(json.dumps({'documentsUpdated':4,'originalInventoryAndLedgerUnchanged':True,'frozenTrackerSha256':hashlib.sha256(raw).hexdigest()}))
