"""Owned Windows process-tree gate for the explicit VS Code test launcher.

This proves lifecycle only. The caller still owns native provenance, packaging
and the unchanged extension-host assertions. No image payload is read here.
"""
import argparse
import ctypes
import json
import os
from pathlib import Path, PureWindowsPath
import re
import shutil
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
MIN_FREE = 500_000_000
MAX_OUTPUT = 128_000_000
MAX_ENTRIES = 20_000


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


def decode(raw):
    require(isinstance(raw, bytes) and len(raw) <= 32768, "Specification capacity")
    return json.loads(raw, object_pairs_hook=unique_object, parse_constant=reject_constant)


def windows_path(value):
    require(isinstance(value, str) and 0 < len(value) <= 4096
            and re.fullmatch(r"[A-Za-z]:[\\/].+", value) is not None, "Absolute local Windows path required")
    for part in re.split(r"[\\/]", value[3:]):
        require(part not in ("", ".", "..") and part[-1] not in " ."
                and not any(ord(c) < 32 or ord(c) == 127 or c in ':*?"<>|' for c in part), "Unsafe path component")
        require(len(part.encode("utf-16-le")) <= 510, "Path component capacity")
        device = part.split(".", 1)[0].lower()
        require(device not in {"con", "prn", "aux", "nul", "conin$", "conout$"}
                and re.fullmatch(r"(?:com|lpt)[1-9¹²³]", device) is None, "Device path refused")
    return PureWindowsPath(value)


def admit_spec(value):
    require(isinstance(value, dict) and set(value) == {"schemaVersion", "fixtureRoot", "program",
            "repositoryRoot", "extensionDirectory", "timeoutSeconds"}, "Specification fields differ")
    require(type(value["schemaVersion"]) is int and value["schemaVersion"] == 1, "Specification schema")
    require(type(value["timeoutSeconds"]) is int and 1 <= value["timeoutSeconds"] <= 180, "Original deadline capacity")
    for key in ("fixtureRoot", "program", "repositoryRoot", "extensionDirectory"):
        windows_path(value[key])
    root, program = windows_path(value["fixtureRoot"]), windows_path(value["program"])
    require(program.name.lower() == "code.exe" and not program.is_relative_to(root), "Selected external Code.exe required")
    require(not windows_path(value["repositoryRoot"]).is_relative_to(root), "Repository must be outside private fixture")
    return value.copy()


def code_arguments(spec):
    root = windows_path(spec["fixtureRoot"])
    repo = windows_path(spec["repositoryRoot"])
    return ["--new-window", "--wait", "--disable-extensions", "--disable-updates", "--skip-welcome",
            "--skip-release-notes", "--user-data-dir", str(root / "profile"), "--extensions-dir", str(root / "extensions"),
            "--extensionDevelopmentPath", spec["extensionDirectory"], "--extensionTestsPath",
            str(repo / "extensions/vscode/test-host.cjs"), str(root / "workspace")]


def admit_receipt(row):
    require(isinstance(row, dict) and set(row) == {"schemaVersion", "startedSuspended", "assignedBeforeResume",
            "exitCode", "activeAtRootExit", "activeBeforeCleanup", "treeExitedBeforeCleanup", "activeAfterCleanup",
            "handlesClosed", "cleanupComplete", "complete", "reason"}, "Incomplete process proof")
    require(type(row["schemaVersion"]) is int and row["schemaVersion"] == 1, "Process proof schema")
    require(all(row[key] is True for key in ("startedSuspended", "assignedBeforeResume", "treeExitedBeforeCleanup",
                                            "handlesClosed", "cleanupComplete", "complete")), "Missing lifecycle proof")
    require(type(row["activeAtRootExit"]) is int and 0 <= row["activeAtRootExit"] <= 0xffffffff, "Root exit accounting unavailable")
    require(all(type(row[key]) is int and row[key] == 0 for key in ("exitCode", "activeBeforeCleanup", "activeAfterCleanup"))
            and row["reason"] == "success", "Natural successful empty process tree required")


def ordinary(path, directory=False):
    require(path.is_absolute(), "Absolute owned path required")
    at = path
    while True:
        require(not at.is_symlink() and not at.is_junction(), "Linked owned path refused")
        if at == at.parent:
            break
        at = at.parent
    require(path.is_dir() if directory else path.is_file(), "Owned path unavailable")


