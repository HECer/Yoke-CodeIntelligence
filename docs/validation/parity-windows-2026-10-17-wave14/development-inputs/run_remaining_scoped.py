"""Finish the isolated PDF checks serially; never replace whole acceptance."""
import os
from pathlib import Path
import subprocess
import sys

BASE = Path(__file__).resolve().parent
ROOT = Path('E:/Yoke-Worktrees/pdf-mixed-view-20261017')
environment = os.environ.copy()
environment['RTK_SUPPRESS_HOOK_WARNING'] = '1'
for label, attempt in [('pdf', 'r2'), ('format', 'r1'), ('clippy', 'r1'),
                       ('typecheck', 'r1'), ('typescript', 'r1')]:
    print('Running isolated '+label+' '+attempt, flush=True)
    status = subprocess.run([sys.executable, '-B', '-X', 'utf8',
        str(BASE/'run_scoped.py'), label, attempt], cwd=ROOT, env=environment).returncode
    if status:
        sys.exit(status)
