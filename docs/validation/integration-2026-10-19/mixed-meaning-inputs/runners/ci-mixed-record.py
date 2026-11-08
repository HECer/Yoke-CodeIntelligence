from pathlib import Path
import subprocess,json,hashlib,shutil,datetime,re,tempfile
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence')
base=root/'validation/integration-20261018/mixed-meaning-inputs'
out=root/'docs/validation/integration-2026-10-19/mixed-meaning-inputs'
temp=Path(__file__).parent
sha=lambda b:hashlib.sha256(b).hexdigest()
read=lambda n:json.loads((base/n).read_bytes())
def git(*args):return subprocess.check_output(['git','-c','core.longpaths=true',*args],cwd=root,stderr=subprocess.PIPE)
source=read('source-integration.json');final=read('final/result.json');integrated=read('integrated/result.json')
assert final['allChecksPassed'] and final['sourceUnchanged'] and integrated['allChecksPassed'] and integrated['sourceUnchanged']
assert git('rev-parse','HEAD').decode().strip()==source['runtimeCommit'] and not git('status','--porcelain')
assert re.search(r'\bpass\s+260\b',(base/'integrated/tests.log').read_text(encoding='utf-8'))
assert re.search(r'\bskipped\s+3\b',(base/'integrated/tests.log').read_text(encoding='utf-8'))
assert 'Ran 17 tests' in (base/'integrated/python-contracts.log').read_text(encoding='utf-8')
tracker_path=root/'docs/parity/native-reconciliation-2026-10-15.json'
old=json.loads(tracker_path.read_bytes());(base/'frozen-tracker.json').write_bytes(tracker_path.read_bytes())
prior_body=json.loads(subprocess.check_output(['gh','pr','view','7','--repo','HECer/Yoke-CodeIntelligence','--json','body'],cwd=root))['body'].encode()
assert len(prior_body)==6617 and sha(prior_body)=='21979084ab76c249161d69253c2a9da0b36766a04ee3db47218ed36b730dbea0'
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
r4['meaningInputPreparationAddition']='Opaque checked mixed_meaning_inputs_v1 preparation preserves exact source/Office XML/decoded text/explicit PDF page reads, original mixed receipt/source replay, ordered repeated context occurrences, bounded text/materialization/output, and deterministic selected-pin/item/label binding. Data preparation calls no model/provider and writes no source or graph. Mixed semantic execution and general cross-source graph links remain open.'
for path in ['native/src/mixed_meaning_input.rs','native/docs/mixed-views.md','native/tests/mixed_view_cli.rs','native/tests/mixed_pdf_view.rs','docs/validation/integration-2026-10-19/mixed-meaning-inputs/index.json']:
    if path not in r4['evidence']:r4['evidence'].append(path)
tracker_path.write_text(json.dumps(old,indent=2,ensure_ascii=False)+'\n',encoding='utf-8')
original=json.loads((base/'frozen-tracker.json').read_bytes())
for key in ['inventoryLedger','historicalInventory','ledgerSummary']:assert original[key]==old[key]
t=source['nativeTotals'];summary=f"{t['passed']} passed / {t['failed']} failed / {t['ignored']} ignored"
p=root/'docs/STATUS.md';text=p.read_text(encoding='utf-8');marker='## Current native Rust implementation'
addition=f'''The checked mixed-source input preparation adds a closed `--meaning-inputs`
request and opaque library capability over existing frozen native, Office
decoded/XML and explicitly permitted PDF page reads. Exact excerpts, original
View/Meaning receipts and coordinate-space precision remain intact. Ordered
context occurrences count repeated bytes separately; host labels establish no
semantic identity. Preparation starts no provider/model or acquisition and
persists no input, source or graph. Mixed semantic execution and general
cross-source graph relationships remain open.

Source `{source['sourceCommit']}`, runtime merge `{source['runtimeCommit']}`.
Seven new default cases include the actual native PDF worker/CLI permission
case; they are included in, not added to, the final 13 selected groups:
**{summary}**. Format, build and all-target Clippy with warnings denied pass;
{final['inputs']} native inputs remain unchanged. Integrated format, Python17,
typecheck and TypeScript260/3skipped pass with {integrated['frozenInputs']}
inputs unchanged. The earlier compile-only lifetime failure is retained and
does not count as executed tests. This is scoped Windows evidence, not complete
R4, whole-native, platform, main or release acceptance. Broader R1–R9 remains
active. See [the contract](../native/docs/mixed-views.md) and
[indexed evidence](validation/integration-2026-10-19/mixed-meaning-inputs/index.json).

'''
assert marker in text;text=text.replace(marker,addition+marker,1);p.write_text(text,encoding='utf-8')
p=root/'docs/ARCHITECTURE.md';text=p.read_text(encoding='utf-8');marker='## Native Rust ownership\n'
addition='''
`mixed_meaning_input` prepares opaque checked input data from `CheckedMixedView`.
Typed native, Office decoded/XML and explicitly permitted PDF page references
reuse the existing checked reader. Bounded owned read items preserve exact text,
coordinate precision and enclosing XML/container evidence; nested mixed metadata
retains original native View/Meaning receipt and source-scope provenance. Unit IDs
bind selected pins, labels and exact items; the input hash also binds request
order and repeated context occurrences. Host selections establish no semantic
identity. Exported JSON cannot construct the capability. Preparation performs no
provider execution, source acquisition or persistence; mixed semantic execution
and graph relationships remain separate open work. See [mixed inputs](../native/docs/mixed-views.md).
'''
assert marker in text;text=text.replace(marker,marker+addition,1);p.write_text(text,encoding='utf-8')
p=root/'README.md';text=p.read_text(encoding='utf-8')
text+='''
Explicit [mixed-source input preparation](native/docs/mixed-views.md) exports
checked frozen code, Office text/XML and permitted PDF page excerpts with their
original provenance and bounded ordered context groups. It performs no model
call; mixed semantic execution and cross-source graph relationships remain open.
''';p.write_text(text,encoding='utf-8')
out.mkdir(exist_ok=False)
for file in sorted(base.rglob('*')):
    if file.is_file():
        target=out/file.relative_to(base);target.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(file,target)
