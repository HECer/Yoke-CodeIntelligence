"""Preserve verified wave3 evidence into a new immutable bundle, without retries.

Only a finalized successful five-phase --accepted-tag is admissible. Earlier
failed shared freezes and all immediate wave3 focused receipts remain separate;
their overlapping counts never contribute to accepted whole-matrix totals.
"""
import argparse
import copy
import datetime
import hashlib
import importlib.util
import json
import os
from pathlib import Path, PurePosixPath
import re
import stat
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
EVIDENCE = Path(__file__).resolve().parent
DESTINATION = ROOT / "docs/validation/parity-windows-2026-10-15-wave3"
PHASES = ("format", "clippy", "native", "python", "typescript")
RUNNER_SHA256 = "0d7ed0fa533d55c6518da3dfdc322d6e9e227361f1e88070edbaf2b7067ba45e"
HISTORICAL_RUNNER_SHA256 = "fec205667564b2aa82bcc40f17b2c8378b9ffe8ce4ea90a077045b6837e2ec9f"
KNOWN_RUNNER_SHA256 = frozenset((RUNNER_SHA256, HISTORICAL_RUNNER_SHA256))
EXPECTED_SOURCE_FILES = 803
FAILED_TAG = "wave3-frozen-20261015-1720"
FAILED_SOURCE_SHA256 = "2cde7792d669cc3067ea869a6882cc00bb41d1d72feb4a04a9f214ec1b53ce50"
PRIOR = (
    ("docs/validation/parity-windows-2026-10-15/index.json", 160,
     "156852befe63b51cd99d66f9b9a857157659715f31ffb0a653720373677d91a0"),
    ("docs/validation/parity-windows-2026-10-15-wave2/index.json", 210,
     "0e2e504fb30ec388ae46efe53dd4639de55926753ffc8b9b5da74fc55c71633e"),
)
FOCUS_FILES = ("receipt.json", "run.log", "source-before.json", "source-after.json")
TOP_FILES = ("freeze.json", "source-freeze.json", "runner.py")
TEST_SUMMARY = re.compile(rb"test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;")


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def encoded(value):
    return (json.dumps(value, indent=2, ensure_ascii=False, sort_keys=True) + "\n").encode("utf-8")


def relative_path(raw):
    require(isinstance(raw, str) and "\\" not in raw and ":" not in raw, "Invalid portable relative path")
    path = PurePosixPath(raw)
    require(raw and not path.is_absolute() and all(part not in ("", ".", "..") for part in path.parts), "Unsafe relative path")
    require(path.as_posix() == raw, "Noncanonical relative path")
    return Path(*path.parts)


def plain_path(path, boundary, file=False, missing=False):
    """Reject reparse components, escapes and non-regular evidence leaves."""
    require(path.is_absolute() and path.is_relative_to(boundary) and path != boundary, "Evidence path escaped its boundary")
    require(boundary.resolve(strict=True) == boundary, "Evidence boundary resolves elsewhere")
    current = boundary
    for part in path.relative_to(boundary).parts:
        current = current / part
        if not current.exists() and not current.is_symlink():
            require(missing, "Missing evidence path: " + str(current))
            continue
        info = current.lstat()
        require(not info.st_file_attributes & stat.FILE_ATTRIBUTE_REPARSE_POINT, "Reparse evidence path refused")
        require(current.resolve(strict=True) == current, "Resolved evidence path differs")
        if current != path:
            require(stat.S_ISDIR(info.st_mode), "Evidence ancestor is not a directory")
        elif file:
            require(stat.S_ISREG(info.st_mode), "Evidence leaf is not a regular file")
        else:
            require(stat.S_ISDIR(info.st_mode), "Evidence path is not a directory")
    return path


def read(path, boundary=ROOT):
    return plain_path(path, boundary, file=True).read_bytes()


def load(path, boundary=ROOT):
    return json.loads(read(path, boundary).decode("utf-8"))


