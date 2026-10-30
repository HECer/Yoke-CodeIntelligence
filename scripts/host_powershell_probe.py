"""Fixed Windows PowerShell startup observations, never native acceptance.

Only owned literal commands run. No Yoke image, token, profile or project code
is read/executed. Every child is assigned to owned Jobs before resumption.
"""
import argparse
import ctypes as c
import hashlib
import json
import os
from pathlib import Path, PureWindowsPath
import re
import shutil
import subprocess
import sys
import threading
import time

ROOT = Path(__file__).resolve().parents[1]
ENVIRONMENT = ("PATH", "SystemRoot", "SystemDrive", "WINDIR", "USERPROFILE", "HOME",
               "TEMP", "TMP", "LANG", "RUSTUP_HOME", "CARGO_HOME")
MAX_LOG = 65_536
TRIAL_SECONDS = 20
MEMBER_SECONDS = .25
CLEANUP_SECONDS = 2
READER_JOIN_SECONDS = .25
MAX_MEMBERS = 16
MAX_IMAGE_CHARS = 4096


class JobProcessIds(c.Structure):
    _fields_ = [("assigned", c.c_uint32), ("returned", c.c_uint32),
                ("ids", c.c_size_t * MAX_MEMBERS)]


class CleanupUnproven(RuntimeError):
    def __init__(self, observation):
        super().__init__("Diagnostic owned cleanup unproven")
        self.observation = observation


def selected_environment(source):
    environment = {key: source[key] for key in ENVIRONMENT if key in source}
    environment.update(CI="1", DO_NOT_TRACK="1", NO_COLOR="1")
    return environment


def commands():
    prefix = ("[Console]::Error.WriteLine('owned-script-entry'); "
              "[Console]::Error.WriteLine('owned-host-version:'+$PSVersionTable.PSVersion.ToString()); "
              "$ErrorActionPreference='Stop'; ")
    suffix = "; [Console]::Error.WriteLine('owned-script-end')"
    before = "[Console]::Error.WriteLine('owned-before-write-output'); "
    after = "; [Console]::Error.WriteLine('owned-after-write-output')"
    plain = prefix + before + "Write-Output 'owned-shell-test'" + after + suffix
    console = (prefix + "[Console]::Error.WriteLine('owned-before-console-out'); "
               "[Console]::Out.WriteLine('owned-shell-test'); "
               "[Console]::Error.WriteLine('owned-after-console-out')" + suffix)
    folder = (prefix + "[Console]::Error.WriteLine('owned-before-folder'); "
              "$folder=[Environment]::GetFolderPath([Environment+SpecialFolder]::CommonApplicationData,"
              "[Environment+SpecialFolderOption]::DoNotVerify); "
              "[Console]::Error.WriteLine('owned-after-folder'); "
              "if(-not [IO.Path]::IsPathRooted($folder)){throw 'Managed known folder is not absolute'}; "
              + before + "Write-Output 'owned-windows-runtime'" + after + suffix)
    return [("single-primary", plain, 1, "primary"),
            ("nested-primary-folder", folder, 2, "primary"),
            ("nested-snapshot", plain, 2, "snapshot"),
            ("nested-snapshot-console", console, 2, "snapshot"),
            ("nested-snapshot-folder", folder, 2, "snapshot")]


def captured_write_output_commands():
    """Separate single-trial invocation; never extend the canonical trial budget."""
    _name, plain, count, resume = commands()[2]
    original = ("[Console]::Error.WriteLine('owned-before-write-output'); "
                "Write-Output 'owned-shell-test'; "
                "[Console]::Error.WriteLine('owned-after-write-output')")
    captured = ("[Console]::Error.WriteLine('owned-before-captured-write-output'); "
                "$value = Write-Output 'owned-shell-test'; "
                "[Console]::Error.WriteLine('owned-after-captured-write-output'); "
                "[Console]::Out.WriteLine($value)")
    require(plain.count(original) == 1, "Canonical Write-Output control changed")
    return [("nested-snapshot-captured-write-output", plain.replace(original, captured, 1),
             count, resume)]


