"""Pure/mocked HTTP actor adversaries. No socket, credential store, browser or native process."""
from contextlib import contextmanager
import copy
import ctypes
import importlib.util
import io
import json
from pathlib import Path
import queue
import tempfile
import unittest
from unittest import mock


SCRIPTS = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("owned_browser_for_http_tests", SCRIPTS / "host_history_browser.py")
BROWSER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BROWSER)
HTTP = BROWSER.HTTP


def wire(body=b"{}", *, status=200, extra=b"", length=None):
    length = len(body) if length is None else length
    return (f"HTTP/1.1 {status} Owned\r\nContent-Length: {length}\r\nConnection: close\r\n".encode()
            + b"Content-Type: application/json\r\n" + extra + b"\r\n" + body)


class Stream:
    def __init__(self, chunks):
        self.chunks, self.timeouts, self.sent = list(chunks), [], []

    def settimeout(self, value):
        self.timeouts.append(value)

    def recv(self, maximum):
        if not self.chunks:
            return b""
        value = self.chunks.pop(0)
        if len(value) > maximum:
            self.chunks.insert(0, value[maximum:])
        return value[:maximum]

    def sendall(self, value):
        self.sent.append(value)

    def __enter__(self):
        return self

    def __exit__(self, *error):
        pass


class HttpWire(unittest.TestCase):
    def test_fragmented_reply_requires_exact_body_and_real_eof(self):
        body = json.dumps({"owned": "π 🦀 e\u0301\r\n"}, ensure_ascii=False).encode()
        source = wire(body)
        for chunks in ([source], [source[index:index + 1] for index in range(len(source))]):
            stream = Stream(chunks)
            with mock.patch.object(HTTP.time, "monotonic", return_value=0):
                status, headers, observed = HTTP.read_reply(stream, 1)
            self.assertEqual((status, observed), (200, body))
            self.assertEqual(headers["content-length"], str(len(body)))
            self.assertTrue(stream.timeouts and all(value == 1 for value in stream.timeouts))
        for source in (wire(body)[:-1], wire(body) + b"unexpected", b"", wire(body, length=len(body) + 1)):
            with self.subTest(source=source[:70]), mock.patch.object(HTTP.time, "monotonic", return_value=0), \
                 self.assertRaises(RuntimeError):
                HTTP.read_reply(Stream([source]), 1)

    def test_duplicate_ambiguous_and_oversized_framing_refuses(self):
        cases = [wire(extra=b"content-length: 2\r\n"), wire(extra=b"Transfer-Encoding: chunked\r\n"),
                 wire(extra=b"Trailer: x\r\n"), wire(extra=b"X-Owned: private\x01value\r\n"),
                 wire(length=HTTP.MAX_JSON + 1), wire().replace(b"Connection: close", b"Connection: keep-alive"),
                 wire().replace(b"Content-Length: 2", b"Content-Length: +2"),
                 wire().replace(b"Content-Length: 2", b"Content-Length: 02"),
                 wire().replace(b"HTTP/1.1 200 Owned", b"HTTP/2 200 Owned"),
                 wire(extra=b"".join(b"X-" + str(i).encode() + b": value\r\n" for i in range(49))),
                 wire(extra=b"".join(b"X-" + str(i).encode() + b": " + b"x" * 3900 + b"\r\n" for i in range(5)))]
        for source in cases:
            with self.subTest(size=len(source)), mock.patch.object(HTTP.time, "monotonic", return_value=0), \
                 self.assertRaises(RuntimeError):
                HTTP.read_reply(Stream([source]), 1)

    def test_continuous_bytes_cannot_renew_the_absolute_http_deadline(self):
        source = wire(b"long response")
        stream = Stream([bytes([value]) for value in source])
        with mock.patch.object(HTTP.time, "monotonic", side_effect=(index / 10 for index in range(30))):
            with self.assertRaisesRegex(RuntimeError, "absolute deadline"):
                HTTP.read_reply(stream, 1)
        self.assertGreater(len(stream.chunks), 0)
        self.assertEqual(stream.timeouts, sorted(stream.timeouts, reverse=True))
        self.assertLess(len(stream.timeouts), 10)

    def test_request_is_fixed_loopback_and_never_redirects_or_accepts_header_input(self):
        stream = Stream([wire(status=302, extra=b"Location: https://untrusted.invalid/\r\n")])
        with mock.patch.object(HTTP.socket, "create_connection", return_value=stream) as connect, \
             mock.patch.object(HTTP.time, "monotonic", return_value=0):
            status, _, _ = HTTP.request(1234, b"{}", "a" * 64, "b" * 64)
        self.assertEqual(status, 302)
        connect.assert_called_once_with(("127.0.0.1", 1234), timeout=10)
        request = stream.sent[0]
        self.assertIn(b"POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:1234\r\n", request)
        self.assertIn(b"Mcp-Protocol-Version: 2025-11-25\r\n", request)
        self.assertIn(b"Mcp-Session-Id: " + b"b" * 64, request)
        for token, session, origin in (("secret\r\nInjected: yes", None, None),
                                        ("a" * 64, "foreign", None), ([], None, None),
                                        ("a" * 64, None, "https://foreign.invalid")):
            with mock.patch.object(HTTP.socket, "create_connection") as connect, self.assertRaises(RuntimeError):
                HTTP.request(1234, b"{}", token, session, origin=origin)
            connect.assert_not_called()

    def test_json_and_finite_sse_preserve_unicode_and_exact_response_identity(self):
        value = {"jsonrpc": "2.0", "id": 3, "result": {"owned": "π\r\n🦀"}}
        raw = json.dumps(value, ensure_ascii=False).encode()
        self.assertEqual(HTTP.admit_rpc({"content-type": "application/json"}, raw, 3), value["result"])
        notification = b'event: message\ndata: {"jsonrpc":"2.0","method":"notifications/tools/list_changed"}\n\n'
        response = b"event: message\ndata: " + raw + b"\n\n"
        self.assertEqual(HTTP.admit_rpc({"content-type": "text/event-stream"}, notification * 32 + response, 3),
                         value["result"])
        for kind, body in (("text/event-stream", notification * 33 + response),
                           ("text/event-stream", response[:-1]), ("text/event-stream", response + response),
                           ("text/event-stream", notification.replace(b"tools/list_changed", b"foreign") + response),
                           ("text/event-stream", b"data: " + raw + b"\n\n"),
                           ("application/json", raw.replace(b'"id": 3', b'"id": 4')),
                           ("application/json", raw.replace(b'"id": 3', b'"id": true')),
                           ("application/json", raw.replace(b'"result":', b'"error":')),
                           ("application/json", b'{"jsonrpc":"2.0","id":3,"id":3,"result":{}}'),
                           ("application/json", b" " * (HTTP.MAX_JSON + 1)), ("text/html", raw)):
            with self.subTest(kind=kind, size=len(body)), self.assertRaises((RuntimeError, ValueError)):
                HTTP.admit_rpc({"content-type": kind}, body, 3)


