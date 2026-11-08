"""Require the five independently reviewed changes before a fresh freeze."""
import hashlib
import json
from pathlib import Path
import runpy

ROOT = Path(__file__).resolve().parents[4]
EVIDENCE = ROOT / "target/parity-validation-20261015"
RUNNER = EVIDENCE / "acceptance_wave4.py"
assert hashlib.sha256(RUNNER.read_bytes()).hexdigest() == (
    "b51bbffbddd07be0aa6ef019fc950d772c628d77b3567f646d7a837726aa3d15"
)
expected = {
    "native/src/retrieval.rs": "756cf82de843fae42cfbb26f520ed5df005cc0c0a256a6c5d13a7ebce09347d6",
    "native/src/retrieval_marker_decode.rs": "a279a5845b9fadfed75ffb9ec3d016f0d2c24661c6ed50ca253bf64de609af02",
    "native/src/retrieval_partition.rs": "4fece1b267f95c37ef52fc533c71acaf320480e3c2c6649d1c2f00f7570a2b55",
    "native/src/retrieval_scopes.rs": "937c5110e3f066610040367ff06d0035d13c6477db8b98894fed51f4cd18cae3",
    "native/src/retrieval_saved_graph_tests.rs": "4def3db064be0bd175e9251f28ae101efb62dfffdbb3ad5af191678273f85bbf",
}
old = json.loads((EVIDENCE / "wave5-frozen-20261016-0050/source-freeze.json").read_bytes())
runner = runpy.run_path(str(RUNNER))
current = runner["source_manifest"]()
changed = {name: current.get(name) for name in old.keys() | current.keys()
           if old.get(name) != current.get(name)}
assert changed == expected, changed
record = {
    "scope": "reviewed_clippy_only_delta_not_test_acceptance",
    "failedTag": "wave5-frozen-20261016-0050",
    "oldSourceManifestSha256": runner["digest"](runner["encoded"](old)),
    "newSourceManifestSha256": runner["digest"](runner["encoded"](current)),
    "sourceFiles": len(current),
    "changed": {name: {"before": old[name], "after": value}
                for name, value in sorted(changed.items())},
    "rootReview": "Exact mechanical guards and ScopeSelection argument grouping reviewed; no new semantics or lint allowances.",
}
with (EVIDENCE / "wave5-clippy-repair-review.json").open("x", encoding="utf-8") as handle:
    json.dump(record, handle, indent=2)
print(json.dumps(record))
