from pathlib import Path
import json,hashlib,shutil,re,datetime
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence')
base=root/'validation/integration-20261018/wrapped-propagation';out=root/'docs/validation/integration-2026-10-19/wrapped-propagation'
read=lambda n:json.loads((base/n).read_bytes());sha=lambda b:hashlib.sha256(b).hexdigest()
source=read('source-integration.json');final=read('final-r3/result.json');actual=read('live-final-r3/result.json');integrated=read('integrated/result.json')
assert all(r['allChecksPassed'] and r['sourceUnchanged'] for r in [final,actual,integrated])
assert source['nativeGroups']==13 and source['nativeTotals']=={'passed':587,'failed':0,'ignored':13}
assert final['inputs']==737 and integrated['frozenInputs']==743
assert source['newCases']=={'unitAdmission':1,'defaultIntegration':9,'actualCompilerAndLanguageServer':1}
assert actual['commands'][0]['totals']=={'passed':2,'failed':0,'ignored':0}
assert sha((root/'target/debug/yoke.exe').read_bytes())==source['nativeBinarySha256']
assert source['actualTools']['compilerVersion']=='Version 5.8.3' and source['actualTools']['serverVersion']=='5.1.3'
assert read('baseline/result.json')['sourceUnchanged'] and read('baseline/result.json')['commands'][0]['totals']=={'passed':0,'failed':1,'ignored':0}
assert read('asi-baseline/result.json')['sourceUnchanged'] and read('asi-baseline/result.json')['commands'][0]['totals']=={'passed':0,'failed':1,'ignored':0}
for name in source['paths']:assert sha((base/'final-r3/source'/name).read_bytes())==read('final-r3/source-before.json')[name],name
ts=(base/'integrated/tests.log').read_text(encoding='utf-8')
assert re.search(r'\bpass\s+260\b',ts) and re.search(r'\bskipped\s+3\b',ts)
assert 'Ran 17 tests' in (base/'integrated/python-contracts.log').read_text(encoding='utf-8')
old=read('frozen-tracker.json');current=json.loads((root/'docs/parity/native-reconciliation-2026-10-15.json').read_bytes())
for key in ['inventoryLedger','historicalInventory','ledgerSummary']:assert old[key]==current[key]
out.mkdir(exist_ok=False)
for file in sorted(base.rglob('*')):
    if file.is_file():
        target=out/file.relative_to(base);target.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(file,target)
runners=out/'runners';runners.mkdir()
for file in sorted(Path(__file__).parent.glob('ci-wrapped-*')):
    if file.is_file() and file.suffix in ['.py','.rs','.md','.ps1']:shutil.copyfile(file,runners/file.name)
