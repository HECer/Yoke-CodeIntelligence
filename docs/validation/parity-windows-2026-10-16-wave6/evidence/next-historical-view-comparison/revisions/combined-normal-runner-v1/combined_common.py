"""Root-only combined normal acceptance admission; import executes no gate."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path, PurePosixPath
import re
import stat
import subprocess

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[4]
EVIDENCE = ROOT / "target/parity-validation-20261015"
R3 = EVIDENCE / "next-historical-view-comparison"
RUNNER = EVIDENCE / "acceptance_wave4.py"
RUNNER_SHA = "b51bbffbddd07be0aa6ef019fc950d772c628d77b3567f646d7a837726aa3d15"
DECISION = R3 / "ROOT-COMBINED-ACCEPTANCE-DECISION-V1.json"
DECISION_SHA = "63f0b106fa4b0696d0b9612b244c57f0dc749804328b043a2447fbd85d2c97cf"
OLD = R3 / "revisions/normal-root-runner-v1"
OLD_MANIFEST_SHA = "1af60d5b080aff516c328ab694181876c72b13fb33030ec8f0a09d0a58aed494"
OLD_REVIEW_SHA = "40589aae5bfbf71eb3d8ce03c70bb67e7735a3fdbb604f7fcf44de85883908ea"
TARGETS = ("selected_view_analysis", "protocol_connection", "meaning_view_archives",
           "registry_replay_review", "collection_registry_access", "graph_view")
BASE = ["cargo", "test", "--workspace", "--locked", "-j", "2", "--no-fail-fast"]
PHASES = ("format", "clippy", "python", "typescript", "native", "finalize")
SHA = re.compile(r"[0-9a-f]{64}\Z")


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def encoded(value):
    return (json.dumps(value, indent=2, ensure_ascii=False, sort_keys=True) + "\n").encode("utf-8")


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def no_reparse(path):
    for candidate in (path, *path.parents):
        try:
            info = candidate.lstat()
        except FileNotFoundError:
            continue
        require(not stat.S_ISLNK(info.st_mode)
                and not getattr(info, "st_file_attributes", 0) & stat.FILE_ATTRIBUTE_REPARSE_POINT,
                "Reparse input/output path refused")
    return path


def file_hash(path, maximum=128 * 1024 * 1024):
    no_reparse(path)
    before = path.lstat()
    require(stat.S_ISREG(before.st_mode) and before.st_size <= maximum, "Input kind/byte limit")
    result, size = hashlib.sha256(), 0
    identity = lambda item: (item.st_dev, item.st_ino, item.st_size, item.st_mtime_ns)
    with path.open("rb") as stream:
        require(identity(os.fstat(stream.fileno())) == identity(before), "Input changed on open")
        while chunk := stream.read(64 * 1024):
            size += len(chunk)
            require(size <= maximum, "Input grew beyond byte limit")
            result.update(chunk)
        final = os.fstat(stream.fileno())
    require(identity(before) == identity(final) == identity(path.lstat()) and size == before.st_size,
            "Input changed during read")
    return result.hexdigest()


def unique_object(pairs):
    value = {}
    for key, item in pairs:
        require(key not in value, "Duplicate JSON property")
        value[key] = item
    return value


def load_json(path, maximum=8 * 1024 * 1024):
    no_reparse(path)
    require(path.stat().st_size <= maximum, "JSON byte limit")
    return json.loads(path.read_text(encoding="utf-8"), object_pairs_hook=unique_object)


def write_json(path, value):
    require(path.is_relative_to(EVIDENCE), "Output escaped evidence root")
    no_reparse(path)
    with path.open("xb") as stream:
        stream.write(encoded(value))


def relative_path(name):
    require(isinstance(name, str), "Expected relative path string")
    relative = PurePosixPath(name)
    require(relative.parts and not relative.is_absolute() and relative.as_posix() == name
            and all(part not in (".", "..") and ":" not in part and "\\" not in part
                    for part in relative.parts), "Unsafe relative path")
    return relative


def evidence_path(name):
    path = ROOT.joinpath(*relative_path(name).parts)
    require(path.is_relative_to(EVIDENCE), "Input must be contained in ignored acceptance evidence")
    return no_reparse(path)


def pinned(item, inputs):
    require(isinstance(item, dict) and set(item) == {"path", "sha256"}
            and isinstance(item["sha256"], str) and SHA.fullmatch(item["sha256"]), "Invalid evidence pin")
    path = evidence_path(item["path"])
    require(file_hash(path) == item["sha256"], "Evidence SHA differs: " + item["path"])
    require(path not in inputs or inputs[path] == item["sha256"], "Conflicting evidence pins")
    inputs[path] = item["sha256"]
    return path


def source_manifest():
    raw = subprocess.check_output(
        ["rtk", "proxy", "git", "ls-files", "-c", "-o", "--exclude-standard", "-z"], cwd=ROOT)
    require(len(raw) <= 8 * 1024 * 1024, "Git inventory byte limit")
    names = sorted(set(name for name in raw.decode("utf-8").split("\0") if name))
    require(0 < len(names) <= 10000, "Git inventory file limit")
    output, total = {}, 0
    for name in names:
        path = no_reparse(ROOT.joinpath(*relative_path(name).parts))
        require(path.is_file(), "Missing/non-file Git-visible source")
        total += path.stat().st_size
        require(total <= 512 * 1024 * 1024, "Source total byte limit")
        output[name] = file_hash(path, 16 * 1024 * 1024)
    return output


def parse_arguments():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("expected-map", "expected-map-sha256", "promotion-receipt", "promotion-receipt-sha256",
                 "reviewed-context", "reviewed-context-sha256", "tag-prefix"):
        parser.add_argument("--" + name, required=True)
    args = parser.parse_args()
    require(re.fullmatch(r"combined-[a-z0-9][a-z0-9-]{0,64}", args.tag_prefix), "Unsafe/far-too-long tag prefix")
    return args


class Context:
    def verify(self):
        require(file_hash(Path(__file__)) == self.common_sha, "Combined helper changed")
        require(file_hash(RUNNER) == RUNNER_SHA and file_hash(DECISION) == DECISION_SHA,
                "Frozen runner/root scheduling decision changed")
        require(file_hash(OLD / "PREPARATION-MANIFEST.json") == OLD_MANIFEST_SHA,
                "Old frozen normal tooling changed")
        require(file_hash(OLD / "STATIC-REVIEW.json") == OLD_REVIEW_SHA, "Old frozen static review changed")
        old = load_json(OLD / "PREPARATION-MANIFEST.json")
        for name, pin in old["artifacts"].items():
            require(file_hash(OLD / name) == pin["sha256"], "Old frozen normal artifact changed")
        for path, expected in self.inputs.items():
            require(file_hash(path) == expected, "Reviewed context/evidence changed: " + str(path))

    def check_source(self):
        value = source_manifest()
        require(value == self.expected and digest(encoded(value)) == self.source_sha,
                "Normal source differs from the reviewed final combined map")
        return value

    def receipt_context(self):
        return {"reviewedContextPath": self.context_relative, "reviewedContextSha256": self.context_sha,
                "sourceManifestSha256": self.source_sha, "promotionReceiptSha256": self.promotion_receipt_sha,
                "promotionManifestSha256": self.promotion_sha,
                "normalMainSha256": self.expected["native/src/main.rs"],
                "normalLibSha256": self.expected["native/src/lib.rs"],
                "normalCacheOriginReceiptSha256": self.cache_receipt_sha,
                "imageGateOriginsReviewed": self.gate_roles,
                "cacheOriginQuarantineInvokedByWrapper": False,
                "artifactProvenanceInferredFromDepMetadata": False}


def admit(args, common_sha):
    require(os.name == "nt" and ROOT.name == "Code-Intelligence"
            and (ROOT / "native/Cargo.toml").is_file(), "Unexpected normal Windows checkout")
    ctx = Context()
    ctx.common_sha, ctx.inputs = common_sha, {}
    descriptor = lambda name, value: {"path": name, "sha256": value}
    ctx.context_relative, ctx.context_sha = args.reviewed_context, args.reviewed_context_sha256
    context_path = pinned(descriptor(args.reviewed_context, args.reviewed_context_sha256), ctx.inputs)
    review = load_json(context_path)
    require(set(review) == {"schemaVersion", "scope", "approved", "expectedMap", "sourceFiles", "promotionReceipt",
                           "promotionManifest", "sourcePins", "imageGates", "normalCachePlan", "normalCacheReceipt"}
            and review["schemaVersion"] == 1 and review["scope"] == "root_reviewed_combined_r3_r5_normal_acceptance"
            and review["approved"] is True, "Missing/unknown combined review context")
    require(review["expectedMap"] == descriptor(args.expected_map, args.expected_map_sha256)
            and review["promotionReceipt"] == descriptor(args.promotion_receipt, args.promotion_receipt_sha256),
            "Explicit caller map/promotion pins differ from reviewed context")
    expected_path = pinned(review["expectedMap"], ctx.inputs)
    ctx.expected = load_json(expected_path)
    ctx.source_sha = args.expected_map_sha256
    require(isinstance(ctx.expected, dict) and 0 < len(ctx.expected) <= 10000
            and type(review["sourceFiles"]) is int and len(ctx.expected) == review["sourceFiles"]
            and all(isinstance(value, str) and SHA.fullmatch(value) for value in ctx.expected.values())
            and digest(encoded(ctx.expected)) == ctx.source_sha, "Final source-map identity/encoding differs")
    for name in ctx.expected:
        relative_path(name)
    mandatory = {"native/src/main.rs", "native/src/lib.rs", "Cargo.toml", "Cargo.lock", "native/Cargo.toml",
                 *["native/tests/" + target + ".rs" for target in TARGETS]}
    require(isinstance(review["sourcePins"], dict) and mandatory.issubset(review["sourcePins"])
            and all(name in ctx.expected and isinstance(pin, str) and SHA.fullmatch(pin)
                    and ctx.expected[name] == pin for name, pin in review["sourcePins"].items()),
            "Explicit final main/lib/Cargo/H18 source context differs")
    promotion_path = pinned(review["promotionReceipt"], ctx.inputs)
    pinned(review["promotionManifest"], ctx.inputs)
    ctx.promotion_receipt_sha = args.promotion_receipt_sha256
    ctx.promotion_sha = review["promotionManifest"]["sha256"]
    promotion = load_json(promotion_path)
    require(promotion["complete"] is True and promotion["normalFiles"] == len(ctx.expected)
            and promotion["afterMapSha256"] == ctx.source_sha and promotion["manifestSha256"] == ctx.promotion_sha,
            "Final combined normal promotion has not completed on this reviewed map")
    gates = review["imageGates"]
    require(isinstance(gates, list) and 3 <= len(gates) <= 32, "Missing/bounded image-gate origin closure")
    ctx.gate_roles, names = [], set()
    for gate in gates:
        require(set(gate) == {"name", "role", "imageRoot", "sourceMap", "sourceBefore", "sourceAfter", "receipt",
                             "raw", "helper", "mainSha256", "libSha256", "originEvidence", "originLimitations"}
                and gate["role"] in ("R3", "R5-library", "R5-CLI")
                and isinstance(gate["name"], str) and gate["name"] not in names, "Unknown/duplicate image gate")
        names.add(gate["name"])
        ctx.gate_roles.append({"name": gate["name"], "role": gate["role"], "receiptSha256": gate["receipt"]["sha256"]})
        image = ROOT.joinpath(*relative_path(gate["imageRoot"]).parts)
        require(image.is_relative_to(ROOT / "target") and image != ROOT / "target", "Image root is not contained target source")
        image_map = load_json(pinned(gate["sourceMap"], ctx.inputs))
        require(image_map == load_json(pinned(gate["sourceBefore"], ctx.inputs))
                == load_json(pinned(gate["sourceAfter"], ctx.inputs)), "Image source changed across its recorded gate")
        receipt = load_json(pinned(gate["receipt"], ctx.inputs))
        pinned(gate["raw"], ctx.inputs)
        pinned(gate["helper"], ctx.inputs)
        require(receipt["exitCode"] == 0 and receipt["failed"] == 0 and receipt["passed"] > 0
                and receipt["groups"] > 0 and not receipt["validationErrors"] and not receipt["imageSourceChanges"]
                and receipt["imageManifestSha256"] == digest(encoded(image_map))
                and receipt["reviewedManifestFileSha256"] == gate["sourceMap"]["sha256"]
                and receipt["logSha256"] == gate["raw"]["sha256"]
                and receipt["helperSha256"] == gate["helper"]["sha256"]
                and image_map["native/src/main.rs"] == gate["mainSha256"]
                and image_map["native/src/lib.rs"] == gate["libSha256"], "Image gate/source/main/lib pins differ")
        command = receipt["command"]
        require(command[command.index("--manifest-path") + 1] == str(image / "Cargo.toml")
                and command[command.index("--target-dir") + 1] == str(ROOT / "target"), "Image gate command origin differs")
        require(isinstance(gate["originEvidence"], list) and 1 <= len(gate["originEvidence"]) <= 32
                and isinstance(gate["originLimitations"], list) and gate["originLimitations"]
                and all(isinstance(item, str) for item in gate["originLimitations"]), "Image gate origin evidence/limits absent")
        for pin in gate["originEvidence"]:
            pinned(pin, ctx.inputs)
    require({item["role"] for item in ctx.gate_roles} == {"R3", "R5-library", "R5-CLI"}, "Combined feature gate role missing")
    plan_path = pinned(review["normalCachePlan"], ctx.inputs)
    cache_path = pinned(review["normalCacheReceipt"], ctx.inputs)
    core = load_json(plan_path, 64 * 1024 * 1024)["core"]
    cache = load_json(cache_path, 64 * 1024 * 1024)
    require(core["workspaceRoot"] == core["currentRoot"] == str(ROOT)
            and core["targetRoot"] == str(ROOT / "target")
            and core["currentLibSha256"] == ctx.expected["native/src/lib.rs"]
            and core["selection"] == "all_own_package_fingerprints_for_context_switch"
            and isinstance(core["profiles"], list) and "debug" in core["profiles"]
            and len(core["profiles"]) == len(set(core["profiles"]))
            and set(core["profiles"]).issubset({"debug", "release"})
            and not core["explicitNames"], "Cache plan does not admit final normal context")
    for name in ("native/src/lib.rs", "Cargo.toml", "Cargo.lock", "native/Cargo.toml"):
        require(core["currentSourcePins"][name]["sha256"] == ctx.expected[name], "Cache plan source context differs")
    require(cache["status"] == "selected_fingerprint_metadata_retained_and_invalidated"
            and cache["reviewedPlanSha256"] == review["normalCachePlan"]["sha256"]
            and cache["completeOwnPackageInvalidationForSelectedProfiles"] is True
            and cache["remainingOwnPackageDirectoryCount"] == 0 and cache["profiles"] == core["profiles"]
            and cache["selection"] == core["selection"] and cache["cacheMutationPerformed"] is True
            and cache["beforeCurrentSourcePins"] == cache["afterCurrentSourcePins"] == core["currentSourcePins"]
            and cache["normalSourceWrites"] is False and cache["cargoInvoked"] is False,
            "Whole own-package normal-context cache invalidation is not recorded")
    ctx.cache_receipt_sha = review["normalCacheReceipt"]["sha256"]
    ctx.h18_tag, ctx.full_tag, ctx.pipeline_tag = (args.tag_prefix + suffix for suffix in ("-h18", "-full", "-pipeline"))
    ctx.verify()
    ctx.check_source()
    return ctx


def frozen_runner():
    require(file_hash(RUNNER) == RUNNER_SHA, "Frozen acceptance runner changed")
    specification = importlib.util.spec_from_file_location("combined_frozen_acceptance_helpers", RUNNER)
    module = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(module)
    return module


def fresh_fixture(path, drive):
    parent = Path("D:/Yoke-Test-Scratch") if drive == "D" else Path("C:/Users/Public/Yoke-Test-Scratch")
    require(path.is_absolute() and path.drive.upper() == drive + ":" and path.parent == parent
            and not path.exists() and not path.is_relative_to(ROOT), "Unexpected/nonfresh fixture root")
    no_reparse(path)


def source_changes(before, after):
    return sorted(name for name in before.keys() | after.keys() if before.get(name) != after.get(name))
