"""Private Windows CI HTTP actor for the real host-history browser harness.

No entry point, local mode, credential fallback, or product implementation.
Only the admitted browser worker constructs this actor. Secret bytes stay in
RAM and fresh logon-scoped Windows credentials, never fixture files/artifacts.
"""
from contextlib import contextmanager
import ctypes
import hashlib
import hmac
import importlib.util
import json
import os
from pathlib import Path
import queue
import re
import socket
import subprocess
import sys
import threading
import time


ROOT = Path(__file__).resolve().parents[1]
_loader = importlib.util.spec_from_file_location("owned_http_history_live", ROOT / "scripts/host_history_live.py")
LIVE = importlib.util.module_from_spec(_loader)
_loader.loader.exec_module(LIVE)
require = LIVE.require
MAX_JSON = LIVE.MAX_JSON
MAX_HEADER = 16_384
SERVICE_MS = 120_000
VERSION = "2025-11-25"
PREFIX = "yoke-code-intelligence/service/token/"


class CredentialApiFailure(RuntimeError):
    def __init__(self, operation, code):
        require(operation in ("read", "write", "delete") and type(code) is int and 0 <= code <= 0xFFFFFFFF,
                "Closed credential API failure required")
        super().__init__("Owned credential API failed")
        self.operation, self.code = operation, code


def report_actor_failure(stage, role, failure=None):
    allowed = {"credential-create", "service-start", "listener-readiness", "transport-auth",
               "transport-initialize", "dashboard-open", "cleanup"}
    stage = stage if type(stage) is str and stage in allowed else "other"
    role = role if type(role) is str and role in ("old", "current") else "other"
    detail = ""
    if type(failure) is CredentialApiFailure:
        operation, code = getattr(failure, "operation", None), getattr(failure, "code", None)
        detail = ("; credential-api=" + operation + "; code=" + str(code)
                  if type(operation) is str and operation in ("read", "write", "delete")
                  and type(code) is int and 0 <= code <= 0xFFFFFFFF
                  else "; credential-api=other; code=unavailable")
    try:
        print("Owned browser HTTP actor failed: stage=" + stage + "; role=" + role + detail, file=sys.stderr)
    except Exception:
        pass


def remaining(deadline):
    value = deadline - time.monotonic()
    require(value > 0, "Owned HTTP absolute deadline")
    return value


def parse_headers(raw):
    require(len(raw) <= MAX_HEADER, "Owned HTTP header capacity")
    lines = raw.split(b"\r\n")
    require(2 <= len(lines) <= 49 and re.fullmatch(rb"HTTP/1\.1 [1-5][0-9]{2} [\x20-\x7e]{1,80}", lines[0]),
            "Owned HTTP status/header framing differs")
    headers = {}
    for line in lines[1:]:
        name, separator, value = line.partition(b":")
        require(separator and re.fullmatch(rb"[A-Za-z0-9-]{1,64}", name)
                and re.fullmatch(rb"[\x20-\x7e]{0,4096}", value), "Owned HTTP header framing differs")
        key = name.decode("ascii").lower()
        require(key not in headers, "Duplicate owned HTTP header")
        headers[key] = value.strip().decode("ascii")
    require("transfer-encoding" not in headers and "trailer" not in headers
            and headers.get("connection", "").lower() == "close"
            and re.fullmatch(r"0|[1-9][0-9]{0,6}", headers.get("content-length", "")),
            "Owned HTTP response length/closure required")
    length = int(headers["content-length"])
    require(length <= MAX_JSON, "Owned HTTP response capacity")
    return int(lines[0][9:12]), headers, length


