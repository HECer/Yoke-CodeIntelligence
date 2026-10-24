"""Owned Windows CI acceptance for the release host-document history workflow.

Run after the actual release build. Native processes run only in the suspended,
assigned-before-resume worker Job. Fixture inputs are inert, private, create-only
data; capabilities stay in memory and never enter the uploaded evidence.
"""
import argparse
import base64
import ctypes
import hashlib
import http.client
import json
import os
from pathlib import Path
import queue
import re
import shutil
import struct
import subprocess
import sys
import tempfile
import threading
import time
import urllib.parse


ROOT = Path(__file__).resolve().parents[1]
MAX_JSON = 1_000_000
MAX_FRAME = 16_000_000
MAX_OUTPUT = 32_000_000
MIN_FREE = 500_000_000
MAGIC = b"YOKE-DOCUMENT-HISTORY\0\x01"


def source_inputs():
    selected = subprocess.check_output(["git", "ls-files", "-z", "--", "Cargo.toml", "Cargo.lock",
        "package.json", "package-lock.json", "native", "src", "python", "scripts",
        ".github/workflows/rust.yml"], cwd=ROOT)
    require(len(selected) <= MAX_JSON, "Source inventory capacity exceeded")
    names = selected.decode("utf-8").split("\0")
    require(names[-1] == "" and 0 < len(names) - 1 <= 2048, "Source inventory cardinality differs")
    result = {}
    total = 0
    for name in names[:-1]:
        require(name not in result and "\\" not in name
                and all(part not in ("", ".", "..") for part in name.split("/")), "Unsafe source inventory path")
        path = ROOT / name
        require(path.is_file() and not path.is_symlink() and not path.is_junction()
                and path.resolve().is_relative_to(ROOT), "Linked source input refused")
        size = path.stat().st_size
        total += size
        require(size <= 16_000_000 and total <= 96_000_000, "Source inventory byte capacity exceeded")
        result[name] = sha(path)
    return result


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def sha(path):
    value = hashlib.sha256()
    with Path(path).open("rb") as source:
        for block in iter(lambda: source.read(1_000_000), b""):
            value.update(block)
    return value.hexdigest()


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, "Duplicate JSON field")
        result[key] = value
    return result


def decode(data, maximum=MAX_JSON):
    require(len(data) <= maximum, "JSON capacity exceeded")
    return json.loads(data, object_pairs_hook=unique_object)


def read_json(path, maximum=MAX_JSON):
    require(path.is_file() and not path.is_symlink() and not path.is_junction()
            and path.stat().st_size <= maximum, "JSON file admission failed")
    with path.open("rb") as source:
        return decode(source.read(maximum + 1), maximum)


def write_json(path, value):
    data = json.dumps(value, ensure_ascii=False, indent=2).encode("utf-8") + b"\n"
    require(len(data) <= MAX_JSON, "Evidence JSON capacity exceeded")
    with Path(path).open("xb") as target:
        target.write(data)


def local_endpoint(value):
    parsed = urllib.parse.urlparse(value)
    require(parsed.scheme == "http" and parsed.hostname == "127.0.0.1"
            and parsed.username is None and parsed.password is None
            and parsed.path == "/" and not parsed.query
            and parsed.port is not None
            and re.fullmatch(r"[0-9a-f]{64}", parsed.fragment),
            "Unexpected private dashboard endpoint")
    return parsed.port, parsed.fragment


