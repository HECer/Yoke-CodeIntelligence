"""STAGED wave5 preservation. No actual copying before root's exact-plan approval.

No Cargo, test execution, candidate runner import, cleanup, or network calls.
Evidence consistency never certifies universal parity or release readiness.
"""
import argparse
import datetime
import json
import os
from pathlib import Path
import re
import subprocess
import sys

sys.dont_write_bytecode = True
from receipt_core import (MAX_FILES, MAX_TOTAL, PHASES, TOP_FILES, check_manifest,
                          encoded, library_partition, literal_contract, load,
                          load_json_bytes, parse_python, plain, read, relative_path,
                          require, sha, unique_paths, valid_hash, validate_bundle)

STAGING = Path(__file__).resolve().parent
PRIOR = (
    ("docs/validation/parity-windows-2026-10-15/index.json", 160, "156852befe63b51cd99d66f9b9a857157659715f31ffb0a653720373677d91a0"),
    ("docs/validation/parity-windows-2026-10-15-wave2/index.json", 210, "0e2e504fb30ec388ae46efe53dd4639de55926753ffc8b9b5da74fc55c71633e"),
    ("docs/validation/parity-windows-2026-10-15-wave3/index.json", 416, "98646f7e98060d475129f4db26e353f37f57059658f639382888a20240208e97"),
    ("docs/validation/parity-windows-2026-10-15-wave4/index.json", 864, "1b829007c444e02be250626ca0e418830932d478a49ad1c88dc828eb3a72acbf"),
)
REVIEWED_RUNNER_SHA256 = "b51bbffbddd07be0aa6ef019fc950d772c628d77b3567f646d7a837726aa3d15"
TOOL_FILES = ("preserve_wave5.py", "receipt_core.py", "test_preservation.py", "PLAN.md")
RAW_SUFFIXES = {".json", ".log", ".py", ".md", ".txt", ".patch"}
MAX_NODES = 20_000
MAX_DEPTH = 64
CLAIMS = dict(universalParity=False, releaseCertified=False,
              scope="Unreleased Windows development checkpoint; deferred platforms and remaining capabilities stay open")


def source_manifest(workspace):
    raw = subprocess.check_output(["rtk", "proxy", "git", "ls-files", "-c", "-o", "--exclude-standard", "-z"], cwd=workspace)
    names = sorted(set(raw.decode("utf-8").split("\0")) - {""})
    unique_paths(names)
    manifest = {}
    for name in names:
        path = workspace / relative_path(name)
        try:
            path.lstat()
        except FileNotFoundError:
            continue  # Git can still list a path deleted before this freeze.
        manifest[name] = sha(read(path, workspace))
    check_manifest(manifest)
    return manifest


def verify_prior_pins(workspace):
    verified = []
    for relative, count, expected in PRIOR:
        path = workspace / relative_path(relative)
        raw = read(path, workspace)
        require(sha(raw) == expected, "Historical index changed: " + relative)
        index = load_json_bytes(raw)
        require(isinstance(index, dict), "Historical index is not an object")
        pins = index["sha256"]
        require(isinstance(pins, dict) and len(pins) == count, "Historical pin count changed")
        unique_paths(pins)
        prefix = path.parent.relative_to(workspace).as_posix() + "/"
        for name, value in pins.items():
            require(name.startswith(prefix) and valid_hash(value) and sha(read(workspace / relative_path(name), workspace)) == value, "Historical artifact changed: " + name)
        sidecar = path.with_name("index.sha256")
        require(index.get("indexDigestFile") in (None, "index.sha256"), "Unknown historical digest contract")
        if index.get("indexDigestFile") == "index.sha256":
            require(sidecar.is_file(), "Advertised historical index sidecar is missing")
        if sidecar.exists():
            require(read(sidecar, workspace) == (expected + "  index.json\n").encode("ascii"), "Historical index sidecar differs")
        verified.append(dict(index=relative, indexSha256=expected, verifiedPins=count, sidecarVerified=sidecar.exists()))
    return verified


def bounded_entries(directory, limit):
    entries = []
    with os.scandir(directory) as scan:
        for entry in scan:
            require(len(entries) < limit, "Evidence directory node capacity exhausted")
            entries.append(Path(entry.path))
    return sorted(entries)


