from pathlib import Path
import json,hashlib
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');base=root/'validation/integration-20261018/function-bindings'
source=json.loads((base/'source-integration.json').read_bytes());totals=source['nativeTotals'];assert totals['failed']==0 and source['nativeGroups']==15
p=root/'docs/parity/native-reconciliation-2026-10-15.json';raw=p.read_bytes();old=json.loads(raw);value=json.loads(raw);(base/'frozen-tracker.json').write_bytes(raw)
scope=next(s for s in value['prioritizedRemainingScopes'] if s['id']=='R1')
evidence=[*source['paths'],'docs/validation/integration-2026-10-19/function-bindings/index.json']
for name in evidence:
    if name not in scope['evidence']:scope['evidence'].append(name)
addition=' Explicit captured_es_function_propagation_v3 now adds single plain const/let/var statements directly initialized by an arrow or anonymous function expression, alongside regular named functions. Existing captured function entity/name/ranges bind the variable_declarator exactly; only its owning statement/export range is removed. Original entity/evidence/analyzer identities remain, v1/v2 boundaries are unchanged, and v2 wrappers/survivingAST plus all provider/source/dependency/budget/host/application safeguards remain. Multiple/pattern/ambient/using/nonfunction/parenthesized/named-inner-expression forms and direct declaration comments refuse; surviving assignments/value consumers cannot be forced away. Actual native CLI/TypeScript5.8.3/language-server5.1.3 TS/JS qualification covers all three keywords, async arrows and Greek identifiers with copied compilation and exact application/recovery. No analyzed module execution, general alias/rebinding/consumer proof or broader R1-R9 certification follows.'
assert addition not in scope['missingBehavior'];scope['missingBehavior']+=addition
scope['functionBindingPropagationCheckpoint']={'index':evidence[-1],'newCases':source['newCases'],'nativeGroups':15,'nativeTotals':totals,
    'platform':'Windows','actualTools':source['actualTools'],'oldPolicyBoundariesChanged':False,'analyzerOrOriginalEntityIdentityChanged':False,
    'legacyPlanShapeOrIdentityFormulaChanged':False,'analyzedModuleExecuted':False,'wholeNativeRerun':False,'fullContractCertified':False}
cap='native_function_binding_propagation_v3';assert not any(c['id']==cap for c in value['reconciledCapabilities'])
value['reconciledCapabilities'].append({'id':cap,'behavior':addition.strip(),'evidence':evidence,
    'remainingReview':'Finite opt-in declaration admission only. V1/v2 retain their original root forms. Owned fixture proofs are distinct from real CLI/compiler/LSP qualification. Other shapes/languages/data declarations, alias/rebinding analysis, complete references/dispatch/consumers, full upstream/IDE/platform/whole-native/main/release and broaderR1-R9 remain open.',
    'fullContractCertified':False})
for key in ['inventoryLedger','historicalInventory','ledgerSummary']:assert old[key]==value[key]
p.write_text(json.dumps(value,indent=2,ensure_ascii=False)+'\n',encoding='utf-8')
paragraph=f'''[Function binding propagation](validation/integration-2026-10-19/function-bindings/index.json)
adds explicit `captured_es_function_propagation_v3` for single plain const/let/var
bindings initialized directly by arrows or anonymous function expressions.
Regular named functions remain supported. Existing function entities bind exact
variable-declarator/name ranges; the owning statement/export supplies the wider
removal range. Original entity/evidence/analyzer identities remain unchanged.
Defaultv1 and explicitv2 keep their previous module boundary. Multiple/pattern/
ambient/using/nonfunction/parenthesized/named-inner-expression forms and direct
declaration comments refuse. Surviving assignments or value consumers cannot be
forced away. See the [contract](../native/docs/propagated-delete.md#explicit-function-binding-policy).

Eleven new cases include ten default integration tests and one actual gate.
Final **15 selected native groups** record **{totals['passed']} passed /
{totals['failed']} failed / {totals['ignored']} ignored**, with **738 inputs** unchanged.
Actual native CLI, **TypeScript5.8.3** and **language-server5.1.3** qualify TS/JS
const/let/var cases, async arrows and Greek names. Copied compilation, independent
permission/approval/application and exact recovery pass without executing source
modules. The existingv2 actual route also passes on the final freeze. Format,
build and all-target Clippy with warnings denied pass; mandatory integrated checks
are indexed separately. Counts overlap and are not added across phases.

The original unsupported-policy regression and all intermediate source/logs remain.
The owned protocol fixture keeps named-function lookup precedence and adds a
variable fallback; it is not a compiler. The now-supportedv3 negative selector
case moves to unsupportedv4. Private cycles, retained/exported dependencies,
argument closure without unused sweeping,16/17declarations,128/129usages,
missing/tampered proofs and automatic-semicolon boundaries are checked.
Source/provider quotas, original deadline and guarded canonical plans remain.
Other declaration/file/language forms, alias/rebinding/general consumer analysis
and full upstream/IDE/whole-native/platform/main/release acceptance remain open.
BroaderR1-R9 remains active.

'''
p=root/'docs/STATUS.md';s=p.read_text(encoding='utf-8');marker='Windows ZIP and VSIX development packages';assert s.count(marker)==1
p.write_text(s.replace(marker,paragraph+marker),encoding='utf-8',newline='\r\n')
p=root/'docs/ARCHITECTURE.md';s=p.read_text(encoding='utf-8');s+='''
Propagation v3 extends the finite module declaration binder to single plain
function-valued const/let/var statements. Existing variable-declarator function
entities and exact name/ranges remain authoritative; a separate owning statement
range supplies keyword/export/semicolon removal. No analyzer or entity identity
is invented. The same original-region dependency closure, provider/source
admission and surviving-AST postcondition feed one ordinary guarded plan. See the
[contract](../native/docs/propagated-delete.md#explicit-function-binding-policy)
and [evidence](validation/integration-2026-10-19/function-bindings/index.json).
''';p.write_text(s,encoding='utf-8',newline='\r\n')
p=root/'README.md';s=p.read_text(encoding='utf-8');needle='## TypeScript advanced runtime';assert s.count(needle)==1
s=s.replace(needle,'''Explicit [function binding propagation](native/docs/propagated-delete.md#explicit-function-binding-policy)
adds single const/let/var arrow and anonymous-function bindings while retaining
original entity/evidence ranges, mandatory provider proofs and guarded plans.

'''+needle);p.write_text(s,encoding='utf-8',newline='\r\n')
print(json.dumps({'documentsUpdated':4,'originalInventoryAndLedgerUnchanged':True,'frozenTrackerSha256':hashlib.sha256(raw).hexdigest()}))