def admitted_frame(data, plan, originals):
    require(len(data) <= MAX_FRAME and data.startswith(MAGIC), "Invalid archive framing")
    offset = len(MAGIC)

    def take(length):
        nonlocal offset
        require(0 <= length <= len(data) - offset, "Truncated archive framing")
        result = data[offset:offset + length]
        offset += length
        return result

    def length():
        return struct.unpack("<I", take(4))[0]

    size = length()
    require(size <= 512_000 and decode(take(size), 512_000) == plan, "Archive manifest differs")
    require(0 < len(plan["records"]) <= 512, "Archive record capacity differs")
    rows = []
    seen = set()
    for record in plan["records"]:
        name = record["name"]
        require(isinstance(name, str) and name not in seen and name in originals
                and re.fullmatch(r"yoke-doc-[0-9a-f]{64}-[0-9a-f]{64}\.(?:plan|e[0-9]{4})\.json", name),
                "Unknown, duplicate or unsafe archive record")
        seen.add(name)
        size = length()
        require(type(record["bytes"]) is int and 0 < size == record["bytes"] <= 512_000
                and re.fullmatch(r"[0-9a-f]{64}", record["hash"]), "Archive record size or hash differs")
        content = take(size)
        digest = hashlib.sha256(content).hexdigest()
        require(content == originals[name] and digest == record["hash"], "Original archive bytes differ")
        rows.append({"name": name, "bytes": size, "sha256": digest})
    require(offset == len(data), "Trailing archive bytes")
    for name in seen:
        stem = re.fullmatch(r"(yoke-doc-[0-9a-f]{64}-[0-9a-f]{64})\.(?:plan|e[0-9]{4})\.json", name).group(1)
        original_journal = {key for key in originals if key.startswith(stem + ".")}
        require(original_journal <= seen, "Package splits an original journal")
    return rows


