from pathlib import Path
import subprocess,json,hashlib,shutil,tempfile,re
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence')
out=root/'docs/validation/integration-2026-10-19/python-empty-suite'
temp=Path(__file__).parent
sha=lambda b:hashlib.sha256(b).hexdigest()
source=json.loads((out/'source-integration.json').read_bytes())
assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=root).decode().strip()==source['runtimeCommit']
assert not subprocess.check_output(['git','diff','--cached','--name-only'],cwd=root)
docs=['README.md','docs/STATUS.md','docs/ARCHITECTURE.md',
    'docs/parity/native-reconciliation-2026-10-15.json','docs/validation/integration-2026-10-19/README.md']
assert set(subprocess.check_output(['git','diff','--name-only'],cwd=root).decode().splitlines())==set(docs)
for name in ['ci-python-body-stage.py','ci-python-body-pr.py']:shutil.copyfile(temp/name,out/'runners'/name)
index_path=out/'index.json';index=json.loads(index_path.read_bytes());index['notes']=[n.replace('or the original deadline is already expired before setup','or the original deadline has expired when no timeout is present') for n in index['notes']];index['artifacts']=[]
for file in sorted(out.rglob('*')):
    if file.is_file() and file!=index_path:
        raw=file.read_bytes();index['artifacts'].append({'path':file.relative_to(out).as_posix(),'bytes':len(raw),'sha256':sha(raw)})
index_path.write_text(json.dumps(index,indent=2,ensure_ascii=False)+'\n',encoding='utf-8')
paths=[]
for item in index['artifacts']:
    path=out/item['path'];assert path.resolve().is_relative_to(out.resolve()) and not path.is_symlink()
    raw=path.read_bytes();assert len(raw)==item['bytes'] and sha(raw)==item['sha256'],item['path']
    assert not re.search(rb'-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----\r?\n[A-Za-z0-9+/=\r\n]{80,}-----END',raw),item['path']
    paths.append(path.relative_to(root).as_posix())
paths.append(index_path.relative_to(root).as_posix());paths+=docs
old=json.loads((out/'frozen-tracker.json').read_bytes())
current=json.loads((root/'docs/parity/native-reconciliation-2026-10-15.json').read_bytes())
for key in ['inventoryLedger','historicalInventory','ledgerSummary']:assert old[key]==current[key]
prior=index['priorEvidenceReadme'];readme=(root/'docs/validation/integration-2026-10-19/README.md').read_bytes()
assert sha(readme[:prior['bytes']])==prior['sha256']
with tempfile.NamedTemporaryFile(prefix='ci-python-body-pathspec-',suffix='.bin',delete=False) as file:
    file.write(b'\0'.join(p.encode() for p in paths)+b'\0');pathspec=Path(file.name)
try:
    subprocess.run(['git','-c','core.longpaths=true','add','-f','--pathspec-from-file='+str(pathspec),'--pathspec-file-nul'],cwd=root,check=True,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
finally:pathspec.unlink()
staged=set(filter(None,subprocess.check_output(['git','diff','--cached','--name-only','-z'],cwd=root).decode().split('\0')))
assert staged==set(paths),(staged-set(paths),set(paths)-staged)
for item in index['artifacts']:
    path=(out/item['path']).relative_to(root).as_posix()
    raw=subprocess.check_output(['git','-c','core.longpaths=true','show',':'+path],cwd=root)
    assert sha(raw)==item['sha256'],path
assert subprocess.check_output(['git','show',':'+index_path.relative_to(root).as_posix()],cwd=root)==index_path.read_bytes()
subprocess.run(['git','-c','core.whitespace=cr-at-eol','diff','--cached','--check'],cwd=root,check=True)
subprocess.run(['git','-c','core.longpaths=true','commit','-m','docs: record explicit Python empty-suite deletion validation'],cwd=root,check=True,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root).decode().strip()
assert not subprocess.check_output(['git','status','--porcelain'],cwd=root)
print(json.dumps({'head':head,'artifacts':len(index['artifacts']),'stagedPaths':len(paths),
    'indexSha256':sha(index_path.read_bytes()),'rawArtifactBytesVerified':True}),flush=True)
