"""Offline archive regressions; synthetic binaries are never executed."""
import contextlib
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch
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

    def package(self):
        args = ["package-vscode.py", "--native-archive", str(self.native), "--output", str(self.output)]
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


if __name__ == "__main__":
    unittest.main()
