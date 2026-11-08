"""Bounded own-package fingerprint plans and explicit retained quarantine.

Default scope is all own-package fingerprints for a Cargo source-context switch.
Dependency-path metadata is diagnostic evidence, never binary provenance proof.
"""
from pathlib import Path, PureWindowsPath
from datetime import datetime, timezone
import argparse
import hashlib
import json
import ntpath
import os
import re
import stat
import struct
import subprocess
import sys

PACKAGE = "yoke-code-intelligence"
OWN_NAME = re.compile(r"yoke-code-intelligence-[0-9a-f]{16}\Z")
LEAF = re.compile(r"[A-Za-z0-9_.-]{1,200}\Z")
TAG = re.compile(r"[a-z0-9][a-z0-9-]{0,79}\Z")
MAX_DIRS = 4096
MAX_LEAVES = 16
MAX_FILE = 8_000_000
MAX_TOTAL = 128_000_000
MAX_REPORT = 64_000_000
MAX_ENUMERATED = 50_000
MAX_DEP_FILES = 32768
MAX_PATH = 8192
MAGIC = b"\x01\x00\x00\x00\xff\x01"


class Refusal(RuntimeError):
    pass


def require(condition, message):
    if not condition:
        raise Refusal(message)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def encoded(value):
    data = (json.dumps(value, indent=2, sort_keys=True) + "\n").encode()
    require(len(data) <= MAX_REPORT, "report byte bound exceeded")
    return data


def bounded_entries(path, maximum):
    result = []
    for entry in path.iterdir():
        require(len(result) < maximum, "directory entry bound exceeded")
        result.append(entry)
    return result


def no_reparse(path, *, exists=True):
    path = Path(os.path.abspath(path))
    cursor = path
    while True:
        try:
            info = cursor.lstat()
        except FileNotFoundError:
            require(not exists or cursor != path, "required path is absent")
        else:
            require(not stat.S_ISLNK(info.st_mode), "symbolic link refused")
            require(not getattr(info, "st_file_attributes", 0) & stat.FILE_ATTRIBUTE_REPARSE_POINT, "reparse point refused")
        if cursor.parent == cursor:
            break
        cursor = cursor.parent
    return path


def under(path, root):
    path = no_reparse(path, exists=False)
    require(path.is_relative_to(root), "path escapes required root")
    return path


def identity(info):
    return {"device": info.st_dev, "inode": info.st_ino, "size": info.st_size, "mtimeNs": info.st_mtime_ns}


def read_plain(path, maximum=MAX_FILE):
    path = no_reparse(path)
    before = path.stat()
    require(stat.S_ISREG(before.st_mode) and before.st_size <= maximum, "file kind or size refused")
    with path.open("rb") as stream:
        data = stream.read(maximum + 1)
    after = path.stat()
    require(identity(before) == identity(after) and len(data) == before.st_size, "file changed while reading")
    return data, {"sha256": sha(data), "bytes": len(data), "identity": identity(after)}


