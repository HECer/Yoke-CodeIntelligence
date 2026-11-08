"""Root-only original Windows phases/finalize on one reviewed combined normal map."""
import datetime
from pathlib import Path
import subprocess
import sys

sys.dont_write_bytecode = True
import combined_common as common

COMMON_SHA = "035eb4e6f0a513b5c6bc271803640678feca1c73b8015167ad7602de9b013751"
GOLDEN_SHA = "0c21b1e45653b00393243efeb8ffcef5c1741b863f2283ad739fc2aa2e59b7d2"


def checked_h18(ctx):
    bundle = common.EVIDENCE / ctx.h18_tag
    path = bundle / "receipt.json"
    receipt = common.load_json(path)
    common.require(receipt["accepted"] is True and receipt["exitCode"] == 0 and receipt["origin"] == "fresh_execution"
                   and receipt["passed"] == 51 and receipt["groups"] == 6
                   and receipt["failed"] == receipt["ignored"] == 0
                   and not receipt["validationErrors"] and not receipt["sourceChanges"], "Combined H18 not accepted")
    common.require(all(receipt.get(key) == value for key, value in ctx.receipt_context().items()), "H18 admission context differs")
    common.require(receipt["commonHelperSha256"] == COMMON_SHA and receipt["frozenRunnerSha256"] == common.RUNNER_SHA
                   and receipt["testNameOracleSha256"] == GOLDEN_SHA
                   and receipt["wrapperSha256"] == common.file_hash(common.HERE / "run_h18_combined.py"), "H18 wrapper/helper pins differ")
    for name, pin in receipt["artifactSha256"].items():
        common.require(common.relative_path(name).name == name and common.file_hash(bundle / name) == pin,
                       "H18 artifact closure differs")
    common.require(common.file_hash(bundle / "source-before.json") == common.file_hash(bundle / "source-after.json") == ctx.source_sha,
                   "H18 full source maps differ")
    common.require(common.file_hash(common.HERE / "H18-EXPECTED-TESTS.json") == GOLDEN_SHA, "H18 oracle changed")
    golden = common.load_json(common.HERE / "H18-EXPECTED-TESTS.json")["targets"]
    parser = common.frozen_runner()
    actual = {}
    common.require(len(receipt["results"]) == 3, "H18 batch coverage differs")
    for offset, result in enumerate(receipt["results"]):
        targets = common.TARGETS[offset * 2:offset * 2 + 2]
        label = "h18-" + str(offset).zfill(2)
        command = common.BASE + [part for target in targets for part in ("--test", target)] + ["--", "--test-threads=4"]
        common.require(result["label"] == label and result["targets"] == list(targets) and result["command"] == command
                       and result["origin"] == "fresh_execution" and result["exitCode"] == 0
                       and common.file_hash(bundle / (label + ".log")) == result["rawSha256"], "H18 raw command/result differs")
        actual.update(parser.group_map(parser.harness_groups((bundle / (label + ".log")).read_bytes(), False), targets))
    common.require(actual == receipt["actualGroups"] and set(actual) == set(common.TARGETS), "H18 raw groups differ")
    for target, group in actual.items():
        common.require(set(group["rows"]) == set(golden[target]) and set(group["rows"].values()) == {"ok"}
                       and group["passed"] == len(golden[target])
                       and all(group[key] == 0 for key in ("failed", "ignored", "filtered", "measured")), "H18 exact name/result oracle differs")
    return path, common.file_hash(path)