def output_usage(root):
    total = count = 0
    for directory, directories, files in os.walk(root, followlinks=False):
        require(len(Path(directory).relative_to(root).parts) <= 32, "Fixture depth capacity")
        file_names = set(files)
        for name in directories + files:
            path = Path(directory) / name
            require(not path.is_symlink() and not path.is_junction(), "Linked fixture entry refused")
            count += 1
            require(count <= MAX_ENTRIES, "Fixture entry capacity")
            if name in file_names:
                total += path.stat().st_size
                require(total <= MAX_OUTPUT, "Fixture output capacity")
    return total


def write_json(path, value):
    raw = (json.dumps(value, indent=2, ensure_ascii=True) + "\n").encode()
    require(len(raw) <= 32768, "Receipt capacity")
    with path.open("xb") as stream:
        stream.write(raw)


def close_handles(handles, close):
    complete = True
    for handle in handles:
        if handle is not None:
            try:
                complete = bool(close(handle)) and complete
            except Exception:
                complete = False
    return complete


def failure_reason(phase, error):
    """Publish only fixed categories, never paths, arguments or exception text."""
    if phase not in ("launch", "job_limits", "job_assignment", "resume", "wait",
                     "disk_reserve", "output_inventory", "exit_code", "accounting"):
        return "api_error"
    category = ("missing_entry" if isinstance(error, FileNotFoundError) else
                "access_denied" if isinstance(error, PermissionError) else
                "os_error" if isinstance(error, OSError) else
                "refused" if isinstance(error, ValueError) else "exception")
    return phase + "_" + category