def tag_path(tag):
    require(re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_.-]{0,100}", tag) is not None, "Unsafe evidence tag")
    return plain_path(EVIDENCE / tag, EVIDENCE)


def source_manifest():
    names = subprocess.check_output(["rtk", "proxy", "git", "ls-files", "-c", "-o", "--exclude-standard", "-z"], cwd=ROOT)
    return {name: sha(read(ROOT / relative_path(name)))
            for name in sorted(set(names.decode("utf-8").split("\0")))
            if name and (ROOT / relative_path(name)).is_file()}


def verify_prior_pins():
    verified = []
    for relative, count, expected_index in PRIOR:
        index_path = ROOT / relative_path(relative)
        raw = read(index_path)
        require(sha(raw) == expected_index, "Historical index changed: " + relative)
        pins = json.loads(raw)["sha256"]
        require(len(pins) == count, "Historical pin count changed")
        prefix = index_path.parent.relative_to(ROOT).as_posix() + "/"
        for name, expected in pins.items():
            require(name.startswith(prefix) and sha(read(ROOT / relative_path(name))) == expected, "Historical evidence changed: " + name)
        verified.append(dict(index=relative, indexSha256=expected_index, verifiedPins=count))
    return verified


def approved_runner(bundle):
    raw = read(bundle / "runner.py")
    expected = load(bundle / "freeze.json")["runnerSha256"]
    require(expected in KNOWN_RUNNER_SHA256 and sha(raw) == expected, "Unreviewed frozen acceptance runner")
    # Import only the independently reviewed, hash-pinned definitions. The
    # __main__ guard remains false and bytecode writes are disabled.
    prior = sys.dont_write_bytecode
    sys.dont_write_bytecode = True
    try:
        spec = importlib.util.spec_from_file_location("wave3_preservation_parser_only", bundle / "runner.py")
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
    finally:
        sys.dont_write_bytecode = prior
    return module


def freeze_bundle(bundle):
    freeze = load(bundle / "freeze.json")
    manifest = load(bundle / "source-freeze.json")
    require(freeze["phases"] == list(PHASES) and freeze["sourceFiles"] == len(manifest) == EXPECTED_SOURCE_FILES,
            "Unexpected phase or 803-file source freeze")
    require(freeze["runnerSha256"] in KNOWN_RUNNER_SHA256 and sha(read(bundle / "runner.py")) == freeze["runnerSha256"], "Runner pin differs")
    require(sha(encoded(manifest)) == freeze["sourceManifestSha256"] == sha(read(bundle / "source-freeze.json")), "Source freeze hash differs")
    for name, digest in manifest.items():
        relative_path(name)
        require(re.fullmatch(r"[0-9a-f]{64}", digest) is not None, "Malformed source digest")
    return freeze, manifest


