"""Separate explicit CI acceptance for one existing Windows credential/TLS case.

Default target/shard coverage and the case's ignore, assertions and deadlines are
unchanged. Fresh Cargo output is admitted before the assigned-before-resume
worker runs only this exact case. No existing user credential is selected.
"""
import argparse
import ctypes
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
CASE = "actual_tls_receiver_and_sender_cli_recover_using_os_credentials_and_selected_ca"
MAX_IMAGE = 256_000_000
MAX_PRODUCER = 16_000_000
MAX_CASE_LOG = 1_000_000
OUTPUT_REL = Path("validation/ci-host-credential-tls")
BUILD_REL = Path("validation/ci-credential-build")
FIXTURE_REL = Path("validation/ci-credential-fixture")


def sibling(name):
    spec = importlib.util.spec_from_file_location(name, ROOT / "scripts" / (name + ".py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


OWNED = sibling("host_history_live")
TESTS = sibling("run_native_tests")
require = OWNED.require
write_json = OWNED.write_json
read_json = OWNED.read_json
output_size = OWNED.output_size
worker_job_name = OWNED.worker_job_name
MIN_FREE = OWNED.MIN_FREE


def ci_identity():
    require(os.environ.get("GITHUB_ACTIONS") == "true" and os.environ.get("RUNNER_OS") == "Windows"
            and os.name == "nt" and sys.platform == "win32" and ctypes.sizeof(ctypes.c_void_p) == 8,
            "Credential TLS acceptance requires explicit 64-bit Windows CI")
    require(shutil.disk_usage(ROOT).free >= MIN_FREE, "Credential TLS disk reserve unavailable")
    environment = TESTS.controlled_environment()
    TESTS.check_runner_policy(environment)
    require(not any(key in environment for key in ("CARGO_TARGET_DIR", "RUSTC", "RUSTC_WRAPPER",
                                                  "RUSTC_WORKSPACE_WRAPPER")),
            "Credential TLS producer requires its fixed host/compiler selection")
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True, timeout=30).strip()
    require(re.fullmatch(r"[0-9a-f]{40}", revision) is not None
            and revision == os.environ.get("GITHUB_SHA"), "Credential TLS source revision differs")
    return revision, environment


def ordinary(path, directory=False):
    require(path.is_absolute() and path.is_relative_to(ROOT), "Credential TLS path outside checkout")
    at = path
    while True:
        require(not at.is_symlink() and not at.is_junction(), "Linked credential TLS path refused")
        if at == ROOT:
            break
        at = at.parent
    require(path.is_dir() if directory else path.is_file(), "Credential TLS path unavailable")


def metadata(path):
    ordinary(path)
    value = path.stat()
    require(0 < value.st_size <= MAX_IMAGE, "Credential TLS image size admission failed")
    return {"bytes": value.st_size, "mtimeNs": value.st_mtime_ns,
            "device": value.st_dev, "inode": value.st_ino}


def producer_command(build):
    return ["cargo", "test", "--locked", "-j", "2", "--package", "yoke-code-intelligence",
            "--test", "digest_receiver", "--no-run", "--message-format=json",
            "--target-dir", str(build)]


def artifact_roles(raw, build, package_id):
    require(isinstance(raw, bytes) and 0 < len(raw) <= MAX_PRODUCER, "Credential TLS producer log capacity")
    require(isinstance(package_id, str) and 0 < len(package_id) <= 4096,
            "Credential TLS package identity unavailable")
    expected_cli = build / "debug/yoke.exe"
    expected_tests = build / "debug/deps"
    roles = {}
    finished = 0
    for line in raw.splitlines():
        require(finished == 0, "Cargo evidence follows build completion")
        value = OWNED.decode(line)
        require(isinstance(value, dict), "Invalid Cargo producer record")
        if value.get("reason") == "build-finished":
            require(type(value.get("success")) is bool and value["success"], "Cargo producer did not succeed")
            finished += 1
        if value.get("reason") != "compiler-artifact" or value.get("package_id") != package_id:
            continue
        target, profile = value.get("target"), value.get("profile")
        require(isinstance(target, dict) and isinstance(profile, dict), "Incomplete Cargo target evidence")
        role = ("cli" if target.get("name") == "yoke" and target.get("kind") == ["bin"]
                and profile.get("test") is False else
                "test" if target.get("name") == "digest_receiver" and target.get("kind") == ["test"]
                and profile.get("test") is True else None)
        if role is None:
            continue
        executable = value.get("executable")
        filenames = value.get("filenames")
        require(type(value.get("fresh")) is bool and value["fresh"] is False
                and isinstance(executable, str) and isinstance(filenames, list)
                and 0 < len(filenames) <= 32 and all(isinstance(p, str) for p in filenames)
                and executable in filenames,
                "Fresh Cargo executable evidence required")
        path = Path(executable)
        require(path.is_absolute() and ".." not in path.parts and role not in roles,
                "Ambiguous credential TLS executable")
        require(path == expected_cli if role == "cli" else
                path.parent == expected_tests and re.fullmatch(r"digest_receiver-[0-9a-f]{16}\.exe", path.name),
                "Credential TLS executable role/path differs")
        roles[role] = {"path": str(path), "target": target["name"]}
    require(finished == 1 and set(roles) == {"cli", "test"}, "Incomplete paired Cargo producer")
    return roles


def case_result(raw):
    require(isinstance(raw, bytes) and len(raw) <= MAX_CASE_LOG, "Credential TLS case log capacity")
    text = raw.decode("utf-8").replace("\r\n", "\n")
    rows = re.findall(r"(?m)^test ([A-Za-z0-9_:]+) \.\.\. (ok|FAILED|ignored)(?:, [^\n]*)?$", text)
    require(rows == [(CASE, "ok")] and re.search(r"(?m)^running 1 test$", text) is not None,
            "Exact credential TLS case did not pass")
    summaries = re.findall(r"(?m)^test result: ok\. 1 passed; 0 failed; 0 ignored; 0 measured; ([0-9]+) filtered out; finished in [0-9.]+s$", text)
    require(len(summaries) == 1 and 0 < int(summaries[0]) <= 4096, "Credential TLS libtest result differs")
    return {"case": CASE, "passed": 1, "failed": 0, "ignored": 0, "filteredOut": int(summaries[0]),
            "wholeCargoTarget": False, "countsAreNotCombined": True}


def prepare():
    revision, environment = ci_identity()
    validation = ROOT / "validation"
    require(not validation.is_symlink() and not validation.is_junction(), "Linked validation namespace refused")
    validation.mkdir(exist_ok=True)
    ordinary(validation, True)
    output, build = ROOT / OUTPUT_REL, ROOT / BUILD_REL
    require(not output.exists() and not build.exists(), "Fresh credential TLS output namespaces required")
    output.mkdir()
    build.mkdir()
    inputs = OWNED.source_inputs()
    write_json(output / "source-inputs.json", {"sourceRevision": revision, "sha256": inputs})
    complete = False
    try:
        meta = subprocess.run(["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"],
                              cwd=ROOT, env=environment, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                              timeout=60, check=True)
        value = OWNED.decode(meta.stdout)
        packages = [p for p in value["packages"] if p["name"] == "yoke-code-intelligence"
                    and Path(p["manifest_path"]) == ROOT / "native/Cargo.toml"
                    and p["id"] in value["workspace_members"]]
        require(len(packages) == 1, "Credential TLS package selection differs")
        with (output / "producer.stdout").open("xb") as stdout, (output / "producer.stderr").open("xb") as stderr:
            produced = subprocess.run(producer_command(build), cwd=ROOT, env=environment,
                                      stdout=stdout, stderr=stderr, timeout=1800)
        require(produced.returncode == 0, "Credential TLS producer failed")
        output_size(output)
        log = output / "producer.stdout"
        require(log.stat().st_size <= MAX_PRODUCER, "Credential TLS producer log capacity")
        roles = artifact_roles(log.read_bytes(), build, packages[0]["id"])
        for role in roles.values():
            role["metadata"] = metadata(Path(role["path"]))
        require(OWNED.source_inputs() == inputs, "Credential TLS producer source changed")
        write_json(output / "producer.json", {"schemaVersion": 1, "sourceRevision": revision,
            "roles": roles, "freshTargetDirectory": True, "packageVersion": packages[0]["version"],
            "packageId": packages[0]["id"],
            "command": producer_command(build), "sourceInputsUnchanged": True})
        complete = True
    finally:
        write_json(output / "preparation-result.json", {"complete": complete, "sourceRevision": revision,
                   "nativeExecuted": False, "scope": "Fresh compilation only; not TLS runtime acceptance"})
    return 0


def test_command(test):
    return [str(test), "--exact", CASE, "--ignored", "--test-threads=1"]


def admit_job(row):
    require(row.get("startedSuspended") is True and row.get("assignedBeforeResume") is True
            and type(row.get("exitCode")) is int and row["exitCode"] == 0
            and row.get("treeExitedBeforeCleanup") is True
            and type(row.get("activeBeforeCleanup")) is int and row["activeBeforeCleanup"] == 0
            and type(row.get("activeAfterCleanup")) is int and row["activeAfterCleanup"] == 0
            and row.get("complete") is True, "Credential TLS owned process-tree proof incomplete")


def bounded_read(path, maximum):
    ordinary(path)
    require(path.stat().st_size <= maximum, "Credential TLS retained log capacity")
    with path.open("rb") as stream:
        raw = stream.read(maximum + 1)
    require(len(raw) <= maximum, "Credential TLS retained log capacity")
    return raw


def admit_case(result, revision, stdout, stderr):
    require(result.get("complete") is True and result.get("sourceRevision") == revision
            and result.get("workerJobAdmission") == {"specificJobMembershipConfirmed": True,
                "queryHandleClosedBeforeNativeRead": True}, "Credential TLS worker proof incomplete")
    require(len(stderr) <= MAX_CASE_LOG and case_result(stdout) == result.get("observation"),
            "Credential TLS case result differs")
    hashes = result.get("imageSHA256")
    require(isinstance(hashes, dict) and set(hashes) == {"cli", "test"}
            and all(isinstance(h, str) and re.fullmatch(r"[0-9a-f]{64}", h) for h in hashes.values()),
            "Credential TLS image hash evidence incomplete")
    require(result.get("logSHA256") == {"case.stdout": hashlib.sha256(stdout).hexdigest(),
            "case.stderr": hashlib.sha256(stderr).hexdigest()}, "Credential TLS log hash differs")


def bounded_worker(spec_path, fixture, output, environment, job_name):
    # The layout and suspended-assignment ordering follow the already exercised
    # repository Windows runner, with explicit 64-bit admission and cleanup proof.
    import _winapi
    import msvcrt

    class Basic(ctypes.Structure):
        _fields_ = [("processTime", ctypes.c_int64), ("jobTime", ctypes.c_int64), ("flags", ctypes.c_uint32),
                    ("minWorking", ctypes.c_size_t), ("maxWorking", ctypes.c_size_t),
                    ("activeLimit", ctypes.c_uint32), ("affinity", ctypes.c_size_t),
                    ("priority", ctypes.c_uint32), ("scheduling", ctypes.c_uint32)]

    class Extended(ctypes.Structure):
        _fields_ = [("basic", Basic), ("io", ctypes.c_uint64 * 6), ("processMemory", ctypes.c_size_t),
                    ("jobMemory", ctypes.c_size_t), ("peakProcessMemory", ctypes.c_size_t),
                    ("peakJobMemory", ctypes.c_size_t)]

    require(ctypes.sizeof(Extended) == 144, "Unsupported Windows Job layout")
    api = ctypes.WinDLL("kernel32", use_last_error=True)
    for name, args, result in (
        ("CreateJobObjectW", [ctypes.c_void_p, ctypes.c_wchar_p], ctypes.c_void_p),
        ("SetInformationJobObject", [ctypes.c_void_p, ctypes.c_int, ctypes.c_void_p, ctypes.c_uint32], ctypes.c_int),
        ("AssignProcessToJobObject", [ctypes.c_void_p, ctypes.c_void_p], ctypes.c_int),
        ("QueryInformationJobObject", [ctypes.c_void_p, ctypes.c_int, ctypes.c_void_p, ctypes.c_uint32, ctypes.c_void_p], ctypes.c_int),
        ("TerminateJobObject", [ctypes.c_void_p, ctypes.c_uint32], ctypes.c_int),
        ("ResumeThread", [ctypes.c_void_p], ctypes.c_uint32), ("CloseHandle", [ctypes.c_void_p], ctypes.c_int),
    ):
        function = getattr(api, name)
        function.argtypes, function.restype = args, result
    require(shutil.disk_usage(ROOT).free >= MIN_FREE, "Native live acceptance disk reserve unavailable")
    job_name = worker_job_name(job_name)
    ctypes.set_last_error(0)
    job = api.CreateJobObjectW(None, job_name)
    require(bool(job), "Windows Job unavailable")
    if ctypes.get_last_error() == 183:
        require(api.CloseHandle(job), "Occupied Job handle close failed")
        raise RuntimeError("Owned worker Job name is already occupied")
    process = thread = None
    row = {"startedSuspended": True, "assignedBeforeResume": False, "complete": False}

    def active():
        info = ctypes.create_string_buffer(48)
        require(api.QueryInformationJobObject(job, 1, info, 48, None), "Windows Job query failed")
        return int.from_bytes(info.raw[40:44], "little")

    try:
        limits = Extended()
        limits.basic.flags = 0x2000
        require(api.SetInformationJobObject(job, 9, ctypes.byref(limits), ctypes.sizeof(limits)), "Job limit failed")
        with open(os.devnull, "rb") as stdin, (output / "worker.stdout").open("xb") as stdout, (output / "worker.stderr").open("xb") as stderr:
            handles = [msvcrt.get_osfhandle(stream.fileno()) for stream in (stdin, stdout, stderr)]
            for handle in handles:
                os.set_handle_inheritable(handle, True)
            startup = subprocess.STARTUPINFO()
            startup.dwFlags = subprocess.STARTF_USESTDHANDLES
            startup.hStdInput, startup.hStdOutput, startup.hStdError = handles
            startup.lpAttributeList = {"handle_list": handles}
            argv = [sys.executable, "-I", str(Path(__file__).resolve()), "--worker", str(spec_path)]
            process, thread, pid, _ = _winapi.CreateProcess(
                None, subprocess.list2cmdline(argv), None, None, True,
                subprocess.CREATE_NO_WINDOW | 0x00000004, environment, str(fixture), startup)
            row["pid"] = pid
            require(api.AssignProcessToJobObject(job, process), "Suspended Job assignment failed")
            row["assignedBeforeResume"] = True
            require(api.ResumeThread(thread) != 0xffffffff, "Worker resume failed")
            deadline = time.monotonic() + 180
            while _winapi.WaitForSingleObject(process, 100) == 258:
                require(time.monotonic() < deadline, "Owned live acceptance deadline")
                require(shutil.disk_usage(ROOT).free >= MIN_FREE, "Owned disk-headroom stop")
                output_size(output)
                output_size(fixture)
            row["exitCode"] = _winapi.GetExitCodeProcess(process)
            row["activeAtParentExit"] = active()
            # A signalled root alone does not prove that its assigned tree has
            # exited. Observe accounting settling before forced cleanup, using
            # at most one second within the original worker deadline.
            drain_deadline = min(deadline, time.monotonic() + 1)
            remaining_active = row["activeAtParentExit"]
            while remaining_active and time.monotonic() < drain_deadline:
                time.sleep(.01)
                remaining_active = active()
            row["activeBeforeCleanup"] = remaining_active
            row["treeExitedBeforeCleanup"] = remaining_active == 0
    finally:
        if process is not None and not row["assignedBeforeResume"]:
            _winapi.TerminateProcess(process, 124)
        require(api.TerminateJobObject(job, 125), "Owned Job termination failed")
        deadline = time.monotonic() + 10
        while active() and time.monotonic() < deadline:
            time.sleep(.05)
        row["activeAfterCleanup"] = active()
        for handle in (thread, process, job):
            if handle is not None:
                require(api.CloseHandle(handle), "Owned Job handle close failed")
        row["complete"] = (row.get("exitCode") == 0
                           and row.get("treeExitedBeforeCleanup") is True
                           and row["activeAfterCleanup"] == 0)
        write_json(output / "job.json", row)
    require(row["complete"], "Owned live worker did not complete")


def worker(spec_path):
    revision, environment = ci_identity()
    require(spec_path == ROOT / FIXTURE_REL / "spec.json", "Credential TLS worker path differs")
    spec = read_json(spec_path)
    require(set(spec) == {"schemaVersion", "sourceRevision", "output", "fixture", "workerJobName"},
            "Credential TLS worker specification differs")
    require(type(spec["schemaVersion"]) is int and spec["schemaVersion"] == 1 and spec["sourceRevision"] == revision,
            "Credential TLS worker source differs")
    admission = OWNED.require_worker_job(spec["workerJobName"])
    output, fixture = ROOT / OUTPUT_REL, ROOT / FIXTURE_REL
    require(Path(spec["output"]) == output and Path(spec["fixture"]) == fixture,
            "Credential TLS worker namespace differs")
    ordinary(output, True)
    ordinary(fixture, True)
    producer = read_json(output / "producer.json")
    inputs = read_json(output / "source-inputs.json")
    require(producer["sourceRevision"] == inputs["sourceRevision"] == revision
            and producer["freshTargetDirectory"] is True and producer["sourceInputsUnchanged"] is True
            and producer["command"] == producer_command(ROOT / BUILD_REL)
            and set(producer["roles"]) == {"cli", "test"}
            and OWNED.source_inputs() == inputs["sha256"],
            "Credential TLS source/producer evidence differs")
    expected = artifact_roles(bounded_read(output / "producer.stdout", MAX_PRODUCER),
                              ROOT / BUILD_REL, producer["packageId"])
    hashes = {}
    for name, role in producer["roles"].items():
        path = Path(role["path"])
        require(expected[name]["path"] == str(path) and metadata(path) == role["metadata"],
                "Credential TLS image metadata differs")
        hashes[name] = OWNED.sha(path)
        require(metadata(path) == role["metadata"], "Credential TLS image changed while hashing")
    test = Path(producer["roles"]["test"]["path"])
    with (output / "case.stdout").open("xb") as stdout, (output / "case.stderr").open("xb") as stderr:
        result = subprocess.run(test_command(test), cwd=fixture, env=environment,
                                stdout=stdout, stderr=stderr, timeout=120)
    require(result.returncode == 0, "Credential TLS native case failed")
    require((output / "case.stderr").stat().st_size <= MAX_CASE_LOG, "Credential TLS stderr capacity")
    require((output / "case.stdout").stat().st_size <= MAX_CASE_LOG, "Credential TLS stdout capacity")
    observation = case_result(bounded_read(output / "case.stdout", MAX_CASE_LOG))
    for name, role in producer["roles"].items():
        path = Path(role["path"])
        require(metadata(path) == role["metadata"] and OWNED.sha(path) == hashes[name],
                "Credential TLS image changed during runtime")
    require(OWNED.source_inputs() == inputs["sha256"], "Credential TLS runtime source changed")
    write_json(output / "case-result.json", {"complete": True, "sourceRevision": revision,
        "workerJobAdmission": admission, "observation": observation, "imageSHA256": hashes,
        "credentialCleanupScope": "The unchanged passing test deletes its own three random credentials",
        "logSHA256": {n: OWNED.sha(output / n) for n in ("case.stdout", "case.stderr")}})
    return 0


def run():
    revision, environment = ci_identity()
    output, fixture = ROOT / OUTPUT_REL, ROOT / FIXTURE_REL
    ordinary(output, True)
    require(read_json(output / "preparation-result.json").get("complete") is True,
            "Credential TLS fresh producer required")
    require(not fixture.exists(), "Fresh credential TLS fixture required")
    fixture.mkdir()
    for key, name in (("HOME", "home"), ("USERPROFILE", "home"), ("APPDATA", "appdata"),
                      ("LOCALAPPDATA", "local"), ("TEMP", "tmp"), ("TMP", "tmp")):
        path = fixture / name
        path.mkdir(exist_ok=True)
        environment[key] = str(path)
    environment.update(PYTHONUTF8="1", PYTHONDONTWRITEBYTECODE="1", DO_NOT_TRACK="1")
    job_name = "Local\\YokeHistoryCI-" + os.urandom(32).hex()
    spec = fixture / "spec.json"
    write_json(spec, {"schemaVersion": 1, "sourceRevision": revision, "output": str(output),
                     "fixture": str(fixture), "workerJobName": job_name})
    complete = False
    try:
        bounded_worker(spec, fixture, output, environment, job_name)
        admit_job(read_json(output / "job.json"))
        result = read_json(output / "case-result.json")
        admit_case(result, revision, bounded_read(output / "case.stdout", MAX_CASE_LOG),
                   bounded_read(output / "case.stderr", MAX_CASE_LOG))
        inputs = read_json(output / "source-inputs.json")
        require(OWNED.source_inputs() == inputs["sha256"], "Credential TLS final source differs")
        subprocess.run(["git", "diff", "--no-ext-diff", "--no-textconv", "--exit-code", "HEAD", "--"],
                       cwd=ROOT, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        output_size(output)
        complete = True
    finally:
        write_json(output / "result.json", {"schemaVersion": 1, "product": "Code Intelligence",
            "version": "0.5.0", "sourceRevision": revision, "complete": complete,
            "scope": "One explicitly selected existing Windows credential-backed TLS case",
            "case": CASE, "wholeCargoTarget": False, "countsAreNotCombined": True,
            "defaultIgnoredCaseUnchanged": True, "fullPlatformOrReleaseAccepted": False})
    return 0


def main(arguments=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("operation", choices=("prepare", "run"), nargs="?")
    parser.add_argument("--worker", type=Path, help=argparse.SUPPRESS)
    args = parser.parse_args(arguments)
    require((args.worker is not None) != (args.operation is not None), "Select one credential TLS operation")
    if args.worker is not None:
        return worker(args.worker)
    return prepare() if args.operation == "prepare" else run()


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Exception as error:
        # A fixed category prevents compiler/credential contents entering failure text.
        print("Credential TLS acceptance refused: " + type(error).__name__, file=sys.stderr)
        sys.exit(1)