readme=root/'docs/validation/integration-2026-10-19/README.md';prefix=readme.read_bytes();prior={'bytes':len(prefix),'sha256':sha(prefix)}
index={'schemaVersion':1,'kind':'native_explicit_wrapped_call_propagation_v2',
    'asOf':datetime.datetime.now(datetime.timezone.utc).isoformat(),**source,'platform':'Windows',
    'nativeInputs':final['inputs'],'integratedInputs':integrated['frozenInputs'],'finalNative':final,
    'actualCompilerAndLanguageServer':actual,'integrated':integrated,'priorEvidenceReadme':prior,
    'defaultPolicyChanged':False,'publicFacadeSchemasChanged':False,'legacyPlanShapeOrIdentityFormulaChanged':False,
    'survivingSyntaxChecked':True,'analyzedModuleExecuted':False,'nonWindowsRuntimeValidated':False,
    'wholeNativeRerun':False,'fullUpstreamContractCertified':False,'mainOrReleaseVerified':False,
    'notes':[
        'Own MIT explicit native safeDelete.propagationPolicy=captured_es_function_propagation_v2; requires propagate:true and nonempty symbolname. Absent or explicitv1 retains released effects and identity formula. Published facade schemas and permission/source/provider boundaries are unchanged. Unknown/null/nontext policies or incompatible selections refuse before indexing/provider startup.',
        'One bounded captured TS/JS ES named-function module retains every exact provider definition/reference, AST/entity/name/range/source binding, context, conservative captured candidate and dependency/cycle/export safeguard. Up to32 combined parenthesized/await/void wrappers around a plain identifier call may form a complete statement directly in program/statement_block. Optional/type/assertion/non-null/parenthesized-callee/value/unclassified forms, wrapper comments and lone control-flow slots refuse. One deduplicated original-coordinate plan retains complete:false/exhaustiveCompilerProof:false/remainingUsagesMayBreak:true.',
        'The proposed source is reparsed with the original deadline and256000byte bound. A private surviving-syntax comparison omits only fully removed original subtrees and compares every retained named/anonymous node kind, preorder and offset after mapped removals against the result. A shared200000visited-node bound, sorted nonoverlapping spans and prefix-offset lookup bound traversal/work. This rejects unrelated statement merging at automatic-semicolon boundaries without adding punctuation or changing immutable entity/evidence coordinates. Syntax correspondence is not semantic/runtime/dispatch/consumer completeness.',
        'Eleven new distinct cases:1unit+9default integration+1explicit actual gate. Tables cover both analyzer epochs, BOM/nonBMP/CRLF, original byte spans/comments, dependency retention/nested-call deduplication, explicit/defaultv1 effects, unknown policy/selection rejection before provider start, missing/malformed/omitted/truncated/foreign/source-tampered proofs, wrapper32/33 admission, unsupported expression/control slots, automatic-semicolon boundary refusal and all-declarations-removed empty output. Earlier tests and actualv1 gate remain.',
        'Initial baseline0/1/0 reproduces unknown native policy with unchanged sources. R1selected26/1/0 has only a new test comparing independently created Plan IDs/createdAt/expiresAt. It is corrected to compare source/result/workspace bindings, exact files/diff/warnings and shape; no Plan implementation/formula is changed. First actual2/0/0 and intermediate13groups584/0/13 predate the additional syntax guard. A fresh asi-baseline0/1/0 reproduces accepted unrelated expression merging before the guard; all failed/intermediate logs/manifests are retained and nonadditive.',
        'Initial guard-green6/2/0 correctly rejects statement merging but also refuses two permitted leading-declaration removals. An isolated unit ast-probe0/1/0 diagnoses only the program-container start: mapped original0 versus parsed surviving-token1, with equal end38. The correction normalizes this undelimited container start to physical0; all surviving child kinds/nesting/field roles/order/mapped ranges remain exact. The unit guards retained leading whitespace. An added default case admits20000 and refuses40000 otherwise valid statements within the256000byte source bound, exercising the combined200000-node traversal limit before plan publication.',
        'Final-r2 native13groups587/0/13, format and build pass, but alltargetClippy denies two single_range_in_vec_init warnings in the new cfg(test) fixture. Replacing only those one-element Range arrays with an explicit once-collected Vec preserves production bytes and the unit intent without lint suppression. The exact one-file test-only delta and source manifests are retained; final-r3 repeats the full selected validation on final sources.',
        'Final13selectedgroups587/0/13 include469/0/7 library and29/0/0 propagated-delete contract. Explicit actual2/0/0 overlaps the two ignored live cases: existingv1 plus one newv2, each TS/JS. Actual Node24.21.0/TypeScript5.8.3/language-server5.1.3 references feed original source-bound plans; trusted tsc --noEmit verifies copied results only. Execution-permission/approval/write/recovery denials and exact approved application/recovery are exercised. No analyzed source module executes; this is not a compiler-reference completeness or runtime equivalence attestation.',
        'Format/build/alltargetClippy-Dwarnings pass with737native inputs unchanged across final and actual phases. The production binary hash is observed after final build, without inventing a pre-test hash or equating in-process library tests with production executable execution. All737inputs have exact or CRLF-only integration materialization proof. The PG JSONL stays2779bytes/f67a05349a6cedf55222afd3bb3231c169d382dd4b078d76544daac517530b47 raw-equal. Integrated format/Python17/typecheck/TS260+3skip pass with743inputs unchanged.',
        'Original353inventory/73unreconciled/77backendunknown, falsefullContractCertified flags, prior evidence README bytes and old OS1314 fixture prerequisites remain. Other expression/declaration/file/language/provider forms, semantics/dispatch/exhaustiveconsumers, full upstream/IDE/nonWindows/whole-native/platform/main/release acceptance and broaderR1-R9 remain open. Full user task and AgentMemory actions remain active.'
    ],'artifacts':[]}
append=f'''\n## Explicit wrapped-call propagation checkpoint\n\nSource `{source['sourceCommit']}`, runtime merge `{source['runtimeCommit']}`.\n[Indexed evidence](wrapped-propagation/index.json) retains1unit+9default+1actual new\ncases, final13selectedgroups587/0/13, original provider/source bindings, exact\nwrapped-statement removals and bounded surviving-syntax correspondence. Unknown\nconsumers stay visible; v1/facade defaults and canonical Plan identity remain.\nActual TypeScript5.8.3/language-server5.1.3 validate v1/v2 TS/JS copied results and\nguarded application/recovery without running source modules. Format/build/Clippy\nand integrated743unchanged/Python17/typecheck/TS260+3skips pass. Both original\nrefusals, corrected timestamp test and statement-merging regression are retained.\nScoped runs overlap; no whole-native/platform/upstream/main/release acceptance\nfollows. Previous README bytes remain; broaderR1-R9 continues.\n'''.encode()
readme.write_bytes(prefix+append);assert readme.read_bytes().startswith(prefix)
for file in sorted(out.rglob('*')):
    if file.is_file():
        raw=file.read_bytes();index['artifacts'].append({'path':file.relative_to(out).as_posix(),'bytes':len(raw),'sha256':sha(raw)})
(out/'index.json').write_text(json.dumps(index,indent=2,ensure_ascii=False)+'\n',encoding='utf-8')
print(json.dumps({'artifacts':len(index['artifacts']),'indexSha256':sha((out/'index.json').read_bytes()),'priorEvidenceReadme':prior}),flush=True)
