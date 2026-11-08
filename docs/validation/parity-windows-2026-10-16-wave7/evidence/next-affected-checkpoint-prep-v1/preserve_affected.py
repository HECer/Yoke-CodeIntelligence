"""Plan/apply a fresh bounded R5B evidence bundle; execute no gates or renderer."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import runpy
import stat
import sys

sys.dont_write_bytecode = True
HERE = Path(__file__).absolute().parent
EVIDENCE = HERE.parent
ROOT = EVIDENCE.parent.parent
DEST = "docs/validation/parity-windows-2026-10-16-wave7"
REUSE = EVIDENCE / "next-combined-preservation-v2/preserve_combined.py"
REUSE_SHA = "4f4e598146bcb8b96f8cc256f63b4f73c6d8e34202f66f90f2de1c020b7263d6"
INPUTS_SHA = "ad902165e741bf4aec7933d02c871e10e9a38928a2abac2ae4ecfd9cb1ad12f5"
RUNNER_SHA = "b51bbffbddd07be0aa6ef019fc950d772c628d77b3567f646d7a837726aa3d15"
PHASES = ("format", "clippy", "native", "python", "typescript")
MAX_FILES, MAX_TOTAL = 20_000, 1024 * 1024 * 1024


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def bootstrap():
    """Load only exact reviewed IO/parser code; call none of its old planners."""
    require(os.name == "nt" and ROOT.name == "Code-Intelligence", "Expected Windows root")
    for path in (HERE, REUSE, *REUSE.parents):
        info = path.lstat()
        require(not stat.S_ISLNK(info.st_mode)
                and not getattr(info, "st_file_attributes", 0) & 0x400,
                "Reparse bootstrap refused")
    require(REUSE.resolve(strict=True) == REUSE, "Aliased bootstrap refused")
    before = REUSE.stat()
    require(stat.S_ISREG(before.st_mode) and before.st_size <= 128 * 1024,
            "Bootstrap size/type")
    identity = lambda x: (x.st_dev, x.st_ino, x.st_size, x.st_mtime_ns)
    with REUSE.open("rb") as stream:
        opened = os.fstat(stream.fileno())
        raw = stream.read(before.st_size + 1)
        completed = os.fstat(stream.fileno())
    require(len(raw) == before.st_size
            and identity(before) == identity(opened) == identity(completed) == identity(REUSE.stat())
            and hashlib.sha256(raw).hexdigest() == REUSE_SHA, "Reviewed IO changed")
    module = runpy.run_path(str(REUSE))
    require(module["ROOT"] == ROOT, "IO root differs")
    return module


IO = bootstrap()
core = IO["core"]
read, plain, source_manifest = IO["read"], IO["plain"], IO["source_manifest"]
sha, encoded, relative_path = core.sha, core.encoded, core.relative_path
valid_hash, parse = core.valid_hash, core.load_json_bytes


def obj(path):
    return parse(read(path, ROOT))


def direct_names(folder):
    """Direct names only, never recursive history or opaque JSON pointers."""
    plain(folder, ROOT)
    names = []
    with os.scandir(folder) as scan:
        for entry in scan:
            require(len(names) < MAX_FILES, "Directory entry limit")
            names.append(entry.name)
    core.unique_paths(names)
    return set(names)


def mapped(source):
    path = ROOT / relative_path(source)
    suffix = (path.relative_to(EVIDENCE).as_posix() if path.is_relative_to(EVIDENCE)
              else "root-context/" + source)
    return DEST + "/evidence/" + suffix


def validate_cache(inputs, which, manifest):
    receipt_name, plan_name = inputs[which + "CacheReceipt"], inputs[which + "CachePlan"]
    receipt = obj(ROOT / receipt_name)
    require(receipt["status"] == "selected_fingerprint_metadata_retained_and_invalidated"
            and receipt["selection"] == "all_own_package_fingerprints_for_context_switch"
            and receipt["profiles"] == ["debug"]
            and receipt["completeOwnPackageInvalidationForSelectedProfiles"] is True
            and receipt["remainingOwnPackageDirectoryCount"] == 0
            and receipt["normalSourceWrites"] is False and receipt["cargoInvoked"] is False
            and receipt["reviewedPlanSha256"] == inputs["files"][plan_name]["sha256"],
            "Cache context incomplete")
    pins = receipt["afterCurrentSourcePins"]
    require(set(pins) == {"Cargo.toml", "Cargo.lock", "native/Cargo.toml", "native/src/lib.rs"}
            and receipt["beforeCurrentSourcePins"] == pins
            and all(manifest[name] == row["sha256"] for name, row in pins.items()),
            "Cache source bindings differ")
    return dict(receipt=mapped(receipt_name),
                receiptSha256=inputs["files"][receipt_name]["sha256"],
                plan=mapped(plan_name), planSha256=inputs["files"][plan_name]["sha256"],
                artifactProvenanceVerified=receipt["artifactProvenanceVerified"],
                laterCompileAndSourceOriginEvidenceStillRequired=
                    receipt["laterCompileAndSourceOriginEvidenceStillRequired"])


def validate_controls(inputs):
    base = ROOT / relative_path(inputs["promotionRoot"])
    before, after = obj(base / "normal-before.json"), obj(base / "normal-after.json")
    receipt, context = obj(base / "receipt.json"), obj(base / "normal-context.json")
    plan = obj(EVIDENCE / "next-git-affected-promotion-v1/APPLY-PLAN.json")
    require(receipt["accepted"] is True and receipt["normalCheckoutAcceptanceClaim"] is False
            and len(receipt["completed"]) == len(set(receipt["completed"])) == 19
            and set(receipt["completed"]) == {row["path"] for row in plan["actions"]}
            and receipt["normalBeforeSha256"] == sha(encoded(before))
            and receipt["normalAfterSha256"] == sha(encoded(after))
            and receipt["normalFiles"] == len(after), "Promotion receipt differs")
    overlay = dict(before)
    for row in plan["actions"]:
        require(overlay.get(row["path"], "ABSENT") == row["before"], "Promotion preimage differs")
        overlay[row["path"]] = row["after"]
    require(overlay == after, "Promotion full map differs")
    image = obj(ROOT / relative_path(inputs["imageManifest"]))
    require(receipt["imageMapSha256"] == sha(encoded(image)), "Promotion image differs")
    records = [parse(line) for line in read(base / "journal.jsonl", ROOT).splitlines()]
    require(records and records[-1].get("event") == "finished"
            and records[-1]["receiptSha256"] == sha(read(base / "receipt.json", ROOT)),
            "Promotion journal incomplete")
    require(context == {
        "cacheOriginReceiptPath": inputs["normalCacheReceipt"],
        "cacheOriginReceiptSha256": inputs["files"][inputs["normalCacheReceipt"]]["sha256"],
        "fullAcceptancePending": True, "fullTag": inputs["failedAttempt"]["tag"],
        "normalSourceMapSha256": receipt["normalAfterSha256"],
        "promoterReceiptSha256": sha(read(base / "receipt.json", ROOT)),
        "rootExclusiveCargo": True}, "Root normal context differs")
    return after, image, receipt


def focused_rows(inputs):
    rows = []
    for spec in inputs["focused"]:
        base = ROOT / relative_path(spec["path"])
        receipt = obj(base / "receipt.json")
        require(all(receipt.get(k) == v for k, v in spec["expected"].items())
                and receipt["validationErrors"] == [] and receipt["imageSourceChanges"] == [],
                "Focused receipt differs")
        before, after = obj(base / "source-before.json"), obj(base / "source-after.json")
        require(before == after and len(before) == receipt["sourceFiles"]
                and sha(encoded(before)) == receipt.get("imageManifestSha256",
                    receipt.get("reviewedManifestFileSha256")), "Focused source map differs")
        raw = read(base / "run.log", ROOT)
        require(sha(raw) == receipt.get("logSha256", receipt.get("rawSha256")),
                "Focused raw log differs")
        counts = {}
        if "groups" in receipt:
            groups = re.findall(rb"test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;", raw)
            counts = dict(groups=len(groups), passed=sum(int(g[0]) for g in groups),
                          failed=sum(int(g[1]) for g in groups), ignored=sum(int(g[2]) for g in groups))
            require(all(receipt[k] == v for k, v in counts.items()), "Focused raw totals differ")
        rows.append(dict(role=spec["role"], receipt=mapped(spec["path"] + "/receipt.json"),
                         receiptSha256=sha(read(base / "receipt.json", ROOT)),
                         scope=receipt["scope"], exitCode=receipt["exitCode"],
                         imageManifestSha256=sha(encoded(before)), countsAreNonadditive=True, **counts))
    return rows


def failed_attempt(inputs, promoted):
    history = dict(inputs["failedAttempt"])
    base = EVIDENCE / history["tag"]
    freeze = obj(base / "freeze.json")
    require(obj(base / "source-freeze.json") == promoted
            and freeze["sourceManifestSha256"] == sha(encoded(promoted))
            and freeze["runnerSha256"] == RUNNER_SHA, "Failed-attempt freeze differs")
    pins = {}
    for phase, expected in history["phases"].items():
        receipt = obj(base / phase / "receipt.json")
        require(all(receipt[k] == value for k, value in expected.items())
                and receipt["sourceChanges"] == receipt["validationErrors"] == []
                and receipt["sourceManifestSha256"] == freeze["sourceManifestSha256"]
                and receipt["runnerSha256"] == RUNNER_SHA
                and obj(base / phase / "source-before.json") == promoted
                and obj(base / phase / "source-after.json") == promoted,
                "Failed-attempt phase differs")
        name = (base / phase / "receipt.json").relative_to(ROOT).as_posix()
        pins[phase] = dict(path=mapped(name), sha256=inputs["files"][name]["sha256"])
    require(history["accepted"] is False and any(row["failed"] for row in history["phases"].values()),
            "Failed-attempt history cannot become acceptance")
    history["phaseReceipts"] = pins
    return history


def process_repair(inputs, promoted, manifest):
    base = ROOT / relative_path(inputs["processRepairRoot"])
    overlay = obj(base / "repair-overlay-v1.json")
    repaired = dict(promoted)
    for name, row in overlay["changes"].items():
        relative_path(name)
        require(repaired.get(name, "ABSENT") == row["before"], "Process repair preimage differs")
        repaired[name] = row["after"]
    require(repaired == manifest == obj(base / "source-with-process-repair-v1.json")
            and overlay["sourceManifestSha256"] == sha(encoded(manifest))
            and overlay["sourceFiles"] == len(manifest)
            and overlay["firstFailedFullTag"] == inputs["failedAttempt"]["tag"]
            and overlay["rootExclusiveNormalCargoContextRetained"] is True
            and overlay["rustSourceChangesSincePromotion"] == [], "Process repair overlay differs")
    gates = []
    for folder, exit_code, passed, failed in [
        ("process-helper-nonzero-red-20261016", 1, 0, 1),
        ("process-helper-and-lifetime-green-20261016", 0, 11, 0),
    ]:
        receipt = obj(base / folder / "receipt.json")
        raw = read(base / folder / "run.log", ROOT)
        values = re.findall(r"^[#\u2139] (tests|pass|fail|cancelled|skipped|todo) (\d+)\r?$",
                            raw.decode("utf-8"), re.M)
        counts = {key: int(value) for key, value in values}
        require(len(values) == len(counts) == 6 and receipt["exitCode"] == exit_code
                and receipt["before"] == receipt["after"] and sha(raw) == receipt["logSha256"]
                and counts == {k: int(v) for k, v in receipt["counts"].items()}
                and counts == dict(tests=passed + failed, **{"pass": passed, "fail": failed,
                    "cancelled": 0, "skipped": 0, "todo": 0}), "Process raw gate differs")
        name = (base / folder / "receipt.json").relative_to(ROOT).as_posix()
        gates.append(dict(receipt=mapped(name), receiptSha256=inputs["files"][name]["sha256"],
                          exitCode=exit_code, counts=counts, countsAreNonadditive=True))
    return dict(overlay=mapped(inputs["processRepairRoot"] + "/repair-overlay-v1.json"),
                sourceManifestSha256=overlay["sourceManifestSha256"], gates=gates,
                countsAreNonadditive=True)


def aborted_attempt(inputs, manifest):
    """Retain stopped native output as partial history, never a native receipt."""
    history = dict(inputs["abortedAttempt"])
    base = EVIDENCE / history["tag"]
    freeze = obj(base / "freeze.json")
    source_sha = sha(encoded(manifest))
    require(history["accepted"] is False and history["nativePhaseAborted"] is True
            and history["nativeFinalReceiptPreserved"] is False
            and history["summaryPreserved"] is False
            and obj(base / "source-freeze.json") == manifest
            and freeze["sourceManifestSha256"] == source_sha
            and freeze["runnerSha256"] == RUNNER_SHA
            and sha(read(base / "runner.py", ROOT)) == RUNNER_SHA
            and direct_names(base) == {*PHASES, "freeze.json", "source-freeze.json", "runner.py"},
            "Aborted-attempt freeze/root differs")
    control = obj(ROOT / relative_path(history["control"]))
    inspection = obj(ROOT / relative_path(history["inspection"]))
    require(control["accepted"] is False and control["nativePhaseAborted"] is True
            and control["fullTag"] == history["tag"] and control["reason"] == history["reason"]
            and control["taskkillExitCode"] == 0 and control["nativeFinalReceiptExists"] is False
            and control["sourceChanges"] is False and control["sourceManifestSha256"] == source_sha
            and inspection["Runner"] == [control["ownedRunner"]], "Aborted control differs")
    pins = {}
    require(set(history["phases"]) == {"format", "clippy", "python", "typescript"},
            "Aborted completed phase set differs")
    for phase, expected in history["phases"].items():
        folder = base / phase
        receipt = obj(folder / "receipt.json")
        require(expected["accepted"] is True and expected["failed"] == 0
                and all(receipt[k] == value for k, value in expected.items())
                and receipt["phase"] == phase
                and receipt["sourceChanges"] == receipt["validationErrors"] == []
                and receipt["sourceManifestSha256"] == source_sha
                and receipt["runnerSha256"] == RUNNER_SHA
                and obj(folder / "source-before.json") == manifest
                and obj(folder / "source-after.json") == manifest
                and direct_names(folder) == {"receipt.json", *receipt["artifactSha256"]},
                "Aborted completed phase differs")
        for name, digest in receipt["artifactSha256"].items():
            require(len(Path(relative_path(name)).parts) == 1, "Nested aborted phase artifact")
            source = (folder / name).relative_to(ROOT).as_posix()
            require(inputs["files"][source]["sha256"] == digest,
                    "Aborted phase artifact binding differs")
        name = (folder / "receipt.json").relative_to(ROOT).as_posix()
        pins[phase] = dict(path=mapped(name), sha256=inputs["files"][name]["sha256"])
    partial = history["partialNativeFiles"]
    core.unique_paths(partial)
    require("receipt.json" not in partial and "source-after.json" not in partial
            and direct_names(base / "native") == set(partial)
            and obj(base / "native/source-before.json") == manifest,
            "Aborted partial native membership differs")
    for name in partial:
        require(len(Path(relative_path(name)).parts) == 1, "Nested partial native artifact")
        require((base / "native" / name).relative_to(ROOT).as_posix() in inputs["files"],
                "Unpinned partial native artifact")
    require(set(control["failedFixtureLogSha256"]) == {f"library-c-acl-{i}.log" for i in range(3)},
            "Aborted ACL failure set differs")
    for name, digest in control["failedFixtureLogSha256"].items():
        source = (base / "native" / name).relative_to(ROOT).as_posix()
        require(inputs["files"][source]["sha256"] == digest, "Aborted ACL failure log differs")
    history["phaseReceipts"] = pins
    history["partialNativeDirectory"] = mapped((base / "native").relative_to(ROOT).as_posix())
    for name in ("control", "inspection"):
        original = history[name]
        history[name] = dict(path=mapped(original), sha256=inputs["files"][original]["sha256"])
    history["countsAreNonadditive"] = True
    return history


def acl_qualification(inputs, manifest):
    """Three exact fixture qualifications have no authority to accept a full run."""
    spec = inputs["aclQualification"]
    base = ROOT / relative_path(spec["root"])
    receipt = obj(base / "receipt.json")
    require(receipt["scope"] == spec["scope"]
            == "three_exact_acl_fixture_qualification_not_full_acceptance"
            and receipt["accepted"] is True and receipt["sourceChanges"] == []
            and receipt["sourceManifestSha256"] == sha(encoded(manifest))
            and receipt["tempRoot"] == spec["tempRoot"]
            and len(receipt["results"]) == len(spec["tests"]) == len(set(spec["tests"])) == 3
            and spec["countsAreNonadditive"] is True
            and direct_names(base) == {"receipt.json", "acl-0.log", "acl-1.log", "acl-2.log"},
            "ACL qualification scope differs")
    for i, (row, name) in enumerate(zip(receipt["results"], spec["tests"])):
        raw = read(base / f"acl-{i}.log", ROOT)
        require(row["command"] == ["cargo", "test", "--workspace", "--locked", "-j2", "--lib",
                name, "--", "--exact", "--test-threads=4"]
                and row["exitCode"] == 0 and row["exactPass"] is True
                and sha(raw) == row["logSha256"]
                and re.findall(rb"test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;",
                               raw) == [(b"1", b"0", b"0")]
                and len(re.findall(rb"^test " + re.escape(name.encode()) + rb" \.\.\. ok\r?$",
                                   raw, re.M)) == 1,
                "ACL qualification exact result differs")
    source = (base / "receipt.json").relative_to(ROOT).as_posix()
    return dict(scope=receipt["scope"], receipt=mapped(source),
                receiptSha256=inputs["files"][source]["sha256"],
                sourceManifestSha256=receipt["sourceManifestSha256"],
                groups=3, passed=3, failed=0, ignored=0, countsAreNonadditive=True,
                fullAcceptanceClaim=False)


def build(tag, source_sha, summary_sha):
    require(re.fullmatch(r"affected-r5b-[a-z0-9][a-z0-9-]{0,80}-full", tag)
            and valid_hash(source_sha) and valid_hash(summary_sha), "Reviewed tag/source/summary required")
    inputs_raw = read(HERE / "INPUTS.json", ROOT)
    require(sha(inputs_raw) == INPUTS_SHA, "Frozen input catalog changed")
    inputs = parse(inputs_raw)
    require(inputs["schemaVersion"] == 1
            and tag not in {inputs["failedAttempt"]["tag"], inputs["abortedAttempt"]["tag"]},
            "Failed or aborted tag cannot become acceptance")
    catalog, total = {}, 0

    def add(name, expected=None):
        nonlocal total
        relative_path(name)
        raw = read(ROOT / relative_path(name), ROOT)
        if expected is not None:
            require(sha(raw) == expected["sha256"] and len(raw) == expected["bytes"], "Pinned input changed: " + name)
        if name not in catalog:
            total += len(raw)
            require(len(catalog) < MAX_FILES and total <= MAX_TOTAL, "Evidence catalog limit")
            catalog[name] = dict(destination=mapped(name), sha256=sha(raw), bytes=len(raw))
        return raw

    for name, expected in inputs["files"].items():
        add(name, expected)
    for leaf in ("preserve_affected.py", "INPUTS.json", "PRESERVER.md"):
        add((HERE / leaf).relative_to(ROOT).as_posix())
    promoted, image, promotion = validate_controls(inputs)
    caches = {"image": validate_cache(inputs, "image", image),
              "normal": validate_cache(inputs, "normal", promoted)}
    focused = focused_rows(inputs)
    failed = failed_attempt(inputs, promoted)
    bundle = EVIDENCE / tag
    require(sha(read(bundle / "summary.json", ROOT)) == summary_sha, "Root-reviewed full summary differs")
    checked = core.validate_bundle(bundle, source_sha, RUNNER_SHA, ROOT)
    manifest = checked["manifest"]
    caches["normal"] = validate_cache(inputs, "normal", manifest)
    process = process_repair(inputs, promoted, manifest)
    aborted = aborted_attempt(inputs, manifest)
    qualification = acl_qualification(inputs, manifest)
    require(direct_names(bundle) == {*PHASES, "freeze.json", "source-freeze.json", "runner.py", "summary.json"},
            "Full bundle root membership differs")
    for leaf in ("freeze.json", "source-freeze.json", "runner.py", "summary.json"):
        add((bundle / leaf).relative_to(ROOT).as_posix())
    for phase in PHASES:
        receipt = checked["receipts"][phase]
        names = {"receipt.json", *receipt["artifactSha256"]}
        require(direct_names(bundle / phase) == names, "Full phase membership differs")
        for name in names:
            require(len(Path(name).parts) == 1, "Nested phase artifact refused")
            add((bundle / phase / name).relative_to(ROOT).as_posix())
    normal = source_manifest()
    require(normal == manifest and sha(encoded(normal)) == source_sha,
            "Current full normal map differs from reviewed accepted freeze")
    core.unique_paths(row["destination"] for row in catalog.values())
    phase_results = {p: {k: checked["receipts"][p][k] for k in ("groups", "passed", "failed", "ignored")}
                     for p in PHASES}
    matrix = {k: checked["nativeMatrix"][k] for k in
              ("cargoTargets", "integrationTargets", "libraryRegistered", "libraryIgnored")}
    changes = [dict(path=n, before=promoted.get(n, "ABSENT"), after=normal.get(n, "ABSENT"))
               for n in sorted(promoted.keys() | normal.keys()) if promoted.get(n) != normal.get(n)]
    index = dict(schemaVersion=1, kind="affected_windows_development_checkpoint",
                 acceptedTag=tag, sourceManifestSha256=source_sha, runnerSha256=RUNNER_SHA,
                 sourceFiles=len(manifest), acceptedPhases=list(PHASES),
                 phaseResults=phase_results, nativeMatrix=matrix,
                 fullSummary=dict(path=mapped((bundle / "summary.json").relative_to(ROOT).as_posix()),
                                  sha256=summary_sha),
                 focusedEvidence=focused, failedNormalAttempts=[failed, aborted],
                 promotion=dict(receipt=mapped(inputs["promotionRoot"] + "/receipt.json"),
                                receiptSha256=sha(read(ROOT / inputs["promotionRoot"] / "receipt.json", ROOT)),
                                originalNormalSourceMapSha256=promotion["normalAfterSha256"],
                                postPromotionSourceChanges=changes, laterSourcePinExplicitlyReviewed=True),
                 cacheOrigin=caches, processRepair=process, aclQualification=qualification,
                 rawCountsAreNonadditive=True,
                 claims=dict(universalParity=False, releaseCertified=False, newParityRowsCertified=False),
                 sha256={row["destination"]: row["sha256"] for row in catalog.values()})
    additions = dict(index["sha256"])
    additions[DEST + "/index.json"] = sha(encoded(index))
    additions[DEST + "/index.sha256"] = sha((sha(encoded(index)) + "\n").encode())
    # The reviewed plan cannot contain its own SHA; one extra file is explicit.
    all_names = sorted([*additions, DEST + "/reviewed-plan.json"])
    core.unique_paths(all_names)
    require(len(all_names) <= MAX_FILES, "Destination file limit")
    ignored = IO["ignored_additions"](all_names)
    return dict(schemaVersion=1, kind="reviewed_affected_preservation_plan", destination=DEST,
                reviewed=dict(tag=tag, sourceManifestSha256=source_sha, summarySha256=summary_sha),
                normalBefore=normal, catalog=catalog, catalogFiles=len(catalog), catalogBytes=total,
                index=index, baseAdditions=additions, ignoredOutputs=ignored,
                basePlannedBytes=total + len(encoded(index)) + 65,
                destinationFilesIncludingReviewedPlan=len(all_names),
                reviewedPlanFileAddedWithCallerSuppliedDigest=True)


def plan_path(raw, missing=False):
    path = Path(raw)
    require(path.is_absolute() and path.parent == HERE
            and re.fullmatch(r"REVIEWED-PLAN-[a-z0-9-]+\.json", path.name), "Direct reviewed plan path required")
    return plain(path, HERE, file=True, missing=missing)


def exclusive(path, raw):
    with path.open("xb") as stream:
        require(stream.write(raw) == len(raw), "Short output write")
        stream.flush()
        os.fsync(stream.fileno())


def apply(plan, raw, digest):
    require(build(plan["reviewed"]["tag"], plan["reviewed"]["sourceManifestSha256"],
                  plan["reviewed"]["summarySha256"]) == plan, "Reviewed plan no longer matches actual inputs")
    destination = ROOT / DEST
    plain(destination, ROOT, missing=True)
    require(not destination.exists(), "Destination already exists")
    require(plan["basePlannedBytes"] + len(raw) <= MAX_TOTAL, "Complete output byte limit")
    backup = HERE / ("application-" + digest)
    plain(backup, HERE, missing=True)
    backup.mkdir(exist_ok=False)
    exclusive(backup / "normal-before.json", encoded(plan["normalBefore"]))
    exclusive(backup / "reviewed-plan.json", raw)
    expected = plan["baseAdditions"] | {DEST + "/reviewed-plan.json": digest}
    with (backup / "journal.jsonl").open("xb") as journal:
        def record(value):
            data = encoded(value)
            require(journal.write(data) == len(data), "Short journal write")
            journal.flush()
            os.fsync(journal.fileno())

        def parents(path):
            current = ROOT
            for part in path.parent.relative_to(ROOT).parts:
                current /= part
                plain(current, ROOT, missing=True)
                if not current.exists():
                    require(current == destination or current.is_relative_to(destination),
                            "Missing parent outside new destination")
                    record(dict(mkdirIntent=current.relative_to(ROOT).as_posix()))
                    current.mkdir(exist_ok=False)
                    plain(current, ROOT)

        def write(name, data):
            target = ROOT / relative_path(name)
            require(name.startswith(DEST + "/") and sha(data) == expected[name], "Unplanned output")
            parents(target)
            plain(target, ROOT, file=True, missing=True)
            record(dict(writeIntent=name, sha256=expected[name], bytes=len(data)))
            exclusive(target, data)
            require(sha(read(target, ROOT)) == expected[name], "Copied bytes differ")
            record(dict(written=name))

        record(dict(operation="fresh_wave7_preservation", reviewedPlanSha256=digest,
                    normalBeforeSha256=sha(encoded(plan["normalBefore"]))))
        try:
            for name, row in sorted(plan["catalog"].items()):
                data = read(ROOT / relative_path(name), ROOT)
                require(sha(data) == row["sha256"] and len(data) == row["bytes"], "Evidence changed before copy")
                write(row["destination"], data)
            index_raw = encoded(plan["index"])
            write(DEST + "/index.json", index_raw)
            write(DEST + "/index.sha256", (sha(index_raw) + "\n").encode())
            write(DEST + "/reviewed-plan.json", raw)
            actual, stack, nodes, total = {}, [destination], 0, 0
            while stack:
                folder = stack.pop()
                for name in direct_names(folder):
                    nodes += 1
                    require(nodes <= MAX_FILES and len((folder / name).relative_to(destination).parts) <= 64,
                            "Destination inventory limit")
                    path = folder / name
                    info = path.lstat()
                    if stat.S_ISDIR(info.st_mode):
                        plain(path, ROOT)
                        stack.append(path)
                    else:
                        data = read(path, ROOT)
                        total += len(data)
                        require(total <= MAX_TOTAL, "Destination byte limit")
                        actual[path.relative_to(ROOT).as_posix()] = sha(data)
            require(actual == expected, "Complete destination inventory differs")
            require(IO["ignored_additions"](sorted(expected)) == plan["ignoredOutputs"], "Git ignore partition changed")
            for name, row in plan["catalog"].items():
                require(sha(read(ROOT / relative_path(name), ROOT)) == row["sha256"], "Original evidence changed")
            visible = {n: pin for n, pin in expected.items() if n not in plan["ignoredOutputs"]}
            after = source_manifest()
            require(after == plan["normalBefore"] | visible, "Unexpected full normal map changes")
            exclusive(backup / "normal-after.json", encoded(after))
            receipt = dict(complete=True, reviewedPlanSha256=digest,
                           index=DEST + "/index.json", indexSha256=sha(index_raw),
                           copiedFiles=len(actual), copiedBytes=total,
                           normalAfterSha256=sha(encoded(after)), normalAfterFiles=len(after),
                           acceptedTag=plan["reviewed"]["tag"], runtimeSourceChanges=[],
                           claims=plan["index"]["claims"])
            exclusive(backup / "receipt.json", encoded(receipt))
            record(receipt)
            print(json.dumps(receipt, sort_keys=True))
        except Exception as error:
            record(dict(complete=False, error=str(error), partialOutputsRetained=True,
                        originalsRetained=True, rollbackPerformed=False))
            raise


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    planning = commands.add_parser("plan")
    planning.add_argument("--tag", required=True)
    planning.add_argument("--reviewed-source-manifest-sha256", required=True)
    planning.add_argument("--reviewed-summary-sha256", required=True)
    planning.add_argument("--plan-out", required=True)
    applying = commands.add_parser("apply")
    applying.add_argument("--plan", required=True)
    applying.add_argument("--reviewed-plan-sha256", required=True)
    applying.add_argument("--allow-new-evidence-bundle", action="store_true", required=True)
    args = parser.parse_args()
    if args.command == "plan":
        path = plan_path(args.plan_out, missing=True)
        result = build(args.tag, args.reviewed_source_manifest_sha256, args.reviewed_summary_sha256)
        raw = encoded(result)
        require(result["basePlannedBytes"] + len(raw) <= MAX_TOTAL, "Complete output byte limit")
        plain(path, HERE, file=True, missing=True)
        exclusive(path, raw)
        print(json.dumps(dict(plan=str(path), planSha256=sha(raw),
                              catalogFiles=result["catalogFiles"], catalogBytes=result["catalogBytes"],
                              destinationFiles=result["destinationFilesIncludingReviewedPlan"],
                              destinationBytes=result["basePlannedBytes"] + len(raw),
                              sourceManifestSha256=args.reviewed_source_manifest_sha256,
                              phaseResults=result["index"]["phaseResults"], normalWrites=False), sort_keys=True))
    else:
        require(valid_hash(args.reviewed_plan_sha256), "Reviewed plan digest required")
        raw = read(plan_path(args.plan), ROOT)
        require(sha(raw) == args.reviewed_plan_sha256, "Reviewed plan changed")
        apply(parse(raw), raw, args.reviewed_plan_sha256)


if __name__ == "__main__":
    main()

