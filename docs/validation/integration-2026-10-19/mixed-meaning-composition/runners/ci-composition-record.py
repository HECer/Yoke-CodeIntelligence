from pathlib import Path
import subprocess,json,hashlib,shutil,re
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');work=root.parent/'Yoke-CI-r1-attach'
base=root/'validation/integration-20261018/mixed-meaning-composition';out=root/'docs/validation/integration-2026-10-19/mixed-meaning-composition';temp=Path(__file__).parent
sha=lambda b:hashlib.sha256(b).hexdigest();read=lambda n:json.loads((base/n).read_bytes())
def git(*a):return subprocess.check_output(['git','-c','core.longpaths=true',*a],cwd=root,stderr=subprocess.PIPE)
def write(p,v):p.write_text(json.dumps(v,indent=2,ensure_ascii=False)+'\n',encoding='utf-8')
source=read('source-integration.json');final=read('final/result.json');integrated=read('integrated/result.json')
assert all(r['allChecksPassed'] and r['sourceUnchanged'] for r in [final,integrated])
assert final['commands'][0]['groups']==20 and source['newCases']['defaultIntegration']==8
initial=read('feature-r1/result.json');assert initial['commands'][0]['groups']==0 and initial['commands'][0]['exitCode']==101
assert read('green-r1/result.json')['commands'][0]['totals']=={'passed':27,'failed':0,'ignored':0}
assert git('rev-parse','HEAD').decode().strip()==source['runtimeCommit'] and not git('status','--porcelain')
assert 'Ran 17 tests' in (base/'integrated/python-contracts.log').read_text(encoding='utf-8')
ts=(base/'integrated/tests.log').read_text(encoding='utf-8');assert re.search(r'\bpass\s+260\b',ts) and re.search(r'\bskipped\s+3\b',ts)
proof=[]
for phase in ['feature-r1','green-r1','final']:
    before=read(phase+'/source-before.json');assert before==read(phase+'/source-after.json')
    for name in source['paths']:
        if name not in before:continue
        saved=base/phase/'source'/name;candidates=[]
        if saved.exists():candidates.append(('original_phase_capture',saved.read_bytes()))
        candidates.append(('current_scoped_exact',(work/name).read_bytes()))
        try:
            raw=git('show',source['parentCommit']+':'+name)
            candidates += [('parent_git_exact',raw),('parent_crlf_materialization',raw.replace(b'\r\n',b'\n').replace(b'\n',b'\r\n'))]
        except subprocess.CalledProcessError:pass
        match=next(((origin,b) for origin,b in candidates if sha(b)==before[name]),None);assert match is not None,(phase,name)
        saved.parent.mkdir(parents=True,exist_ok=True);saved.write_bytes(match[1]);proof.append({'phase':phase,'path':name,'sha256':sha(match[1]),'origin':match[0]})
write(base/'source-completeness.json',proof)
prior=json.loads(subprocess.check_output(['gh','pr','view','7','--repo','HECer/Yoke-CodeIntelligence','--json','body'],cwd=root))['body'].encode()
assert len(prior)==6943 and sha(prior)=='e5d70ac85918c665df8173de98bcf28e7f1ca00fe2a00c9275052b9f03231e9d'
(base/'prior-pr-body.md').write_bytes(prior)
tracker=root/'docs/parity/native-reconciliation-2026-10-15.json';raw=tracker.read_bytes();old=json.loads(raw);original=json.loads(raw);(base/'frozen-tracker.json').write_bytes(raw)
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
r4['mixedCheckedCompositionAddition']='Opaque composition consumes2..4 independently source/response-checked owned parent graphs, including different workspaces/history, and an exact ordered closed review. Parent-qualified concept/observation identities preserve original provider relations, parallel/reverse/self observations, ordered hyperedge roles, four typed coordinate spaces and complete nested receipt/accounting provenance. Explicit host-authored links retain separate review positions, unverified provenance and separate degree counts; query traversal includes them only when requested. Labels/aliases merge no concepts, hyperedges imply no flow, and no source/canonical/runtime/semantic identity authority follows. CLI selects2..4 explicit jobs from one root Store and handles PDF/Office permissions perparent; crossworkspace library callers supply independently loaded capabilities. No provider, persistence or sourcewrites are added. Eight adversarial/CLI cases qualify bounded Windows behavior, including both Store losses.'
r4['missingBehavior']='Broader mixed graph analytics, semantic identity/reasoning and full external semantic-provider qualification remain open. Cross-workspace library composition now consumes independently checked parents, while CLI selection remains within one explicitly selected Store. Typed preparation, checked claims/provider jobs and single/composed graph queries have bounded Windows evidence; no complete layout/OCR, model-quality, exhaustive binary-document interpretation or full upstream certification is implied.'
r4['suggestedScope']='Reconcile remaining R1-R9 acceptance against actual current implementations and evidence, then address concrete missing mixed analytics or external semantic-provider qualification. Preserve independently checked source scopes, immutable original receipt/claim evidence, explicit host assertions, separate PDF/provider permissions and false semantic/canonical authority.'
for p in ['native/src/mixed_meaning_composition.rs','native/tests/mixed_meaning_composition.rs','native/docs/mixed-meaning-composition.md','docs/validation/integration-2026-10-19/mixed-meaning-composition/index.json']:
    if p not in r4['evidence']:r4['evidence'].append(p)
