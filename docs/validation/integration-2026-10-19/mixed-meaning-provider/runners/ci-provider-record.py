from pathlib import Path
import subprocess,json,hashlib,shutil,datetime,re,tempfile
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');base=root/'validation/integration-20261018/mixed-meaning-provider'
out=root/'docs/validation/integration-2026-10-19/mixed-meaning-provider';temp=Path(__file__).parent
sha=lambda b:hashlib.sha256(b).hexdigest();read=lambda n:json.loads((base/n).read_bytes())
def git(*args):return subprocess.check_output(['git','-c','core.longpaths=true',*args],cwd=root,stderr=subprocess.PIPE)
source=read('source-integration.json');final=read('final-r2/result.json');integrated=read('integrated/result.json')
assert all(x['allChecksPassed'] and x['sourceUnchanged'] for x in [final,integrated])
assert final['commands'][0]['groups']==18 and source['newCases']['defaultIntegration']==11
assert read('feature-red-r1/result.json')['commands'][0]['totals']=={'passed':4,'failed':1,'ignored':0}
assert read('green-r3/result.json')['commands'][0]['totals']=={'passed':43,'failed':0,'ignored':0}
assert read('final/result.json')['commands'][0]['totals']==final['commands'][0]['totals']
assert read('final/result.json')['commands'][-1]['exitCode']==101
assert git('rev-parse','HEAD').decode().strip()==source['runtimeCommit'] and not git('status','--porcelain')
assert re.search(r'\bpass\s+260\b',(base/'integrated/tests.log').read_text(encoding='utf-8'))
assert re.search(r'\bskipped\s+3\b',(base/'integrated/tests.log').read_text(encoding='utf-8'))
assert 'Ran 17 tests' in (base/'integrated/python-contracts.log').read_text(encoding='utf-8')
tracker=root/'docs/parity/native-reconciliation-2026-10-15.json';old=json.loads(tracker.read_bytes())
(base/'frozen-tracker.json').write_bytes(tracker.read_bytes())
prior_body=json.loads(subprocess.check_output(['gh','pr','view','7','--repo','HECer/Yoke-CodeIntelligence','--json','body'],cwd=root))['body'].encode()
assert len(prior_body)==6680 and sha(prior_body)=='dc606d4db74a5c30cfb5d34316b28652af0b4da0ef46911f8123c8ad8207511a'
(base/'prior-pr-body.md').write_bytes(prior_body)
def find_r4(value):
    if isinstance(value,dict):
        if value.get('id')=='R4':return value
        for v in value.values():
            found=find_r4(v)
            if found is not None:return found
    if isinstance(value,list):
        for v in value:
            found=find_r4(v)
            if found is not None:return found
    return None
r4=find_r4(old);assert r4 is not None
r4['mixedSemanticProviderAddition']='Explicit trusted mixed-meaning build admits one fixed typed input set, fingerprints host provider inputs, atomically reserves/registers a single-use ledger/job, records fully bound costs before domain/error rejection, preserves unknown holds without retries, and publishes immutable response/source-bound graph artifacts with exact row preconditions. Provider-free status/jobs and source/response graph replay retain original pins/provenance, directions/parallel claims/ordered roles; PDF source replay needs separate host worker permission. Typed retention reconstructs saved-data quotations and real parent roots without a PDF process. Own Node protocol fixtures do not qualify semantic model quality or independent billing. General checked graph consumers/cross-workspace joins/fullR4 remain open.'
r4['missingBehavior']='General mixed-space graph lookup/analytics, cross-workspace joins and full external semantic-provider qualification remain open. Typed preparation, checked host claims and explicit source-bound provider jobs with durable accounting/replay have bounded Windows evidence; no complete layout/OCR, model-quality or exhaustive binary-document interpretation is implied.'
r4['suggestedScope']='Add opaque checked mixed interpretation graph lookup/degree/directed-trace consumers without promoting serialized JSON, aliases or provider claims into canonical source authority. Preserve original typed source/receipt replay, parallel observations and ordered role slots; retain explicit PDF worker and trusted-provider boundaries.'
for path in ['native/src/mixed_meaning_provider.rs','native/src/mixed_meaning_cli.rs','native/src/retention.rs','native/tests/mixed_meaning_provider.rs','native/docs/mixed-meaning-provider.md','docs/validation/integration-2026-10-19/mixed-meaning-provider/index.json']:
    if path not in r4['evidence']:r4['evidence'].append(path)
