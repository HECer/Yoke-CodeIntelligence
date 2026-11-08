"""Pure causal inventory-command probes; never execute Cargo or runner main."""
import argparse
import ast
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import sys

HERE = Path(__file__).resolve().parent


def sha(data):
    return hashlib.sha256(data).hexdigest()


def module_at(name, path):
    ast.parse(path.read_bytes(), filename=str(path))
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)

    def forbidden(*args, **kwargs):
        raise RuntimeError("Subprocesses are forbidden in the pure inventory oracle")

    module.subprocess.run = forbidden
    module.subprocess.check_output = forbidden
    return module


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", required=True)
    args = parser.parse_args()
    output = Path(args.output).resolve()
    if output.parent != HERE or output.exists():
        raise RuntimeError("Oracle output must be a fresh sibling evidence file")
    sys.dont_write_bytecode = True
    old_dir = HERE / "wave3-frozen-20261015-1734"
    paths = [HERE / "acceptance_wave3.py", HERE / "preserve_wave3.py", old_dir / "runner.py",
             old_dir / "native/library-inventory-all.log", old_dir / "native/library-inventory-ignored.log",
             old_dir / "native/receipt.json", HERE / "wave3-inventory-default-all/run.log",
             HERE / "wave3-inventory-default-all/receipt.json", HERE / "wave3-inventory-default-ignored/run.log",
             HERE / "wave3-inventory-default-ignored/receipt.json", HERE / "acceptance.py"]
    original = {path: path.read_bytes() for path in paths}
    old = module_at("wave3_inventory_old_parser_only", old_dir / "runner.py")
    new = module_at("wave3_inventory_new_parser_only", HERE / "acceptance_wave3.py")
    preserver = module_at("wave3_inventory_preserver_only", HERE / "preserve_wave3.py")
    terse_all, terse_ignored = (original[old_dir / ("native/library-inventory-" + kind + ".log")] for kind in ("all", "ignored"))
    default_all, default_ignored = (original[HERE / ("wave3-inventory-default-" + kind + "/run.log")] for kind in ("all", "ignored"))
    cases, traces = [], {}

    def case(name, callback, reject=False):
        try:
            detail = callback()
        except Exception as error:
            cases.append(dict(name=name, accepted=reject, expectedRejection=reject,
                              error=type(error).__name__ + ": " + str(error)))
        else:
            cases.append(dict(name=name, accepted=not reject, expectedRejection=reject, detail=detail))

    def inventory(module, label, all_data, ignored_data, default_required):
        trace, saved = [], []
        instance = module.PhaseRun(HERE, {}, {}, HERE)
        expected = {"library-inventory-all": module.BASE + ["--lib", "--", "--list"],
                    "library-inventory-ignored": module.BASE + ["--lib", "--", "--ignored", "--list"]}

        def fixture_run(actual_label, command, *args, **kwargs):
            trace.append(dict(label=actual_label, command=command))
            if default_required:
                module.require(command == expected[actual_label], "Inventory did not use the exact default-list command")
            return dict(exitCode=0), all_data if actual_label.endswith("-all") else ignored_data

        instance.run = fixture_run
        module.write_json = lambda path, value: saved.append(value)
        traces[label] = trace
        records = instance.inventories("library-inventory", ["--lib"], ["yoke"])
        module.require(len(saved) == 1 and saved[0] == records, "Pure inventory output differs")
        return records["yoke"]

    def checked_default_inventory():
        record = inventory(new, "new-default", default_all, default_ignored, True)
        new.require(len(record["registered"]) == 149 and len(record["ignored"]) == 1, "Actual default inventory differs")
        new.require(new.IGNORED_WORKER in record["ignored"] and set(new.ACL_TESTS).issubset(record["registered"]), "Worker/ACL name drift")
        return dict(registered=149, ignored=1, ignoredNames=record["ignored"])

    case("actual-terse-all-rejects-missing-summary", lambda: new.harness_groups(terse_all, True), True)
    case("actual-terse-ignored-rejects-missing-summary", lambda: new.harness_groups(terse_ignored, True), True)
    case("old-inventory-call-actual-terse-fails", lambda: inventory(old, "old-terse", terse_all, terse_ignored, False), True)
    case("new-exact-default-calls-149-ignored1", checked_default_inventory)

    def identical_rows():
        for terse, default in ((terse_all, default_all), (terse_ignored, default_ignored)):
            rows = lambda data: re.findall(rb"^(.+): test\r?$", data, re.MULTILINE)
            new.require(rows(terse) == rows(default), "Default and terse rows differ")
        return dict(allRows=149, ignoredRows=1)

    case("actual-terse-and-default-rows-identical", identical_rows)
    truncated = re.sub(rb"^149 tests, 0 benchmarks\r?\n?", b"", default_all, flags=re.MULTILINE)
    case("default-missing-summary-still-rejected", lambda: inventory(new, "missing-summary", truncated, default_ignored, True), True)
    case("default-count-drift-rejected", lambda: inventory(new, "count-drift", default_all.replace(b"149 tests,", b"150 tests,", 1), default_ignored, True), True)
    first = next(line for line in default_all.splitlines() if line.endswith(b": test"))
    case("default-duplicate-row-rejected", lambda: inventory(new, "duplicate-row", default_all.replace(first, first + b"\n" + first, 1), default_ignored, True), True)
    foreign = default_ignored.replace(new.IGNORED_WORKER.encode(), b"foreign::not_registered", 1)
    case("default-unregistered-ignore-rejected", lambda: inventory(new, "foreign-ignore", default_all, foreign, True), True)

    def original_failure_truth():
        receipt = json.loads(original[old_dir / "native/receipt.json"])
        new.require(receipt["accepted"] is False and receipt["passed"] == receipt["failed"] == receipt["ignored"] == 0,
                    "Original parser failure was relabeled as executed tests")
        new.require(all(row["exitCode"] == 0 for row in receipt["results"]) and
                    receipt["validationErrors"] == ["RuntimeError: Unterminated Cargo harness output"], "Original failure cause differs")
        return dict(executedTests=0, productFailure=False, validationErrors=receipt["validationErrors"])

    case("original1734-parser-failure-remains-zero-executed", original_failure_truth)

    def inverse_only_commands():
        raw = original[HERE / "acceptance_wave3.py"]
        text = raw.decode("utf-8")
        pairs = (("[\"--\", \"--list\"]", "[\"--\", \"--list\", \"--format\", \"terse\"]"),
                 ("[\"--\", \"--ignored\", \"--list\"]", "[\"--\", \"--ignored\", \"--list\", \"--format\", \"terse\"]"))
        for before, after in pairs:
            new.require(text.count(before) == 1, "Unexpected inverse command match count")
            text = text.replace(before, after, 1)
        new.require(text.encode("utf-8") == original[old_dir / "runner.py"], "Runner changed beyond two inventory flags")
        return dict(inverseSha256=sha(text.encode("utf-8")), exactHistoricalBytes=True)

    case("runner-inverse-only-two-command-flags", inverse_only_commands)

    def preserver_roles():
        failed = preserver.verify_failed_attempt("wave3-frozen-20261015-1734")
        native = failed["receipts"]["native"]
        new.require(not native["accepted"] and native["validationErrors"] and native["passed"] == native["failed"] == native["ignored"] == 0,
                    "Preserver lost parser-failure truth")
        new.require(preserver.RUNNER_SHA256 == sha(original[HERE / "acceptance_wave3.py"]) and
                    preserver.HISTORICAL_RUNNER_SHA256 == sha(original[old_dir / "runner.py"]) and
                    preserver.RUNNER_SHA256 != preserver.HISTORICAL_RUNNER_SHA256, "Preserver hash roles differ")
        return dict(historicalRunner=preserver.HISTORICAL_RUNNER_SHA256, acceptedRunner=preserver.RUNNER_SHA256,
                    availablePhases=failed["availablePhases"], missingPhases=failed["missingPhases"], missingFinalize=failed["missingFinalize"])

    case("preserver-keeps-old-runner-and-zero-test-failure-history", preserver_roles)
    unchanged = all(path.read_bytes() == data for path, data in original.items())
    report = dict(runnerSha256=sha(original[HERE / "acceptance_wave3.py"]), preserverSha256=sha(original[HERE / "preserve_wave3.py"]),
                  oracleSha256=sha(Path(__file__).read_bytes()), astParsed=True,
                  runnerMainExecuted=False, preserverMainExecuted=False, subprocessesExecuted=0, suitesExecuted=0, copiesExecuted=0,
                  immutableInputsUnchanged=unchanged, cases=cases, commandTraces=traces,
                  passed=sum(item["accepted"] for item in cases), failed=sum(not item["accepted"] for item in cases),
                  inputSha256={str(path.relative_to(HERE)): sha(data) for path, data in original.items()})
    output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({key: report[key] for key in ("runnerSha256", "preserverSha256", "oracleSha256", "passed", "failed",
                                                 "immutableInputsUnchanged", "runnerMainExecuted", "preserverMainExecuted", "subprocessesExecuted")}))
    for item in cases:
        if not item["accepted"]:
            print(json.dumps(item))
    return 0 if unchanged and report["failed"] == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
