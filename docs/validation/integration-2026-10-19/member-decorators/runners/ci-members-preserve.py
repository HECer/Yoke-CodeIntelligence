from pathlib import Path
import json,hashlib,shutil,re,datetime
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence')
base=root/'validation/integration-20261018/member-decorators'
out=root/'docs/validation/integration-2026-10-19/member-decorators'
read=lambda n:json.loads((base/n).read_bytes());sha=lambda b:hashlib.sha256(b).hexdigest()
source=read('source-integration.json');final=read('final/result.json');actual=read('live-final-r2/result.json');siblings=read('live-siblings-final-r2/result.json');python=read('live-python-final-r2/result.json');integrated=read('integrated/result.json')
assert all(r['allChecksPassed'] and r['sourceUnchanged'] for r in [final,actual,siblings,python,integrated])
assert source['nativeGroups']==12 and source['nativeTotals']=={'passed':577,'failed':0,'ignored':11}
assert final['inputs']==736 and integrated['frozenInputs']==742
assert source['newCases']=={'unitAdmission':1,'defaultIntegration':8,'actualCompilerAndAst':1,'actualLanguageServerSafeDelete':1}
assert sha((root/'target/debug/yoke.exe').read_bytes())==source['nativeBinarySha256']==read('final/native-test-binary.json')['sha256']
assert source['actualTools']['compilerVersion']=='Version 5.8.3' and source['actualTools']['serverVersion']=='5.1.3'
assert not read('baseline/result.json')['sourceUnchanged'] and read('baseline-r2/result.json')['sourceUnchanged']
assert read('baseline-validity.json')['firstBaselineIsFrozenEvidence'] is False
for name in source['paths']:assert sha((base/'final/source'/name).read_bytes())==read('final/source-before.json')[name],name
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
for file in sorted(Path(__file__).parent.glob('ci-members-*')):
    if file.is_file() and file.suffix in ['.py','.rs','.md','.ps1']:shutil.copyfile(file,runners/file.name)
