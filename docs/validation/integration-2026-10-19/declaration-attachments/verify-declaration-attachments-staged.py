from pathlib import Path
import json,hashlib,subprocess
root=Path.cwd();out=root/'docs/validation/integration-2026-10-19/declaration-attachments';index=json.loads((out/'index.json').read_bytes());files=[]
for entry in index['artifacts']:
 p=out/entry['path'];assert p.resolve().is_relative_to(out.resolve());raw=p.read_bytes();assert len(raw)==entry['bytes'] and hashlib.sha256(raw).hexdigest()==entry['sha256'],str(p);files.append(p.relative_to(root).as_posix())
files.append((out/'index.json').relative_to(root).as_posix());normal=['docs/STATUS.md','docs/parity/native-reconciliation-2026-10-15.json','docs/validation/integration-2026-10-19/README.md'];subprocess.run(['git','add','--',*normal],check=True);subprocess.run(['git','add','-f','--',*files],check=True)
for name in files:assert subprocess.check_output(['git','show',':'+name])==(root/name).read_bytes(),name
actual=set(subprocess.check_output(['git','diff','--cached','--name-only','-z']).decode().strip('\0').split('\0'));assert actual==set(files+normal)
subprocess.run(['git','-c','core.whitespace=cr-at-eol','diff','--cached','--check'],check=True);print(json.dumps({'stagedArtifactCount':len(files),'normalMetadataCount':len(normal),'indexSha256':hashlib.sha256((out/'index.json').read_bytes()).hexdigest()}))