def local_app_data_commands():
    """Keep the original folder/cmdlet trial; vary one environment key only."""
    _name, script, count, resume = commands()[-1]
    return [("nested-snapshot-folder-local-app-data", script, count, resume)]


def command_boundary_commands():
    """Two fresh children isolate lookup/autoload from an explicit fixed import."""
    _name, captured, count, resume = captured_write_output_commands()[0]
    entry = "[Console]::Error.WriteLine('owned-before-captured-write-output'); "
    invocation = "$value = Write-Output 'owned-shell-test'"
    require(captured.count(entry) == 1 and captured.count(invocation) == 1,
            "Captured Write-Output control changed")

    def phase(name):
        return ("[Console]::Error.WriteLine('owned-" + name + "-ms:'"
                "+$boundaryClock.ElapsedMilliseconds.ToString([Globalization.CultureInfo]::InvariantCulture)); ")

    clock = ("[Console]::Error.WriteLine('owned-before-boundary-clock'); "
             "$boundaryClock=[Diagnostics.Stopwatch]::StartNew(); ")
    lookup = (phase("before-command-lookup")
              + "$command=$ExecutionContext.InvokeCommand.GetCommand('Write-Output',"
              "[System.Management.Automation.CommandTypes]::Cmdlet); "
              + phase("after-command-lookup")
              + "if($command -isnot [System.Management.Automation.CmdletInfo] "
              "-or $command.Name -cne 'Write-Output' "
              "-or $command.ModuleName -cne 'Microsoft.PowerShell.Utility')"
              "{throw 'Owned Write-Output command binding unavailable'}; ")
    imported = (phase("before-fixed-module-import")
                + "Import-Module -Name ($PSHOME+'\\Modules\\Microsoft.PowerShell.Utility"
                "\\Microsoft.PowerShell.Utility.psd1') -ErrorAction Stop; "
                + phase("after-fixed-module-import"))
    call = (phase("before-command-invoke") + "$value = & $command 'owned-shell-test'; "
            + phase("after-command-invoke"))
    return [("nested-snapshot-" + name,
             captured.replace(entry, clock + preparation + lookup + entry, 1)
                     .replace(invocation, call.rstrip("; "), 1), count, resume)
            for name, preparation in (("command-lookup", ""), ("fixed-module-import", imported))]


def fixed_import_original_commands():
    """Retain ordinary string-name invocation after one fixed host-module import."""
    name, script, count, resume = commands()[-1]
    require((name, count, resume) == ("nested-snapshot-folder", 2, "snapshot"),
            "Canonical folder control changed")
    prelude = (
        "[Console]::Error.WriteLine('owned-before-original-import-clock'); "
        "$originalImportClock=[Diagnostics.Stopwatch]::StartNew(); "
        "[Console]::Error.WriteLine('owned-before-original-module-import-ms:'"
        "+$originalImportClock.ElapsedMilliseconds.ToString([Globalization.CultureInfo]::InvariantCulture)); "
        "Import-Module -Name ($PSHOME+'\\Modules\\Microsoft.PowerShell.Utility"
        "\\Microsoft.PowerShell.Utility.psd1') -ErrorAction Stop; "
        "[Console]::Error.WriteLine('owned-after-original-module-import-ms:'"
        "+$originalImportClock.ElapsedMilliseconds.ToString([Globalization.CultureInfo]::InvariantCulture)); "
    )
    return [("nested-snapshot-fixed-import-original", prelude + script, count, resume)]


def local_app_data_environment(source):
    value = source.get("LOCALAPPDATA")
    require(isinstance(value, str) and 0 < len(value) <= 32767
            and not any(ord(char) < 32 for char in value),
            "Host LocalAppData is unavailable")
    path = PureWindowsPath(value)
    require(path.is_absolute() and re.fullmatch(r"[A-Za-z]:", path.drive)
            and not any(part in (".", "..") for part in re.split(r"[\\/]", value)),
            "Host LocalAppData must be an absolute local directory")
    require(Path(value).is_dir(), "Host LocalAppData directory is unavailable")
    return {**selected_environment(source), "LOCALAPPDATA": value}


