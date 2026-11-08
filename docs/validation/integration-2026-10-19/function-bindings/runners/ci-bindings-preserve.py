from pathlib import Path
import json,hashlib,shutil,re,datetime
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');base=root/'validation/integration-20261018/function-bindings'
out=root/'docs/validation/integration-2026-10-19/function-bindings';temp=Path(__file__).parent
read=lambda n:json.loads((base/n).read_bytes());sha=lambda b:hashlib.sha256(b).hexdigest()
source=read('source-integration.json');final=read('final/result.json');actual=read('live-final/result.json');integrated=read('integrated/result.json')
assert all(r['allChecksPassed'] and r['sourceUnchanged'] for r in [final,actual,integrated])
assert source['nativeGroups']==15 and source['nativeTotals']['failed']==0
assert final['inputs']==738 and integrated['frozenInputs']==744
assert source['newCases']=={'defaultIntegration':10,'actualFunctionBindingsCli':1}
assert actual['commands'][0]['totals']=={'passed':2,'failed':0,'ignored':0}
assert source['actualTools']['compilerVersion']=='Version 5.8.3' and source['actualTools']['serverVersion']=='5.1.3'
assert sha((root/'target/debug/yoke.exe').read_bytes())==source['nativeBinarySha256']
assert set(re.findall(r'actual-native-session-cli-executable=.* sha256=([a-f0-9]{64})',(base/'live-final/native.log').read_text(encoding='utf-8')))=={source['nativeBinarySha256']}
assert read('baseline/result.json')['commands'][0]['totals']=={'passed':0,'failed':1,'ignored':0}
for name in source['paths']:assert sha((base/'final/source'/name).read_bytes())==read('final/source-before.json')[name],name
ts=(base/'integrated/tests.log').read_text(encoding='utf-8');assert re.search(r'\bpass\s+260\b',ts) and re.search(r'\bskipped\s+3\b',ts)
assert 'Ran 17 tests' in (base/'integrated/python-contracts.log').read_text(encoding='utf-8')
old=read('frozen-tracker.json');current=json.loads((root/'docs/parity/native-reconciliation-2026-10-15.json').read_bytes())
for key in ['inventoryLedger','historicalInventory','ledgerSummary']:assert old[key]==current[key]
prior_body=(temp/'ci-pr-7-body-session-refactor-final.md').read_bytes()
assert len(prior_body)==6151 and sha(prior_body)=='39882b2bd084116b61c9a5065f0414666f91cd5c4efd475ebcbc65c069350f91'
(base/'prior-pr-body.md').write_bytes(prior_body);out.mkdir(exist_ok=False)
for file in sorted(base.rglob('*')):
    if file.is_file():
        target=out/file.relative_to(base);target.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(file,target)
runners=out/'runners';runners.mkdir()
for file in sorted(temp.glob('ci-bindings-*')):
    if file.is_file() and file.suffix in ['.py','.rs','.md','.ps1']:shutil.copyfile(file,runners/file.name)
