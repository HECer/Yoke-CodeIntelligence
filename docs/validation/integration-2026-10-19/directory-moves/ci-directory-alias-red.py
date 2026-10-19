from pathlib import Path
import subprocess,time,json
root=Path('C:/Users/Lomi/Documents/NN-Developed/Yoke-CodeIntelligence');out=root/'validation/integration-20261018/directory-moves-development';cargo=str(Path.home()/'.cargo/bin/cargo.exe')
subprocess.run([cargo,'fmt','--all'],check=True)
p=out/'case-alias-red-source/native/tests/facade_directory_move.rs';p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(Path('native/tests/facade_directory_move.rs').read_bytes())
cmd=[cargo,'test','--locked','-j1','--target-dir',str(root/'target'),'--test','facade_directory_move','directory_destination_parent_requires_its_exact_existing_host_spelling','--','--exact','--test-threads=1'];start=time.monotonic()
with (out/'case-alias-red.log').open('wb') as log:r=subprocess.run(cmd,stdout=log,stderr=subprocess.STDOUT)
print(json.dumps({'gate':'directory-parent-case-alias-red','exitCode':r.returncode,'elapsedSeconds':round(time.monotonic()-start,3)}));raise SystemExit(r.returncode)