def read_reply(stream, deadline):
    wire = bytearray()
    boundary, length, status, headers = None, None, None, None
    while True:
        stream.settimeout(remaining(deadline))
        block = stream.recv(min(8192, MAX_HEADER + MAX_JSON + 1 - len(wire)))
        remaining(deadline)
        if not block:
            require(boundary is not None and len(wire) == boundary + length, "Owned HTTP response truncated")
            return status, headers, bytes(wire[boundary:])
        wire.extend(block)
        if boundary is None:
            mark = wire.find(b"\r\n\r\n")
            require(mark >= 0 or len(wire) <= MAX_HEADER, "Owned HTTP header capacity")
            if mark >= 0:
                require(mark + 4 <= MAX_HEADER, "Owned HTTP header capacity")
                status, headers, length = parse_headers(bytes(wire[:mark]))
                boundary = mark + 4
        require(len(wire) <= MAX_HEADER + MAX_JSON and
                (boundary is None or len(wire) <= boundary + length), "Owned HTTP response has extra bytes")


def request(port, body, token, session=None, *, origin=None, deadline=None):
    require(type(port) is int and 1 <= port <= 65535 and type(body) is bytes and len(body) <= MAX_JSON,
            "Fixed owned HTTP request required")
    require(token is None or (type(token) is str and re.fullmatch(r"[0-9a-f]{64}", token)),
            "Bounded owned HTTP credential required")
    require(session is None or (type(session) is str and re.fullmatch(r"[0-9a-f]{64}", session)),
            "Bounded owned HTTP Session required")
    require(origin is None or origin == "http://untrusted.invalid", "Fixed refusal Origin required")
    deadline = time.monotonic() + 10 if deadline is None else deadline
    headers = [f"POST /mcp HTTP/1.1", f"Host: 127.0.0.1:{port}", "Connection: close",
               "Accept: application/json, text/event-stream", "Content-Type: application/json",
               "Content-Length: " + str(len(body))]
    if token is not None:
        headers.append("Authorization: Bearer " + token)
    if session is not None:
        headers.extend(("Mcp-Session-Id: " + session, "Mcp-Protocol-Version: " + VERSION))
    if origin:
        headers.append("Origin: " + origin)
    with socket.create_connection(("127.0.0.1", port), timeout=remaining(deadline)) as stream:
        stream.settimeout(remaining(deadline))
        stream.sendall(("\r\n".join(headers) + "\r\n\r\n").encode("ascii") + body)
        return read_reply(stream, deadline)


def admit_rpc(headers, body, identifier):
    require(type(body) is bytes and len(body) <= MAX_JSON, "Bounded owned MCP response required")
    kind = headers.get("content-type")
    if kind == "application/json":
        frames = [LIVE.decode(body)]
    else:
        require(kind == "text/event-stream" and body.endswith(b"\n\n"), "Finite owned MCP response required")
        events = body[:-2].split(b"\n\n")
        require(0 < len(events) <= 33, "Owned MCP notification capacity")
        prefix = b"event: message\ndata: "
        require(all(event.startswith(prefix) and b"\n" not in event[len(prefix):] for event in events),
                "Owned MCP event framing differs")
        frames = [LIVE.decode(event[len(prefix):]) for event in events]
    for frame in frames[:-1]:
        require(type(frame) is dict and frame.get("jsonrpc") == "2.0"
                and frame.get("method") == "notifications/tools/list_changed" and "id" not in frame
                and "result" not in frame and "error" not in frame, "Unexpected owned MCP notification")
    reply = frames[-1]
    require(type(reply) is dict and reply.get("jsonrpc") == "2.0" and type(reply.get("id")) is int
            and reply["id"] == identifier and "error" not in reply and type(reply.get("result")) is dict,
            "Unexpected owned MCP response")
    return reply["result"]


class FileTime(ctypes.Structure):
    _fields_ = [("low", ctypes.c_uint32), ("high", ctypes.c_uint32)]


class Credential(ctypes.Structure):
    _fields_ = [("flags", ctypes.c_uint32), ("kind", ctypes.c_uint32), ("target", ctypes.c_wchar_p),
                ("comment", ctypes.c_wchar_p), ("last_written", FileTime), ("blob_size", ctypes.c_uint32),
                ("blob", ctypes.POINTER(ctypes.c_ubyte)), ("persist", ctypes.c_uint32),
                ("attribute_count", ctypes.c_uint32), ("attributes", ctypes.c_void_p),
                ("target_alias", ctypes.c_wchar_p), ("username", ctypes.c_wchar_p)]