class CredentialApi:
    def __init__(self):
        self.storage, self.events = {}, []

    @contextmanager
    def locked(self, target):
        self.events.append("lock")
        try:
            yield
        finally:
            self.events.append("unlock")

    def read(self, target):
        self.events.append("read")
        value = self.storage.get(target)
        return None if value is None else bytearray(value)

    def write(self, target, value):
        self.events.append("write")
        self.storage[target] = bytes(value)

    def delete(self, target):
        self.events.append("delete")
        del self.storage[target]


class CredentialOwnership(unittest.TestCase):
    def test_os_read_copy_is_bounded_wiped_and_freed_and_write_is_logon_only(self):
        api = HTTP.WindowsCredentialApi.__new__(HTTP.WindowsCredentialApi)
        api.advapi = mock.Mock()
        target = HTTP.PREFIX + "owned-history-browser-" + "a" * 48
        token = bytearray(b"x" * 32)
        native_copy = bytearray(token)
        blob = (ctypes.c_ubyte * 32).from_buffer(native_copy)
        value = HTTP.Credential(flags=0, kind=1, target=target, persist=1, blob=blob, blob_size=32)
        def read(name, kind, flags, pointer):
            self.assertEqual((name, kind, flags), (target, 1, 0))
            ctypes.cast(pointer, ctypes.POINTER(ctypes.POINTER(HTTP.Credential)))[0] = ctypes.pointer(value)
            return 1
        api.advapi.CredReadW.side_effect = read
        self.assertEqual(api.read(target), token)
        self.assertEqual(native_copy, bytes(32))
        api.advapi.CredFree.assert_called_once()
        def write(pointer, flags):
            written = ctypes.cast(pointer, ctypes.POINTER(HTTP.Credential)).contents
            self.assertEqual(flags, 0)
            self.assertEqual((written.target, written.persist, written.kind, written.blob_size), (target, 1, 1, 32))
            self.assertEqual(ctypes.string_at(written.blob, 32), token)
            return 1
        api.advapi.CredWriteW.side_effect = write
        api.write(target, token)
        self.assertEqual(token, b"x" * 32)

    def test_create_readback_delete_absence_and_zeroing_are_required(self):
        api = CredentialApi()
        credential = HTTP.SessionCredential(api)
        self.assertRegex(credential.name, r"^owned-history-browser-[0-9a-f]{48}$")
        self.assertLessEqual(len(credential.name), 80)
        self.assertEqual(credential.target, HTTP.PREFIX + credential.name)
        self.assertEqual(len(credential.bearer()), 64)
        self.assertEqual(api.events, ["lock", "read", "write", "read", "unlock"])
        credential.close()
        self.assertTrue(credential.removed)
        self.assertEqual(api.storage, {})
        self.assertEqual(credential.token, bytes(32))
        self.assertEqual(api.events[-5:], ["lock", "read", "delete", "read", "unlock"])
        credential.close()
        self.assertEqual(api.events.count("delete"), 1)
        with self.assertRaises(RuntimeError):
            credential.bearer()

    def test_existing_and_replaced_credential_occupants_are_preserved(self):
        api = CredentialApi()
        target = HTTP.PREFIX + "owned-history-browser-" + "61" * 24
        api.storage[target] = b"foreign bytes kept" * 2
        with mock.patch.object(HTTP.os, "urandom", side_effect=[b"a" * 24, b"b" * 32]), self.assertRaises(RuntimeError):
            HTTP.SessionCredential(api)
        self.assertEqual(api.storage[target], b"foreign bytes kept" * 2)
        self.assertNotIn("write", api.events)
        self.assertNotIn("delete", api.events)
        credential = HTTP.SessionCredential(CredentialApi())
        credential.api.storage[credential.target] = b"foreign replacement preserved".ljust(32, b"!")
        with self.assertRaises(RuntimeError):
            credential.close()
        self.assertNotIn("delete", credential.api.events)
        self.assertFalse(credential.removed)
        self.assertEqual(credential.token, bytes(32))
        with self.assertRaises(RuntimeError):
            credential.close()

    def test_read_write_delete_and_absence_failures_cannot_create_cleanup_proof(self):
        for failure in ("read", "write"):
            api = CredentialApi()
            with mock.patch.object(api, failure, side_effect=OSError("owned unavailable")), self.assertRaises(OSError):
                HTTP.SessionCredential(api)
            self.assertNotIn("delete", api.events)
        for failure in ("delete", "absence"):
            api = CredentialApi()
            credential = HTTP.SessionCredential(api)
            with mock.patch.object(api, "delete", side_effect=OSError("owned unavailable") if failure == "delete" else None), \
                 self.assertRaises((OSError, RuntimeError)):
                credential.close()
            self.assertFalse(credential.removed)
            self.assertEqual(credential.token, bytes(32))
            with self.assertRaises(RuntimeError):
                credential.close()

    def test_native_credential_layout_and_logon_scope_are_checked_before_copy(self):
        self.assertEqual(ctypes.sizeof(HTTP.Credential), 80)
        target = HTTP.PREFIX + "owned-history-browser-" + "a" * 48
        token = bytearray(b"x" * 32)
        buffer = (ctypes.c_ubyte * 32).from_buffer(token)
        value = HTTP.Credential(flags=0, kind=1, target=target, persist=1, blob=buffer, blob_size=32)
        self.assertEqual(HTTP.credential_value(value, target), token)
        for key, changed in (("flags", 1), ("kind", 2), ("persist", 2), ("attribute_count", 1),
                             ("blob_size", 31), ("target", target + "-foreign")):
            original = getattr(value, key)
            setattr(value, key, changed)
            with self.subTest(key=key), self.assertRaises(RuntimeError):
                HTTP.credential_value(value, target)
            setattr(value, key, original)

    def test_native_creation_mutex_releases_and_closes_even_after_body_failure(self):
        api = HTTP.WindowsCredentialApi.__new__(HTTP.WindowsCredentialApi)
        api.kernel = mock.Mock()
        api.kernel.CreateMutexW.return_value = 123
        api.kernel.WaitForSingleObject.return_value = 0
        api.kernel.ReleaseMutex.return_value = 1
        api.kernel.CloseHandle.return_value = 1
        target = HTTP.PREFIX + "owned-history-browser-" + "a" * 48
        with self.assertRaisesRegex(RuntimeError, "owned body"):
            with api.locked(target):
                raise RuntimeError("owned body")
        name = "Global\\YokeServiceCreate-" + HTTP.hashlib.sha256(target.lower().encode("ascii")).hexdigest()
        api.kernel.CreateMutexW.assert_called_once_with(None, False, name)
        api.kernel.ReleaseMutex.assert_called_once_with(123)
        api.kernel.CloseHandle.assert_called_once_with(123)
        api.kernel.reset_mock()
        api.kernel.WaitForSingleObject.return_value = 258
        with self.assertRaises(RuntimeError):
            with api.locked(target):
                self.fail("Timed-out ownership must not enter")
        api.kernel.ReleaseMutex.assert_not_called()
        api.kernel.CloseHandle.assert_called_once_with(123)


