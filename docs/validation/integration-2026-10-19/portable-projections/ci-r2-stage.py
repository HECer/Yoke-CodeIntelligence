from pathlib import Path
import json,hashlib,subprocess
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');directory=Path('docs/validation/integration-2026-10-19/portable-projections');out=root/directory;index=json.loads((out/'index.json').read_bytes())
paths=[]
for artifact in index['artifacts']:
    p=Path(artifact['path']);assert not p.is_absolute() and '..' not in p.parts
    raw=(out/p).read_bytes();assert len(raw)==artifact['bytes'] and hashlib.sha256(raw).hexdigest()==artifact['sha256'];paths.append((directory/p).as_posix())
paths.append((directory/'index.json').as_posix())
for start in range(0,len(paths),50):subprocess.run(['git','add','-f','--']+paths[start:start+50],cwd=root,check=True)
subprocess.run(['git','add','--','docs/STATUS.md','docs/parity/native-reconciliation-2026-10-15.json','docs/validation/integration-2026-10-19/README.md'],cwd=root,check=True)
for path in paths:
    staged=subprocess.check_output(['git','show',':'+path],cwd=root);assert staged==(root/path).read_bytes(),path
subprocess.run(['git','-c','core.whitespace=cr-at-eol','diff','--cached','--check'],cwd=root,check=True)
print(json.dumps({'verifiedEvidenceFiles':len(paths),'stagedPaths':len(subprocess.check_output(['git','diff','--cached','--name-only','-z'],cwd=root).split(b'\0'))-1,'indexSha256':hashlib.sha256((out/'index.json').read_bytes()).hexdigest()}))
