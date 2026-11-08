from pathlib import Path
import json,hashlib,shutil,re,datetime
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');base=root/'validation/integration-20261018/session-refactor'
out=root/'docs/validation/integration-2026-10-19/session-refactor';temp=Path(__file__).parent
read=lambda n:json.loads((base/n).read_bytes());sha=lambda b:hashlib.sha256(b).hexdigest()
source=read('source-integration.json');final=read('final/result.json');actual=read('live-final/result.json');integrated=read('integrated/result.json')
assert all(r['allChecksPassed'] and r['sourceUnchanged'] for r in [final,actual,integrated])
assert source['nativeGroups']==15 and source['nativeTotals']['failed']==0
assert final['inputs']==738 and integrated['frozenInputs']==744
assert source['newCases']=={'defaultIntegration':9,'actualNativeCliCompilerAndLanguageServer':1}
assert actual['commands'][0]['totals']=={'passed':1,'failed':0,'ignored':0}
assert source['actualTools']['compilerVersion']=='Version 5.8.3' and source['actualTools']['serverVersion']=='5.1.3'
assert sha((root/'target/debug/yoke.exe').read_bytes())==source['nativeBinarySha256']
assert set(re.findall(r'actual-native-session-cli-executable=.* sha256=([a-f0-9]{64})',(base/'live-final/native.log').read_text(encoding='utf-8')))=={source['nativeBinarySha256']}
assert read('baseline/result.json')['commands'][0]['totals']=={'passed':0,'failed':1,'ignored':0}
for name in source['paths']:assert sha((base/'final/source'/name).read_bytes())==read('final/source-before.json')[name],name
ts=(base/'integrated/tests.log').read_text(encoding='utf-8');assert re.search(r'\bpass\s+260\b',ts) and re.search(r'\bskipped\s+3\b',ts)
assert 'Ran 17 tests' in (base/'integrated/python-contracts.log').read_text(encoding='utf-8')
old=read('frozen-tracker.json');current=json.loads((root/'docs/parity/native-reconciliation-2026-10-15.json').read_bytes())
for key in ['inventoryLedger','historicalInventory','ledgerSummary']:assert old[key]==current[key]
prior_body=(temp/'ci-pr-7-body-wrapped-propagation-final.md').read_bytes()
assert len(prior_body)==60414 and sha(prior_body)=='adb7b9ec4e5fbeaa1bbb2048aed82c105b38d82ec4a62157d18d30e8c90693ce'
(base/'prior-pr-body.md').write_bytes(prior_body)
out.mkdir(exist_ok=False)
for file in sorted(base.rglob('*')):
    if file.is_file():
        target=out/file.relative_to(base);target.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(file,target)
runners=out/'runners';runners.mkdir()
for file in sorted(temp.glob('ci-session-*')):
    if file.is_file() and file.suffix in ['.py','.rs','.md','.ps1']:shutil.copyfile(file,runners/file.name)
