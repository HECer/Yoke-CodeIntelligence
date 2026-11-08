"""Complete formatting coverage, failure propagation and literal host paths."""
import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

SPEC = importlib.util.spec_from_file_location(
    "native_fmt", Path(__file__).resolve().parents[1] / "native_fmt.py"
)
FMT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(FMT)


class FormattingAdmission(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix="native-format-")
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name).resolve()
        self.manifest = self.root / "Cargo.toml"
        self.manifest.write_text("[workspace]\n", encoding="utf8")

    def package(self, root, name, targets, dependencies=()):
        root.mkdir(parents=True, exist_ok=True)
        (root / "Cargo.toml").touch()
        result = []
        for name_, edition in targets:
            source = root / name_
            source.parent.mkdir(parents=True, exist_ok=True)
            source.write_text("// owned\n", encoding="utf8")
            result.append({"src_path": str(source), "edition": edition})
        return {"id": name, "manifest_path": str(root / "Cargo.toml"),
                "targets": result, "dependencies": [{"path": str(p)} for p in dependencies]}

    def test_every_target_kind_uses_target_edition_and_path_dependencies_are_recursive(self):
        dependency = self.root / "dep"
        package = self.package(self.root, "main", [
            ("src/lib.rs", "2021"), ("src/main.rs", "2024"),
            ("tests/test.rs", "2021"), ("examples/example.rs", "2018"),
            ("benches/bench.rs", "2018"), ("build.rs", "2015")], [dependency])
        dep = self.package(dependency, "dep", [("src/lib.rs", "2024")], [self.root])
        responses = {self.manifest: {"packages": [package]},
                     dependency / "Cargo.toml": {"packages": [dep]}}
        with mock.patch.object(FMT, "cargo_metadata", side_effect=responses.__getitem__) as reader:
            roots = FMT.format_roots(self.manifest)
        self.assertEqual(len(roots), 7)
        self.assertEqual(set(roots), {
            (Path(t["src_path"]).resolve(), t["edition"])
            for p in [package, dep] for t in p["targets"]})
        self.assertEqual(reader.call_count, 2)

    def test_repeated_members_and_exact_paths_are_deduplicated(self):
        p = self.package(self.root, "main", [("src/lib.rs", "2024")])
        with mock.patch.object(FMT, "cargo_metadata", return_value={"packages": [p, p]}):
            self.assertEqual(len(FMT.format_roots(self.manifest)), 1)

    def test_conflicting_editions_refuse_before_any_formatter(self):
        p = self.package(self.root, "main", [("src/lib.rs", "2024")])
        p["targets"].append({**p["targets"][0], "edition": "2021"})
        with mock.patch.object(FMT, "cargo_metadata", return_value={"packages": [p]}):
            with self.assertRaises(FMT.FormatError):
                FMT.format_roots(self.manifest)

    def test_missing_path_dependency_is_not_silently_omitted(self):
        p = self.package(self.root, "main", [("src/lib.rs", "2024")], [self.root / "missing"])
        with mock.patch.object(FMT, "cargo_metadata", return_value={"packages": [p]}):
            with self.assertRaises(FMT.FormatError):
                FMT.format_roots(self.manifest)

    def test_malformed_relative_missing_or_unsupported_targets_refuse(self):
        p = self.package(self.root, "main", [("src/lib.rs", "2024")])
        for mutation in [
            {"src_path": "relative.rs", "edition": "2024"},
            {"src_path": str(self.root / "missing.rs"), "edition": "2024"},
            {**p["targets"][0], "edition": "2099"},
            {**p["targets"][0], "edition": []},
            {"src_path": 7, "edition": "2024"},
        ]:
            with self.subTest(target=mutation), mock.patch.object(
                FMT, "cargo_metadata", return_value={"packages": [{**p, "targets": [mutation]}]}
            ):
                with self.assertRaises(FMT.FormatError):
                    FMT.format_roots(self.manifest)

    def test_empty_inventory_refuses_instead_of_complete_success(self):
        for data in [{"packages": []}, {}, {"packages": [{"targets": []}]}]:
            with self.subTest(data=data), mock.patch.object(FMT, "cargo_metadata", return_value=data):
                with self.assertRaises(FMT.FormatError):
                    FMT.format_roots(self.manifest)

    def test_target_and_manifest_capacity_refuse_whole_plan(self):
        p = self.package(self.root, "main", [("src/lib.rs", "2024"), ("src/main.rs", "2024")])
        with mock.patch.object(FMT, "MAX_ROOTS", 1), mock.patch.object(
            FMT, "cargo_metadata", return_value={"packages": [p]}
        ):
            with self.assertRaises(FMT.FormatError):
                FMT.format_roots(self.manifest)
        with mock.patch.object(FMT, "MAX_MANIFESTS", 0):
            with self.assertRaises(FMT.FormatError):
                FMT.format_roots(self.manifest)

    def test_one_literal_root_per_command_preserves_override_and_checks_after_failure(self):
        roots = [(self.root / ("source " + chr(0x1f9ed) + " ;$literal.rs"), "2024"),
                 (self.root / "other.rs", "2021")]
        override = str(self.root / "owned formatter ;$.exe")
        with mock.patch.dict(os.environ, {"RUSTFMT": override}), mock.patch.object(
            FMT.subprocess, "run", side_effect=[subprocess.CompletedProcess([], 17),
                                               subprocess.CompletedProcess([], 0)]
        ) as child, contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(FMT.check_roots(roots), 17)
        self.assertEqual([c.args[0] for c in child.call_args_list], [
            [override, str(path), "--edition", edition, "--check"] for path, edition in roots])
        self.assertTrue(all(not c.kwargs.get("shell", False) for c in child.call_args_list))

    def test_metadata_is_offline_locked_and_does_not_run_builds(self):
        def child(command, **options):
            self.assertEqual(command, ["cargo", "metadata", "--offline", "--locked",
                                      "--no-deps", "--format-version", "1", "--manifest-path",
                                      str(self.manifest)])
            options["stdout"].write(b'{"packages":[]}')
            return subprocess.CompletedProcess(command, 0)
        with mock.patch.object(FMT.subprocess, "run", side_effect=child):
            self.assertEqual(FMT.cargo_metadata(self.manifest), {"packages": []})

    def test_metadata_capacity_invalid_json_and_original_failure_are_not_success(self):
        for payload in [b"x" * (FMT.MAX_METADATA_BYTES + 1), b"not-json",
                        b'{"packages":[],"packages":[]}']:
            def child(command, **options):
                options["stdout"].write(payload)
                return subprocess.CompletedProcess(command, 0)
            with self.subTest(size=len(payload)), mock.patch.object(FMT.subprocess, "run", side_effect=child):
                with self.assertRaises(FMT.FormatError):
                    FMT.cargo_metadata(self.manifest)
        with mock.patch.object(FMT.subprocess, "run", return_value=subprocess.CompletedProcess([], 101)):
            with self.assertRaises(FMT.FormatError) as error:
                FMT.cargo_metadata(self.manifest)
            self.assertEqual(error.exception.returncode, 101)

    def test_check_is_explicit_and_arbitrary_formatter_flags_are_not_accepted(self):
        for arguments in [[], ["--check", "--config", "skip_children=true"]]:
            with contextlib.redirect_stderr(io.StringIO()):
                with self.assertRaises(SystemExit) as error:
                    FMT.main(arguments)
            self.assertEqual(error.exception.code, 2)