class Actor:
    """One owning stdio Session; each reader closes over its own process."""

    def __init__(self, native, source, profile, output, label, protocol="native", python=None):
        self.frames = queue.Queue(maxsize=8)
        self.reader_error = None
        self.identifier = 0
        self.protocol = protocol
        self.port = self.capability = None
        self.closed = False
        # Native diagnostics stay with the private fixture, outside the uploaded
        # evidence tree. Management capabilities are never transcribed.
        self.stderr = (profile.parent / (label + ".stderr")).open("xb")
        argv = [str(native), "--root", str(source), "--timeout", "30000", "--max-bytes", "4000000",
                "serve-session", "--profile", protocol, "--id", label, "--host-profile", str(profile)]
        if python:
            argv += ["--python", str(python), "--trust-python"]
        self.process = subprocess.Popen(argv, cwd=source, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                        stderr=self.stderr, creationflags=subprocess.CREATE_NO_WINDOW)
        self.reader = threading.Thread(target=self._read, daemon=True)
        self.reader.start()
        initialized = self.rpc("initialize", {"protocolVersion": "2025-06-18", "capabilities": {},
                              "clientInfo": {"name": "owned-host-history-ci", "version": "0.5.0"}})
        require(initialized["serverInfo"]["version"] == "0.5.0", "Unexpected native version")
        opened = self.tool("open_dashboard")
        require(opened["browserOpened"] is False, "Native attempted to launch a browser")
        self.port, self.capability = local_endpoint(opened["url"])
        self.fingerprint = self.state()["view"]["management"]["sessionStateSha256"]

    def _read(self):
        try:
            while True:
                line = self.process.stdout.readline(MAX_JSON + 1)
                if not line:
                    return
                require(len(line) <= MAX_JSON and line.endswith(b"\n"), "Protocol frame capacity exceeded")
                self.frames.put(decode(line), timeout=2)
        except BaseException as error:
            # Never retain a response body/capability in an error message.
            self.reader_error = type(error).__name__

    def rpc(self, method, params):
        self.identifier += 1
        request = {"jsonrpc": "2.0", "id": self.identifier, "method": method, "params": params}
        self.process.stdin.write(json.dumps(request, ensure_ascii=False).encode() + b"\n")
        self.process.stdin.flush()
        deadline = time.monotonic() + 10
        for _ in range(33):
            require(self.reader_error is None, "Owned protocol reader failed")
            remaining = deadline - time.monotonic()
            require(remaining > 0, "Owned protocol response deadline")
            frame = self.frames.get(timeout=remaining)
            if frame.get("method") == "notifications/tools/list_changed":
                continue
            require(frame.get("id") == self.identifier and "error" not in frame,
                    "Unexpected owned protocol response")
            return frame["result"]
        raise RuntimeError("Owned notification capacity exceeded")

    def tool(self, name, arguments=None):
        arguments = arguments or {}
        if self.protocol == "native" and name in ("open_dashboard", "get_current_config"):
            arguments = {"operation": {"open_dashboard": "openDashboard",
                                       "get_current_config": "configuration"}[name], "args": arguments}
            name = "ci_session"
        result = self.rpc("tools/call", {"name": name, "arguments": arguments})
        require(result.get("isError") is False, "Owned native tool refused")
        value = result.get("structuredContent")
        if value is None:
            value = decode(result["content"][0]["text"].encode())
        require(value["status"] == "ok", "Owned native tool incomplete")
        return value["data"]

    def http(self, method, path, value=None):
        connection = http.client.HTTPConnection("127.0.0.1", self.port, timeout=6)
        headers = {"Authorization": "Bearer " + self.capability,
                   "Origin": "http://127.0.0.1:" + str(self.port)}
        body = None
        if value is not None:
            headers["Content-Type"] = "application/json"
            body = json.dumps(value, ensure_ascii=False).encode()
        try:
            connection.request(method, path, body, headers)
            response = connection.getresponse()
            data = response.read(MAX_JSON + 1)
            require(len(data) <= MAX_JSON, "Management response capacity exceeded")
            # Rejections may have a fixed plain-text body. Do not log it.
            return response.status, decode(data) if response.status == 200 else None
        finally:
            connection.close()

    def state(self):
        status, value = self.http("GET", "/api/state")
        require(status == 200, "Owning dashboard unavailable")
        return value

    def manage(self, action):
        view = self.state()["view"]
        return self.http("POST", "/api/manage", {
            "id": os.urandom(32).hex(), "sessionStateSha256": view["management"]["sessionStateSha256"],
            "project": view["activeProject"], "action": action})

    def document(self, request, expected=200):
        status, value = self.manage({"kind": "document", "request": request})
        require(status == expected, "Unexpected selected document outcome")
        return value["data"] if status == 200 else None

    def plan(self, document, content):
        snapshot = self.document({"kind": "read", "documentId": document})["document"]
        return self.document({"kind": "plan", "documentId": document,
                              "expectedRevision": snapshot["revision"], "content": content})["plan"]

    def apply(self, document, plan, expected=200):
        return self.document({"kind": "apply", "documentId": document, "planId": plan["id"]}, expected)

    def shutdown(self):
        status, value = self.manage({"kind": "shutdown"})
        require(status == 200 and value["data"]["shutdownAccepted"] is True
                and value["data"]["processExitConfirmed"] is False, "Owning shutdown acknowledgement differs")
        self.process.wait(timeout=8)
        self.reader.join(timeout=3)
        require(self.process.returncode == 0 and not self.process.stdin.closed
                and not self.reader.is_alive() and self.reader_error is None, "Owning exit not proved")
        proof = {"nativeExitCode": 0, "stdinWasStillOpen": True, "beforeJobCleanup": True}
        self.close()
        return proof

    def close(self):
        if self.closed:
            return
        if self.process.poll() is None:
            self.process.kill()
            self.process.wait(timeout=8)
        self.reader.join(timeout=3)
        require(not self.reader.is_alive(), "Owned protocol reader did not stop")
        self.process.stdin.close()
        self.process.stdout.close()
        self.stderr.close()
        self.capability = None
        self.closed = True


def tree_hashes(*roots):
    result = {}
    for index, root in enumerate(roots):
        for path in sorted(root.iterdir()):
            require(path.is_file() and not path.is_symlink() and not path.is_junction(),
                    "Unexpected fixture journal entry")
            result[str(index) + "/" + path.name] = sha(path)
    return result


def retain_foreign(path, destination, expected):
    require(path.parent.resolve() == destination.parent.resolve() and path.is_file()
            and not path.is_symlink() and not destination.exists() and path.read_bytes() == expected,
            "Owned fixture relocation precondition differs")
    path.rename(destination)
    require(destination.read_bytes() == expected, "Retained foreign fixture differs")


