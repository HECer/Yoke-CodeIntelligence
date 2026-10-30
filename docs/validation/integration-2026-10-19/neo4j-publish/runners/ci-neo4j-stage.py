from pathlib import Path
import subprocess,json,hashlib,shutil,tempfile,re
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');out=root/'docs/validation/integration-2026-10-19/neo4j-publish';sha=lambda b:hashlib.sha256(b).hexdigest();index_path=out/'index.json';index=json.loads(index_path.read_bytes());temp=Path(__file__).parent
for name in ['ci-neo4j-stage.py','ci-neo4j-pr.py']:shutil.copyfile(temp/name,out/'runners'/name)
index['artifacts']=[]
for file in sorted(out.rglob('*')):
 if file.is_file() and file!=index_path:raw=file.read_bytes();index['artifacts'].append({'path':file.relative_to(out).as_posix(),'bytes':len(raw),'sha256':sha(raw)})
index_path.write_bytes((json.dumps(index,indent=2,ensure_ascii=False)+'\n').encode());paths=[]
for item in index['artifacts']:
 path=out/item['path'];assert path.resolve().is_relative_to(out.resolve()) and not path.is_symlink();raw=path.read_bytes();assert len(raw)==item['bytes'] and sha(raw)==item['sha256'],item['path'];assert not re.search(rb'-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----\r?\n[A-Za-z0-9+/=\r\n]{80,}-----END',raw),item['path'];paths.append(path.relative_to(root).as_posix())
paths.append(index_path.relative_to(root).as_posix());paths+=['docs/STATUS.md','docs/ARCHITECTURE.md','docs/parity/native-reconciliation-2026-10-15.json','docs/validation/integration-2026-10-19/README.md']
old=json.loads((out/'frozen-tracker.json').read_bytes());current=json.loads((root/'docs/parity/native-reconciliation-2026-10-15.json').read_bytes())
for key in ['inventoryLedger','historicalInventory','ledgerSummary']:assert old[key]==current[key]
prior=index['priorEvidenceReadme'];readme=(root/'docs/validation/integration-2026-10-19/README.md').read_bytes();assert sha(readme[:prior['bytes']])==prior['sha256']
with tempfile.NamedTemporaryFile(prefix='ci-neo4j-pathspec-',suffix='.bin',delete=False) as file:file.write(b'\0'.join(p.encode() for p in paths)+b'\0');pathspec=Path(file.name)
try:subprocess.run(['git','-c','core.longpaths=true','add','-f','--pathspec-from-file='+str(pathspec),'--pathspec-file-nul'],cwd=root,check=True)
finally:pathspec.unlink()
staged=set(filter(None,subprocess.check_output(['git','diff','--cached','--name-only','-z'],cwd=root).decode().split('\0')));assert staged==set(paths),(staged-set(paths),set(paths)-staged)
for item in index['artifacts']:
 path=(out/item['path']).relative_to(root).as_posix();raw=subprocess.check_output(['git','show',':'+path],cwd=root);assert sha(raw)==item['sha256'],path
assert subprocess.check_output(['git','show',':'+index_path.relative_to(root).as_posix()],cwd=root)==index_path.read_bytes()
subprocess.run(['git','-c','core.whitespace=cr-at-eol','diff','--cached','--check'],cwd=root,check=True)
subprocess.run(['git','commit','-m','docs: record reviewed Neo4j publication validation'],cwd=root,check=True)
head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root).decode().strip();print(json.dumps({'head':head,'artifacts':len(index['artifacts']),'stagedPaths':len(paths),'indexSha256':sha(index_path.read_bytes()),'rawArtifactBytesVerified':True}),flush=True)