def windows_api():
    from ctypes import wintypes as w

    class Basic(c.Structure):
        _fields_ = [("process", c.c_int64), ("job", c.c_int64), ("flags", w.DWORD),
                    ("minimum", c.c_size_t), ("maximum", c.c_size_t), ("active", w.DWORD),
                    ("affinity", c.c_size_t), ("priority", w.DWORD), ("scheduling", w.DWORD)]

    class Extended(c.Structure):
        _fields_ = [("basic", Basic), ("io", c.c_uint64 * 6), ("process", c.c_size_t),
                    ("job", c.c_size_t), ("peak_process", c.c_size_t), ("peak_job", c.c_size_t)]

    class Thread(c.Structure):
        _fields_ = [("size", w.DWORD), ("usage", w.DWORD), ("id", w.DWORD),
                    ("owner", w.DWORD), ("base", w.LONG), ("delta", w.LONG), ("flags", w.DWORD)]

    class Association(c.Structure):
        _fields_ = [("key", c.c_void_p), ("port", c.c_void_p)]

    if c.sizeof(Extended) != 144 or c.sizeof(Thread) != 28:
        raise RuntimeError("Unsupported Windows diagnostic layout")
    api = c.WinDLL("kernel32", use_last_error=True)
    for name, args, result in [
        ("CreateJobObjectW", [c.c_void_p, c.c_wchar_p], c.c_void_p),
        ("SetInformationJobObject", [c.c_void_p, c.c_int, c.c_void_p, w.DWORD], w.BOOL),
        ("QueryInformationJobObject", [c.c_void_p, c.c_int, c.c_void_p, w.DWORD, c.c_void_p], w.BOOL),
        ("AssignProcessToJobObject", [c.c_void_p, c.c_void_p], w.BOOL),
        ("TerminateJobObject", [c.c_void_p, w.UINT], w.BOOL),
        ("ResumeThread", [c.c_void_p], w.DWORD),
        ("CloseHandle", [c.c_void_p], w.BOOL),
        ("CreateIoCompletionPort", [c.c_void_p, c.c_void_p, c.c_size_t, w.DWORD], c.c_void_p),
        ("CreateToolhelp32Snapshot", [w.DWORD, w.DWORD], c.c_void_p),
        ("Thread32First", [c.c_void_p, c.POINTER(Thread)], w.BOOL),
        ("Thread32Next", [c.c_void_p, c.POINTER(Thread)], w.BOOL),
        ("OpenThread", [w.DWORD, w.BOOL, w.DWORD], c.c_void_p),
        ("OpenProcess", [w.DWORD, w.BOOL, w.DWORD], c.c_void_p),
        ("IsProcessInJob", [c.c_void_p, c.c_void_p, c.c_void_p], w.BOOL),
        ("GetProcessTimes", [c.c_void_p] * 5, w.BOOL),
        ("QueryFullProcessImageNameW", [c.c_void_p, w.DWORD, c.c_wchar_p, c.c_void_p], w.BOOL),
        ("WaitForSingleObject", [c.c_void_p, w.DWORD], w.DWORD),
        ("GetExitCodeProcess", [c.c_void_p, c.c_void_p], w.BOOL),
    ]:
        function = getattr(api, name)
        function.argtypes, function.restype = args, result
    return api, Extended, Thread, Association


def require(value, label):
    if not value:
        raise RuntimeError(label)


