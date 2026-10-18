"""Record one selected Rust gate against a pinned isolated source image."""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import time

ROOT = Path(__file__).resolve().parents[3]
EVIDENCE = ROOT / "target/parity-validation-20261015"
IMAGE = ROOT / "target/r5-shadow-unavailable-v1"
MAX_FILE_BYTES = 16 * 1024 * 1024
MAX_TOTAL_BYTES = 256 * 1024 * 1024
MAX_ENTRIES = 20_000
MAX_MANIFEST_BYTES = 8 * 1024 * 1024


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def encoded(value):
    return (json.dumps(value, indent=2, ensure_ascii=False, sort_keys=True) + "\n").encode()


def inventory():
    assert IMAGE.is_dir() and IMAGE.resolve(strict=True) == IMAGE
    result = {}
    entries = 0
    total_bytes = 0
    for parent, directories, files in os.walk(IMAGE, followlinks=False):
        for leaf in [*directories, *files]:
            entries += 1
            assert entries <= MAX_ENTRIES
            path = Path(parent) / leaf
            info = path.lstat()
            assert not info.st_file_attributes & stat.FILE_ATTRIBUTE_REPARSE_POINT
            assert not stat.S_ISLNK(info.st_mode)
            if leaf in files:
                assert stat.S_ISREG(info.st_mode)
                assert 0 <= info.st_size <= MAX_FILE_BYTES
                total_bytes += info.st_size
                assert total_bytes <= MAX_TOTAL_BYTES
                digest = hashlib.sha256()
                read_bytes = 0
                with path.open("rb") as stream:
                    while chunk := stream.read(1024 * 1024):
                        read_bytes += len(chunk)
                        assert read_bytes <= info.st_size
                        digest.update(chunk)
                assert read_bytes == info.st_size
                current = path.lstat()
                assert (info.st_dev, info.st_ino, info.st_size, info.st_mtime_ns) == (current.st_dev, current.st_ino, current.st_size, current.st_mtime_ns)
                result[path.relative_to(IMAGE).as_posix()] = digest.hexdigest()
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--tag", required=True)
    parser.add_argument("--temp", required=True)
    parser.add_argument("--expected-manifest", required=True)
    parser.add_argument("--reviewed-manifest-sha256", required=True)
    parser.add_argument("arguments", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    assert re.fullmatch(r"r5-[a-z0-9-]+", args.tag)
    manifest_path = Path(args.expected_manifest).resolve(strict=True)
    assert manifest_path.is_relative_to(EVIDENCE.resolve(strict=True))
    assert manifest_path.stat().st_size <= MAX_MANIFEST_BYTES
    raw = manifest_path.read_bytes()
    assert sha(raw) == args.reviewed_manifest_sha256
    expected = json.loads(raw)
    assert isinstance(expected, dict) and all(isinstance(name, str) and re.fullmatch(r"[0-9a-f]{64}", value) for name, value in expected.items())
    before = inventory()
    assert before == expected
    suffix = args.arguments[1:] if args.arguments[:1] == ["--"] else args.arguments
    assert suffix and suffix[0] in ("--lib", "--test")
    command = ["cargo", "test", "--workspace", "--locked", "-j2", "--no-fail-fast",
               "--manifest-path", str(IMAGE / "Cargo.toml"), "--target-dir", str(ROOT / "target"), *suffix]
    temp = Path(args.temp).resolve()
    assert temp != Path("D:/Yoke-Test-Scratch") and temp.is_relative_to(Path("D:/Yoke-Test-Scratch"))
    assert not temp.exists()
    output = EVIDENCE / args.tag
    output.mkdir(exist_ok=False)
    temp.mkdir()
    (output / "source-before.json").write_bytes(encoded(before))
    print(json.dumps({"scope": "isolated_image_selected_gate", "command": command,
                      "sourceFiles": len(before), "log": str(output / "run.log")}), flush=True)
    start = time.monotonic()
    with (output / "run.log").open("xb") as stream:
        status = subprocess.run(["rtk", "proxy", *command], cwd=IMAGE,
                                env=dict(os.environ, TEMP=str(temp), TMP=str(temp)),
                                stdout=stream, stderr=subprocess.STDOUT).returncode
    after = inventory()
    (output / "source-after.json").write_bytes(encoded(after))
    data = (output / "run.log").read_bytes()
    groups = re.findall(rb"test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;", data)
    record = {
        "scope": "isolated_source_image_selected_gate_not_normal_checkout_acceptance",
        "checkedAtUtc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "command": command, "exitCode": status, "seconds": round(time.monotonic() - start, 3),
        "imageSourceChanges": sorted(name for name in before.keys() | after.keys() if before.get(name) != after.get(name)),
        "sourceFiles": len(before), "imageManifestSha256": sha(encoded(before)),
        "reviewedManifestFileSha256": sha(raw), "tempRoot": str(temp),
        "logSha256": sha(data), "groups": len(groups),
        "passed": sum(int(g[0]) for g in groups), "failed": sum(int(g[1]) for g in groups),
        "ignored": sum(int(g[2]) for g in groups),
        "interpretation": "Compiler or fixture failures are not feature RED. Only this exact image and selected commands were exercised. The root checkout requires its own later regression/full acceptance.",
        "helperSha256": sha(Path(__file__).read_bytes()),
    }
    record["validationErrors"] = ["isolated image source changed during the selected gate"] if record["imageSourceChanges"] else []
    (output / "receipt.json").write_bytes(encoded(record))
    print(json.dumps(record), flush=True)
    lines = data.decode("utf-8", errors="replace").splitlines()
    diagnostic = [line[:1200] for line in lines if line.startswith(("error", "Error:", "test result:", "---- ")) or " ... FAILED" in line]
    print("\n".join(diagnostic[-60:]), flush=True)
    return status or (1 if record["validationErrors"] else 0)


if __name__ == "__main__":
    raise SystemExit(main())