def credential_value(value, target):
    require(value.flags == 0 and value.kind == 1 and value.target == target and value.persist == 1
            and value.attribute_count == 0 and value.blob_size == 32 and bool(value.blob),
            "Owned logon credential identity differs")
    return bytearray(ctypes.string_at(value.blob, 32))


class WindowsCredentialApi:
    def __init__(self):
        require(os.name == "nt" and ctypes.sizeof(ctypes.c_void_p) == 8 and ctypes.sizeof(Credential) == 80,
                "Windows x64 credential API required")
        self.advapi = ctypes.WinDLL("advapi32", use_last_error=True)
        self.kernel = ctypes.WinDLL("kernel32", use_last_error=True)
        signatures = ((self.advapi, "CredReadW", [ctypes.c_wchar_p, ctypes.c_uint32, ctypes.c_uint32,
                       ctypes.POINTER(ctypes.POINTER(Credential))], ctypes.c_int),
                      (self.advapi, "CredWriteW", [ctypes.POINTER(Credential), ctypes.c_uint32], ctypes.c_int),
                      (self.advapi, "CredDeleteW", [ctypes.c_wchar_p, ctypes.c_uint32, ctypes.c_uint32], ctypes.c_int),
                      (self.advapi, "CredFree", [ctypes.c_void_p], None),
                      (self.kernel, "CreateMutexW", [ctypes.c_void_p, ctypes.c_int, ctypes.c_wchar_p], ctypes.c_void_p),
                      (self.kernel, "WaitForSingleObject", [ctypes.c_void_p, ctypes.c_uint32], ctypes.c_uint32),
                      (self.kernel, "ReleaseMutex", [ctypes.c_void_p], ctypes.c_int),
                      (self.kernel, "CloseHandle", [ctypes.c_void_p], ctypes.c_int))
        for library, name, arguments, result in signatures:
            function = getattr(library, name)
            function.argtypes, function.restype = arguments, result

    @contextmanager
    def locked(self, target):
        # Same fixed create mutex namespace as the native credential store.
        name = "Global\\YokeServiceCreate-" + hashlib.sha256(target.lower().encode("ascii")).hexdigest()
        handle = self.kernel.CreateMutexW(None, False, name)
        require(bool(handle), "Owned credential mutex unavailable")
        acquired = False
        try:
            require(self.kernel.WaitForSingleObject(handle, 5000) in (0, 0x80), "Owned credential mutex deadline")
            acquired = True
            yield
        finally:
            released = bool(self.kernel.ReleaseMutex(handle)) if acquired else True
            closed = bool(self.kernel.CloseHandle(handle))
            require(released and closed, "Owned credential mutex cleanup unproven")

    def read(self, target):
        pointer = ctypes.POINTER(Credential)()
        if not self.advapi.CredReadW(target, 1, 0, ctypes.byref(pointer)):
            code = ctypes.get_last_error()
            if code == 1168:
                return None
            raise CredentialApiFailure("read", code)
        require(bool(pointer), "Owned credential buffer unavailable")
        try:
            return credential_value(pointer.contents, target)
        finally:
            value = pointer.contents
            if value.blob and 0 < value.blob_size <= 2560:
                ctypes.memset(value.blob, 0, value.blob_size)
            self.advapi.CredFree(pointer)

    def write(self, target, token):
        blob = (ctypes.c_ubyte * 32).from_buffer(token)
        value = Credential(flags=0, kind=1, target=target, blob_size=32, blob=blob, persist=1)
        if not self.advapi.CredWriteW(ctypes.byref(value), 0):
            raise CredentialApiFailure("write", ctypes.get_last_error())

    def delete(self, target):
        if not self.advapi.CredDeleteW(target, 1, 0):
            raise CredentialApiFailure("delete", ctypes.get_last_error())