readme=root/'docs/validation/integration-2026-10-19/README.md';prefix=readme.read_bytes();prior={'bytes':len(prefix),'sha256':sha(prefix)}
index={'schemaVersion':1,'kind':'native_session_semantic_refactor_preparation','asOf':datetime.datetime.now(datetime.timezone.utc).isoformat(),
    **source,'platform':'Windows','nativeInputs':738,'integratedInputs':744,'finalNative':final,
    'actualNativeCliCompilerAndLanguageServer':actual,'integrated':integrated,'priorEvidenceReadme':prior,
    'priorPRBodyArchive':{'path':'prior-pr-body.md','bytes':len(prior_body),'sha256':sha(prior_body)},
    'existingNativeArgumentsAndPlanIdentityChanged':False,'publishedFacadeArgumentSchemasChanged':False,
    'normalNativeEightToolContractChanged':False,'nativeSessionDescriptionAdvertisesRoute':True,
    'sourceWrittenOrSessionSteeredByPreparation':False,'approvalOrExecutionAuthorityAdded':False,
    'analyzedModuleExecuted':False,'nonWindowsRuntimeValidated':False,'wholeNativeRerun':False,
    'fullUpstreamContractCertified':False,'mainOrReleaseVerified':False,'notes':[
        'Own MIT native Session refactor.prepare routes existing safeDelete/rename/inline/move through the existing batch session CLI and native ci_session MCP tool. Closed operation/arguments envelope,64000byte inner serializedUTF8 bound, mode/host-project restrictions and exact current persisted-session comparison precede provider work. Original caller deadline/cancellation are retained. A checked constant-result SELECT compares the opened record; no transaction is held over providers and no lease over future state/source changes is claimed.',
        'Unchanged native arguments/results retain exact original source/provider/snapshot proofs and canonical Plan identity/publication. There is no session revision/project steering, original source write, model/runtime grant or automatic approval/verification/application. The nativeSession description advertises the operation; native8 and published facade argument schemas are unchanged. Unknown commands/authority fields, query/readonly targets, stale session/snapshot and unsupported native shapes fail rather than producing empty complete results.',
        'Ten new distinct cases:9default integration+1explicit actual gate. Tables cover analyzer8/9, BOM/nonBMP/CRLF and exact source effects, unchanged session configuration, mode/readonly/originaldeadline denial, actual second-client steering, exact64000/64001admission, all four native operation routes, unknown envelope/authority fields, source paths and stale snapshot, preserved defaultv1 refusal, MCP result/error/tool discovery, CLI host trust/planning/duplicateJSON and no auto-approval/source writes. Existing client/core tests remain.',
        'Baseline0/1/0 with unchanged738inputs reproduces missing Session operation. R1threegroups15/0/0 incl6newdefault and R2fivegroups25/0/1 incl9newdefault pass. R2 predates the final description/discovery/documentation update. First actual1/0/0 already uses the final738input source. Final and actual counts overlap and are not added; all raw logs/manifests/modified source variants remain.',
        'Actual selected native executable SHA is printed before CLI use and rechecked after both TS/JS cases, matching the final production binary. Trusted host Node24.21.0/TypeScript5.8.3/language-server5.1.3 perform real nativeCLI source-bound safeDeletev2 planning and copied tsc --noEmit verification. Separate preapproval/preverification/execution-permission/write/stale-source/recovery denials, exact approved application and exact recovery are exercised. No analyzed module executes. Actual qualification is for this owned safeDeletev2 path, not every forwarded intent/client installation or exhaustive compiler/runtime/consumer behavior.',
        'Final15selected native groups and all format/build/alltargetClippy-Dwarnings results are embedded with738inputs unchanged. The final actual phase uses the identical freeze. All738inputs have exact or CRLF-only integration materialization proof; PostgreSQLJSONL remains2779bytes/f67a05349a6cedf55222afd3bb3231c169d382dd4b078d76544daac517530b47 raw-equal. Integratedformat/Python17/typecheck/TS260+3skip pass with744inputs unchanged. Exact counts are taken from raw result groups, not inferred.',
        'Original353inventory/73unreconciled/77backendunknown and falsefullContractCertified flags, prior evidence README bytes and oldOS1314 prerequisites remain. Other provider intents/clients/platforms, complete semantics/dispatch/consumers, full upstream/IDE/nonWindows/whole-native/platform/main/release acceptance and broaderR1-R9 remain open. Full task/actions stay active.'
    ],'artifacts':[]}
t=source['nativeTotals'];append=f'''\n## Native session semantic planning checkpoint\n\nSource `{source['sourceCommit']}`, runtime merge `{source['runtimeCommit']}`.\n[Indexed evidence](session-refactor/index.json) retains9default+1actual newcases,\nfinal15selectedgroups{t['passed']}/{t['failed']}/{t['ignored']}, closed nativeSession/CLI/MCP\nplanning, current-state admission, unchanged native arguments and guarded plans.\nActual nativeCLI bytes match the final binary; TypeScript5.8.3/language-server5.1.3\nvalidate TS/JS safeDeletev2, copied compilation and independent application/recovery.\n738native/744integratedinputs remain unchanged within their freezes; quality,\nPython17, typecheck and TS260+3skip pass. No model/source execution or authority\nupgrade follows. Previous README bytes and priorPRbody are preserved; scopes\noverlap and broaderR1-R9 remains active without whole-native/platform/main/release\nacceptance.\n'''.encode()
readme.write_bytes(prefix+append);assert readme.read_bytes().startswith(prefix)
for file in sorted(out.rglob('*')):
    if file.is_file():
        raw=file.read_bytes();index['artifacts'].append({'path':file.relative_to(out).as_posix(),'bytes':len(raw),'sha256':sha(raw)})
(out/'index.json').write_text(json.dumps(index,indent=2,ensure_ascii=False)+'\n',encoding='utf-8')
print(json.dumps({'artifacts':len(index['artifacts']),'indexSha256':sha((out/'index.json').read_bytes()),'priorEvidenceReadme':prior}),flush=True)