def run_owned(program, arguments, fixture, timeout):
    """Launch only the caller-selected tool; fail closed on incomplete cleanup."""
    import _winapi
    import msvcrt

    require(os.name == "nt" and sys.platform == "win32" and ctypes.sizeof(ctypes.c_void_p) == 8,
            "64-bit Windows lifecycle backend required")
    require(type(timeout) is int and 1 <= timeout <= 180, "Original deadline capacity")
    ordinary(fixture, True)
    ordinary(program)
    for name in ("stdout.log", "stderr.log", "job.json"):
        require(not (fixture / name).exists(), "Fresh process output required")
    require(shutil.disk_usage(fixture).free >= MIN_FREE, "Owned disk reserve unavailable")

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
    job_name = "Local\\YokeVSCode-" + os.urandom(32).hex()
    ctypes.set_last_error(0)
    job = api.CreateJobObjectW(None, job_name)
    require(bool(job), "Windows Job unavailable")
    if ctypes.get_last_error() == 183:
        api.CloseHandle(job)
        raise ValueError("Owned Job name occupied")
    process = thread = None
    row = {"schemaVersion": 1, "startedSuspended": False, "assignedBeforeResume": False,
           "exitCode": None, "activeAtRootExit": None, "activeBeforeCleanup": None,
           "treeExitedBeforeCleanup": False, "activeAfterCleanup": None,
           "handlesClosed": False, "cleanupComplete": False, "complete": False, "reason": "api_error"}

    def active():
        info = ctypes.create_string_buffer(48)
        require(api.QueryInformationJobObject(job, 1, info, 48, None), "Windows Job query failed")
        return int.from_bytes(info.raw[40:44], "little")

    phase = "job_limits"
    try:
        limits = Extended()
        limits.basic.flags = 0x2000  # KILL_ON_JOB_CLOSE; no breakaway grant.
        require(api.SetInformationJobObject(job, 9, ctypes.byref(limits), ctypes.sizeof(limits)), "Job limit failed")
        phase = "launch"
        environment = os.environ.copy()
        for key in ("ELECTRON_RUN_AS_NODE", "VSCODE_IPC_HOOK_CLI", "VSCODE_PORTABLE", "VSCODE_LOGS", "VSCODE_EXTENSIONS"):
            environment.pop(key, None)
        for key, name in (("HOME", "home"), ("USERPROFILE", "home"), ("APPDATA", "appdata"),
                          ("LOCALAPPDATA", "local"), ("TEMP", "tmp"), ("TMP", "tmp")):
            path = fixture / "environment" / name
            path.mkdir(parents=True, exist_ok=True)
            ordinary(path, True)
            environment[key] = str(path)
        environment["YOKE_OWNED_JOB"] = job_name
        deadline = time.monotonic() + timeout
        with open(os.devnull, "rb") as stdin, (fixture / "stdout.log").open("xb") as stdout, (fixture / "stderr.log").open("xb") as stderr:
            handles = [msvcrt.get_osfhandle(stream.fileno()) for stream in (stdin, stdout, stderr)]
            for handle in handles:
                os.set_handle_inheritable(handle, True)
            startup = subprocess.STARTUPINFO()
            startup.dwFlags = subprocess.STARTF_USESTDHANDLES | subprocess.STARTF_USESHOWWINDOW
            startup.wShowWindow = subprocess.SW_HIDE
            startup.hStdInput, startup.hStdOutput, startup.hStdError = handles
            startup.lpAttributeList = {"handle_list": handles}
            process, thread, _, _ = _winapi.CreateProcess(None, subprocess.list2cmdline([str(program), *arguments]),
                None, None, True, subprocess.CREATE_NO_WINDOW | 0x00000004, environment, str(fixture), startup)
            row["startedSuspended"] = True
            phase = "job_assignment"
            require(api.AssignProcessToJobObject(job, process), "Suspended Job assignment failed")
            row["assignedBeforeResume"] = True
            phase = "resume"
            require(api.ResumeThread(thread) != 0xffffffff, "Owned tool resume failed")
            while True:
                phase = "wait"
                wait = _winapi.WaitForSingleObject(process, 100)
                require(wait in (0, 258), "Owned process wait failed")
                if wait == 0:
                    break
                if time.monotonic() >= deadline:
                    row["reason"] = "deadline"
                    raise ValueError("Owned tool deadline")
                phase = "disk_reserve"
                require(shutil.disk_usage(fixture).free >= MIN_FREE, "Owned disk reserve unavailable")
                phase = "output_inventory"
                output_usage(fixture)
            if time.monotonic() >= deadline:
                row["reason"] = "deadline"
                raise ValueError("Owned tool deadline")
            phase = "exit_code"
            row["exitCode"] = _winapi.GetExitCodeProcess(process)
            phase = "accounting"
            row["activeAtRootExit"] = active()
            remaining = row["activeAtRootExit"]
            drain = min(deadline, time.monotonic() + 1)
            while remaining and time.monotonic() < drain:
                time.sleep(.01)
                remaining = active()
            row["activeBeforeCleanup"] = remaining
            row["treeExitedBeforeCleanup"] = remaining == 0
            phase = "disk_reserve"
            require(shutil.disk_usage(fixture).free >= MIN_FREE, "Owned disk reserve unavailable")
            phase = "output_inventory"
            output_usage(fixture)
            if time.monotonic() >= deadline:
                row["reason"] = "deadline"
                raise ValueError("Owned tool deadline")
            row["reason"] = "root_exit" if row["exitCode"] != 0 else "tree_alive" if remaining else "success"
    except Exception as error:
        if row["reason"] == "api_error":
            row["reason"] = failure_reason(phase, error)
    finally:
        cleanup_ok = True
        if process is not None and not row["assignedBeforeResume"]:
            try:
                _winapi.TerminateProcess(process, 124)
                cleanup_ok = _winapi.WaitForSingleObject(process, 10000) == 0
            except Exception:
                cleanup_ok = False
        try:
            cleanup_ok = bool(api.TerminateJobObject(job, 125)) and cleanup_ok
            end = time.monotonic() + 10
            while active() and time.monotonic() < end:
                time.sleep(.05)
            row["activeAfterCleanup"] = active()
            cleanup_ok = row["activeAfterCleanup"] == 0 and cleanup_ok
        except Exception:
            cleanup_ok = False
        closed = close_handles((thread, process, job), api.CloseHandle)
        row["handlesClosed"] = closed
        row["cleanupComplete"] = cleanup_ok and closed
        row["complete"] = (row["reason"] == "success" and row["assignedBeforeResume"]
                           and row["exitCode"] == 0 and row["treeExitedBeforeCleanup"]
                           and row["cleanupComplete"])
        write_json(fixture / "job.json", row)
    return row


def selected_code(spec_path):
    ordinary(spec_path)
    require(spec_path.name == "job-spec.json" and spec_path.stat().st_size <= 32768, "Fixed bounded specification required")
    with spec_path.open("rb") as stream:
        spec = admit_spec(decode(stream.read(32769)))
    fixture = Path(spec["fixtureRoot"])
    ordinary(fixture, True)
    require(spec_path.parent == fixture, "Specification outside its private fixture")
    require(not (fixture / "result.json").exists(), "Fresh extension-host result required")
    for path in (fixture / "workspace", fixture / "profile", fixture / "extensions", Path(spec["extensionDirectory"])):
        ordinary(path, True)
    ordinary(Path(spec["repositoryRoot"]) / "extensions/vscode/test-host.cjs")
    row = run_owned(Path(spec["program"]), code_arguments(spec), fixture, spec["timeoutSeconds"])
    admit_receipt(row)
    return 0


