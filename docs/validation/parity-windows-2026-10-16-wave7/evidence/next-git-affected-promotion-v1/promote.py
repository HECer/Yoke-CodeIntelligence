"""Apply only the reviewed 19-file R5B overlay; never run gates or clean up."""
import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import stat
import subprocess

HERE = Path(__file__).absolute().parent
ROOT = HERE.parents[2]
IMAGE = ROOT / "target/r5b-affected-v1"
EVIDENCE = "target/parity-validation-20261015/"
BEFORE_MAP = "target/parity-validation-20261015/next-combined-metadata-v1/application-caae35463fff5c8d65713e56823fdd8c7a8bba60fc5a5d90bdd30789b8968f40/normal-after.json"
BEFORE_SHA = "3f3f86ff7e0fefc2cb9d1defb104b3c2a1aec909a5198493da6522d292967ecd"
IMAGE_MAP = "target/parity-validation-20261015/next-git-affected-implementation-v1/final-image-quality-v1.json"
IMAGE_SHA = "a3c40b90bb1232496b89ec5af68c1d34998eb7fd2534e20c5d17655c46e9b33a"
QUALITY_HELPER = "5b844195b9f66ad62d807f56c693a249cef97237880651db11c0f461f78c9d64"
INVENTORY_HELPER = "d3b861aa34e3d7889b0b18f791496fed04c768f40ca9a231b3372564a10713d0"
PREIMAGES = {'native/README.md': '52b27c05f75b8fd3a2cac0f39b3f9bd3c45a2f3035ec3504c961053e82ea43d0', 'native/docs/git-affected-details.md': 'ABSENT', 'native/docs/git-workflows.md': 'efa4d38decd72f32d145f34f748526ea917170a69db78c3f00f08f46d944ae6b', 'native/src/git_affected.rs': 'ABSENT', 'native/src/git_affected_admission_tests.rs': 'ABSENT', 'native/src/git_affected_cli.rs': 'ABSENT', 'native/src/git_affected_index.rs': 'ABSENT', 'native/src/git_affected_output.rs': 'ABSENT', 'native/src/git_affected_source.rs': 'ABSENT', 'native/src/git_affected_tests.rs': 'ABSENT', 'native/src/git_affected_walk.rs': 'ABSENT', 'native/src/git_impact_details.rs': 'aa1b882f7dc3bebe94de5f95997f39897ef68a2f795892e6a04edbb974b6742e', 'native/src/git_impact_details_scope_tests.rs': '6d7efb8f7defd2cf047d5270650324b9ed8c24ea381a5f2beec1e28e9fe02d8e', 'native/src/git_workflows.rs': '0e2d9ea13aa11a8d54559012b015da61fbee27afe2596d32315b5a268d6c26cb', 'native/src/main.rs': 'c114e0f1f8509f970d22a167259498f8798f32a43113ed61792844f6c488af43', 'native/tests/git_affected_cli.rs': 'ABSENT', 'native/tests/git_affected_details.rs': 'ABSENT', 'native/tests/git_affected_qualification.rs': 'ABSENT', 'native/tests/support/git_affected_fixture.rs': 'ABSENT'}
MAX_FILE = 256 * 1024 * 1024


def require(ok, message):
    if not ok:
        raise RuntimeError(message)


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def encoded(value):
    return (json.dumps(value, indent=2, ensure_ascii=False, sort_keys=True) + "\n").encode()


def identity(info):
    return info.st_dev, info.st_ino, info.st_size, info.st_mtime_ns


