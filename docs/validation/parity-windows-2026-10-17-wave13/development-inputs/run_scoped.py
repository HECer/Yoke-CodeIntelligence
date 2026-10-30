"""Run scoped development checks; these never substitute for integrated final gates."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

root = Path('E:/Yoke-Worktrees/outcome-learning-20261017')
base = Path(__file__).resolve().parent
environment = os.environ.copy()
public = dict(CARGO_TARGET_DIR=str(base), CARGO_HOME='E:/Yoke-Build-Cache/cargo-home-yoke-20261017',
    CARGO_NET_OFFLINE='true', TEMP=str(base/'temp'), TMP=str(base/'temp'), RTK_SUPPRESS_HOOK_WARNING='1')
environment.update(public)
assert (base/'temp').is_dir()
def manifest():
    names = subprocess.check_output(['git','ls-files','-c','-o','--exclude-standard','-z'],cwd=root).decode().split('\0')
    return {name:hashlib.sha256((root/name).read_bytes()).hexdigest()
        for name in sorted(set(names)) if name and (root/name).is_file()}
before = manifest()
label = sys.argv[1]
commands = {
    'feedback': ['cargo','test','--workspace','--locked','-j','1','--test','feedback_cli','--','--test-threads=2'],
    'regressions': ['cargo','test','--workspace','--locked','-j','1',
        *[argument for name in ['knowledge','knowledge_cli','decision_records','notes','retention','maintenance',
            'mixed_view_cli','view_comparison_cli','graph_view'] for argument in ['--test',name]],'--','--test-threads=2'],
    'comparison-units': ['cargo','test','--workspace','--locked','-j','1','--lib','graph_view::comparison_','--','--test-threads=2'],
    'typecheck': ['npm.cmd','run','typecheck'],
    'typescript': ['npm.cmd','test'],
    'format': ['cargo','fmt','--all','--','--check'],
    'clippy': ['cargo','clippy','--workspace','--all-targets','--locked','-j','1','--','-D','warnings'],
}
command = commands[label]
directory = base/'development'
path = directory/(label+'-'+sys.argv[2]+'.log')
assert not path.exists()
with path.open('xb') as stream:
    result = subprocess.run(['rtk','proxy',*command],cwd=root,env=environment,stdout=stream,stderr=subprocess.STDOUT)
after = manifest()
receipt = dict(label=label,command=command,exitCode=result.returncode,environment=public,
    sourceBefore=hashlib.sha256((json.dumps(before,ensure_ascii=False,indent=2)+'\n').encode()).hexdigest(),
    sourceAfter=hashlib.sha256((json.dumps(after,ensure_ascii=False,indent=2)+'\n').encode()).hexdigest(),
    sourceChanges=sorted(n for n in set(before)|set(after) if before.get(n)!=after.get(n)),
    log=path.name,logSha256=hashlib.sha256(path.read_bytes()).hexdigest(),
    interpretation='Separate scoped development; no full parity or integrated acceptance claim.')
path.with_suffix('.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8')
print(json.dumps(receipt),flush=True)
if result.returncode:
    print(path.read_text(encoding='utf-8')[-10000:],flush=True)
sys.exit(result.returncode)
