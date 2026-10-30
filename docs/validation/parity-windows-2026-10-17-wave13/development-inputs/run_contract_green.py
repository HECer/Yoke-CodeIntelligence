"""Replay the unchanged behavioral RED cases against the newly built own binary."""
import hashlib
import importlib.util
import json
from pathlib import Path
import unittest

base = Path(__file__).resolve().parent
script = base/'contract_red.py'
executable = base/'debug/yoke.exe'
assert executable.is_file()
sha = lambda path:hashlib.sha256(path.read_bytes()).hexdigest()
before = sha(executable)
spec = importlib.util.spec_from_file_location('unchanged_feedback_contract',script)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
module.EXE = executable
result = unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromModule(module))
assert sha(executable)==before
receipt = dict(executable=str(executable),executableSha256=before,contractSha256=sha(script),
    ran=result.testsRun,failures=len(result.failures),errors=len(result.errors),skipped=len(result.skipped),
    interpretation='Same three actual CLI RED cases against the newly compiled R9 binary; scoped development only.')
(base/'development/contract-green.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8')
print(json.dumps(receipt),flush=True)
raise SystemExit(0 if result.wasSuccessful() else 1)