def exercise(native, fixture, output, label, protocol, python):
    fixture.mkdir()
    output.mkdir()
    source, old, new, archive, host = [fixture / name for name in ("source-İ", "old", "new", "archive-π", "host")]
    for path in (source, old, new, archive, host):
        path.mkdir()
    source_bytes = "// owned inert e\u0301 🦀\r\nfn untouched() {}\r\n".encode()
    (source / "main.rs").write_bytes(source_bytes)
    target = host / "settings-π.yaml"
    original = "\ufeff# owned configuration π\r\nvalue: preserved\r\n".encode()
    target.write_bytes(original)
    config = {"documents": [{"id": "old", "label": "Owned old settings", "path": str(target),
                              "scope": {"kind": "global"}, "writable": True}],
              "journalRoot": str(old), "maxDocumentBytes": 64000}
    destination = decode(json.dumps(config).encode())
    destination["documents"][0].update(id="new", label="Owned destination settings")
    destination["journalRoot"] = str(new)
    roots = [{"name": "default", "root": str(source)}]
    before_selection, after_selection = fixture / "from.json", fixture / "to.json"
    write_json(before_selection, {"documents": config, "projects": roots})
    write_json(after_selection, {"documents": destination, "projects": roots})

    def profile(path, documents):
        write_json(path, {"context": "desktop-app", "modes": ["editing"], "steering": {},
                         "dashboard": {"management": {"sessionControls": False, "memories": False,
                         "documents": documents, "shutdown": True, "operationTimeoutMs": 3000},
                         "lifetimeMs": 600000}})

    old_profile, new_profile = fixture / "old-host.json", fixture / "new-host.json"
    profile(old_profile, config)
    profile(new_profile, destination)
    command = [str(native), "--timeout", "30000", "--max-bytes", "1000000", "host-documents",
               "--from-selection", str(before_selection), "--to-selection", str(after_selection),
               "--history-record-budget", "32"]

    def cli(name, arguments, success=True):
        stdout, stderr = output / (name + ".stdout"), output / (name + ".stderr")
        with stdout.open("xb") as out_file, stderr.open("xb") as err_file:
            result = subprocess.run(command + arguments, cwd=source, stdout=out_file, stderr=err_file,
                                    timeout=40, creationflags=subprocess.CREATE_NO_WINDOW)
        write_json(output / (name + ".result.json"), {"command": name, "exitCode": result.returncode,
                                                    "expectedSuccess": success})
        require(stdout.stat().st_size <= MAX_JSON and stderr.stat().st_size <= MAX_JSON,
                "CLI output capacity exceeded")
        require((result.returncode == 0) == success, "Unexpected owned CLI outcome: " + name)
        return decode(stdout.read_bytes()) if success else None

    def args(operation, path, *extra):
        return [operation, "--plan", str(path), "--archive-root", str(archive), *extra]

    actors = []
    runtime = None
    try:
        previous = Actor(native, source, old_profile, output, label + "-old", protocol, python)
        actors.append(previous)
        for saved in range(6):
            content = original.decode() + "# explicit saved history " + str(saved) + " 🦀\r\n"
            plan = previous.plan("old", content)
            previous.apply("old", plan)
            require(target.read_bytes() == content.encode(), "Original save bytes differ")
        old_cached = previous.plan("old", "# old proposal remains retired\r\n")
        if python:
            session = previous.tool("get_current_config")["sessionId"]
            value = previous.tool("serena_repl", {"session_id": session, "code": "owned_marker = 45\nprint(owned_marker)"})
            require(value["stdout"] == "45\n", "Persistent runtime initialization differs")
            pid = int(previous.tool("serena_repl", {"session_id": session,
                        "code": "import os\nprint(os.getpid())"})["stdout"].strip())
            runtime = RuntimeHandle(pid, python)
        before = target.read_bytes()
        migration = cli("migration-plan", ["binding-plan", "--from-document", "old", "--to-document", "new"])
        migration_path = fixture / "migration.json"
        write_json(migration_path, migration)
        receipt = cli("migration-apply", ["binding-apply", "--plan", str(migration_path), "--allow-host-writes"])
        require(receipt["outcome"] == "committed" and receipt["fileChanged"] is False
                and target.read_bytes() == before, "Migration modified configuration bytes")
        owner = host / (".yoke-doc-owner-" + migration["target"] + ".json")
        owner_hash = sha(owner)
        current = Actor(native, source, new_profile, output, label + "-current")
        actors.append(current)
        current_cached = current.plan("new", "# current approved proposal must block during archival\r\n")
        require(current_cached["ownerGeneration"] == migration["nonce"], "Current proposal generation differs")
        originals = {path.name: path.read_bytes() for path in old.iterdir()
                     if path.name.startswith("yoke-doc-" + migration["fromAccess"] + "-")}
        require(len(originals) > 32, "Fixture does not exercise a bounded remainder")
        first = cli("first-plan", ["history-plan", "--migration", str(migration_path), "--archive-root", str(archive)])
        require(first["recordBudget"] == 32 and 0 < len(first["records"]) <= 32
                and first["unselectedClosedJournalsAtReview"] > 0, "Missing explicit package remainder")
        first_path = fixture / "first.json"
        write_json(first_path, first)
        baseline = tree_hashes(old, new)
        cli("no-grant", args("history-apply", first_path), False)
        require(tree_hashes(old, new) == baseline, "Missing grant mutated journals")
        foreign_archive = archive / ("yoke-doc-history-" + first["id"] + ".bin")
        foreign_archive.write_bytes(b"owned foreign archive occupant")
        cli("foreign-output", args("history-apply", first_path, "--allow-host-writes"), False)
        require(tree_hashes(old, new) == baseline, "Foreign output mutated journals")
        retain_foreign(foreign_archive, archive / "retained-foreign.bin", b"owned foreign archive occupant")
        completion = host / (".yoke-doc-history-" + first["id"] + ".complete.json")
        completion.write_bytes(b"owned foreign completion")
        cli("blocked-completion", args("history-apply", first_path, "--allow-host-writes"), False)
        pending = cli("pending-inspect", args("history-inspect", first_path))
        require(pending["phase"] == "committed" and pending["markerPresent"] is True
                and pending["complete"] is False and pending["immutableArchiveVerified"] is True,
                "Blocked completion was reported complete")
        unchanged = tree_hashes(old, new)
        for actor, document, cached in ((previous, "old", old_cached), (current, "new", current_cached)):
            data = actor.document({"kind": "pending", "documentId": document})
            require(data["blockedByHistoryArchive"] is True and data["complete"] is False
                    and data["journals"] == [], "Missing incomplete history blocker")
            actor.apply(document, cached, 422)
        require(tree_hashes(old, new) == unchanged and target.read_bytes() == before
                and sha(owner) == owner_hash, "Pending authority refusal mutated bytes")
        retain_foreign(completion, host / "retained-foreign-completion.json", b"owned foreign completion")
        cli("stale-event", args("history-recover", first_path, "--expected-event", "a" * 64,
                                "--direction", "resume", "--allow-host-writes"), False)
        require(tree_hashes(old, new) == unchanged, "Stale event mutated journals")
        completed = cli("first-resume", args("history-recover", first_path, "--expected-event", pending["eventId"],
                                            "--direction", "resume", "--allow-host-writes"))
        require(completed["complete"] is True and completed["markerPresent"] is False
                and completed["allClosedJournalsSelectedAtReview"] is False
                and completed["originalAbsenceProvesThisProcessDeletedIt"] is False, "First package result differs")
        second = cli("second-plan", ["history-plan", "--migration", str(migration_path), "--archive-root", str(archive)])
        require(second["id"] != first["id"] and second["unselectedClosedJournalsAtReview"] == 0,
                "Remainder lacks separate fresh review")
        second_path = fixture / "second.json"
        write_json(second_path, second)
        completed = cli("second-apply", args("history-apply", second_path, "--allow-host-writes"))
        require(completed["complete"] and completed["allClosedJournalsSelectedAtReview"], "Second package incomplete")
        require({r["name"] for p in (first, second) for r in p["records"]} == set(originals), "Package membership differs")
        frame_hashes = {}
        for tag, plan, path in (("first", first, first_path), ("second", second, second_path)):
            frame = archive / ("yoke-doc-history-" + plan["id"] + ".bin")
            require(frame.stat().st_size <= MAX_FRAME, "Frame capacity exceeded")
            rows = admitted_frame(frame.read_bytes(), plan, originals)
            frame_hashes[frame] = sha(frame)
            write_json(output / (tag + "-frame-proof.json"), {"sha256": sha(frame), "records": rows})
            for index, record in enumerate(plan["records"]):
                read = cli(tag + "-read-" + str(index), args("history-read", path, "--record", record["name"]))
                require(base64.b64decode(read["bytesBase64"], validate=True) == originals[record["name"]]
                        and read["sha256"] == record["hash"] and read["immutableRecordEvidenceOnly"] is True,
                        "Immutable record read differs")
        cli("unknown-read", args("history-read", first_path, "--record", "../foreign"), False)
        require(not any((old / name).exists() for name in originals)
                and (old / ("yoke-binding-migration-" + migration["id"] + ".plan.json")).exists()
                and target.read_bytes() == before and sha(owner) == owner_hash, "Archival changed unrelated evidence")
        for actor in actors:
            require(actor.state()["view"]["management"]["sessionStateSha256"] == actor.fingerprint,
                    "Existing owning Session reloaded")
        fresh_text = "# newly reviewed destination save after archival π\r\n"
        fresh = current.plan("new", fresh_text)
        current.apply("new", fresh)
        header = decode((new / ("yoke-doc-" + migration["toAccess"] + "-" + fresh["id"] + ".plan.json")).read_bytes())
        require(target.read_bytes() == fresh_text.encode() and sha(owner) == owner_hash
                and header["plan"]["ownerGeneration"] == migration["nonce"]
                and header["plan"]["afterHash"] == sha(target)
                and all(sha(path) == digest for path, digest in frame_hashes.items()), "Fresh destination save differs")
        previous.apply("old", old_cached, 422)
        current_exit = current.shutdown()
        require(previous.state()["view"]["management"]["sessionStateSha256"] == previous.fingerprint,
                "Ending destination ended another Session")
        if python:
            require(previous.tool("serena_repl", {"session_id": session, "code": "print(owned_marker)"})["stdout"] == "45\n",
                    "Existing runtime value changed")
        previous_exit = previous.shutdown()
        if runtime:
            runtime.require_exited()
        require((source / "main.rs").read_bytes() == source_bytes, "Analyzed source changed")
        return {"variant": label, "complete": True, "packages": 2, "originalRecords": len(originals),
                "allOriginalsReadBack": True, "oldAndCurrentAuthorityRefusedDuringPending": True,
                "configurationAndOwnerExactDuringArchival": True, "freshSameHostSaveAfterArchival": True,
                "actualJournalCurrentGeneration": True, "bothArchiveFramesRemainExact": True,
                "persistent45Retained": bool(python), "runtimeExitBeforeJobCleanup": bool(runtime),
                "currentExit": current_exit, "oldExit": previous_exit, "analyzedSourceUnchanged": True}
    finally:
        for actor in reversed(actors):
            actor.close()
        if runtime:
            runtime.close()


