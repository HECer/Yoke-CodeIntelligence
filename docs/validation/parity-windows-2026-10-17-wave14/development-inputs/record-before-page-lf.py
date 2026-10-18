"""Pin actual behavioral RED and independently check the PDF fixture oracle."""
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile

BASE = Path(__file__).resolve().parent
ROOT = Path('E:/Yoke-Worktrees/pdf-mixed-view-20261017')
sha = lambda raw: hashlib.sha256(raw).hexdigest()
raw = (BASE/'baseline-source-freeze.json').read_bytes()
assert sha(raw) == 'd6510d06d4cc8d3f53a0f375f4801e2529e24b16c7a5a960e47bafcfb4c4f5ec'
names = subprocess.check_output(['git','ls-files','-c','-o','--exclude-standard','-z'],cwd=ROOT).decode().split('\0')
current = {name:sha((ROOT/name).read_bytes()) for name in sorted(set(names)) if name and (ROOT/name).is_file()}
assert current == json.loads(raw)
spec = importlib.util.spec_from_file_location('pdf_contract',BASE/'contract_red.py')
contract = importlib.util.module_from_spec(spec)
spec.loader.exec_module(contract)
binary = sha(contract.EXE.read_bytes())
assert binary == 'a0005cf51bb78e69a5b4aa75967d683b3eb653021d3abf773c6f747ac47b6b7d'
with tempfile.TemporaryDirectory(prefix='yoke-pdf-oracle-',dir='E:/Yoke-Test-Scratch') as folder:
    selected = Path(folder)/'original.pdf'
    selected.write_bytes(contract.pdf())
    result = subprocess.run([str(contract.EXE),'media-pdf-worker','--input',str(selected)],
        capture_output=True,timeout=30,check=True)
    parsed = json.loads(result.stdout)
    expected = 'Native storage Gr\u00fc\u00dfe'
    assert parsed['segments'][0]['text'] == expected, repr(parsed['segments'][0]['text'])
    assert sha(contract.EXE.read_bytes()) == binary
    (BASE/'original-pdf-worker-oracle.json').write_bytes(result.stdout)
log = (BASE/'cli-red-r2.log').read_bytes()
assert b'Ran 2 tests' in log and b'FAILED (failures=2)' in log
assert b"unexpected argument '--native-pdf-text'" in log and b'mixed_view_input_invalid' in log
receipt = dict(kind='behavioral_red_before_pdf_implementation',passed=0,failed=2,
    copiedSourceManifestSha256=sha(raw),copiedSourceUnchanged=True,
    baselineExecutable=str(contract.EXE),baselineExecutableSha256=binary,
    executableBuildSourceNotReasserted=True,contractSha256=sha((BASE/'contract_red.py').read_bytes()),
    log='cli-red-r2.log',logSha256=sha(log),
    originalPdfWorkerOracleSha256=sha((BASE/'original-pdf-worker-oracle.json').read_bytes()),
    fixturePdfSha256=sha(contract.pdf()),decodedPageHash=sha(expected.encode()),
    initialAttempt='cli-red.log retains a fixture CLI typo; the positive case there is not used as behavioral RED.',
    interpretation='Actual unchanged prior executable: valid frozen source/PDF setup reaches missing native-pdf-text flag; schema2 lacks permission marker. No PDF implementation, build, full acceptance or parity claim.')
with (BASE/'cli-red-r2-receipt.json').open('x',encoding='utf-8') as stream:
    stream.write(json.dumps(receipt,indent=2)+'\n')
print(json.dumps(receipt))