class HttpOwnership(unittest.TestCase):
    def test_failure_diagnostics_use_only_closed_stages_roles_and_tolerate_a_failed_sink(self):
        secret = "https://private.invalid/#private-capability"
        for stage, role, expected in (("credential-create", "old", "credential-create; role=old"),
                                      (secret, "current", "other; role=current"),
                                      ("cleanup", {"private": secret}, "cleanup; role=other")):
            with mock.patch.object(HTTP.sys, "stderr", new_callable=io.StringIO) as output:
                HTTP.report_actor_failure(stage, role)
                self.assertEqual(output.getvalue(), "Owned browser HTTP actor failed: stage=" + expected + "\n")
                self.assertNotIn(secret, output.getvalue())
        for error in (OSError("private sink unavailable"), ValueError("private sink closed"),
                      RuntimeError("private sink failed")):
            with mock.patch.object(HTTP.sys, "stderr") as output:
                output.write.side_effect = error
                HTTP.report_actor_failure("cleanup", "current")
        with mock.patch.object(HTTP.sys, "stderr", new_callable=io.StringIO) as output:
            HTTP.report_actor_failure("credential-create", "old", HTTP.CredentialApiFailure("write", 1312))
            self.assertEqual(output.getvalue(), "Owned browser HTTP actor failed: stage=credential-create; role=old; credential-api=write; code=1312\n")
        for operation, code in ((secret, 1), ("write", secret), ("read", -1), ("delete", 0x100000000)):
            with self.assertRaises(RuntimeError):
                HTTP.CredentialApiFailure(operation, code)
        error = HTTP.CredentialApiFailure("write", 1)
        error.operation = secret
        with mock.patch.object(HTTP.sys, "stderr", new_callable=io.StringIO) as output:
            HTTP.report_actor_failure("credential-create", "old", error)
            self.assertNotIn(secret, output.getvalue())
            self.assertIn("credential-api=other; code=unavailable", output.getvalue())

    def test_wrapper_selects_http_actor_only_with_the_specific_job_and_no_python_authority(self):
        with mock.patch.object(HTTP, "HttpActor") as actor:
            BROWSER.owning_actor(1, 2, 3, 4, "browser-old", "http", None, "specific-job")
            actor.assert_called_once_with(1, 2, 3, 4, "browser-old", "specific-job")
        for python, job in ((None, None), (Path("unselected-python"), "specific-job")):
            with mock.patch.object(HTTP, "HttpActor") as actor, self.assertRaises(RuntimeError):
                BROWSER.owning_actor(1, 2, 3, 4, "browser-old", "http", python, job)
            actor.assert_not_called()

    def test_http_result_requires_both_profiles_authorities_sessions_services_and_credential_cleanup(self):
        profile = {"protocol": "session", "toolCount": 1, "sessionDashboardToolObserved": True,
                   "serenaDashboardToolsAbsent": True}
        authority = {"foreignSessionRefused": True, "authenticationRefusals": [401, 401, 403]}
        session = {"scope": "owning_http_mcp_session", "revokedSessionStatus": 404,
                   "serviceStillRunning": True, "beforeJobCleanup": True}
        service = {"nativeExitCode": 0, "stdinWasStillOpen": True, "stdoutEOF": True,
                   "naturalHostDurationMs": 120000, "observedElapsedMs": 120100,
                   "beforeJobCleanup": True, "logonScopedCredential": True, "ownedCredentialRemoved": True}
        valid = {"automationComplete": True, "variant": "http", "owningProtocol": "session", "owningTransport": "http",
                 "browserStart": {"variant": "http"},
                 "profileObservations": [{**profile, "role": role} for role in ("old", "current")],
                 "httpAuthorities": [{**authority, "role": role} for role in ("old", "current")],
                 "httpSessionExits": [{**session, "role": role} for role in ("current", "old")],
                 "httpServiceExits": [{**service, "role": role} for role in ("current", "old")],
                 "persistentRuntimes": [], "nativeExits": [], "peerHttpSessionSurvived": True}
        BROWSER.admit_variant_result(valid, "http")
        changes = [("profileObservations", "protocol", "native"), ("profileObservations", "toolCount", True),
                   ("profileObservations", "sessionDashboardToolObserved", False),
                   ("httpAuthorities", "authenticationRefusals", [200, 401, 403]),
                   ("httpAuthorities", "foreignSessionRefused", False),
                   ("httpSessionExits", "scope", "owning_stdio_mcp_connection"),
                   ("httpSessionExits", "revokedSessionStatus", 200),
                   ("httpSessionExits", "serviceStillRunning", False),
                   ("httpServiceExits", "nativeExitCode", False),
                   ("httpServiceExits", "nativeExitCode", 1), ("httpServiceExits", "stdoutEOF", False),
                   ("httpServiceExits", "observedElapsedMs", 119999),
                   ("httpServiceExits", "naturalHostDurationMs", 1),
                   ("httpServiceExits", "beforeJobCleanup", False),
                   ("httpServiceExits", "logonScopedCredential", False),
                   ("httpServiceExits", "ownedCredentialRemoved", False)]
        for section, field, value in changes:
            changed = copy.deepcopy(valid)
            changed[section][1][field] = value
            with self.subTest(section=section, field=field, value=value), self.assertRaises(RuntimeError):
                BROWSER.admit_variant_result(changed, "http")
        for changes in ({"peerHttpSessionSurvived": False}, {"owningTransport": "stdio"},
                        {"httpServiceExits": []}, {"httpSessionExits": []},
                        {"nativeExits": [{"nativeExitCode": 0}, {"nativeExitCode": 0}]}):
            with self.subTest(changes=changes), self.assertRaises(RuntimeError):
                BROWSER.admit_variant_result({**valid, **changes}, "http")
        actor = mock.Mock(protocol="session")
        actor.rpc.return_value = {"tools": [{"name": "ci_session"}]}
        self.assertTrue(BROWSER.profile_observation(actor, "http", "current")["sessionDashboardToolObserved"])
        actor.rpc.return_value["tools"].append({"name": "open_dashboard"})
        with self.assertRaises(RuntimeError):
            BROWSER.profile_observation(actor, "http", "current")

    def actor(self):
        actor = HTTP.HttpActor.__new__(HTTP.HttpActor)
        actor.protocol, actor.role = "session", "current"
        actor.identifier, actor.sid, actor.mcp_port = 2, "b" * 64, 1234
        actor.port, actor.capability = 2345, "c" * 64
        actor.session_closed, actor.closed, actor.credential_removed = False, False, False
        actor.credential = mock.Mock()
        actor.credential.bearer.return_value = "a" * 64
        actor.process = mock.Mock(returncode=0)
        actor.process.stdin.closed = False
        actor.process.poll.return_value = None
        actor.reader, actor.stderr = mock.Mock(), mock.Mock()
        actor.reader.is_alive.return_value = False
        actor.reader_error = None
        actor.frames = queue.Queue(maxsize=8)
        actor.frames.put({"status": "stopped"})
        actor.service_started, actor.service_deadline = 0, 128
        return actor

    def ack(self):
        return 200, {"data": {"shutdownAccepted": True, "scope": "owning_http_mcp_session",
                             "serviceShutdown": False, "processExitConfirmed": False,
                             "connectionCloseConfirmed": False}}

    def test_job_and_windows_ci_authority_precede_credentials_files_or_native_launch(self):
        profile = Path("old-host.json")
        with mock.patch.object(HTTP.LIVE, "require_worker_job", side_effect=RuntimeError("Job absent")), \
             mock.patch.object(HTTP, "require_windows_ci") as ci, \
             mock.patch.object(HTTP, "WindowsCredentialApi") as api, \
             mock.patch.object(HTTP.subprocess, "Popen") as process:
            with self.assertRaisesRegex(RuntimeError, "Job absent"):
                HTTP.HttpActor(None, None, profile, None, "browser-old", "specific-job")
            ci.assert_not_called()
            api.assert_not_called()
            process.assert_not_called()
        with mock.patch.object(HTTP.LIVE, "require_worker_job"), \
             mock.patch.dict(HTTP.os.environ, {"GITHUB_ACTIONS": "false"}), \
             mock.patch.object(HTTP, "WindowsCredentialApi") as api:
            with self.assertRaises(RuntimeError):
                HTTP.HttpActor(None, None, profile, None, "browser-old", "specific-job")
            api.assert_not_called()
        credential = mock.Mock()
        with mock.patch.object(HTTP.LIVE, "require_worker_job"), mock.patch.object(HTTP, "require_windows_ci"), \
             mock.patch.object(HTTP, "WindowsCredentialApi"), \
             mock.patch.object(HTTP, "SessionCredential", return_value=credential), \
             mock.patch.object(HTTP.LIVE, "read_json", return_value={}), \
             mock.patch.object(HTTP.LIVE, "write_json", side_effect=RuntimeError("Owned fixture unavailable")), \
             mock.patch.object(HTTP, "report_actor_failure", side_effect=RuntimeError("Owned observer unavailable")), \
             mock.patch.object(HTTP.subprocess, "Popen") as process:
            with self.assertRaisesRegex(RuntimeError, "Owned observer unavailable"):
                HTTP.HttpActor(None, Path("inert-source"), profile, None, "browser-old", "specific-job")
            credential.close.assert_called_once()
            process.assert_not_called()

    def test_constructor_uses_public_cli_fixed_profile_and_reference_without_inline_secret(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            profile = root / "old-host.json"
            HTTP.LIVE.write_json(profile, {"context": "desktop-app", "dashboard": {"management": {"shutdown": True}}})
            frames = queue.Queue(maxsize=8)
            frames.put({"status": "listening", "transport": "mcp_streamable_http_json", "address": "127.0.0.1:1234",
                        "endpoint": "/mcp", "grantAuthority": "explicit_host_profile",
                        "secrets": "os_credential_store_and_ram_only"})
            process, thread, credential = mock.Mock(), mock.Mock(), mock.Mock()
            process.poll.return_value = None
            thread.is_alive.return_value = False
            credential.name = "owned-history-browser-" + "d" * 48
            credential.bearer.return_value = "1f" * 32
            initialized = {"jsonrpc": "2.0", "id": 1, "result": {"protocolVersion": HTTP.VERSION,
                            "serverInfo": {"version": "0.5.0"}}}
            opened = {"jsonrpc": "2.0", "id": 2, "result": {"isError": False,
                       "structuredContent": {"status": "ok", "data": {"browserOpened": False,
                                           "url": "http://127.0.0.1:2345/#" + "c" * 64}}}}
            responses = [(401, {}, b""), (401, {}, b""), (403, {}, b""),
                         (200, {"content-type": "application/json", "mcp-session-id": "b" * 64}, json.dumps(initialized).encode()),
                         (200, {"content-type": "application/json"}, json.dumps(opened).encode())]
            with mock.patch.object(HTTP.LIVE, "require_worker_job"), mock.patch.object(HTTP, "require_windows_ci"), \
                 mock.patch.object(HTTP, "WindowsCredentialApi"), \
                 mock.patch.object(HTTP, "SessionCredential", return_value=credential), \
                 mock.patch.object(HTTP.subprocess, "CREATE_NO_WINDOW", 0x08000000, create=True), \
                 mock.patch.object(HTTP.subprocess, "Popen", return_value=process) as launch, \
                 mock.patch.object(HTTP.threading, "Thread", return_value=thread), \
                 mock.patch.object(HTTP.queue, "Queue", return_value=frames), \
                 mock.patch.object(HTTP.HttpActor, "wire", side_effect=responses), \
                 mock.patch.object(HTTP.HttpActor, "state", return_value={"view": {"management": {"sessionStateSha256": "f" * 64}}}):
                actor = HTTP.HttpActor(Path("inert-native"), root, profile, root, "browser-old", "specific-job")
                arguments = launch.call_args.args[0]
                self.assertIn("serve-http", arguments)
                self.assertIn("--trust-service", arguments)
                self.assertEqual(arguments[arguments.index("--duration-ms") + 1], "120000")
                for forbidden in ("--trust-python", "--allow-writes", "--allow-remote-bind"):
                    self.assertNotIn(forbidden, arguments)
                config = HTTP.LIVE.read_json(root / "browser-old-http-host.json")
                self.assertEqual(config["bind"], "127.0.0.1:0")
                principal = config["principals"][0]
                self.assertEqual(principal["profile"], "session")
                self.assertEqual(principal["root"], str(root))
                self.assertEqual(principal["tokenCredentialName"], credential.name)
                self.assertNotIn(credential.bearer(), json.dumps(config))
                self.assertNotIn(credential.bearer(), " ".join(arguments))
                self.assertEqual(actor.auth_refusals, [401, 401, 403])
                actor.close()
                credential.close.assert_called_once()
                process.kill.assert_called_once()

    def test_rpc_and_peer_checks_cannot_follow_redirect_or_change_session_identity(self):
        actor = self.actor()
        for status, headers, body in ((302, {}, b""), (200, {"mcp-session-id": "d" * 64}, b"{}"),
                                      (200, {"content-type": "application/json"}, b'{"jsonrpc":"2.0","id":3,"result":{"token":"' + b"a" * 64 + b'"}}')):
            with mock.patch.object(actor, "wire", return_value=(status, headers, body)) as request, self.assertRaises(RuntimeError):
                actor.rpc("tools/list", {})
            request.assert_called_once()
        actor = self.actor()
        with mock.patch.object(actor, "wire", return_value=(200, {"content-type": "application/json"},
                b'{"jsonrpc":"2.0","id":3,"result":{}}')), \
             mock.patch.object(HTTP.time, "monotonic", side_effect=[0, 11]), self.assertRaisesRegex(RuntimeError, "absolute deadline"):
            actor.rpc("tools/list", {})
        peer = self.actor()
        peer.sid, peer.mcp_port = "d" * 64, 3456
        actor.auth_refusals = [401, 401, 403]
        with mock.patch.object(actor, "wire", return_value=(404, {}, b"")) as request:
            self.assertTrue(actor.require_peer_isolated(peer)["foreignSessionRefused"])
            self.assertEqual(request.call_args.kwargs["sid"], peer.sid)
            self.assertEqual(request.call_args.kwargs["token"], actor.credential.bearer())
        for status in (200, 401, 409, 500):
            with mock.patch.object(actor, "wire", return_value=(status, {}, b"")), self.assertRaises(RuntimeError):
                actor.require_peer_isolated(peer)

    def test_dashboard_shutdown_proves_session_revocation_while_service_is_still_alive(self):
        actor = self.actor()
        with mock.patch.object(actor, "manage", return_value=self.ack()), \
             mock.patch.object(actor, "wire", side_effect=[(409, {}, b""), (404, {}, b"")]) as request, \
             mock.patch.object(HTTP.time, "monotonic", return_value=0), mock.patch.object(HTTP.time, "sleep"):
            proof = actor.shutdown()
        self.assertTrue(proof["serviceStillRunning"])
        self.assertTrue(actor.session_closed)
        self.assertEqual([row.kwargs["deadline"] for row in request.call_args_list], [8, 8])
        actor.process.kill.assert_not_called()
        actor.credential.close.assert_not_called()
        for field, value in (("scope", "owning_stdio_mcp_connection"), ("serviceShutdown", True),
                             ("processExitConfirmed", True), ("connectionCloseConfirmed", True)):
            actor = self.actor()
            status, reply = self.ack()
            reply["data"][field] = value
            with mock.patch.object(actor, "manage", return_value=(status, reply)), \
                 mock.patch.object(actor, "wire") as request, self.assertRaises(RuntimeError):
                actor.shutdown()
            request.assert_not_called()

    def test_session_close_cannot_borrow_service_exit_or_retry_forever(self):
        for mode in ("root-exited", "still-closing", "unexpected-success", "changed-session"):
            actor = self.actor()
            actor.process.poll.return_value = 0 if mode == "root-exited" else None
            status = 409 if mode == "still-closing" else 200 if mode == "unexpected-success" else 404
            headers = {"mcp-session-id": "d" * 64} if mode == "changed-session" else {}
            with mock.patch.object(actor, "manage", return_value=self.ack()), \
                 mock.patch.object(actor, "wire", return_value=(status, headers, b"")) as request, \
                 mock.patch.object(HTTP.time, "monotonic", return_value=0), mock.patch.object(HTTP.time, "sleep"), \
                 self.assertRaises(RuntimeError):
                actor.shutdown()
            self.assertFalse(actor.session_closed)
            self.assertLessEqual(request.call_count, 64)

    def test_natural_service_exit_requires_raw_status_eof_duration_and_credential_absence(self):
        actor = self.actor()
        actor.session_closed = True
        actor.process.poll.return_value = 0
        with mock.patch.object(HTTP.time, "monotonic", return_value=120.1):
            proof = actor.require_service_exit()
        self.assertEqual(proof["naturalHostDurationMs"], 120000)
        self.assertTrue(proof["ownedCredentialRemoved"])
        self.assertTrue(proof["stdoutEOF"])
        actor.process.kill.assert_not_called()
        actor.credential.close.assert_called_once()
        actor.close()
        actor.credential.close.assert_called_once()
        for mode in ("session-open", "bad-exit", "stdin-closed", "reader-live", "reader-error", "missing-stopped",
                     "extra-output", "too-early", "credential-cleanup"):
            actor = self.actor()
            actor.session_closed = mode != "session-open"
            actor.process.poll.return_value = 0
            actor.process.returncode = 1 if mode == "bad-exit" else 0
            actor.process.stdin.closed = mode == "stdin-closed"
            actor.reader.is_alive.return_value = mode == "reader-live"
            actor.reader_error = "owned-failure" if mode == "reader-error" else None
            if mode == "missing-stopped":
                actor.frames.get_nowait()
            if mode == "extra-output":
                actor.frames.put({"unexpected": "private"})
            if mode == "credential-cleanup":
                actor.credential.close.side_effect = RuntimeError("Owned credential cleanup refused")
            with self.subTest(mode=mode), mock.patch.object(HTTP.time, "monotonic", return_value=119.9 if mode == "too-early" else 120.1), \
                 self.assertRaises(RuntimeError):
                actor.require_service_exit()

    def test_failure_cleanup_attempts_owned_credential_even_after_process_or_reader_failure(self):
        actor = self.actor()
        actor.process.wait.side_effect = RuntimeError("Owned root exit unavailable")
        actor.reader.is_alive.return_value = True
        with self.assertRaisesRegex(RuntimeError, "Owned root exit unavailable"):
            actor.close()
        actor.credential.close.assert_called_once()
        actor.process.stdin.close.assert_called_once()
        actor.process.stdout.close.assert_called_once()
        actor.stderr.close.assert_called_once()
        self.assertIsNone(actor.sid)
        self.assertIsNone(actor.capability)
        actor.close()
        actor.process.wait.assert_called_once_with(timeout=8)


if __name__ == "__main__":
    unittest.main()