class RuntimeHandle:
    def __init__(self, pid, executable):
        self.api = ctypes.WinDLL("kernel32", use_last_error=True)
        self.api.OpenProcess.argtypes = [ctypes.c_ulong, ctypes.c_int, ctypes.c_ulong]
        self.api.OpenProcess.restype = ctypes.c_void_p
        self.api.QueryFullProcessImageNameW.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_wchar_p,
                                                       ctypes.POINTER(ctypes.c_ulong)]
        self.api.QueryFullProcessImageNameW.restype = ctypes.c_int
        self.api.WaitForSingleObject.argtypes = [ctypes.c_void_p, ctypes.c_ulong]
        self.api.WaitForSingleObject.restype = ctypes.c_ulong
        self.api.CloseHandle.argtypes = [ctypes.c_void_p]
        self.api.CloseHandle.restype = ctypes.c_int
        self.handle = self.api.OpenProcess(0x00101000, False, pid)
        require(bool(self.handle), "Owned runtime handle unavailable")
        try:
            name = ctypes.create_unicode_buffer(32768)
            size = ctypes.c_ulong(len(name))
            require(self.api.QueryFullProcessImageNameW(self.handle, 0, name, ctypes.byref(size))
                    and Path(name.value).resolve() == Path(executable).resolve()
                    and self.api.WaitForSingleObject(self.handle, 0) == 258, "Owned runtime identity differs")
        except BaseException:
            self.close()
            raise

    def require_exited(self):
        require(self.api.WaitForSingleObject(self.handle, 5000) == 0, "Runtime did not exit before Job cleanup")

    def close(self):
        if self.handle:
            require(bool(self.api.CloseHandle(self.handle)), "Runtime handle close failed")
            self.handle = None