tracker.write_text(json.dumps(old,indent=2,ensure_ascii=False)+'\n',encoding='utf-8')
original=json.loads((base/'frozen-tracker.json').read_bytes())
for key in ['inventoryLedger','historicalInventory','ledgerSummary']:assert old[key]==original[key]
t=source['nativeTotals'];totals=f"{t['passed']} passed / {t['failed']} failed / {t['ignored']} ignored"
p=root/'docs/STATUS.md';text=p.read_text(encoding='utf-8');marker='## Current native Rust implementation'
addition=f'''The separate `mixed-meaning build` now executes one fully admitted, explicitly
trusted host request over typed native/Office/PDF units and fixed context groups.
It registers a single-use ledger/job before sending, accounts for a completely
bound reply before semantic/error rejection, retains unknown reservations and
publishes immutable original response and graph artifacts with exact row
preconditions. Status/jobs start no provider; load replays original source,
receipts, exact quotations and ledger response events. PDF replay retains its
own explicit worker permission. Retention recomputes native/document/media
dependencies and stored quote consistency without launching a PDF process.
Original aliases, claim order, directed parallel/self observations and ordered
hyperedge roles remain. Interpretations/usage/bills stay unverified; later ledger
reconciliation does not upgrade original unknown receipts. No default model
call, analyzed-source capture, automatic project-code execution or native-source
promotion is added.

Source `{source['sourceCommit']}`, runtime merge `{source['runtimeCommit']}`.
Eleven new default cases include actual owned Node/PDF protocol processes and
one end-to-end CLI workflow, Unicode/four-space evidence, invalid usage/claims/
error/overrun accounting, malformed/unbound replies, unknown-cost reconciliation,
caller transaction/SQLite limits, metadata4096/4097 admission, private malformed
host config and corrupted quote retention. These are protocol/host-process
qualifications; no semantic model, real service billing or model quality was
evaluated. Final18groups **{totals}** and format/build/all-target Clippy pass
with {final['inputs']} unchanged inputs. Integrated format/Python17/typecheck/
TypeScript260+3skipped pass with {integrated['frozenInputs']} inputs unchanged.
Earlier failed/diagnostic variants remain separate and nonadditive. General
mixed graph consumers, cross-workspace composition, full R4/R1–R9, whole-native/
platform/main/release acceptance remain open. See [the contract](../native/docs/mixed-meaning-provider.md)
and [indexed evidence](validation/integration-2026-10-19/mixed-meaning-provider/index.json).

'''
assert marker in text;p.write_text(text.replace(marker,addition+marker,1),encoding='utf-8')
p=root/'docs/ARCHITECTURE.md';text=p.read_text(encoding='utf-8');marker='## Native Rust ownership\n'
addition='''
`mixed_view::provider` and the separate `mixed-meaning` CLI use a fixed typed
mixed-source wire contract under explicit trusted-host permission. Existing
fingerprinting takes host input paths without a fabricated native selection.
Atomic ledger/job registration precedes one single-use send; a fully bound bill
is recorded before domain/error rejection. Unknown sends retain reservations
without retries. Immutable input, payload, raw response and graph artifacts are
linked through exact row preconditions. Provider-free status/jobs preserve
original versus later aggregate accounting; graph load replays checked source
and response under separate PDF worker permission. Typed retention recomputes
native/document/media roots and exact saved-data quote consistency without a
PDF process. Saved JSON supplies no checked source capability. Interpretations
and provider reports remain unverified; native parent source writes remain off,
while an explicitly trusted provider has no claimed OS/network isolation. See
[mixed provider builds](../native/docs/mixed-meaning-provider.md).
'''
assert marker in text;p.write_text(text.replace(marker,marker+addition,1),encoding='utf-8')
p=root/'README.md';text=p.read_text(encoding='utf-8');p.write_text(text+'''\nExplicit [mixed-source meaning builds](native/docs/mixed-meaning-provider.md)
add trusted host-provider execution, durable accounting/response artifacts and
source-bound replay. Status/jobs start no provider; unknown sends retain their
reservation and never retry automatically. Interpretations remain unverified.
''',encoding='utf-8')
out.mkdir(exist_ok=False)
for file in sorted(base.rglob('*')):
    if file.is_file():
        target=out/file.relative_to(base);target.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(file,target)
