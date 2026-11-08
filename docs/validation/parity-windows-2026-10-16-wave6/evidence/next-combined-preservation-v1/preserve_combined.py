"""Plan or create the wave6 evidence bundle; never execute acceptance code or gates."""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import types

sys.dont_write_bytecode = True
STAGING = Path(__file__).absolute().parent
EVIDENCE = STAGING.parent
ROOT = EVIDENCE.parent.parent
CORE_PATH = EVIDENCE / "next-wave5-preservation/receipt_core.py"
CORE_SHA = "35e097ea51dee414c88c68d645ad43ca6bc2d71c6335152f05107e99a22f99f3"
# Execute only these reviewed generic parser bytes, never runner or wrapper code.
for candidate in (CORE_PATH, *CORE_PATH.parents, STAGING):
    info = candidate.lstat()
    if stat.S_ISLNK(info.st_mode) or getattr(info, "st_file_attributes", 0) & 0x400:
        raise RuntimeError("Reparse preservation helper refused")
if CORE_PATH.stat().st_size > 128 * 1024:
    raise RuntimeError("Generic receipt parser exceeds bootstrap limit")
core_raw = CORE_PATH.read_bytes()
if hashlib.sha256(core_raw).hexdigest() != CORE_SHA:
    raise RuntimeError("Frozen generic receipt parser changed")
core = types.ModuleType("frozen_wave5_receipt_core")
exec(compile(core_raw, str(CORE_PATH), "exec"), core.__dict__)
require, sha, encoded = core.require, core.sha, core.encoded
read, load, plain = core.read, core.load, core.plain
relative_path, unique_paths = core.relative_path, core.unique_paths

SOURCE_SHA = "11f688628aa77bf3f50c440cd954aa49e0588800aeabb838bc17d09efe9ef30f"
RUNNER_SHA = "b51bbffbddd07be0aa6ef019fc950d772c628d77b3567f646d7a837726aa3d15"
DESTINATION = "docs/validation/parity-windows-2026-10-16-wave6"
COMBINED = "next-historical-view-comparison/revisions/combined-normal-runner-v1"
OLD = "next-historical-view-comparison/revisions/normal-root-runner-v1"
DECISION = "next-historical-view-comparison/ROOT-COMBINED-ACCEPTANCE-DECISION-V1.json"
FROZEN_TOOLS = {
    "acceptance_wave4.py": RUNNER_SHA,
    "next-wave5-preservation/receipt_core.py": CORE_SHA,
    DECISION: "63f0b106fa4b0696d0b9612b244c57f0dc749804328b043a2447fbd85d2c97cf",
    COMBINED + "/combined_common.py": "035eb4e6f0a513b5c6bc271803640678feca1c73b8015167ad7602de9b013751",
    COMBINED + "/run_h18_combined.py": "bfb42228fd8a8711366d945acb8ccfd0a0660c457547962dafda8cdca10f3ab3",
    COMBINED + "/run_combined_windows_acceptance.py": "70a8fff46d663f1481bbe4d5875e76d0a6193e747cd532a97a661d496aa23ebc",
    COMBINED + "/H18-EXPECTED-TESTS.json": "0c21b1e45653b00393243efeb8ffcef5c1741b863f2283ad739fc2aa2e59b7d2",
    COMBINED + "/PREPARATION-MANIFEST.json": "e22a44cb06be1c3a67f0232275d0ffa48a20e43f72664ee34696f1e80ebfeeef",
    COMBINED + "/STATIC-REVIEW.json": "ec07f200ca2ebe8379298c09a57abee1aa912b3ccb2c45c9e3b7aa6c095eef06",
    OLD + "/PREPARATION-MANIFEST.json": "1af60d5b080aff516c328ab694181876c72b13fb33030ec8f0a09d0a58aed494",
    OLD + "/STATIC-REVIEW.json": "40589aae5bfbf71eb3d8ce03c70bb67e7735a3fdbb604f7fcf44de85883908ea",
}
CONTROL_PINS = {
    "cache-origin-admission-v4/cache_origin.py": "a24fd7ab1c40c0c815117878f16be97760076a6efffd56479386cc363d4865db",
    "next-git-impact-details/revisions/normal-final-promotion-v3/apply_normal.py": "64be3d9a6e91808807abaaf28e0e3abdb391e28ece0ee27a5beff94a8a2a71b9",
    "next-git-impact-details/ROOT-FINAL-NORMAL-GATE-BINDINGS-V3.json": "8b18b29828c3e3e8151340e5543cc2e2854b3c53740a3ae178967f7b5f0764f1",
    "next-git-impact-details/reviews/normal-final-promotion-v3-cli-oracle-static-review-v1.json": "c9e850bb1775e7d9d0df80110646a7ff67ff44ba523f2ddd84be08c602e37499",
}
PRIOR = (
    ("parity-windows-2026-10-15", "156852befe63b51cd99d66f9b9a857157659715f31ffb0a653720373677d91a0"),
    ("parity-windows-2026-10-15-wave2", "0e2e504fb30ec388ae46efe53dd4639de55926753ffc8b9b5da74fc55c71633e"),
    ("parity-windows-2026-10-15-wave3", "98646f7e98060d475129f4db26e353f37f57059658f639382888a20240208e97"),
    ("parity-windows-2026-10-15-wave4", "1b829007c444e02be250626ca0e418830932d478a49ad1c88dc828eb3a72acbf"),
    ("parity-windows-2026-10-16-wave5", "377ae26fc17d845f8279d092687833023e4969b7215a4216c97cbebdb4155fd4"),
)
TOOLS = ("preserve_combined.py", "README.md", "smoke_preservation.py")
STAGES = ("format", "clippy", "python", "typescript", "native", "finalize")
TARGETS = ("selected_view_analysis", "protocol_connection", "meaning_view_archives",
           "registry_replay_review", "collection_registry_access", "graph_view")
