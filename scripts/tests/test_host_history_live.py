import importlib.util
import json
from pathlib import Path
import struct
import tempfile
import unittest
from unittest import mock


SPEC = importlib.util.spec_from_file_location(
    "host_history_live", Path(__file__).resolve().parents[1] / "host_history_live.py"
)
LIVE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(LIVE)


class HistoryEvidenceAdmission(unittest.TestCase):
    def fixture(self):
        name = "yoke-doc-" + "a" * 64 + "-" + "b" * 64 + ".plan.json"
        content = '\ufeff{"owned":"π 🦀 e\u0301\\r\\n"}'.encode("utf-8")
        plan = {"id": "c" * 64, "records": [{"name": name, "bytes": len(content),
                "hash": LIVE.hashlib.sha256(content).hexdigest()}]}
        manifest = json.dumps(plan, ensure_ascii=False).encode()
        frame = LIVE.MAGIC + struct.pack("<I", len(manifest)) + manifest
        frame += struct.pack("<I", len(content)) + content
        return plan, {name: content}, frame

    def test_exact_unicode_records_are_verified_independently_from_native_json(self):
        plan, originals, frame = self.fixture()
        rows = LIVE.admitted_frame(frame, plan, originals)
        self.assertEqual(rows, [{"name": next(iter(originals)), "bytes": len(next(iter(originals.values()))),
                                "sha256": plan["records"][0]["hash"]}])

    def test_truncation_oversized_lengths_and_trailing_bytes_refuse(self):
        plan, originals, frame = self.fixture()
        for size in [0, len(LIVE.MAGIC), len(LIVE.MAGIC) + 3, len(frame) - 1]:
            with self.subTest(size=size), self.assertRaises(RuntimeError):
                LIVE.admitted_frame(frame[:size], plan, originals)
        malicious = LIVE.MAGIC + struct.pack("<I", 0xffffffff)
        for data in [malicious, frame + b"trailing"]:
            with self.assertRaises(RuntimeError):
                LIVE.admitted_frame(data, plan, originals)

    def test_record_bytes_names_and_membership_are_not_trusted(self):
        plan, originals, frame = self.fixture()
        bad = bytearray(frame)
        bad[-1] ^= 1
        with self.assertRaises(RuntimeError):
            LIVE.admitted_frame(bytes(bad), plan, originals)
        with self.assertRaises(RuntimeError):
            LIVE.admitted_frame(frame, plan, {})
        for name in ["../foreign", "C:\\foreign.json", "é.json"]:
            changed = json.loads(json.dumps(plan))
            changed["records"][0]["name"] = name
            manifest = json.dumps(changed).encode()
            content = next(iter(originals.values()))
            forged = LIVE.MAGIC + struct.pack("<I", len(manifest)) + manifest
            forged += struct.pack("<I", len(content)) + content
            with self.assertRaises(RuntimeError):
                LIVE.admitted_frame(forged, changed, {name: content})

    def test_duplicate_json_and_oversized_payload_are_rejected(self):
        for data in [b'{"complete":true,"complete":false}', b'{"outer":{"id":1,"id":2}}']:
            with self.assertRaises(RuntimeError):
                LIVE.decode(data)
        with self.assertRaises(RuntimeError):
            LIVE.decode(b" " * 33, 32)

    def test_a_package_cannot_split_the_original_journal_even_when_its_bytes_match(self):
        plan, originals, frame = self.fixture()
        name = next(iter(originals)).replace(".plan.json", ".e0001.json")
        originals[name] = b"original event not in the selected frame"
        with self.assertRaises(RuntimeError):
            LIVE.admitted_frame(frame, plan, originals)

    def test_json_file_limit_precedes_open_and_never_truncates_to_a_valid_prefix(self):
        with tempfile.TemporaryDirectory(prefix="owned-history-parser-") as directory:
            path = Path(directory) / "owned-π.json"
            path.write_bytes(b'{"ok":true}' + b" " * 64)
            with mock.patch.object(Path, "open", side_effect=AssertionError("Must not open oversized input")):
                with self.assertRaises(RuntimeError):
                    LIVE.read_json(path, 32)
            path.write_bytes(b'{"ok":true}')
            self.assertEqual(LIVE.read_json(path, 32), {"ok": True})

    def test_private_endpoint_admission_never_accepts_remote_or_credential_urls(self):
        token = "a" * 64
        self.assertEqual(LIVE.local_endpoint("http://127.0.0.1:12345/#" + token), (12345, token))
        for url in ["https://127.0.0.1:12345/#" + token, "http://localhost:12345/#" + token,
                    "http://example.com:12345/#" + token, "http://user@127.0.0.1:12345/#" + token,
                    "http://127.0.0.1:12345/other#" + token, "http://127.0.0.1:12345/?x=1#" + token,
                    "http://127.0.0.1:12345/#short"]:
            with self.subTest(url=url), self.assertRaises(RuntimeError):
                LIVE.local_endpoint(url)

    def test_local_non_ci_invocation_refuses_before_reading_native(self):
        with mock.patch.dict(LIVE.os.environ, {"GITHUB_ACTIONS": "false"}):
            with mock.patch.object(LIVE, "sha", side_effect=AssertionError("Native must remain unread")):
                with self.assertRaises(RuntimeError):
                    LIVE.main(["--native", "target/release/yoke.exe", "--output", "validation/ci-host-document-history"])

    def test_worker_cannot_bypass_disk_preflight_via_the_internal_entry_point(self):
        with mock.patch.object(LIVE.os, "name", "nt"):
            with mock.patch.object(LIVE.shutil, "disk_usage", return_value=mock.Mock(free=LIVE.MIN_FREE - 1)):
                with mock.patch.object(LIVE, "read_json", side_effect=AssertionError("Input must remain unread")):
                    with self.assertRaises(RuntimeError):
                        LIVE.worker(None)

    def test_worker_job_name_is_explicit_before_any_windows_api_access(self):
        for name in [None, "", "unrelated", "Global\\YokeHistoryCI-" + "a" * 64,
                     "Local\\YokeHistoryCI-" + "a" * 63, "Local\\YokeHistoryCI-" + "a" * 64 + "\\other"]:
            with self.subTest(name=name), mock.patch.object(LIVE.ctypes, "WinDLL", create=True,
                    side_effect=AssertionError("Invalid Job name must not reach Windows")):
                with self.assertRaises(RuntimeError):
                    LIVE.require_worker_job(name)

    def test_specific_job_membership_requires_successful_query_and_true_result(self):
        name = "Local\\YokeHistoryCI-" + "a" * 64
        for opened, query_succeeded, assigned in [(0, 1, 1), (123, 0, 1), (123, 1, 0), (123, 1, 1)]:
            with self.subTest(opened=opened, queried=query_succeeded, assigned=assigned):
                api = mock.Mock()
                api.OpenJobObjectW.return_value = opened
                api.GetCurrentProcess.return_value = 456
                api.CloseHandle.return_value = 1

                def query(process, job, result):
                    self.assertEqual((process, job), (456, 123))
                    result._obj.value = assigned
                    return query_succeeded

                api.IsProcessInJob.side_effect = query
                with mock.patch.object(LIVE.ctypes, "WinDLL", create=True, return_value=api):
                    if opened and query_succeeded and assigned:
                        self.assertEqual(LIVE.require_worker_job(name), {
                            "specificJobMembershipConfirmed": True, "queryHandleClosedBeforeNativeRead": True})
                    else:
                        with self.assertRaises(RuntimeError):
                            LIVE.require_worker_job(name)
                api.OpenJobObjectW.assert_called_once_with(0x0004, False, name)
                if opened:
                    api.CloseHandle.assert_called_once_with(123)
                else:
                    api.IsProcessInJob.assert_not_called()
                    api.CloseHandle.assert_not_called()

    def test_worker_query_handle_must_close_before_admission_succeeds(self):
        api = mock.Mock()
        api.OpenJobObjectW.return_value = 123
        api.CloseHandle.return_value = 0

        def query(process, job, result):
            result._obj.value = 1
            return 1

        api.IsProcessInJob.side_effect = query
        with mock.patch.object(LIVE.ctypes, "WinDLL", create=True, return_value=api):
            with self.assertRaisesRegex(RuntimeError, "query handle close failed"):
                LIVE.require_worker_job("Local\\YokeHistoryCI-" + "a" * 64)

    def test_internal_worker_refuses_an_unassigned_job_before_native_read_or_execution(self):
        # Simulate sufficient disk only for admission-order testing. No native
        # image exists at the supplied inert path. Neither hashing nor process
        # launch may occur, including on the preceding unguarded worker.
        api = mock.Mock()
        api.OpenJobObjectW.return_value = 0
        spec = {"workerJobName": "Local\\YokeHistoryCI-" + "a" * 64,
                "native": str(LIVE.ROOT / "must-not-open-owned-fixture.exe"),
                "fixture": str(LIVE.ROOT), "output": str(LIVE.ROOT), "nativeSha256": "b" * 64}
        with mock.patch.object(LIVE.os, "name", "nt"), \
             mock.patch.object(LIVE.shutil, "disk_usage", return_value=mock.Mock(free=LIVE.MIN_FREE)), \
             mock.patch.object(LIVE, "read_json", return_value=spec), \
             mock.patch.object(LIVE.ctypes, "WinDLL", create=True, return_value=api), \
             mock.patch.object(LIVE, "sha", side_effect=AssertionError("Native must remain unread")), \
             mock.patch.object(LIVE.subprocess, "Popen", side_effect=AssertionError("Native must not run")):
            with self.assertRaisesRegex(RuntimeError, "Job unavailable"):
                LIVE.worker(None)

    @unittest.skipUnless(LIVE.os.name == "nt", "Actual named Job query requires Windows")
    def test_actual_named_empty_job_cannot_admit_the_unassigned_python_caller(self):
        # This creates only an empty temporary Job and queries the current Python
        # process. It assigns no process and opens/executes no native Yoke image.
        api = LIVE.ctypes.WinDLL("kernel32", use_last_error=True)
        api.CreateJobObjectW.argtypes = [LIVE.ctypes.c_void_p, LIVE.ctypes.c_wchar_p]
        api.CreateJobObjectW.restype = LIVE.ctypes.c_void_p
        api.CloseHandle.argtypes = [LIVE.ctypes.c_void_p]
        api.CloseHandle.restype = LIVE.ctypes.c_int
        name = "Local\\YokeHistoryCI-" + LIVE.os.urandom(32).hex()
        handle = api.CreateJobObjectW(None, name)
        self.assertTrue(handle)
        try:
            with self.assertRaisesRegex(RuntimeError, "outside its assigned Job"):
                LIVE.require_worker_job(name)
        finally:
            self.assertTrue(api.CloseHandle(handle))

    def test_foreign_fixture_relocation_preserves_bytes_and_refuses_other_parents(self):
        with tempfile.TemporaryDirectory(prefix="owned-history-retain-") as directory:
            root = Path(directory)
            source, destination = root / "occupied.bin", root / "retained.bin"
            source.write_bytes(b"foreign bytes")
            other = root / "other"
            other.mkdir()
            with self.assertRaises(RuntimeError):
                LIVE.retain_foreign(source, other / "new.bin", b"foreign bytes")
            with self.assertRaises(RuntimeError):
                LIVE.retain_foreign(source, destination, b"wrong bytes")
            self.assertEqual(source.read_bytes(), b"foreign bytes")
            LIVE.retain_foreign(source, destination, b"foreign bytes")
            self.assertEqual(destination.read_bytes(), b"foreign bytes")

    def test_source_inventory_preserves_exact_unicode_bytes_and_rejects_escape_paths(self):
        with tempfile.TemporaryDirectory(prefix="owned-history-inputs-") as directory:
            root = Path(directory).resolve()
            source = root / "native" / "src"
            source.mkdir(parents=True)
            path = source / "owned-π.rs"
            content = "// exact e\u0301 🦀\r\n".encode()
            path.write_bytes(content)
            with mock.patch.object(LIVE, "ROOT", root):
                with mock.patch.object(LIVE.subprocess, "check_output", return_value=b"native/src/owned-\xcf\x80.rs\0"):
                    self.assertEqual(LIVE.source_inputs(), {"native/src/owned-π.rs": LIVE.hashlib.sha256(content).hexdigest()})
                for value in [b"../escape\0", b"/absolute\0", b"native//file\0", b"native\\file\0"]:
                    with mock.patch.object(LIVE.subprocess, "check_output", return_value=value):
                        with self.assertRaises(RuntimeError):
                            LIVE.source_inputs()


if __name__ == "__main__":
    unittest.main()