runners=out/'runners';runners.mkdir()
for file in sorted(temp.glob('ci-provider-*.py')):shutil.copyfile(file,runners/file.name)
p=root/'docs/validation/integration-2026-10-19/README.md';prefix=p.read_bytes();prior={'bytes':len(prefix),'sha256':sha(prefix)}
p.write_bytes(prefix+f'''\n## Explicit mixed semantic-provider checkpoint\n\nSource `{source['sourceCommit']}`, runtime merge `{source['runtimeCommit']}`.\n[Indexed evidence](mixed-meaning-provider/index.json) retains11new default cases,\nowned Node/PDF process and actual CLI protocol flows, unknown holds/bound bills,\nsource/response replay and corruption-refusing retention. Final18groups\n{t['passed']}/{t['failed']}/{t['ignored']}, quality, Python17, typecheck and TypeScript260+3skip pass with\n{final['inputs']}native/{integrated['frozenInputs']}integratedinputs unchanged. Original native source recipes,\nledger inventories and prior evidence remain. No semantic-model quality, real\nbilling, fullR4/upstream/platform/main/release certification follows; broader\nSTATUS/R1-R9 stays active.\n'''.encode())
index={'schemaVersion':1,'kind':'explicit_checked_mixed_meaning_provider','asOf':datetime.datetime.now(datetime.timezone.utc).isoformat(),
    **source,'platform':'Windows','finalNative':final,'integrated':integrated,'priorEvidenceReadme':prior,
    'priorPRBodyArchive':{'path':'prior-pr-body.md','bytes':len(prior_body),'sha256':sha(prior_body)},
    'ownedProtocolProcessesIncludedInDefault':True,'liveSemanticModelValidated':False,'realServiceBillingVerified':False,
    'defaultModelCallAdded':False,'nativeSnapshotOrGraphRecipeChanged':False,'sourceCaptureOrCodeWritePermissionAdded':False,
    'decodedSpacePromotedToNativeSource':False,'fullR4Complete':False,'nonWindowsRuntimeValidated':False,'mainOrReleaseVerified':False,
    'notes':['Prerequisites31/0/0 on742inputs validate only the initial fingerprint-helper extraction and checked-request ownership refactor. Feature-red-r1 4/1/0 on744inputs reproduces corrupted quotes incorrectly admitted by retention; the full saved-data codec fixes that case. Green-r1 fourgroups39/3/0 reveals an invented decodedTextHash registration field (actual value is in the checked document graph) and a parent fixture client blocking external CLI exclusive access; green-r2 6/2/0 confirms only the document pin issue remains. Existing checked document loading supplies its actual decoded hash; green-r3 fourgroups43/0/0 passes with9provider cases on745inputs. Final adds2budget/privacy cases and current docs.',
        'Initial final18groups614/0/7 pass and fmt/build pass on746inputs; all-targetClippy rejects only2collapsible_if branches. Final-r2 retains identical selection/counts with exactly one source-file delta combining those boolean/pattern conditions; no accounting or output order changes. All failed/diagnostic/final counts overlap and are nonadditive. Complete phase source proofs label exact raw captures versus hash-matched parent Git/CRLF reconstructions honestly.',
        'New closed operation extract_mixed_meaning_v1 never fabricates native Binding/SourceSnapshot authority. Model calls require separate explicit trusted-host config/cost policy; native indexing, mixed-view and facade remain model-free. One fixed admitted payload, original typed pins/text/context order, bounded artifacts, exact row preconditions and single-use Ledger reservations remain. Unknown sends cannot retry or refund automatically. Bound valid costs survive bad claims, invalid usage, process/provider errors and overruns. No cost clamp or original-unknown receipt upgrade occurs.',
        'Default cases use actual owned local Node protocol scripts and the real native PDF helper/CLI. Script input uses UTF8 streaming; all four spaces, nonBMP byte spans, parallel directions/roles, malformed/duplicate/unbound envelopes, cost/usage/error/overrun failures, missing-provider/live-file replay, metadata jobs/quotas and SQLite/caller-transaction boundaries are exercised. This is not a semantic model, provider-internal call count, real paid-service billing or quality qualification.',
        'Retention verifies stored graph/quote consistency from saved data and reconstructs native/document/media dependency keys without a PDF process. It grants no source capability; graph load independently repeats actual checked source/receipt replay with explicit PDF permission. Caller-selected provider fingerprint files/environment imports are not runtime attestation; trusted provider network/filesystem effects are not claimed isolated. sourceWritten=false is native-parent scoped.',
        'General mixed graph consumers/cross-workspace/fullR4/R1-R9, full native/platform/main/release remain open. Original353inventory/73unreconciled/77backendunknown and falsefullContract flags remain. No PR/source finalization releases the active whole-task AgentMemory actions.'],
    'artifacts':[]}