def observe_members(api, jobs, root, root_pid, handles, end):
    """One bounded outer-Job snapshot, not a complete process history.

    Metadata comes only from membership-checked handles, retained until cleanup.
    Image names are OS metadata; no executable bytes or command lines are read.
    """
    result = {"complete": False, "members": None, "unavailable": []}

    def budget():
        if time.monotonic() >= end:
            if "deadline" not in result["unavailable"]:
                result["unavailable"].append("deadline")
            return False
        return True

    if not budget():
        return result
    data = JobProcessIds()
    # A nested Job's outer list already includes its descendants; never sum Jobs.
    if not api.QueryInformationJobObject(jobs[0], 3, c.byref(data), c.sizeof(data), None):
        result["unavailable"].append("job-process-list")
        return result
    result.update(assignedAtQuery=int(data.assigned), returnedAtQuery=int(data.returned))
    if data.returned > MAX_MEMBERS or data.returned > data.assigned:
        result["unavailable"].append("invalid-job-process-list")
        return result
    pids = list(data.ids[:data.returned])
    if any(pid <= 0 or pid > 0xffffffff for pid in pids) or len(set(pids)) != len(pids):
        result["unavailable"].append("invalid-job-process-list")
        return result
    if data.assigned != data.returned:
        result["unavailable"].append("job-process-list-capacity")
    result["members"] = [
        {"pid": pid, "root": pid == root_pid, "inOuterOwnedJob": None,
         "creationTime100ns": None, "imagePath": None, "exited": None,
         "exitCode": None, "unavailable": []}
        for pid in pids
    ]
    for member in result["members"]:
        if not budget():
            member["unavailable"].append("deadline")
            continue
        handle = root if member["root"] else api.OpenProcess(0x00101000, False, member["pid"])
        if not handle:
            member["unavailable"].append("process-open")
            continue
        if not member["root"]:
            # Track even an unverified handle immediately, so errors cannot leak it.
            handles.append(handle)
        if not budget():
            member["unavailable"].append("deadline")
            continue
        owned = c.c_int32()
        if not api.IsProcessInJob(handle, jobs[0], c.byref(owned)):
            member["unavailable"].append("membership-query")
            continue
        member["inOuterOwnedJob"] = bool(owned.value)
        if not owned.value:
            member["unavailable"].append("not-in-owned-job")
            continue

        created, exited, kernel, user = (c.c_uint64() for _ in range(4))
        if budget() and api.GetProcessTimes(handle, c.byref(created), c.byref(exited),
                                           c.byref(kernel), c.byref(user)) and created.value:
            member["creationTime100ns"] = created.value
        else:
            member["unavailable"].append("creation-time")
        image = c.create_unicode_buffer(MAX_IMAGE_CHARS)
        length = c.c_uint32(MAX_IMAGE_CHARS)
        if (budget() and api.QueryFullProcessImageNameW(handle, 0, image, c.byref(length))
                and 0 < length.value < MAX_IMAGE_CHARS):
            member["imagePath"] = image[:length.value]
        else:
            member["unavailable"].append("image-name")
        wait = api.WaitForSingleObject(handle, 0) if budget() else None
        if wait == 0:
            member["exited"] = True
            code = c.c_uint32()
            if budget() and api.GetExitCodeProcess(handle, c.byref(code)):
                member["exitCode"] = code.value
            else:
                member["unavailable"].append("exit-code")
        elif wait == 258:
            member["exited"] = False
        else:
            member["unavailable"].append("exit-state")
    within_budget = budget()
    result["complete"] = (within_budget and not result["unavailable"]
                          and all(not member["unavailable"] for member in result["members"]))
    return result