readme=root/'docs/validation/integration-2026-10-19/README.md';prefix=readme.read_bytes();prior={'bytes':len(prefix),'sha256':sha(prefix)}
index={'schemaVersion':1,'kind':'native_concrete_class_method_decorator_ownership',
    'asOf':datetime.datetime.now(datetime.timezone.utc).isoformat(),**source,
    'platform':'Windows','nativeInputs':final['inputs'],'integratedInputs':integrated['frozenInputs'],
    'finalNative':final,'actualCompilerAndLanguageServer':actual,'actualExistingSiblingGate':siblings,
    'actualExistingPythonGate':python,'integrated':integrated,'priorEvidenceReadme':prior,
    'previouslyRefusedConcreteMemberOwnershipAdded':True,'nativeInputSchemaChanged':False,
    'existingUndecoratedAndInsertReplaceCoordinatesChanged':False,'legacyPlanShapeOrIdentityChanged':False,
    'analyzedModuleOrDecoratorExecuted':False,'compilerFixtureMode':'experimentalDecorators=true',
    'nonWindowsRuntimeValidated':False,'wholeNativeRerun':False,'fullUpstreamContractCertified':False,
    'mainOrReleaseVerified':False,'notes':[
        'Own MIT checked decorator-ownership extension for concrete plain-named TS/TSX/JS/JSX class methods. It replaces a previously explicit refusal without adding an input flag, dependency or authority. Full original AST/name/kind/source-range binding is retained. At most64 contiguous direct sibling decorators and intervening comments/whitespace belong to the selected method. Unsupported gap tokens, constructor, abstract/signature and computed/quoted/private decorated names refuse. Full-result syntax is checked under original4MB/200000node/deadline bounds.',
        'Parent-class and other members’ decorator chains, ordinary comments before the first decorator, all unrelated bytes and original entity/evidence coordinates remain. A pending gap before any decorator is irrelevant to the old undecorated case; the regression includes a preceding comment/semicolon. Only deletion derives the owning range; insert/replace use released original coordinates. Decorator expressions are parsed as source, never invoked. Saved Plan shape/identity and separate usage/approval/verification/write/stale/recovery contracts remain.',
        'The native and provider safeDelete final-preparation paths share this owning source boundary. Actual TypeScript language-server5.1.3 performs definition/reference checks and returns a source-bound safeDelete plan with complete:false, not exhaustive unusedness or consumer proof. No provider wire/host authority is inferred from source. Source-code comment/decorator labels do not attest semantic correctness or runtime execution.',
        'New distinct cases:1unit admission,8default integration and2actual gates. Literal source-effect tables cover analyzer8/9, all four extensions, async/static/generator/get/set concrete methods, stacked decorators, earlier/parent/neighbor comments/decorators, BOM/Unicode/CRLF, exact64/65capacity, malformed/gap/nonconcrete/nonsimple forms, forged names/kinds/paths/ranges and original insert/replace behavior. NativeMCP initialization/tool dispatch prepares the exact source plan with modelCalls0 and no application/execution grants. An earlier decorated-member unsupported table case is replaced by positive coverage; unrelated refusal cases remain.',
        'Actual Node24.21.0 / TypeScript5.8.3 compiler and compiler AST API validate copied TS/TSX/JS/JSX results. AST inspection proves the retained class parentMark decorator and the undecorated kept neighbor while the selected method/chain is absent. The actual language-server safeDelete path is independently compiled/inspected. These owned fixtures use experimentalDecorators:true, not a qualification of all Stage3/transform/runtime semantics. Analyzed modules/decorator expressions never execute; undefined fixture markers would fail runtime execution. Actual verifier processes, output/status/sourceIntegrity and independent pre-verification/pre-approval/write denials, exact approved application and recovery are logged.',
        'The first baseline was started before a formatter terminal session completed. Its sourceUnchanged:false is an invalid freeze, preserved without accepted counts. baseline-validity.json records the exact changed test file. A fresh sequential baseline-r2 has unchanged sources and reproduces the original refusal0/1/0 before implementation. R1positive1/0/0 passes. R2selected26/1/2 exposes a negative test assuming every unsupported form has a native method entity. The correction uses the actual entity when available and an unknown valid selector otherwise; no fake source binding is invented. R3selected29/0/4 and first actual2/0/0 pass. All logs/manifests remain nonadditive.',
        'Final12selectedgroups577/0/11 include468/0/7 library and8/0/2 new default integration; actual gates are not added to these totals. Format/build/alltargetClippy-Dwarnings pass with736native inputs unchanged. The exact159129088byte production binary5dcff4918c9a72d0344f2b9588bac6f7b7ec3d8a4d6eaf58de77628379c3c66d is unchanged after the final build, actual compiler/server and existing sibling/Python gates, and integrated checks. An early binary probe correctly refused before test compilation finished; the later probe is recorded after compilation, without inventing a pre-test hash.',
        'All736native inputs are checked for exact or CRLF-only integration materialization. The captured PostgreSQL JSONL remains2779bytes/f67a05349a6cedf55222afd3bb3231c169d382dd4b078d76544daac517530b47 raw-equal. Integrated format/Python17/typecheck/TS260+3skips pass with742inputs unchanged. Original353inventory/73unreconciled/77backendunknown, false full-contract flags and prior evidence README bytes remain.',
        'Other member/name/language forms, decorator modes/versions/transforms, complete compiler/reference/dispatch/consumer/runtime guarantees, full upstream/IDE/nonWindows/whole-native/platform/main/release and broaderR1-R9 remain open. Earlier OS1314 fixture prerequisites are outside this finite selection. No actual analyzed-program execution, production deployment or publication certification follows. The full task and coordination actions remain active.'
    ],'artifacts':[]}
append=f'''\n## Concrete class-method decorator ownership checkpoint\n\nSource `{source['sourceCommit']}`, runtime merge `{source['runtimeCommit']}`.\n[Indexed evidence](member-decorators/index.json) retains1unit+8default+2actual\nnewcases, final12selectedgroups577/0/11, exact contiguous method/decorator\nownership, original anchors and independent authority/recovery gates. Actual\nTypeScript5.8.3 compiler/AST verifies all four extensions and class/neighbor\nassociation; language-server5.1.3safeDelete produces the same checked plan.\nThe experimental-decorator fixtures execute no analyzed module/decorator.\nExisting sibling/Python gates pass on the identical final736-input freeze.\nFormat/build/Clippy-Dwarnings and integrated742unchanged/Python17/typecheck/\nTS260+3skips pass. Invalid formatter-overlap baseline and corrected missing-entity\ntest assumption remain. No whole-native/platform/upstream/main/release acceptance\nfollows. Previous README bytes remain; broaderR1-R9 continues.\n'''.encode()
readme.write_bytes(prefix+append);assert readme.read_bytes().startswith(prefix)
for file in sorted(out.rglob('*')):
    if file.is_file():
        raw=file.read_bytes();index['artifacts'].append({'path':file.relative_to(out).as_posix(),'bytes':len(raw),'sha256':sha(raw)})
(out/'index.json').write_text(json.dumps(index,indent=2,ensure_ascii=False)+'\n',encoding='utf-8')
print(json.dumps({'artifacts':len(index['artifacts']),'indexSha256':sha((out/'index.json').read_bytes()),'priorEvidenceReadme':prior}),flush=True)