def no_compiler_processes():
    """Complete bounded Windows process snapshot; point-in-time guard only."""
    require(os.name == "nt", "this process guard is Windows-only")
    import ctypes
    from ctypes import wintypes
    import time

    class PROCESSENTRY32W(ctypes.Structure):
        _fields_ = [("dwSize", wintypes.DWORD), ("cntUsage", wintypes.DWORD), ("th32ProcessID", wintypes.DWORD), ("th32DefaultHeapID", ctypes.c_size_t), ("th32ModuleID", wintypes.DWORD), ("cntThreads", wintypes.DWORD), ("th32ParentProcessID", wintypes.DWORD), ("pcPriClassBase", wintypes.LONG), ("dwFlags", wintypes.DWORD), ("szExeFile", wintypes.WCHAR * 260)]

    pointer_bytes = ctypes.sizeof(ctypes.c_void_p)
    require(pointer_bytes in (4, 8) and ctypes.sizeof(wintypes.DWORD) == 4 and ctypes.sizeof(wintypes.LONG) == 4 and ctypes.sizeof(wintypes.WCHAR) == 2, "unsupported Windows process structure ABI")
    require(ctypes.sizeof(PROCESSENTRY32W) == (568 if pointer_bytes == 8 else 556) and PROCESSENTRY32W.szExeFile.offset == (44 if pointer_bytes == 8 else 36), "Windows process structure layout differs")
    try:
        kernel = ctypes.WinDLL("kernel32", use_last_error=True)
        kernel.CreateToolhelp32Snapshot.argtypes = [wintypes.DWORD, wintypes.DWORD]
        kernel.CreateToolhelp32Snapshot.restype = wintypes.HANDLE
        kernel.Process32FirstW.argtypes = [wintypes.HANDLE, ctypes.POINTER(PROCESSENTRY32W)]
        kernel.Process32FirstW.restype = wintypes.BOOL
        kernel.Process32NextW.argtypes = [wintypes.HANDLE, ctypes.POINTER(PROCESSENTRY32W)]
        kernel.Process32NextW.restype = wintypes.BOOL
        kernel.CloseHandle.argtypes = [wintypes.HANDLE]
        kernel.CloseHandle.restype = wintypes.BOOL
    except (OSError, AttributeError) as error:
        raise Refusal("native process inspection unavailable") from error
    ctypes.set_last_error(0)
    snapshot = kernel.CreateToolhelp32Snapshot(0x00000002, 0)
    require(snapshot not in (None, 0, ctypes.c_void_p(-1).value), "native process snapshot creation failed")
    blocked_names = {"cargo.exe", "cargo-clippy.exe", "rustc.exe", "clippy-driver.exe", "rustdoc.exe", "link.exe", "rust-lld.exe", "lld-link.exe"}
    blocked = False
    count = 0
    deadline = time.monotonic() + 2.0
    try:
        entry = PROCESSENTRY32W()
        entry.dwSize = ctypes.sizeof(entry)
        ctypes.set_last_error(0)
        present = kernel.Process32FirstW(snapshot, ctypes.byref(entry))
        while present:
            count += 1
            require(count <= 65536 and time.monotonic() <= deadline, "native process enumeration bound exceeded")
            require(entry.dwSize == ctypes.sizeof(entry), "native process entry size differs")
            # Capacity is measured in UTF-16 WCHAR units, not Python code points.
            name_units = (ctypes.c_uint16 * 260).from_address(ctypes.addressof(entry) + PROCESSENTRY32W.szExeFile.offset)
            terminator = next((index for index in range(260) if name_units[index] == 0), None)
            require(terminator is not None and terminator > 0, "native process entry name is incomplete")
            name = entry.szExeFile
            blocked = blocked or name.casefold() in blocked_names
            ctypes.set_last_error(0)
            present = kernel.Process32NextW(snapshot, ctypes.byref(entry))
        require(ctypes.get_last_error() == 18, "native process enumeration failed")
        require(time.monotonic() <= deadline, "native process enumeration time bound exceeded")
    finally:
        require(bool(kernel.CloseHandle(snapshot)), "native process snapshot close failed")
    require(not blocked, "Cargo/compiler/linker process is active")


def decode_dep_v1(data):
    """Strict Cargo 1.94 EncodedDepInfo v1 reader; never return env keys/values."""
    require(len(data) <= MAX_FILE and data.startswith(MAGIC), "unsupported dep-info marker/version")
    cursor = 6

    def take(count):
        nonlocal cursor
        require(0 <= count <= MAX_FILE and cursor + count <= len(data), "truncated dep-info")
        output = data[cursor:cursor + count]
        cursor += count
        return output

    def number():
        return struct.unpack("<I", take(4))[0]

    def field(maximum):
        count = number()
        require(count <= maximum, "dep-info field exceeds bound")
        return take(count)

    count = number()
    require(count <= MAX_DEP_FILES, "too many dep-info files")
    paths = []
    for _ in range(count):
        kind = take(1)[0]
        require(kind in (0, 1), "unknown dep-info path type")
        try:
            path = field(MAX_PATH).decode("utf-8")
        except UnicodeDecodeError as error:
            raise Refusal("non-UTF8 dependency path is unknown") from error
        require(path and "\0" not in path, "empty or NUL dependency path")
        has_checksum = take(1)[0]
        require(has_checksum in (0, 1), "unknown checksum flag")
        if has_checksum:
            take(8)
            field(256)  # Existing checksum bytes are consumed, not treated as provenance.
        paths.append((kind, path))
    env_count = number()
    require(env_count <= MAX_DEP_FILES, "too many environment entries")
    for _ in range(env_count):
        field(MAX_PATH)  # Never publish environment names or values.
        present = take(1)[0]
        require(present in (0, 1), "unknown environment presence flag")
        if present:
            field(MAX_FILE)
    require(cursor == len(data), "trailing dep-info bytes are unknown")
    return paths, env_count