def tree_files(directory, boundary):
    plain(directory, boundary)
    files, pending, total, nodes = [], [(directory, 0)], 0, 1
    while pending:
        parent, depth = pending.pop()
        entries = bounded_entries(parent, MAX_NODES - nodes)
        nodes += len(entries)
        for path in entries:
            if path.is_dir():
                plain(path, boundary)
                require(depth + 1 <= MAX_DEPTH, "Evidence directory depth exhausted")
                pending.append((path, depth + 1))
            else:
                data = read(path, boundary)
                require(path.suffix in RAW_SUFFIXES, "Unknown raw evidence kind requires review")
                total += len(data)
                files.append(path)
                require(len(files) <= MAX_FILES and total <= MAX_TOTAL, "Raw evidence exceeds limits")
    unique_paths(path.relative_to(directory).as_posix() for path in files)
    return sorted(files)


def classify_raw(directory, boundary, frozen):
    outcomes, errors = {}, []
    paths = [(phase, directory / phase / "receipt.json") for phase in PHASES]
    if not frozen:
        paths.append(("focused", directory / "receipt.json"))
    for phase, path in paths:
        if not path.exists():
            continue
        try:
            receipt = load(path, boundary)
            require(isinstance(receipt, dict), "Raw receipt is not a JSON object")
            outcomes[phase] = {key: receipt.get(key) for key in ("scope", "accepted", "exitCode", "passed", "failed", "ignored", "sourceChanges", "validationErrors")}
        except (RuntimeError, KeyError, TypeError) as error:
            errors.append(phase + ": " + str(error))
    return dict(tag=directory.name, classification="raw_frozen_attempt_not_certified" if frozen else ("raw_qualified_manual_gate_not_full_acceptance" if directory.name.startswith("wave5-qualified-") else "raw_focused_gate_not_certified"),
                acceptedFullRun=False, recordedOutcomes=outcomes, parseDiagnostics=errors,
                countsInterpretation="Overlapping historical counts are never added to accepted totals; runner code is never imported")


def tool_hashes():
    return {name: sha(read(STAGING / name, STAGING)) for name in TOOL_FILES}