def main():
    ctx = common.admit(common.parse_arguments(), COMMON_SHA)
    h18_path, h18_sha = checked_h18(ctx)
    before = ctx.check_source()
    bundle, output = common.EVIDENCE / ctx.full_tag, common.EVIDENCE / ctx.pipeline_tag
    common.require(not bundle.exists() and not output.exists(), "Combined full/pipeline evidence tag already exists")
    for phase in common.PHASES[:-1]:
        common.fresh_fixture(Path("D:/Yoke-Test-Scratch") / (ctx.full_tag + "-" + phase), "D")
    acl = Path("C:/Users/Public/Yoke-Test-Scratch") / (ctx.full_tag + "-acl")
    common.fresh_fixture(acl, "C")
    output.mkdir()
    common.write_json(output / "source-before.json", before)
    self_sha = common.file_hash(Path(__file__))
    results, errors, after = [], [], None
    raw = output / "pipeline.log"
    try:
        with raw.open("xb") as stream:
            for phase in common.PHASES:
                ctx.verify()
                ctx.check_source()
                common.require(common.file_hash(Path(__file__)) == self_sha, "Combined wrapper changed")
                command = [sys.executable, "-X", "utf8", str(common.RUNNER), phase, "--tag", ctx.full_tag]
                if phase != "finalize":
                    command += ["--temp", str(Path("D:/Yoke-Test-Scratch") / (ctx.full_tag + "-" + phase))]
                if phase == "native":
                    command += ["--acl-temp", str(acl)]
                print(common.encoded({"pipelineStage": phase, "tag": ctx.full_tag}).decode("utf-8"), flush=True)
                status = subprocess.run(["rtk", "proxy", *command], cwd=common.ROOT, stdout=stream, stderr=subprocess.STDOUT).returncode
                stream.flush()
                stage = {"stage": phase, "command": command, "exitCode": status}
                results.append(stage)
                receipt_path = bundle / ("summary.json" if phase == "finalize" else phase + "/receipt.json")
                receipt = common.load_json(receipt_path)
                stage.update(receiptSha256=common.file_hash(receipt_path),
                             **{key: receipt.get(key) for key in ("accepted", "passed", "failed", "ignored", "groups", "sourceChanges", "validationErrors")})
                common.require(receipt["sourceManifestSha256"] == ctx.source_sha and receipt["runnerSha256"] == common.RUNNER_SHA,
                               "Phase runner/source differs from reviewed final combined freeze")
                common.require(status == 0 and receipt["accepted"] is True, "Unaccepted phase: " + phase)
    except Exception as error:
        errors.append(type(error).__name__ + ": " + str(error))
    try:
        after = common.source_manifest()
        common.write_json(output / "source-after.json", after)
        common.require(after == ctx.expected, "Normal source changed during combined acceptance")
        ctx.verify()
        common.require(common.file_hash(Path(__file__)) == self_sha, "Combined wrapper changed")
        common.require(checked_h18(ctx)[1] == h18_sha, "H18 artifact closure changed during acceptance")
    except Exception as error:
        errors.append(type(error).__name__ + ": " + str(error))
    accepted = not errors and len(results) == 6 and all(row["exitCode"] == 0 and row.get("accepted") is True for row in results)
    record = {"scope": "combined_normal_sequential_orchestration_only_use_phase_receipts",
              "checkedAtUtc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
              "tag": ctx.full_tag, "pipelineTag": ctx.pipeline_tag, "stages": results,
              "accepted": accepted, "exitCode": 0 if accepted else 1, "validationErrors": errors,
              "runnerSha256": common.RUNNER_SHA, "pipelineSha256": self_sha,
              "commonHelperSha256": COMMON_SHA, "pipelineLogSha256": common.file_hash(raw),
              "sourceFilesBefore": len(before), "sourceFilesAfter": len(after) if after is not None else None,
              "sourceBeforeSha256": common.file_hash(output / "source-before.json"),
              "sourceAfterSha256": common.file_hash(output / "source-after.json") if after is not None else None,
              "sourceChanges": common.source_changes(before, after) if after is not None else None,
              "h18ReceiptSha256": h18_sha, **ctx.receipt_context(),
              "qualifiedCapacityRuns": "separate_nonadditive_receipts_not_reexecuted_by_this_wrapper",
              "interpretation": "No estimated/inherited counts. Only the original five fresh frozen phase receipts plus verified finalize establish full Windows acceptance. No context switch/quarantine between these normal gates."}
    common.write_json(output / "pipeline.json", record)
    print(common.encoded(record).decode("utf-8"), flush=True)
    return record["exitCode"]


if __name__ == "__main__":
    raise SystemExit(main())