for file in sorted(out.rglob('*')):
    if file.is_file():
        raw=file.read_bytes();index['artifacts'].append({'path':file.relative_to(out).as_posix(),'bytes':len(raw),'sha256':sha(raw)})
(out/'index.json').write_text(json.dumps(index,indent=2,ensure_ascii=False)+'\n',encoding='utf-8')
paths=['README.md','docs/STATUS.md','docs/ARCHITECTURE.md','docs/parity/native-reconciliation-2026-10-15.json','docs/validation/integration-2026-10-19/README.md']
for item in index['artifacts']:
    file=out/item['path'];assert file.resolve().is_relative_to(out.resolve()) and not file.is_symlink()
    assert sha(file.read_bytes())==item['sha256'] and file.stat().st_size==item['bytes'];paths.append(file.relative_to(root).as_posix())
paths.append((out/'index.json').relative_to(root).as_posix())
assert not git('diff','--cached','--name-only') and set(git('diff','--name-only').decode().splitlines())==set(paths[:5])
with tempfile.NamedTemporaryFile(prefix='ci-provider-stage-',suffix='.bin',delete=False) as f:
    f.write(b'\0'.join(p.encode() for p in paths)+b'\0');spec=Path(f.name)
try:git('add','-f','--pathspec-from-file='+str(spec),'--pathspec-file-nul')
finally:spec.unlink()
assert set(filter(None,git('diff','--cached','--name-only','-z').decode().split('\0')))==set(paths)
for item in index['artifacts']:assert sha(git('show',':'+(out/item['path']).relative_to(root).as_posix()))==item['sha256']
git('-c','core.whitespace=cr-at-eol','diff','--cached','--check')
git('commit','-m','docs: record explicit mixed meaning provider validation')
assert not git('status','--porcelain')
print(json.dumps({'head':git('rev-parse','HEAD').decode().strip(),'artifacts':len(index['artifacts']),'indexSha256':sha((out/'index.json').read_bytes()),'priorEvidenceReadme':prior}),flush=True)
