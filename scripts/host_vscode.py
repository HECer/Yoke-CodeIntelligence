"""Fresh source-bound Windows combined VSIX and real Rust extension-host CI.

Every payload/hash/version/package/editor operation runs in an owned worker.
The same-run Python lifecycle probe is a prerequisite, not VSIX acceptance.
"""
import argparse
import contextlib
import ctypes
import hashlib
import importlib.util
import json
import os
from pathlib import Path, PureWindowsPath
import re
import shutil
import stat
import struct
import subprocess
import sys
import time
import urllib.parse
import urllib.request
import xml.etree.ElementTree as ET
import zipfile

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "validation/ci-vscode-acceptance"
WORKERS = ROOT / "validation/ci-vscode-workers"
PACKAGES = ROOT / "validation/ci-vscode-packages"
CODE_VERSION = "1.143.1"
CODE_URL = "https://vscode.download.prss.microsoft.com/dbazure/download/stable/e646868227c3e3a9e3f24277c8c7220be42c0be7/VSCode-win32-x64-1.143.1.zip"
CODE_SHA256 = "b4f9643b8a549ca2c85a1bbc9063fc76f1e08fc0036a7f6a1a181cdc05235eca"
CODE_BYTES = 360125130
CODE_ENTRIES = 3940
CODE_EXPANDED = 1058631563
CODE_PACKAGE = "e646868227/resources/app/package.json"
NATIVE_MAX = 160_000_000
PROGRESS = []


