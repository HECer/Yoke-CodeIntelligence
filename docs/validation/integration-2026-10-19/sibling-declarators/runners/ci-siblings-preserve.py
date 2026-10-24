from pathlib import Path
import json,hashlib,shutil,re,datetime
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence')
base=root/'validation/integration-20261018/sibling-declarators'
out=root/'docs/validation/integration-2026-10-19/sibling-declarators'
read=lambda n:json.loads((base/n).read_bytes());sha=lambda b:hashlib.sha256(b).hexdigest()
source=read('source-integration.json');final=read('final/result.json');live=read('live-final-r2/result.json');python=read('live-python-final-r2/result.json');integrated=read('integrated/result.json')
assert all(r['allChecksPassed'] and r['sourceUnchanged'] for r in [final,live,python,integrated])
assert source['nativeGroups']==11 and source['nativeTotals']=={'passed':568,'failed':0,'ignored':9}
assert source['newCases']=={'unitAdmission':1,'defaultIntegration':8,'actualCompiler':1}
assert final['inputs']==734 and integrated['frozenInputs']==740
assert sha((root/'target/debug/yoke.exe').read_bytes())==source['nativeBinarySha256']==read('final/native-test-binary.json')['sha256']
assert source['actualCompiler']['compilerVersion']=='Version 5.8.3' and source['actualCompiler']['nodeVersion']=='v24.21.0'
for name in source['paths']:
    assert sha((base/'final/source'/name).read_bytes())==read('final/source-before.json')[name],name
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
for file in sorted(Path(__file__).parent.glob('ci-siblings-*')):
    if file.is_file() and file.suffix in ['.py','.rs','.md','.ps1']:shutil.copyfile(file,runners/file.name)