def phase_receipt(bundle, phase, freeze, manifest, accepted):
    directory = plain_path(bundle / phase, bundle)
    receipt = load(directory / "receipt.json")
    require(isinstance(accepted, bool) and receipt["phase"] == phase and receipt["phases"] == list(PHASES), "Phase identity differs")
    for key in ("sourceManifestSha256", "runnerSha256", "sourceFiles"):
        require(receipt[key] == freeze[key], "Phase freeze differs")
    before, after = load(directory / "source-before.json"), load(directory / "source-after.json")
    require(before == manifest, "Phase input source manifest differs from its freeze")
    changes = sorted(name for name in before.keys() | after.keys() if before.get(name) != after.get(name))
    require(changes == receipt["sourceChanges"] and receipt["accepted"] is accepted, "Phase change/acceptance record differs")
    if accepted:
        require(after == manifest and not receipt["validationErrors"], "Accepted phase has source changes or validation errors")
    require(receipt["results"], "Empty phase result list")
    names = [result["label"] for result in receipt["results"]]
    require(len(names) == len(set(names)), "Duplicate phase result labels")
    artifacts = receipt["artifactSha256"]
    required_artifacts = {"source-before.json", "source-after.json", *(label + ".log" for label in names)}
    known_artifacts = set(required_artifacts)
    if phase == "native":
        known_artifacts.update(("target-inventory.json", "cleaned-before.json"))
        known_artifacts.update(label[:-4] + ".json" for label in names if label.endswith("-inventory-all"))
        known_artifacts.update(label + "-cleaned.json" for label in names if re.fullmatch(r"integration-[0-9]+", label))
    require(required_artifacts.issubset(artifacts) and set(artifacts).issubset(known_artifacts), "Missing or unknown phase evidence files")
    if accepted:
        require(set(artifacts) == known_artifacts, "Accepted phase has an incomplete evidence file set")
    for name, expected in artifacts.items():
        rel = relative_path(name)
        require(len(rel.parts) == 1 and rel.suffix in (".log", ".json"), "Unexpected phase artifact kind/path")
        require(sha(read(directory / rel)) == expected, "Phase artifact hash differs: " + name)
    for result in receipt["results"]:
        require(re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9-]*", result["label"]) is not None, "Unsafe result label")
        require(result["origin"] == "fresh_execution" and result["sourceManifestSha256"] == freeze["sourceManifestSha256"] and result["runnerSha256"] == freeze["runnerSha256"],
                "Retained result or mismatched result freeze")
        require(sha(read(directory / (result["label"] + ".log"))) == result["logSha256"], "Raw result log differs")
        if accepted:
            require(result["exitCode"] == 0 and result["failed"] == 0, "Failed result in accepted phase")
    require(all(receipt[key] == sum(result[key] for result in receipt["results"]) for key in ("groups", "passed", "failed", "ignored")), "Phase result totals differ")
    return receipt


def replay_static(module, directory, phase, receipt):
    results = {result["label"]: result for result in receipt["results"]}
    consumed = []
    parsed = []
    instance = module.PhaseRun(directory, {}, {}, directory)

    def fixture_run(label, command, *args, **kwargs):
        require(label in results and results[label]["command"] == command, "Static command differs from reviewed runner")
        require(results[label]["inventoryOnly"] is False, "Static result incorrectly marked as inventory")
        consumed.append(label)
        checked = copy.deepcopy(results[label])
        parsed.append(checked)
        return checked, read(directory / (label + ".log"))

    instance.run = fixture_run
    instance.static(phase)
    require(set(consumed) == set(results) and len(consumed) == len(results), "Static result coverage differs")
    require(all(checked[key] == results[checked["label"]][key]
                for checked in parsed for key in ("groups", "passed", "failed", "ignored")), "Static parsed totals differ from receipts")