def lexical_inside(path, root):
    path = ntpath.normcase(ntpath.normpath(path))
    root = ntpath.normcase(ntpath.normpath(str(root)))
    return path == root or path.startswith(root.rstrip("\\/") + "\\")


def origin_diagnostic(data, current, target):
    try:
        paths, env_count = decode_dep_v1(data)
    except Refusal as error:
        return {"status": "unknown_format_or_unreadable", "reason": str(error), "artifactProvenanceVerified": False}
    counts = {"packageRelativeOriginalRootUnknown": 0, "declaredCurrentRootReference": 0, "foreignOrExternalBuildRootReference": 0}
    samples = []
    for kind, path in paths:
        base = current / "native" if kind == 0 else target
        absolute = ntpath.normpath(path if PureWindowsPath(path).is_absolute() else ntpath.join(str(base), path))
        # Normal workspace contains target/images too; its exact native package
        # boundary avoids mistaking a nested foreign image for normal source.
        category = "packageRelativeOriginalRootUnknown" if kind == 0 else "declaredCurrentRootReference" if lexical_inside(absolute, current / "native") else "foreignOrExternalBuildRootReference"
        counts[category] += 1
        if len(samples) < 12:
            samples.append({"pathType": kind, "encodedPath": path, "lexicalAbsolutePath": absolute, "category": category})
    return {"status": "decoded_v1_recorded_paths_only", "pathCounts": counts, "pathSamples": samples, "environmentEntriesConsumedWithoutDisclosure": env_count, "referencedPathsOpened": False, "artifactProvenanceVerified": False}


def context(workspace, target, current, expected_lib):
    workspace = no_reparse(workspace)
    target = no_reparse(target)
    current = no_reparse(current)
    require(target == workspace / "target", "target must be the workspace's literal target directory")
    require(current == workspace or current.is_relative_to(target), "current root must be normal workspace or a contained target image")
    require(re.fullmatch(r"[0-9a-f]{64}", expected_lib) is not None, "explicit current lib SHA256 required")
    lib, lib_pin = read_plain(current / "native/src/lib.rs")
    require(sha(lib) == expected_lib, "current root library pin differs")
    source_pins = {"native/src/lib.rs": lib_pin}
    for name in ("Cargo.toml", "Cargo.lock", "native/Cargo.toml"):
        _, source_pins[name] = read_plain(current / name)
    return workspace, target, current, source_pins