readme=root/'docs/validation/integration-2026-10-19/README.md';prefix=readme.read_bytes()
prior={'bytes':len(prefix),'sha256':sha(prefix)}
index={'schemaVersion':1,'kind':'native_ts_js_preserved_sibling_declarators',
    'asOf':datetime.datetime.now(datetime.timezone.utc).isoformat(),**source,
    'platform':'Windows','nativeInputs':final['inputs'],'integratedInputs':integrated['frozenInputs'],
    'finalNative':final,'actualCompilerGate':live,'actualExistingPythonGate':python,
    'integrated':integrated,'priorEvidenceReadme':prior,
    'nativeDefaultDeletionPolicyChanged':False,'semanticProviderFacadePolicyChanged':False,
    'legacyPlanSchemaOrIdentityChanged':False,'analyzedModuleExecuted':False,
    'fullUpstreamContractCertified':False,'nonWindowsRuntimeValidated':False,
    'mainOrReleaseVerified':False,'wholeNativeRerun':False,'notes':[
        'Own MIT explicit native siblingDeclarators reject/preserve policy, with no dependency or provider/model changes. Absent/reject retains released strict refusal for multi-declarator statements. The option is limited to a singleton simple TS/TSX/JS/JSX function-valued variable binding in an ordinary const/let/var program or statement-block declaration. Native eight-tool MCP adds only the closed optional field. Combined deletion policies, mixed edits and invalid values/types refuse before capture or publication.',
        'Full original AST/name/kind/path/language/range binding produces an opaque checked selection. Only a direct parent comma is selected; nested commas in initializer/parameters/types/strings/JSX are not delimiter candidates. Two separate original-coordinate deletions preserve all unrelated declarators, export/ambient wrappers, semicolon and whitespace. A single declarator retains its existing whole-statement effect. The complete resulting source is reparsed under the same deadline,4MB source and200000-node walk bounds. No source formatting or entity/evidence-coordinate rewriting occurs.',
        'Direct inter-declarator comments can carry ambiguous attachments, so this finite policy refuses them rather than deleting or reattaching those bytes. Comments inside an admitted initializer belong to its existing AST range. Loop/nonstatement bindings, named expressions/methods, non-simple selected names, other languages and unsupported forms refuse. Source syntax/binding does not establish unusedness, consumer safety, compiler reference completeness or runtime behavior. Semantic-provider/facade callers keep strict defaults.',
        'New distinct cases are1unit admission,8default integration and1explicit actual compiler gate. Literal independent expected-byte tables cover first/middle/last positions, const/let/var, all four extensions, analyzer8/9, BOM/Unicode/CRLF, nested commas and owned comments. Separate refusals cover direct comments, malformed source, exact source/AST capacities and deadline, forged anchors/paths/kinds, policy JSON and mixed operations. Positive nativeMCP initialization/tool dispatch prepares the exact planned effect with modelCalls0 and no write/execution grants.',
        'Actual Node24.21.0 and TypeScript5.8.3 compile copied TS,JS,TSX,JSX results with fixed owned tsconfig and intrinsic JSX declarations. This is --noEmit compiler execution, not analyzed-module execution; no project scripts/plugins are selected. The gate checks pre-verification/pre-approval/write denials, exact approved application and source recovery. Runtime metadata hashes the selected Node executable and bin/tsc launcher; it does not claim a complete TypeScript SDK/dependency attestation. No vendor implementation is copied into the core or evidence.',
        'Initial3groups15/0/1 and later4groups26/0/1 pass, as does the first explicit actual compiler gate. These overlapping runs are nonadditive. Final11selected groups record568/0/9, including467/0/7 library and8/0/1 new default integration. Format/build/alltargetClippy-Dwarnings pass. Final734native inputs are unchanged. The exact159118848byte production binary234ae8075873578e6ee5f3d59eb50dd2559a63eb5f0dc703c897b033b5cfd6e4 remains identical after the build, final compiler and existing Python gate, and integrated checks.',
        'The explicit final actual compiler gate qualifies all four source extensions on the exact final734-input freeze. The existing Python AST-only permission/application/recovery gate is also rerun on that identical source after the common policy dispatch refactor. Gates are not added to default-run totals. All initial source bodies, full manifests, command logs and final sources are retained; no failure or successful variant is invented.',
        'All734native inputs materialize exactly or through CRLF-only checkout differences. The captured PostgreSQL JSONL remains2779bytes and f67a05349a6cedf55222afd3bb3231c169d382dd4b078d76544daac517530b47 raw-equal. Integrated format/Python17/typecheck/TS260+3skips pass with740inputs unchanged. Original353inventory/73unreconciled/77backendunknown and false full-contract flags, plus prior evidence README bytes, remain unchanged.',
        'This finite selected Windows validation does not replace whole-native or whole-platform acceptance. Earlier OS1314 prerequisite failures remain outside the selection. Other comment/binding/container/language forms, generalized refactoring/consumer guarantees, semantic-provider options, full upstream/IDE equivalence, nonWindows, main/release and broaderR1-R9 remain open. The full task and coordination actions stay active.'
    ],'artifacts':[]}
append=f'''\n## Explicit native TS/JS sibling-declarator deletion checkpoint\n\nSource `{source['sourceCommit']}`, runtime merge `{source['runtimeCommit']}`.\n[Indexed evidence](sibling-declarators/index.json) retains1unit+8default+1actual\nnewcases, final11selectedgroups568/0/9, exact AST-delimited source edits, strict\ndefaults and ambiguous-comment refusals. Actual Node24.21/TypeScript5.8.3\ncompile copied TS/JS/TSX/JSX results and check approved application/exact recovery;\nthe existing Python AST-only gate also passes on the identical final source.\nNative734inputs/exact binary and integrated740inputs remain frozen. Format/\nbuild/Clippy-Dwarnings and mandatory Python17/typecheck/TS260+3skips pass. No\nanalyzed-module execution, consumer/semantic/IDE or whole-native/platform/main/\nrelease certification follows. Prior README bytes remain; broaderR1-R9 continues.\n'''.encode()
readme.write_bytes(prefix+append);assert readme.read_bytes().startswith(prefix)
for file in sorted(out.rglob('*')):
    if file.is_file():
        raw=file.read_bytes();index['artifacts'].append({'path':file.relative_to(out).as_posix(),'bytes':len(raw),'sha256':sha(raw)})
(out/'index.json').write_text(json.dumps(index,indent=2,ensure_ascii=False)+'\n',encoding='utf-8')
print(json.dumps({'artifacts':len(index['artifacts']),'indexSha256':sha((out/'index.json').read_bytes()),'priorEvidenceReadme':prior}),flush=True)
