"""Root-only fresh five-phase acceptance on the promoted R3 normal freeze."""
import datetime
from pathlib import Path
import subprocess
import sys

sys.dont_write_bytecode = True
import normal_runner_common as common

COMMON_SHA = "ef78be98e82967e3d3d96fcfc8e6f6ec1a4a21428a9f80f3c0b7f0ceb72a15eb"
GOLDEN_SHA = "0c21b1e45653b00393243efeb8ffcef5c1741b863f2283ad739fc2aa2e59b7d2"
PHASES = ("format", "clippy", "python", "typescript", "native", "finalize")


def main():
    expected = common.verify_inputs(COMMON_SHA)
    h18_path = common.EVIDENCE / common.H18_TAG / "receipt.json"
    h18 = common.load_json(h18_path)
    common.require(h18["accepted"] is True and h18["exitCode"] == 0 and h18["origin"] == "fresh_execution"
                   and h18["sourceManifestSha256"] == common.EXPECTED_SOURCE
                   and h18["passed"] == 51 and h18["groups"] == 6
                   and h18["failed"] == h18["ignored"] == 0 and not h18["validationErrors"]
                   and not h18["sourceChanges"], "Fresh normal H18 is not accepted")
    h18_bundle = h18_path.parent
    common.require(common.file_hash(h18_bundle / "h18.log") == h18["rawSha256"]
                   and common.file_hash(h18_bundle / "source-before.json") == h18["sourceBeforeSha256"] == common.EXPECTED_SOURCE
                   and common.file_hash(h18_bundle / "source-after.json") == h18["sourceAfterSha256"] == common.EXPECTED_SOURCE
                   and common.file_hash(common.HERE / "run_h18_normal.py") == h18["wrapperSha256"]
                   and h18["commonHelperSha256"] == COMMON_SHA
                   and h18["frozenRunnerSha256"] == common.RUNNER_SHA
                   and h18["testNameOracleSha256"] == GOLDEN_SHA,
                   "H18 artifact/helper/source closure differs")
    common.require(common.file_hash(common.HERE / "H18-EXPECTED-TESTS.json") == GOLDEN_SHA,
                   "H18 name oracle changed")
    parser = common.frozen_runner()
    actual_h18 = parser.group_map(parser.harness_groups((h18_bundle / "h18.log").read_bytes(), False), common.TARGETS)
    common.require(actual_h18 == h18["actualGroups"], "H18 raw results differ from receipt groups")
    golden = common.load_json(common.HERE / "H18-EXPECTED-TESTS.json")["targets"]
    for target, group in actual_h18.items():
        common.require(set(group["rows"]) == set(golden[target])
                       and set(group["rows"].values()) == {"ok"}
                       and group["passed"] == len(golden[target])
                       and all(group[key] == 0 for key in ("failed", "ignored", "filtered", "measured")),
                       "H18 raw name/result oracle differs: " + target)
    common.require(sum(group["passed"] for group in actual_h18.values()) == 51,
                   "H18 raw count differs from oracle")
    h18_sha = common.file_hash(h18_path)
    before = common.check_source(expected)
    bundle = common.EVIDENCE / common.FULL_TAG
    output = common.EVIDENCE / common.PIPELINE_TAG
    common.require(not bundle.exists() and not output.exists(), "Acceptance/pipeline evidence already exists")
    for phase in PHASES[:-1]:
        common.fresh_fixture(Path("D:/Yoke-Test-Scratch") / (common.FULL_TAG + "-" + phase), "D")
    acl = Path("C:/Users/Public/Yoke-Test-Scratch") / (common.FULL_TAG + "-acl")
    common.fresh_fixture(acl, "C")
    output.mkdir()
    common.write_json(output / "source-before.json", before)
    self_sha = common.file_hash(Path(__file__))
    results, errors, after, status = [], [], None, 1
    raw = output / "pipeline.log"
    try:
        with raw.open("xb") as stream:
            for phase in PHASES:
                common.verify_inputs(COMMON_SHA)
                common.check_source(expected)
                common.require(common.file_hash(Path(__file__)) == self_sha, "Acceptance wrapper changed")
                command = [sys.executable, "-X", "utf8", str(common.RUNNER), phase,
                           "--tag", common.FULL_TAG]
                if phase != "finalize":
                    command += ["--temp", str(Path("D:/Yoke-Test-Scratch") / (common.FULL_TAG + "-" + phase))]
                if phase == "native":
                    command += ["--acl-temp", str(acl)]
                print(common.encoded({"pipelineStage": phase, "tag": common.FULL_TAG}).decode("utf-8"), flush=True)
                status = subprocess.run(["rtk", "proxy", *command], cwd=common.ROOT,
                                        stdout=stream, stderr=subprocess.STDOUT).returncode
                stream.flush()
                stage = {"stage": phase, "command": command, "exitCode": status}
                results.append(stage)
                receipt_path = bundle / ("summary.json" if phase == "finalize" else phase + "/receipt.json")
                common.require(receipt_path.exists(), "Stage did not create its receipt: " + phase)
                receipt = common.load_json(receipt_path)
                stage.update(receiptSha256=common.file_hash(receipt_path),
                             **{key: receipt.get(key) for key in ("accepted", "passed", "failed", "ignored", "groups", "sourceChanges", "validationErrors")})
                common.require(receipt["sourceManifestSha256"] == common.EXPECTED_SOURCE
                               and receipt["runnerSha256"] == common.RUNNER_SHA,
                               "Stage source/runner differs from promoted freeze")
                print(common.encoded(stage).decode("utf-8"), flush=True)
                common.require(status == 0 and receipt["accepted"] is True, "Unaccepted stage: " + phase)
    except Exception as error:
        errors.append(type(error).__name__ + ": " + str(error))
    try:
        after = common.source_manifest()
        common.write_json(output / "source-after.json", after)
        common.require(after == expected, "Normal source changed during acceptance")
        common.verify_inputs(COMMON_SHA)
        common.require(common.file_hash(Path(__file__)) == self_sha, "Acceptance wrapper changed")
        common.require(common.file_hash(h18_path) == h18_sha, "H18 receipt changed during acceptance")
    except Exception as error:
        errors.append(type(error).__name__ + ": " + str(error))
    accepted = not errors and len(results) == 6 and all(row["exitCode"] == 0 and row.get("accepted") is True for row in results)
    record = {
        "scope": "sequential_orchestration_only_use_individual_phase_receipts",
        "checkedAtUtc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "tag": common.FULL_TAG, "pipelineTag": common.PIPELINE_TAG, "stages": results,
        "accepted": accepted, "exitCode": 0 if accepted else 1, "validationErrors": errors,
        "runnerSha256": common.RUNNER_SHA, "pipelineSha256": self_sha,
        "commonHelperSha256": COMMON_SHA, "pipelineLogSha256": common.file_hash(raw),
        "sourceManifestSha256": common.EXPECTED_SOURCE,
        "sourceFilesBefore": len(before), "sourceFilesAfter": len(after) if after is not None else None,
        "sourceBeforeSha256": common.file_hash(output / "source-before.json"),
        "sourceAfterSha256": common.file_hash(output / "source-after.json") if after is not None else None,
        "sourceChanges": common.source_changes(before, after) if after is not None else None,
        "promotionManifestSha256": common.PROMOTION_SHA,
        "h18ReceiptSha256": h18_sha,
        "baseWrapperSha256": common.BASE_WRAPPER_SHA,
        "cacheOriginQuarantineInvokedByWrapper": False,
        "cacheOriginPrecondition": "Root must record the reviewed own-package fingerprint quarantine before switching from R5 to normal; do not switch Cargo source context or quarantine between normal phases.",
        "qualifiedCapacityRuns": "separate_nonadditive_receipts_not_reexecuted_by_this_wrapper",
        "interpretation": "No estimated or inherited test-count credit. Only fresh frozen phase receipts and their verified finalize summary establish full Windows acceptance; this pipeline record only records orchestration."
    }
    common.write_json(output / "pipeline.json", record)
    print(common.encoded(record).decode("utf-8"), flush=True)
    return record["exitCode"]


if __name__ == "__main__":
    raise SystemExit(main())
