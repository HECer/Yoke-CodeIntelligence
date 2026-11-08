"""Record the one reviewed rustfmt-only assertion wrap without changing source."""
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]
CONTROL = Path(__file__).resolve().parent
path = ROOT / "native/tests/monorepo_pre_feature_envelope.rs"
before = (CONTROL / "revisions/monorepo_pre_feature_envelope.before-rustfmt.rs").read_bytes()
after = path.read_bytes()
old = b'    assert!(body["status"].as_str() == Some("error"), "metadata={summary}");'
new = b'''    assert!(
        body["status"].as_str() == Some("error"),
        "metadata={summary}"
    );'''
assert before.count(old) == 1 and after == before.replace(old, new)
result = {
    "file": path.relative_to(ROOT).as_posix(),
    "beforeSha256": hashlib.sha256(before).hexdigest(),
    "afterSha256": hashlib.sha256(after).hexdigest(),
    "onlyDelta": "rustfmt assertion line wrapping; expression and message unchanged",
    "testsExecutedByThisRepair": False,
}
with (CONTROL / "format-repair.json").open("x", encoding="utf-8") as handle:
    json.dump(result, handle, indent=2)
print(json.dumps(result))
