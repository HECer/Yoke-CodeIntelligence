from pathlib import Path
import json,hashlib,shutil,re,datetime
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence')
base=root/'validation/integration-20261018/python-empty-suite'
out=root/'docs/validation/integration-2026-10-19/python-empty-suite'
read=lambda n:json.loads((base/n).read_bytes());sha=lambda b:hashlib.sha256(b).hexdigest()
source=read('source-integration.json');final=read('final-r2/result.json');live=read('live-final-r2/result.json');integrated=read('integrated/result.json')
assert final['allChecksPassed'] and final['sourceUnchanged'] and live['allChecksPassed'] and live['sourceUnchanged']
assert integrated['allChecksPassed'] and integrated['sourceUnchanged'] and integrated['frozenInputs']==738
assert source['nativeTotals']=={'passed':559,'failed':0,'ignored':8} and source['nativeGroups']==10
assert read('final/result.json')['commands'][0]['totals']=={'passed':558,'failed':1,'ignored':8}
assert source['newCases']=={'unitAdmission':1,'defaultIntegration':8,'actualPythonAstOnly':1}
assert read('live-final-r2/python-runtime.json')['version']=='Python 3.14.7'
assert sha((root/'target/debug/yoke.exe').read_bytes())==source['nativeBinarySha256']==read('final/native-test-binary.json')['sha256']
assert re.search(r'\bpass\s+260\b',(base/'integrated/tests.log').read_text(encoding='utf-8'))
assert re.search(r'\bskipped\s+3\b',(base/'integrated/tests.log').read_text(encoding='utf-8'))
assert 'Ran 17 tests' in (base/'integrated/python-contracts.log').read_text(encoding='utf-8')
for name in source['paths']:
    raw=(base/'final-r2/source'/name).read_bytes()
    assert sha(raw)==read('final-r2/source-before.json')[name],name
old=read('frozen-tracker.json');current=json.loads((root/'docs/parity/native-reconciliation-2026-10-15.json').read_bytes())
for key in ['inventoryLedger','historicalInventory','ledgerSummary']:assert old[key]==current[key]
out.mkdir(exist_ok=False)
for file in sorted(base.rglob('*')):
    if file.is_file():
        target=out/file.relative_to(base);target.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(file,target)
runners=out/'runners';runners.mkdir()
for file in sorted(Path(__file__).parent.glob('ci-python-body-*')):
    if file.is_file() and file.suffix in ['.py','.rs','.md','.ps1']:shutil.copyfile(file,runners/file.name)
