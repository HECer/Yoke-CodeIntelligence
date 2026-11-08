from pathlib import Path
import json,hashlib
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence')
base=root/'validation/integration-20261018/wrapped-propagation';sha=lambda b:hashlib.sha256(b).hexdigest()
source=json.loads((base/'source-integration.json').read_bytes())
assert source['nativeGroups']==13 and source['nativeTotals']=={'passed':587,'failed':0,'ignored':13}
tracker=root/'docs/parity/native-reconciliation-2026-10-15.json';raw=tracker.read_bytes()
(base/'frozen-tracker.json').write_bytes(raw);old=json.loads(raw);value=json.loads(raw)
r1=next(s for s in value['prioritizedRemainingScopes'] if s['id']=='R1')
paths=[*source['paths'],'docs/validation/integration-2026-10-19/wrapped-propagation/index.json']
for path in paths:
    if path not in r1['evidence']:r1['evidence'].append(path)
addition=' Explicit native safeDelete.propagationPolicy captured_es_function_propagation_v2 now admits plain identifier calls within at most32 parentheses/await/void wrappers when the complete statement belongs directly to a program or block. Original provider/source/dependency/unknown-consumer contracts remain; defaultv1 and public facade schemas are unchanged. Full-result grammar plus bounded original-to-result surviving-node kind/range/order comparison refuses unintended statement merging, including automatic-semicolon boundaries. Optional/type/assertion/value/unclassified forms and lone control-flow slots refuse. Actual TypeScript5.8.3/language-server5.1.3 compiler-only TS/JS and independent permission/approval/application/recovery have owned Windows evidence; source modules never execute. Broader propagation, semantic/runtime/consumer completeness, other declarations/files/languages and full IDE/platform equivalence remain open.'
assert addition not in r1['missingBehavior'];r1['missingBehavior']+=addition
r1['wrappedCallPropagationCheckpoint']={'index':paths[-1],'newCases':source['newCases'],'nativeGroups':13,'nativeTotals':source['nativeTotals'],
    'platform':'Windows','actualTools':source['actualTools'],'defaultPolicyChanged':False,'publicFacadeSchemasChanged':False,
    'survivingSyntaxChecked':True,'legacyPlanShapeOrIdentityFormulaChanged':False,'analyzedModuleExecuted':False,'wholeNativeRerun':False,'fullContractCertified':False}
cap='native_wrapped_call_propagation_v2';assert not any(c['id']==cap for c in value['reconciledCapabilities'])
value['reconciledCapabilities'].append({'id':cap,'behavior':addition.strip(),'evidence':paths,
    'remainingReview':'Finite opt-in native policy only. Defaultv1, published facade arguments and immutable Plan identity formula remain. Preserved syntax structure is not semantic/runtime/dispatch/consumer equivalence; broader R1-R9, all upstream/IDE/platform/whole-native/main/release acceptance remain open.',
    'fullContractCertified':False})
for key in ['inventoryLedger','historicalInventory','ledgerSummary']:assert value[key]==old[key]
tracker.write_text(json.dumps(value,indent=2,ensure_ascii=False)+'\n',encoding='utf-8')
paragraph='''[Explicit wrapped-call propagation](validation/integration-2026-10-19/wrapped-propagation/index.json)
adds Rust API `safeDelete.propagationPolicy: captured_es_function_propagation_v2`.
It admits complete standalone plain calls within at most32 parentheses, `await`
or `void` wrappers, directly in a program or block. Defaultv1 and published
facade schemas stay unchanged. Original provider/source bindings and dependency
closure remain; unknown consumers remain visible. Full-result grammar and
bounded surviving-node kind/nesting/field/order/mapped-byte comparisons refuse unintended
statement merging at automatic-semicolon boundaries. Other expression consumers,
optional/type/assertion/unclassified forms and lone control-flow slots refuse.
See the [contract](../native/docs/propagated-delete.md).

There are **eleven new cases**: one unit, nine default integration and one actual gate.
Final **13 selected native groups** record **587 passed / 0 failed / 13 ignored**,
including **469 / 0 / 7 library tests**, with **737 inputs** unchanged.
Actual **TypeScript5.8.3** and **language-server5.1.3** validate copied TS/JS
results for both v1 and v2, with separately exercised execution permission,
approval, write permission, exact application and recovery. No analyzed module
runs. Format/build/all-target Clippy-Dwarnings pass; mandatory integrated checks
are indexed separately. Counts overlap and are not added to other runs.

The first selected run records26/1/0 because a new test compared independently
created plan IDs/timestamps. Its correction compares bound snapshots and exact
effects while retaining the released identity formula. An intermediate passing
run predates the additional surviving-syntax guard. A fresh regression proves
the statement-merging risk before the guard; every log/source variant remains.
Other propagation/declaration/file/language/provider intents and exhaustive
compiler, consumer, runtime or IDE guarantees remain open. Earlier OS1314 fixture
prerequisites lie outside this selection; no whole-native/platform/main/release
acceptance follows. BroaderR1-R9 work remains active.

'''
p=root/'docs/STATUS.md';s=p.read_text(encoding='utf-8');marker='Windows ZIP and VSIX development packages';assert s.count(marker)==1
p.write_text(s.replace(marker,paragraph+marker),encoding='utf-8',newline='\r\n')
p=root/'docs/ARCHITECTURE.md';s=p.read_text(encoding='utf-8')
s+='''
Explicit native propagation v2 derives complete wrapped-call statement removals
from the same original AST/provider bindings. Before publication, a second
grammar tree must preserve every surviving node kind, order and mapped original
byte range within a shared200000-node budget. This refuses newly merged unrelated
expressions at automatic-semicolon boundaries. Defaultv1, facade schemas,
canonical Plan identity and independent application authority remain. Syntax
correspondence does not establish semantic/runtime equivalence. See the
[contract](../native/docs/propagated-delete.md) and
[evidence](validation/integration-2026-10-19/wrapped-propagation/index.json).
'''
p.write_text(s,encoding='utf-8',newline='\r\n')
p=root/'README.md';s=p.read_text(encoding='utf-8');needle='## TypeScript advanced runtime';assert s.count(needle)==1
s=s.replace(needle,'''Explicit native [wrapped-call propagation](native/docs/propagated-delete.md)
can remove standalone parenthesized, `await` and `void` calls while checking
surviving syntax, original source proofs and guarded application.

'''+needle)
p.write_text(s,encoding='utf-8',newline='\r\n')
print(json.dumps({'documentsUpdated':4,'originalInventoryAndLedgerUnchanged':True,'frozenTrackerSha256':sha(raw)}))
