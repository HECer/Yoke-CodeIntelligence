"""Pure/mocked adversaries; no browser, native image, worker, Job, or network.

Authored source-only. Running these later does not establish live acceptance.
"""
import ast
import copy
from contextlib import ExitStack
import importlib.util
import io
import json
from pathlib import Path
import queue
import subprocess
import tempfile
import unittest
from unittest import mock


SCRIPTS = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("owned_browser", SCRIPTS / "host_history_browser.py")
BROWSER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BROWSER)


class WorkerAdmission(unittest.TestCase):
    def test_driver_failure_checkpoints_are_closed_and_never_admit_success(self):
        original = {"phase": "pending", "ok": False, "reason": "real-browser-failed",
                    "checkpoint": "pending-snapshot", "viewport": "mobile"}
        secret = "http://private.invalid/#private-capability"
        for change, expected in (({}, "checkpoint=pending-snapshot; viewport=mobile"),
                                 ({"checkpoint": secret}, "checkpoint=other; viewport=mobile"),
                                 ({"viewport": {"private": secret}}, "checkpoint=pending-snapshot; viewport=other"),
                                 ({"phase": secret}, "checkpoint=other; viewport=other"),
                                 ({"extra": secret}, "checkpoint=other; viewport=other")):
            with self.subTest(change=change), mock.patch.object(BROWSER.sys, "stderr", new_callable=io.StringIO) as log:
                value = {**original, **change}
                BROWSER.report_driver_failure(value, "pending")
                self.assertEqual(log.getvalue(), "Owned browser driver failed: " + expected + "\n")
                self.assertNotIn(secret, log.getvalue())
                with self.assertRaises(RuntimeError):
                    BROWSER.admit_driver_reply(value, "pending", "private-capability")
        for error in (OSError("private unavailable sink"), ValueError("private closed sink")):
            with mock.patch.object(BROWSER.sys, "stderr") as log:
                log.write.side_effect = error
                BROWSER.report_driver_failure(original, "pending")
                with self.assertRaises(RuntimeError):
                    BROWSER.admit_driver_reply(original, "pending", "private-capability")
        with mock.patch.object(BROWSER.sys, "stderr", new_callable=io.StringIO) as log:
            BROWSER.report_driver_failure({"phase": "pending", "ok": True}, "pending")
            self.assertEqual(log.getvalue(), "")

    def test_exercise_failure_reports_only_closed_labels_and_tolerates_a_closed_sink(self):
        for phase, expected in (("browser-before", "browser-before"),
                                ("before-integrity", "before-integrity"),
                                ("history-selection", "history-selection"),
                                ("https://private.invalid/#private-capability", "other"),
                                ({"private": "capability"}, "other"), (None, "other")):
            with self.subTest(phase=phase), mock.patch.object(BROWSER.sys, "stderr", new_callable=io.StringIO) as log:
                BROWSER.report_exercise_failure(phase)
                self.assertEqual(log.getvalue(), "Owned browser exercise failed: " + expected + "\n")
        for error in (OSError("private unavailable sink"), ValueError("private closed sink")):
            with mock.patch.object(BROWSER.sys, "stderr") as log:
                log.write.side_effect = error
                BROWSER.report_exercise_failure("browser-before")

    def test_before_review_failures_preserve_refusal_owned_bytes_and_teardown(self):
        # Mock actors/CLI/browser only. This exercises refusal and diagnosis;
        # it cannot provide live browser, worker or native-image acceptance.
        for mode in ("driver-error", "tree-mutation", "unavailable-sink"):
            with self.subTest(mode=mode), tempfile.TemporaryDirectory(prefix="owned-browser-stage-") as directory:
                fixture = Path(directory).resolve()
                target = fixture / "host/settings-\u03c0.yaml"
                owner = fixture / ("host/.yoke-doc-owner-" + "b" * 64 + ".json")
                foreign = fixture / "new/foreign-kept.json"
                closed, commands = [], []
                failure = RuntimeError("private endpoint and capability")

                class Actor:
                    def __init__(self, native, source, profile, output, label):
                        self.label = label

                    def plan(self, document, content):
                        return {"id": "a" * 64, "content": content}

                    def apply(self, document, plan):
                        target.write_bytes(plan["content"].encode())

                    def close(self):
                        closed.append(self.label)

                class Browser:
                    def __init__(self, *arguments):
                        pass

                    def call(self, request):
                        if mode != "tree-mutation":
                            raise failure
                        foreign.write_bytes(b"retained owned mutation witness")
                        return {"lastPlanId": "a" * 64}

                    def close(self):
                        closed.append("browser")

                def cli(command, **options):
                    if "binding-plan" in command:
                        commands.append("binding-plan")
                        result = {"target": "b" * 64, "fromAccess": "c" * 64, "nonce": "d" * 64}
                    elif "binding-apply" in command:
                        commands.append("binding-apply")
                        owner.write_bytes(b"retained owner")
                        result = {"outcome": "committed", "fileChanged": False}
                    else:
                        raise AssertionError("No later CLI operation may run after refusal")
                    options["stdout"].write(json.dumps(result).encode())
                    return subprocess.CompletedProcess(command, 0)

                with mock.patch.object(BROWSER.LIVE, "require_worker_job"), \
                     mock.patch.object(BROWSER.subprocess, "CREATE_NO_WINDOW", 0x08000000, create=True), \
                     mock.patch.object(BROWSER.LIVE, "Actor", Actor), \
                     mock.patch.object(BROWSER, "Browser", Browser), \
                     mock.patch.object(BROWSER.subprocess, "run", side_effect=cli), \
                     mock.patch.object(BROWSER.sys, "stderr", new_callable=io.StringIO) as log:
                    if mode == "unavailable-sink":
                        log.write = mock.Mock(side_effect=OSError("private sink failure"))
                    with self.assertRaises(RuntimeError) as caught:
                        BROWSER.exercise(fixture / "unused-native", fixture / "unused-node",
                                         fixture / "unused-browser", fixture, fixture / "evidence",
                                         "unused-job", {"native/src/session_dashboard.html": "e" * 64})
                    if mode == "tree-mutation":
                        self.assertEqual(str(caught.exception), "Browser draft/review changed owned file state")
                        self.assertEqual(foreign.read_bytes(), b"retained owned mutation witness")
                        self.assertEqual(log.getvalue(), "Owned browser exercise failed: before-integrity\n")
                    else:
                        self.assertIs(caught.exception, failure)
                        if mode != "unavailable-sink":
                            self.assertEqual(log.getvalue(), "Owned browser exercise failed: browser-before\n")
                self.assertEqual(closed, ["browser", "browser-current", "browser-old"])
                self.assertEqual(commands, ["binding-plan", "binding-apply"])
                self.assertEqual(owner.read_bytes(), b"retained owner")
                self.assertEqual(target.read_bytes(),
                                 "\ufeff# owned global \u03c0\r\nvalue: preserved\r\n# original saved history 1 \U0001f980\r\n".encode())

    def test_link_diagnostic_emits_closed_labels_without_private_paths(self):
        root = Path(tempfile.gettempdir()).resolve() / "yoke-owned-history-browser-private"
        error = BROWSER.LIVE.EvidenceLinkRefusal(root, root / "private-capability" / "private-target", "junction")
        with mock.patch.object(BROWSER.LIVE, "output_size", side_effect=error), \
             mock.patch.object(BROWSER.sys, "stderr", new_callable=io.StringIO) as log:
            with self.assertRaises(BROWSER.LIVE.EvidenceLinkRefusal) as caught:
                BROWSER.output_size(root)
            self.assertIs(caught.exception, error)
            self.assertEqual(log.getvalue(), "Owned browser output scan failed: fixture link\n"
                             "Owned browser refused link: area=other; entry=other; kind=junction; depth=2\n"
                             "Owned browser link route: other/other; componentSha256="
                             + ",".join(error.component_sha256) + "\n")
            self.assertNotIn("private-capability", log.getvalue())
            self.assertNotIn("private-target", log.getvalue())
            self.assertNotIn(str(root), log.getvalue())

    def test_private_profile_paths_share_one_unlinked_unicode_home(self):
        with tempfile.TemporaryDirectory(prefix="owned-browser-profile-π-") as directory:
            fixture = Path(directory).resolve()
            with mock.patch.dict(BROWSER.os.environ, {"HOME": "foreign-user", "APPDATA": "foreign-roaming"}):
                environment = BROWSER.private_profile_environment(fixture)
            home = fixture / "home"
            expected = {"HOME": home, "USERPROFILE": home, "APPDATA": home / "AppData/Roaming",
                        "LOCALAPPDATA": home / "AppData/Local", "TEMP": home / "AppData/Local/Temp",
                        "TMP": home / "AppData/Local/Temp"}
            self.assertEqual(environment, {key: str(path) for key, path in expected.items()})
            for path in set(expected.values()) | {home / "AppData/LocalLow", home / "Desktop",
                                                  home / "Documents", home / "Downloads"}:
                self.assertEqual(BROWSER.plain_path(path), path)
                self.assertTrue(path.is_dir())
            self.assertEqual({path.name for path in fixture.iterdir()}, {"home"})
            self.assertLess(BROWSER.output_size(fixture), BROWSER.MAX_OUTPUT)

    def test_private_internet_cache_compatibility_role_is_an_ordinary_owned_directory(self):
        with tempfile.TemporaryDirectory(prefix="owned-browser-cache-\u03c0-") as directory:
            fixture = Path(directory).resolve()
            BROWSER.private_profile_environment(fixture)
            cache = fixture / "home/AppData/Local/Microsoft/Windows/INetCache/Content.IE5"
            self.assertTrue(cache.is_dir())
            self.assertFalse(cache.is_symlink())
            self.assertFalse(cache.is_junction())
            self.assertEqual(BROWSER.plain_path(cache), cache)
            self.assertEqual(list(cache.iterdir()), [])
            self.assertLess(BROWSER.output_size(fixture), BROWSER.MAX_OUTPUT)

    def test_known_cache_name_never_exempts_a_later_junction_from_the_original_scan(self):
        with tempfile.TemporaryDirectory(prefix="owned-browser-cache-refusal-") as directory:
            fixture = Path(directory).resolve()
            BROWSER.private_profile_environment(fixture)
            cache = fixture / "home/AppData/Local/Microsoft/Windows/INetCache/Content.IE5"
            original = Path.is_junction
            with mock.patch.object(Path, "is_junction", lambda path: path == cache or original(path)), \
                 mock.patch.object(BROWSER.sys, "stderr", new_callable=io.StringIO):
                with self.assertRaises(BROWSER.LIVE.EvidenceLinkRefusal) as caught:
                    BROWSER.output_size(fixture)
            self.assertEqual(caught.exception.kind, "junction")
            self.assertEqual(caught.exception.depth, 7)
            self.assertEqual(caught.exception.route[-2:], ("internet-cache", "internet-content-cache"))

    def test_private_profile_refuses_occupied_or_linked_roots_before_reuse(self):
        with tempfile.TemporaryDirectory(prefix="owned-browser-profile-refusal-") as directory:
            fixture = Path(directory).resolve()
            home = fixture / "home"
            home.mkdir()
            foreign = home / "foreign.txt"
            foreign.write_bytes(b"retained foreign bytes")
            with self.assertRaises(FileExistsError):
                BROWSER.private_profile_environment(fixture)
            self.assertEqual(foreign.read_bytes(), b"retained foreign bytes")
            self.assertEqual({path.name for path in home.iterdir()}, {"foreign.txt"})
            for method in ("is_symlink", "is_junction"):
                with self.subTest(method=method), mock.patch.object(Path, method, return_value=True), \
                     mock.patch.object(Path, "mkdir") as mkdir:
                    with self.assertRaises(RuntimeError):
                        BROWSER.private_profile_environment(fixture)
                    mkdir.assert_not_called()

    def test_output_scan_reports_only_fixed_role_and_preserves_failure(self):
        for root, role in [(Path('ci-host-document-history-browser'), 'evidence'),
                           (Path('yoke-owned-history-browser-private'), 'fixture'),
                           (Path('private-other'), 'other')]:
            failure = RuntimeError('private path')
            with self.subTest(role=role), mock.patch.object(BROWSER.LIVE, 'output_size', side_effect=failure), \
                 mock.patch.object(BROWSER.sys, 'stderr', new_callable=io.StringIO) as log:
                with self.assertRaises(RuntimeError) as caught:
                    BROWSER.output_size(root)
                self.assertIs(caught.exception, failure)
                self.assertEqual(log.getvalue(), 'Owned browser output scan failed: ' + role + ' other\n')
        for message, reason in [('Unexpected evidence link', 'link'),
                                ('Evidence entry capacity exceeded', 'entries'),
                                ('Evidence byte capacity exceeded', 'bytes')]:
            with self.subTest(reason=reason), \
                 mock.patch.object(BROWSER.LIVE, 'output_size', side_effect=RuntimeError(message)), \
                 mock.patch.object(BROWSER.sys, 'stderr', new_callable=io.StringIO) as log:
                with self.assertRaises(RuntimeError):
                    BROWSER.output_size(Path('ci-host-document-history-browser'))
                self.assertEqual(log.getvalue(), 'Owned browser output scan failed: evidence ' + reason + '\n')

    def test_preflight_reports_only_a_closed_phase_and_preserves_failure(self):
        for phase in ("ci-identity", "arguments", "disk-reserve", "native-path", "node-path",
                      "node-path-absolute", "node-path-links", "node-path-resolve",
                      "node-path-resolved-links", "node-path-canonical",
                      "source-inputs", "producer-marker", "producer-admission", "output-path", "output-create",
                      "private-profile"):
            with self.subTest(phase=phase), mock.patch.object(BROWSER.sys, "stderr", new_callable=io.StringIO) as log:
                error = RuntimeError("private capability and path")
                with self.assertRaises(RuntimeError) as caught:
                    BROWSER.preflight(phase, mock.Mock(side_effect=error))
                self.assertIs(caught.exception, error)
                self.assertEqual(log.getvalue(), "Owned browser preflight failed: " + phase + "\n")
        callback = mock.Mock(return_value=42)
        self.assertEqual(BROWSER.preflight("disk-reserve", callback), 42)
        callback.assert_called_once_with()
        forbidden = mock.Mock()
        with self.assertRaises(ValueError):
            BROWSER.preflight("private capability and path", forbidden)
        forbidden.assert_not_called()

    def test_node_preflight_preserves_unlinked_canonical_path_admission(self):
        with tempfile.TemporaryDirectory(prefix="owned-node-admission-") as directory:
            node = Path(directory).resolve() / "node.exe"
            node.touch()
            self.assertEqual(BROWSER.preflight_node_path(node), BROWSER.plain_path(node))
            with mock.patch.object(Path, "is_junction", return_value=True), \
                 mock.patch.object(BROWSER.sys, "stderr", new_callable=io.StringIO) as log:
                with self.assertRaises(RuntimeError):
                    BROWSER.preflight_node_path(node)
                self.assertEqual(log.getvalue(), "Owned browser preflight failed: node-path-links\n")
            canonical = node.parent / "canonical-node.exe"
            canonical.touch()
            with mock.patch.object(Path, "resolve", return_value=canonical):
                self.assertEqual(BROWSER.preflight_node_path(node), canonical)
            with mock.patch.object(Path, "resolve", return_value=canonical), \
                 mock.patch.object(Path, "is_symlink", autospec=True, side_effect=lambda path: path == canonical), \
                 mock.patch.object(BROWSER.sys, "stderr", new_callable=io.StringIO) as log:
                with self.assertRaises(RuntimeError):
                    BROWSER.preflight_node_path(node)
                self.assertEqual(log.getvalue(), "Owned browser preflight failed: node-path-resolved-links\n")
            with mock.patch.object(Path, "resolve", side_effect=OSError("private path")), \
                 mock.patch.object(BROWSER.sys, "stderr", new_callable=io.StringIO) as log:
                with self.assertRaises(OSError):
                    BROWSER.preflight_node_path(node)
                self.assertEqual(log.getvalue(), "Owned browser node resolution: other-os-error\n"
                                 "Owned browser node path shape: other\n"
                                 "Owned browser preflight failed: node-path-resolve\n")
            for error, classification in [(FileNotFoundError("private path"), "missing"),
                                          (PermissionError("private path"), "denied")]:
                with self.subTest(classification=classification), \
                     mock.patch.object(Path, "resolve", side_effect=error), \
                     mock.patch.object(BROWSER.sys, "stderr", new_callable=io.StringIO) as log:
                    with self.assertRaises(type(error)):
                        BROWSER.preflight_node_path(node)
                    self.assertEqual(log.getvalue(), f"Owned browser node resolution: {classification}\n"
                                     "Owned browser preflight failed: node-path-resolve\n")
            with mock.patch.object(Path, "resolve", side_effect=OSError(22, "private path")), \
                 mock.patch.object(BROWSER.sys, "stderr", new_callable=io.StringIO) as log:
                with self.assertRaises(OSError):
                    BROWSER.preflight_node_path(node)
                self.assertEqual(log.getvalue(), "Owned browser node resolution: other-os-error\n"
                                 "Owned browser node path shape: other\n"
                                 "Owned browser node resolution errno: 22\n"
                                 "Owned browser preflight failed: node-path-resolve\n")
            for suffix, shape in [(':another:part', 'multiple-colons'), ('"', 'quote')]:
                with self.subTest(shape=shape), \
                     mock.patch.object(Path, "is_symlink", return_value=False), \
                     mock.patch.object(Path, "is_junction", return_value=False), \
                     mock.patch.object(Path, "resolve", side_effect=OSError(22, "private path")), \
                     mock.patch.object(BROWSER.sys, "stderr", new_callable=io.StringIO) as log:
                    with self.assertRaises(OSError):
                        BROWSER.preflight_node_path(str(node) + suffix)
                    self.assertIn("Owned browser node path shape: " + shape, log.getvalue())
                    self.assertNotIn("private path", log.getvalue())
            with mock.patch.object(BROWSER, "plain_path", side_effect=RuntimeError("private path")), \
                 mock.patch.object(BROWSER.sys, "stderr", new_callable=io.StringIO) as log:
                with self.assertRaises(RuntimeError):
                    BROWSER.preflight_node_path(node)
                self.assertEqual(log.getvalue(), "Owned browser preflight failed: node-path-canonical\n")

    def test_disk_preflight_refuses_before_paths_or_output_creation(self):
        with mock.patch.object(BROWSER, "ci_identity", return_value={}), \
             mock.patch.object(BROWSER.shutil, "disk_usage", return_value=mock.Mock(free=BROWSER.MIN_FREE - 1)), \
             mock.patch.object(BROWSER, "native_path") as native, \
             mock.patch.object(Path, "mkdir") as mkdir, \
             mock.patch.object(BROWSER.sys, "stderr", new_callable=io.StringIO) as log:
            with self.assertRaises(RuntimeError):
                BROWSER.main(["--native", "yoke.exe", "--node", "node.exe", "--output", "evidence"])
            native.assert_not_called()
            mkdir.assert_not_called()
            self.assertEqual(log.getvalue(), "Owned browser preflight failed: disk-reserve\n")

    def test_real_driver_shutdown_protocol_with_in_memory_pipes(self):
        result = subprocess.run(
            ["node", "--test", str(SCRIPTS / "tests/host_history_browser_protocol.test.cjs")],
            cwd=SCRIPTS.parent, capture_output=True, text=True, timeout=20, check=False)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_non_ci_refuses_before_spec_or_native_reads(self):
        with mock.patch.object(BROWSER, "ci_identity", side_effect=RuntimeError("CI refused")), \
             mock.patch.object(BROWSER, "read_json") as reader, \
             mock.patch.object(BROWSER, "sha") as native, \
             mock.patch.object(BROWSER.subprocess, "Popen") as launch:
            with self.assertRaisesRegex(RuntimeError, "CI refused"):
                BROWSER.worker(None)
            reader.assert_not_called()
            native.assert_not_called()
            launch.assert_not_called()

    def test_closed_schema_and_nonce_are_admitted_before_paths(self):
        valid = {key: "inert" for key in ("schema", "workerJobName", "nonce", "native", "fixture", "output",
                                         "node", "producer", "inputs", "ci", "variant")}
        valid.update(schema=BROWSER.SCHEMA, variant="native", nonce="a" * 64,
                     workerJobName="Local\\YokeHistoryCI-" + "b" * 64)
        for changed in ({}, {**valid, "unexpected": True}, {**valid, "nonce": "wrong"},
                        {**valid, "schema": "foreign"}, {**valid, "workerJobName": "Local\\foreign"},
                        {**valid, "variant": "http"}, {**valid, "variant": "serena"},
                        {**valid, "variant": ["native"]}):
            with self.subTest(spec=changed), \
                 mock.patch.dict(BROWSER.os.environ, {"YOKE_BROWSER_WORKER_NONCE": "a" * 64,
                                                     "YOKE_BROWSER_WORKER_VARIANT": "native"}), \
                 mock.patch.object(BROWSER, "plain_path", side_effect=AssertionError("Must not inspect paths")), \
                 mock.patch.object(BROWSER, "sha") as digest:
                with self.assertRaises(RuntimeError):
                    BROWSER.validate_spec(changed, Path("inert"))
                digest.assert_not_called()

    def test_specific_job_and_producer_precede_first_native_payload_access(self):
        spec = {"workerJobName": "Local\\YokeHistoryCI-" + "b" * 64,
                "ci": {}, "inputs": {}, "producer": {}}
        for gate in ("job", "source", "producer"):
            order = []

            def job(_):
                order.append("job")
                if gate == "job":
                    raise RuntimeError("Job refused")

            def producer(*_):
                order.append("producer")
                raise RuntimeError("Producer refused")

            with self.subTest(gate=gate), \
                 mock.patch.object(BROWSER, "ci_identity", return_value={}), \
                 mock.patch.object(BROWSER, "plain_path", side_effect=lambda p: p), \
                 mock.patch.object(BROWSER, "read_json", return_value=spec), \
                 mock.patch.object(BROWSER, "validate_spec", return_value=("native", "fixture", "output", "node")), \
                 mock.patch.object(BROWSER.LIVE, "require_worker_job", side_effect=job), \
                 mock.patch.object(BROWSER.LIVE, "source_inputs", return_value={"different": True} if gate == "source" else {}), \
                 mock.patch.object(BROWSER, "validate_producer", side_effect=producer), \
                 mock.patch.object(BROWSER, "sha", side_effect=AssertionError("No native image read")), \
                 mock.patch.object(BROWSER, "exercise", side_effect=AssertionError("No worker execution")):
                with self.assertRaises(RuntimeError):
                    BROWSER.worker(Path("inert-spec.json"))
                self.assertEqual(order, ["job", "producer"] if gate == "producer" else ["job"])

    def producer(self):
        native = mock.Mock()
        native.__str__ = mock.Mock(return_value="explicit-owned-native")
        native.stat.return_value = mock.Mock(st_size=100, st_mtime_ns=10)
        value = {"schema": BROWSER.SCHEMA, "ci": {"GITHUB_SHA": "a" * 40}, "sourceInputs": {"driver": "b" * 64},
                 "startedNs": 11, "finishedNs": 12, "native": str(native), "nativeSize": 100, "nativeMtimeNs": 10}
        return native, value

    def test_producer_is_exact_fresh_and_metadata_bound_without_opening_an_image(self):
        native, value = self.producer()
        BROWSER.validate_producer(value, value["ci"], value["sourceInputs"], native, 13)
        changes = [{"extra": True}, {"schema": "foreign"}, {"sourceInputs": {}}, {"ci": {}},
                   {"native": "other"}, {"nativeSize": 160000001}, {"nativeSize": True},
                   {"nativeMtimeNs": -1}, {"finishedNs": 14}, {"startedNs": 13},
                   {"startedNs": "11"}, {"nativeMtimeNs": 11}, {"nativeSize": 99}]
        for change in changes:
            changed = {**copy.deepcopy(value), **change}
            with self.subTest(change=change), self.assertRaises(RuntimeError):
                BROWSER.validate_producer(changed, value["ci"], value["sourceInputs"], native, 13)
        with self.assertRaises(RuntimeError):
            BROWSER.validate_producer(value, value["ci"], value["sourceInputs"], native, 901000000013)
        native.open.assert_not_called()
        native.read_bytes.assert_not_called()

    def test_duplicate_oversized_truncated_and_nonobject_specs_cannot_be_admitted(self):
        for raw in (b'{"schema":1,"schema":2}', b'{"nested":{"nonce":1,"nonce":2}}', b'{"schema":'):
            with self.subTest(raw=raw), self.assertRaises((RuntimeError, ValueError)):
                BROWSER.LIVE.decode(raw)
        with self.assertRaises(RuntimeError):
            BROWSER.LIVE.decode(b" " * 65, 64)
        with tempfile.TemporaryDirectory(prefix="owned-browser-parser-") as directory:
            path = Path(directory) / "owned-spec-π.json"
            path.write_bytes(b'{}' + b' ' * 65)
            with mock.patch.object(Path, "open", side_effect=AssertionError("Oversized input must remain unopened")):
                with self.assertRaises(RuntimeError):
                    BROWSER.read_json(path, 64)
        for value in ([], None, True, "fake"):
            with self.assertRaises(RuntimeError):
                BROWSER.validate_spec(value, Path("inert"))

    def test_link_or_escape_path_is_refused_before_native_payload_reads(self):
        for value in (Path("relative.exe"), Path("../outside.exe")):
            with self.assertRaises(RuntimeError):
                BROWSER.plain_path(value)
        with tempfile.TemporaryDirectory(prefix="owned-browser-path-") as directory:
            target = Path(directory).resolve() / "foreign.exe"
            target.write_bytes(b"inert data, never an executable")
            with self.assertRaises(RuntimeError):
                BROWSER.native_path(target)


