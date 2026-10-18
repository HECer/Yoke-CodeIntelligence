"""Root-only normal-source gate helpers; importing this file runs no gate."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path, PurePosixPath
import stat
import subprocess

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[4]
EVIDENCE = ROOT / "target/parity-validation-20261015"
R3 = EVIDENCE / "next-historical-view-comparison"
PLAN = R3 / "revisions/normal-promotion-prep-v2"
RUNNER = EVIDENCE / "acceptance_wave4.py"
EXPECTED_SOURCE = "afdc36edc71c1396dece79d8cc5ae1d5fddf81100c687a5878e373ac43825bcf"
PROMOTION_SHA = "8cecd97b741523de422bbf00aeb9ccc9c13e05d34ac1435ed0d828865c315785"
REVIEW_SHA = "8f50bace7f162aac25b8bfcc9628c0641f638d4bbd5b709222fda94fbbea2ab4"
RUNNER_SHA = "b51bbffbddd07be0aa6ef019fc950d772c628d77b3567f646d7a837726aa3d15"
BASE_WRAPPER = EVIDENCE / "next-universal-slice/root/run_wave5_acceptance_after_test_migration.py"
BASE_WRAPPER_SHA = "c8432150d24b9a35c0c38a14525d07b546f55faefb89c647529138a8afe87b27"
H18_TAG = "r3-normal-h18-20261016-v1"
FULL_TAG = "r3-normal-frozen-20261016-v1"
PIPELINE_TAG = "r3-normal-pipeline-20261016-v1"
TARGETS = ("selected_view_analysis", "protocol_connection", "meaning_view_archives",
           "registry_replay_review", "collection_registry_access", "graph_view")


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def encoded(value):
    return (json.dumps(value, indent=2, ensure_ascii=False, sort_keys=True) + "\n").encode("utf-8")


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def file_hash(path, maximum=128 * 1024 * 1024):
    before = path.lstat()
    require(stat.S_ISREG(before.st_mode) and not path.is_symlink()
            and not getattr(before, "st_file_attributes", 0) & stat.FILE_ATTRIBUTE_REPARSE_POINT,
            "Non-plain input: " + str(path))
    require(before.st_size <= maximum, "Input byte limit: " + str(path))
    result, size = hashlib.sha256(), 0
    with path.open("rb") as stream:
        opened = os.fstat(stream.fileno())
        require((opened.st_dev, opened.st_ino, opened.st_size, opened.st_mtime_ns) ==
                (before.st_dev, before.st_ino, before.st_size, before.st_mtime_ns), "Input changed on open")
        while chunk := stream.read(64 * 1024):
            size += len(chunk)
            require(size <= maximum, "Input grew beyond byte limit")
            result.update(chunk)
        final = os.fstat(stream.fileno())
    after = path.lstat()
    identity = lambda value: (value.st_dev, value.st_ino, value.st_size, value.st_mtime_ns)
    require(identity(before) == identity(final) == identity(after) and size == before.st_size,
            "Input changed during read: " + str(path))
    return result.hexdigest()


def load_json(path, maximum=8 * 1024 * 1024):
    require(path.stat().st_size <= maximum, "JSON byte limit")
    return json.loads(path.read_text(encoding="utf-8"))


def write_json(path, value):
    with path.open("xb") as stream:
        stream.write(encoded(value))


def guarded_source(name):
    relative = PurePosixPath(name)
    require(relative.parts and not relative.is_absolute() and relative.as_posix() == name
            and all(part not in (".", "..") and ":" not in part and "\\" not in part
                    for part in relative.parts), "Unsafe Git inventory path")
    path = ROOT.joinpath(*relative.parts)
    for candidate in (ROOT, *path.relative_to(ROOT).parents):
        current = candidate if candidate.is_absolute() else ROOT / candidate
        if current.exists():
            info = current.lstat()
            require(not current.is_symlink()
                    and not getattr(info, "st_file_attributes", 0) & stat.FILE_ATTRIBUTE_REPARSE_POINT,
                    "Reparse source ancestor refused")
    return path


def source_manifest():
    raw = subprocess.check_output(
        ["rtk", "proxy", "git", "ls-files", "-c", "-o", "--exclude-standard", "-z"], cwd=ROOT)
    require(len(raw) <= 8 * 1024 * 1024, "Git inventory byte limit")
    names = sorted(set(name for name in raw.decode("utf-8").split("\0") if name))
    require(0 < len(names) <= 10000, "Git inventory file limit")
    result, total = {}, 0
    for name in names:
        path = guarded_source(name)
        require(path.is_file(), "Missing/non-file Git-visible source: " + name)
        total += path.stat().st_size
        require(total <= 512 * 1024 * 1024, "Git inventory total byte limit")
        result[name] = file_hash(path, 16 * 1024 * 1024)
    return result


def verify_inputs(common_sha):
    require(os.name == "nt" and ROOT.name == "Code-Intelligence"
            and (ROOT / "native/Cargo.toml").is_file(), "Unexpected normal Windows checkout")
    require(file_hash(Path(__file__)) == common_sha, "Shared wrapper helper changed")
    require(file_hash(RUNNER) == RUNNER_SHA, "Frozen acceptance runner changed")
    require(file_hash(BASE_WRAPPER) == BASE_WRAPPER_SHA, "Historical wrapper changed")
    require(file_hash(PLAN / "MANIFEST.json") == PROMOTION_SHA, "Promotion manifest changed")
    require(file_hash(R3 / "ROOT-NORMAL-PROMOTION-REVIEW-V2.json") == REVIEW_SHA,
            "Root promotion review changed")
    review = load_json(R3 / "ROOT-NORMAL-PROMOTION-REVIEW-V2.json")
    require(review["approvedManifestSha256"] == PROMOTION_SHA and review["independentReviewClear"] is True,
            "Normal promotion lacks clear root review")
    expected_path = PLAN / "EXPECTED-NORMAL-AFTER.json"
    require(file_hash(expected_path) == EXPECTED_SOURCE, "Expected normal source map changed")
    expected = load_json(expected_path)
    require(len(expected) == 2406 and digest(encoded(expected)) == EXPECTED_SOURCE,
            "Expected normal source encoding/count changed")
    journal = load_json(R3 / "normal-promotion-application-v2/journal.json")
    require(journal["complete"] is True and journal["normalWrites"] == 32
            and journal["normalFiles"] == 2406 and journal["manifestSha256"] == PROMOTION_SHA
            and journal["afterMapSha256"] == EXPECTED_SOURCE, "Normal promotion has not completed")
    return expected


def frozen_runner():
    require(file_hash(RUNNER) == RUNNER_SHA, "Frozen acceptance runner changed")
    specification = importlib.util.spec_from_file_location("r3_frozen_acceptance_helpers", RUNNER)
    module = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(module)
    return module


def check_source(expected):
    current = source_manifest()
    require(current == expected and digest(encoded(current)) == EXPECTED_SOURCE,
            "Normal checkout differs from conditional2406 promoted source")
    return current


def fresh_fixture(path, drive):
    expected_parent = Path("D:/Yoke-Test-Scratch") if drive == "D" else Path("C:/Users/Public/Yoke-Test-Scratch")
    require(path.is_absolute() and path.drive.upper() == drive + ":"
            and path.parent == expected_parent, "Unexpected fixture root")
    require(not path.exists() and not path.is_relative_to(ROOT), "Fixture root is not fresh")


def source_changes(before, after):
    return sorted(name for name in before.keys() | after.keys() if before.get(name) != after.get(name))
