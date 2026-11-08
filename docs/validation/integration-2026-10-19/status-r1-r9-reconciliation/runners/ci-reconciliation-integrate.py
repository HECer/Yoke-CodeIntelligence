from pathlib import Path
import subprocess,json,hashlib,shutil,re
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');w=root.parent/'Yoke-CI-r1-attach';base=root/'validation/integration-20261018/status-r1-r9-reconciliation';out=root/'docs/validation/integration-2026-10-19/status-r1-r9-reconciliation';temp=Path(__file__).parent
sha=lambda b:hashlib.sha256(b).hexdigest()
def git(cwd,*a):return subprocess.check_output(['git','-c','core.longpaths=true',*a],cwd=cwd,stderr=subprocess.PIPE)
parent='6181fcb01522de2e1d82b7aa6c22d72a1ab65f1b';paths=sorted(['docs/REMAINING_WORK.md','docs/STATUS.md','docs/parity/native-reconciliation-2026-10-15.json','native/docs/github-app-workers.md'])
result=json.loads((base/'reconciliation.json').read_bytes());assert result['runtimeInputsMatched']==753 and result['noBehaviorChange']
for cwd in [root,w]:assert git(cwd,'rev-parse','HEAD').decode().strip()==parent
assert not git(root,'status','--porcelain') and not git(w,'diff','--cached','--name-only')
changed=set(git(w,'diff','--name-only').decode().splitlines())|set(git(w,'ls-files','-o','--exclude-standard').decode().splitlines());assert changed==set(paths)
for p,h in result['documentHashesUnchangedDuringValidation'].items():assert sha((w/p).read_bytes())==h
for p in ['docs/REMAINING_WORK.md','native/docs/github-app-workers.md']:
    for link in re.findall(r'\]\(([^)]+)\)',(w/p).read_text(encoding='utf-8')):
        if '://' not in link:assert ((w/p).parent/link.split('#')[0]).exists(),(p,link)
prior=json.loads(subprocess.check_output(['gh','pr','view','7','--repo','HECer/Yoke-CodeIntelligence','--json','body'],cwd=root))['body'].encode();assert len(prior)==6948 and sha(prior)=='a8afbd2a2ba34bc5a469f21c68382bf046b2a583262676348ff4acb05afc14e9'
(base/'prior-pr-body.md').write_bytes(prior)
git(w,'add','--',*paths);assert set(git(w,'diff','--cached','--name-only').decode().splitlines())==set(paths);git(w,'-c','core.whitespace=cr-at-eol','diff','--cached','--check');git(w,'commit','-m','docs: reconcile implemented R1-R9 behavior and remaining acceptance')
source=git(w,'rev-parse','HEAD').decode().strip();git(root,'merge','--no-ff','-m','Merge current R1-R9 status reconciliation',source);runtime=git(root,'rev-parse','HEAD').decode().strip()
materialization=[]
for n in paths:
    a=(w/n).read_bytes();b=(root/n).read_bytes();assert a.replace(b'\r\n',b'\n')==b.replace(b'\r\n',b'\n')
    materialization.append({'path':n,'scopedSha256':sha(a),'integratedSha256':sha(b),'materialization':'raw_equal' if a==b else 'crlf_only_checkout_materialization'})
    for prefix,data in [('scoped',a),('integrated',b)]:p=base/prefix/n;p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(data)
(base/'source-materialization.json').write_text(json.dumps(materialization,indent=2)+'\n',encoding='utf-8')
integration={'sourceCommit':source,'runtimeCommit':runtime,'parentCommit':parent,'paths':paths,'runtimeBehaviorChanged':False,'nativeOrTypescriptTestsRerun':False,'pythonContracts':17,'runtimeInputsMatched':753,'mainOrReleaseVerified':False}
(base/'source-integration.json').write_text(json.dumps(integration,indent=2)+'\n',encoding='utf-8')
out.mkdir(exist_ok=False)
for p in sorted(base.rglob('*')):
    if p.is_file():target=out/p.relative_to(base);target.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(p,target)
(out/'runners').mkdir()
for p in sorted(temp.glob('ci-reconciliation-*.py')):shutil.copyfile(p,out/'runners'/p.name)
entries=[{'path':str(p.relative_to(out)).replace('\\','/'),'bytes':p.stat().st_size,'sha256':sha(p.read_bytes())} for p in sorted(out.rglob('*')) if p.is_file()]
index={**integration,'kind':'status_reconciliation_validation_v1','validation':result,'artifacts':entries}
(out/'index.json').write_text(json.dumps(index,indent=2)+'\n',encoding='utf-8')
p=root/'docs/validation/integration-2026-10-19/README.md';s=p.read_text(encoding='utf-8');p.write_text(s+f'''\nR1-R9 status reconciliation: [indexed proof](status-r1-r9-reconciliation/index.json),
source `{source}` / merge `{runtime}`. Only documentation/tracker text changes.
All753 runtime inputs match the prior behavioral baseline exactly or by verified
CRLF checkout materialization (579 raw,174 CRLF); inventory/historical ledgers
and all summary counts remain unchanged. Fresh Python17 passes; native and
TypeScript suites are not rerun for this documentation-only delta. Their prior
630/0/7 and260+3skipped scopes retain exact source/binary boundaries. R2/R3/R8
wording now distinguishes completed contracts from remaining implementation and
external acceptance, without marking full parity/main/release complete.\n''',encoding='utf-8')
staged=['docs/validation/integration-2026-10-19/README.md']+[str(p.relative_to(root)).replace('\\','/') for p in sorted(out.rglob('*')) if p.is_file()]
for e in entries:assert sha((out/e['path']).read_bytes())==e['sha256']
pathfile=temp/'ci-reconciliation-pathspec.bin';pathfile.write_bytes(b''.join(p.encode()+b'\0' for p in staged));git(root,'add','-f','--pathspec-from-file='+str(pathfile),'--pathspec-file-nul')
assert set(git(root,'diff','--cached','--name-only').decode().splitlines())==set(staged);git(root,'-c','core.whitespace=cr-at-eol','diff','--cached','--check');git(root,'commit','-m','docs: preserve status reconciliation source and ledger proof')
assert not git(root,'status','--porcelain') and not git(w,'status','--porcelain')
print(json.dumps({'head':git(root,'rev-parse','HEAD').decode().strip(),'sourceCommit':source,'runtimeCommit':runtime,'artifacts':len(entries),'indexSha256':sha((out/'index.json').read_bytes())}),flush=True)