PROBE = r'''import ctypes, json, os, pathlib, subprocess, sys, time
api = ctypes.WinDLL("kernel32", use_last_error=True)
api.OpenJobObjectW.argtypes = [ctypes.c_uint32, ctypes.c_int, ctypes.c_wchar_p]
api.OpenJobObjectW.restype = ctypes.c_void_p
api.GetCurrentProcess.restype = ctypes.c_void_p
api.IsProcessInJob.argtypes = [ctypes.c_void_p, ctypes.c_void_p, ctypes.POINTER(ctypes.c_int)]
api.CloseHandle.argtypes = [ctypes.c_void_p]
handle = api.OpenJobObjectW(4, False, os.environ["YOKE_OWNED_JOB"])
assert handle
assigned = ctypes.c_int()
try:
    assert api.IsProcessInJob(api.GetCurrentProcess(), handle, ctypes.byref(assigned)) and assigned.value
finally:
    assert api.CloseHandle(handle)
scenario = sys.argv[1]
pathlib.Path("child-proof.json" if scenario == "child" else "root-proof.json").write_text(json.dumps({"specificJobMembership": True, "queryHandleClosed": True}))
if scenario == "success":
    subprocess.run([sys.executable, "-I", __file__, "child"], check=True, creationflags=0x08000000)
elif scenario == "orphan":
    subprocess.Popen([sys.executable, "-I", "-c", "import time; time.sleep(60)"], creationflags=0x08000000)
    pathlib.Path("marker.json").write_text('{"passed":true}')
elif scenario == "deadline":
    time.sleep(60)
elif scenario == "nonzero":
    sys.exit(7)
else:
    assert scenario == "child"
'''


def ci_probe():
    require(os.environ.get("GITHUB_ACTIONS") == "true" and os.environ.get("RUNNER_OS") == "Windows"
            and os.name == "nt" and sys.platform == "win32" and ctypes.sizeof(ctypes.c_void_p) == 8,
            "Explicit 64-bit hosted Windows probe required")
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True, timeout=30).strip()
    require(re.fullmatch(r"[0-9a-f]{40}", revision) is not None and revision == os.environ.get("GITHUB_SHA"), "Probe source differs")
    output = ROOT / "validation/ci-vscode-owned-job"
    require(not output.exists(), "Fresh probe output required")
    ordinary(ROOT, True)
    validation = output.parent
    require(not validation.is_symlink() and not validation.is_junction(), "Linked probe namespace refused")
    validation.mkdir(exist_ok=True)
    ordinary(validation, True)
    output.mkdir()
    ordinary(output, True)
    rows = []
    for scenario in ("success", "nonzero", "orphan", "deadline"):
        fixture = output / (scenario + " owned π")
        fixture.mkdir()
        script = fixture / "owned probe.py"
        script.write_text(PROBE, encoding="utf-8")
        row = run_owned(Path(sys.executable), ["-I", str(script), scenario], fixture, 2 if scenario == "deadline" else 30)
        require(row["assignedBeforeResume"] is True and row["handlesClosed"] is True
                and row["cleanupComplete"] is True and row["activeAfterCleanup"] == 0, "Probe cleanup proof incomplete")
        proof = json.loads((fixture / "root-proof.json").read_bytes())
        require(proof == {"specificJobMembership": True, "queryHandleClosed": True}, "Probe membership proof incomplete")
        if scenario == "success":
            admit_receipt(row)
            require(json.loads((fixture / "child-proof.json").read_bytes()) == proof, "Child membership proof incomplete")
        else:
            require(row["complete"] is False, "Negative lifecycle probe unexpectedly passed")
            require(row["reason"] == {"nonzero": "root_exit", "orphan": "tree_alive", "deadline": "deadline"}[scenario], "Negative lifecycle category differs")
            if scenario == "orphan":
                require(json.loads((fixture / "marker.json").read_bytes()) == {"passed": True}
                        and row["exitCode"] == 0 and row["activeBeforeCleanup"] > 0
                        and row["treeExitedBeforeCleanup"] is False, "Passed-marker orphan refusal unproved")
        rows.append({"scenario": scenario, "processProof": row})
    write_json(output / "result.json", {"schemaVersion": 1, "sourceRevision": revision, "complete": True,
        "scope": "Owned Python Job lifecycle probes only", "cases": rows,
        "VSCodeOrYokeExecuted": False, "VSIXOrReleaseAccepted": False})
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--spec", type=Path)
    group.add_argument("--ci-probe", action="store_true")
    args = parser.parse_args()
    return ci_probe() if args.ci_probe else selected_code(args.spec)


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Exception as error:
        print("Owned VS Code Job refused: " + type(error).__name__, file=sys.stderr)
        sys.exit(1)