BASE = ["cargo", "test", "--workspace", "--locked", "-j", "2", "--no-fail-fast"]
CLAIMS = dict(universalParity=False, releaseCertified=False,
              scope="Unreleased Windows development checkpoint; deferred platforms and capabilities remain open")
MAX_NODES, MAX_DEPTH = 20_000, 64


def source_manifest():
    """Full current Git-visible normal map; no acceptance runner import."""
    raw = subprocess.check_output(["rtk", "proxy", "git", "ls-files", "-c", "-o", "--exclude-standard", "-z"], cwd=ROOT)
    require(len(raw) <= 8 * 1024 * 1024, "Git inventory exceeds byte limit")
    names = sorted(set(raw.decode("utf-8").split("\0")) - {""})
    require(0 < len(names) <= 20_000, "Normal inventory exceeds file limit")
    unique_paths(names)
    result, total = {}, 0
    for name in names:
        data = read(ROOT / relative_path(name), ROOT)
        total += len(data)
        require(total <= 1024 * 1024 * 1024, "Normal inventory exceeds byte budget")
        result[name] = sha(data)
    core.check_manifest(result)
    return result


def prior_indexes():
    """Only re-pin the five already accepted index files, never their artifact lists."""
    result = []
    for name, expected in PRIOR:
        relative = "docs/validation/" + name + "/index.json"
        require(sha(read(ROOT / relative_path(relative), ROOT)) == expected, "Historical index changed: " + name)
        result.append(dict(path=relative, sha256=expected, checkedScope="index_bytes_only", historicalArtifactsReaudited=False))
    return result


def entries(directory, boundary=EVIDENCE):
    plain(directory, boundary.parent if directory == boundary else boundary)
    result = []
    with os.scandir(directory) as scan:
        for entry in scan:
            require(len(result) < MAX_NODES, "Directory node budget exhausted")
            result.append(Path(entry.path))
    return sorted(result)


