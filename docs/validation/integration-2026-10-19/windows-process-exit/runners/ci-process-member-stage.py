from pathlib import Path
import json,hashlib,subprocess
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');folder=root/'docs/validation/integration-2026-10-19/windows-process-exit';index=json.loads((folder/'index.json').read_bytes());paths=[]
for artifact in index['artifacts']:
    path=(folder/artifact['path']).resolve();assert path.is_relative_to(folder.resolve()) and path.is_file();raw=path.read_bytes();assert len(raw)==artifact['bytes'] and hashlib.sha256(raw).hexdigest()==artifact['sha256'],artifact['path'];paths.append(path.relative_to(root).as_posix())
staged=set(subprocess.check_output(['git','diff','--cached','--name-only'],cwd=root).decode().splitlines())
assert staged.issubset({'docs/STATUS.md','docs/ARCHITECTURE.md','docs/parity/native-reconciliation-2026-10-15.json','docs/validation/integration-2026-10-19/README.md','native/README.md',*paths,(folder/'index.json').relative_to(root).as_posix()}),staged
subprocess.run(['git','-c','core.whitespace=cr-at-eol','diff','--check'],cwd=root,check=True)
doc_paths=['docs/STATUS.md','docs/ARCHITECTURE.md','docs/parity/native-reconciliation-2026-10-15.json','docs/validation/integration-2026-10-19/README.md','native/README.md']
subprocess.run(['git','add','--',*doc_paths],cwd=root,check=True)
# Force-add only the checked immutable manifest members and the manifest itself.
spec=Path.home()/'AppData/Local/Temp/ci-process-member-checked-paths.nul';spec.write_bytes(b'\0'.join(p.encode() for p in [*paths,(folder/'index.json').relative_to(root).as_posix()])+b'\0')
subprocess.run(['git','add','-f','--pathspec-from-file='+str(spec),'--pathspec-file-nul'],cwd=root,check=True,stdout=subprocess.DEVNULL)
for artifact in index['artifacts']:
    relative=(folder/artifact['path']).relative_to(root).as_posix();raw=subprocess.check_output(['git','show',':'+relative],cwd=root);assert hashlib.sha256(raw).hexdigest()==artifact['sha256'],relative
assert subprocess.check_output(['git','show',':'+(folder/'index.json').relative_to(root).as_posix()],cwd=root)==(folder/'index.json').read_bytes()
subprocess.run(['git','-c','core.whitespace=cr-at-eol','diff','--cached','--check'],cwd=root,check=True)
subprocess.run(['git','commit','-m','docs: preserve Windows held-member exit proof and validation'],cwd=root,check=True,stdout=subprocess.PIPE)
head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root).decode().strip();assert not subprocess.check_output(['git','status','--porcelain'],cwd=root).strip()
print(json.dumps({'head':head,'artifacts':len(paths),'stagedRawBytesVerified':True,'indexSha256':hashlib.sha256((folder/'index.json').read_bytes()).hexdigest()}))