def build_plan(spec):
    require(set(spec) == {"workspace", "evidenceRoot", "acceptedTag", "reviewedSourceSha256", "reviewedRunnerSha256", "destination"}, "Unknown/missing explicit preservation inputs")
    require(spec["reviewedRunnerSha256"] == REVIEWED_RUNNER_SHA256, "Accepted runner changed; independent runner/verifier review required")
    workspace, evidence = Path(spec["workspace"]), Path(spec["evidenceRoot"])
    require(os.name == "nt" and workspace.is_absolute() and workspace.resolve(strict=True) == workspace and workspace.name == "Code-Intelligence", "Canonical Windows workspace required")
    require(evidence == workspace / "target/parity-validation-20261015", "Unexpected evidence boundary")
    plain(evidence, workspace)
    require(re.fullmatch(r"wave5-frozen-[A-Za-z0-9][A-Za-z0-9_.-]{0,90}", spec["acceptedTag"]) is not None, "Concrete wave5 full-freeze tag required")
    destination = relative_path(spec["destination"])
    require(destination.parent.as_posix() == "docs/validation" and destination.name.startswith("parity-windows-") and "wave5" in destination.name, "New direct wave5 validation destination required")
    target_root = workspace / destination
    require(not target_root.exists() and not target_root.is_symlink(), "Destination exists; no merge or overwrite")
    plain(target_root, workspace, missing=True)
    historical = verify_prior_pins(workspace)
    bundle = plain(evidence / spec["acceptedTag"], evidence)
    accepted = validate_bundle(bundle, spec["reviewedSourceSha256"], spec["reviewedRunnerSha256"], workspace)
    require(source_manifest(workspace) == accepted["manifest"], "Current normal files differ from accepted freeze")
    items, planned_bytes = [], 0

    def add(source, target):
        nonlocal planned_bytes
        require(len(items) < MAX_FILES, "Preservation plan file capacity exhausted")
        relative_path(target)
        data = read(source, workspace)
        require(planned_bytes + len(data) <= MAX_TOTAL, "Preservation plan byte capacity exhausted")
        planned_bytes += len(data)
        items.append(dict(source=source.relative_to(workspace).as_posix(), target=target, bytes=len(data), sha256=sha(data)))

    expected = set((*TOP_FILES, "summary.json"))
    for phase, receipt in accepted["receipts"].items():
        expected.update(phase + "/" + name for name in ("receipt.json", *receipt["artifactSha256"]))
    require({path.relative_to(bundle).as_posix() for path in tree_files(bundle, evidence)} == expected, "Unpinned/missing accepted-bundle files")
    for relative in sorted(expected):
        add(bundle / relative_path(relative), "accepted/" + bundle.name + "/" + relative)
    raw_attempts, discovery = [], []
    for path in bounded_entries(evidence, MAX_NODES):
        if not path.name.startswith("wave5-") or path == bundle:
            continue
        discovery.append(path.name)
        if path.is_dir():
            frozen = path.name.startswith("wave5-frozen-")
            category = "failed-attempts" if frozen else "focused"
            paths = tree_files(path, evidence)
            require(paths, "Empty wave5 directory needs explicit review")
            raw_attempts.append(classify_raw(path, evidence, frozen))
            for source in paths:
                add(source, category + "/" + path.name + "/" + source.relative_to(path).as_posix())
        else:
            require(path.suffix.lower() in RAW_SUFFIXES, "Unknown standalone wave5 evidence kind")
            raw_attempts.append(dict(tag=path.name, classification="raw_standalone_oracle_or_review", acceptedFullRun=False, countsInterpretation="Opaque bytes, never added to accepted totals"))
            add(path, "oracles-or-review/" + path.name)
    for name in TOOL_FILES:
        add(STAGING / name, "tools/" + name)
    unique_paths(item["target"] for item in items)
    require(len(items) <= MAX_FILES and sum(item["bytes"] for item in items) <= MAX_TOTAL, "Preservation plan exceeds bounds")
    require(source_manifest(workspace) == accepted["manifest"] and verify_prior_pins(workspace) == historical, "Inputs changed during planning")
    # Deterministic: apply reconstructs the entire plan before its first write.
    return dict(schemaVersion=1, operation="create_new_evidence_bundle_only", claims=CLAIMS,
                inputs=spec, toolSha256=tool_hashes(), sourceFiles=len(accepted["manifest"]),
                sourceManifestSha256=accepted["freeze"]["sourceManifestSha256"], runnerSha256=accepted["freeze"]["runnerSha256"],
                previousEvidence=historical, phases=list(PHASES),
                phaseResults={phase: {key: receipt[key] for key in ("groups", "passed", "failed", "ignored")} for phase, receipt in accepted["receipts"].items()},
                nativeMatrix=accepted["nativeMatrix"], rawEvidenceDiscovery=discovery, rawAttempts=raw_attempts,
                rawCountsAreNonadditive=True, limits=dict(maxFiles=MAX_FILES, maxTotalBytes=MAX_TOTAL, maxNodesPerTree=MAX_NODES, maxDepth=MAX_DEPTH),
                files=sorted(items, key=lambda item: item["target"]))


def create_json(path, value, boundary):
    plain(path, boundary, file=True, missing=True)
    with path.open("xb") as stream:
        stream.write(encoded(value))