def output_size(root):
    total = count = 0
    for path in root.rglob("*"):
        require(not path.is_symlink() and not path.is_junction(), "Unexpected evidence link")
        count += 1
        require(count <= 1024, "Evidence entry capacity exceeded")
        if path.is_file():
            try:
                total += path.stat().st_size
            except FileNotFoundError:
                # SQLite sidecars in private fixtures can close concurrently.
                continue
            require(total <= MAX_OUTPUT, "Evidence byte capacity exceeded")
    return total


def worker_job_name(value):
    require(isinstance(value, str) and re.fullmatch(r"Local\\YokeHistoryCI-[0-9a-f]{64}", value),
            "Explicit owned worker Job name required")
    return value


def require_worker_job(name):
    """Prove membership in this parent's specific Job before any native read.

    Query access cannot assign this process to a Job. Close the non-inheritable
    query handle before continuing so it cannot retain kill-on-close ownership.
    """
    name = worker_job_name(name)
    api = ctypes.WinDLL("kernel32", use_last_error=True)
    for function, args, result in (
        ("OpenJobObjectW", [ctypes.c_uint32, ctypes.c_int, ctypes.c_wchar_p], ctypes.c_void_p),
        ("GetCurrentProcess", [], ctypes.c_void_p),
        ("IsProcessInJob", [ctypes.c_void_p, ctypes.c_void_p, ctypes.POINTER(ctypes.c_int)], ctypes.c_int),
        ("CloseHandle", [ctypes.c_void_p], ctypes.c_int),
    ):
        entry = getattr(api, function)
        entry.argtypes, entry.restype = args, result
    handle = api.OpenJobObjectW(0x0004, False, name)
    require(bool(handle), "Owned worker Job unavailable")
    try:
        assigned = ctypes.c_int()
        require(api.IsProcessInJob(api.GetCurrentProcess(), handle, ctypes.byref(assigned))
                and assigned.value, "Worker is outside its assigned Job")
    finally:
        require(api.CloseHandle(handle), "Owned worker Job query handle close failed")
    return {"specificJobMembershipConfirmed": True, "queryHandleClosedBeforeNativeRead": True}


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
    require(os.name == "nt" and shutil.disk_usage(ROOT).free >= MIN_FREE,
            "Native live worker platform or disk reserve unavailable")
    spec = read_json(spec_path, 64000)
    admission = require_worker_job(spec.get("workerJobName"))
    native, fixture, output = (Path(spec[key]) for key in ("native", "fixture", "output"))
    require(os.name == "nt" and native.is_absolute() and fixture.is_absolute() and output.is_absolute(),
            "Explicit Windows fixture paths required")
    require(sha(native) == spec["nativeSha256"], "Selected native bytes differ")
    variants = []
    complete = False
    try:
        for label, protocol, python in (("native", "native", None), ("serena", "serena", None),
                                        ("persistent-python", "serena", Path(sys.executable))):
            variants.append(exercise(native, fixture / label, output / label, label, protocol, python))
        require(sha(native) == spec["nativeSha256"], "Native executable changed")
        complete = True
    finally:
        write_json(output / "live-result.json", {"product": "Code Intelligence", "version": "0.5.0",
            "complete": complete, "nativeSha256": spec["nativeSha256"], "variants": variants,
            "workerJobAdmission": admission,
            "modelCalls": 0, "analyzedSourceExecuted": False, "existingCredentialsSelected": False,
            "nativeBrowserAcceptanceClaimed": False})
    return 0