class PrivateEvidenceAndCleanup(unittest.TestCase):
    def test_capabilities_urls_headers_and_oversized_observations_never_enter_evidence(self):
        capability = "a" * 64
        safe = {"phase": "pending", "ok": True, "states": [{"draftSha256": "b" * 64}]}
        self.assertEqual(BROWSER.admit_driver_reply(safe, "pending", capability), safe)
        for payload in (capability, "http://127.0.0.1:1234", "https://external.invalid/", "ws://127.0.0.1/",
                        "wss://example.invalid/", "Bearer private", "x" * BROWSER.MAX_JSON):
            with self.subTest(payload=payload[:80]), self.assertRaises(RuntimeError):
                BROWSER.admit_driver_reply({**safe, "nested": [{"error": payload}]}, "pending", capability)
        for reply in ({**safe, "ok": False}, {**safe, "phase": "after"}, [], None):
            with self.assertRaises(RuntimeError):
                BROWSER.admit_driver_reply(reply, "pending", capability)

    def test_driver_reader_refuses_truncated_oversized_duplicate_and_malformed_frames(self):
        for raw in (b'{"ok":true}', b'{"ok":true,"ok":false}\n', b'{broken}\n',
                    b' ' * (BROWSER.MAX_JSON + 1) + b'\n'):
            owner = object.__new__(BROWSER.Browser)
            owner.frames, owner.error = queue.Queue(maxsize=4), None
            owner.process = mock.Mock(stdout=io.BytesIO(raw))
            owner._read()
            self.assertEqual(owner.error, "driver-reader-failed")
            self.assertTrue(owner.frames.empty())

    def artifact_proof(self):
        states = [{"screenshot": viewport + "-" + phase + ".png", "actualScreenshotCapture": True,
                   "humanVisualReviewComplete": False, "noHorizontalOverflow": True,
                   "screenshotBytes": 33} for phase in ("before", "pending", "after")
                  for viewport in ("desktop", "mobile")]
        return {"result": {"before": {"states": states[:2]}, "pending": {"states": states[2:4]},
                           "saves": [{"browser": item} for item in states[4:]]}}

    def test_empty_duplicate_escape_or_unreviewed_artifacts_cannot_be_protocol_only_success(self):
        for alteration in ("empty", "duplicate", "escape", "claim"):
            proof = self.artifact_proof()
            first = proof["result"]["before"]["states"][0]
            if alteration == "empty":
                proof["result"]["saves"] = []
            elif alteration == "duplicate":
                first["screenshot"] = "mobile-before.png"
            elif alteration == "escape":
                first["screenshot"] = "../desktop-before.png"
            else:
                first["humanVisualReviewComplete"] = True
            with self.subTest(alteration=alteration), \
                 mock.patch.object(BROWSER, "plain_path", side_effect=AssertionError("No artifact open")), \
                 self.assertRaises(RuntimeError):
                BROWSER.verify_artifacts(Path("inert-output"), proof)

    def test_screenshot_capacity_and_invalid_header_refuse_before_hashing_any_payload(self):
        for size in (32, 2_000_001, 33):
            image = mock.MagicMock()
            image.stat.return_value.st_size = size
            image.open.return_value.__enter__.return_value.read.return_value = b"invalid PNG fixture, never live!!!"
            with self.subTest(size=size), \
                 mock.patch.object(BROWSER, "plain_path", return_value=image), \
                 mock.patch.object(BROWSER, "sha", side_effect=AssertionError("No unadmitted payload hash")), \
                 self.assertRaises(RuntimeError):
                BROWSER.verify_artifacts(Path("inert-output"), self.artifact_proof())
            if size != 33:
                image.open.assert_not_called()

    def test_worker_success_cannot_be_created_by_forced_cleanup(self):
        row = {"assignedBeforeResume": True, "exitCode": 0, "treeExitedBeforeCleanup": True,
               "activeBeforeCleanup": 0, "activeAfterCleanup": 0}
        self.assertTrue(BROWSER.success_proof(row))
        for change in ({"assignedBeforeResume": False}, {"exitCode": 1}, {"exitCode": False},
                       {"treeExitedBeforeCleanup": False}, {"activeBeforeCleanup": 1},
                       {"activeAfterCleanup": 1}, {"activeBeforeCleanup": False}):
            self.assertFalse(BROWSER.success_proof({**row, **change}))
        self.assertFalse(BROWSER.success_proof({"activeAfterCleanup": 0}))

    def test_browser_shutdown_requires_both_raw_exits_and_pipe_and_stdout_eof(self):
        original = {"phase": "close", "ok": True, "browserCloseRequested": True,
                    "browserExitCode": 0, "cdpEOF": True, "browserOutputEOF": True}
        for change in ({}, {"browserCloseRequested": False}, {"browserExitCode": 125}, {"cdpEOF": False},
                       {"browserOutputEOF": False}, {"driverExit": 125}, {"readerAlive": True}, {"extraFrame": True}):
            owner = object.__new__(BROWSER.Browser)
            owner.process = mock.Mock(returncode=change.get("driverExit", 0))
            owner.reader = mock.Mock()
            owner.reader.is_alive.return_value = change.get("readerAlive", False)
            owner.frames = queue.Queue()
            if change.get("extraFrame"):
                owner.frames.put({"extra": True})
            owner.stderr = mock.Mock()
            owner.error = None
            owner.call = mock.Mock(return_value={**original, **change})
            with self.subTest(change=change):
                if change:
                    with self.assertRaises(RuntimeError):
                        owner.shutdown()
                else:
                    self.assertTrue(owner.shutdown()["beforeForcedCleanup"])
            owner.process.kill.assert_not_called()

    def test_original_tree_runner_and_three_variant_gate_remain_unchanged(self):
        old = ast.parse((SCRIPTS / "host_history_live.py").read_text(encoding="utf-8"))
        new = ast.parse((SCRIPTS / "host_history_browser.py").read_text(encoding="utf-8"))
        function = lambda tree, name: next(n for n in tree.body if isinstance(n, ast.FunctionDef) and n.name == name)
        self.assertEqual(ast.dump(function(old, "bounded_worker")), ast.dump(function(new, "bounded_worker")))
        self.assertIn('("persistent-python", "serena", Path(sys.executable))',
                      (SCRIPTS / "host_history_live.py").read_text(encoding="utf-8"))
        self.assertIn('"nativeBrowserAcceptanceClaimed": False',
                      (SCRIPTS / "host_history_live.py").read_text(encoding="utf-8"))

    def test_browser_source_has_no_mock_success_or_security_bypass(self):
        source = (SCRIPTS / "host_history_browser.cjs").read_text(encoding="utf-8")
        for forbidden in ("--no-sandbox", "--disable-web-security", "--remote-allow-origins", "Fetch.fulfillRequest",
                          "Page.setDocumentContent", "window.fetch =", "globalThis.fetch =", "--remote-debugging-port"):
            self.assertNotIn(forbidden, source)
        for required in ("Fetch.continueRequest", "Fetch.failRequest", "Page.captureScreenshot", "Input.insertText",
                         "Runtime.exceptionThrown", "Browser.close", "PNG_LIMIT = 2_000_000", "headless: true",
                         "visualReviewComplete: false", "humanVisualReviewComplete: false"):
            self.assertIn(required, source)


