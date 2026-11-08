from pathlib import Path
import json,hashlib
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence')
base=root/'validation/integration-20261018/sibling-declarators'
source=json.loads((base/'source-integration.json').read_bytes());sha=lambda b:hashlib.sha256(b).hexdigest()
assert source['nativeGroups']==11 and source['nativeTotals']=={'passed':568,'failed':0,'ignored':9}
tracker=root/'docs/parity/native-reconciliation-2026-10-15.json';old_raw=tracker.read_bytes()
(base/'frozen-tracker.json').write_bytes(old_raw);old=json.loads(old_raw);value=json.loads(old_raw)
r1=next(s for s in value['prioritizedRemainingScopes'] if s['id']=='R1')
paths=[*source['paths'],'docs/validation/integration-2026-10-19/sibling-declarators/index.json']
for path in paths:
    if path not in r1['evidence']:r1['evidence'].append(path)
assert r1['missingBehavior'].count('Multi-declarators,')==1
r1['missingBehavior']=r1['missingBehavior'].replace('Multi-declarators,',
    'Multi-declarators without the explicit native sibling-preservation policy,')
r1['missingBehavior']+=' Separate native deleteSymbol.siblingDeclarators reject/preserve now admits one simple function-valued TS/TSX/JS/JSX variable declarator in a statement const/let/var. Two exact AST-derived byte edits remove that node and one direct parent comma while preserving other declarators, wrapper/semicolon and whitespace. Ambiguous direct comments, loop/nonstatement bindings, named expressions, other languages, mixed operations and combined policies refuse. Full resulting syntax and original source/AST/deadline bounds remain checked; absent/reject and semantic-provider/facade defaults stay strict. Actual TypeScript5.8.3 compiler-only copied-result verification for all four extensions and independent permission/approval/application/recovery have owned Windows evidence. No analyzed module execution, consumer safety, exhaustive semantic/IDE or other-platform guarantee follows.'
r1['siblingDeclaratorsCheckpoint']={'index':paths[-1],'newCases':source['newCases'],
    'nativeGroups':source['nativeGroups'],'nativeTotals':source['nativeTotals'],'platform':'Windows',
    'nativePolicy':'deleteSymbol.siblingDeclarators=reject|preserve','actualCompiler':source['actualCompiler'],
    'actualExistingPythonPolicyRevalidated':True,'defaultPolicyChanged':False,
    'semanticProviderFacadePolicyChanged':False,'legacyPlanShapeOrIdentityChanged':False,
    'analyzedModuleExecuted':False,'wholeNativeRerun':False,'fullContractCertified':False}
cap='native_ts_js_preserved_sibling_declarators'
assert not any(c['id']==cap for c in value['reconciledCapabilities'])
value['reconciledCapabilities'].append({'id':cap,
    'behavior':'Explicit singleton native deleteSymbol.siblingDeclarators reject/preserve preserves strict defaults and admits a checked simple TS/TSX/JS/JSX function-valued variable declarator in a const/let/var statement. Full native AST/name/kind/range/source binding selects the declaration and exactly one direct comma as separate original-coordinate byte edits. Other declarations, export/ambient wrappers, semicolon and all unrelated bytes remain intact; full resulting syntax is checked under original source/AST/deadline bounds. Closed validation rejects ambiguous delimiter comments, loop/nonstatement targets, named expressions/methods, other languages, mixed edits and combined policies. NativeMCP, actual TypeScript5.8.3 compiler-only verification for all four extensions and independent permission/approval/apply/exact recovery have owned Windows evidence.',
    'evidence':paths,'remainingReview':'No native default, semantic-provider/facade behavior, saved-plan schema/identity or write/execution/approval/recovery authority is changed. Syntax/source operations do not establish unusedness, compiler reference completeness, runtime behavior or consumer safety. Direct inter-declarator comments and other binding/container shapes remain unsupported. Full upstream/IDE/all-language/nonWindows/platform/main/release and broaderR1-R9 remain open.',
    'fullContractCertified':False})