class SessionCredential:
    """Create-only, logon-lifetime fixture credential; checked removal on normal teardown."""
    def __init__(self, api):
        self.api = api
        self.name = "owned-history-browser-" + os.urandom(24).hex()
        require(re.fullmatch(r"owned-history-browser-[0-9a-f]{48}", self.name) and len(self.name) <= 80,
                "Fixed owned credential namespace required")
        self.target = PREFIX + self.name
        self.token = bytearray(os.urandom(32))
        self.created, self.closed, self.removed = False, False, False
        try:
            with api.locked(self.target):
                require(api.read(self.target) is None, "Owned credential role occupied")
                api.write(self.target, self.token)
                self.created = True
                self.require_identity()
        except BaseException:
            if self.created:
                self.close()
            else:
                self.token[:] = bytes(32)
            raise

    def require_identity(self):
        value = self.api.read(self.target)
        try:
            require(value is not None and hmac.compare_digest(value, self.token), "Owned credential bytes changed")
        finally:
            if value is not None:
                value[:] = bytes(len(value))

    def bearer(self):
        require(self.created and not self.closed, "Live owned credential required")
        return self.token.hex()

    def close(self):
        if self.closed:
            require(self.removed, "Earlier owned credential cleanup unproven")
            return
        self.closed = True  # No second wait on a failed teardown.
        try:
            require(self.created, "Owned credential creation not proved")
            with self.api.locked(self.target):
                self.require_identity()
                self.api.delete(self.target)
                require(self.api.read(self.target) is None, "Owned credential absence unproven")
                self.removed = True
        finally:
            self.token[:] = bytes(32)


def require_windows_ci():
    require(os.name == "nt" and os.environ.get("GITHUB_ACTIONS") == "true"
            and os.environ.get("RUNNER_OS") == "Windows", "Explicit Windows CI HTTP actor required")


