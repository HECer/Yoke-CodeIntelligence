from pathlib import Path
import json,hashlib
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence')
base=root/'validation/integration-20261018/member-decorators'
source=json.loads((base/'source-integration.json').read_bytes());sha=lambda b:hashlib.sha256(b).hexdigest()
assert source['nativeGroups']==12 and source['nativeTotals']=={'passed':577,'failed':0,'ignored':11}
tracker=root/'docs/parity/native-reconciliation-2026-10-15.json';old_raw=tracker.read_bytes()
(base/'frozen-tracker.json').write_bytes(old_raw);old=json.loads(old_raw);value=json.loads(old_raw)
r1=next(s for s in value['prioritizedRemainingScopes'] if s['id']=='R1')
paths=[*source['paths'],'docs/validation/integration-2026-10-19/member-decorators/index.json']
for path in paths:
    if path not in r1['evidence']:r1['evidence'].append(path)
assert r1['missingBehavior'].count('separate TS member decorators')==1
r1['missingBehavior']=r1['missingBehavior'].replace('separate TS member decorators','unsupported decorated member/name forms')
r1['missingBehavior']+=' Concrete TS/TSX/JS/JSX class methods with plain identifier names now have checked contiguous preceding decorator ownership for native deletion and final provider safeDelete preparation. Up to64 direct sibling decorators and intervening whitespace/comments are admitted; other gap tokens, constructors, abstract/signature members and computed/quoted/private decorated names refuse. Parent/neighbor chains, ordinary earlier comments and original method/entity coordinates stay intact. Whole-result syntax and original source/AST/deadline bounds remain checked. Actual TypeScript5.8.3 compiler/AST plus language-server5.1.3 safeDelete and independent application/recovery have owned Windows evidence in experimentalDecorators mode. No analyzed module/decorator execution, broader consumer/dispatch/runtime proof, other modes/versions or universal IDE equivalence follows.'
r1['memberDecoratorCheckpoint']={'index':paths[-1],'newCases':source['newCases'],
    'nativeGroups':source['nativeGroups'],'nativeTotals':source['nativeTotals'],'platform':'Windows',
    'actualTools':source['actualTools'],'compilerFixtureMode':'experimentalDecorators=true',
    'previouslyRefusedConcreteMemberOwnershipAdded':True,'nativeInputSchemaChanged':False,
    'existingInsertReplaceCoordinatesChanged':False,'legacyPlanShapeOrIdentityChanged':False,
    'analyzedModuleOrDecoratorExecuted':False,'wholeNativeRerun':False,'fullContractCertified':False}
cap='native_concrete_class_method_decorator_ownership'
assert not any(c['id']==cap for c in value['reconciledCapabilities'])
value['reconciledCapabilities'].append({'id':cap,
    'behavior':'Native deleteSymbol and final provider safeDelete preparation now admit a checked owning range for a concrete TS/TSX/JS/JSX class method with a plain identifier name and up to64 preceding direct sibling decorators. Original AST/name/kind/range binding, whitespace-only gaps with admitted comments, full-result syntax replay and source/AST/deadline bounds preserve parent/neighbor chains, earlier comments and original entity/evidence coordinates. Constructors, signature/abstract and computed/quoted/private decorated names or unsupported gaps refuse. NativeMCP, selected compiler/AST and actual language-server safeDelete, independent application gates and exact recovery have owned Windows evidence. No new input/write/execution/approval authority is introduced.',
    'evidence':paths,'remainingReview':'This adds previously refused concrete-method ownership; insert/replace coordinates, ordinary undecorated effects and immutable saved-plan shapes/identity remain. Actual qualification uses TypeScript5.8.3, language-server5.1.3 and experimentalDecorators mode. Other decorator modes/transforms/versions, remaining member/name/language forms, exhaustive references/dispatch/consumer/runtime guarantees, full IDE/upstream/nonWindows/whole-platform/main/release and broaderR1-R9 remain open.',
    'fullContractCertified':False})