def inventory(target, current, profiles, only):
    require(1 <= len(profiles) <= 2 and len(set(profiles)) == len(profiles), "duplicate or absent profile")
    require(all(profile in ("debug", "release") for profile in profiles), "unsupported fingerprint profile")
    require(not only or len(profiles) == 1, "explicit directory subset requires one profile")
    require(len(only) == len(set(only)) and all(OWN_NAME.fullmatch(name) for name in only), "explicit subset names invalid")
    output = []
    total = 0
    for profile in profiles:
        parent = under(target / profile / ".fingerprint", target)
        if not parent.exists():
            require(not only, "explicit fingerprint parent is absent")
            continue
        no_reparse(parent)
        if only:
            paths = [parent / name for name in sorted(only)]
        else:
            paths = []
            for path in bounded_entries(parent, MAX_ENUMERATED):
                if OWN_NAME.fullmatch(path.name):
                    paths.append(path)
            paths.sort()
        require(len(output) + len(paths) <= MAX_DIRS, "fingerprint directory bound exceeded")
        for path in paths:
            no_reparse(path)
            require(path.is_dir(), "fingerprint is not a plain directory")
            leaves = sorted(bounded_entries(path, MAX_LEAVES))
            files = {}
            diagnostics = {}
            for leaf in leaves:
                require(LEAF.fullmatch(leaf.name) is not None, "unsafe fingerprint leaf name")
                data, pinned = read_plain(leaf)
                total += len(data)
                require(total <= MAX_TOTAL, "total fingerprint byte bound exceeded")
                files[leaf.name] = pinned
                if leaf.name.startswith("dep-"):
                    diagnostics[leaf.name] = origin_diagnostic(data, current, target)
            output.append({"profile": profile, "name": path.name, "relative": path.relative_to(target).as_posix(), "directoryIdentity": identity(path.stat()), "files": files, "diagnostic": diagnostics})
    return output, total


def plan_core(args):
    no_compiler_processes()
    workspace, target, current, source_pins = context(Path(args.workspace_root), Path(args.target_root), Path(args.current_root), args.current_lib_sha256)
    profiles = sorted(args.profile or ["debug"])
    selected, total = inventory(target, current, profiles, args.only_directory or [])
    no_compiler_processes()
    require(TAG.fullmatch(args.quarantine_tag) is not None, "invalid quarantine tag")
    destination = under(target / "cache-origin-quarantines" / args.quarantine_tag, target)
    require(not destination.exists(), "quarantine destination must be fresh")
    return {"schema": "yoke_own_package_fingerprint_plan_1", "workspaceRoot": str(workspace), "targetRoot": str(target), "currentRoot": str(current), "currentLibSha256": args.current_lib_sha256, "currentSourcePins": source_pins, "profiles": profiles, "selection": "explicit_subset_not_whole_cache_admission" if args.only_directory else "all_own_package_fingerprints_for_context_switch", "explicitNames": sorted(args.only_directory or []), "quarantineRoot": str(destination), "fingerprintDirectories": selected, "directoryCount": len(selected), "totalBytes": total, "artifactProvenanceVerified": False, "cacheMutationPerformed": False, "limits": {"directories": MAX_DIRS, "fingerprintParentEntries": MAX_ENUMERATED, "leavesPerDirectory": MAX_LEAVES, "fileBytes": MAX_FILE, "totalBytes": MAX_TOTAL, "reportBytes": MAX_REPORT}, "notes": ["Recorded dep paths do not authenticate rlib/rmeta/executable contents", "Package-relative metadata does not retain its original source root", "Only own-package fingerprint directories are selected; dependency and compiled-artifact files are untouched", "Root must coordinate exclusive Cargo ownership; point-in-time process checks cannot prevent a later external Cargo start"]}


def write_fresh(path, data):
    no_reparse(path, exists=False)
    path.parent.mkdir(parents=True, exist_ok=True)
    no_reparse(path.parent)
    with path.open("xb") as stream:
        stream.write(data)
        stream.flush()
        os.fsync(stream.fileno())


def observe_failure_path(path):
    """Point-in-time metadata only; never follow a reparse point or read bytes."""
    result = {"path": path, "observationIsPointInTime": True}
    try:
        checked = no_reparse(Path(path), exists=False)
        info = checked.lstat()
        result.update({"status": "plain_directory" if stat.S_ISDIR(info.st_mode) else "unexpected_plain_kind", "identity": identity(info)})
    except FileNotFoundError:
        result["status"] = "absent"
    except (OSError, Refusal) as error:
        result.update({"status": "unknown_or_refused", "errorType": type(error).__name__})
    return result