runners=out/'runners';runners.mkdir()
for file in sorted(temp.glob('ci-mixed-*.py')):shutil.copyfile(file,runners/file.name)
p=root/'docs/validation/integration-2026-10-19/README.md';prefix=p.read_bytes()
prior={'bytes':len(prefix),'sha256':sha(prefix)}
p.write_bytes(prefix+f'''\n## Checked mixed meaning input preparation\n\nSource `{source['sourceCommit']}`, runtime merge `{source['runtimeCommit']}`.\n[Indexed evidence](mixed-meaning-inputs/index.json) retains7new default cases\n(including actual nativePDFworker/CLI), final13selectedgroups{t['passed']}/{t['failed']}/{t['ignored']},\nquality checks and unchanged{final['inputs']}native/{integrated['frozenInputs']}integratedinputs.\nPython17, typecheck and TypeScript260+3skip pass. Exact typed source spaces and\noriginal receipt replay remain; preparation is data only without provider calls\nor persistence. Mixed semantic execution/graph links and broaderR1-R9 remain open.\n'''.encode())
index={'schemaVersion':1,'kind':'checked_mixed_meaning_input_preparation','asOf':datetime.datetime.now(datetime.timezone.utc).isoformat(),
    **source,'platform':'Windows','finalNative':final,'integrated':integrated,'priorEvidenceReadme':prior,
    'priorPRBodyArchive':{'path':'prior-pr-body.md','bytes':len(prior_body),'sha256':sha(prior_body)},
    'compileOnlyAttempt':'green-r1','firstGreen':'green-r2',
    'originalQuerySchemaOrIdentityChanged':False,'decodedCoordinatesPromotedToNativeSource':False,
    'providerOrModelCallAdded':False,'sourceAcquisitionAdded':False,'persistenceAdded':False,
    'fullR4Complete':False,'nonWindowsRuntimeValidated':False,'mainOrReleaseVerified':False,
    'notes':['green-r1 failed at compile time: extracted read item borrowed its local scope; no tests executed. The corrected helper borrows the caller scope; preparation owns only bounded exact read data. green-r2 passes30/0/0 before the final limit/preinitialization case and README addition; final evidence is separate and nonadditive.',
        'The new PDF case uses the actual native executable through the existing bounded helper; it is a default test, not an additional ignored gate or independent model-quality validation.',
        'Host labels and context groups do not establish semantic identity; repeated ordered occurrences remain and count byte-exactly. Exported JSON cannot construct CheckedMeaningInputs. All source/receipt authority comes from the already checked mixed view.',
        'Existing ledger inventories and historical evidence remain. This finite preparation slice does not complete semantic execution, cross-source graph replay, R4/R1-R9 or full platform/release acceptance.'],
    'artifacts':[]}
for file in sorted(out.rglob('*')):
    if file.is_file():
        raw=file.read_bytes();index['artifacts'].append({'path':file.relative_to(out).as_posix(),'bytes':len(raw),'sha256':sha(raw)})
(out/'index.json').write_text(json.dumps(index,indent=2,ensure_ascii=False)+'\n',encoding='utf-8')
paths=['README.md','docs/STATUS.md','docs/ARCHITECTURE.md','docs/parity/native-reconciliation-2026-10-15.json','docs/validation/integration-2026-10-19/README.md']
for item in index['artifacts']:
    file=out/item['path'];assert file.resolve().is_relative_to(out.resolve()) and not file.is_symlink()
    raw=file.read_bytes();assert sha(raw)==item['sha256'] and len(raw)==item['bytes']
    paths.append(file.relative_to(root).as_posix())
paths.append((out/'index.json').relative_to(root).as_posix())
assert not git('diff','--cached','--name-only')
assert set(git('diff','--name-only').decode().splitlines())==set(paths[:5])
with tempfile.NamedTemporaryFile(prefix='ci-mixed-stage-',suffix='.bin',delete=False) as f:
    f.write(b'\0'.join(p.encode() for p in paths)+b'\0');spec=Path(f.name)
try:git('add','-f','--pathspec-from-file='+str(spec),'--pathspec-file-nul')
finally:spec.unlink()
assert set(filter(None,git('diff','--cached','--name-only','-z').decode().split('\0')))==set(paths)
for item in index['artifacts']:assert sha(git('show',':'+(out/item['path']).relative_to(root).as_posix()))==item['sha256']
git('-c','core.whitespace=cr-at-eol','diff','--cached','--check')
git('commit','-m','docs: record checked mixed-source input preparation evidence')
assert not git('status','--porcelain')
print(json.dumps({'head':git('rev-parse','HEAD').decode().strip(),'artifacts':len(index['artifacts']),'indexSha256':sha((out/'index.json').read_bytes()),'priorEvidenceReadme':prior}),flush=True)
