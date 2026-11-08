"""Record read-only formatting or strict Clippy against an exact isolated image."""
import argparse
import datetime
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess
import time

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
EVIDENCE = ROOT / "target/parity-validation-20261015"
IMAGE = ROOT / "target/r5b-affected-v1"


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def encoded(value):
    return (json.dumps(value, sort_keys=True, indent=2) + "\n").encode()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--kind", choices=["format", "clippy"], required=True)
    for name in ["tag", "temp", "expected-manifest", "reviewed-manifest-sha256"]:
        parser.add_argument("--" + name, required=True)
    args = parser.parse_args()
    assert re.fullmatch(r"r5b-[a-z0-9-]+", args.tag)
    manifest_path = Path(args.expected_manifest).resolve(strict=True)
    assert manifest_path.is_relative_to(HERE) and manifest_path.stat().st_size <= 8 * 1024 * 1024
    raw = manifest_path.read_bytes()
    assert sha(raw) == args.reviewed_manifest_sha256
    expected = json.loads(raw)
    assert isinstance(expected, dict) and all(isinstance(name, str) and re.fullmatch(r"[0-9a-f]{64}", value) for name, value in expected.items())
    spec = importlib.util.spec_from_file_location("image_gate", HERE / "shadow_gate.py")
    gate = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(gate)
    before = gate.inventory()
    assert before == expected
    common = ["--manifest-path", str(IMAGE / "Cargo.toml")]
    command = ["cargo", "fmt", "--all", *common, "--", "--check"] if args.kind == "format" else [
        "cargo", "clippy", "--workspace", "--keep-going", "--all-targets", "--locked", "-j2", *common,
        "--target-dir", str(ROOT / "target"), "--", "-D", "warnings"]
    temp = Path(args.temp).resolve()
    assert temp != Path("D:/Yoke-Test-Scratch") and temp.is_relative_to(Path("D:/Yoke-Test-Scratch")) and not temp.exists()
    output = EVIDENCE / args.tag
    output.mkdir(exist_ok=False)
    temp.mkdir()
    with (output / "source-before.json").open("xb") as stream:
        stream.write(encoded(before))
    print(json.dumps(dict(command=command, log=str(output / "run.log"))), flush=True)
    started = time.monotonic()
    with (output / "run.log").open("xb") as stream:
        code = subprocess.run(["rtk", "proxy", *command], cwd=IMAGE, env=dict(os.environ, TEMP=str(temp), TMP=str(temp)), stdout=stream, stderr=subprocess.STDOUT).returncode
    after = gate.inventory()
    with (output / "source-after.json").open("xb") as stream:
        stream.write(encoded(after))
    changes = sorted(name for name in before.keys() | after.keys() if before.get(name) != after.get(name))
    record = dict(kind=args.kind, scope="isolated_image_only_not_normal_checkout_acceptance", exitCode=code,
                  checkedAtUtc=datetime.datetime.now(datetime.timezone.utc).isoformat(), seconds=round(time.monotonic() - started, 3),
                  command=command, sourceFiles=len(before), imageSourceChanges=changes,
                  reviewedManifestFileSha256=sha(raw), rawSha256=sha((output / "run.log").read_bytes()),
                  helperSha256=sha(Path(__file__).read_bytes()), inventoryHelperSha256=sha((HERE / "shadow_gate.py").read_bytes()),
                  validationErrors=["isolated image source changed"] if changes else [])
    with (output / "receipt.json").open("xb") as stream:
        stream.write(encoded(record))
    print(json.dumps(record), flush=True)
    if code:
        print((output / "run.log").read_text(encoding="utf-8", errors="replace")[-18000:], flush=True)
    return code or bool(changes)


if __name__ == "__main__":
    raise SystemExit(main())
