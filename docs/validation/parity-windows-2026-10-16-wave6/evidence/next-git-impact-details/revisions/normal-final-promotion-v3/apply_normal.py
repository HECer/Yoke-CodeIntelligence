"""Root-only application of an explicitly approved, receipt-bound final packet."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path, PurePosixPath
import re
import stat
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[4]
R5 = HERE.parents[1]
EVIDENCE = ROOT / "target/parity-validation-20261015"
OUTPUT = R5 / "normal-applications/normal-final-promotion-v3"
IMAGE_SHA = "00fece6c858b70a0cfe9f9d01919bcf4e6f51722b4ec1d82483ba6a4e2df3cee"
BEFORE_SHA = "afdc36edc71c1396dece79d8cc5ae1d5fddf81100c687a5878e373ac43825bcf"


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def encoded(value):
    return (json.dumps(value, indent=2, sort_keys=True) + "\n").encode()


def guarded(root, name, missing=False):
    relative = PurePosixPath(name)
    require(relative.parts and relative.as_posix() == name and not relative.is_absolute()
            and all(p not in (".", "..") and ":" not in p and "\\" not in p for p in relative.parts), "Unsafe relative path")
    path = root.joinpath(*relative.parts)
    for item in (root, *reversed(path.relative_to(root).parents), relative):
        current = item if item.is_absolute() else root.joinpath(*item.parts)
        try:
            info = current.lstat()
        except FileNotFoundError:
            require(missing, "Missing guarded path: " + str(current))
            continue
        require(not stat.S_ISLNK(info.st_mode)
                and not getattr(info, "st_file_attributes", 0) & stat.FILE_ATTRIBUTE_REPARSE_POINT,
                "Reparse path refused: " + str(current))
    require(path.is_relative_to(root), "Escaped guarded root")
    return path


def read(root, name, maximum=16 * 1024 * 1024):
    path = guarded(root, name)
    before = path.lstat()
    require(stat.S_ISREG(before.st_mode) and before.st_size <= maximum, "Non-file or oversized input")
    with path.open("rb") as stream:
        opened = os.fstat(stream.fileno())
        raw = stream.read(maximum + 1)
        final = os.fstat(stream.fileno())
    after = path.lstat()
    identity = lambda info: (info.st_dev, info.st_ino, info.st_size, info.st_mtime_ns)
    require(identity(before) == identity(opened) == identity(final) == identity(after)
            and len(raw) == before.st_size <= maximum, "Input changed during read")
    return raw


def checked_json(root, name, expected):
    raw = read(root, name)
    require(sha(raw) == expected, "Pinned input hash differs: " + name)
    return json.loads(raw)


def write_new(path, raw):
    with path.open("xb") as stream:
        stream.write(raw)
        stream.flush()
        os.fsync(stream.fileno())


def verify_gates(binding):
    require(binding["status"] == "RELEASED_BY_ROOT" and binding["imageMapSha256"] == IMAGE_SHA,
            "Final image gates remain held")
    require(set(binding["gates"]) == {"public", "private", "legacyMeaningArchiveCli", "format", "strictClippy"}, "Incomplete final gate set")
    pins = json.loads(read(HERE, "SOURCE-PINS.json"))
    for kind, pin in binding["gates"].items():
        receipt = checked_json(ROOT, pin["receipt"], pin["receiptSha256"])
        log = read(ROOT, pin["log"], 128 * 1024 * 1024)
        require(sha(log) == pin["logSha256"] == receipt.get("logSha256", receipt.get("rawSha256")), "Actual raw log differs")
        require(receipt["exitCode"] == 0 and receipt["imageSourceChanges"] == []
                and receipt["validationErrors"] == [] and receipt["sourceFiles"] == 1568
                and receipt["reviewedManifestFileSha256"] == IMAGE_SHA, "Gate failed or source changed")
        helper = "shadow_quality.py" if kind in ("format", "strictClippy") else "shadow_gate.py"
        helper_name = (R5 / helper).relative_to(ROOT).as_posix()
        require(receipt["helperSha256"] == pins[helper_name]["sha256"] == sha(read(ROOT, helper_name)), "Gate helper differs")
        if helper == "shadow_quality.py":
            inventory_name = (R5 / "shadow_gate.py").relative_to(ROOT).as_posix()
            require(receipt["inventoryHelperSha256"] == pins[inventory_name]["sha256"], "Quality inventory helper differs")
        receipt_dir = PurePosixPath(pin["receipt"]).parent
        for name in ("source-before.json", "source-after.json"):
            observed_raw = read(ROOT, (receipt_dir / name).as_posix())
            require(sha(observed_raw) == IMAGE_SHA and json.loads(observed_raw) == json.loads(read(HERE, "FINAL-IMAGE-MAP.json")), "Gate source map differs")
        command = receipt["command"]
        require(command[:1] == ["cargo"] and "--manifest-path" in command
                and Path(command[command.index("--manifest-path") + 1]) == ROOT / "target/r5-shadow-unavailable-v1/Cargo.toml", "Gate ran another image")
        if kind in ("public", "private", "legacyMeaningArchiveCli"):
            require(command[1] == "test" and "--locked" in command and "--workspace" in command, "Unexpected test command")
            groups = re.findall(rb"test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out;", log)
            passed, failed, ignored = [sum(int(group[i]) for group in groups) for i in range(3)]
            require(groups and failed == ignored == 0 and all(int(g[3]) == 0 for g in groups), "Raw test gate did not pass")
            require((passed, failed, ignored, len(groups)) == (receipt["passed"], receipt["failed"], receipt["ignored"], receipt["groups"]), "Receipt disagrees with raw test results")
            split = command.index("--") if "--" in command else len(command)
            tail = command[split + 1:]
            selector = "git_workflows::impact_details::scope_tests"
            expected_tail = [item for item in tail if not re.fullmatch(r"--test-threads=[1-9][0-9]*", item)]
            targets = [command[i + 1] for i, value in enumerate(command[:split]) if value == "--test"]
            if kind == "private":
                require(command[:split].count("--lib") == 1 and "--test" not in command
                        and command[:split].count(selector) == command.count(selector) == 1
                        and targets == [] and expected_tail == []
                        and passed == 4 and len(groups) == 1, "Private selected gate differs")
            else:
                require(expected_tail == [] and all(int(g[4]) == 0 for g in groups), "Public/legacy tests were filtered")
                if kind == "public":
                    require(set(targets) == {"git_impact_details", "git_impact_details_qualification", "git_impact_details_no_git", "git_impact_details_sqlite_limits", "git_impact_details_cli"}
                            and len(targets) == 5 and passed == 49 and len(groups) == 5, "Public final49 gate differs")
                else:
                    require(targets == ["meaning_archive_cli"] and passed > 0 and len(groups) == 1, "Legacy archive CLI gate differs")
        elif kind == "format":
            require(command[1] == "fmt" and "--all" in command and command[-2:] == ["--", "--check"], "Default formatting check differs")
        else:
            require(command[1] == "clippy" and all(value in command for value in ("--workspace", "--all-targets", "--locked"))
                    and command[-3:] == ["--", "-D", "warnings"], "Strict all-target Clippy differs")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--apply", action="store_true", required=True)
    parser.add_argument("--approved-manifest-sha256", required=True)
    parser.add_argument("--root-gate-bindings", required=True, help="Root-relative reviewed gate binding JSON")
    parser.add_argument("--root-gate-bindings-sha256", required=True)
    args = parser.parse_args()
    require(os.name == "nt" and ROOT.name == "Code-Intelligence", "Unexpected checkout or platform")
    manifest = checked_json(HERE, "MANIFEST.json", args.approved_manifest_sha256)
    require(manifest["status"] == "APPLY_HELD" and manifest["actions"] == 16
            and manifest["createOnly"] == 13 and manifest["replaceExact"] == 3, "Unexpected prepared action set")
    for name, pin in manifest["artifacts"].items():
        raw = read(HERE, name)
        require(len(raw) == pin["bytes"] and sha(raw) == pin["sha256"], "Packet artifact closure changed")
    require(sha(read(HERE, "apply_normal.py")) == manifest["artifacts"]["apply_normal.py"]["sha256"], "Applier changed")
    binding = checked_json(ROOT, args.root_gate_bindings, args.root_gate_bindings_sha256)
    require(binding["approvedManifestSha256"] == args.approved_manifest_sha256
            and binding["independentStaticReviewClear"] is True, "Root approval/review is not bound to this manifest")
    verify_gates(binding)
    before_raw, after_raw = read(HERE, "EXPECTED-NORMAL-BEFORE.json"), read(HERE, "EXPECTED-NORMAL-AFTER.json")
    require(sha(before_raw) == BEFORE_SHA and sha(after_raw) == manifest["normalAfterMapSha256"], "Expected map changed")
    before, after = json.loads(before_raw), json.loads(after_raw)
    require(len(before) == 2406 and len(after) == 2419, "Unexpected full source map size")
    pins = json.loads(read(HERE, "SOURCE-PINS.json"))
    runner_name = "target/parity-validation-20261015/acceptance_wave4.py"
    require(sha(read(ROOT, runner_name)) == pins[runner_name]["sha256"] == "b51bbffbddd07be0aa6ef019fc950d772c628d77b3567f646d7a837726aa3d15", "Frozen acceptance runner changed")
    sys.dont_write_bytecode = True
    specification = importlib.util.spec_from_file_location("r5_frozen_acceptance", ROOT / runner_name)
    runner = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(runner)
    def inventory(expected):
        names_raw = subprocess.check_output(["rtk", "proxy", "git", "ls-files", "-c", "-o", "--exclude-standard", "-z"], cwd=ROOT)
        require(len(names_raw) <= 8 * 1024 * 1024, "Git inventory byte limit")
        names = [name for name in names_raw.decode("utf-8").split("\0") if name]
        require(len(names) == len(set(names)) == len(expected) <= 10_000 and set(names) == set(expected), "Git source names differ before guarded hashing")
        total = 0
        for name in names:
            path = guarded(ROOT, name)
            info = path.lstat()
            require(stat.S_ISREG(info.st_mode) and info.st_size <= 16 * 1024 * 1024, "Non-plain or oversized normal source")
            total += info.st_size
            require(total <= 512 * 1024 * 1024, "Normal source total byte limit")
        observed = runner.source_manifest()
        require(observed == expected, "Complete frozen source_manifest differs")
        return observed
    rows = json.loads(read(HERE, "COPY-MAPPINGS.json"))
    require(len(rows) == 16 and len({r["destination"] for r in rows}) == 16, "Duplicate or missing actions")
    expected, payloads = dict(before), {}
    for row in rows:
        name = row["destination"]
        require(name.startswith("native/"), "Unexpected action destination")
        target = guarded(ROOT, name, missing=row["operation"] == "create_only")
        raw = read(HERE, row["payload"])
        require(sha(raw) == row["afterSha256"] and len(raw) == row["afterBytes"], "Payload changed")
        if row["operation"] == "create_only":
            require(name not in before and not os.path.lexists(target), "Create target already exists")
        else:
            require(row["operation"] == "replace_exact" and before[name] == row["beforeSha256"]
                    and read(ROOT, name) == read(HERE, row["before"]), "Replacement preimage differs")
        payloads[name] = raw
        expected[name] = sha(raw)
    require(expected == after, "Actions do not produce the exact after map")
    require(inventory(before) == before, "Fresh complete normal baseline differs; no writes allowed")
    guarded(ROOT, OUTPUT.parent.relative_to(ROOT).as_posix(), missing=True)
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    guarded(R5, "normal-applications")
    OUTPUT.mkdir(exist_ok=False)
    write_new(OUTPUT / "normal-before.json", before_raw)
    for row in rows:
        if row["operation"] == "replace_exact":
            write_new(OUTPUT / (row["destination"].replace("/", "--") + ".before.bin"), read(ROOT, row["destination"]))
    report = dict(complete=False, manifestSha256=args.approved_manifest_sha256,
        gateBindingsSha256=args.root_gate_bindings_sha256, appliedTargets=[], normalWrites=0,
        inFlight=None, multiFileAtomic=False, automaticRollback=False, arbitraryDeletes=False)
    def journal():
        with (OUTPUT / "journal.json").open("wb") as stream:
            stream.write(encoded(report))
            stream.flush()
            os.fsync(stream.fileno())
    journal()
    try:
        for row in rows:
            name = row["destination"]
            target = guarded(ROOT, name, missing=row["operation"] == "create_only")
            require(target.parent.is_dir(), "Destination parent missing")
            raw = payloads[name]
            report["inFlight"] = dict(target=name, operation=row["operation"], writeAttempted=False,
                                       writeReturned=False, verified=False, temporary=None)
            journal()
            if row["operation"] == "create_only":
                require(not os.path.lexists(target), "Create target appeared")
                report["inFlight"]["writeAttempted"] = True
                journal()
                write_new(target, raw)
            else:
                require(sha(read(ROOT, name)) == row["beforeSha256"], "Replacement changed immediately before write")
                descriptor, temporary_name = tempfile.mkstemp(prefix=".r5-promotion-", dir=target.parent)
                temporary = Path(temporary_name)
                report["inFlight"]["temporary"] = temporary.relative_to(ROOT).as_posix()
                journal()
                with os.fdopen(descriptor, "wb") as stream:
                    stream.write(raw)
                    stream.flush()
                    os.fsync(stream.fileno())
                require(sha(read(ROOT, name)) == row["beforeSha256"] and temporary.read_bytes() == raw, "Replacement race or temporary mismatch")
                report["inFlight"]["writeAttempted"] = True
                journal()
                os.replace(temporary, target)
            report["inFlight"]["writeReturned"] = True
            journal()
            require(read(ROOT, name) == raw, "Written target verification failed")
            report["inFlight"]["verified"] = True
            report["appliedTargets"].append(name)
            report["normalWrites"] += 1
            report["inFlight"] = None
            journal()
        observed = inventory(after)
        write_new(OUTPUT / "normal-after.json", encoded(observed))
        require(observed == after, "Complete normal postflight differs")
        report.update(complete=True, normalFiles=2419, afterMapSha256=sha(after_raw))
    except BaseException as error:
        report.update(errorType=type(error).__name__, error=str(error)[:1000])
        raise
    finally:
        journal()
    print(json.dumps(report))


if __name__ == "__main__":
    main()