readme=root/'docs/validation/integration-2026-10-19/README.md';prefix=readme.read_bytes();prior={'bytes':len(prefix),'sha256':sha(prefix)}
index={'schemaVersion':1,'kind':'native_function_binding_propagation_v3','asOf':datetime.datetime.now(datetime.timezone.utc).isoformat(),
    **source,'platform':'Windows','nativeInputs':738,'integratedInputs':744,'finalNative':final,
    'actualFunctionBindingsAndExistingV2Cli':actual,'integrated':integrated,'priorEvidenceReadme':prior,
    'priorPRBodyArchive':{'path':'prior-pr-body.md','bytes':len(prior_body),'sha256':sha(prior_body)},
    'oldPolicyBoundariesChanged':False,'analyzerOrOriginalEntityIdentityChanged':False,
    'legacyPlanShapeOrIdentityFormulaChanged':False,'analyzedModuleExecuted':False,'approvalOrExecutionAuthorityAdded':False,
    'nonWindowsRuntimeValidated':False,'wholeNativeRerun':False,'fullUpstreamContractCertified':False,'mainOrReleaseVerified':False,
    'notes':[
        'Own MIT explicit captured_es_function_propagation_v3 extends finite module admission to single plain const/let/var statements whose direct initializer is arrow_function or anonymous function_expression with a body. Existing regular function declarations remain. V1/default and V2 retain their original module boundary. V3 inherits V2 wrappers and surviving-AST postcondition; no new input field or execution/write/approval authority is introduced.',
        'Each variable_declarator must match an existing captured function entity path/language/kind/fullrange/name-range/name exactly. Whole statement/export ranges supply keyword/semicolon/wrapper removal without modifying original entity/evidence/producer/analyzer identities. Immutable base Graph equality and original disk bytes are checked. Multiple/pattern/ambient/using/nonfunction/parenthesized/named-inner-expression forms and direct declaration comments refuse. Surviving assignment/value/constructor consumers cannot be forced away.',
        'Same exact provider definitions/references, source/context consistency, conservative possible/literal candidates, original-region dependency closure/privatecycles/export retention and unknown-consumer flags remain. Ordinary unused bindings are not swept. Source/graph/file/declaration/reference/usage/deadline/cancellation capacities remain; physical originals are rechecked. One canonical guarded plan retains normal approval/verification/write/stale/recovery and existing nontransactional publication boundaries, not a future-consumer lease.',
        'Eleven new distinct cases:10default integration+1actual CLI gate. Tables cover both analyzer epochs and three keywords, original coordinates/immutable Graph/BOM/nonBMP/CRLF, older-policy refusal, supported mixed modules, unsupported shapes/forceconsumers, cycles/export/survivingdeps/argumentclosure, exact declaration16/17 and usage128/129 limits, missing/malformed/omitted/truncated/foreign/tampered proofs and automatic-semicolon merging. Existing native client and sibling guards remain.',
        'Baseline0/1/0 with738inputs unchanged reproduces unsupportedv3 before implementation. R1two groups38/0/1 and R2three groups56/0/3 pass, including all new default functions by R2. Own protocol fixture retains named-function lookup precedence and adds a variable fallback; it is explicitly not a compiler. The now-supportedv3 negative version selector is promoted to unsupportedv4. First actual2/0/0 passes before the sole subsequent source change: private helper lifetime spelling is elided without behavior changes. Final and actual phases use identical final source; all logs/manifests/modified source variants remain nonadditive.',
        'Actual final2/0/0 overlaps two ignored session_refactor cases: existingv2 and newv3. V3 runs6real nativeCLI fixtures(TS/JSxconst/let/var), covering arrow, anonymous function and async arrow targets plus Greek dependency/target identifiers for var. Trusted Node24.21.0/TypeScript5.8.3/language-server5.1.3 perform exact-bound source navigation and copied tsc --noEmit. Preapproval/preverification/execution/write/recovery denials, approved exact application and byte-for-byte recovery pass without executing analyzed modules. Actual selected CLI SHA before/after use matches final binary. This is finite qualification, not exhaustive binding/dispatch/alias/rebinding/runtime/consumer proof.',
        'Final15selected native groups and format/build/alltargetClippy-Dwarnings are embedded with738inputs unchanged. All738inputs have exact or CRLF-only integration materialization proof; PGJSONL stays2779bytes/f67a05349a6cedf55222afd3bb3231c169d382dd4b078d76544daac517530b47 raw-equal. Integratedformat/Python17/typecheck/TS260+3skip pass with744inputs unchanged. Original353inventory/73unreconciled/77backendunknown, falsefullContract flags, prior evidence README bytes and all prior failures remain.',
        'Other data/declaration/file/language forms, general alias/rebinding/consumer handling, full upstream/IDE/nonWindows/whole-native/platform/main/release and broaderR1-R9 remain open. Existing OS1314 prerequisites are outside this selection. Full task and AgentMemory actions remain active.'
    ],'artifacts':[]}
t=source['nativeTotals'];append=f'''\n## Explicit function binding propagation checkpoint\n\nSource `{source['sourceCommit']}`, runtime merge `{source['runtimeCommit']}`.\n[Indexed evidence](function-bindings/index.json) retains10default+1actual newcases,\nfinal15selectedgroups{t['passed']}/{t['failed']}/{t['ignored']}, exact existing variable-declarator\nfunction bindings and owning statement ranges, with V1/V2 boundaries retained.\nActual nativeCLI/TypeScript5.8.3/language-server5.1.3 validate TS/JS const/let/var,\nasync arrows and Greek names, copied compilation and independent apply/recovery.\n738native/744integratedinputs remain unchanged within their freezes; quality,\nPython17, typecheck and TS260+3skip pass. Source/Graph/entity identities and prior\nREADME bytes remain; no complete compiler/runtime/consumer or whole-native/\nplatform/upstream/main/release acceptance follows. BroaderR1-R9 continues.\n'''.encode()
readme.write_bytes(prefix+append);assert readme.read_bytes().startswith(prefix)
for file in sorted(out.rglob('*')):
    if file.is_file():
        raw=file.read_bytes();index['artifacts'].append({'path':file.relative_to(out).as_posix(),'bytes':len(raw),'sha256':sha(raw)})
(out/'index.json').write_text(json.dumps(index,indent=2,ensure_ascii=False)+'\n',encoding='utf-8')
print(json.dumps({'artifacts':len(index['artifacts']),'indexSha256':sha((out/'index.json').read_bytes()),'priorEvidenceReadme':prior}),flush=True)
