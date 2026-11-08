"""Snapshot owned Wave 5 staging as opaque, nonadditive review evidence."""
import hashlib
import json
from pathlib import Path
import stat

ROOT = Path(__file__).resolve().parents[4]
EVIDENCE = ROOT / "target/parity-validation-20261015"
SOURCE = EVIDENCE / "next-universal-slice"
DESTINATION = EVIDENCE / "wave5-implementation-lineage"
ALLOWED = {".json", ".log", ".py", ".md", ".txt", ".patch", ".rs", ".ps1"}
assert ROOT.resolve(strict=True) == ROOT
assert not DESTINATION.exists()


def plain(path, directory):
    info = path.lstat()
    assert not info.st_file_attributes & stat.FILE_ATTRIBUTE_REPARSE_POINT, path
    assert stat.S_ISDIR(info.st_mode) if directory else stat.S_ISREG(info.st_mode)
    assert path.resolve(strict=True) == path


def scan():
    found = {}
    pending = [SOURCE]
    nodes = 0
    total = 0
    while pending:
        parent = pending.pop()
        plain(parent, True)
        assert parent.is_relative_to(SOURCE)
        assert len(parent.relative_to(SOURCE).parts) <= 20
        for path in parent.iterdir():
            nodes += 1
            assert nodes <= 2_000
            info = path.lstat()
            assert not info.st_file_attributes & stat.FILE_ATTRIBUTE_REPARSE_POINT
            if stat.S_ISDIR(info.st_mode):
                pending.append(path)
            else:
                plain(path, False)
                assert path.suffix in ALLOWED
                assert info.st_size <= 2_000_000
                data = path.read_bytes()
                total += len(data)
                assert total <= 10_000_000
                found[path.relative_to(SOURCE).as_posix()] = (
                    len(data), hashlib.sha256(data).hexdigest()
                )
    return found


before = scan()
plain(EVIDENCE, True)
DESTINATION.mkdir(exist_ok=False)
items = []
for relative, (size, digest) in sorted(before.items()):
    source = SOURCE / relative
    # Preserve exact bytes while explicitly treating Rust/PowerShell as inert
    # text, within the already reviewed preserver's closed raw suffix contract.
    target_name = relative + ".txt" if source.suffix in {".rs", ".ps1"} else relative
    target = DESTINATION / "staged" / target_name
    target.parent.mkdir(parents=True, exist_ok=True)
    plain(source, False)
    data = source.read_bytes()
    assert len(data) == size and hashlib.sha256(data).hexdigest() == digest
    with target.open("xb") as stream:
        stream.write(data)
    assert hashlib.sha256(target.read_bytes()).hexdigest() == digest
    items.append({"original": relative, "preserved": "staged/" + target_name,
                  "bytes": size, "sha256": digest})
assert scan() == before, "Staging changed during snapshot; retain partial evidence"
record = {
    "scope": "owned_implementation_lineage_only_not_a_test_gate",
    "acceptedFullRun": False,
    "countsInterpretation": "No test counts; byte copies and inverse/review lineage only",
    "source": SOURCE.relative_to(ROOT).as_posix(),
    "files": items,
    "filesCopied": len(items),
    "bytesCopied": sum(item["bytes"] for item in items),
    "sourceUnchanged": True,
    "scriptSha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
}
with (DESTINATION / "LINEAGE.json").open("x", encoding="utf-8") as stream:
    json.dump(record, stream, indent=2)
print(json.dumps({key: value for key, value in record.items() if key != "files"}))
