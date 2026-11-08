"""Fresh five-phase run after the separately recorded Clippy-only repair."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[4]
EVIDENCE = ROOT / "target/parity-validation-20261015"
RUNNER = EVIDENCE / "acceptance_wave4.py"
TAG = "wave5-frozen-20261016-0110"
assert hashlib.sha256(RUNNER.read_bytes()).hexdigest() == (
    "b51bbffbddd07be0aa6ef019fc950d772c628d77b3567f646d7a837726aa3d15"
)
assert not (EVIDENCE / TAG).exists()

python = [sys.executable, "-X", "utf8"]
stages = []
for phase in ["format", "clippy", "python", "typescript", "native"]:
    command = python + [str(RUNNER), phase, "--tag", TAG,
        "--temp", f"D:/Yoke-Test-Scratch/{TAG}-{phase}"]
    if phase == "native":
        command += ["--acl-temp", f"C:/Users/Public/Yoke-Test-Scratch/{TAG}-acl"]
    stages.append((phase, command))
stages.append(("finalize", python + [str(RUNNER), "finalize", "--tag", TAG]))

results = []
status = 1
for label, command in stages:
    print(json.dumps({"pipelineStage": label, "command": command}), flush=True)
    status = subprocess.run(["rtk", "proxy", *command], cwd=ROOT).returncode
    results.append({"stage": label, "exitCode": status})
    if status != 0:
        break

record = {
    "scope": "sequential_orchestration_only_use_individual_phase_receipts",
    "tag": TAG,
    "stages": results,
    "exitCode": status,
    "runnerSha256": hashlib.sha256(RUNNER.read_bytes()).hexdigest(),
    "pipelineSha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
    "previousFailedTag": "wave5-frozen-20261016-0050",
    "qualifiedCapacityRuns": "separate_nonadditive_receipts_not_reexecuted_by_this_wrapper",
}
with (EVIDENCE / "wave5-pipeline-0110.json").open("x", encoding="utf-8") as handle:
    json.dump(record, handle, indent=2)
print(json.dumps(record), flush=True)
raise SystemExit(status)
