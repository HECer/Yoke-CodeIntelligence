from pathlib import Path
import subprocess,json,hashlib,shutil,datetime,re,tempfile
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence')
base=root/'validation/integration-20261018/mixed-meaning-claims'
out=root/'docs/validation/integration-2026-10-19/mixed-meaning-claims';temp=Path(__file__).parent
sha=lambda b:hashlib.sha256(b).hexdigest();read=lambda n:json.loads((base/n).read_bytes())
def git(*args):return subprocess.check_output(['git','-c','core.longpaths=true',*args],cwd=root,stderr=subprocess.PIPE)
source=read('source-integration.json');final=read('final/result.json');integrated=read('integrated/result.json');green=read('green-r1/result.json')
assert all(x['allChecksPassed'] and x['sourceUnchanged'] for x in [green,final,integrated])
assert green['commands'][0]['totals']=={'passed':43,'failed':0,'ignored':0}
assert read('green-r1/source-before.json')==read('green-r1/source-after.json')==read('final/source-before.json')==read('final/source-after.json')
assert git('rev-parse','HEAD').decode().strip()==source['runtimeCommit'] and not git('status','--porcelain')
assert re.search(r'\bpass\s+260\b',(base/'integrated/tests.log').read_text(encoding='utf-8'))
assert re.search(r'\bskipped\s+3\b',(base/'integrated/tests.log').read_text(encoding='utf-8'))
assert 'Ran 17 tests' in (base/'integrated/python-contracts.log').read_text(encoding='utf-8')
tracker=root/'docs/parity/native-reconciliation-2026-10-15.json';old=json.loads(tracker.read_bytes())
(base/'frozen-tracker.json').write_bytes(tracker.read_bytes())
prior_body=json.loads(subprocess.check_output(['gh','pr','view','7','--repo','HECer/Yoke-CodeIntelligence','--json','body'],cwd=root))['body'].encode()
assert len(prior_body)==6576 and sha(prior_body)=='d3a1d6980320031465eb0e2c29e83f0f875e2e66fc5b5a7bacfe481edf4524ee'
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
r4['mixedInterpretationClaimAddition']='Opaque checked mixed_meaning_claims_v1 validates host-submitted unverified interpretations against exact checked inputHash/unit ranges and re-read typed support quotations. Original concept keys/aliases, claim order, directed parallel/self relations and ordered hyperedge roles remain. No provider receipt or semantic truth is invented; no decoded space becomes native source authority. No execution/persistence is added. Semantic provider extraction, durable mixed graph publication/replay and general graph consumers remain open.'
for path in ['native/src/mixed_meaning_claims.rs','native/docs/mixed-meaning-claims.md','native/tests/mixed_meaning_claims.rs','docs/validation/integration-2026-10-19/mixed-meaning-claims/index.json']:
    if path not in r4['evidence']:r4['evidence'].append(path)
tracker.write_text(json.dumps(old,indent=2,ensure_ascii=False)+'\n',encoding='utf-8')
original=json.loads((base/'frozen-tracker.json').read_bytes())
for key in ['inventoryLedger','historicalInventory','ledgerSummary']:assert old[key]==original[key]
t=source['nativeTotals'];totals=f"{t['passed']} passed / {t['failed']} failed / {t['ignored']} ignored"
p=root/'docs/STATUS.md';text=p.read_text(encoding='utf-8');marker='## Current native Rust implementation'
addition=f'''The checked mixed interpretation claim path adds `--meaning-claims HOST_FILE`
beside explicit prepared inputs. Host-submitted concepts, directed parallel/self
relations and ordered role-bearing hyperedges retain their original keys and
order. Every support descriptor stays inside its submitted typed unit and is
reread from the opaque checked mixed view, including exact Unicode quotation,
XML origins and original PDF/container pins. The original prepared input and
native receipt/source-scope metadata remain nested. Interpretation attestation
stays `host_submitted_unverified_interpretation` / `not_established`; source
quotations do not establish semantic truth or canonical source authority.
This read-only check starts no provider and persists no graph or receipt.

Source `{source['sourceCommit']}`, runtime merge `{source['runtimeCommit']}`.
Five new default cases cover all four spaces, directions/parallel observations,
role slots, invalid support/coverage, exact UTF-8/output/128000-byte quote bounds,
deadlines, explicit no-claim coverage and strict CLI preinitialization refusal.
The actual native worker is exercised by the fixture; one new case invokes the
actual CLI. First four groups43/0/0 and final14groups **{totals}** pass on the
same {final['inputs']} unchanged inputs, with format/build/all-target Clippy.
Integrated format/Python17/typecheck/TypeScript260+3skipped pass with
{integrated['frozenInputs']} unchanged inputs. Counts overlap and are nonadditive.
Semantic provider extraction, durable mixed graph replay/general consumers and
broader R1–R9 remain active. Whole-native/platform/main/release acceptance does
not follow. See [the contract](../native/docs/mixed-meaning-claims.md) and
[indexed evidence](validation/integration-2026-10-19/mixed-meaning-claims/index.json).

'''
assert marker in text;p.write_text(text.replace(marker,addition+marker,1),encoding='utf-8')
p=root/'docs/ARCHITECTURE.md';text=p.read_text(encoding='utf-8');marker='## Native Rust ownership\n'
addition='''
`mixed_meaning_claims` checks host-submitted interpretation fragments against
opaque checked mixed inputs. The shared fragment shape validator receives only
range descriptors; legacy native callers still validate their original Binding
paths, hashes and byte bounds first. The mixed caller creates no native Binding
or SourceSnapshot. Exact support quotations reuse the original typed reader and
retain native/decoded/XML/PDF coordinate precision and origins. The checked result
borrows the checked inputs, preserves the original fragment including directed
parallel observations and ordered roles, and exports nested source/receipt
provenance. Host interpretation attestation remains unverified. No provider
receipt, persistence, model execution or semantic truth is created. See
[mixed interpretation claims](../native/docs/mixed-meaning-claims.md).
'''
assert marker in text;p.write_text(text.replace(marker,marker+addition,1),encoding='utf-8')
p=root/'README.md';text=p.read_text(encoding='utf-8')
p.write_text(text+'''\n[Mixed interpretation claim checks](native/docs/mixed-meaning-claims.md) retain
host-submitted directed/parallel/role-bearing observations and reread exact typed
source support. The result preserves unverified interpretation status and
original source provenance without a provider call or persisted graph.
''',encoding='utf-8')
out.mkdir(exist_ok=False)
for file in sorted(base.rglob('*')):
    if file.is_file():
        target=out/file.relative_to(base);target.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(file,target)