def tree_files(directory, boundary=EVIDENCE, allowed=(".json", ".log", ".py")):
    """Finite plain evidence trees; executable/source/cache payloads are refused."""
    pending, files, nodes, total = [(directory, 0)], [], 0, 0
    while pending:
        parent, depth = pending.pop()
        for path in entries(parent, boundary):
            nodes += 1
            require(nodes <= MAX_NODES and depth <= MAX_DEPTH, "Evidence tree node/depth budget exhausted")
            if path.is_dir():
                plain(path, boundary)
                require(depth + 1 <= MAX_DEPTH, "Evidence tree depth budget exhausted")
                pending.append((path, depth + 1))
            else:
                require(allowed is None or path.suffix.lower() in allowed, "Unexpected gate-tree file kind")
                data = read(path, boundary)
                total += len(data)
                require(len(files) < core.MAX_FILES and total <= core.MAX_TOTAL, "Evidence tree file/byte budget exhausted")
                files.append(path)
    unique_paths(path.relative_to(directory).as_posix() for path in files)
    return sorted(files)


def tool_hashes():
    return {name: sha(read(STAGING / name, STAGING)) for name in TOOLS} | {"receipt_core.py": CORE_SHA}


def ignored_additions(names):
    """Bind Git's ignore partition before output exists; never infer it from after."""
    unique_paths(names)
    data = ("\0".join(names) + "\0").encode("utf-8")
    require(len(data) <= 8 * 1024 * 1024, "Ignore inventory input budget exhausted")
    result = subprocess.run(["rtk", "proxy", "git", "check-ignore", "--no-index", "--stdin", "-z"],
                            cwd=ROOT, input=data, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    require(result.returncode in (0, 1) and not result.stderr and len(result.stdout) <= len(data), "Git ignore inventory failed")
    ignored = sorted(set(result.stdout.decode("utf-8").split("\0")) - {""})
    require(set(ignored).issubset(names), "Unexpected ignored output name")
    return ignored


def context_fields(context, pin, manifest):
    return dict(reviewedContextPath=pin["path"], reviewedContextSha256=pin["sha256"],
                sourceManifestSha256=SOURCE_SHA, promotionReceiptSha256=context["promotionReceipt"]["sha256"],
                promotionManifestSha256=context["promotionManifest"]["sha256"],
                normalMainSha256=manifest["native/src/main.rs"], normalLibSha256=manifest["native/src/lib.rs"],
                normalCacheOriginReceiptSha256=context["normalCacheReceipt"]["sha256"],
                imageGateOriginsReviewed=[{key: gate[key] for key in ("name", "role")} |
                                          {"receiptSha256": gate["receipt"]["sha256"]} for gate in context["imageGates"]],
                cacheOriginQuarantineInvokedByWrapper=False, artifactProvenanceInferredFromDepMetadata=False)


def checked_h18(path, manifest, context, context_pin):
    receipt = load(path / "receipt.json", EVIDENCE)
    require(receipt["scope"] == "combined_normal_H18_six_old_targets"
            and receipt["accepted"] is True and receipt["origin"] == "fresh_execution"
            and type(receipt["exitCode"]) is int and receipt["exitCode"] == 0
            and receipt["sourceChanges"] == receipt["validationErrors"] == [], "H18 is not clean fresh success")
    expected_files = {"receipt.json", *receipt["artifactSha256"]}
    require({p.relative_to(path).as_posix() for p in tree_files(path)} == expected_files, "H18 artifact closure differs")
    for name, expected in receipt["artifactSha256"].items():
        require(len(relative_path(name).parts) == 1 and sha(read(path / name, EVIDENCE)) == expected, "H18 artifact hash differs")
    require(load(path / "source-before.json", EVIDENCE) == load(path / "source-after.json", EVIDENCE) == manifest, "H18 source maps differ")
    require(all(receipt.get(key) == value for key, value in context_fields(context, context_pin, manifest).items()), "H18 reviewed context differs")
    for key, tool in (("wrapperSha256", "run_h18_combined.py"), ("commonHelperSha256", "combined_common.py"), ("testNameOracleSha256", "H18-EXPECTED-TESTS.json")):
        require(receipt[key] == FROZEN_TOOLS[COMBINED + "/" + tool], "H18 frozen helper/oracle differs")
    require(receipt["frozenRunnerSha256"] == RUNNER_SHA, "H18 frozen runner differs")
    golden = load(EVIDENCE / COMBINED / "H18-EXPECTED-TESTS.json", EVIDENCE)["targets"]
    require(set(golden) == set(TARGETS) and len(receipt["results"]) == 3, "H18 target/batch coverage differs")
    actual = {}
    for offset, result in enumerate(receipt["results"]):
        selected, label = TARGETS[offset * 2:offset * 2 + 2], "h18-" + str(offset).zfill(2)
        command = BASE + [part for target in selected for part in ("--test", target)] + ["--", "--test-threads=4"]
        raw = read(path / (label + ".log"), EVIDENCE)
        require(result["label"] == label and result["targets"] == list(selected) and result["command"] == command
                and result["origin"] == "fresh_execution" and type(result["exitCode"]) is int and result["exitCode"] == 0
                and result["rawSha256"] == sha(raw), "H18 command/raw origin differs")
        actual.update(core.group_map(core.harness_groups(raw, False), selected))
    require(actual == receipt["actualGroups"] and set(actual) == set(TARGETS), "H18 raw groups differ")
    for target, group in actual.items():
        require(set(group["rows"]) == set(golden[target]) and set(group["rows"].values()) == {"ok"}
                and group["passed"] == len(golden[target])
                and all(group[key] == 0 for key in ("failed", "ignored", "filtered", "measured")), "H18 exact named results differ")
    counts = dict(groups=len(actual), **{key: sum(group[key] for group in actual.values()) for key in ("passed", "failed", "ignored")})
    require(all(type(receipt[key]) is int and receipt[key] == value for key, value in counts.items()), "H18 raw totals differ")
    return counts


def checked_pipeline(path, full, h18, context, context_pin, manifest):
    receipt = load(path / "pipeline.json", EVIDENCE)
    require(receipt["scope"] == "combined_normal_sequential_orchestration_only_use_phase_receipts"
            and receipt["accepted"] is True and type(receipt["exitCode"]) is int and receipt["exitCode"] == 0
            and receipt["sourceChanges"] == receipt["validationErrors"] == []
            and receipt["runnerSha256"] == RUNNER_SHA
            and receipt["pipelineSha256"] == FROZEN_TOOLS[COMBINED + "/run_combined_windows_acceptance.py"]
            and receipt["commonHelperSha256"] == FROZEN_TOOLS[COMBINED + "/combined_common.py"]
            and receipt["reviewedContextPath"] == context_pin["path"]
            and receipt["reviewedContextSha256"] == context_pin["sha256"]
            and receipt["sourceManifestSha256"] == SOURCE_SHA, "Combined pipeline is not finalized clean success on reviewed context")
    require(all(receipt.get(key) == value for key, value in context_fields(context, context_pin, manifest).items()), "Pipeline reviewed context differs")
    require({p.relative_to(path).as_posix() for p in tree_files(path)} == {"pipeline.json", "pipeline.log", "source-before.json", "source-after.json"}, "Pipeline file closure differs")
    for label in ("before", "after"):
        raw = read(path / ("source-" + label + ".json"), EVIDENCE)
        require(core.load_json_bytes(raw) == manifest and sha(raw) == receipt["source" + label.title() + "Sha256"], "Pipeline source maps differ")
    require(sha(read(path / "pipeline.log", EVIDENCE)) == receipt["pipelineLogSha256"]
            and sha(read(h18 / "receipt.json", EVIDENCE)) == receipt["h18ReceiptSha256"], "Pipeline log/H18 pins differ")
    require(len(receipt["stages"]) == len(STAGES), "Pipeline stage coverage differs")
    for stage, row in zip(STAGES, receipt["stages"]):
        require(row["stage"] == stage and type(row["exitCode"]) is int and row["exitCode"] == 0 and row["accepted"] is True, "Pipeline stage failed or differs")
        command = row["command"]
        require(isinstance(command, list) and command[:3] == [sys.executable, "-X", "utf8"]
                and command[3:7] == [str(EVIDENCE / "acceptance_wave4.py"), stage, "--tag", full.name], "Pipeline did not use original runner/tag")
        tail = [] if stage == "finalize" else ["--temp", str(Path("D:/Yoke-Test-Scratch") / (full.name + "-" + stage))]
        if stage == "native":
            tail += ["--acl-temp", str(Path("C:/Users/Public/Yoke-Test-Scratch") / (full.name + "-acl"))]
        require(command[7:] == tail, "Pipeline fixture roots/extra command arguments differ")
        target = full / ("summary.json" if stage == "finalize" else stage + "/receipt.json")
        require(sha(read(target, EVIDENCE)) == row["receiptSha256"], "Pipeline stage receipt hash differs")
        actual = load(target, EVIDENCE)
        require(all(row[key] == actual.get(key) for key in ("accepted", "passed", "failed", "ignored", "groups", "sourceChanges", "validationErrors")), "Pipeline stage receipt fields differ")
    return dict(accepted=True, orchestrationOnly=True, countsAreNonadditive=True, stages=list(STAGES))


def context_pins(context):
    """Follow only explicit pins in this context, never the contents of pinned JSON."""
    pins = []
    def visit(value, depth=0):
        require(depth <= 16, "Context nesting budget exhausted")
        if isinstance(value, dict):
            if set(value) == {"path", "sha256"}:
                require(core.valid_hash(value["sha256"]), "Invalid context evidence pin")
                pins.append(value)
                require(len(pins) <= 512, "Context pin budget exhausted")
            else:
                for item in value.values():
                    visit(item, depth + 1)
        elif isinstance(value, list):
            for item in value:
                visit(item, depth + 1)
    visit(context)
    return pins


def build_plan(spec):
    require(set(spec) == {"tagPrefix", "reviewedContext", "reviewedContextSha256"}, "Unknown plan input")
    require(os.name == "nt" and ROOT.name == "Code-Intelligence" and ROOT.resolve(strict=True) == ROOT, "Canonical normal Windows checkout required")
    plain(STAGING, ROOT)
    require(re.fullmatch(r"combined-r3-r5-[a-z0-9][a-z0-9-]{0,64}", spec["tagPrefix"]) is not None, "Concrete bounded combined tag required")
    destination = ROOT / relative_path(DESTINATION)
    require(not destination.exists() and not destination.is_symlink(), "Destination already exists; partial outputs are never adopted")
    plain(destination, ROOT, missing=True)
    full, h18, pipeline = [EVIDENCE / (spec["tagPrefix"] + suffix) for suffix in ("-full", "-h18", "-pipeline")]
    accepted = core.validate_bundle(full, SOURCE_SHA, RUNNER_SHA, ROOT)
    manifest = accepted["manifest"]
    require(source_manifest() == manifest, "Current normal files differ from accepted freeze")
    historical = prior_indexes()
    items, total = {}, 0
    def add(path, expected=None):
        nonlocal total
        plain(path, EVIDENCE, file=True)
        name = path.relative_to(ROOT).as_posix()
        if name in items:
            require(expected is None or items[name]["sha256"] == expected, "Conflicting duplicate evidence pins")
            return
        data = read(path, EVIDENCE)
        digest = sha(data)
        require(expected is None or digest == expected, "Bound evidence pin changed: " + name)
        total += len(data)
        require(len(items) < core.MAX_FILES and total <= core.MAX_TOTAL, "Whole preservation file/byte budget exhausted")
        target = "evidence/" + path.relative_to(EVIDENCE).as_posix()
        items[name] = dict(source=name, target=target, bytes=len(data), sha256=digest)
    for name, expected in (FROZEN_TOOLS | CONTROL_PINS).items():
        add(EVIDENCE / relative_path(name), expected)
    # Only the bounded tooling artifact fields, without chasing historical sourcePins.
    for directory in (OLD, COMBINED):
        preparation = load(EVIDENCE / directory / "PREPARATION-MANIFEST.json", EVIDENCE)
        require(isinstance(preparation["artifacts"], dict) and len(preparation["artifacts"]) <= 16, "Runner tool closure exceeds limit")
        for name, pin in preparation["artifacts"].items():
            require(len(relative_path(name).parts) == 1 and core.valid_hash(pin["sha256"]), "Invalid tool artifact pin")
            add(EVIDENCE / directory / name, pin["sha256"])
    context_path = ROOT / relative_path(spec["reviewedContext"])
    require(core.valid_hash(spec["reviewedContextSha256"]), "Explicit reviewed context digest required")
    context_pin = dict(path=spec["reviewedContext"], sha256=spec["reviewedContextSha256"])
    add(context_path, context_pin["sha256"])
    context = load(context_path, EVIDENCE)
    require(context["scope"] == "root_reviewed_combined_r3_r5_normal_acceptance" and context["approved"] is True
            and context["expectedMap"]["sha256"] == SOURCE_SHA and context["sourceFiles"] == len(manifest), "Wrong reviewed root context")
    pins = context_pins(context)
    for pin in pins:
        add(ROOT / relative_path(pin["path"]), pin["sha256"])
    require(load(ROOT / relative_path(context["expectedMap"]["path"]), EVIDENCE) == manifest, "Context expected full map differs")
    h18_counts = checked_h18(h18, manifest, context, context_pin)
    pipeline_result = checked_pipeline(pipeline, full, h18, context, context_pin, manifest)
    expected = set((*core.TOP_FILES, "summary.json"))
    for phase, receipt in accepted["receipts"].items():
        expected.update(phase + "/" + name for name in ("receipt.json", *receipt["artifactSha256"]))
    require({p.relative_to(full).as_posix() for p in tree_files(full)} == expected, "Accepted full bundle contains unpinned/missing files")
    for directory in (full, h18, pipeline):
        for path in tree_files(directory):
            add(path)
    focused = []
    for path in entries(EVIDENCE):
        if not path.name.startswith(("r3-", "r5-")):
            continue
        if path.is_dir():
            files = tree_files(path)
            require(files, "Empty focused evidence tree needs review")
            outcome = {}
            receipt = path / "receipt.json"
            if receipt.exists():
                value = load(receipt, EVIDENCE)
                outcome = {key: value.get(key) for key in ("scope", "accepted", "exitCode", "passed", "failed", "ignored", "sourceChanges", "validationErrors")}
            for source in files:
                add(source)
        else:
            require(path.suffix.lower() in (".json", ".log"), "Unexpected direct focused evidence kind")
            files, outcome = [path], {}
            add(path)
        focused.append(dict(tag=path.name, files=len(files), recordedOutcome=outcome,
                            classification="historical_focused_attempt_not_full_acceptance", countsAreNonadditive=True))
    for name in TOOLS:
        add(STAGING / name)
    unique_paths(item["target"] for item in items.values())
    additions = [DESTINATION + "/" + item["target"] for item in items.values()]
    additions += [DESTINATION + "/" + name for name in ("reviewed-plan.json", "index.json", "index.sha256")]
    ignored = ignored_additions(additions)
    require(source_manifest() == manifest and prior_indexes() == historical, "Normal source or historical indexes changed during planning")
    return dict(schemaVersion=1, operation="create_wave6_evidence_only", inputs=spec, destination=DESTINATION,
                claims=CLAIMS, toolSha256=tool_hashes(), normalBefore=manifest, sourceManifestSha256=SOURCE_SHA,
                sourceFiles=len(manifest), runnerSha256=RUNNER_SHA, previousEvidence=historical,
                historicalArtifactListsReaudited=False, acceptedTag=full.name,
                acceptedPhases=list(core.PHASES),
                phaseResults={phase: {key: receipt[key] for key in ("groups", "passed", "failed", "ignored")} for phase, receipt in accepted["receipts"].items()},
                nativeMatrix=accepted["nativeMatrix"], h18=dict(tag=h18.name, **h18_counts, countsAreNonadditive=True),
                pipeline=dict(tag=pipeline.name, **pipeline_result), focusedAttempts=focused,
                contextClosure=dict(explicitPins=len(pins), referencedJsonRecursivelyFollowed=False,
                                    imageSourceTreesCopied=False, cacheMovePayloadsCopied=False),
                controlPins=CONTROL_PINS,
                gitIgnoredAdditions=ignored, rawCountsAreNonadditive=True,
                files=sorted(items.values(), key=lambda item: item["target"]))


def exclusive_write(path, raw, boundary):
    plain(path, boundary, file=True, missing=True)
    with path.open("xb") as stream:
        stream.write(raw)
        stream.flush()
        os.fsync(stream.fileno())


def append_journal(stream, record):
    stream.write((json.dumps(record, ensure_ascii=False, sort_keys=True) + "\n").encode("utf-8"))
    stream.flush()
    os.fsync(stream.fileno())


def apply_plan(path, expected):
    require(core.valid_hash(expected), "Root-reviewed exact plan digest required")
    plain(path, STAGING, file=True)
    raw = read(path, STAGING)
    require(sha(raw) == expected, "Approved plan bytes differ")
    plan = core.load_json_bytes(raw)
    require(plan["toolSha256"] == tool_hashes(), "Preservation helper changed after review")
    require(encoded(build_plan(plan["inputs"])) == raw, "Rebuilt plan differs; no writes performed")
    destination = ROOT / relative_path(DESTINATION)
    journal_path = STAGING / ("application-" + expected + ".jsonl")
    plain(journal_path, STAGING, file=True, missing=True)
    with journal_path.open("xb") as journal:
        append_journal(journal, dict(operation="begin", planSha256=expected, destination=DESTINATION,
                                     normalBeforeSha256=SOURCE_SHA, normalWrites="new evidence directory only"))
        try:
            append_journal(journal, dict(inFlight=DESTINATION, kind="directory"))
            destination.mkdir(exist_ok=False)
            hashes = {}
            for item in plan["files"]:
                data = read(ROOT / relative_path(item["source"]), EVIDENCE)
                require(len(data) == item["bytes"] and sha(data) == item["sha256"], "Admitted evidence changed before copy")
                target = destination / relative_path(item["target"])
                append_journal(journal, dict(inFlight=target.relative_to(ROOT).as_posix(), sha256=item["sha256"]))
                plain(target, ROOT, file=True, missing=True)
                target.parent.mkdir(parents=True, exist_ok=True)
                exclusive_write(target, data, ROOT)
                require(sha(read(target, ROOT)) == item["sha256"], "Copied evidence hash differs")
                hashes[target.relative_to(ROOT).as_posix()] = item["sha256"]
                append_journal(journal, dict(complete=target.relative_to(ROOT).as_posix()))
            generated = {"reviewed-plan.json": raw}
            hashes[DESTINATION + "/reviewed-plan.json"] = expected
            additions = sorted([*hashes, DESTINATION + "/index.json", DESTINATION + "/index.sha256"])
            index = dict(schemaVersion=1, preservedAtUtc=datetime.datetime.now(datetime.timezone.utc).isoformat(),
                         claims=CLAIMS, acceptedTag=plan["acceptedTag"], acceptedPhases=plan["acceptedPhases"],
                         sourceManifestSha256=SOURCE_SHA, sourceFiles=plan["sourceFiles"], runnerSha256=RUNNER_SHA,
                         reviewedPlanSha256=expected, phaseResults=plan["phaseResults"], nativeMatrix=plan["nativeMatrix"],
                         h18=plan["h18"], pipeline=plan["pipeline"], previousEvidence=plan["previousEvidence"],
                         historicalArtifactListsReaudited=False, focusedAttempts=plan["focusedAttempts"],
                         contextClosure=plan["contextClosure"], rawCountsAreNonadditive=True,
                         controlPins=plan["controlPins"],
                         gitIgnoredAdditions=plan["gitIgnoredAdditions"],
                         countsInterpretation="Five accepted fresh phase receipts only; H18, focused attempts, inventories and repeated gate roles never add to their totals",
                         metadataBoundary="Root may separately update concise repository status after this evidence-only operation",
                         expectedPostFreezeEvidenceAdditions=additions, sha256=hashes, indexDigestFile="index.sha256")
            generated["index.json"] = encoded(index)
            generated["index.sha256"] = (sha(generated["index.json"]) + "  index.json\n").encode("ascii")
            for name, data in generated.items():
                append_journal(journal, dict(inFlight=DESTINATION + "/" + name, sha256=sha(data)))
                exclusive_write(destination / name, data, ROOT)
                require(read(destination / name, ROOT) == data, "Generated evidence file differs")
                append_journal(journal, dict(complete=DESTINATION + "/" + name))
            require(prior_indexes() == plan["previousEvidence"], "Historical index changed during copying")
            expected_additions = hashes | {DESTINATION + "/" + name: sha(data) for name, data in generated.items()}
            # Every output, including Git-ignored raw logs, must exist with no extra file.
            actual_outputs = {path.relative_to(ROOT).as_posix(): sha(read(path, ROOT))
                              for path in tree_files(destination, ROOT, allowed=None)}
            require(actual_outputs == expected_additions, "Missing, changed or unplanned destination evidence")
            require(ignored_additions(sorted(expected_additions)) == plan["gitIgnoredAdditions"], "Planned ignore partition changed")
            require(tool_hashes() == plan["toolSha256"], "Preservation tools changed during application")
            require(all(sha(read(ROOT / relative_path(item["source"]), EVIDENCE)) == item["sha256"] for item in plan["files"]), "Original evidence changed during application")
            after = source_manifest()
            require(not set(expected_additions).intersection(plan["normalBefore"]), "Evidence additions collide with before map")
            visible_additions = {name: digest for name, digest in expected_additions.items() if name not in plan["gitIgnoredAdditions"]}
            require(after == plan["normalBefore"] | visible_additions, "Unexpected normal changes or missing planned Git-visible additions")
            result = dict(complete=True, destination=DESTINATION, files=len(expected_additions),
                          indexSha256=sha(generated["index.json"]), normalAfterSha256=sha(encoded(after)),
                          historicalIndexesHashCheckedOnly=len(PRIOR), claims=CLAIMS)
            append_journal(journal, result)
            return result
        except Exception as error:
            append_journal(journal, dict(complete=False, error=type(error).__name__ + ": " + str(error),
                                         partialOutputRetained=True, rollbackPerformed=False))
            raise


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="operation", required=True)
    plan = commands.add_parser("plan", help="Read finalized receipts and write a fresh ignored review plan")
    for name in ("tag-prefix", "reviewed-context", "reviewed-context-sha256", "plan-out"):
        plan.add_argument("--" + name, required=True)
    apply = commands.add_parser("apply", help="Create only the approved new evidence directory")
    apply.add_argument("--plan", required=True)
    apply.add_argument("--reviewed-plan-sha256", required=True)
    apply.add_argument("--allow-new-evidence-bundle", action="store_true", required=True)
    args = parser.parse_args()
    if args.operation == "plan":
        path = Path(args.plan_out)
        require(path.is_absolute() and path.parent == STAGING and re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_.-]{0,100}\.json", path.name), "Plan must be a fresh direct staging JSON file")
        value = build_plan(dict(tagPrefix=args.tag_prefix, reviewedContext=args.reviewed_context,
                                reviewedContextSha256=args.reviewed_context_sha256))
        data = encoded(value)
        exclusive_write(path, data, STAGING)
        print(json.dumps(dict(plan=str(path), planSha256=sha(data), files=len(value["files"]), normalWrites=False)))
    else:
        print(json.dumps(apply_plan(Path(args.plan), args.reviewed_plan_sha256)))


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, ValueError, KeyError, TypeError, OSError) as error:
        print("Preservation refused: " + str(error), file=sys.stderr)
        raise SystemExit(1)
