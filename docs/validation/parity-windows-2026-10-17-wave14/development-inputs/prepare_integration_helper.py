"""Prepare a future nine-path operation; never run it before accepted Wave13."""
import hashlib
from pathlib import Path
import re

BASE = Path(__file__).resolve().parent
source = BASE.parent/'outcome-learning-20261017/integrate.py'
raw = source.read_bytes()
sha = lambda value: hashlib.sha256(value).hexdigest()
assert sha(raw) == '7e65528091d28639b8b27d18b6b473c8cbdf92c2a095467dce34be879936e863'
assert sha((BASE/'baseline-source-freeze.json').read_bytes()) == 'd6510d06d4cc8d3f53a0f375f4801e2529e24b16c7a5a960e47bafcfb4c4f5ec'
assert sha((BASE/'development/candidate-r3-source-freeze.json').read_bytes()) == 'a3e0210e314936ffe007661111e2780d02081dbd41c9ca21467a85129646f247'
text = raw.decode('utf-8').replace('\r\n', '\n')

def replace(old, new):
    global text
    assert text.count(old) == 1, old
    text = text.replace(old, new)

replace('Apply the sealed fourteen-path feedback delta after checked Wave12 acceptance.',
        'Apply the sealed nine-path PDF delta after checked Wave13 acceptance.')
replace("AUX = Path('E:/Yoke-Worktrees/outcome-learning-20261017')",
        "AUX = Path('E:/Yoke-Worktrees/pdf-mixed-view-20261017')")
replace("PARENT = ROOT / 'docs/validation/parity-windows-2026-10-17-wave12'",
        "PARENT = ROOT / 'docs/validation/parity-windows-2026-10-17-wave13'")
replace("BASELINE_SHA = '1ef41d7564050c370f7db36fd539e0db1fa081248fe282db4997c46572d09e26'",
        "BASELINE_SHA = 'd6510d06d4cc8d3f53a0f375f4801e2529e24b16c7a5a960e47bafcfb4c4f5ec'")
replace("CANDIDATE_SHA = 'd6510d06d4cc8d3f53a0f375f4801e2529e24b16c7a5a960e47bafcfb4c4f5ec'",
        "CANDIDATE_SHA = 'a3e0210e314936ffe007661111e2780d02081dbd41c9ca21467a85129646f247'")
names = ['native/src/media.rs', 'native/src/mixed_view.rs',
    'native/src/mixed_view_query.rs', 'native/src/mixed_view_cli.rs',
    'native/src/mixed_pdf_input.rs', 'native/tests/mixed_pdf_view.rs',
    'native/tests/support/mixed_pdf_fixture.rs', 'native/docs/mixed-pdf-views.md',
    'native/docs/mixed-views.md']
replacement = 'NAMES = [\n'+''.join('    '+repr(name)+',\n' for name in names)+']\n'
text, count = re.subn(r'NAMES = \[\n[\s\S]+?\n\]\n', replacement, text)
assert count == 1
replace('Auxiliary delta exceeds fourteen reviewed paths:', 'Auxiliary delta exceeds nine reviewed paths:')
replace("    subprocess.run([sys.executable, '-B', '-X', 'utf8',\n        str(BASE.parent/'mixed-document-view-20261017/verify_checkpoint.py'), expected_index],",
        "    verifier = BASE.parent/'outcome-learning-20261017/verify_checkpoint.py'\n"
        "    require(sha(verifier.read_bytes()) == '7cabba3e438926a5667aa5e41f6e22bf443d9fb0e9459b3fe2dd1d97c12fbe2e',\n"
        "        'Reviewed parent verifier changed')\n"
        "    subprocess.run([sys.executable, '-B', '-X', 'utf8',\n        str(verifier), expected_index],")
replace("    # The parent was accepted after preserving Wave11. Only its four status\n"
        "    # updates and newly preserved Wave11 evidence may extend the sealed baseline.\n"
        "    prior_prefix = 'docs/validation/parity-windows-2026-10-17-wave11/'\n"
        "    allowed = {name for name in set(frozen)|set(baseline)\n"
        "        if name in METADATA or (name.startswith(prior_prefix) and name not in baseline)}",
        "    # The sealed auxiliary baseline predates the two accepted parent archives.\n"
        "    # Product bytes must match; only the four status files and those newly\n"
        "    # admitted archive paths may differ. The parent verifier replays its\n"
        "    # complete integration maps and closed Wave12 proof before this check.\n"
        "    prior_prefixes = ('docs/validation/parity-windows-2026-10-17-wave11/',\n"
        "        'docs/validation/parity-windows-2026-10-17-wave12/')\n"
        "    allowed = {name for name in set(frozen)|set(baseline)\n"
        "        if name in METADATA or (name.startswith(prior_prefixes) and name not in baseline)}")
destination = BASE/'integrate.py'
with destination.open('xb') as stream:
    stream.write(text.encode('utf-8'))
print(dict(prepared=str(destination), sha256=sha(destination.read_bytes()), executed=False))