def trial(shell, fixture, environment, definition):
    require(shutil.disk_usage(ROOT).free >= 500_000_000, "Diagnostic disk reserve unavailable")
    import _winapi
    import msvcrt

    name, command, count, resume = definition
    api, Extended, Thread, Association = windows_api()
    handles, jobs, pipes, readers = [], [], [], []
    process = thread = None
    logs = [bytearray(), bytearray()]
    reader_done = [False, False]
    reader_error = [False, False]
    overflow = [False, False]
    row = {"name": name, "jobsRequested": count, "resumeMode": resume,
           "startedSuspended": False, "assignedBeforeResume": False,
           "timedOut": False, "nativeAcceptance": False,
           "membersBeforeCleanup": {"complete": False, "members": None,
                                    "unavailable": ["not-assigned"]}}
    started = time.monotonic()
    end = started + TRIAL_SECONDS
    cleanup_errors = []
    members_sampled = False

    def active(job):
        data = c.create_string_buffer(48)
        require(api.QueryInformationJobObject(job, 1, data, 48, None), "Job accounting unavailable")
        return int.from_bytes(data.raw[40:44], "little")

    def read_stream(index, fd):
        try:
            while True:
                data = os.read(fd, 8192)
                if not data:
                    reader_done[index] = True
                    return
                remaining = MAX_LOG - len(logs[index])
                logs[index].extend(data[:remaining])
                overflow[index] |= len(data) > remaining
        except OSError:
            reader_error[index] = True
        finally:
            os.close(fd)

    def sample_members():
        nonlocal members_sampled
        members_sampled = True
        sample_started = time.monotonic()
        try:
            observation = observe_members(
                api, jobs, process, pid, handles, min(end, sample_started + MEMBER_SECONDS))
            observation.update(startedElapsedSeconds=sample_started - started,
                               finishedElapsedSeconds=time.monotonic() - started)
        except Exception as error:
            # Optional observation must never skip mandatory owned cleanup.
            observation = {"complete": False, "members": None,
                           "unavailable": ["observation-error"], "errorType": type(error).__name__}
        row["membersBeforeCleanup"] = observation

    try:
        for index in range(count):
            job = api.CreateJobObjectW(None, None)
            require(job, "Owned diagnostic Job unavailable")
            handles.append(job)
            jobs.append(job)
            limits = Extended()
            limits.basic.flags = 0x2000 if index == 0 else 0
            require(api.SetInformationJobObject(job, 9, c.byref(limits), c.sizeof(limits)), "Job limits unavailable")
            if index:
                port = api.CreateIoCompletionPort(c.c_void_p(-1), None, 0, 1)
                require(port, "Owned completion port unavailable")
                handles.append(port)
                association = Association(job, port)
                require(api.SetInformationJobObject(job, 7, c.byref(association), c.sizeof(association)), "Completion association failed")
        stdin_read, stdin_write = os.pipe()
        stdout_read, stdout_write = os.pipe()
        stderr_read, stderr_write = os.pipe()
        pipes.extend([stdin_read, stdin_write, stdout_read, stdout_write, stderr_read, stderr_write])
        child_handles = [msvcrt.get_osfhandle(fd) for fd in (stdin_read, stdout_write, stderr_write)]
        for handle in child_handles:
            os.set_handle_inheritable(handle, True)
        startup = subprocess.STARTUPINFO()
        startup.dwFlags = subprocess.STARTF_USESTDHANDLES
        startup.hStdInput, startup.hStdOutput, startup.hStdError = child_handles
        startup.lpAttributeList = {"handle_list": child_handles}
        argv = [str(shell), "-NoProfile", "-NonInteractive", "-Command", command]
        process, thread, pid, tid = _winapi.CreateProcess(None, subprocess.list2cmdline(argv), None, None, True,
            subprocess.CREATE_NO_WINDOW | 4, environment, str(fixture), startup)
        row["startedSuspended"] = True
        row["pid"] = pid
        for job in jobs:
            require(api.AssignProcessToJobObject(job, process), "Suspended Job assignment failed")
        row["assignedBeforeResume"] = True
        for fd in (stdin_read, stdout_write, stderr_write, stdin_write):
            os.close(fd)
            pipes.remove(fd)
        for index, fd in enumerate((stdout_read, stderr_read)):
            reader = threading.Thread(target=read_stream, args=(index, fd), daemon=True)
            readers.append(reader)
            pipes.remove(fd)
            reader.start()
        resumed = []
        if resume == "primary":
            previous = api.ResumeThread(thread)
            require(previous != 0xffffffff, "Primary thread resume failed")
            resumed.append({"primary": True, "previousSuspendCount": previous})
        else:
            snapshot = api.CreateToolhelp32Snapshot(4, 0)
            require(snapshot not in (None, c.c_void_p(-1).value), "Thread snapshot unavailable")
            try:
                entry = Thread()
                entry.size = c.sizeof(entry)
                available = api.Thread32First(snapshot, c.byref(entry))
                scanned = 0
                while available:
                    scanned += 1
                    require(scanned <= 131072 and time.monotonic() < end, "Thread snapshot capacity/deadline")
                    if entry.owner == pid:
                        handle = api.OpenThread(2, False, entry.id)
                        require(handle, "Owned thread open failed")
                        try:
                            previous = api.ResumeThread(handle)
                            require(previous != 0xffffffff, "Owned thread resume failed")
                            require(len(resumed) < 128, "Owned thread observation capacity")
                            resumed.append({"primary": entry.id == tid, "previousSuspendCount": previous})
                        finally:
                            if not api.CloseHandle(handle):
                                cleanup_errors.append("thread-handle-close")
                                raise RuntimeError("Owned thread close failed")
                    available = api.Thread32Next(snapshot, c.byref(entry))
                row["snapshotEntriesScanned"] = scanned
                row["snapshotEndError"] = c.get_last_error()
            finally:
                if not api.CloseHandle(snapshot):
                    cleanup_errors.append("snapshot-handle-close")
                    raise RuntimeError("Snapshot close failed")
        row["resumedThreads"] = resumed
        row["primaryObservedResumed"] = any(item["primary"] and item["previousSuspendCount"] > 0 for item in resumed)
        row["resumeElapsedSeconds"] = time.monotonic() - started
        while _winapi.WaitForSingleObject(process, 10) == 258:
            now = time.monotonic()
            if now >= end:
                row["timedOut"] = True
                break
            if not members_sampled and now >= end - MEMBER_SECONDS:
                # Sample within the original deadline; the child keeps running.
                sample_members()
        if not row["timedOut"]:
            row["exitCode"] = _winapi.GetExitCodeProcess(process)
            row["timedOut"] = time.monotonic() >= end
        remaining = [active(job) for job in jobs]
        drain_end = min(end, time.monotonic() + .5)
        while any(remaining) and time.monotonic() < drain_end:
            time.sleep(.01)
            remaining = [active(job) for job in jobs]
        row["activeBeforeCleanup"] = remaining
        row["treeExitedBeforeCleanup"] = all(value == 0 for value in remaining)
    except Exception as error:
        row["errorType"] = type(error).__name__
        row["error"] = str(error)
    finally:
        if row["assignedBeforeResume"] and not members_sampled:
            sample_members()
        if process is not None and not row["assignedBeforeResume"]:
            try:
                _winapi.TerminateProcess(process, 124)
            except OSError:
                cleanup_errors.append("root-termination")
        for job in jobs:
            if not api.TerminateJobObject(job, 125):
                cleanup_errors.append("job-termination")
        cleanup_end = time.monotonic() + CLEANUP_SECONDS
        try:
            while jobs and any(active(job) for job in jobs) and time.monotonic() < cleanup_end:
                time.sleep(.01)
            row["activeAfterCleanup"] = [active(job) for job in jobs]
            if any(value != 0 for value in row["activeAfterCleanup"]):
                cleanup_errors.append("job-still-active")
        except RuntimeError:
            row["activeAfterCleanup"] = None
            cleanup_errors.append("job-accounting")
        row["rootExitedAfterCleanup"] = process is None
        if process is not None:
            try:
                milliseconds = max(0, min(CLEANUP_SECONDS * 1000,
                                          int((cleanup_end - time.monotonic()) * 1000)))
                row["rootExitedAfterCleanup"] = _winapi.WaitForSingleObject(process, milliseconds) == 0
                if not row["rootExitedAfterCleanup"]:
                    cleanup_errors.append("root-exit")
            except OSError:
                cleanup_errors.append("root-wait")
        for reader in readers:
            reader.join(timeout=READER_JOIN_SECONDS)
        for fd in pipes:
            try:
                os.close(fd)
            except OSError:
                cleanup_errors.append("pipe-close")
        for handle in [thread, process, *reversed(handles)]:
            if handle is not None:
                if not api.CloseHandle(handle):
                    cleanup_errors.append("handle-close")
    row.update(elapsedSeconds=time.monotonic() - started,
               stdout=bytes(logs[0]).decode("utf8", "replace"), stderr=bytes(logs[1]).decode("utf8", "replace"),
               streamEOF=reader_done, readerError=reader_error, outputExceeded=overflow)
    if cleanup_errors:
        row.update(cleanupUnproven=True, cleanupErrors=cleanup_errors, diagnosticComplete=False)
        raise CleanupUnproven(row)
    row["diagnosticComplete"] = (not row.get("error") and not row["timedOut"]
        and row.get("exitCode") == 0 and row.get("primaryObservedResumed") is True
        and row.get("treeExitedBeforeCleanup") is True
        and row.get("rootExitedAfterCleanup") is True
        and row["membersBeforeCleanup"]["complete"] is True
        and all(reader_done) and not any(reader_error) and not any(overflow))
    return row