class ActualCargoFormatting(unittest.TestCase):
    def test_actual_cargo_and_rustfmt_reach_modules_dependency_and_preserve_source(self):
        self.assertIsNotNone(shutil.which("cargo"), "Cargo is required by native formatting CI")
        self.assertIsNotNone(shutil.which("rustfmt"), "rustfmt is required by native formatting CI")
        with tempfile.TemporaryDirectory(prefix="native-format-actual-") as directory:
            root = Path(directory) / ("owned ;$ e\u0301 " + chr(0x1f9ed))
            package = root / "main"
            dependency = root / "dep"
            for p in [package / "src", dependency / "src", package / "tests"]:
                p.mkdir(parents=True)
            (root / "Cargo.toml").write_text('[workspace]\nmembers=["main"]\nexclude=["dep"]\nresolver="2"\n')
            (package / "Cargo.toml").write_text('[package]\nname="owned-main"\nversion="0.0.0"\nedition="2024"\n[dependencies]\nowned-dep={path="../dep"}\n')
            (dependency / "Cargo.toml").write_text('[package]\nname="owned-dep"\nversion="0.0.0"\nedition="2021"\n')
            (root / "rustfmt.toml").write_text("hard_tabs = true\n")
            (package / "src/lib.rs").write_text("mod child;\n")
            (package / "src/child.rs").write_text("pub fn child() {\n\tlet _value = 1;\n}\n")
            (package / "tests/owned ;$ test.rs").write_text("// owned test target\n")
            canary = root / "BUILD_MUST_NOT_RUN"
            (package / "build.rs").write_text('fn main() {\n\tstd::fs::write("../BUILD_MUST_NOT_RUN", "executed").unwrap();\n}\n')
            bad = dependency / "src/lib.rs"
            bad.write_bytes(b"pub fn dependency( ){ }\n")
            # Lock resolution is a separate explicit fixture preparation, never a build.
            prepared = subprocess.run(["cargo", "generate-lockfile", "--offline", "--manifest-path", str(root / "Cargo.toml")],
                                      capture_output=True, timeout=60)
            self.assertEqual(prepared.returncode, 0, prepared.stderr)
            before = {p: p.read_bytes() for p in root.rglob("*.rs")}
            canonical_command = ["cargo", "fmt", "--all", "--check", "--manifest-path", str(root / "Cargo.toml")]
            canonical = subprocess.run(canonical_command, capture_output=True, timeout=120)
            self.assertNotEqual(canonical.returncode, 0)
            command = [sys.executable, "-X", "utf8", str(SPEC.origin), "--check",
                       "--manifest-path", str(root / "Cargo.toml")]
            result = subprocess.run(command, capture_output=True, timeout=120)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn(b"4 roots, 1 failed", result.stdout)
            self.assertEqual(before, {p: p.read_bytes() for p in before})
            bad.write_bytes(b"pub fn dependency() {}\n")
            # A non-root child module must also be checked through rustfmt recursion.
            child = package / "src/child.rs"
            child.write_bytes(b"pub fn child( ){ }\n")
            result = subprocess.run(command, capture_output=True, timeout=120)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn(b"4 roots, 1 failed", result.stdout)
            child.write_bytes(b"pub fn child() {}\n")
            result = subprocess.run(command, capture_output=True, timeout=120)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn(b"4 roots, 0 failed", result.stdout)
            canonical = subprocess.run(canonical_command, capture_output=True, timeout=120)
            self.assertEqual(canonical.returncode, 0, canonical.stdout + canonical.stderr)
            self.assertFalse(canary.exists())
            self.assertFalse((root / "target").exists())
            self.assertFalse((dependency / "target").exists())


if __name__ == "__main__":
    unittest.main()
