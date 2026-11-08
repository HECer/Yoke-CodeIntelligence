from pathlib import Path
import subprocess,json,hashlib,shutil,re,datetime
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence')
work=root.parent/'Yoke-CI-r1-attach';base=root/'validation/integration-20261018/mixed-meaning-consumers'
out=root/'docs/validation/integration-2026-10-19/mixed-meaning-consumers';temp=Path(__file__).parent
sha=lambda b:hashlib.sha256(b).hexdigest()
read=lambda p:json.loads((base/p).read_bytes())
def git(*args):return subprocess.check_output(['git','-c','core.longpaths=true',*args],cwd=root,stderr=subprocess.PIPE)
def write(p,v):p.write_text(json.dumps(v,indent=2,ensure_ascii=False)+'\n',encoding='utf-8')
source=read('source-integration.json');final=read('final-r2/result.json');integrated=read('integrated/result.json')
assert all(r['allChecksPassed'] and r['sourceUnchanged'] for r in [final,integrated])
assert final['commands'][0]['groups']==19 and source['newCases']['defaultIntegration']==8
assert read('final/source-before.json')==read('final-r2/source-before.json')
assert read('feature-r1/result.json')['commands'][0]['totals']=={'passed':17,'failed':1,'ignored':0}
assert git('rev-parse','HEAD').decode().strip()==source['runtimeCommit'] and not git('status','--porcelain')
assert 'Ran 17 tests' in (base/'integrated/python-contracts.log').read_text(encoding='utf-8')
ts=(base/'integrated/tests.log').read_text(encoding='utf-8');assert re.search(r'\bpass\s+260\b',ts) and re.search(r'\bskipped\s+3\b',ts)
# Recover only raw source variants whose hashes actually match phase inventories.
proof=[]
for phase in ['feature-r1','final','final-r2']:
    manifest=read(phase+'/source-before.json');assert manifest==read(phase+'/source-after.json')
    for name in source['paths']:
        if name not in manifest:continue
        candidates=[('current_scoped_exact',(work/name).read_bytes())]
        saved=base/phase/'source'/name
        if saved.exists():candidates.insert(0,('original_phase_capture',saved.read_bytes()))
        try:
            old=git('show',source['parentCommit']+':'+name)
            candidates += [('parent_git_lf',old),('parent_crlf_materialization',old.replace(b'\r\n',b'\n').replace(b'\n',b'\r\n'))]
        except subprocess.CalledProcessError:pass
        if phase=='feature-r1' and name=='native/tests/mixed_meaning_queries.rs':
            current=(work/name).read_text(encoding='utf-8')
            initial=current.split('\n#[test]\nfn malformed_cli_query_is_private_and_refuses_before_missing_store_initialization')[0].rstrip()+'\n'
            initial=initial.replace('"mixed_view_input_invalid"','"mixed_view_selection_invalid"')
            candidates += [('reconstructed_exact_hash_matched_initial_test',initial.encode()),('reconstructed_exact_hash_matched_initial_test_crlf',initial.replace('\n','\r\n').encode())]
        match=next(((origin,b) for origin,b in candidates if sha(b)==manifest[name]),None)
        assert match is not None,(phase,name)
        saved.parent.mkdir(parents=True,exist_ok=True);saved.write_bytes(match[1])
        proof.append({'phase':phase,'path':name,'sha256':sha(match[1]),'origin':match[0]})
write(base/'source-completeness.json',proof)
prior=json.loads(subprocess.check_output(['gh','pr','view','7','--repo','HECer/Yoke-CodeIntelligence','--json','body'],cwd=root))['body'].encode()
assert len(prior)==7172 and sha(prior)=='d5be8ca829d0b006642298a4387e223ed7240ed5d8a98724c2b168e1a64af9de'
(base/'prior-pr-body.md').write_bytes(prior)
tracker=root/'docs/parity/native-reconciliation-2026-10-15.json';old=json.loads(tracker.read_bytes());original=json.loads(tracker.read_bytes())
(base/'frozen-tracker.json').write_bytes(tracker.read_bytes())
def find(v):
    if isinstance(v,dict):
        if v.get('id')=='R4':return v
        for x in v.values():
            r=find(x)
            if r is not None:return r
    elif isinstance(v,list):
        for x in v:
            r=find(x)
            if r is not None:return r
