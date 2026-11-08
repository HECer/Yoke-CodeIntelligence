"""Run the unchanged actual behavioral RED contract on the newly built binary."""
import hashlib
import importlib.util
import json
from pathlib import Path
import unittest

BASE = Path(__file__).resolve().parent
script = BASE/'contract_red.py'
executable = BASE/'debug/yoke.exe'
sha = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
assert sha(script) == 'fe2091eebfee097f447dfc6b3b358201f4026a961002361e298b8f7f0573c968'
before = sha(executable)
receipt_path = BASE/'development/contract-green.json'
assert not receipt_path.exists()
spec = importlib.util.spec_from_file_location('unchanged_pdf_contract', script)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
module.EXE = executable
result = unittest.TextTestRunner(verbosity=2).run(
    unittest.defaultTestLoader.loadTestsFromModule(module))
assert sha(executable) == before
receipt = dict(executable=str(executable), executableSha256=before,
    contractSha256=sha(script), ran=result.testsRun, failures=len(result.failures),
    errors=len(result.errors), skipped=len(result.skipped),
    interpretation='Same two actual CLI RED cases; isolated scoped development only.')
with receipt_path.open('x', encoding='utf-8') as stream:
    stream.write(json.dumps(receipt, indent=2)+'\n')
print(json.dumps(receipt), flush=True)
raise SystemExit(0 if result.wasSuccessful() else 1)