def failure_record(moved, in_flight, error, observer=observe_failure_path):
    """Separate the recorded verified prefix from the current unfinished move."""
    observations = None
    if in_flight is not None:
        observations = {key: observer(in_flight[key]) for key in ("source", "retained")}
    return {"status": "partial_quarantine_retained_do_not_reuse_destination", "completedMoves": moved, "completedMovesMeaning": "file verification and individual move-record write/fsync returned; no directory durability guarantee", "currentInFlightMove": in_flight, "inFlightPathObservations": observations, "errorType": type(error).__name__, "detail": str(error) if isinstance(error, Refusal) else "operation failed; inspect retained paths", "rollbackPerformed": False, "observationsDoNotAuthorizeRecoveryOrProveArtifactOrigin": True}


def quarantine(args):
    require(args.allow_quarantine_own_package_fingerprints, "explicit quarantine flag required")
    plan_path = no_reparse(Path(args.plan))
    data, _ = read_plain(plan_path, 64_000_000)
    require(sha(data) == args.plan_sha256, "reviewed plan pin differs")
    plan = json.loads(data)
    core = plan["core"]
    require(core["currentRoot"] == str(no_reparse(Path(args.current_root))) and core["currentLibSha256"] == args.current_lib_sha256, "explicit current root/pin differs from plan")
    # Reconstruct every selected path and all file/content/identity pins, never
    # trust a serialized arbitrary move source or destination.
    replay = argparse.Namespace(workspace_root=core["workspaceRoot"], target_root=core["targetRoot"], current_root=args.current_root, current_lib_sha256=args.current_lib_sha256, profile=core["profiles"], only_directory=core["explicitNames"], quarantine_tag=Path(core["quarantineRoot"]).name)
    require(plan_core(replay) == core, "plan source/context/fingerprint pins changed")
    target = Path(core["targetRoot"])
    destination = under(Path(core["quarantineRoot"]), target)
    require(destination.parent == target / "cache-origin-quarantines", "quarantine parent differs")
    destination.parent.mkdir(exist_ok=True)
    no_reparse(destination.parent)
    destination.mkdir()
    write_fresh(destination / "before.json", data)
    moved = []
    in_flight = None
    try:
        for item in core["fingerprintDirectories"]:
            no_compiler_processes()
            source = under(target / item["profile"] / ".fingerprint" / item["name"], target)
            receiver = under(destination / item["profile"] / item["name"], destination)
            sequence = str(len(moved) + 1).zfill(4)
            in_flight = {"sequence": sequence, "source": str(source), "retained": str(receiver), "profile": item["profile"], "name": item["name"], "expectedDirectoryIdentity": item["directoryIdentity"], "expectedFiles": item["files"], "phase": "validating_before_rename", "renameAttempted": False, "renameReturned": False, "retainedVerificationCompleted": False, "moveRecordWritten": False}
            # Exact selected directory only; no recursive enumeration/move/delete.
            actual, _ = inventory(target, Path(core["currentRoot"]), [item["profile"]], [item["name"]])
            require(actual == [item], "fingerprint changed before rename")
            receiver.parent.mkdir(exist_ok=True)
            no_reparse(receiver.parent)
            require(not receiver.exists() and source.stat().st_dev == receiver.parent.stat().st_dev, "rename destination/cross-device refused")
            no_reparse(source)
            # Intent is retained before attempting the rename. A crash can still
            # leave its outcome unknown; this record is not an atomic commit.
            write_fresh(destination / ("intent-" + sequence + ".json"), encoded(in_flight))
            in_flight["phase"] = "rename_requested"
            in_flight["renameAttempted"] = True
            os.rename(source, receiver)
            in_flight["renameReturned"] = True
            in_flight["phase"] = "renamed_pending_verification"
            require(not source.exists(), "fingerprint source still present after rename")
            retained_files = {}
            for name, pinned in item["files"].items():
                _, retained = read_plain(receiver / name)
                require(retained == pinned, "retained fingerprint file differs")
                retained_files[name] = retained
            entry = {"source": str(source), "retained": str(receiver), "beforeDirectoryIdentity": item["directoryIdentity"], "afterDirectoryIdentity": identity(receiver.stat()), "beforeFiles": item["files"], "afterFiles": retained_files}
            in_flight["retainedVerificationCompleted"] = True
            in_flight["phase"] = "verified_pending_move_record"
            write_fresh(destination / ("move-" + sequence + ".json"), encoded(entry))
            in_flight["moveRecordWritten"] = True
            moved.append(entry)
            in_flight = None
        no_compiler_processes()
        for item in core["fingerprintDirectories"]:
            source = under(target / item["profile"] / ".fingerprint" / item["name"], target)
            require(not source.exists(), "selected fingerprints reappeared")
        remaining_count = None
        if core["selection"] == "all_own_package_fingerprints_for_context_switch":
            remaining, _ = inventory(target, Path(core["currentRoot"]), core["profiles"], [])
            require(not remaining, "own-package fingerprints remain after full invalidation")
            remaining_count = 0
        _, _, _, final_source_pins = context(Path(core["workspaceRoot"]), target, Path(core["currentRoot"]), core["currentLibSha256"])
        require(final_source_pins == core["currentSourcePins"], "current source anchor changed")
        receipt = {"status": "selected_fingerprint_metadata_retained_and_invalidated", "reviewedPlanSha256": args.plan_sha256, "profiles": core["profiles"], "selection": core["selection"], "cacheMutationPerformed": True, "movedDirectories": moved, "remainingOwnPackageDirectoryCount": remaining_count, "unselectedFingerprintContentsInspectedDuringSubset": False, "completeOwnPackageInvalidationForSelectedProfiles": core["selection"] == "all_own_package_fingerprints_for_context_switch", "beforeCurrentSourcePins": core["currentSourcePins"], "afterCurrentSourcePins": final_source_pins, "artifactProvenanceVerified": False, "compiledArtifactDeletion": False, "normalSourceWrites": False, "cargoInvoked": False, "laterCompileAndSourceOriginEvidenceStillRequired": True}
        write_fresh(destination / "receipt.json", encoded(receipt))
        return receipt
    except Exception as error:
        write_fresh(destination / "failure.json", encoded(failure_record(moved, in_flight, error)))
        raise


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="operation", required=True)
    create = sub.add_parser("plan")
    for name in ("workspace-root", "target-root", "current-root", "current-lib-sha256", "quarantine-tag", "plan-out"):
        create.add_argument("--" + name, required=True)
    create.add_argument("--profile", action="append", choices=("debug", "release"))
    create.add_argument("--only-directory", action="append")
    apply = sub.add_parser("quarantine")
    for name in ("plan", "plan-sha256", "current-root", "current-lib-sha256"):
        apply.add_argument("--" + name, required=True)
    apply.add_argument("--allow-quarantine-own-package-fingerprints", action="store_true")
    args = parser.parse_args()
    try:
        if args.operation == "plan":
            core = plan_core(args)
            output = Path(args.plan_out)
            expected_parent = Path(core["targetRoot"]) / "cache-origin-plans"
            output = under(output, Path(core["targetRoot"]))
            require(output.parent == expected_parent and re.fullmatch(r"[a-z0-9][a-z0-9-]{0,79}\.json", output.name), "plan output must be a fresh named file in target/cache-origin-plans")
            payload = {"createdAtUtc": datetime.now(timezone.utc).isoformat(), "core": core}
            write_fresh(output, encoded(payload))
            print(json.dumps({"plan": str(output), "sha256": sha(output.read_bytes()), "selectedDirectories": core["directoryCount"], "selection": core["selection"], "cacheMutationPerformed": False}))
        else:
            result = quarantine(args)
            print(json.dumps({"status": result["status"], "moved": len(result["movedDirectories"]), "profiles": result["profiles"], "completeOwnPackageInvalidationForSelectedProfiles": result["completeOwnPackageInvalidationForSelectedProfiles"], "artifactProvenanceVerified": False}))
    except (Refusal, OSError, ValueError, KeyError) as error:
        print(json.dumps({"status": "refused", "errorType": type(error).__name__, "detail": str(error) if isinstance(error, Refusal) else "invalid input or filesystem operation; no success claimed"}), file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