def main(arguments=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--captured-write-output", action="store_true",
                      help="Run only the separate captured Write-Output diagnostic control")
    mode.add_argument("--local-app-data", action="store_true",
                      help="Run only the original folder trial with host LOCALAPPDATA")
    mode.add_argument("--command-boundary", action="store_true",
                      help="Run separate lookup and fixed module-import boundary controls")
    mode.add_argument("--fixed-import-original", action="store_true",
                      help="Run the exact original folder command after fixed Utility import")
    args = parser.parse_args(arguments)
    definitions = (captured_write_output_commands() if args.captured_write_output else
                   local_app_data_commands() if args.local_app_data else
                   command_boundary_commands() if args.command_boundary else
                   fixed_import_original_commands() if args.fixed_import_original else commands())
    require(os.name == "nt", "PowerShell diagnostic requires Windows")
    revision = os.environ.get("GITHUB_SHA")
    require(revision is None or re.fullmatch(r"[0-9a-f]{40}", revision), "Diagnostic source revision invalid")
    require(shutil.disk_usage(ROOT).free >= 500_000_000, "Diagnostic disk reserve unavailable")
    # Validate the supplemental host value before creating an evidence directory.
    environment = (local_app_data_environment(os.environ) if args.local_app_data else
                   selected_environment(os.environ))
    output = args.output.resolve()
    output.mkdir(exist_ok=False, parents=True)
    fixture = output / "fixture"
    fixture.mkdir()
    shell = Path(os.environ["SystemRoot"]) / "System32/WindowsPowerShell/v1.0/powershell.exe"
    require(shell.is_absolute() and shell.is_file(), "Selected system PowerShell unavailable")
    report = {"schema": 2, "diagnosticOnly": True, "nativeAcceptance": False,
              "diagnosticComplete": False,
              "diagnosticMode": ("captured-write-output" if args.captured_write_output else
                                 "local-app-data" if args.local_app_data else
                                 "command-boundary" if args.command_boundary else
                                 "fixed-import-original" if args.fixed_import_original else "canonical"),
              "nativeYokeImageReadOrExecuted": False, "trialDeadlineSeconds": TRIAL_SECONDS,
              "memberObservationSeconds": MEMBER_SECONDS, "maxObservedMembers": MAX_MEMBERS,
              "sourceRevision": revision,
              "sourceSha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              "environmentNames": sorted(environment), "trials": []}
    (output / "result.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf8")
    for definition in definitions:
        try:
            row = trial(shell, fixture, environment, definition)
        except CleanupUnproven as error:
            report["trials"].append(error.observation)
            (output / "result.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf8")
            raise
        report["trials"].append(row)
        (output / "result.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf8")
        print(json.dumps({key: row.get(key) for key in
              ("name", "diagnosticComplete", "timedOut", "elapsedSeconds", "primaryObservedResumed", "activeAfterCleanup")}))
    report["diagnosticComplete"] = all(row["diagnosticComplete"] for row in report["trials"])
    (output / "result.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf8")
    return 0


if __name__ == "__main__":
    sys.exit(main())
