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