runners=out/'runners';runners.mkdir()
for file in sorted(temp.glob('ci-claims-*.py')):shutil.copyfile(file,runners/file.name)
p=root/'docs/validation/integration-2026-10-19/README.md';prefix=p.read_bytes();prior={'bytes':len(prefix),'sha256':sha(prefix)}
p.write_bytes(prefix+f'''\n## Checked mixed interpretation claim checkpoint\n\nSource `{source['sourceCommit']}`, runtime merge `{source['runtimeCommit']}`.\n[Indexed evidence](mixed-meaning-claims/index.json) retains5new default cases\n(including actual nativeCLI), green4groups43/0/0 and final14groups{t['passed']}/{t['failed']}/{t['ignored']}.\nExact typed quotations preserve four source spaces, original prepared inputs/\nreceipts, parallel directed/self relations and ordered hyperedge roles. Host\ninterpretation remains unverified; no provider/persistence/source authority is\nadded. Quality checks, Python17, typecheck and TypeScript260+3skip pass with\n{final['inputs']}native/{integrated['frozenInputs']}integratedinputs unchanged. Semantic provider execution,\ndurable mixed graphs/general consumers and broaderR1-R9 remain open.\n'''.encode())
index={'schemaVersion':1,'kind':'checked_mixed_interpretation_claims','asOf':datetime.datetime.now(datetime.timezone.utc).isoformat(),
    **source,'platform':'Windows','firstGreen':green,'finalNative':final,'integrated':integrated,'priorEvidenceReadme':prior,
    'priorPRBodyArchive':{'path':'prior-pr-body.md','bytes':len(prior_body),'sha256':sha(prior_body)},
    'legacyNativeBindingAdmissionChanged':False,'originalQuerySchemaOrIdentityChanged':False,
    'decodedSpacePromotedToNativeSource':False,'providerOrModelCallAdded':False,'providerReceiptInvented':False,
    'persistenceAdded':False,'fullR4Complete':False,'nonWindowsRuntimeValidated':False,'mainOrReleaseVerified':False,
    'notes':['Five default tests include actual native PDF helpers and one actual CLI case. No ignored gate/model-quality/live provider qualification is claimed. The first four groups43/0/0 and final14groups are overlapping and nonadditive; all native phases use exactly the same frozen source.',
        'Range-only shared shape checking constructs no native path/Binding/SourceSnapshot. The legacy native wrapper retains original native binding validation first. Mixed supports are privately bound to already checked units and exact original typed reader quotations, including enclosing XML origins and explicit PDF pins.',
        'Original fragment keys/order/aliases, directed parallel/self observations and ordered hyperedge roles remain intact. Unique quote lookup does not merge claim occurrences or concept identity. Exact quote text capacity128000 passes and128001 refuses without writes; no-claim coverage is an explicit unverified host observation.',
        'No semantic provider execution, durable mixed graph publication/replay/general consumers, complete R4/R1-R9 or whole-native/platform/main/release acceptance follows. Ledger inventories and historical evidence remain.'],
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
with tempfile.NamedTemporaryFile(prefix='ci-claims-stage-',suffix='.bin',delete=False) as f:
    f.write(b'\0'.join(p.encode() for p in paths)+b'\0');spec=Path(f.name)
try:git('add','-f','--pathspec-from-file='+str(spec),'--pathspec-file-nul')
finally:spec.unlink()
assert set(filter(None,git('diff','--cached','--name-only','-z').decode().split('\0')))==set(paths)
for item in index['artifacts']:assert sha(git('show',':'+(out/item['path']).relative_to(root).as_posix()))==item['sha256']
git('-c','core.whitespace=cr-at-eol','diff','--cached','--check')
git('commit','-m','docs: record checked mixed interpretation claim evidence')
assert not git('status','--porcelain')
print(json.dumps({'head':git('rev-parse','HEAD').decode().strip(),'artifacts':len(index['artifacts']),'indexSha256':sha((out/'index.json').read_bytes()),'priorEvidenceReadme':prior}),flush=True)
