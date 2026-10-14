"""Offline behavioral regressions for native packaging license discovery."""
import base64
import hashlib
import importlib.util
import json
import contextlib
import io
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch
import zipfile

SPEC = importlib.util.spec_from_file_location(
    "package_native", str(Path(__file__).resolve().parents[1] / "package-native.py")
)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)
REVISION = "a" * 40


def git_blob(data):
    return hashlib.sha1(b"blob " + str(len(data)).encode() + b"\0" + data).hexdigest()


class LicenseDiscovery(unittest.TestCase):
    def invoke(self, entries, path="", truncated=False, forged=False):
        package = {"name": "synthetic-package", "version": "1.2.3", "license": "MIT", "repository": "https://github.com/example/fixture.git"}
        payloads = {git_blob(data): data for _, data in entries}
        tree = {"truncated": truncated, "tree": [
            {"path": name, "type": "blob", "mode": "100644", "sha": git_blob(data), "size": len(data)} for name, data in entries
        ]}
        calls = []

        def fetch(*args, **kwargs):
            calls.append(args)
            endpoint = args[-1]
            if "/git/trees/" in endpoint:
                return json.dumps(tree)
            sha = endpoint.rsplit("/", 1)[1]
            data = b"tampered" if forged else payloads[sha]
            return json.dumps({"sha": sha, "size": len(data), "encoding": "base64", "content": base64.b64encode(data).decode()})

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / ".cargo_vcs_info.json").write_text(json.dumps({"git": {"sha1": REVISION}, "path_in_vcs": path}), encoding="utf-8")
            with patch.object(MODULE, "command", side_effect=fetch):
                result = MODULE.github_licenses(package, root)
        return result, calls

    def test_nested_package_notice_and_ancestor_license_exclude_sibling_crates(self):
        result, calls = self.invoke([
            ("LICENSE", b"Synthetic root notice"),
            ("crates/target/src/unicode/LICENSE", b"Synthetic Unicode notice"),
            ("crates/other/LICENSE", b"Unrelated sibling notice"),
        ], path="crates/target")
        self.assertEqual([item[0] for item in result], ["LICENSE", "crates/target/src/unicode/LICENSE"])
        self.assertIn(f"/git/trees/{REVISION}?recursive=1", calls[0][-1])
        self.assertTrue(all(f"/blob/{REVISION}/" in item[2] for item in result))
        self.assertTrue(all("fixture.git/blob/" not in item[2] for item in result))

    def test_nested_license_without_root_license_is_found(self):
        result, _ = self.invoke([("grammar/LICENSE", b"Synthetic nested license")])
        self.assertEqual(result[0][0], "grammar/LICENSE")

    def test_truncated_recursive_tree_fails_even_with_root_license(self):
        with self.assertRaisesRegex(RuntimeError, "truncated"):
            self.invoke([("LICENSE", b"Synthetic notice")], truncated=True)

    def test_blob_tampering_fails_exact_git_object_identity(self):
        with self.assertRaisesRegex(RuntimeError, "identity|hash"):
            self.invoke([("LICENSE", b"Synthetic notice")], forged=True)

    def test_no_license_is_a_revision_specific_failure(self):
        with self.assertRaisesRegex(RuntimeError, REVISION):
            self.invoke([("Cargo.toml", b'license = "MIT"')])


class PackagingInputs(unittest.TestCase):
    """Offline host-binary/metadata fixtures; no compiler or model is invoked."""

    def fixture(self, root, version):
        script = root / "scripts/package-native.py"
        script.parent.mkdir()
        (root / "target/release").mkdir(parents=True)
        (root / "native").mkdir()
        binary = root / "target/release" / ("yoke.exe" if sys.platform == "win32" else "yoke")
        binary.write_bytes(b"Labeled executable fixture")
        for name in ["LICENSE", "SECURITY.md", "CHANGELOG.md", "THIRD_PARTY_NOTICES.md", "Cargo.lock", "UPSTREAMS.json", "native/README.md"]:
            (root / name).write_text("Synthetic public guidance", encoding="utf-8")
        package = {"id": "fixture", "name": "yoke-code-intelligence", "version": "0.5.0"}
        metadata = {"packages": [package], "resolve": {"nodes": [{"id": "fixture", "deps": []}]}}

        def invoke(*args, **kwargs):
            if args[:2] == ("rustc", "-vV"):
                return "host: x86_64-pc-windows-msvc\n"
            if args[:2] == ("cargo", "metadata"):
                return json.dumps(metadata)
            if args == (str(binary), "--version"):
                return f"yoke {version}\n"
            raise AssertionError(f"Unexpected fixture command: {args}")

        return script, invoke

    def test_stale_release_binary_cannot_be_labeled_with_a_new_manifest_version(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            script, invoke = self.fixture(root, "0.4.0")
            output = root / "native.zip"
            with patch.object(MODULE, "__file__", str(script)), patch.object(MODULE, "command", side_effect=invoke), patch.object(sys, "argv", [str(script), "--skip-build", "--output", str(output)]), contextlib.redirect_stdout(io.StringIO()):
                with self.assertRaisesRegex(RuntimeError, "version"):
                    MODULE.main()
            self.assertFalse(output.exists())

    def test_matching_release_keeps_public_security_and_release_guidance(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            script, invoke = self.fixture(root, "0.5.0")
            output = root / "native.zip"
            with patch.object(MODULE, "__file__", str(script)), patch.object(MODULE, "command", side_effect=invoke), patch.object(sys, "argv", [str(script), "--skip-build", "--output", str(output)]), contextlib.redirect_stdout(io.StringIO()):
                MODULE.main()
            with zipfile.ZipFile(output) as archive:
                self.assertEqual(json.loads(archive.read("manifest.json"))["version"], "0.5.0")
                self.assertEqual(archive.read("SECURITY.md"), b"Synthetic public guidance")
                self.assertEqual(archive.read("CHANGELOG.md"), b"Synthetic public guidance")


if __name__ == "__main__":
    unittest.main()