def sibling(name):
    spec = importlib.util.spec_from_file_location(name, ROOT / "scripts" / (name + ".py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


JOB = sibling("vscode_owned_job")
TESTS = sibling("run_native_tests")
admit_job = JOB.admit_receipt


def require(condition, message):
    if not condition:
        raise ValueError(message)


def unique_object(pairs):
    value = {}
    for key, item in pairs:
        require(key not in value, "Duplicate JSON field")
        value[key] = item
    return value


def reject_constant(_):
    raise ValueError("Non-JSON constant")


def decode(raw, maximum=1_000_000):
    require(isinstance(raw, bytes) and len(raw) <= maximum, "JSON capacity")
    return json.loads(raw, object_pairs_hook=unique_object, parse_constant=reject_constant)


def admit_lifecycle(value, revision):
    require(isinstance(value, dict) and set(value) == {"schemaVersion", "sourceRevision", "complete", "scope", "cases", "VSCodeOrYokeExecuted", "VSIXOrReleaseAccepted"}, "Lifecycle fields differ")
    require(type(value["schemaVersion"]) is int and value["schemaVersion"] == 1
            and value["sourceRevision"] == revision and value["complete"] is True
            and value["scope"] == "Owned Python Job lifecycle probes only"
            and value["VSCodeOrYokeExecuted"] is False and value["VSIXOrReleaseAccepted"] is False, "Lifecycle scope/source differs")
    require(isinstance(value["cases"], list) and len(value["cases"]) == 4, "Four lifecycle cases required")
    require([r.get("scenario") for r in value["cases"]] == ["success", "nonzero", "orphan", "deadline"], "Lifecycle cases differ")
    for row in value["cases"]:
        require(set(row) == {"scenario", "processProof"}, "Lifecycle case fields differ")
        proof = row["processProof"]
        if row["scenario"] == "success":
            admit_job(proof)
            continue
        require(isinstance(proof, dict) and proof.get("startedSuspended") is True
                and proof.get("assignedBeforeResume") is True and proof.get("handlesClosed") is True
                and proof.get("cleanupComplete") is True and type(proof.get("activeAfterCleanup")) is int
                and proof["activeAfterCleanup"] == 0 and proof.get("complete") is False, "Negative lifecycle cleanup incomplete")
        expected = {"nonzero": "root_exit", "orphan": "tree_alive", "deadline": "deadline"}[row["scenario"]]
        require(proof.get("reason") == expected, "Negative lifecycle category differs")
        normalized = proof.copy()
        normalized.update(complete=True, reason="success", exitCode=0)
        if row["scenario"] == "nonzero":
            require(type(proof.get("exitCode")) is int and proof["exitCode"] == 7, "Nonzero probe differs")
        if row["scenario"] == "orphan":
            require(type(proof.get("exitCode")) is int and proof["exitCode"] == 0
                    and type(proof.get("activeBeforeCleanup")) is int and proof["activeBeforeCleanup"] > 0
                    and proof.get("treeExitedBeforeCleanup") is False, "Orphan probe differs")
            normalized.update(activeBeforeCleanup=0, treeExitedBeforeCleanup=True)
        if row["scenario"] == "deadline":
            require(proof.get("exitCode") is None and proof.get("activeAtRootExit") is None
                    and proof.get("activeBeforeCleanup") is None and proof.get("treeExitedBeforeCleanup") is False, "Deadline probe differs")
            normalized.update(activeAtRootExit=0, activeBeforeCleanup=0, treeExitedBeforeCleanup=True)
        admit_job(normalized)


def producer_role(raw, root, package_id):
    require(isinstance(raw, bytes) and 0 < len(raw) <= 16_000_000, "Producer capacity")
    selected = None
    finished = False
    for line in raw.splitlines():
        require(not finished, "Producer record after completion")
        row = decode(line)
        require(isinstance(row, dict), "Invalid producer record")
        if row.get("reason") == "build-finished":
            require(row.get("success") is True, "Producer did not succeed")
            finished = True
        if row.get("reason") != "compiler-artifact" or row.get("package_id") != package_id:
            continue
        target, profile = row.get("target", {}), row.get("profile", {})
        require(isinstance(target, dict) and isinstance(profile, dict), "Producer target/profile missing")
        if target.get("name") != "yoke" or target.get("kind") != ["bin"]:
            continue
        require(profile.get("test") is False and row.get("fresh") is False and selected is None, "Fresh unique release role required")
        executable = row.get("executable")
        filenames = row.get("filenames")
        require(isinstance(executable, str) and isinstance(filenames, list) and 0 < len(filenames) <= 32
                and all(isinstance(name, str) for name in filenames) and executable in filenames
                and PureWindowsPath(executable) == root / "target/release/yoke.exe"
                and isinstance(target.get("src_path"), str)
                and PureWindowsPath(target["src_path"]) == root / "native/src/main.rs", "Release executable role differs")
        selected = {"path": executable, "target": "yoke"}
    require(finished and selected is not None, "Incomplete release producer")
    return selected


def safe_members(infos, maximum_total, maximum_entry, maximum_count):
    require(0 < len(infos) <= maximum_count, "Archive entry capacity")
    selected, aliases = {}, {}
    total = 0
    for info in infos:
        name = info.filename
        require(name == info.orig_filename and "\\" not in name and 0 < len(name) <= 4096, "Archive path differs")
        parts = (name[:-1] if info.is_dir() else name).split("/")
        for part in parts:
            require(part not in ("", ".", "..") and part[-1] not in " ."
                    and not any(ord(c) < 32 or ord(c) == 127 or c in ':*?"<>|' for c in part), "Unsafe archive path")
            device = part.split(".", 1)[0].lower()
            require(device not in {"con", "prn", "aux", "nul", "conin$", "conout$"}
                    and re.fullmatch(r"(?:com|lpt)[1-9¹²³]", device) is None
                    and len(part.encode("utf-16-le")) <= 510, "Archive alias/device refused")
        key = "/".join(parts).casefold()
        require(key not in aliases and not info.flag_bits & 1 and info.compress_type in (0, 8), "Duplicate/encrypted/unsupported archive entry")
        mode = (info.external_attr >> 16) & 0o170000
        require(mode in (0, 0o040000 if info.is_dir() else 0o100000), "Special archive entry refused")
        require(type(info.file_size) is int and 0 <= info.file_size <= maximum_entry
                and (not info.is_dir() or info.file_size == 0), "Archive entry byte capacity")
        total += info.file_size
        require(total <= maximum_total, "Archive total byte capacity")
        aliases[key] = info
        selected[name] = info
    for key in aliases:
        parts = key.split("/")
        require(all(parent not in aliases or aliases[parent].is_dir()
                    for parent in ("/".join(parts[:i]) for i in range(1, len(parts)))), "Archive file/directory conflict")
    return selected


def admit_host_report(value, bundled):
    expected = {"kind", "passed", "runtime", "bundledRuntime", "vscodeVersion", "unsavedSourceBound",
                "workspaceExecutableIgnored", "protectedOpenBufferExcluded", "savedSourceUnchanged"}
    require(isinstance(value, dict) and set(value) == expected, "Host result fields differ")
    require(value["kind"] == "actual-vscode-extension-host" and value["runtime"] == "rust"
            and value["vscodeVersion"] == "1.143.1" and type(value["bundledRuntime"]) is bool
            and value["bundledRuntime"] == bundled, "Host result scope differs")
    require(all(value[key] is True for key in ("passed", "unsavedSourceBound", "workspaceExecutableIgnored",
                "protectedOpenBufferExcluded", "savedSourceUnchanged")), "Host assertions incomplete")


def admit_stage_result(value, phase, revision):
    require(isinstance(value, dict) and set(value) == {"schemaVersion", "sourceRevision", "phase", "complete",
        "workerAdmission", "nativeSHA256", "sourceInputsUnchanged", "builtInputsUnchanged", "observation"}, "Stage fields differ")
    admission = value["workerAdmission"]
    require(isinstance(admission, dict) and set(admission) == {"specificJobMembership", "queryHandleClosedBeforePayloadRead"}
            and all(flag is True for flag in admission.values()), "Typed worker admission required")
    require(type(value["schemaVersion"]) is int and value["schemaVersion"] == 1
        and value["sourceRevision"] == revision and value["phase"] == phase and value["complete"] is True
        and value["sourceInputsUnchanged"] is True and value["builtInputsUnchanged"] is True
        and isinstance(value["nativeSHA256"], str) and re.fullmatch(r"[0-9a-f]{64}", value["nativeSHA256"]) is not None
        and isinstance(value["observation"], dict), "Stage proof incomplete")


def admit_state(value, revision, root, temp):
    require(isinstance(value, dict) and set(value) == {"schemaVersion", "sourceRevision", "toolRoot", "compilerHomes", "workerRoots"}
            and type(value["schemaVersion"]) is int and value["schemaVersion"] == 1 and value["sourceRevision"] == revision, "State source/schema differs")
    tool = PureWindowsPath(value["toolRoot"])
    require(tool.parent == temp and re.fullmatch(r"yvs-[0-9a-f]{16}", tool.name) is not None, "Editor tool role differs")
    roots = value["workerRoots"]
    require(isinstance(roots, dict) and set(roots) == {"tool", "package", "explicit", "bundled"}, "Worker roles differ")
    for phase, name in roots.items():
        path = PureWindowsPath(name)
        if phase in ("tool", "package"):
            require(path == root / "validation/ci-vscode-workers" / phase, "Producer worker role differs")
        else:
            require(path.parent == temp and not path.is_relative_to(root)
                    and re.fullmatch("yvh-" + phase + r"-[0-9a-f]{16}", path.name) is not None, "Isolated real host root required")
    homes = value["compilerHomes"]
    require(isinstance(homes, dict) and set(homes) == {"CARGO_HOME", "RUSTUP_HOME"}, "Compiler home roles differ")
    require(all(isinstance(name, str) and len(name) <= 4096 and PureWindowsPath(name).is_absolute()
                and ".." not in PureWindowsPath(name).parts and not PureWindowsPath(name).is_relative_to(root)
                for name in homes.values()), "Host compiler home outside checkout required")
    return value


def read_bytes(path, maximum):
    JOB.ordinary(path)
    require(path.stat().st_size <= maximum, "Input byte capacity")
    with path.open("rb") as stream:
        raw = stream.read(maximum + 1)
    require(len(raw) <= maximum, "Input byte capacity")
    return raw


def read_json(path, maximum=1_000_000):
    return decode(read_bytes(path, maximum), maximum)


def write_json(path, value, maximum=1_000_000):
    raw = (json.dumps(value, indent=2, ensure_ascii=True) + "\n").encode()
    require(len(raw) <= maximum, "Receipt byte capacity")
    with path.open("xb") as stream:
        stream.write(raw)


def hash_file(path, maximum):
    JOB.ordinary(path)
    size = path.stat().st_size
    require(0 <= size <= maximum, "Hash byte capacity")
    value, total = hashlib.sha256(), 0
    with path.open("rb") as stream:
        while block := stream.read(1_000_000):
            total += len(block)
            require(total <= maximum, "Hash byte capacity")
            value.update(block)
    require(total == size and path.stat().st_size == size, "Hash input changed")
    return value.hexdigest()


def metadata(path, maximum):
    JOB.ordinary(path)
    value = path.stat()
    require(0 < value.st_size <= maximum, "Role byte capacity")
    return {"bytes": value.st_size, "mtimeNs": value.st_mtime_ns, "device": value.st_dev, "inode": value.st_ino}


def identity():
    require(os.environ.get("GITHUB_ACTIONS") == "true" and os.environ.get("RUNNER_OS") == "Windows"
            and os.name == "nt" and sys.platform == "win32" and ctypes.sizeof(ctypes.c_void_p) == 8,
            "Explicit 64-bit hosted Windows acceptance required")
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True, timeout=30).strip()
    require(re.fullmatch(r"[0-9a-f]{40}", revision) is not None and revision == os.environ.get("GITHUB_SHA"), "Source revision differs")
    environment = TESTS.controlled_environment()
    TESTS.check_runner_policy(environment)
    require(not any(k in environment for k in ("CARGO_TARGET_DIR", "RUSTC", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER")), "Compiler selection differs")
    return revision, environment


def source_inputs():
    raw = subprocess.check_output(["git", "ls-files", "-z", "--", "Cargo.toml", "Cargo.lock", "package.json", "package-lock.json",
        ".gitattributes", "native", "src", "python", "scripts", "workers", "extensions/vscode", "docs", "LICENSE",
        "SECURITY.md", "CHANGELOG.md", "THIRD_PARTY_NOTICES.md", "UPSTREAMS.json", ".github/workflows"], cwd=ROOT, timeout=30)
    require(len(raw) <= 1_000_000, "Source inventory capacity")
    paths = raw.decode().split("\0")
    require(paths[-1] == "" and 0 < len(paths) <= 4097, "Source inventory cardinality")
    selected, total = {}, 0
    for name in paths[:-1]:
        path = Path(name)
        if name.startswith("docs/") and not (name.endswith(".md") or (path.parent.as_posix() == "docs/validation"
                and path.name.startswith("native-") and not path.name.startswith("native-distribution") and path.suffix in (".json", ".txt"))):
            continue
        require(name not in selected and "\\" not in name and all(p not in ("", ".", "..") for p in name.split("/")), "Unsafe source input")
        item = ROOT / name
        JOB.ordinary(item)
        size = item.stat().st_size
        total += size
        require(size <= 16_000_000 and total <= 96_000_000, "Source input byte capacity")
        selected[name] = hash_file(item, 16_000_000)
    return selected


def built_inputs():
    selected, total = {}, 0
    for directory in (ROOT / "dist", ROOT / "node_modules/typescript"):
        JOB.ordinary(directory, True)
        for path in sorted(directory.rglob("*")):
            require(not path.is_symlink() and not path.is_junction(), "Linked built input")
            if not path.is_file():
                continue
            total += path.stat().st_size
            require(len(selected) < 4096 and total <= 96_000_000, "Built input capacity")
            selected[path.relative_to(ROOT).as_posix()] = hash_file(path, 16_000_000)
    require("dist/cli.js" in selected and "node_modules/typescript/lib/typescript.js" in selected, "Combined runtime inputs missing")
    return selected


def cargo_command():
    return ["cargo", "build", "--workspace", "--release", "--locked", "-j", "2", "--message-format=json"]


def load_state(revision):
    value = read_json(WORKERS / "state.json")
    return admit_state(value, revision, PureWindowsPath(str(ROOT)), PureWindowsPath(os.environ["RUNNER_TEMP"]))


def prepare():
    revision, environment = identity()
    require(shutil.disk_usage(ROOT).free >= 12_000_000_000, "Fresh release disk admission unavailable")
    require(not (ROOT / "target").exists() and not (ROOT / "dist").exists(), "Fresh native and TypeScript output required")
    validation = ROOT / "validation"
    require(not validation.is_symlink() and not validation.is_junction(), "Linked validation namespace")
    validation.mkdir(exist_ok=True)
    JOB.ordinary(validation, True)
    require(not any(path.exists() for path in (OUTPUT, WORKERS, PACKAGES)), "Fresh acceptance namespaces required")
    admit_lifecycle(read_json(ROOT / "validation/ci-vscode-lifecycle-proof/result.json"), revision)
    for path in (OUTPUT, WORKERS, PACKAGES):
        path.mkdir()
    PROGRESS.append("source")
    inputs = source_inputs()
    write_json(OUTPUT / "source-inputs.json", {"sourceRevision": revision, "sha256": inputs})
    host = Path.home()
    homes = {}
    PROGRESS.append("compiler-homes")
    for key, suffix in (("CARGO_HOME", ".cargo"), ("RUSTUP_HOME", ".rustup")):
        path = Path(environment.get(key, str(host / suffix)))
        JOB.ordinary(path, True)
        homes[key] = str(path)
    temp = Path(os.environ["RUNNER_TEMP"])
    JOB.ordinary(temp, True)
    tool = temp / ("yvs-" + os.urandom(8).hex())
    require(not tool.exists(), "Fresh editor tool namespace required")
    PROGRESS.append("node")
    node = subprocess.check_output(["node", "--version"], cwd=ROOT, env=environment, text=True, timeout=10).strip()
    require(node == "v22.16.0", "Fixed Node producer required")
    PROGRESS.append("cargo-metadata")
    meta = subprocess.check_output(["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"], cwd=ROOT, env=environment, timeout=60)
    data = decode(meta)
    require(Path(data["target_directory"]) == ROOT / "target", "Cargo target directory differs")
    packages = [p for p in data["packages"] if p["name"] == "yoke-code-intelligence" and p["version"] == "0.5.0"
                and Path(p["manifest_path"]) == ROOT / "native/Cargo.toml" and p["id"] in data["workspace_members"]]
    require(len(packages) == 1, "Native package differs")
    PROGRESS.append("native-producer")
    with (OUTPUT / "producer.stdout").open("xb") as stdout, (OUTPUT / "producer.stderr").open("xb") as stderr:
        made = subprocess.run(cargo_command(), cwd=ROOT, env=environment, stdout=stdout, stderr=stderr, timeout=1800)
    require(made.returncode == 0, "Fresh release producer failed")
    require((OUTPUT / "producer.stderr").stat().st_size <= 16_000_000, "Producer stderr capacity")
    PROGRESS.append("native-role")
    role = producer_role(read_bytes(OUTPUT / "producer.stdout", 16_000_000), PureWindowsPath(str(ROOT)), packages[0]["id"])
    role["metadata"] = metadata(Path(role["path"]), NATIVE_MAX)
    PROGRESS.append("typescript-producer")
    with (OUTPUT / "typescript.stdout").open("xb") as stdout, (OUTPUT / "typescript.stderr").open("xb") as stderr:
        made = subprocess.run(["npm.cmd", "run", "build"], cwd=ROOT, env=environment, stdout=stdout, stderr=stderr, timeout=180)
    require(made.returncode == 0 and source_inputs() == inputs, "Fresh TypeScript/source production failed")
    require(all((OUTPUT / name).stat().st_size <= 4_000_000 for name in ("typescript.stdout", "typescript.stderr")), "TypeScript log capacity")
    require(read_json(ROOT / "node_modules/typescript/package.json")["version"] == "5.8.3", "Locked TypeScript version differs")
    built = built_inputs()
    PROGRESS.append("built-inputs")
    write_json(OUTPUT / "built-inputs.json", {"sourceRevision": revision, "sha256": built})
    write_json(OUTPUT / "producer.json", {"sourceRevision": revision, "complete": True, "nativeRole": role,
        "packageId": packages[0]["id"], "packageVersion": "0.5.0", "command": cargo_command(),
        "freshTargetAndDist": True, "sourceInputsUnchanged": True, "nodeVersion": node, "typescriptVersion": "5.8.3",
        "typescriptCommand": ["npm.cmd", "run", "build"], "nativeExecuted": False})
    state = {"schemaVersion": 1, "sourceRevision": revision, "toolRoot": str(tool), "compilerHomes": homes,
        "workerRoots": {"tool": str(WORKERS / "tool"), "package": str(WORKERS / "package"),
            "explicit": str(temp / ("yvh-explicit-" + os.urandom(8).hex())),
            "bundled": str(temp / ("yvh-bundled-" + os.urandom(8).hex()))}}
    admit_state(state, revision, PureWindowsPath(str(ROOT)), PureWindowsPath(str(temp)))
    write_json(WORKERS / "state.json", state)
    return 0


def confirm_job():
    name = os.environ.get("YOKE_OWNED_JOB", "")
    require(re.fullmatch(r"Local\\YokeVSCode-[0-9a-f]{64}", name) is not None, "Owned Job selection unavailable")
    api = ctypes.WinDLL("kernel32", use_last_error=True)
    for function, args, result in (
        ("OpenJobObjectW", [ctypes.c_uint32, ctypes.c_int, ctypes.c_wchar_p], ctypes.c_void_p),
        ("GetCurrentProcess", [], ctypes.c_void_p),
        ("IsProcessInJob", [ctypes.c_void_p, ctypes.c_void_p, ctypes.POINTER(ctypes.c_int)], ctypes.c_int),
        ("CloseHandle", [ctypes.c_void_p], ctypes.c_int),
    ):
        entry = getattr(api, function)
        entry.argtypes, entry.restype = args, result
    handle = api.OpenJobObjectW(4, False, name)
    require(bool(handle), "Specific owned Job unavailable")
    try:
        assigned = ctypes.c_int()
        require(api.IsProcessInJob(api.GetCurrentProcess(), handle, ctypes.byref(assigned)) and assigned.value,
                "Worker is outside the specific owned Job")
    finally:
        require(api.CloseHandle(handle), "Owned Job query handle close failed")
    return {"specificJobMembership": True, "queryHandleClosedBeforePayloadRead": True}


def zip_directory(tail, start, size, maximum_count):
    require(isinstance(tail, bytes) and len(tail) <= 65577 and start >= 0 and start + len(tail) == size, "ZIP tail capacity/layout")
    # ZipFile selects the last footer signature. Do not validate a different
    # earlier footer when an ambiguous comment could override its byte budget.
    position = tail.rfind(b"PK\x05\x06")
    require(position >= 0 and position + 22 <= len(tail), "ZIP directory unavailable")
    end = struct.unpack_from("<4s4H2LH", tail, position)
    require(position + 22 + end[-1] == len(tail), "Ambiguous ZIP footer/comment refused")
    _, disk, directory_disk, disk_entries, entries, directory_bytes, offset, _ = end
    require(disk == directory_disk == 0 and disk_entries == entries and 0 < entries <= maximum_count
            and directory_bytes <= 8_000_000 and offset + directory_bytes == start + position, "ZIP directory capacity/layout")
    require((start == 0 or position >= 20) and (position < 20 or tail[position - 20:position - 16] != b"PK\x06\x07"), "ZIP64 directory override refused")
    return entries


@contextlib.contextmanager
def admitted_zip(path, maximum_archive, maximum_total, maximum_entry, maximum_count):
    JOB.ordinary(path)
    require(path.stat().st_size <= maximum_archive, "Archive byte capacity")
    with path.open("rb") as stream:
        stream.seek(0, 2)
        size = stream.tell()
        start = max(0, size - 65577)
        stream.seek(start)
        entries = zip_directory(stream.read(65577), start, size, maximum_count)
        stream.seek(0)
        with zipfile.ZipFile(stream) as archive:
            infos = archive.infolist()
            require(len(infos) == entries, "ZIP directory changed")
            yield archive, safe_members(infos, maximum_total, maximum_entry, maximum_count)


def member_hash(archive, info):
    digest, total = hashlib.sha256(), 0
    with archive.open(info) as stream:
        while block := stream.read(1_000_000):
            total += len(block)
            require(total <= info.file_size, "ZIP payload length changed")
            digest.update(block)
    require(total == info.file_size, "ZIP payload length differs")
    return digest.hexdigest()


def extract(archive, members, destination):
    require(not destination.exists(), "Fresh extraction required")
    require(shutil.disk_usage(destination.parent).free >= JOB.MIN_FREE + sum(info.file_size for info in members.values()), "Extraction volume reserve unavailable")
    destination.mkdir()
    JOB.ordinary(destination, True)
    hashes = {}
    for name, info in members.items():
        target = destination.joinpath(*name.rstrip("/").split("/"))
        if info.is_dir():
            target.mkdir(parents=True, exist_ok=True)
            JOB.ordinary(target, True)
            continue
        target.parent.mkdir(parents=True, exist_ok=True)
        JOB.ordinary(target.parent, True)
        value, total = hashlib.sha256(), 0
        with archive.open(info) as source, target.open("xb") as output:
            while block := source.read(1_000_000):
                total += len(block)
                require(total <= info.file_size, "Extraction byte capacity")
                require(shutil.disk_usage(destination).free >= JOB.MIN_FREE + len(block), "Extraction volume reserve unavailable")
                value.update(block)
                output.write(block)
        require(total == info.file_size, "Extraction length differs")
        hashes[name] = value.hexdigest()
    return hashes


def check_tree(directory, pins, maximum_entry, maximum_total):
    JOB.ordinary(directory, True)
    actual, total = set(), 0
    for path in directory.rglob("*"):
        require(not path.is_symlink() and not path.is_junction(), "Linked package/tool input")
        if not path.is_file():
            continue
        name = path.relative_to(directory).as_posix()
        require(name in pins and len(actual) < 12288, "Package/tool inventory differs")
        total += path.stat().st_size
        require(total <= maximum_total and hash_file(path, maximum_entry) == pins[name], "Package/tool bytes changed")
        actual.add(name)
    require(actual == set(pins), "Package/tool files missing")


def tool_phase(state):
    tool = Path(state["toolRoot"])
    JOB.ordinary(tool.parent, True)
    require(not tool.exists(), "Fresh editor tool namespace required")
    require(shutil.disk_usage(tool.parent).free >= JOB.MIN_FREE + CODE_BYTES + CODE_EXPANDED, "Editor volume admission unavailable")
    tool.mkdir()
    archive_path = tool / "editor.zip"

    class FixedRedirect(urllib.request.HTTPRedirectHandler):
        def redirect_request(self, request, file, code, message, headers, new_url):
            parsed = urllib.parse.urlparse(new_url)
            require(parsed.scheme == "https" and parsed.hostname == "vscode.download.prss.microsoft.com"
                    and parsed.port in (None, 443) and parsed.username is None, "Editor redirect refused")
            return super().redirect_request(request, file, code, message, headers, new_url)

    opener = urllib.request.build_opener(FixedRedirect())
    digest, total = hashlib.sha256(), 0
    deadline = time.monotonic() + 120
    request = urllib.request.Request(CODE_URL, headers={"Accept-Encoding": "identity"})
    PROGRESS.append("tool-download")
    with opener.open(request, timeout=20) as response, archive_path.open("xb") as output:
        length = response.headers.get("Content-Length")
        require(length is None or length == str(CODE_BYTES), "Editor download length differs")
        while block := response.read(1_000_000):
            total += len(block)
            require(total <= CODE_BYTES and time.monotonic() < deadline, "Editor procurement budget")
            require(shutil.disk_usage(tool).free >= JOB.MIN_FREE + (CODE_BYTES - total) + CODE_EXPANDED + len(block), "Editor volume reserve unavailable")
            output.write(block)
            digest.update(block)
    require(total == CODE_BYTES and digest.hexdigest() == CODE_SHA256 and time.monotonic() < deadline, "Pinned editor archive or deadline differs")
    PROGRESS.append("tool-extract")
    with admitted_zip(archive_path, CODE_BYTES, CODE_EXPANDED, 256_000_000, 6000) as (archive, members):
        require(len(members) == CODE_ENTRIES and sum(i.file_size for i in members.values()) == CODE_EXPANDED
                and "Code.exe" in members and CODE_PACKAGE in members, "Pinned editor inventory differs")
        pins = extract(archive, members, tool / "editor")
    require(read_json(tool / "editor" / CODE_PACKAGE)["version"] == CODE_VERSION, "Editor package version differs")
    write_json(tool / "tool-inputs.json", pins, 4_000_000)
    code = tool / "editor/Code.exe"
    return {"version": CODE_VERSION, "archiveSHA256": CODE_SHA256, "archiveBytes": CODE_BYTES,
        "files": len(pins), "expandedBytes": CODE_EXPANDED, "inputMapSHA256": hash_file(tool / "tool-inputs.json", 4_000_000),
        "codeSHA256": pins["Code.exe"], "codeMetadata": metadata(code, 256_000_000), "editorExecuted": False}


def package_phase(state, environment, source, built, native_hash):
    native = PACKAGES / "native.zip"
    vsix = PACKAGES / "combined.vsix"
    environment.update(state["compilerHomes"])
    TESTS.check_runner_policy(environment)
    commands = [[sys.executable, "-I", str(ROOT / "scripts/package-native.py"), "--skip-build", "--fetch-missing-licenses", "--output", str(native)],
                [sys.executable, "-I", str(ROOT / "scripts/package-vscode.py"), "--native-archive", str(native), "--include-typescript", "--output", str(vsix)]]
    for label, command in zip(("package-native", "package-vsix"), commands):
        PROGRESS.append(label)
        subprocess.run(command, cwd=ROOT, env=environment, check=True, timeout=180)
    PROGRESS.append("package-reopen")
    expected = {}
    with admitted_zip(native, 256_000_000, 256_000_000, NATIVE_MAX, 8192) as (archive, members):
        require("manifest.json" in members and members["manifest.json"].file_size <= 1_000_000, "Native manifest missing")
        manifest = decode(archive.read(members["manifest.json"]))
        require(manifest.get("version") == "0.5.0" and manifest.get("target") == "x86_64-pc-windows-msvc"
                and manifest.get("binarySha256") == native_hash and "yoke.exe" in members, "Native archive provenance differs")
        for name, info in members.items():
            require(info.file_size <= (NATIVE_MAX if name == "yoke.exe" else 128_000_000), "Original native importer capacity")
            if not info.is_dir():
                expected["extension/runtime/native/" + name] = member_hash(archive, info)
                if name in source:
                    require(expected["extension/runtime/native/" + name] == source[name], "Native archive source bytes differ")
        require(expected["extension/runtime/native/yoke.exe"] == native_hash, "Native archive CLI bytes differ")
        require(isinstance(manifest.get("dependencies"), list) and len(manifest["dependencies"]) <= 4096, "Native license inventory capacity")
        for dependency in manifest["dependencies"]:
            require(isinstance(dependency.get("licenseTexts"), list) and 0 < len(dependency["licenseTexts"]) <= 64, "Native license text inventory incomplete")
            for notice in dependency["licenseTexts"]:
                require(expected.get("extension/runtime/native/" + notice["path"]) == notice["sha256"], "Native license text hash differs")
    for name, digest in source.items():
        if name.startswith("extensions/vscode/") and name != "extensions/vscode/test-host.cjs":
            expected["extension/" + name.removeprefix("extensions/vscode/")] = digest
        if name.startswith("workers/"):
            expected["extension/runtime/" + name] = digest
    expected.update({"extension/runtime/" + name: digest for name, digest in built.items()})
    expected.update({"extension/LICENSE": source["LICENSE"], "extension/runtime/package.json": source["package.json"],
                     "extension/runtime/THIRD_PARTY_NOTICES.md": source["THIRD_PARTY_NOTICES.md"]})
    with admitted_zip(vsix, 256_000_000, 384_000_000, NATIVE_MAX, 12288) as (archive, members):
        files = {name for name, info in members.items() if not info.is_dir()}
        require(files == set(expected) | {"extension.vsixmanifest", "[Content_Types].xml"}, "Combined VSIX inventory differs")
        for name, info in members.items():
            require(info.file_size <= (NATIVE_MAX if name == "extension/runtime/native/yoke.exe" else 128_000_000), "Original importer entry bound")
        require(members["extension.vsixmanifest"].file_size <= 1_000_000 and members["extension/package.json"].file_size <= 1_000_000, "VSIX manifest capacity")
        identity_row = ET.fromstring(archive.read(members["extension.vsixmanifest"])).find("{http://schemas.microsoft.com/developer/vsx-schema/2011}Metadata/{http://schemas.microsoft.com/developer/vsx-schema/2011}Identity")
        require(identity_row is not None and all(identity_row.get(k) == v for k, v in
            {"Version": "0.5.0", "Publisher": "hecer", "Id": "code-intelligence-local", "TargetPlatform": "win32-x64"}.items()), "VSIX identity differs")
        package = decode(archive.read(members["extension/package.json"]))
        require(package["version"] == "0.5.0" and package["publisher"] == "hecer" and package["name"] == "code-intelligence-local", "Extension manifest differs")
        for name, digest in expected.items():
            require(member_hash(archive, members[name]) == digest, "Combined VSIX member bytes differ")
        extracted = extract(archive, members, PACKAGES / "unpacked")
    write_json(PACKAGES / "package-inputs.json", extracted, 4_000_000)
    bundled = PACKAGES / "unpacked/extension/runtime/native/yoke.exe"
    require(extracted["extension/runtime/native/yoke.exe"] == native_hash, "Bundled CLI differs")
    return {"nativeArchiveSHA256": hash_file(native, 256_000_000), "vsixSHA256": hash_file(vsix, 256_000_000),
        "members": len(extracted), "inputMapSHA256": hash_file(PACKAGES / "package-inputs.json", 4_000_000),
        "bundledMetadata": metadata(bundled, NATIVE_MAX), "bundledNativeSHA256": native_hash,
        "typescriptRuntimeBundledAndByteVerified": True, "typescriptExtensionModeExecuted": False}


def host_phase(phase, state, environment, native_hash):
    PROGRESS.append("host-inputs")
    tool = Path(state["toolRoot"])
    tool_result = read_json(OUTPUT / "tool.json")["observation"]
    require(hash_file(tool / "tool-inputs.json", 4_000_000) == tool_result["inputMapSHA256"], "Editor input map changed")
    check_tree(tool / "editor", read_json(tool / "tool-inputs.json", 4_000_000), 256_000_000, CODE_EXPANDED)
    code = tool / "editor/Code.exe"
    require(metadata(code, 256_000_000) == tool_result["codeMetadata"], "Editor identity changed")
    packaged = read_json(OUTPUT / "package.json")["observation"]
    require(hash_file(PACKAGES / "combined.vsix", 256_000_000) == packaged["vsixSHA256"]
            and hash_file(PACKAGES / "package-inputs.json", 4_000_000) == packaged["inputMapSHA256"], "VSIX input changed")
    check_tree(PACKAGES / "unpacked", read_json(PACKAGES / "package-inputs.json", 4_000_000), NATIVE_MAX, 384_000_000)
    bundled = phase == "bundled"
    native = PACKAGES / "unpacked/extension/runtime/native/yoke.exe" if bundled else ROOT / "target/release/yoke.exe"
    require(hash_file(native, NATIVE_MAX) == native_hash, "Selected host CLI differs")
    fixture = Path(state["workerRoots"][phase]) / "host"
    fixture.mkdir()
    workspace, profile, extensions = fixture / "workspace", fixture / "profile", fixture / "extensions"
    for directory in (workspace / ".vscode", profile / "User", extensions):
        directory.mkdir(parents=True)
    (workspace / "main.rs").write_bytes(b'fn main() { println!("fixture"); }\n')
    write_json(workspace / ".vscode/settings.json", {"codeIntelligence.nativeExecutable": str(workspace / "must-not-run.exe"), "codeIntelligence.runtime": "typescript"})
    write_json(profile / "User/settings.json", {"security.workspace.trust.enabled": False, "telemetry.telemetryLevel": "off",
        "update.mode": "none", "extensions.autoCheckUpdates": False, "extensions.autoUpdate": False,
        "workbench.startupEditor": "none", "window.restoreWindows": "none"})
    extension = PACKAGES / "unpacked/extension" if bundled else ROOT / "extensions/vscode"
    spec = JOB.admit_spec({"schemaVersion": 1, "fixtureRoot": str(fixture), "program": str(code),
        "repositoryRoot": str(ROOT), "extensionDirectory": str(extension), "timeoutSeconds": 180})
    for key in ("GH_TOKEN", "GITHUB_TOKEN", "ACTIONS_RUNTIME_TOKEN", "ACTIONS_ID_TOKEN_REQUEST_TOKEN"):
        environment.pop(key, None)
    environment.update(YOKE_VSCODE_NATIVE=str(native), YOKE_VSCODE_RESULT=str(fixture / "result.json"), YOKE_VSCODE_BUNDLED="1" if bundled else "0")
    startup = subprocess.STARTUPINFO()
    startup.dwFlags = subprocess.STARTF_USESHOWWINDOW
    startup.wShowWindow = subprocess.SW_HIDE
    PROGRESS.append("host-launch")
    with (fixture / "editor.stdout").open("xb") as stdout, (fixture / "editor.stderr").open("xb") as stderr:
        ran = subprocess.run([str(code), *JOB.code_arguments(spec)], cwd=workspace, env=environment,
            stdout=stdout, stderr=stderr, startupinfo=startup, creationflags=subprocess.CREATE_NO_WINDOW, timeout=180)
    require(ran.returncode == 0, "Actual editor root did not exit successfully")
    PROGRESS.append("host-report")
    report = read_json(fixture / "result.json", 65536)
    admit_host_report(report, bundled)
    require((workspace / "main.rs").read_bytes() == b'fn main() { println!("fixture"); }\n', "Fixture saved source changed")
    require(hash_file(native, NATIVE_MAX) == native_hash, "Host CLI changed during execution")
    check_tree(tool / "editor", read_json(tool / "tool-inputs.json", 4_000_000), 256_000_000, CODE_EXPANDED)
    check_tree(PACKAGES / "unpacked", read_json(PACKAGES / "package-inputs.json", 4_000_000), NATIVE_MAX, 384_000_000)
    return {"hostReport": report, "editorExitCode": ran.returncode, "nativeSHA256": native_hash,
            "originalTestHostSHA256": hash_file(ROOT / "extensions/vscode/test-host.cjs", 1_000_000),
            "editorInputsUnchanged": True, "VSIXInputsByteVerified": True, "savedFixtureSourceUnchanged": True}


def run_worker(phase):
    revision, environment = identity()
    admission = confirm_job()
    PROGRESS.append("job")
    state = load_state(revision)
    require(state["schemaVersion"] == 1 and state["sourceRevision"] == revision, "Worker source differs")
    source = read_json(OUTPUT / "source-inputs.json")
    built = read_json(OUTPUT / "built-inputs.json")
    producer = read_json(OUTPUT / "producer.json")
    require(source["sourceRevision"] == built["sourceRevision"] == producer["sourceRevision"] == revision
            and producer["complete"] is True and producer["freshTargetAndDist"] is True
            and producer["sourceInputsUnchanged"] is True and producer["command"] == cargo_command()
            and source_inputs() == source["sha256"] and built_inputs() == built["sha256"], "Source/production inputs changed")
    PROGRESS.append("source")
    role = producer_role(read_bytes(OUTPUT / "producer.stdout", 16_000_000), PureWindowsPath(str(ROOT)), producer["packageId"])
    native = Path(role["path"])
    require(role["path"] == producer["nativeRole"]["path"] and metadata(native, NATIVE_MAX) == producer["nativeRole"]["metadata"], "Native producer metadata changed")
    native_hash = hash_file(native, NATIVE_MAX)
    require(metadata(native, NATIVE_MAX) == producer["nativeRole"]["metadata"], "Native changed while hashing")
    if phase == "tool":
        observation = tool_phase(state)
    elif phase == "package":
        observation = package_phase(state, environment, source["sha256"], built["sha256"], native_hash)
    else:
        observation = host_phase(phase, state, environment, native_hash)
    PROGRESS.append("final-inputs")
    require(source_inputs() == source["sha256"] and built_inputs() == built["sha256"]
            and metadata(native, NATIVE_MAX) == producer["nativeRole"]["metadata"]
            and hash_file(native, NATIVE_MAX) == native_hash, "Inputs changed during owned stage")
    write_json(OUTPUT / (phase + ".json"), {"schemaVersion": 1, "sourceRevision": revision, "phase": phase, "complete": True,
        "workerAdmission": admission, "nativeSHA256": native_hash, "sourceInputsUnchanged": True, "builtInputsUnchanged": True,
        "observation": observation})
    return 0


def worker(phase):
    complete = False
    try:
        value = run_worker(phase)
        complete = value == 0
        return value
    finally:
        if PROGRESS and PROGRESS[0] == "job":
            require(len(PROGRESS) <= 16, "Worker phase capacity")
            state = load_state(os.environ["GITHUB_SHA"])
            write_json(Path(state["workerRoots"][phase]) / "worker-status.json", {"phase": phase, "reached": PROGRESS, "complete": complete})


def stage(phase):
    revision, _ = identity()
    state = load_state(revision)
    require(state["sourceRevision"] == revision, "Stage source differs")
    order = ["tool", "package", "explicit", "bundled"]
    for earlier in order[:order.index(phase)]:
        admit_job(read_json(OUTPUT / (earlier + "-job.json")))
        prior = read_json(OUTPUT / (earlier + ".json"))
        admit_stage_result(prior, earlier, revision)
    fixture = Path(state["workerRoots"][phase])
    require(not fixture.exists() and not (OUTPUT / (phase + ".json")).exists(), "Fresh stage required")
    fixture.mkdir()
    row = JOB.run_owned(Path(sys.executable), ["-I", str(Path(__file__).resolve()), "--worker", phase], fixture, 180)
    for name in ("job.json", "stdout.log", "stderr.log"):
        (OUTPUT / (phase + "-" + name)).write_bytes(read_bytes(fixture / name, 4_000_000))
    for name in ("worker-status.json", "host/editor.stdout", "host/editor.stderr"):
        if (fixture / name).exists():
            target = OUTPUT / (phase + "-" + name.replace("/", "-"))
            require(not target.exists(), "Fresh diagnostic text output required")
            target.write_bytes(read_bytes(fixture / name, 4_000_000))
    admit_job(row)
    result = read_json(OUTPUT / (phase + ".json"))
    admit_stage_result(result, phase, revision)
    return 0


def finish():
    revision, _ = identity()
    rows = {}
    for phase in ("tool", "package", "explicit", "bundled"):
        admit_job(read_json(OUTPUT / (phase + "-job.json")))
        row = read_json(OUTPUT / (phase + ".json"))
        admit_stage_result(row, phase, revision)
        rows[phase] = row
    require(len({r["nativeSHA256"] for r in rows.values()}) == 1, "Stage native identities differ")
    for phase in ("explicit", "bundled"):
        admit_host_report(rows[phase]["observation"]["hostReport"], phase == "bundled")
    require(source_inputs() == read_json(OUTPUT / "source-inputs.json")["sha256"]
            and built_inputs() == read_json(OUTPUT / "built-inputs.json")["sha256"], "Final source/built inputs changed")
    write_json(OUTPUT / "result.json", {"schemaVersion": 1, "sourceRevision": revision, "complete": True,
        "product": "Code Intelligence", "nativeVersion": "0.5.0", "retainedTypeScriptVersion": "0.3.0", "editorVersion": CODE_VERSION,
        "scope": "Fresh combined VSIX bytes and explicit/bundled real Rust extension-host assertions",
        "explicitRustHostPassed": True, "bundledRustHostPassed": True, "allOwnedTreesExitedBeforeCleanup": True,
        "typescriptRuntimeBundledAndByteVerified": True, "typescriptExtensionModeExecuted": False,
        "wholePlatformCombinedFeatureParityMainOrReleaseAccepted": False})
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("operation", choices=("prepare", "stage", "finish"), nargs="?")
    parser.add_argument("--phase", choices=("tool", "package", "explicit", "bundled"))
    parser.add_argument("--worker", choices=("tool", "package", "explicit", "bundled"), help=argparse.SUPPRESS)
    args = parser.parse_args()
    require((args.worker is not None) != (args.operation is not None), "Select one acceptance operation")
    if args.worker:
        return worker(args.worker)
    require((args.operation == "stage") == (args.phase is not None), "Select one fixed stage")
    complete = False
    try:
        value = prepare() if args.operation == "prepare" else stage(args.phase) if args.operation == "stage" else finish()
        complete = value == 0
        return value
    finally:
        if args.operation == "prepare" and PROGRESS:
            write_json(OUTPUT / "preparation-status.json", {"reached": PROGRESS, "complete": complete, "nativeExecuted": False})


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Exception as error:
        print("Fresh VSIX acceptance refused: " + type(error).__name__, file=sys.stderr)
        sys.exit(1)