readme=root/'docs/validation/integration-2026-10-19/README.md';prefix=readme.read_bytes()
prior={'bytes':len(prefix),'sha256':sha(prefix)}
index={'schemaVersion':1,'kind':'native_python_explicit_empty_suite_deletion',
    'asOf':datetime.datetime.now(datetime.timezone.utc).isoformat(),**source,
    'platform':'Windows','nativeInputs':final['inputs'],'integratedInputs':integrated['frozenInputs'],
    'finalNative':final,'actualPythonAstOnly':live,'integrated':integrated,'priorEvidenceReadme':prior,
    'nativeDefaultDeletionPolicyChanged':False,'semanticProviderFacadePolicyChanged':False,
    'legacyPlanSchemaOrIdentityChanged':False,'analyzedModuleOrDecoratorExecuted':False,
    'fullUpstreamContractCertified':False,'nonWindowsRuntimeValidated':False,'wholeNativeRerun':False,
    'mainOrReleaseVerified':False,'notes':[
        'Own MIT native extension without new dependencies. Native ci_change_prepare.deleteSymbol has an optional closed emptyPythonBody enum reject/pass. Absent/reject preserves released refusal for an otherwise empty required Python suite. The option is Python-only and singleton: mixed explicit edits, other symbolic operations, rename/bulk and invalid values/types refuse. Eight-tool MCP and ordinary independent approval/write/verification/stale/recovery authority remain unchanged.',
        'Exact original error-free AST declaration/name/kind/path/language/ranges and owning decorator selection precede a literal pass only when the required parent block has no other non-comment statement. Existing docstrings, assignments, siblings and module-level deletion need no filler. The same bounded parser checks the entire resulting text under the original deadline and4MBsource/200000node bounds. Entity/evidence coordinates, BOM/Unicode/CRLF/indentation and neighboring comment bytes remain original. This is a source-edit/syntax contract, not exhaustive consumer, compiler or runtime semantics.',
        'The new policy is not forwarded by semantic-provider/facade deletion; those callers retain strict defaults. Saved Plan shape and identity formula are unchanged and exact canonical plan/delta/review replay remains mandatory. NativeMCPConnection initialization/tool dispatch prepares the exact planned edit with modelCalls0 and no application/verification/approval/journal authority. Other languages, ambiguous/forged binding, malformed/null/oversized source, expired deadline, unknown JSON and mixed intents refuse.',
        'New distinct cases:1unit source/AST-admission test,8default integration tests and1explicit actual Python gate. Variant tables cover analyzer8/9, async/nested functions/classes, decorators, comments, BOM/CRLF/Unicode/tabs, docstrings/siblings, module/control-flow suites, strict defaults and protocol/authority refusals. Cases are not multiplied by table variants and actual gates are not added to default whole-run totals.',
        'Actual selected Python3.14.7 executable4942b86a6597e5aee0128daa00050ed79bc21f6e709a78eb19cbfeb0c2f39ac9 runs stdlib ast.parse on copied original bytes under explicit verification authority. Unknown parent decorators remain and unknown removed decorators never execute; importing/executing this module would fail those names. The gate checks independent pre-verification/pre-approval/write denials, exact approved application and byte recovery. It proves selected syntax/structure and observed verifier execution, not analyzed-program runtime behavior or an OS sandbox.',
        'R1selected22/1/1 exposes a new test assumption: Engine.invoke returns status:error inside a successful Rust Result. Corrected admission tests inspect public validation, response error status and unchanged snapshot/plan counts. A positive full nativeMCP request is added. R2selected24/0/1 and the first explicit actual Python gate pass. Raw logs and original full source-hash manifests remain nonadditive.',
        'The first broad10groups records558/1/8; all new cases pass. The sole failure is an unchanged HTTP idle-cleanup fixture asserting that its20ms request budget must be fully exhausted. finish_rejection may return early on transport timeout and already caps every read by the original remaining budget. Only the test changes: configured positive OS read timeout must be<=the original20ms (or the original deadline is already expired before setup), with its existing250ms upper wall-clock bound retained. HTTP runtime source is unchanged. No claim is made about the exact OS scheduling/rounding cause of that particular early return.',
        'Final identical10group selection records559/0/8, including466/0/7 library and8/0/1 default new integration. Format/build/alltargetClippy-Dwarnings pass. Native732inputs remain unchanged in each frozen run. The only source difference from prior broadfinal is native/src/mcp_http_rejection_tests.rs; the159089152byte production binary95791c456b0db2ea3734b424e501f8d15df8e97cfec16b4755f193a94411c64f stays identical after build, final live gate and integrated checks. The explicit final actual-Python gate passes with the exact same732-source freeze. No inferred merged totals or whole-native rerun follows.',
        'Initial runner invocations recorded all732input hashes but inherited a generic feedback raw-source selection. Available R1 raw bodies were backfilled only where their complete hash matched the originally recorded manifest; source-backfill receipts identify hash-only historical variants, including the first wrong new test file. No unavailable original raw bytes are invented. Final corrected runner and final actual gate contain all nine changed source/doc/test bodies, checked against their frozen hashes. Initial command logs/manifests and the copied unrelated prior feedback files remain intact.',
        'Raw-or-CRLF-only integration materialization is checked for all732native inputs. The PostgreSQL response remains exactly2779bytes/f67a05349a6cedf55222afd3bb3231c169d382dd4b078d76544daac517530b47. Integrated format/Python17/typecheck/TS260+3skips pass with738inputs unchanged. Historical353inventory/73unreconciled/77backendunknown/falsefull-contract flags and previous evidence README bytes remain. Earlier unchanged OS1314 prerequisites are outside this finite selection; nonWindows/full-native/whole-platform/upstream/main/release and broaderR1-R9 remain open. The full task/actions remain active.'
    ],'artifacts':[]}
append=f'''\n## Explicit native Python empty-suite deletion checkpoint\n\nSource `{source['sourceCommit']}`, runtime merge `{source['runtimeCommit']}`.\n[Indexed evidence](python-empty-suite/index.json) retains1unit+8default+1actual\nnewcases, final10selectedgroups559/0/8, actual Python3.14.7 AST-only verification,\nindependent application/recovery gates and nativeMCP. The original error-envelope\nassumption and558/1/8idle-timeout test failure remain with their corrections.\nHTTP runtime source and exact binary stay unchanged across the test-only socket\nceiling correction. Format/build/Clippy-Dwarnings and mandatory integrated\nformat/Python17/typecheck/TS260+3skips pass with738inputs unchanged. No analyzed\nmodule/decorator execution, semantic-provider policy change, whole-native/platform/\nmain/release certification follows. Previous README bytes remain; broaderR1-R9\nwork continues.\n'''.encode()
readme.write_bytes(prefix+append);assert readme.read_bytes().startswith(prefix)
for file in sorted(out.rglob('*')):
    if file.is_file():
        raw=file.read_bytes();index['artifacts'].append({'path':file.relative_to(out).as_posix(),'bytes':len(raw),'sha256':sha(raw)})
(out/'index.json').write_text(json.dumps(index,indent=2,ensure_ascii=False)+'\n',encoding='utf-8')
print(json.dumps({'artifacts':len(index['artifacts']),'indexSha256':sha((out/'index.json').read_bytes()),'priorEvidenceReadme':prior}),flush=True)
