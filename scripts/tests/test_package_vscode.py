"""Offline archive regressions; synthetic binaries are never executed."""
import contextlib
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import sys
import struct
import tempfile
import unittest
from unittest.mock import patch
import warnings
import zipfile

SPEC = importlib.util.spec_from_file_location(
    "package_vscode", str(Path(__file__).resolve().parents[1] / "package-vscode.py")
)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class NativeVsix(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        extension = self.root / "extensions/vscode"
        extension.mkdir(parents=True)
        (extension / "package.json").write_text(json.dumps({
            "version": "0.4.0", "name": "synthetic-yoke", "publisher": "fixture",
            "displayName": "Packaging fixture", "description": "No runtime execution",
            "engines": {"vscode": "^1.90.0"}
        }), encoding="utf-8")
        (extension / "extension.cjs").write_text("// Synthetic archive fixture\n", encoding="utf-8")
        (extension / "test-host.cjs").write_text("// Host fixture must not ship\n", encoding="utf-8")
        (self.root / "package.json").write_text('{"version":"0.3.0"}', encoding="utf-8")
        (self.root / "LICENSE").write_text("Synthetic own-license fixture\n", encoding="utf-8")
        self.binary = b"#!/bin/sh\n# Synthetic binary fixture; never executed\n"
        self.native = self.root / "native.zip"
        with zipfile.ZipFile(self.native, "w") as archive:
            info = zipfile.ZipInfo("yoke")
            info.external_attr = 0o100755 << 16
            archive.writestr(info, self.binary)
            archive.writestr("manifest.json", json.dumps({
                "version": "0.4.0", "target": "x86_64-unknown-linux-gnu",
                "binarySha256": hashlib.sha256(self.binary).hexdigest()
            }))
        self.output = self.root / "fixture.vsix"

    def package(self, include_typescript=False):
        args = ["package-vscode.py", "--native-archive", str(self.native), "--output", str(self.output)]
        if include_typescript:
            args.append("--include-typescript")
        with patch.object(MODULE, "ROOT", self.root), patch.object(sys, "argv", args), contextlib.redirect_stdout(io.StringIO()):
            MODULE.main()

    def test_native_binary_remains_executable_in_unix_vsix(self):
        self.package()
        with zipfile.ZipFile(self.output) as archive:
            entry = archive.getinfo("extension/runtime/native/yoke")
            self.assertEqual(entry.create_system, 3)
            self.assertEqual((entry.external_attr >> 16) & 0o777, 0o755)
            self.assertEqual(archive.read(entry), self.binary)
            self.assertIn(b'TargetPlatform="linux-x64"', archive.read("extension.vsixmanifest"))

    def test_host_fixture_and_typescript_are_excluded_from_native_default(self):
        self.package()
        with zipfile.ZipFile(self.output) as archive:
            self.assertIsNone(archive.testzip())
            self.assertNotIn("extension/test-host.cjs", archive.namelist())
            self.assertFalse(any(name.startswith("extension/runtime/dist/") or name.startswith("extension/runtime/node_modules/") for name in archive.namelist()))

    def test_existing_output_is_preserved(self):
        self.output.write_bytes(b"Existing archive fixture")
        with self.assertRaises(SystemExit):
            self.package()
        self.assertEqual(self.output.read_bytes(), b"Existing archive fixture")

    def native_members(self, extra=(), manifest=None):
        if manifest is None:
            manifest = json.dumps({
                "version": "0.4.0", "target": "x86_64-unknown-linux-gnu",
                "binarySha256": hashlib.sha256(self.binary).hexdigest()
            }).encode()
        with warnings.catch_warnings():
            warnings.simplefilter("ignore", UserWarning)
            with zipfile.ZipFile(self.native, "w") as archive:
                archive.writestr("manifest.json", manifest)
                archive.writestr("yoke", self.binary)
                for name, data in extra:
                    if isinstance(name, str):
                        # Preserve the actual ZIP name even on Windows, where
                        # ZipInfo's constructor would normalize backslashes.
                        entry = zipfile.ZipInfo()
                        entry.filename = entry.orig_filename = name
                    else:
                        entry = name
                    archive.writestr(entry, data)

    def assert_refused_before_native_payload(self):
        reads = []
        original = zipfile.ZipFile.read

        def read(archive, *args, **kwargs):
            if Path(archive.filename) == self.native:
                reads.append(str(args[0]))
            return original(archive, *args, **kwargs)

        with patch.object(zipfile.ZipFile, "read", read):
            with self.assertRaises(SystemExit):
                self.package()
        self.assertEqual(reads, [], "Unsafe entry metadata must refuse before payload reads")
        self.assertFalse(self.output.exists())

    def test_unsafe_and_ambiguous_member_names_refuse_before_any_payload_read(self):
        for index, name in enumerate([
            "../escape", "/absolute", "C:/drive", "notes:stream", "a\\b",
            "a//b", "a/./b", "trailing. ", "NUL.txt", "aux/notice",
            "control\x01name", "Manifest.json", "manifest.json", "yoke",
        ]):
            self.output = self.root / f"counterexample-{index}.vsix"
            with self.subTest(name=name):
                self.native_members([(name, b"Owned archive counterexample")])
                self.assert_refused_before_native_payload()

    def test_file_parent_conflicts_refuse_before_any_payload_read(self):
        self.native_members([("licenses", b"file"), ("licenses/NOTICE", b"child")])
        self.assert_refused_before_native_payload()

    def test_symlink_member_refuses_before_any_payload_read(self):
        linked = zipfile.ZipInfo("linked-notice")
        linked.create_system = 3
        linked.external_attr = 0o120777 << 16
        self.native_members([(linked, b"outside")])
        self.assert_refused_before_native_payload()

    def test_entry_and_total_budgets_precede_manifest_or_binary_reads(self):
        original = zipfile.ZipFile.infolist
        for index, sizes in enumerate([
            {"manifest.json": 1_000_001},
            {"yoke": 128_000_001},
            {"one": 128_000_000, "two": 128_000_000},
        ]):
            self.output = self.root / f"budget-{index}.vsix"
            self.native_members([("one", b"one"), ("two", b"two")])

            def declared(archive):
                entries = original(archive)
                if Path(archive.filename) == self.native:
                    for entry in entries:
                        if entry.filename in sizes:
                            entry.file_size = sizes[entry.filename]
                return entries

            with self.subTest(sizes=sizes), patch.object(zipfile.ZipFile, "infolist", declared):
                self.assert_refused_before_native_payload()

    def test_duplicate_manifest_fields_and_non_object_manifest_refuse_without_output(self):
        for index, manifest in enumerate([
            b'{"version":"0.4.0","version":"0.4.0","target":"x86_64-unknown-linux-gnu","binarySha256":"'
            + hashlib.sha256(self.binary).hexdigest().encode() + b'"}',
            b"[]",
            b'{"version":"0.4.0","target":"x86_64-unknown-linux-gnu","binarySha256":NaN}',
        ]):
            self.output = self.root / f"manifest-{index}.vsix"
            with self.subTest(manifest=manifest):
                self.native_members(manifest=manifest)
                with self.assertRaises(SystemExit):
                    self.package()
                self.assertFalse(self.output.exists())

    def test_directory_budgets_refuse_before_zip_metadata_allocation(self):
        original = self.native.read_bytes()
        position = original.rfind(b"PK\x05\x06")
        for entries, size in [(8193, 1024), (2, 8_000_001)]:
            encoded = bytearray(original)
            struct.pack_into("<HHI", encoded, position + 8, entries, entries, size)
            self.native.write_bytes(encoded)
            with self.subTest(entries=entries, size=size), patch.object(
                MODULE.zipfile, "ZipFile", side_effect=AssertionError("Metadata allocated too early")
            ):
                with self.assertRaises(SystemExit):
                    self.package()
            self.assertFalse(self.output.exists())

    def test_valid_zip_comment_and_manifest_checksum_tampering_are_distinct(self):
        with zipfile.ZipFile(self.native, "a") as archive:
            archive.comment = b"Owned portable archive comment"
        self.package()
        self.output = self.root / "tampered.vsix"
        self.native_members(manifest=json.dumps({
            "version": "0.4.0", "target": "x86_64-unknown-linux-gnu",
            "binarySha256": "0" * 64
        }).encode())
        with self.assertRaises(SystemExit):
            self.package()
        self.assertFalse(self.output.exists())

    def test_unicode_notices_and_explicit_directories_preserve_exact_bytes(self):
        name = "licenses/caf\u00e9-e\u0301-\U0001f9ed/NOTICE"
        payload = "Owned Unicode notice \u03c0\n".encode()
        self.native_members([("licenses/", b""), (name, payload)])
        self.package()
        with zipfile.ZipFile(self.output) as archive:
            self.assertEqual(archive.read("extension/runtime/native/" + name), payload)
            self.assertNotIn("extension/runtime/native/licenses/", archive.namelist())

    def test_combined_archive_retains_both_runtimes_and_exact_notices(self):
        inputs = {
            "dist/cli.js": b"// Owned synthetic CLI; never execute\n",
            "workers/owned.py": b"# Owned synthetic worker; never execute\n",
            "node_modules/typescript/lib/typescript.js": b"// Synthetic compiler fixture\n",
            "THIRD_PARTY_NOTICES.md": b"Owned retained runtime notice\n",
        }
        for name, data in inputs.items():
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
        self.package(include_typescript=True)
        with zipfile.ZipFile(self.output) as archive:
            for name, data in inputs.items():
                self.assertEqual(archive.read("extension/runtime/" + name), data)
            self.assertEqual(archive.read("extension/runtime/native/yoke"), self.binary)
            self.assertEqual(json.loads(archive.read("extension/runtime/package.json"))["version"], "0.3.0")

    def test_all_existing_native_targets_retain_their_platform_identity(self):
        for target, binary, platform in [
            ("x86_64-pc-windows-msvc", "yoke.exe", "win32-x64"),
            ("x86_64-unknown-linux-gnu", "yoke", "linux-x64"),
            ("aarch64-apple-darwin", "yoke", "darwin-arm64"),
            ("x86_64-apple-darwin", "yoke", "darwin-x64"),
        ]:
            with self.subTest(target=target):
                with zipfile.ZipFile(self.native, "w") as archive:
                    archive.writestr(binary, self.binary)
                    archive.writestr("manifest.json", json.dumps({
                        "version": "0.4.0", "target": target,
                        "binarySha256": hashlib.sha256(self.binary).hexdigest()
                    }))
                self.output = self.root / (platform + ".vsix")
                self.package()
                with zipfile.ZipFile(self.output) as archive:
                    self.assertEqual(archive.read("extension/runtime/native/" + binary), self.binary)
                    self.assertIn(('TargetPlatform="' + platform + '"').encode(),
                                  archive.read("extension.vsixmanifest"))

    def test_current_release_checksum_precondition_is_preserved(self):
        built = self.root / "target/release/yoke"
        built.parent.mkdir(parents=True)
        for index, contents in enumerate([b"stale", b"x" * len(self.binary)]):
            built.write_bytes(contents)
            self.output = self.root / f"stale-{index}.vsix"
            with self.assertRaises(SystemExit):
                self.package()
            self.assertFalse(self.output.exists())
            self.assertEqual(built.read_bytes(), contents)
        built.write_bytes(self.binary)
        self.output = self.root / "matching.vsix"
        self.package()


if __name__ == "__main__":
    unittest.main()
