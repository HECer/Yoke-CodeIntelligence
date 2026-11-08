"""Root-only combined normal H18, retaining all 51 names in three disk-bounded batches."""
import datetime
import os
from pathlib import Path
import subprocess
import sys
import time

sys.dont_write_bytecode = True
import combined_common as common

COMMON_SHA = "035eb4e6f0a513b5c6bc271803640678feca1c73b8015167ad7602de9b013751"
GOLDEN_SHA = "0c21b1e45653b00393243efeb8ffcef5c1741b863f2283ad739fc2aa2e59b7d2"


def main():
    ctx = common.admit(common.parse_arguments(), COMMON_SHA)
    golden_path = common.HERE / "H18-EXPECTED-TESTS.json"
    common.require(common.file_hash(golden_path) == GOLDEN_SHA, "H18 name oracle changed")
    golden = common.load_json(golden_path)["targets"]
    common.require(set(golden) == set(common.TARGETS) and sum(map(len, golden.values())) == 51,
                   "H18 exact 51-name/six-target oracle drift")
    before = ctx.check_source()
    output = common.EVIDENCE / ctx.h18_tag
    common.require(not output.exists(), "H18 evidence tag exists; preserve it and use a reviewed fresh tag")
    temporary = Path("D:/Yoke-Test-Scratch") / ctx.h18_tag
    common.fresh_fixture(temporary, "D")
    parser = common.frozen_runner()
    temporary = parser.new_temp(str(temporary), "D")
    output.mkdir()
    common.write_json(output / "source-before.json", before)
    self_sha = common.file_hash(Path(__file__))
    errors, groups, results, status, after = [], {}, [], None, None
    started = time.monotonic()
    deps = common.ROOT / "target/debug/deps"
    try:
        # Existing frozen-runner exact-leaf cleanup only; no tree/cache deletion.
        common.write_json(output / "cleaned-before.json", parser.checked_cleanup(deps, common.TARGETS))
        for offset in range(0, len(common.TARGETS), 2):
            ctx.verify()
            ctx.check_source()
            common.require(common.file_hash(Path(__file__)) == self_sha, "H18 wrapper changed")
            targets = common.TARGETS[offset:offset + 2]
            command = common.BASE + [part for target in targets for part in ("--test", target)] + ["--", "--test-threads=4"]
            label = "h18-" + str(offset // 2).zfill(2)
            raw = output / (label + ".log")
            with raw.open("xb") as stream:
                status = subprocess.run(["rtk", "proxy", *command], cwd=common.ROOT,
                                        env=dict(os.environ, TEMP=str(temporary), TMP=str(temporary),
                                                 PYTHONUTF8="1", NO_COLOR="1", CARGO_TERM_COLOR="never"),
                                        stdout=stream, stderr=subprocess.STDOUT).returncode
            result = {"label": label, "targets": list(targets), "command": command,
                      "exitCode": status, "rawSha256": common.file_hash(raw), "origin": "fresh_execution"}
            results.append(result)
            actual = parser.group_map(parser.harness_groups(raw.read_bytes(), False), targets)
            common.require(status == 0, "H18 Cargo batch returned nonzero")
            for target, group in actual.items():
                common.require(target not in groups, "H18 target selected twice")
                groups[target] = group
                common.require(set(group["rows"]) == set(golden[target]) and set(group["rows"].values()) == {"ok"}
                               and group["passed"] == len(golden[target])
                               and all(group[key] == 0 for key in ("failed", "ignored", "filtered", "measured")),
                               "H18 exact names/results differ: " + target)
            ctx.verify()
            ctx.check_source()
            common.write_json(output / (label + "-cleaned.json"), parser.checked_cleanup(deps, targets))
        common.require(set(groups) == set(common.TARGETS) and len(results) == 3
                       and sum(group["passed"] for group in groups.values()) == 51, "H18 coverage incomplete")
    except Exception as error:
        errors.append(type(error).__name__ + ": " + str(error))
    try:
        after = common.source_manifest()
        common.write_json(output / "source-after.json", after)
        common.require(after == ctx.expected, "Normal source changed during H18")
        ctx.verify()
        common.require(common.file_hash(Path(__file__)) == self_sha, "H18 wrapper changed")
        common.require(common.file_hash(golden_path) == GOLDEN_SHA, "H18 oracle changed")
    except Exception as error:
        errors.append(type(error).__name__ + ": " + str(error))
    artifacts = {path.name: common.file_hash(path) for path in sorted(output.iterdir()) if path.is_file()}
    receipt = {
        "scope": "combined_normal_H18_six_old_targets", "origin": "fresh_execution",
        "checkedAtUtc": datetime.datetime.now(datetime.timezone.utc).isoformat(), "tag": ctx.h18_tag,
        "accepted": status == 0 and not errors and len(groups) == 6 and len(results) == 3,
        "exitCode": status, "seconds": round(time.monotonic() - started, 3),
        "expectedPassed": 51, "expectedGroups": 6, "results": results, "actualGroups": groups,
        **{key: sum(group[key] for group in groups.values()) if groups else None for key in ("passed", "failed", "ignored")},
        "groups": len(groups) if groups else None,
        "sourceChanges": common.source_changes(before, after) if after is not None else None,
        "validationErrors": errors, "sourceFilesBefore": len(before),
        "sourceFilesAfter": len(after) if after is not None else None,
        "artifactSha256": artifacts, "tempRoot": str(temporary),
        "wrapperSha256": self_sha, "commonHelperSha256": COMMON_SHA,
        "frozenRunnerSha256": common.RUNNER_SHA, "testNameOracleSha256": GOLDEN_SHA,
        **ctx.receipt_context(),
        "interpretation": "All six original targets and all 51 names must freshly pass. Three sequential two-target commands retain the original parameters and use only frozen-runner exact-leaf cleanup; no inherited or full-acceptance credit."
    }
    common.write_json(output / "receipt.json", receipt)
    print(common.encoded(receipt).decode("utf-8"), flush=True)
    return 0 if receipt["accepted"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