def plain(base, name, missing=False):
    relative = PurePosixPath(name)
    require(name and relative.as_posix() == name and not relative.is_absolute()
            and all(part not in (".", "..") and not any(c in part for c in "\\:\0")
                    for part in relative.parts), "Unsafe relative path")
    require(base.resolve(strict=True) == base, "Noncanonical boundary")
    current = base
    for index, part in enumerate((None, *relative.parts)):
        if part is not None:
            current /= part
        try:
            info = current.lstat()
        except FileNotFoundError:
            require(missing and index == len(relative.parts), "Missing ancestor: " + str(current))
            require(current.parent.resolve(strict=True) == current.parent, "Aliased parent")
            return current
        require(not stat.S_ISLNK(info.st_mode)
                and not getattr(info, "st_file_attributes", 0) & 0x400, "Reparse path")
        require(stat.S_ISDIR(info.st_mode) if index < len(relative.parts)
                else stat.S_ISDIR(info.st_mode) or stat.S_ISREG(info.st_mode), "Special path")
    require(current.resolve(strict=True) == current, "Aliased path")
    return current


def read(base, name, maximum=MAX_FILE):
    path = plain(base, name)
    before = path.lstat()
    require(stat.S_ISREG(before.st_mode) and 0 <= before.st_size <= maximum, "Input size/type")
    with path.open("rb") as stream:
        opened = os.fstat(stream.fileno())
        raw = stream.read(min(before.st_size + 1, maximum + 1))
        completed = os.fstat(stream.fileno())
    after = plain(base, name).lstat()
    require(identity(before) == identity(opened) == identity(completed) == identity(after)
            and len(raw) == before.st_size, "Input changed: " + name)
    return raw


def pinned(base, name, expected):
    raw = read(base, name)
    require(sha(raw) == expected, "Hash differs: " + name)
    return json.loads(raw)


def write_new(path, raw):
    with path.open("xb") as stream:
        require(stream.write(raw) == len(raw), "Short write")
        stream.flush()
        os.fsync(stream.fileno())


def normal_manifest(expected):
    # Same Git inventory command/inclusion rule as frozen acceptance_wave4.py;
    # reads are strengthened with plain-path and before/opened/after checks.
    raw = subprocess.check_output(
        ["rtk", "proxy", "git", "ls-files", "-c", "-o", "--exclude-standard", "-z"], cwd=ROOT)
    names = sorted(name for name in set(raw.decode("utf-8").split("\0"))
                   if name and (ROOT / name).is_file())
    require(set(names) == set(expected), "Whole-normal path set differs")
    actual = {name: sha(read(ROOT, name)) for name in names}
    require(actual == expected, "Whole-normal bytes differ")
    return actual


def image_manifest(expected):
    names = []
    entries = 0
    for parent, directories, files in os.walk(IMAGE, followlinks=False):
        for leaf in [*directories, *files]:
            entries += 1
            require(entries <= 20_000, "Image entry cap")
            path = plain(IMAGE, (Path(parent) / leaf).relative_to(IMAGE).as_posix())
            if leaf in files:
                require(path.is_file(), "Image non-file")
                names.append(path.relative_to(IMAGE).as_posix())
    require(set(names) == set(expected), "Image path set differs")
    require({name: sha(read(IMAGE, name, 16 * 1024 * 1024)) for name in names} == expected,
            "Image bytes differ")


def quality(kind, receipt_name, expected_sha, image):
    relative = PurePosixPath(receipt_name)
    require(receipt_name.startswith(EVIDENCE) and relative.name == "receipt.json"
            and re.fullmatch(r"r5b-[a-z0-9-]+", relative.parent.name)
            and relative.parent.parent.as_posix() == EVIDENCE.rstrip("/"), "Quality receipt path")
    receipt = pinned(ROOT, receipt_name, expected_sha)
    require(receipt.get("kind") == kind and receipt.get("exitCode") == 0
            and receipt.get("imageSourceChanges") == [] and receipt.get("validationErrors") == []
            and receipt.get("sourceFiles") == 646
            and receipt.get("reviewedManifestFileSha256") == IMAGE_SHA
            and receipt.get("helperSha256") == QUALITY_HELPER
            and receipt.get("inventoryHelperSha256") == INVENTORY_HELPER, "Quality gate not green")
    folder = relative.parent
    require(sha(read(ROOT, (folder / "run.log").as_posix())) == receipt["rawSha256"], "Quality raw log")
    for leaf in ("source-before.json", "source-after.json"):
        require(pinned(ROOT, (folder / leaf).as_posix(), IMAGE_SHA) == image, "Quality map")
    common = ["--manifest-path", str(IMAGE / "Cargo.toml")]
    expected = (["cargo", "fmt", "--all", *common, "--", "--check"] if kind == "format" else
                ["cargo", "clippy", "--workspace", "--keep-going", "--all-targets", "--locked", "-j2",
                 *common, "--target-dir", str(ROOT / "target"), "--", "-D", "warnings"])
    require(receipt.get("command") == expected, "Quality command differs")


