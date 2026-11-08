"""Low-impact helper smoke only; all created bytes remain under ignored staging."""
import ast
from pathlib import Path
import runpy
import sys
import tempfile

sys.dont_write_bytecode = True
path = Path(__file__).absolute().with_name("preserve_combined.py")
ast.parse(path.read_bytes())
module = runpy.run_path(str(path))
staging = path.parent
pin = {"path": "target/x.json", "sha256": "a" * 64}
assert module["context_pins"]({"x": [pin], "sourcePins": {"native/src/x.rs": "b" * 64}}) == [pin]
for value in ("../x", "C:/x", "a//b", "NUL", "a."):
    try:
        module["relative_path"](value)
    except RuntimeError:
        pass
    else:
        raise AssertionError("Unsafe path accepted")
names = [module["DESTINATION"] + "/smoke.log", module["DESTINATION"] + "/smoke.json"]
assert module["ignored_additions"](names) == [names[0]]
directory = Path(tempfile.mkdtemp(prefix="smoke-", dir=staging))
output = directory / "exclusive.json"
module["exclusive_write"](output, b"original", staging)
try:
    module["exclusive_write"](output, b"replacement", staging)
except FileExistsError:
    pass
else:
    raise AssertionError("Exclusive create overwrote existing file")
assert output.read_bytes() == b"original"
journal = directory / "journal.jsonl"
with journal.open("xb") as stream:
    module["append_journal"](stream, {"inFlight": "only-ignored-smoke"})
assert journal.read_bytes().endswith(b"\n")
assert set(module["tree_files"](directory, staging, allowed=None)) == {output, journal}
assert len(module["prior_indexes"]()) == 5
print("AST and bounded smoke PASS: pinned generic parser, explicit-pin-only traversal, path refusals, "
      "ignore partition, exclusive create, fsynced journal, complete output inventory, five index-byte hashes.")
print("No runner import, gates, full planning, apply or normal writes. Smoke retained: " + directory.name)