def main(arguments=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--worker", type=Path, help=argparse.SUPPRESS)
    parser.add_argument("--native", type=Path)
    parser.add_argument("--output", type=Path)
    options = parser.parse_args(arguments)
    if options.worker:
        return worker(options.worker)
    require(os.name == "nt" and os.environ.get("GITHUB_ACTIONS") == "true",
            "Native live acceptance requires the explicit Windows CI environment")
    require(options.native is not None and options.output is not None, "Select native executable and output")
    require(shutil.disk_usage(ROOT).free >= MIN_FREE, "Native live acceptance disk reserve unavailable")
    selected_native = options.native.absolute()
    require(all(not path.is_symlink() and not path.is_junction()
                for path in (selected_native, selected_native.parent, selected_native.parent.parent)),
            "Linked release path refused")
    native = selected_native.resolve(strict=True)
    require(native == (ROOT / "target/release/yoke.exe").resolve(strict=True)
            and native.is_file() and not native.is_symlink(), "Use the just-built release executable")
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    require(re.fullmatch(r"[0-9a-f]{40}", revision) and revision == os.environ.get("GITHUB_SHA"),
            "CI source revision differs")
    output = options.output.absolute()
    require(not (ROOT / "validation").is_symlink() and not (ROOT / "validation").is_junction(),
            "Linked evidence namespace refused")
    require(output.parent.resolve() == (ROOT / "validation").resolve()
            and output.name == "ci-host-document-history", "Unexpected evidence destination")
    output.mkdir(parents=True, exist_ok=False)
    fixture = Path(tempfile.mkdtemp(prefix="yoke-owned-host-history-ci-")).resolve()
    environment = {name: os.environ[name] for name in ("PATH", "SystemRoot", "SystemDrive", "WINDIR") if name in os.environ}
    for key, name in (("HOME", "home"), ("USERPROFILE", "home"), ("APPDATA", "roaming"),
                      ("LOCALAPPDATA", "local"), ("TEMP", "tmp"), ("TMP", "tmp")):
        path = fixture / name
        path.mkdir(exist_ok=True)
        environment[key] = str(path)
    environment.update(PYTHONUTF8="1", PYTHONDONTWRITEBYTECODE="1", DO_NOT_TRACK="1")
    digest = sha(native)
    inputs = source_inputs()
    write_json(output / "source-inputs.json", {
        "sourceRevision": revision,
        "scope": "Tracked native/src/python/scripts and Cargo/package manifests; not the complete checkout",
        "sha256": inputs})
    spec_path = fixture / "spec.json"
    job_name = "Local\\YokeHistoryCI-" + os.urandom(32).hex()
    write_json(spec_path, {"native": str(native), "nativeSha256": digest, "fixture": str(fixture),
                          "output": str(output), "workerJobName": job_name})
    complete = False
    try:
        bounded_worker(spec_path, fixture, output, environment, job_name)
        result = read_json(output / "live-result.json")
        require(result["complete"] is True and len(result["variants"]) == 3 and sha(native) == digest,
                "Live acceptance proof incomplete")
        require(result.get("workerJobAdmission") == {"specificJobMembershipConfirmed": True,
                    "queryHandleClosedBeforeNativeRead": True}, "Worker Job admission proof incomplete")
        require(source_inputs() == inputs, "Selected source inputs changed during live acceptance")
        subprocess.run(["git", "diff", "--no-ext-diff", "--no-textconv", "--exit-code", "HEAD", "--"], cwd=ROOT, check=True,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        output_size(output)
        complete = True
    finally:
        write_json(output / "result.json", {"product": "Code Intelligence", "version": "0.5.0",
            "complete": complete, "sourceRevision": revision, "nativeSha256": digest,
            "validatorSha256": sha(Path(__file__)), "fixtureRoot": str(fixture),
            "unchangedDiskStopBytes": MIN_FREE, "nativeBrowserAcceptanceClaimed": False})
    print(json.dumps({"product": "Code Intelligence", "version": "0.5.0", "complete": complete,
                      "sourceRevision": revision, "nativeSha256": digest, "stdioVariants": 3}))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as error:
        # Tracebacks can retain protocol values. Upload bounded fixture evidence,
        # report only the error type, and fail rather than disclose capabilities.
        print("Owned host-history acceptance failed: " + type(error).__name__, file=sys.stderr)
        raise SystemExit(1)