def main():
    parser = argparse.ArgumentParser()
    for name in ("reviewed-promoter-sha", "reviewed-plan-sha", "reviewed-image-map-sha",
                 "format-receipt", "format-receipt-sha", "clippy-receipt", "clippy-receipt-sha", "run-name"):
        parser.add_argument("--" + name, required=True)
    args = parser.parse_args()
    for name in ("reviewed_promoter_sha", "reviewed_plan_sha", "reviewed_image_map_sha",
                 "format_receipt_sha", "clippy_receipt_sha"):
        require(re.fullmatch(r"[0-9a-f]{64}", getattr(args, name)), "SHA must be exact lowercase hex")
    require(re.fullmatch(r"[a-z0-9][a-z0-9-]{0,60}", args.run_name), "Unsafe run name")
    require(sha(read(HERE, Path(__file__).name)) == args.reviewed_promoter_sha, "Promoter differs")
    require(args.reviewed_image_map_sha == IMAGE_SHA, "Unreviewed image map")
    plan = pinned(HERE, "APPLY-PLAN.json", args.reviewed_plan_sha)
    before = pinned(ROOT, BEFORE_MAP, BEFORE_SHA)
    image = pinned(ROOT, IMAGE_MAP, IMAGE_SHA)
    require(len(before) == 2859 and len(image) == 646, "Map count differs")
    require(all(re.fullmatch(r"[0-9a-f]{64}", value) for value in [*before.values(), *image.values()]), "Map hash shape")
    require({name: before.get(name, "ABSENT") for name in PREIMAGES} == PREIMAGES, "Preimage authority")
    delta = {name for name in image if before.get(name) != image[name]}
    require(delta == set(PREIMAGES), "Not the exact 19-file overlay")
    actions = [{"path": name, "before": PREIMAGES[name], "after": image[name],
                "operation": "CREATE_FILE" if PREIMAGES[name] == "ABSENT" else "REPLACE_FILE"}
               for name in PREIMAGES]
    require(plan["actions"] == actions and plan["image"]["sha256"] == IMAGE_SHA
            and plan["normalBefore"]["sha256"] == BEFORE_SHA, "Reviewed plan differs")
    helper_root = EVIDENCE + "next-git-affected-implementation-v1/"
    require(sha(read(ROOT, helper_root + "shadow_quality.py")) == QUALITY_HELPER
            and sha(read(ROOT, helper_root + "shadow_gate.py")) == INVENTORY_HELPER, "Quality helper differs")
    quality("format", args.format_receipt, args.format_receipt_sha, image)
    quality("clippy", args.clippy_receipt, args.clippy_receipt_sha, image)
    image_manifest(image)
    payloads = {name: read(IMAGE, name, 16 * 1024 * 1024) for name in PREIMAGES}
    originals = {}
    original_identities = {}
    for name, expected in PREIMAGES.items():
        path = plain(ROOT, name, missing=expected == "ABSENT")
        if expected == "ABSENT":
            require(not path.exists(), "Create target exists")
        else:
            require(path.lstat().st_nlink == 1, "Replacement hardlink refused")
            original_identities[name] = identity(path.lstat())
            originals[name] = read(ROOT, name, 16 * 1024 * 1024)
            require(identity(plain(ROOT, name).lstat()) == original_identities[name]
                    and sha(originals[name]) == expected, "Replacement preimage differs")
        require(sha(payloads[name]) == image[name], "Copy payload differs")
    normal_manifest(before)
    after = dict(before)
    after.update({name: image[name] for name in PREIMAGES})
    output = plain(HERE, "application-" + args.run_name, missing=True)
    require(not output.exists(), "Application output already exists")
    output.mkdir()
    (output / "backups").mkdir()
    journal = (output / "journal.jsonl").open("xb")
    def record(event):
        raw = (json.dumps(event, sort_keys=True) + "\n").encode()
        require(journal.write(raw) == len(raw), "Short journal write")
        journal.flush()
        os.fsync(journal.fileno())
    completed = []
    in_flight = None
    try:
        record({"event": "start", "promoterSha256": args.reviewed_promoter_sha,
                "planSha256": args.reviewed_plan_sha, "imageMapSha256": IMAGE_SHA,
                "normalBeforeSha256": BEFORE_SHA, "qualityReceiptPins": {
                    args.format_receipt: args.format_receipt_sha, args.clippy_receipt: args.clippy_receipt_sha}})
        write_new(output / "normal-before.json", encoded(before))
        write_new(output / "normal-expected-after.json", encoded(after))
        for index, (name, raw) in enumerate(originals.items()):
            backup = "backups/" + str(index) + ".bin"
            write_new(output / backup, raw)
            require(sha(read(output, backup)) == PREIMAGES[name], "Backup differs")
            record({"event": "backup", "path": name, "backup": backup, "sha256": PREIMAGES[name]})
        for name, expected in PREIMAGES.items():
            path = plain(ROOT, name, missing=expected == "ABSENT")
            if expected != "ABSENT":
                require(identity(path.lstat()) == original_identities[name], "Original object changed")
            in_flight = name
            record({"event": "intent", "path": name, "before": expected, "after": image[name]})
            with path.open("xb" if expected == "ABSENT" else "r+b") as stream:
                opened = os.fstat(stream.fileno())
                require(stat.S_ISREG(opened.st_mode) and opened.st_nlink == 1, "Write target type/link")
                if expected != "ABSENT":
                    require(identity(plain(ROOT, name).lstat()) == identity(opened)
                            == original_identities[name], "Replacement identity differs")
                    require(sha(stream.read(min(opened.st_size + 1, 16 * 1024 * 1024 + 1))) == expected,
                            "Opened replacement preimage differs")
                    stream.seek(0)
                require(stream.write(payloads[name]) == len(payloads[name]), "Short target write")
                stream.truncate(len(payloads[name]))
                stream.flush()
                os.fsync(stream.fileno())
                written = os.fstat(stream.fileno())
                require((opened.st_dev, opened.st_ino) == (written.st_dev, written.st_ino), "Object identity changed")
            current = plain(ROOT, name).lstat()
            require((current.st_dev, current.st_ino) == (written.st_dev, written.st_ino)
                    and sha(read(ROOT, name)) == image[name], "Written target differs")
            completed.append(name)
            in_flight = None
            record({"event": "complete", "path": name, "after": image[name],
                    "identity": [current.st_dev, current.st_ino]})
        actual = normal_manifest(after)
        write_new(output / "normal-after.json", encoded(actual))
        receipt = {"accepted": True, "imageMapSha256": IMAGE_SHA, "completed": completed,
                   "normalBeforeSha256": BEFORE_SHA, "normalAfterSha256": sha(encoded(actual)),
                   "normalFiles": len(actual), "normalOnlyFilesPreserved": 2226,
                   "normalCheckoutAcceptanceClaim": False}
        write_new(output / "receipt.json", encoded(receipt))
        record({"event": "finished", "receiptSha256": sha(encoded(receipt))})
        print(json.dumps(receipt), flush=True)
    except BaseException as error:
        record({"event": "failed", "completed": completed, "inFlight": in_flight,
                "error": str(error), "automaticRollback": False})
        raise
    finally:
        journal.close()


if __name__ == "__main__":
    main()
