"""Read-only inverse of the frozen Clippy patch; never writes source files."""
import hashlib
import json
from pathlib import Path
import re

BASE = Path(__file__).resolve().parent
WORKSPACE = BASE.parents[5]


def digest(data):
    return hashlib.sha256(data).hexdigest()


def main():
    pins = json.loads((BASE / "before-sha256.json").read_bytes())
    lines = (BASE / "CLIPPY-ONLY.patch").read_text(encoding="utf-8").splitlines(keepends=True)
    changes = {}
    name = None
    for line in lines:
        if line.startswith("+++ after/native/src/"):
            name = line.removeprefix("+++ after/native/src/").rstrip("\n")
            assert name in pins and name not in changes
            changes[name] = []
        elif line.startswith("@@ "):
            match = re.fullmatch(r"@@ -\d+(?:,\d+)? \+(\d+)(?:,\d+)? @@.*\n", line)
            assert match is not None and name is not None
            changes[name].append([int(match[1]) - 1, [], []])
        elif line.startswith("--- "):
            continue
        else:
            assert name is not None and changes[name] and line[:1] in (" ", "+", "-")
            _, after, before = changes[name][-1]
            if line[0] in (" ", "+"):
                after.append(line[1:])
            if line[0] in (" ", "-"):
                before.append(line[1:])
    assert set(changes) == set(pins)
    result = {}
    for name, hunks in changes.items():
        original = (BASE / "before" / name).read_bytes()
        final = (BASE / "after" / name).read_bytes()
        assert digest(original) == pins[name]
        assert (WORKSPACE / "native/src" / name).read_bytes() == final
        restored = final.decode("utf-8").splitlines(keepends=True)
        for start, after, before in reversed(hunks):
            assert restored[start:start + len(after)] == after
            restored[start:start + len(after)] = before
        restored_bytes = "".join(restored).encode("utf-8")
        assert restored_bytes == original
        result[name] = {"beforeSha256": pins[name], "afterSha256": digest(final),
                        "inverseSha256": digest(restored_bytes), "inverseEqualsOriginal": True}
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