for key in ['inventoryLedger','historicalInventory','ledgerSummary']:assert value[key]==old[key]
tracker.write_text(json.dumps(value,indent=2,ensure_ascii=False)+'\n',encoding='utf-8')
status=root/'docs/STATUS.md';text=status.read_text(encoding='utf-8');marker='Windows ZIP and VSIX development packages';assert text.count(marker)==1
paragraph='''[Concrete class-method decorator ownership](validation/integration-2026-10-19/member-decorators/index.json)
adds a checked deletion range for plain-named concrete TS/TSX/JS/JSX methods.
After exact original AST/name/kind/range binding, up to64 contiguous direct
sibling decorators and their whitespace/comment gaps belong to the selected
method. Parent and neighbor decorations, ordinary earlier comments and original
entity/evidence coordinates remain intact. Unsupported gap tokens, constructor,
abstract/signature and computed/quoted/private decorated names refuse. The
entire resulting source is parsed under the original bounds. This adds a
previously refused native and provider-finalization case; no input/authority
schema changes. See the [contract](../native/docs/symbol-deletion.md).

There are **11 new cases**: one unit admission, eight default integration and
two explicitly selected actual gates. Final **12 selected native groups** record
**577 passed / 0 failed / 11 ignored**, including **468 / 0 / 7 library tests**,
with **736 inputs** unchanged. Actual **TypeScript5.8.3** compiler/AST checks all
four extensions and proves parent/neighbor decorator association; actual
**language-server5.1.3** safeDelete produces the same checked source plan.
The fixtures use `experimentalDecorators: true`. Independent permission,
approval, application and exact-recovery checks pass without executing analyzed
modules or decorators. Existing sibling/Python gates are revalidated on the
identical final source. Format/build/all-target Clippy-Dwarnings pass;
mandatory integrated checks are indexed separately.

The initial baseline overlapped a still-running formatter and had changed
source hashes; it is retained as invalid frozen evidence and contributes no
accepted counts. A clean repeated baseline proves the original refusal. An
early negative test assumed every unsupported form had an indexed method;
the corrected test treats a missing entity as an unknown selection. All logs
and source manifests remain. Scoped runs and actual gates are nonadditive.

Saved-plan identity, insert/replace ranges and independent usage, write,
verification, stale-source and recovery contracts remain. Other member/name/
language forms, decorator modes/versions, compiler/consumer/dispatch/runtime
proof and universal IDE parity stay open. Earlier OS1314 prerequisites are
outside this finite selection; no whole-native/platform/main/release acceptance
follows. BroaderR1-R9 work remains active.

'''
status.write_text(text.replace(marker,paragraph+marker),encoding='utf-8',newline='\r\n')
architecture=root/'docs/ARCHITECTURE.md';text=architecture.read_text(encoding='utf-8')
text+='''
Concrete class-method deletion derives an owning decorator range from the same
checked original TS/JS AST binding. Bounded preceding sibling decorators and
intervening comment/whitespace gaps are admitted without rewriting method
coordinates; full-result parsing checks the proposed bytes. Parent/neighbor
chains and undecorated/insert/replace behavior remain. The native and final
safeDelete paths share this source boundary, while provider usage/host authority
and canonical plan/application/recovery remain separate. See the
[contract](../native/docs/symbol-deletion.md) and
[evidence](validation/integration-2026-10-19/member-decorators/index.json).
'''
architecture.write_text(text,encoding='utf-8',newline='\r\n')
readme=root/'README.md';text=readme.read_text(encoding='utf-8');needle='## TypeScript advanced runtime';assert text.count(needle)==1
text=text.replace(needle,'''Checked [class-method decorator ownership](native/docs/symbol-deletion.md)
extends native and provider-finalized deletion while preserving neighboring
decorators, immutable anchors and independent application gates.

'''+needle)
readme.write_text(text,encoding='utf-8',newline='\r\n')
(base/'documentation-proof.json').write_text(json.dumps({'historicalInventoryLedgerAndSummaryUnchanged':True,
    'oldTrackerSha256':sha(old_raw),'currentTrackerSha256':sha(tracker.read_bytes()),'mainOrReleaseVerified':False},indent=2)+'\n',encoding='utf-8')
print(json.dumps({'historicalLedgerUnchanged':True,'newCases':source['newCases'],'nativeTotals':source['nativeTotals']}),flush=True)