class HttpActor(LIVE.Actor):
    """One real MCP-HTTP Session and its independently bounded owning service."""
    def __init__(self, native, source, profile, output, label, job_name):
        LIVE.require_worker_job(job_name)
        require_windows_ci()
        require(label in ("browser-old", "browser-current") and profile.name ==
                ("old-host.json" if label == "browser-old" else "new-host.json"), "Fixed HTTP actor roles required")
        self.protocol, self.role = "session", "old" if label == "browser-old" else "current"
        self.identifier, self.sid, self.port, self.capability = 0, None, None, None
        self.process = self.reader = self.stderr = self.credential = None
        self.frames, self.reader_error = queue.Queue(maxsize=8), None
        self.closed, self.session_closed, self.credential_removed = False, False, False
        stage = "credential-create"
        try:
            self.credential = SessionCredential(WindowsCredentialApi())
            config = profile.parent / (label + "-http-host.json")
            LIVE.write_json(config, {"bind": "127.0.0.1:0", "endpoint": "/mcp", "requestTimeoutMs": 10000,
                "maxBodyBytes": MAX_JSON, "maxConnections": 4, "workers": 2, "maxSessions": 2,
                "maxSessionsPerPrincipal": 1, "sessionTtlMs": SERVICE_MS, "maxRequestsPerSession": 128,
                "principals": [{"id": label, "tokenCredentialName": self.credential.name,
                    "root": str(source), "profile": "session", "hostProfile": LIVE.read_json(profile, 128000)}]})
            self.stderr = (profile.parent / (label + "-http.stderr")).open("xb")
            self.service_started = time.monotonic()
            stage = "service-start"
            self.process = subprocess.Popen([str(native), "--root", str(source), "--timeout", "30000",
                "--max-bytes", "1000000", "serve-http", "--trust-service", "--duration-ms", str(SERVICE_MS),
                "--config", str(config)], cwd=source, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                stderr=self.stderr, creationflags=subprocess.CREATE_NO_WINDOW)
            self.reader = threading.Thread(target=self._read, daemon=True)
            self.reader.start()
            stage = "listener-readiness"
            ready = self.frames.get(timeout=10)
            require(type(ready) is dict and set(ready) == {"status", "transport", "address", "endpoint",
                    "grantAuthority", "secrets"} and ready["status"] == "listening"
                    and ready["transport"] == "mcp_streamable_http_json" and ready["endpoint"] == "/mcp"
                    and ready["grantAuthority"] == "explicit_host_profile"
                    and ready["secrets"] == "os_credential_store_and_ram_only"
                    and re.fullmatch(r"127\.0\.0\.1:[1-9][0-9]{0,4}", ready["address"]),
                    "Owned HTTP listener readiness differs")
            self.service_deadline = time.monotonic() + SERVICE_MS / 1000 + 8
            self.mcp_port = int(ready["address"].split(":")[1])
            require(self.mcp_port <= 65535, "Owned HTTP listener port differs")
            stage = "transport-auth"
            self.auth_refusals = self.check_authentication()
            stage = "transport-initialize"
            initialized = self.rpc("initialize", {"protocolVersion": VERSION, "capabilities": {},
                "clientInfo": {"name": "owned-host-history-browser-http", "version": "0.5.0"}})
            require(initialized["protocolVersion"] == VERSION and initialized["serverInfo"]["version"] == "0.5.0",
                    "Owned HTTP initialization differs")
            stage = "dashboard-open"
            opened = self.tool("open_dashboard")
            require(opened["browserOpened"] is False, "Native attempted to launch a browser")
            self.port, self.capability = LIVE.local_endpoint(opened["url"])
            self.fingerprint = self.state()["view"]["management"]["sessionStateSha256"]
        except BaseException as error:
            try:
                report_actor_failure(stage, self.role, error)
            finally:
                self.close()
            raise

    def wire(self, body, *, token=None, sid=None, origin=None, deadline=None):
        return request(self.mcp_port, body, token, sid, origin=origin, deadline=deadline)

    def check_authentication(self):
        body = b'{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}'
        rows = []
        for token, origin, expected in ((None, None, 401), (os.urandom(32).hex(), None, 401),
                                        (self.credential.bearer(), "http://untrusted.invalid", 403)):
            status, headers, _ = self.wire(body, token=token, origin=origin)
            require(status == expected and "mcp-session-id" not in headers, "HTTP authority refusal differs")
            rows.append(status)
        return rows

    def rpc(self, method, params):
        deadline = time.monotonic() + 10
        self.identifier += 1
        require(self.identifier <= 128 and not self.session_closed, "Owned HTTP RPC capacity/closure")
        body = json.dumps({"jsonrpc": "2.0", "id": self.identifier, "method": method, "params": params},
                          ensure_ascii=False).encode()
        status, headers, body = self.wire(body, token=self.credential.bearer(), sid=self.sid, deadline=deadline)
        require(status == 200, "Owned HTTP RPC refused")
        if method == "initialize":
            require(self.sid is None and re.fullmatch(r"[0-9a-f]{64}", headers.get("mcp-session-id", "")),
                    "Owned HTTP Session issuance differs")
            self.sid = headers["mcp-session-id"]
        else:
            require(self.sid is not None and "mcp-session-id" not in headers, "Owned HTTP Session changed")
        require(self.credential.bearer().encode() not in body, "HTTP response disclosed credential")
        result = admit_rpc(headers, body, self.identifier)
        remaining(deadline)
        return result

    def tool(self, name, arguments=None):
        require(name in ("open_dashboard", "get_current_config"), "Fixed HTTP dashboard tool required")
        operation = {"open_dashboard": "openDashboard", "get_current_config": "configuration"}[name]
        result = self.rpc("tools/call", {"name": "ci_session", "arguments": {"operation": operation,
                                                                            "args": arguments or {}}})
        require(result.get("isError") is False, "Owned HTTP tool refused")
        value = result.get("structuredContent")
        if value is None:
            value = LIVE.decode(result["content"][0]["text"].encode())
        require(value["status"] == "ok", "Owned HTTP tool incomplete")
        return value["data"]

    def require_peer_isolated(self, peer):
        require(self.sid != peer.sid and self.mcp_port != peer.mcp_port, "Distinct HTTP authorities required")
        body = b'{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}'
        status, headers, _ = self.wire(body, token=self.credential.bearer(), sid=peer.sid)
        require(status == 404 and "mcp-session-id" not in headers, "Foreign HTTP Session admitted")
        return {"role": self.role, "foreignSessionRefused": True, "authenticationRefusals": self.auth_refusals}

    def shutdown(self):
        status, value = self.manage({"kind": "shutdown"})
        data = value["data"] if status == 200 else {}
        require(status == 200 and data.get("shutdownAccepted") is True
                and data.get("scope") == "owning_http_mcp_session" and data.get("serviceShutdown") is False
                and data.get("processExitConfirmed") is False and data.get("connectionCloseConfirmed") is False,
                "Owning HTTP Session shutdown acknowledgement differs")
        deadline = time.monotonic() + 8
        body = b'{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}'
        for _ in range(64):
            status, headers, _ = self.wire(body, token=self.credential.bearer(), sid=self.sid, deadline=deadline)
            require("mcp-session-id" not in headers, "Closing HTTP Session changed")
            if status == 404:
                require(self.process.poll() is None, "Service exit cannot prove HTTP Session isolation")
                self.session_closed = True
                return {"role": self.role, "scope": "owning_http_mcp_session", "revokedSessionStatus": 404,
                        "serviceStillRunning": True, "beforeJobCleanup": True}
            require(status == 409, "Unexpected closing HTTP Session outcome")
            remaining(deadline)
            time.sleep(0.01)
        raise RuntimeError("HTTP Session closure not proved")

    def require_service_exit(self):
        require(self.session_closed, "HTTP Session must close before service exit proof")
        self.process.wait(timeout=remaining(self.service_deadline))
        self.reader.join(timeout=min(3, remaining(self.service_deadline)))
        require(self.process.returncode == 0 and not self.process.stdin.closed and not self.reader.is_alive()
                and self.reader_error is None and self.frames.qsize() == 1
                and self.frames.get_nowait() == {"status": "stopped"}
                and self.frames.empty(), "Owned HTTP service raw exit/EOF not proved")
        elapsed = int((time.monotonic() - self.service_started) * 1000)
        require(SERVICE_MS <= elapsed <= SERVICE_MS + 18000, "Owned HTTP natural service duration differs")
        self.close()
        require(self.credential_removed, "Owned HTTP credential cleanup unproven")
        return {"role": self.role, "nativeExitCode": 0, "stdinWasStillOpen": True,
                "stdoutEOF": True, "naturalHostDurationMs": SERVICE_MS, "observedElapsedMs": elapsed,
                "beforeJobCleanup": True, "logonScopedCredential": True, "ownedCredentialRemoved": True}

    def close(self):
        if self.closed:
            return
        self.closed = True
        failure = None
        operations = []
        if self.process:
            def stop():
                if self.process.poll() is None:
                    self.process.kill()  # Failure teardown only; cannot manufacture service exit proof.
                    self.process.wait(timeout=8)
            operations.append(stop)
        if self.reader:
            def stop_reader():
                self.reader.join(timeout=3)
                require(not self.reader.is_alive(), "Owned HTTP stdout reader did not close")
            operations.append(stop_reader)
        if self.process:
            operations.extend(stream.close for stream in (self.process.stdin, self.process.stdout))
        if self.stderr:
            operations.append(self.stderr.close)
        if self.credential:
            def remove_credential():
                self.credential.close()
                self.credential_removed = True
            operations.append(remove_credential)
        for operation in operations:
            try:
                operation()
            except BaseException as error:
                if failure is None:
                    failure = error
        self.capability, self.sid = None, None
        if failure is not None:
            report_actor_failure("cleanup", self.role, failure)
            raise failure