def replay_native(module, directory, receipt):
    results = {result["label"]: result for result in receipt["results"]}
    used = set()

    def result(label, command, inventory=False):
        require(label in results and label not in used, "Missing or repeated native result: " + label)
        row = results[label]
        require(row["command"] == command and row["inventoryOnly"] is inventory, "Native command or inventory role differs")
        if inventory:
            require(all(row[key] == 0 for key in ("groups", "passed", "failed", "ignored")), "Inventory result contributes invented test counts")
        used.add(label)
        return copy.deepcopy(row), read(directory / (label + ".log"))

    _, metadata_raw = result("cargo-metadata", ["cargo", "metadata", "--no-deps", "--locked", "--format-version", "1"], True)
    metadata = json.loads(metadata_raw)
    require(Path(metadata["target_directory"]).resolve() == ROOT / "target", "Unexpected Cargo target directory")
    packages = [package for package in metadata["packages"] if package["id"] in metadata["workspace_members"]]
    require(len(packages) == 1 and packages[0]["name"] == "yoke-code-intelligence", "Workspace package drift")
    targets = load(directory / "target-inventory.json")
    require(targets == packages[0]["targets"], "Metadata target inventory differs")
    by_kind = {kind: sorted(target["name"] for target in targets if target["kind"] == [kind])
               for kind in ("lib", "bin", "test", "example", "bench")}
    require(all(target["kind"] in [[kind] for kind in by_kind] for target in targets), "Unsupported Cargo target kind")
    require(len(by_kind["lib"]) == 1 and by_kind["test"] and all(len(names) == len(set(names)) for names in by_kind.values()), "Missing/duplicate Cargo targets")

    def inventory(prefix, selectors, names, empty=False):
        _, all_data = result(prefix + "-all", module.BASE + selectors + ["--", "--list"], True)
        _, ignored_data = result(prefix + "-ignored", module.BASE + selectors + ["--", "--ignored", "--list"], True)
        all_groups = module.group_map(module.harness_groups(all_data, True), names)
        ignored_groups = module.group_map(module.harness_groups(ignored_data, True), names)
        records = {}
        for name in names:
            registered, ignored = sorted(all_groups[name]["rows"]), sorted(ignored_groups[name]["rows"])
            require((empty or registered) and set(ignored).issubset(registered), "Invalid registered/ignored native inventory")
            require(all_groups[name]["executable"] == ignored_groups[name]["executable"], "Inventory artifact drift")
            records[name] = dict(registered=registered, ignored=ignored, executable=all_groups[name]["executable"])
        require(load(directory / (prefix + ".json")) == records, "Saved test inventory differs from raw logs")
        return records

    def execute(label, selectors, records, selected=None, extra=(), cached=False):
        instance = module.PhaseRun(directory, {}, {}, directory)

        def fixture_run(actual_label, command, *args, **kwargs):
            return result(actual_label, command)

        instance.run = fixture_run
        checked, groups = instance.execute(label, selectors, records, selected=selected, extra=extra, cached=cached)
        saved = results[label]
        require(all(checked[key] == saved[key] for key in ("groups", "passed", "failed", "ignored", "testGroups")), "Native result rows/counts differ from raw logs")
        return groups

    library = inventory("library-inventory", ["--lib"], by_kind["lib"])
    lib_name, record = next(iter(library.items()))
    registered, ignored = set(record["registered"]), set(record["ignored"])
    require(set(module.ACL_TESTS).issubset(registered) and not set(module.ACL_TESTS).intersection(ignored), "ACL inventory drift")
    require(module.IGNORED_WORKER in ignored, "Owned worker was not registered and ignored")
    require({name for name in registered if any(skip in name for skip in module.ACL_TESTS)} == set(module.ACL_TESTS), "Ambiguous library skip")
    ordinary = execute("library-d", ["--lib"], library, {lib_name: registered.difference(module.ACL_TESTS)},
                       extra=[item for name in module.ACL_TESTS for item in ("--skip", name)])
    require(Path(results["library-d"]["tempRoot"]).drive.upper() == "D:", "Ordinary library fixtures did not use D")
    partitions = [dict(label="library-d", names=sorted(ordinary[lib_name]["rows"]))]
    for index, name in enumerate(module.ACL_TESTS):
        label = "library-c-acl-" + str(index)
        groups = execute(label, ["--lib", name], library, {lib_name: {name}}, extra=["--exact"], cached=True)
        require(Path(results[label]["tempRoot"]).drive.upper() == "C:" and results[label]["passed"] == 1 and results[label]["ignored"] == 0,
                "Exact ACL fixture did not pass once on C")
        partitions.append(dict(label=label, names=sorted(groups[lib_name]["rows"])))
    covered = [name for partition in partitions for name in partition["names"]]
    require(len(covered) == len(set(covered)) == len(registered) and set(covered) == registered, "Library partitions overlap or omit names")
    expected_coverage = dict(libraryRegistered=len(registered), libraryIgnored=len(ignored), libraryIgnoredNames=sorted(ignored),
                             ignoredWorker=module.IGNORED_WORKER, libraryPartitions=partitions, libraryDisjointCoverageAccepted=True)
    for kind, selector in (("bin", "--bins"), ("example", "--examples"), ("bench", "--benches")):
        if by_kind[kind]:
            execute(kind + "-tests", [selector], inventory(kind + "-inventory", [selector], by_kind[kind], True))
        else:
            expected_coverage[kind + "Targets"] = []
    docs, doc_data = result("doctests", module.BASE + ["--doc", "--", "--test-threads=4"])
    doc_groups = module.harness_groups(re.sub(rb"^\s*Doc-tests (\S+)\r?$", rb" Running doctests (\1.exe)", doc_data, flags=re.MULTILINE), False)
    require(len(doc_groups) == 1 and docs["testGroups"] == doc_groups and docs["groups"] == 1, "Doctest result differs")
    require(all(docs[key] == sum(group[key] for group in doc_groups) for key in ("passed", "failed", "ignored")), "Doctest totals differ")
    selected = []
    for offset in range(0, len(by_kind["test"]), 6):
        names = by_kind["test"][offset:offset + 6]
        selectors = [item for name in names for item in ("--test", name)]
        label = "integration-" + str(offset // 6).zfill(2)
        execute(label, selectors, inventory(label + "-inventory", selectors, names))
        selected.extend(names)
    expected_coverage.update(integrationTargets=len(selected), integrationTargetNames=selected,
                             integrationCoverageAccepted=True, targetNamesByKind=by_kind)
    require(selected == by_kind["test"] and len(selected) == len(set(selected)), "Integration target coverage differs")
    require(receipt["matrixCoverage"] == expected_coverage and used == set(results), "Native coverage or result-label inventory differs")
    return dict(cargoTargets=len(targets), integrationTargets=len(selected), libraryRegistered=len(registered),
                libraryIgnored=len(ignored), libraryIgnoredNames=sorted(ignored), exactAclNames=list(module.ACL_TESTS),
                countedOnlyFreshExecution=True, disjointLibraryCoverage=True)


def verify_accepted(tag):
    require(tag != FAILED_TAG, "The failed 1720 attempt cannot be accepted")
    bundle = tag_path(tag)
    freeze, manifest = freeze_bundle(bundle)
    require(freeze["runnerSha256"] == RUNNER_SHA256, "Accepted bundle must use the newly reviewed default-list runner")
    summary = load(bundle / "summary.json")
    require(summary["accepted"] is True and all(summary[key] == freeze[key] for key in freeze), "Missing/unaccepted final shared summary")
    require(set(summary["phaseReceipts"]) == set(PHASES), "Summary does not cover all five phases")
    require(summary["freezeArtifactSha256"] == {name: sha(read(bundle / name)) for name in TOP_FILES}, "Summary freeze pins differ")
    module = approved_runner(bundle)
    receipts = {}
    for phase in PHASES:
        receipt = phase_receipt(bundle, phase, freeze, manifest, True)
        require(Path(receipt["tempRoot"]).drive.upper() == "D:", "Ordinary phase fixtures did not use D")
        if phase == "native":
            require(Path(receipt["aclTempRoot"]).drive.upper() == "C:", "Native ACL fixture root did not use C")
        else:
            require(receipt["aclTempRoot"] is None, "Non-native phase has an ACL root")
        for result in receipt["results"]:
            is_acl = phase == "native" and result["label"] in {"library-c-acl-" + str(index) for index in range(3)}
            require(result["tempRoot"] == receipt["aclTempRoot" if is_acl else "tempRoot"], "Result fixture root differs from explicit phase matrix")
        expected_summary = dict(receiptSha256=sha(read(bundle / phase / "receipt.json")),
                                **{key: receipt[key] for key in ("passed", "failed", "ignored", "groups")})
        require(summary["phaseReceipts"][phase] == expected_summary, "Summary phase hash/totals differ")
        if phase != "native":
            replay_static(module, bundle / phase, phase, receipt)
        receipts[phase] = receipt
    matrix = replay_native(module, bundle / "native", receipts["native"])
    return dict(bundle=bundle, freeze=freeze, manifest=manifest, receipts=receipts, nativeMatrix=matrix)


def verify_failed_attempt(tag):
    bundle = tag_path(tag)
    freeze, manifest = freeze_bundle(bundle)
    require(not (bundle / "summary.json").exists(), "Failed/incomplete attempt unexpectedly has a final summary")
    available = [phase for phase in PHASES if (bundle / phase / "receipt.json").is_file()]
    receipts = {phase: phase_receipt(bundle, phase, freeze, manifest, load(bundle / phase / "receipt.json")["accepted"])
                for phase in available}
    require(receipts and any(not receipt["accepted"] for receipt in receipts.values()), "Bundle has no failed receipt")
    diagnostics = []
    if "clippy" in receipts:
        diagnostics = re.findall(rb"^error: (?!could not compile)(.+)\r?$", read(bundle / "clippy/clippy.log"), re.MULTILINE)
    if tag == FAILED_TAG:
        require(freeze["sourceManifestSha256"] == FAILED_SOURCE_SHA256 and set(available) == {"format", "clippy", "python", "typescript"},
                "Original 1720 failed freeze or phase inventory differs")
        require(len(diagnostics) == 4 and receipts["clippy"]["results"][0]["exitCode"] == 101, "Original mechanical Clippy failure differs")
    classification = "failed_incomplete_clippy_attempt" if "clippy" in receipts and not receipts["clippy"]["accepted"] else "failed_incomplete_frozen_attempt"
    return dict(bundle=bundle, freeze=freeze, receipts=receipts, tag=tag,
                classification=classification, availablePhases=list(receipts),
                missingPhases=[phase for phase in PHASES if phase not in receipts], missingFinalize=True, acceptedFullRun=False,
                phaseOutcomes={phase: {key: receipt[key] for key in ("accepted", "groups", "passed", "failed", "ignored", "sourceChanges", "validationErrors")}
                               | {"commandExitCodes": [result["exitCode"] for result in receipt["results"]]}
                               for phase, receipt in receipts.items()},
                clippyDiagnosticCount=len(diagnostics), diagnosticMessages=[line.decode("utf-8").rstrip("\r") for line in diagnostics])


def failed_attempts(accepted_tag):
    failed = []
    for directory in sorted(EVIDENCE.iterdir()):
        if not directory.is_dir() or not directory.name.startswith("wave3-frozen-") or directory.name == accepted_tag:
            continue
        if any((directory / phase / "receipt.json").is_file() and
               load(directory / phase / "receipt.json").get("accepted") is False for phase in PHASES):
            failed.append(verify_failed_attempt(directory.name))
    require(any(item["tag"] == FAILED_TAG for item in failed), "Original 1720 failed attempt was not preserved")
    return failed


def focused_evidence():
    focused = []
    for directory in sorted(EVIDENCE.iterdir()):
        if not directory.name.startswith("wave3-") or not directory.is_dir() or not (directory / "receipt.json").is_file():
            continue
        plain_path(directory, EVIDENCE)
        receipt = load(directory / "receipt.json")
        require(receipt["scope"] == "focused_gate_not_frozen_suite", "Unexpected immediate wave3 receipt scope")
        for name in FOCUS_FILES:
            read(directory / name)
        before, after = load(directory / "source-before.json"), load(directory / "source-after.json")
        changes = sorted(name for name in before.keys() | after.keys() if before.get(name) != after.get(name))
        require(changes == receipt["sourceChanges"], "Focused source changes differ")
        data = read(directory / "run.log")
        groups = TEST_SUMMARY.findall(data)
        require(sha(data) == receipt["logSha256"] and receipt["groups"] == len(groups), "Focused raw log/hash/groups differ")
        require(all(receipt[key] == sum(int(group[index]) for group in groups) for index, key in enumerate(("passed", "failed", "ignored"))), "Focused raw summary totals differ")
        focused.append(dict(tag=directory.name, classification="historical_focused_gate",
                            countsInterpretation="overlapping_counts_not_additive", acceptedWholeMatrix=False,
                            **{key: receipt[key] for key in ("scope", "command", "exitCode", "groups", "passed", "failed", "ignored", "logSha256", "sourceChanges")},
                            receiptSha256=sha(read(directory / "receipt.json")), sourceBeforeSha256=sha(read(directory / "source-before.json")),
                            sourceAfterSha256=sha(read(directory / "source-after.json"))))
    require(focused, "No focused wave3 receipts found")
    return focused


def copy_plan(accepted, failed, focused):
    plan = []

    def add(source, target):
        relative = relative_path(target)
        require(relative.suffix in (".json", ".log", ".py"), "Non-evidence copy kind")
        data = read(source)
        plan.append(dict(source=source, target=relative, sha256=sha(data), bytes=len(data)))

    for category, item in [("accepted", accepted), *(("failed-attempts", item) for item in failed)]:
        bundle = item["bundle"]
        prefix = category + "/" + bundle.name + "/"
        for name in TOP_FILES + (("summary.json",) if category == "accepted" else ()):
            add(bundle / name, prefix + name)
        for phase, receipt in item["receipts"].items():
            for name in ("receipt.json", *sorted(receipt["artifactSha256"])):
                add(bundle / phase / name, prefix + phase + "/" + name)
    for item in focused:
        for name in FOCUS_FILES:
            add(EVIDENCE / item["tag"] / name, "focused/" + item["tag"] + "/" + name)
    extras = {
        "acceptance_wave3.py": "tools/acceptance_wave3.py",
        "acceptance_wave3_parser_oracle.py": "tools/acceptance_wave3_parser_oracle.py",
        "acceptance_wave3_inventory_oracle.py": "tools/acceptance_wave3_inventory_oracle.py",
        "wave3-runner-parser-red.json": "parser-oracles/wave3-runner-parser-red.json",
        "wave3-runner-parser-green.json": "parser-oracles/wave3-runner-parser-green.json",
        "wave3-runner-inventory-oracle-final.json": "parser-oracles/wave3-runner-inventory-oracle-final.json",
        "host-recovery-clippy.inverse-proof.json": "review-evidence/host-recovery-clippy.inverse-proof.json",
        "preserve_wave3.py": "tools/preserve_wave3.py",
    }
    require(sha(read(EVIDENCE / "acceptance_wave3.py")) == RUNNER_SHA256, "Current runner differs from reviewed frozen runner")
    for source, target in extras.items():
        add(EVIDENCE / source, target)
    require(len(plan) == len({item["target"] for item in plan}), "Duplicate preservation target")
    for item in plan:
        plain_path(DESTINATION / item["target"], ROOT, file=True, missing=True)
    return plan


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--accepted-tag", required=True)
    args = parser.parse_args()
    require(os.name == "nt" and ROOT.name == "Code-Intelligence", "Unexpected Windows workspace")
    require(not DESTINATION.exists() and not DESTINATION.is_symlink(), "Preservation destination must not exist")
    plain_path(DESTINATION, ROOT, missing=True)
    prior = verify_prior_pins()
    accepted = verify_accepted(args.accepted_tag)
    failed = failed_attempts(args.accepted_tag)
    focused = focused_evidence()
    plan = copy_plan(accepted, failed, focused)
    require(source_manifest() == accepted["manifest"], "Current source differs from accepted freeze; root must reconcile before preservation")
    # All validation precedes the first new normal-file write. No rollback,
    # deletion, move or overwrite is attempted if a later copy/check fails.
    DESTINATION.mkdir()
    hashes = {}
    for item in plan:
        source, target = item["source"], DESTINATION / item["target"]
        require(sha(read(source)) == item["sha256"], "Admitted evidence changed before copy")
        plain_path(target, ROOT, file=True, missing=True)
        target.parent.mkdir(parents=True, exist_ok=True)
        with target.open("xb") as stream:
            stream.write(read(source))
        require(sha(read(target)) == item["sha256"], "Evidence copy hash differs")
        hashes[target.relative_to(ROOT).as_posix()] = item["sha256"]
    require(verify_prior_pins() == prior, "Historical evidence changed during copy")
    native = accepted["receipts"]["native"]
    bundle_prefix = DESTINATION.relative_to(ROOT).as_posix()
    metadata_additions = sorted([*hashes, bundle_prefix + "/index.json", bundle_prefix + "/index.sha256"])
    index = dict(schemaVersion=1, preservedAtUtc=datetime.datetime.now(datetime.timezone.utc).isoformat(),
                 scope="Unreleased Windows development checkpoint; universal parity, deferred platforms and release remain open",
                 acceptedTag=args.accepted_tag, acceptedFullMatrix=bundle_prefix + "/accepted/" + args.accepted_tag,
                 acceptedPhases=list(PHASES), sourceFiles=accepted["freeze"]["sourceFiles"],
                 sourceManifest=bundle_prefix + "/accepted/" + args.accepted_tag + "/source-freeze.json",
                 sourceManifestSha256=accepted["freeze"]["sourceManifestSha256"], runnerSha256=RUNNER_SHA256,
                 **{key: native[key] for key in ("groups", "passed", "failed", "ignored")},
                 nativeMatrix=accepted["nativeMatrix"],
                 phaseResults={phase: {key: receipt[key] for key in ("groups", "passed", "failed", "ignored")}
                               for phase, receipt in accepted["receipts"].items()},
                 failedAttempts=[{key: value for key, value in item.items() if key not in ("bundle", "freeze", "receipts")} for item in failed],
                 focusedEvidence=focused, focusedCountsInterpretation="overlapping_counts_not_additive",
                 excludedEvidence="wave3-metadata-draft has no runtime receipt; other children, binaries, fixtures and CAS are excluded; frozen bundles are handled only in their explicit categories",
                 totalInterpretation="Whole native totals come only from the accepted native receipt; focused, parser-oracle and failed-attempt counts are never added",
                 previousEvidence=prior, previousPinsVerifiedBeforeAndAfterCopy=True,
                 metadataBoundary="Accepted source freeze remains authoritative. Only this separate evidence bundle is added afterward; root must explicitly reconcile final post-freeze metadata edits later",
                 postFreezeInventoryPolicy="Git normal exclusions still apply; copied raw .log files are individually pinned even when absent from the normally unignored source inventory",
                 expectedPostFreezeEvidenceAdditions=metadata_additions, sha256=hashes, indexDigestFile="index.sha256")
    index_raw = encoded(index)
    plain_path(DESTINATION / "index.json", ROOT, file=True, missing=True)
    with (DESTINATION / "index.json").open("xb") as stream:
        stream.write(index_raw)
    plain_path(DESTINATION / "index.sha256", ROOT, file=True, missing=True)
    index_pin = (sha(index_raw) + "  index.json\n").encode("ascii")
    with (DESTINATION / "index.sha256").open("xb") as stream:
        stream.write(index_pin)
    require(verify_prior_pins() == prior, "Historical evidence changed after index creation")
    require(read(DESTINATION / "index.json") == index_raw and read(DESTINATION / "index.sha256") == index_pin, "Index or index pin changed")
    after = source_manifest()
    additions = {name: sha(read(ROOT / relative_path(name))) for name in metadata_additions}
    visible_additions = {name: expected for name, expected in additions.items() if name in after}
    require(after == accepted["manifest"] | visible_additions, "Unreconciled source changes during preservation")
    require(all(sha(read(ROOT / relative_path(name))) == expected for name, expected in hashes.items()), "Preserved evidence changed")
    require(read(DESTINATION / "index.json") == index_raw and read(DESTINATION / "index.sha256") == index_pin, "Final index or pin differs")
    print(json.dumps(dict(destination=str(DESTINATION), files=len(hashes), indexSha256=sha(index_raw),
                          priorPinsVerified=sum(item["verifiedPins"] for item in prior),
                          focusedReceipts=len(focused), passed=native["passed"], failed=native["failed"], ignored=native["ignored"],
                          interpretation="Evidence preserved only; root post-freeze metadata reconciliation remains pending")))


if __name__ == "__main__":
    main()