r4=find(old);assert r4
r4['mixedCheckedGraphQueryAddition']='Opaque source-replayed CheckedGraph owns original native/Office/PDF source proof and exact verified provider graph. Storeless literal label/alias lookup, graph-qualified concept selection, incoming/outgoing multiedge degree counts and bounded directed frontier traces preserve original indices, self-loops, ordered repeated hyperedge slots and exact nested source/receipt/accounting provenance. Hyperedge roles imply no traversal flow. Existing load JSON/artifact identities remain unchanged; serialized JSON cannot construct the capability. Explicit PDF permission is required at load only, no query provider call or source/metadata write. Eight default adversarial/CLI cases include total Store loss; no semantic truth or full R4 certification follows.'
r4['missingBehavior']='Cross-graph/workspace mixed-source composition, broader mixed graph analytics and full external semantic-provider qualification remain open. Typed preparation, checked host claims, explicit source-bound provider jobs and checked single-graph consumers have bounded Windows evidence; no complete layout/OCR, model-quality or exhaustive binary-document interpretation is implied.'
r4['suggestedScope']='Define explicit checked mixed-source cross-graph composition with graph-qualified local concepts and source/receipt provenance, preserving parallel observations, ordered roles and typed coordinate spaces. Add broader analytics only over admitted checked capabilities; retain explicit PDF and provider boundaries and do not promote labels or aliases into semantic identity.'
for path in ['native/src/mixed_meaning_query.rs','native/tests/mixed_meaning_queries.rs','native/docs/mixed-meaning-queries.md','docs/validation/integration-2026-10-19/mixed-meaning-consumers/index.json']:
    if path not in r4['evidence']:r4['evidence'].append(path)
for key in ['inventoryLedger','historicalInventory','ledgerSummary']:assert old[key]==original[key]
write(tracker,old)
t=source['nativeTotals']
addition=f'''The checked mixed interpretation graph now supports graph-qualified concept
lookup, case-sensitive literal label/alias search, directed multiedge degree
counts and bounded frontier traces. Only actual source/response replay can
construct the opaque capability, which owns its frozen source proof and exact
verified graph. Queries survive later metadata corruption or complete Store/CAS
loss without another provider/PDF process. Original graph IDs/load bytes, typed
quotations, receipts, parallel array positions, self-loops and ordered repeated
hyperedge roles remain intact. Roles do not create traversal direction, aliases
do not merge concepts, and complete/canonical-source authority remain false.
The explicit CLI query retains independent PDF permission at load; malformed
query shape refuses before missing Store initialization.

Source `{source['sourceCommit']}`, runtime merge `{source['runtimeCommit']}`.
Eight new default cases include actual owned Node/PDF setup and end-to-end CLI
query, Unicode/ambiguous labels, foreign graph references, all directions/depth0,
self/parallel/role counts, Store loss, closed JSON/privacy, output/deadline
refusals and unchanged records/provider-call markers. Initial17/1 and the first final run's disk-full linker refusal are retained:
the latter executes no complete native acceptance and does not change source.
The initial17/1 run's sole failure expected the wrong static refusal code; no production fix was
needed for that case. Final19groups **{t['passed']} passed / {t['failed']} failed /
{t['ignored']} ignored**, format/build/all-target Clippy pass with {final['inputs']} unchanged
inputs. Integrated format/Python17/typecheck/TypeScript260+3skipped pass with
{integrated['frozenInputs']} inputs unchanged. Counts are scoped and nonadditive; full R4,
cross-workspace composition, external semantic qualification and whole-native/
platform/main/release acceptance remain open. See [query contract](../native/docs/mixed-meaning-queries.md)
and [indexed evidence](validation/integration-2026-10-19/mixed-meaning-consumers/index.json).

'''
p=root/'docs/STATUS.md';text=p.read_text(encoding='utf-8');marker='## Current native Rust implementation';assert marker in text;p.write_text(text.replace(marker,addition+marker,1),encoding='utf-8')
p=root/'docs/ARCHITECTURE.md';text=p.read_text(encoding='utf-8');marker='## Native Rust ownership\n';assert marker in text
p.write_text(text.replace(marker,marker+'''\n`mixed_view::provider::CheckedGraph` owns joint checked source replay and exact
verified provider graph data; no JSON constructor exists. Its bounded literal
lookup, directed multiedge degrees and frontier trace queries use owned data,
including after Store loss. Original indexed claims, ordered repeated roles and
nested source/receipt/accounting metadata remain; hyperedges do not imply flow.
Existing provider load JSON and artifact identities are unchanged. The separate
CLI query requires PDF authority only during load and starts no model. See
[mixed graph queries](../native/docs/mixed-meaning-queries.md).\n''',1),encoding='utf-8')
out.mkdir(exist_ok=False)
for file in sorted(base.rglob('*')):
    if file.is_file():
        target=out/file.relative_to(base);target.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(file,target)