def apply_plan(path, expected_digest):
    require(valid_hash(expected_digest), "Explicit root-reviewed plan hash required")
    raw = read(path, STAGING)
    require(sha(raw) == expected_digest, "Reviewed plan bytes differ")
    plan = load_json_bytes(raw)
    require(plan["toolSha256"] == tool_hashes(), "Preservation tools changed after review")
    require(encoded(build_plan(plan["inputs"])) == raw, "Source/receipts/discovery changed; no writes performed")
    workspace = Path(plan["inputs"]["workspace"])
    destination = workspace / relative_path(plan["inputs"]["destination"])
    destination.mkdir(exist_ok=False)
    hashes = {}
    # No delete, rename, overwrite, rollback or adoption of a partial output.
    for item in plan["files"]:
        data = read(workspace / relative_path(item["source"]), workspace)
        require(len(data) == item["bytes"] and sha(data) == item["sha256"], "Admitted evidence changed before copy")
        target = destination / relative_path(item["target"])
        plain(target, workspace, file=True, missing=True)
        target.parent.mkdir(parents=True, exist_ok=True)
        with target.open("xb") as stream:
            stream.write(data)
        require(sha(read(target, workspace)) == item["sha256"], "Copied evidence hash differs")
        hashes[target.relative_to(workspace).as_posix()] = item["sha256"]
    with (destination / "reviewed-plan.json").open("xb") as stream:
        stream.write(raw)
    hashes[(destination / "reviewed-plan.json").relative_to(workspace).as_posix()] = expected_digest
    require(verify_prior_pins(workspace) == plan["previousEvidence"], "Historical pins changed during copy")
    additions = sorted([*hashes, (destination / "index.json").relative_to(workspace).as_posix(), (destination / "index.sha256").relative_to(workspace).as_posix()])
    index = dict(schemaVersion=1, preservedAtUtc=datetime.datetime.now(datetime.timezone.utc).isoformat(), claims=CLAIMS,
                 acceptedTag=plan["inputs"]["acceptedTag"], acceptedPhases=list(PHASES), sourceFiles=plan["sourceFiles"],
                 sourceManifestSha256=plan["sourceManifestSha256"], runnerSha256=plan["runnerSha256"], reviewedPlanSha256=expected_digest,
                 phaseResults=plan["phaseResults"], nativeMatrix=plan["nativeMatrix"],
                 previousEvidence=plan["previousEvidence"], previousPinsVerifiedBeforeAndAfterCopy=True,
                 rawAttempts=plan["rawAttempts"], rawCountsInterpretation="Failed, focused and inventory counts never contribute to accepted totals",
                 metadataBoundary="Accepted source freeze remains authoritative; root must separately reconcile later normal metadata changes",
                 expectedPostFreezeEvidenceAdditions=additions, sha256=hashes, indexDigestFile="index.sha256")
    index_raw = encoded(index)
    with (destination / "index.json").open("xb") as stream:
        stream.write(index_raw)
    pin = (sha(index_raw) + "  index.json\n").encode("ascii")
    with (destination / "index.sha256").open("xb") as stream:
        stream.write(pin)
    require(verify_prior_pins(workspace) == plan["previousEvidence"], "Historical pins changed after index publication")
    require(all(sha(read(workspace / relative_path(name), workspace)) == value for name, value in hashes.items()), "Preserved bytes changed")
    require(read(destination / "index.json", workspace) == index_raw and read(destination / "index.sha256", workspace) == pin, "Index or sidecar differs")
    manifest_raw = read(Path(plan["inputs"]["evidenceRoot"]) / plan["inputs"]["acceptedTag"] / "source-freeze.json", workspace)
    manifest = load_json_bytes(manifest_raw)
    check_manifest(manifest)
    require(sha(manifest_raw) == sha(encoded(manifest)) == plan["sourceManifestSha256"] == plan["inputs"]["reviewedSourceSha256"], "Reviewed source baseline changed during preservation")
    after = source_manifest(workspace)
    added = {name: sha(read(workspace / relative_path(name), workspace)) for name in additions}
    require(after == manifest | {name: value for name, value in added.items() if name in after}, "Unexpected normal-file changes during preservation")
    return dict(destination=str(destination), files=len(hashes), indexSha256=sha(index_raw), priorPinsVerified=sum(row["verifiedPins"] for row in plan["previousEvidence"]), claims=CLAIMS)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="operation", required=True)
    prepare = sub.add_parser("plan", help="Validate successful receipts; create only a fresh ignored review plan")
    for name in ("workspace", "evidence-root", "accepted-tag", "reviewed-source-sha256", "reviewed-runner-sha256", "destination", "plan-out"):
        prepare.add_argument("--" + name, required=True)
    apply = sub.add_parser("apply", help="Only after root authorizes the concrete plan digest")
    apply.add_argument("--plan", required=True)
    apply.add_argument("--reviewed-plan-sha256", required=True)
    apply.add_argument("--allow-new-evidence-bundle", action="store_true", required=True)
    args = parser.parse_args()
    if args.operation == "plan":
        spec = dict(workspace=args.workspace, evidenceRoot=args.evidence_root, acceptedTag=args.accepted_tag,
                    reviewedSourceSha256=args.reviewed_source_sha256, reviewedRunnerSha256=args.reviewed_runner_sha256, destination=args.destination)
        path = Path(args.plan_out)
        require(path.is_absolute() and path.parent == STAGING, "Plan output must be a fresh direct file in ignored staging")
        plan = build_plan(spec)
        create_json(path, plan, STAGING)
        print(json.dumps(dict(plan=str(path), planSha256=sha(encoded(plan)), sourceFiles=plan["sourceFiles"], files=len(plan["files"]), actualPreservationPerformed=False)))
    else:
        print(json.dumps(apply_plan(Path(args.plan), args.reviewed_plan_sha256)))


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, ValueError, KeyError, TypeError, OSError) as error:
        print("Preservation refused: " + str(error), file=sys.stderr)
        raise SystemExit(1)