for k in ['inventoryLedger','historicalInventory','ledgerSummary']:assert old[k]==original[k]
write(tracker,old)
t=source['nativeTotals'];totals=f"{t['passed']} passed / {t['failed']} failed / {t['ignored']} ignored"
addition=f'''Explicit checked mixed graph composition now combines 2..4 owned source/response-
replayed parents with an ordered host-authored link review. Parent-qualified
concept/observation identities, original parallel/reverse/self relations, ordered
hyperedge slots and complete nested typed source/receipt/accounting remain.
Host links are separate unverified assertions with independent indices and
degree counts; traces follow them only when explicitly requested. Labels and
aliases create no merge or semantic identity, and roles imply no flow. Library
parents may come from different checked workspaces/history. The CLI selects
explicit jobs from one root Store, with per-parent PDF/Office worker permission.
No provider, persistence, source write or default MCP schema change is added.

Source `{source['sourceCommit']}`, runtime merge `{source['runtimeCommit']}`.
Eight new default cases include owned Node/PDF setup, cross-workspace qualified
keys, parallel host/provider rows, split degree counts, Unicode ambiguity,
direction/depth0/capacity, independent parent/endpoint validation, private closed
CLI admission, both Store losses and an actual PDF+Office same-Store CLI query
without installed provider files. Initial feature compilation stops at a test-
only Vec.repeat/Copy constraint before executing any test group; the fixture
uses explicit cloning afterward. Three focused groups pass **27/0/0**. Final20groups
**{totals}** and format/build/all-target Clippy pass with {final['inputs']}
unchanged inputs. Integrated format/Python17/typecheck/TypeScript260+3skipped pass
with {integrated['frozenInputs']} unchanged inputs. Counts remain scoped and nonadditive.
Broader analytics, semantic qualification, full R4/R1-R9 and whole-native/
platform/main/release acceptance remain open. See [composition contract](../native/docs/mixed-meaning-composition.md)
and [indexed evidence](validation/integration-2026-10-19/mixed-meaning-composition/index.json).

'''
p=root/'docs/STATUS.md';s=p.read_text(encoding='utf-8');marker='## Current native Rust implementation';assert marker in s;p.write_text(s.replace(marker,addition+marker,1),encoding='utf-8')
p=root/'docs/ARCHITECTURE.md';s=p.read_text(encoding='utf-8');marker='## Native Rust ownership\n';assert marker in s
p.write_text(s.replace(marker,marker+'''\n`mixed_view::provider::composition` consumes independently checked owned graphs
and a bounded exact ordered link review. Parent-qualified concepts/observations
retain complete original source/receipt/accounting metadata across workspaces or
history. Authored links remain unverified data, with separate indices/degrees
and explicit opt-in traversal. Roles imply no flow; labels imply no identity.
Frozen queries require no Store/provider. The CLI admits explicit jobs only from
its host-selected root, with PDF worker permission checked perparent. No JSON
constructor, persistence, source write or default MCP change is added. See
[mixed composition](../native/docs/mixed-meaning-composition.md).\n''',1),encoding='utf-8')
out.mkdir(exist_ok=False)
for p in sorted(base.rglob('*')):
    if p.is_file():target=out/p.relative_to(base);target.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(p,target)
(out/'runners').mkdir()
for p in sorted(temp.glob('ci-composition-*')):
    if p.suffix in ['.py','.ps1']:shutil.copyfile(p,out/'runners'/p.name)
entries=[{'path':str(p.relative_to(out)).replace('\\','/'),'bytes':p.stat().st_size,'sha256':sha(p.read_bytes())} for p in sorted(out.rglob('*')) if p.is_file()]
write(out/'index.json',{'schemaVersion':1,'kind':'mixed_meaning_composition_validation_v1','sourceCommit':source['sourceCommit'],'runtimeCommit':source['runtimeCommit'],'parentCommit':source['parentCommit'],'finalNative':final,'integrated':integrated,'newDefaultCases':8,'initialResult':initial,'initialExecutedGroups':0,'greenResult':read('green-r1/result.json'),'sourceVariants':proof,'nativeBinarySha256':source['nativeBinarySha256'],'nativeBinaryBytes':source['nativeBinaryBytes'],'wholeNativeRerun':False,'fullR4Complete':False,'mainOrReleaseVerified':False,'artifacts':entries})
p=root/'docs/validation/integration-2026-10-19/README.md';s=p.read_text(encoding='utf-8');p.write_text(s+f'''\nChecked mixed graph composition: [indexed evidence](mixed-meaning-composition/index.json),
source `{source['sourceCommit']}` / merge `{source['runtimeCommit']}`. Final20groups
{totals} plus format/build/Clippy; integrated Python17/typecheck/TS260+3skipped pass.
Eight new default cases cover independent workspaces, qualified original rows,
separate opt-in host links, frozen queries after both Store losses and actual
PDF+Office CLI composition without provider files. Initial test-only Copy/Clone
compilation failure executes0groups; green3groups27/0/0. Scoped nonadditive
evidence, without full R4/whole-native/platform/main/release acceptance.\n''',encoding='utf-8')
paths=['docs/STATUS.md','docs/ARCHITECTURE.md','docs/parity/native-reconciliation-2026-10-15.json','docs/validation/integration-2026-10-19/README.md']+[str(p.relative_to(root)).replace('\\','/') for p in sorted(out.rglob('*')) if p.is_file()]
for e in entries:assert sha((out/e['path']).read_bytes())==e['sha256']
pathfile=temp/'ci-composition-pathspec.bin';pathfile.write_bytes(b''.join(p.encode()+b'\0' for p in paths));git('add','-f','--pathspec-from-file='+str(pathfile),'--pathspec-file-nul')
assert set(git('diff','--cached','--name-only').decode().splitlines())==set(paths);git('-c','core.whitespace=cr-at-eol','diff','--cached','--check');git('commit','-m','docs: preserve mixed graph composition validation and boundaries')
assert not git('status','--porcelain');print(json.dumps({'head':git('rev-parse','HEAD').decode().strip(),'artifacts':len(entries),'indexSha256':sha((out/'index.json').read_bytes()),'sourceVariants':len(proof)}),flush=True)
