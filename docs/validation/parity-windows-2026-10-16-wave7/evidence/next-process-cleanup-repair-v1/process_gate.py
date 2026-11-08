"""Record a selected process regression with explicit fresh D: fixtures."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import time

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--tag", required=True)
    parser.add_argument("tests", nargs="+")
    args = parser.parse_args()
    assert re.fullmatch(r"[a-z0-9-]+", args.tag)
    assert all(name in ("process-cleanup", "process-killer-failure", "process-deadline-transition", "process-spawn-failure") for name in args.tests)
    output = HERE / args.tag
    output.mkdir(exist_ok=False)
    temp = Path("D:/Yoke-Test-Scratch") / args.tag
    assert not temp.exists()
    temp.mkdir()
    names = ["src/core/process.ts", "dist/core/process.js", *[f"test/{name}.test.mjs" for name in args.tests]]
    before = {name: hashlib.sha256((ROOT / name).read_bytes()).hexdigest() for name in names}
    command = ["node", "--test", *[f"test/{name}.test.mjs" for name in args.tests]]
    start = time.monotonic()
    with (output / "run.log").open("xb") as stream:
        status = subprocess.run(["rtk", "proxy", *command], cwd=ROOT,
                                env=dict(os.environ, TEMP=str(temp), TMP=str(temp)), stdout=stream, stderr=subprocess.STDOUT).returncode
    after = {name: hashlib.sha256((ROOT / name).read_bytes()).hexdigest() for name in names}
    data = (output / "run.log").read_bytes()
    counts = dict(re.findall(r"^[#ℹ]\s+(tests|pass|fail|skipped|cancelled|todo)\s+(\d+)\s*$", data.decode("utf-8"), re.MULTILINE))
    record = dict(command=command, exitCode=status, seconds=round(time.monotonic()-start, 3), counts=counts,
                  tempRoot=str(temp), logSha256=hashlib.sha256(data).hexdigest(), before=before, after=after)
    (output / "receipt.json").write_text(json.dumps(record, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(json.dumps(record))
    print(data.decode("utf-8")[-8000:])
    assert before == after
    return status


if __name__ == "__main__":
    raise SystemExit(main())