class OwningVariants(unittest.TestCase):
    def test_mocked_exercise_cases_run_without_a_windows_subprocess_constant(self):
        with mock.patch.dict(BROWSER.subprocess.__dict__):
            BROWSER.subprocess.__dict__.pop("CREATE_NO_WINDOW", None)
            cases = unittest.TestSuite([
                WorkerAdmission("test_before_review_failures_preserve_refusal_owned_bytes_and_teardown"),
                OwningVariants("test_complete_exercise_selects_both_owners_and_checks_live_values_before_independent_exits")])
            result = unittest.TextTestRunner(stream=io.StringIO()).run(cases)
            self.assertTrue(result.wasSuccessful(), (result.failures, result.errors))
            self.assertNotIn("CREATE_NO_WINDOW", BROWSER.subprocess.__dict__)

    def test_worker_variant_is_nonce_bound_and_cannot_use_another_variants_output(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            repository, fixture = root / "repository", root / "yoke-owned-history-browser-inert"
            repository.mkdir()
            fixture.mkdir()
            (repository / "validation").mkdir()
            for _, name in BROWSER.VARIANTS.values():
                (repository / "validation" / name).mkdir()
            node = root / "node.exe"
            node.write_bytes(b"inert host path; never executed")
            for variant in BROWSER.VARIANTS:
                with mock.patch.object(BROWSER, "ROOT", repository):
                    output = BROWSER.variant_output(variant)
                    spec = {"schema": BROWSER.SCHEMA, "workerJobName": "Local\\YokeHistoryCI-" + "b" * 64,
                            "nonce": "a" * 64, "variant": variant, "native": "inert-native",
                            "fixture": str(fixture), "output": str(output), "node": str(node),
                            "producer": {}, "inputs": {}, "ci": {}}
                    with mock.patch.dict(BROWSER.os.environ, {"RUNNER_TEMP": str(root),
                            "YOKE_BROWSER_WORKER_NONCE": "a" * 64, "YOKE_BROWSER_WORKER_VARIANT": variant}), \
                         mock.patch.object(BROWSER, "native_path", return_value="owned-native"), \
                         mock.patch.object(BROWSER, "sha", side_effect=AssertionError("No image payload admission")):
                        self.assertEqual(BROWSER.validate_spec(spec, fixture / "spec.json"),
                                         ("owned-native", fixture, output, node))
                        for foreign in BROWSER.VARIANTS:
                            if foreign != variant:
                                with self.subTest(variant=variant, output=foreign), self.assertRaises(RuntimeError):
                                    BROWSER.validate_spec({**spec, "output": str(BROWSER.variant_output(foreign))},
                                                          fixture / "spec.json")

    def test_workflow_adds_separate_profiles_with_original_worker_and_step_bounds(self):
        source = (SCRIPTS.parent / ".github/workflows/rust.yml").read_text(encoding="utf-8")
        for variant in BROWSER.VARIANTS:
            flag = "" if variant == "native" else "--variant " + variant + " "
            command = ('python scripts/host_history_browser.py ' + flag
                       + '--native target/release/yoke.exe --node "$selectedNode" --output validation/'
                       + BROWSER.VARIANTS[variant][1])
            self.assertEqual(source.count(command + "\n"), 2)
        for step in source.split("      - name: ")[1:]:
            if step.startswith("Exercise the real browser on both owning "):
                self.assertIn("timeout-minutes: 4\n", step)
                self.assertIn("if: always() && !cancelled() && ", step)
                self.assertIn(".outcome == 'success'", step)
        live = (SCRIPTS / "host_history_live.py").read_text(encoding="utf-8")
        browser = (SCRIPTS / "host_history_browser.py").read_text(encoding="utf-8")
        function = lambda text: next(node for node in ast.parse(text).body
                                    if isinstance(node, ast.FunctionDef) and node.name == "bounded_worker")
        self.assertEqual(ast.dump(function(live)), ast.dump(function(browser)))

    def test_complete_exercise_selects_both_owners_and_checks_live_values_before_independent_exits(self):
        # Real Python orchestration against in-memory actors and private fixture
        # files, never a native/browser success substitute.
        for variant, fault in (("native", None), ("serena", None), ("persistent-python", None),
                               ("persistent-python", "current-value"), ("persistent-python", "old-value"),
                               ("persistent-python", "current-exit"), ("persistent-python", "old-exit")):
            with self.subTest(variant=variant, fault=fault), tempfile.TemporaryDirectory() as directory:
                fixture = Path(directory).resolve()
                source, old, new, host, archive = [fixture / name for name in
                                                  ("source-İ", "old", "new", "host", "archive-π")]
                target = host / "settings-π.yaml"
                owner = host / (".yoke-doc-owner-" + "b" * 64 + ".json")
                frame = archive / ("yoke-doc-history-" + "a" * 64 + ".bin")
                events, actors, originals = [], [], {}
                migration = {"target": "b" * 64, "fromAccess": "c" * 64,
                             "toAccess": "d" * 64, "nonce": "e" * 64}
                plan = {"id": "a" * 64, "records": [], "unselectedClosedJournalsAtReview": 0}

                class Actor:
                    def __init__(self, native, root, profile, output, label, protocol="native", python=None):
                        self.protocol, self.label, self.fingerprint = protocol, label, "owned-state"
                        self.writes, self.prints = 0, 0
                        self.pid = 123 if label == "browser-old" else 124
                        actors.append((label, protocol, python))

                    def rpc(self, method, params):
                        if method != "tools/list":
                            raise AssertionError("Only the real profile catalog is expected")
                        return {"tools": [{"name": name} for name in
                                          ("get_current_config", "open_dashboard", "serena_repl")]}

                    def tool(self, name, arguments=None):
                        if name == "get_current_config":
                            return {"sessionId": self.label}
                        code = arguments["code"]
                        events.append((self.label, code))
                        if code == "import os\nprint(os.getpid())":
                            return {"stdout": str(self.pid) + "\n"}
                        if code == "print(owned_marker)":
                            self.prints += 1
                            bad = ((fault == "current-value" and self.pid == 124)
                                   or (fault == "old-value" and self.pid == 123 and self.prints == 2))
                            return {"stdout": "46\n" if bad else "45\n"}
                        if code != "owned_marker = 45\nprint(owned_marker)":
                            raise AssertionError("Unexpected runtime code")
                        return {"stdout": "45\n"}

                    def plan(self, document, content):
                        return {"id": "f" * 64, "content": content}

                    def apply(self, document, proposal, expected=200):
                        if expected == 422:
                            return None
                        target.write_bytes(proposal["content"].encode())
                        path = old / ("yoke-doc-" + "c" * 64 + "-" + str(self.writes) + ".json")
                        path.write_bytes(b"owned-original-" + str(self.writes).encode())
                        originals[path.name] = path.read_bytes()
                        self.writes += 1

                    def document(self, request, expected=200):
                        return None if expected == 422 else {"complete": False,
                                    "blockedByHistoryArchive": True, "journals": []}

                    def state(self):
                        return {"view": {"management": {"sessionStateSha256": self.fingerprint}}}

                    def shutdown(self):
                        events.append((self.label, "shutdown"))
                        return {"nativeExitCode": 0, "stdinWasStillOpen": True, "beforeJobCleanup": True}

                    def close(self):
                        events.append((self.label, "close"))

                class Browser:
                    start = {"ownedInMemoryObservation": True}

                    def __init__(self, *args):
                        pass

                    def call(self, request):
                        if request["phase"] == "before":
                            return {"lastPlanId": "f" * 64}
                        if request["phase"] == "pending":
                            return {}
                        target.write_bytes(request["draft"].encode())
                        identifier = ("f" if request["viewport"] == "desktop" else "9") * 64
                        BROWSER.write_json(new / ("yoke-doc-" + "d" * 64 + "-" + identifier + ".plan.json"),
                                           {"plan": {"ownerGeneration": "e" * 64, "afterHash": BROWSER.sha(target)}})
                        return {"planId": identifier}

                    def shutdown(self):
                        events.append(("browser", "shutdown"))
                        return {"ownedInMemoryObservation": True}

                    def close(self):
                        events.append(("browser", "close"))

                class Handle:
                    def __init__(self, pid, python):
                        self.label = "browser-old" if pid == 123 else "browser-current"

                    def require_exited(self):
                        events.append((self.label, "runtime-exit"))
                        if fault == ("old-exit" if self.label == "browser-old" else "current-exit"):
                            raise RuntimeError("Owned runtime still live")

                    def close(self):
                        events.append((self.label, "handle-close"))

                def cli(command, **options):
                    code = 0
                    if "binding-plan" in command:
                        value = migration
                    elif "binding-apply" in command:
                        owner.write_bytes(b"unchanged-owned-generation")
                        value = {"outcome": "committed", "fileChanged": False}
                    elif "history-plan" in command:
                        plan["records"] = [{"name": name, "hash": BROWSER.sha(old / name)} for name in originals]
                        value = plan
                    elif "history-apply" in command:
                        frame.write_bytes(b"inert-owned-frame")
                        value, code = {}, 1
                    elif "history-inspect" in command:
                        value = {"phase": "committed", "markerPresent": True, "complete": False,
                                 "immutableArchiveVerified": True, "eventId": "9" * 64}
                    elif "history-recover" in command:
                        value = {"complete": True, "markerPresent": False}
                    elif "history-read" in command:
                        name = command[command.index("--record") + 1]
                        value = {"bytesBase64": BROWSER.base64.b64encode(originals[name]).decode(),
                                 "sha256": BROWSER.sha(old / name), "immutableRecordEvidenceOnly": True}
                    else:
                        raise AssertionError("Unexpected host CLI operation")
                    options["stdout"].write(json.dumps(value).encode())
                    return subprocess.CompletedProcess(command, code)

                with ExitStack() as stack:
                    stack.enter_context(mock.patch.object(BROWSER.subprocess, "CREATE_NO_WINDOW", 0x08000000, create=True))
                    for parent, name, replacement in ((BROWSER.LIVE, "Actor", Actor),
                            (BROWSER, "Browser", Browser), (BROWSER.LIVE, "RuntimeHandle", Handle),
                            (BROWSER, "trusted_runtime_python", lambda _: Path("owned-python")),
                            (BROWSER.LIVE, "require_worker_job", lambda _: None),
                            (BROWSER.LIVE, "admitted_frame", lambda *_: [])):
                        stack.enter_context(mock.patch.object(parent, name, replacement))
                    stack.enter_context(mock.patch.object(BROWSER.subprocess, "run", side_effect=cli))
                    stack.enter_context(mock.patch.object(BROWSER.sys, "stderr", new_callable=io.StringIO))
                    call = lambda: BROWSER.exercise(Path("inert-native"), Path("inert-node"), Path("inert-edge"),
                                                    fixture, fixture / "evidence", "inert-job",
                                                    {"native/src/session_dashboard.html": "0" * 64}, variant)
                    if fault:
                        with self.assertRaises(RuntimeError):
                            call()
                    else:
                        BROWSER.admit_variant_result(call(), variant)
                self.assertEqual(actors, [(label, "native" if variant == "native" else "serena",
                                          Path("owned-python") if variant == "persistent-python" else None)
                                         for label in ("browser-old", "browser-current")])
                self.assertIn(("browser-old", "close"), events)
                self.assertIn(("browser-current", "close"), events)
                if variant == "persistent-python":
                    self.assertIn(("browser-old", "handle-close"), events)
                    self.assertIn(("browser-current", "handle-close"), events)
                    if not fault:
                        self.assertLess(events.index(("browser-current", "shutdown")),
                                        events.index(("browser-current", "runtime-exit")))
                        self.assertLess(events.index(("browser-current", "runtime-exit")),
                                        events.index(("browser-old", "shutdown")))
                        self.assertLess(events.index(("browser-old", "shutdown")),
                                        events.index(("browser-old", "runtime-exit")))

    def test_fixed_variants_have_distinct_exact_outputs_and_explicit_python_selection(self):
        outputs = {BROWSER.variant_output(variant) for variant in BROWSER.VARIANTS}
        self.assertEqual(len(outputs), 3)
        self.assertEqual(BROWSER.variant_output("native"),
                         BROWSER.ROOT / "validation/ci-host-document-history-browser")
        for variant in ("http", "../escape", "SERENA", None, ["native"]):
            with self.subTest(variant=variant), self.assertRaises(RuntimeError):
                BROWSER.variant_contract(variant)
        for variant in BROWSER.VARIANTS:
            python = Path("selected-python") if variant == "persistent-python" else None
            with mock.patch.object(BROWSER.LIVE, "Actor") as actor:
                BROWSER.owning_actor(1, 2, 3, 4, "current", variant, python)
                expected = (1, 2, 3, 4, "current")
                if variant != "native":
                    expected += ("serena", python)
                actor.assert_called_once_with(*expected)
            with mock.patch.object(BROWSER.LIVE, "Actor") as actor, self.assertRaises(RuntimeError):
                BROWSER.owning_actor(1, 2, 3, 4, "current", variant, None if python else Path("unselected"))
            actor.assert_not_called()

    def test_python_selection_uses_only_the_running_host_interpreter(self):
        path = mock.Mock()
        path.name = "python.exe"
        path.is_file.return_value = True
        path.is_relative_to.return_value = False
        with mock.patch.object(BROWSER, "plain_path", return_value=path) as plain:
            self.assertIs(BROWSER.trusted_runtime_python(Path("fixture")), path)
            plain.assert_called_once_with(Path(BROWSER.sys.executable))
            for change in ("analyzed", "fixture", "wrong-name", "missing"):
                path.name = "foreign.exe" if change == "wrong-name" else "python.exe"
                path.is_file.return_value = change != "missing"
                path.is_relative_to.side_effect = [True] if change == "analyzed" else [False, change == "fixture"]
                with self.subTest(change=change), self.assertRaises(RuntimeError):
                    BROWSER.trusted_runtime_python(Path("fixture"))

    def test_profile_catalog_refuses_wrong_surface_duplicates_and_unavailable_runtime(self):
        names = ["get_current_config", "open_dashboard", "serena_repl"]
        for variant in ("serena", "persistent-python"):
            actor = mock.Mock(protocol="serena")
            actor.rpc.return_value = {"tools": [{"name": name} for name in names]}
            row = BROWSER.profile_observation(actor, variant, "current")
            self.assertTrue(row["serenaDashboardToolsObserved"])
            actor.rpc.assert_called_once_with("tools/list", {})
        for names in (["ci_session", "get_current_config", "open_dashboard"],
                      ["get_current_config", "open_dashboard"],
                      ["get_current_config", "open_dashboard", "serena_repl", "serena_repl"],
                      ["get_current_config", "open_dashboard", "private://capability"],
                      [], ["x"] * 513):
            actor = mock.Mock(protocol="serena")
            actor.rpc.return_value = {"tools": [{"name": name} for name in names]}
            with self.subTest(names=names[:4]), self.assertRaises(RuntimeError):
                BROWSER.profile_observation(actor, "persistent-python", "current")
        with self.assertRaises(RuntimeError):
            BROWSER.profile_observation(mock.Mock(protocol="native"), "serena", "current")

    def test_runtime_cannot_claim_retention_or_exit_from_a_failed_observation(self):
        actor = mock.Mock()
        actor.tool.side_effect = [{"sessionId": "current"}, {"stdout": "45\n"}, {"stdout": "123\n"},
                                  {"stdout": "46\n"}, {"stdout": "45\n"}]
        with mock.patch.object(BROWSER.LIVE, "RuntimeHandle") as handle:
            runtime = BROWSER.PersistentRuntime(actor, Path("selected-python"), "current")
            handle.assert_called_once_with(123, Path("selected-python"))
            with self.assertRaises(RuntimeError):
                runtime.require_exited()
            handle.return_value.require_exited.assert_not_called()
            with self.assertRaises(RuntimeError):
                runtime.require_retained()
            with self.assertRaises(RuntimeError):
                runtime.require_exited()
            runtime.require_retained()
            handle.return_value.require_exited.side_effect = RuntimeError("Owned exit absent")
            with self.assertRaisesRegex(RuntimeError, "Owned exit absent"):
                runtime.require_exited()
            runtime.close()
            handle.return_value.close.assert_called_once()

    def test_result_cannot_substitute_a_native_session_or_cleanup_for_selected_profile_proof(self):
        profile = {"protocol": "serena", "serenaDashboardToolsObserved": True, "nativeSessionToolAbsent": True,
                   "persistentToolObserved": True, "toolCount": 3}
        runtime = {"marker45Retained": True, "imageIdentityVerified": True, "exitBeforeJobCleanup": True}
        valid = {"automationComplete": True, "variant": "persistent-python", "owningProtocol": "serena",
                 "profileObservations": [{**profile, "role": role} for role in ("old", "current")],
                 "persistentRuntimes": [{**runtime, "role": role} for role in ("current", "old")]}
        BROWSER.admit_variant_result(valid, "persistent-python")
        variants = [{**valid, "variant": "native"}, {**valid, "owningProtocol": "native"},
                    {**valid, "automationComplete": False}, {**valid, "profileObservations": []},
                    {**valid, "persistentRuntimes": []}, {**valid, "profileObservations": [1, 2]}]
        for section, key in (("profileObservations", "serenaDashboardToolsObserved"),
                             ("profileObservations", "nativeSessionToolAbsent"),
                             ("profileObservations", "persistentToolObserved"),
                             ("persistentRuntimes", "marker45Retained"),
                             ("persistentRuntimes", "imageIdentityVerified"),
                             ("persistentRuntimes", "exitBeforeJobCleanup")):
            changed = copy.deepcopy(valid)
            changed[section][0][key] = False
            variants.append(changed)
        for changed in variants:
            with self.subTest(changed=changed), self.assertRaises(RuntimeError):
                BROWSER.admit_variant_result(changed, "persistent-python")
        native = {"automationComplete": True, "variant": "native", "owningProtocol": "native",
                  "profileObservations": [], "persistentRuntimes": []}
        BROWSER.admit_variant_result(native, "native")
        with self.assertRaises(RuntimeError):
            BROWSER.admit_variant_result({**native, "persistentRuntimes": valid["persistentRuntimes"]}, "native")

    def test_cleanup_attempts_all_owned_resources_and_never_suppresses_a_refusal(self):
        resources = [mock.Mock() for _ in range(5)]
        order = []
        failure = RuntimeError("Owned teardown refused")
        for index, resource in enumerate(resources):
            def close(index=index):
                order.append(index)
                if index in (0, 2):
                    raise failure
            resource.close.side_effect = close
        with self.assertRaises(RuntimeError) as caught:
            BROWSER.close_owned_resources(resources[0], resources[1:3], resources[3:])
        self.assertIs(caught.exception, failure)
        self.assertEqual(order, [0, 2, 1, 4, 3])


if __name__ == "__main__":
    unittest.main()