for key in ['inventoryLedger','historicalInventory','ledgerSummary']:assert value[key]==old[key]
tracker.write_text(json.dumps(value,indent=2,ensure_ascii=False)+'\n',encoding='utf-8')
status=root/'docs/STATUS.md';text=status.read_text(encoding='utf-8');marker='Windows ZIP and VSIX development packages';assert text.count(marker)==1
paragraph='''[Explicit TS/JS sibling-declarator deletion](validation/integration-2026-10-19/sibling-declarators/index.json)
adds closed native `deleteSymbol.siblingDeclarators: reject|preserve`. The
absent/reject default remains strict. Explicit preservation binds one simple
function-valued TS/TSX/JS/JSX variable in a statement `const`, `let` or `var`,
then removes that original AST node and one direct parent comma as two separate
byte edits. Other declarators, export/ambient wrappers, semicolon, whitespace
and original entity/evidence coordinates stay intact. Nested commas are not
delimiters. Ambiguous direct comments, unsupported bindings/containers, mixed
edits and combined policies refuse. Full-result syntax replay retains the
source/AST/deadline bounds. See the [contract](../native/docs/symbol-deletion.md).

There are **10 new cases**: one unit admission, eight default integration and
one explicitly selected actual-compiler gate. Final **11 selected native groups**
record **568 passed / 0 failed / 9 existing ignored gates**, including
**467 / 0 / 7 library tests**, with **734 inputs** unchanged. Actual
**TypeScript5.8.3 / Node24.21.0** compile the copied TS, JS, TSX and JSX results,
then separate permission/approval/application/recovery checks preserve exact
BOM/Unicode/CRLF source. No analyzed module executes. The previously implemented
Python AST-only application/recovery gate is revalidated on the identical final
source. Format/build/all-target Clippy with warnings denied pass; mandatory
integrated format/Python/typecheck/TypeScript results are indexed separately.

Earlier 15/0/1 and 26/0/1 selected runs and the initial actual compiler gate stay
separate and nonadditive. Native MCP prepares the exact effect without source
write or execution authority. Saved plan shapes/identities, semantic-provider/
facade defaults and independent application gates remain. Other source shapes,
comment attachments, consumer safety and general semantic/IDE equivalence stay
open. Earlier OS1314 prerequisites lie outside this finite selection; no
whole-native/platform acceptance follows. BroaderR1-R9, main and release remain open.

'''
status.write_text(text.replace(marker,paragraph+marker),encoding='utf-8',newline='\r\n')
architecture=root/'docs/ARCHITECTURE.md';text=architecture.read_text(encoding='utf-8')
text+='''
Native declaration deletion has a separate explicit sibling-preservation policy
for simple TS/JS function-valued variables. The opaque original AST binding
selects a statement declarator and a direct parent comma; independent byte
edits preserve delimiter trivia and all surviving declaration bytes. Ambiguous
direct comments refuse. Full-result parsing, closed singleton policy validation
and ordinary canonical saved-plan/application gates remain. Python and default/
semantic-provider policies retain their existing contracts. See the
[contract](../native/docs/symbol-deletion.md) and
[evidence](validation/integration-2026-10-19/sibling-declarators/index.json).
'''
architecture.write_text(text,encoding='utf-8',newline='\r\n')
readme=root/'README.md';text=readme.read_text(encoding='utf-8');needle='## TypeScript advanced runtime';assert text.count(needle)==1
text=text.replace(needle,'''Explicit native [TS/JS sibling preservation](native/docs/symbol-deletion.md)
can delete one checked function-valued declarator and its AST comma while
keeping unrelated declarations and the strict default intact.

'''+needle)
readme.write_text(text,encoding='utf-8',newline='\r\n')
(base/'documentation-proof.json').write_text(json.dumps({'historicalInventoryLedgerAndSummaryUnchanged':True,
    'oldTrackerSha256':sha(old_raw),'currentTrackerSha256':sha(tracker.read_bytes()),'mainOrReleaseVerified':False},indent=2)+'\n',encoding='utf-8')
print(json.dumps({'historicalLedgerUnchanged':True,'newCases':source['newCases'],'nativeTotals':source['nativeTotals']}),flush=True)
