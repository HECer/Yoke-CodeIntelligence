"""Root-only fresh H18 regression on the exact promoted normal2406 source."""
import datetime
import os
from pathlib import Path
import subprocess
import sys
import time

sys.dont_write_bytecode = True
import normal_runner_common as common

COMMON_SHA = "ef78be98e82967e3d3d96fcfc8e6f6ec1a4a21428a9f80f3c0b7f0ceb72a15eb"
GOLDEN_SHA = "0c21b1e45653b00393243efeb8ffcef5c1741b863f2283ad739fc2aa2e59b7d2"
COMMAND = ["cargo", "test", "--workspace", "--locked", "-j", "2", "--no-fail-fast",
           *[part for target in common.TARGETS for part in ("--test", target)],
           "--", "--test-threads=4"]


def main():
    expected = common.verify_inputs(COMMON_SHA)
    golden_path = common.HERE / "H18-EXPECTED-TESTS.json"
    common.require(common.file_hash(golden_path) == GOLDEN_SHA, "H18 historical name oracle changed")
    golden = common.load_json(golden_path)
    common.require(set(golden["targets"]) == set(common.TARGETS)
                   and sum(len(names) for names in golden["targets"].values()) == 51,
                   "H18 target/name oracle drift")
    before = common.check_source(expected)
    output = common.EVIDENCE / common.H18_TAG
    common.require(not output.exists(), "H18 evidence tag already exists; preserve it and prepare a new tag")
    temporary = Path("D:/Yoke-Test-Scratch") / common.H18_TAG
    common.fresh_fixture(temporary, "D")
    parser = common.frozen_runner()
    temporary = parser.new_temp(str(temporary), "D")
    output.mkdir()
    common.write_json(output / "source-before.json", before)
    self_sha = common.file_hash(Path(__file__))
    errors, groups, status, after = [], {}, None, None
    started = time.monotonic()
    raw = output / "h18.log"
    try:
        with raw.open("xb") as stream:
            status = subprocess.run(["rtk", "proxy", *COMMAND], cwd=common.ROOT,
                                    env=dict(os.environ, TEMP=str(temporary), TMP=str(temporary),
                                             PYTHONUTF8="1", NO_COLOR="1", CARGO_TERM_COLOR="never"),
                                    stdout=stream, stderr=subprocess.STDOUT).returncode
        groups = parser.group_map(parser.harness_groups(raw.read_bytes(), False), common.TARGETS)
        common.require(status == 0, "H18 Cargo returned nonzero")
        for target, group in groups.items():
            common.require(set(group["rows"]) == set(golden["targets"][target]), "H18 test names differ: " + target)
            common.require(set(group["rows"].values()) == {"ok"}
                           and group["passed"] == len(golden["targets"][target])
                           and all(group[key] == 0 for key in ("failed", "ignored", "filtered", "measured")),
                           "H18 result/selection mismatch: " + target)
        common.require(len(groups) == 6 and sum(group["passed"] for group in groups.values()) == 51,
                       "H18 actual counts differ from six-target/51-test oracle")
    except Exception as error:
        errors.append(type(error).__name__ + ": " + str(error))
    try:
        after = common.source_manifest()
        common.write_json(output / "source-after.json", after)
        common.require(after == expected, "Normal source changed during H18")
        common.verify_inputs(COMMON_SHA)
        common.require(common.file_hash(Path(__file__)) == self_sha, "H18 wrapper changed")
        common.require(common.file_hash(golden_path) == GOLDEN_SHA, "H18 oracle changed during execution")
    except Exception as error:
        errors.append(type(error).__name__ + ": " + str(error))
    receipt = {
        "scope": "H18_six_old_targets_fresh_normal_execution",
        "checkedAtUtc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "tag": common.H18_TAG, "command": COMMAND, "cwd": str(common.ROOT),
        "origin": "fresh_execution", "exitCode": status, "seconds": round(time.monotonic() - started, 3),
        "accepted": status == 0 and not errors and len(groups) == 6,
        "expectedPassed": 51, "expectedGroups": 6,
        "actualGroups": groups,
        **{key: sum(group.get(key, 0) for group in groups.values()) if groups else None
           for key in ("passed", "failed", "ignored")},
        "groups": len(groups) if groups else None, "resultGroupsParsed": bool(groups), "tempRoot": str(temporary),
        "sourceFilesBefore": len(before), "sourceFilesAfter": len(after) if after is not None else None,
        "sourceManifestSha256": common.EXPECTED_SOURCE,
        "sourceBeforeSha256": common.file_hash(output / "source-before.json"),
        "sourceAfterSha256": common.file_hash(output / "source-after.json") if after is not None else None,
        "sourceChanges": common.source_changes(before, after) if after is not None else None,
        "validationErrors": errors,
        "rawSha256": common.file_hash(raw) if raw.exists() else None,
        "wrapperSha256": self_sha, "commonHelperSha256": COMMON_SHA,
        "frozenRunnerSha256": common.RUNNER_SHA, "testNameOracleSha256": GOLDEN_SHA,
        "promotionManifestSha256": common.PROMOTION_SHA,
        "cacheOriginQuarantineInvokedByWrapper": False,
        "cacheOriginPrecondition": "Root must record the reviewed own-package fingerprint quarantine before the R5-to-normal Cargo context switch; this wrapper does not perform or certify that action.",
        "interpretation": "Only this fresh execution may establish H18. Historical names are a selection oracle, not inherited pass credit. No full Windows acceptance follows from this focused receipt."
    }
    common.write_json(output / "receipt.json", receipt)
    print(common.encoded(receipt).decode("utf-8"), flush=True)
    return 0 if receipt["accepted"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
