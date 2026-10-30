"""Partition whole default Cargo targets; require every platform receipt.

This changes scheduling only. No libtest name filter, ignored-test promotion,
profile override or source-selected command is admitted. `plan` reads metadata
without compiling or executing a native image. Execution requires explicit CI.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
SHARDS = 8
MAX_GROUPS = 2048
MAX_LOG = 16_000_000


def sibling(name):
    spec = importlib.util.spec_from_file_location(name, ROOT / "scripts" / (name + ".py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


FMT = sibling("native_fmt")
OWNED = sibling("host_history_live")
TESTS = sibling("run_native_tests")
require = OWNED.require


def name(value):
    require(isinstance(value, str) and re.fullmatch(r"[A-Za-z0-9_][A-Za-z0-9_-]{0,127}", value),
            "Unsupported Cargo package or target name")
    return value


def inventory(data, root):
    require(isinstance(data, dict) and isinstance(data.get("packages"), list)
            and isinstance(data.get("workspace_members"), list), "Invalid workspace metadata")
    members = data["workspace_members"]
    require(0 < len(members) <= 64 and len(set(members)) == len(members), "Invalid workspace membership")
    packages = data["packages"]
    require(len(packages) <= 128 and len({p["id"] for p in packages}) == len(packages),
            "Invalid package inventory")
    selected = [p for p in packages if p["id"] in members]
    require({p["id"] for p in selected} == set(members), "Missing workspace package")
    require(len({p["name"] for p in selected}) == len(selected), "Ambiguous package names")
    groups = []
    target_keys = set()
    root = root.resolve(strict=True)
    for package in selected:
        package_name = name(package["name"])
        targets = package.get("targets")
        require(isinstance(targets, list) and 0 < len(targets) <= MAX_GROUPS, "Invalid Cargo targets")
        require(sum(target.get("kind") == ["lib"] for target in targets) <= 1, "Ambiguous package library")
        for target in targets:
            kinds = target.get("kind")
            require(isinstance(kinds, list) and len(kinds) == 1
                    and kinds[0] in ("lib", "bin", "test", "example", "bench", "custom-build"),
                    "Unsupported Cargo target kind")
            kind = kinds[0]
            target_name = name(target["name"])
            key = (package_name, kind, target_name)
            require(key not in target_keys, "Duplicate Cargo target")
            target_keys.add(key)
            require(type(target.get("test")) is bool and type(target.get("doctest")) is bool,
                    "Missing Cargo target policy")
            require((kind not in ("bench", "custom-build") or not target["test"])
                    and (kind == "lib" or not target["doctest"]), "Unsupported default target policy")
            require(not target.get("required-features", []), "Feature-gated target needs explicit reconciliation")
            path = Path(target["src_path"])
            require(path.is_absolute() and path.is_file() and not path.is_symlink()
                    and not path.is_junction(), "Unavailable target source")
            path = path.resolve(strict=True)
            require(path.is_relative_to(root), "Target source outside selected checkout")
            relative = path.relative_to(root).as_posix()
            modes = []
            if kind not in ("bench", "custom-build"):
                if target["test"]:
                    modes.append("test")
                elif kind == "example":
                    modes.append("build")
                if kind == "lib" and target["doctest"]:
                    modes.append("doc")
            for mode in modes:
                groups.append({"id": ":".join((package_name, kind, target_name, mode)),
                    "package": package_name, "kind": kind, "name": target_name,
                    "mode": mode, "source": relative, "weight": max(1, path.stat().st_size)})
                require(len(groups) <= MAX_GROUPS, "Target group capacity exceeded")
    require(len(groups) >= SHARDS, "Insufficient default target groups")
    return sorted(groups, key=lambda row: row["id"])


def partitions(groups):
    require(SHARDS <= len(groups) <= MAX_GROUPS
            and all(isinstance(row.get("id"), str) and type(row.get("weight")) is int
                    and 0 < row["weight"] <= 16_000_000 for row in groups)
            and len({row["id"] for row in groups}) == len(groups),
            "Invalid group identities")
    buckets = [[] for _ in range(SHARDS)]
    weights = [0] * SHARDS
    for row in sorted(groups, key=lambda item: (-item["weight"], item["id"])):
        index = min(range(SHARDS), key=lambda slot: (weights[slot], slot))
        buckets[index].append(row)
        weights[index] += row["weight"]
    require(all(buckets), "Empty shard")
    return [sorted(bucket, key=lambda row: row["id"]) for bucket in buckets]


def command(group, platform):
    require(platform in ("Windows", "Linux", "macOS"), "Unsupported runner platform")
    selection = {"lib": ["--lib"], "bin": ["--bin", group["name"]],
                 "test": ["--test", group["name"]], "example": ["--example", group["name"]]}
    tail = ["--locked", "-j", "2", "--package", group["package"]]
    if group["mode"] == "build":
        require(group["kind"] == "example", "Unsupported compile-only group")
        return ["cargo", "build", "--profile", "test", *tail, "--example", group["name"]]
    target = ["--doc"] if group["mode"] == "doc" else selection[group["kind"]]
    require(group["mode"] in ("test", "doc") and (group["mode"] != "doc" or group["kind"] == "lib"),
            "Unsupported test group")
    return [sys.executable, str(ROOT / "scripts/run_native_tests.py"), "--controlled-test-runner",
            *tail, "--no-fail-fast", *target, "--", "--test-threads=" + ("1" if platform == "Windows" else "2")]


def digest(value):
    return hashlib.sha256(json.dumps(value, ensure_ascii=False, sort_keys=True,
        separators=(",", ":")).encode()).hexdigest()


def ci_context():
    require(os.environ.get("GITHUB_ACTIONS") == "true", "Native shard execution requires CI")
    require(sys.platform in ("win32", "linux", "darwin"), "Unsupported native shard host")
    platform = "Windows" if os.name == "nt" else "macOS" if sys.platform == "darwin" else "Linux"
    require(os.environ.get("RUNNER_OS") == platform, "CI runner platform differs")
    require(shutil.disk_usage(ROOT).free >= OWNED.MIN_FREE, "Native shard disk reserve unavailable")
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    require(re.fullmatch(r"[0-9a-f]{40}", revision) and revision == os.environ.get("GITHUB_SHA"),
            "CI source revision differs")
    return platform, revision


def selected_groups():
    return inventory(FMT.cargo_metadata(ROOT / "Cargo.toml"), ROOT)


def run(index):
    require(type(index) is int and 0 <= index < SHARDS, "Invalid shard index")
    platform, revision = ci_context()
    groups = selected_groups()
    selected = partitions(groups)[index]
    inputs = OWNED.source_inputs()
    require(all(group["source"] in inputs for group in groups), "Untracked target source requires reconciliation")
    output = ROOT / "validation" / "ci-native-shard"
    require(not output.parent.is_symlink() and not output.parent.is_junction(), "Linked output namespace refused")
    output.mkdir(parents=True, exist_ok=False)
    rows = []
    complete = False
    deadline = time.monotonic() + 44 * 60
    try:
        for ordinal, group in enumerate(selected):
            require(shutil.disk_usage(ROOT).free >= OWNED.MIN_FREE, "Native shard disk-headroom stop")
            argv = command(group, platform)
            with (output / f"{ordinal:04d}.stdout").open("xb") as stdout, \
                 (output / f"{ordinal:04d}.stderr").open("xb") as stderr:
                remaining = deadline - time.monotonic()
                require(remaining > 0, "Native shard deadline")
                result = subprocess.run(argv, cwd=ROOT, env=TESTS.controlled_environment(),
                                        stdout=stdout, stderr=stderr, timeout=remaining)
            logs = []
            for suffix in ("stdout", "stderr"):
                path = output / f"{ordinal:04d}.{suffix}"
                require(path.stat().st_size <= MAX_LOG, "Native group log admission capacity exceeded")
                logs.append({"name": path.name, "bytes": path.stat().st_size, "sha256": OWNED.sha(path)})
            OWNED.output_size(output)
            rows.append({"group": group["id"], "exitCode": result.returncode, "logs": logs})
            print(json.dumps({"group": group["id"], "exitCode": result.returncode}), flush=True)
        require(OWNED.source_inputs() == inputs, "Selected native inputs changed")
        subprocess.run(["git", "diff", "--no-ext-diff", "--no-textconv", "--exit-code", "HEAD", "--"],
                       cwd=ROOT, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        complete = len(rows) == len(selected) and all(row["exitCode"] == 0 for row in rows)
    finally:
        OWNED.write_json(output / "result.json", {"schemaVersion": 1, "platform": platform,
            "sourceRevision": revision, "index": index, "shards": SHARDS, "complete": complete,
            "inventory": groups, "inventorySha256": digest(groups), "sourceInputs": inputs,
            "sourceScope": "Selected tracked native/src/python/scripts and Cargo/package inputs; not all build inputs",
            "groups": rows, "wholeCargoTargets": True, "countsAreNotCombined": True})
    return 0 if complete else 1


def check_receipts(receipts, groups, inputs, platform, revision):
    require(len(receipts) == SHARDS, "Missing platform shard receipts")
    require(all(group["source"] in inputs for group in groups), "Target source missing from selected inputs")
    expected = partitions(groups)
    seen = set()
    for receipt in receipts:
        index = receipt.get("index")
        require(type(index) is int and 0 <= index < SHARDS and index not in seen, "Duplicate or invalid shard")
        seen.add(index)
        require(type(receipt.get("schemaVersion")) is int and receipt.get("schemaVersion") == 1 and receipt.get("platform") == platform
                and receipt.get("sourceRevision") == revision and receipt.get("shards") == SHARDS
                and receipt.get("complete") is True and receipt.get("wholeCargoTargets") is True
                and receipt.get("inventory") == groups and digest(receipt.get("inventory")) == digest(groups)
                and receipt.get("inventorySha256") == digest(groups)
                and receipt.get("sourceInputs") == inputs, "Shard source, scope or result differs")
        rows = receipt.get("groups")
        require(isinstance(rows, list) and [row.get("group") for row in rows] == [g["id"] for g in expected[index]]
                and all(type(row.get("exitCode")) is int and row["exitCode"] == 0 for row in rows),
                "Missing, duplicate, failed or substituted Cargo group")
        for ordinal, row in enumerate(rows):
            logs = row.get("logs")
            require(isinstance(logs, list) and len(logs) == 2
                    and [log.get("name") for log in logs] == [f"{ordinal:04d}.stdout", f"{ordinal:04d}.stderr"]
                    and all(type(log.get("bytes")) is int and 0 <= log["bytes"] <= MAX_LOG
                            and isinstance(log.get("sha256"), str) and re.fullmatch(r"[0-9a-f]{64}", log["sha256"])
                            for log in logs), "Missing or invalid group log receipts")
    return {"complete": True, "platform": platform, "sourceRevision": revision,
            "shards": SHARDS, "wholeGroups": len(groups), "inventorySha256": digest(groups),
            "scope": "All default workspace target groups; individual function counts are not inferred"}


def check(path):
    platform, revision = ci_context()
    require(path.absolute() == ROOT / "validation/ci-native-shards-download"
            and not path.is_symlink() and not path.is_junction()
            and not path.parent.is_symlink() and not path.parent.is_junction(), "Unexpected shard artifact namespace")
    groups = selected_groups()
    inputs = OWNED.source_inputs()
    paths = sorted(path.glob("native-shard-*/result.json"))
    require(len(paths) == SHARDS, "Missing platform artifact directories")
    receipts = []
    for result in paths:
        require(not result.parent.is_symlink() and not result.parent.is_junction(), "Linked shard directory refused")
        OWNED.output_size(result.parent)
        receipt = OWNED.read_json(result)
        receipts.append(receipt)
    proof = check_receipts(receipts, groups, inputs, platform, revision)
    for result, receipt in zip(paths, receipts):
        rows = receipt["groups"]
        for row in rows:
            for log in row.get("logs", []):
                require(re.fullmatch(r"[0-9]{4}\.(stdout|stderr)", log["name"]), "Unsafe shard log name")
                file = result.parent / log["name"]
                require(file.is_file() and not file.is_symlink() and not file.is_junction()
                        and type(log["bytes"]) is int and 0 <= log["bytes"] <= MAX_LOG
                        and file.stat().st_size == log["bytes"] and OWNED.sha(file) == log["sha256"],
                        "Shard log bytes differ")
    OWNED.write_json(ROOT / "validation/ci-native-coverage.json", proof)
    print(json.dumps(proof))
    return 0


def main(arguments=None):
    parser = argparse.ArgumentParser(description=__doc__)
    operations = parser.add_subparsers(dest="operation", required=True)
    operations.add_parser("plan")
    selected = operations.add_parser("run")
    selected.add_argument("--index", type=int, required=True)
    aggregate = operations.add_parser("check")
    aggregate.add_argument("--artifacts", type=Path, required=True)
    options = parser.parse_args(arguments)
    if options.operation == "plan":
        groups = selected_groups()
        print(json.dumps({"groups": groups, "shards": [[g["id"] for g in part] for part in partitions(groups)]}, ensure_ascii=False))
        return 0
    return run(options.index) if options.operation == "run" else check(options.artifacts)


if __name__ == "__main__":
    raise SystemExit(main())