(out/'runners').mkdir()
for file in sorted(temp.glob('ci-consumers-*')):
    if file.suffix in ['.py','.ps1']:shutil.copyfile(file,out/'runners'/file.name)
entries=[{'path':str(p.relative_to(out)).replace('\\','/'),'bytes':p.stat().st_size,'sha256':sha(p.read_bytes())} for p in sorted(out.rglob('*')) if p.is_file()]
index={'schemaVersion':1,'kind':'mixed_meaning_consumers_validation_v1','sourceCommit':source['sourceCommit'],'runtimeCommit':source['runtimeCommit'],'parentCommit':source['parentCommit'],'finalNative':final,'integrated':integrated,'newDefaultCases':8,'initialResult':read('feature-r1/result.json'),'sourceVariants':proof,'nativeBinarySha256':source['nativeBinarySha256'],'nativeBinaryBytes':source['nativeBinaryBytes'],'wholeNativeRerun':False,'fullR4Complete':False,'mainOrReleaseVerified':False,'artifacts':entries}
write(out/'index.json',index)
p=root/'docs/validation/integration-2026-10-19/README.md';text=p.read_text(encoding='utf-8')
p.write_text(text+f'''\nChecked mixed interpretation graph consumers: [indexed evidence](mixed-meaning-consumers/index.json),
source `{source['sourceCommit']}` / merge `{source['runtimeCommit']}`. Final19groups
{t['passed']}/{t['failed']}/{t['ignored']} and format/build/Clippy pass; integrated Python17/typecheck/
TypeScript260+3skipped pass. Eight new cases preserve original source/receipt
and directed/parallel/role semantics, including frozen queries after Store loss.
Initial17/1 wrong expected refusal code and disk-full linker failure are retained.
Scoped nonadditive evidence;
cross-workspace/fullR4/whole-native/platform/main/release remain open.\n''',encoding='utf-8')
paths=['docs/STATUS.md','docs/ARCHITECTURE.md','docs/parity/native-reconciliation-2026-10-15.json','docs/validation/integration-2026-10-19/README.md']
paths += [str(p.relative_to(root)).replace('\\','/') for p in sorted(out.rglob('*')) if p.is_file()]
for entry in entries:assert sha((out/entry['path']).read_bytes())==entry['sha256']
pathfile=temp/'ci-consumers-pathspec.bin';pathfile.write_bytes(b''.join(p.encode()+b'\0' for p in paths))
git('add','-f','--pathspec-from-file='+str(pathfile),'--pathspec-file-nul')
assert set(git('diff','--cached','--name-only').decode().splitlines())==set(paths)
git('-c','core.whitespace=cr-at-eol','diff','--cached','--check')
git('commit','-m','docs: record checked mixed graph query validation and remaining work')
assert not git('status','--porcelain')
print(json.dumps({'head':git('rev-parse','HEAD').decode().strip(),'artifacts':len(entries),'indexSha256':sha((out/'index.json').read_bytes()),'sourceVariants':len(proof)}),flush=True)
