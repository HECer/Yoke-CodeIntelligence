import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import tomllib
import unittest
from unittest import mock

SPEC = importlib.util.spec_from_file_location(
    "native_test_runner", Path(__file__).resolve().parents[1] / "run_native_tests.py"
)
RUNNER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(RUNNER)


class NativeTestEnvironment(unittest.TestCase):
    def run_child(self, arguments, expression):
        real_run = subprocess.run
        selected = {name: "owned-ambient-override" for name in RUNNER.TLS_OVERRIDES}
        selected["YOKE_OWNED_ENV_CANARY"] = "retained-child-data"
        with mock.patch.dict(os.environ, selected):
            before = dict(os.environ)

            def cargo(command, *, env):
                self.assertEqual(command, ["cargo", "test", *arguments])
                self.assertEqual(env, {
                    k: v for k, v in before.items() if k not in RUNNER.TLS_OVERRIDES
                })
                return real_run([sys.executable, "-I", "-c", expression], env=env)

            with mock.patch.object(RUNNER.subprocess, "run", side_effect=cargo):
                result = RUNNER.main(arguments)
            self.assertEqual(dict(os.environ), before)
        return result

    def test_actual_child_loses_only_tls_overrides_and_parent_keeps_all_values(self):
        result = self.run_child(
            ["--locked", "--test", "owned name;$literal", "--", "--test-threads=2"],
            "import os; assert all(k not in os.environ for k in "
            "('SSL_CERT_FILE','SSL_CERT_DIR','SSLKEYLOGFILE')); "
            "assert os.environ['YOKE_OWNED_ENV_CANARY']=='retained-child-data'",
        )
        self.assertEqual(result, 0)

    def test_real_child_failure_status_reaches_the_caller(self):
        self.assertEqual(self.run_child(["--lib"], "raise SystemExit(17)"), 17)


class NativeExecutableBoundary(unittest.TestCase):
    def environment(self):
        result = os.environ.copy()
        result.update({name: "owned-ambient-override" for name in RUNNER.TLS_OVERRIDES})
        result["YOKE_OWNED_ENV_CANARY"] = "retained-child-data"
        result["CARGO_NET_OFFLINE"] = "true"
        return result

    def test_actual_executable_mode_handles_unicode_paths_and_literal_arguments(self):
        with tempfile.TemporaryDirectory(prefix="ci-runner-") as directory:
            script = Path(directory) / "runner owned ;$ e\u0301-\U0001f9ed.py"
            shutil.copyfile(SPEC.origin, script)
            arguments = ["owned name;$literal", "\u03c0 e\u0301", "--test-threads=2"]
            expression = (
                "import os,sys,json; "
                "assert all(k not in os.environ for k in "
                "('SSL_CERT_FILE','SSL_CERT_DIR','SSLKEYLOGFILE')); "
                "assert os.environ['YOKE_OWNED_ENV_CANARY']=='retained-child-data'; "
                "print(json.dumps(sys.argv[1:]))"
            )
            result = subprocess.run(
                [sys.executable, "-I", str(script), "--test-executable",
                 sys.executable, "-I", "-c", expression, *arguments],
                env=self.environment(), capture_output=True, text=True, timeout=30,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(json.loads(result.stdout), arguments)

    def test_actual_executable_failure_is_not_changed_to_success(self):
        result = subprocess.run(
            [sys.executable, "-I", str(SPEC.origin), "--test-executable",
             sys.executable, "-I", "-c", "raise SystemExit(17)"],
            env=self.environment(), capture_output=True, text=True, timeout=30,
        )
        self.assertEqual(result.returncode, 17, result.stderr)

    def test_post_cargo_reinjection_is_removed_only_at_the_executable_boundary(self):
        real_run = subprocess.run
        arguments = ["--locked", "--test", "owned name;$literal", "--", "--test-threads=2"]
        with mock.patch.dict(os.environ, self.environment()):
            before = dict(os.environ)

            def cargo(command, **options):
                if command == ["rustc", "-vV"]:
                    return subprocess.CompletedProcess(command, 0, "host: x86_64-pc-windows-msvc\n", "")
                self.assertEqual(command[0], "cargo")
                position = command.index("--config")
                configuration = tomllib.loads(command[position + 1])
                runner = configuration["target"]["x86_64-pc-windows-msvc"]["runner"]
                self.assertEqual(command[position + 2:], ["test", *arguments])
                injected = dict(options["env"])
                injected.update({name: "" for name in RUNNER.TLS_OVERRIDES})
                expression = (
                    "import os; assert all(k not in os.environ for k in "
                    "('SSL_CERT_FILE','SSL_CERT_DIR','SSLKEYLOGFILE')); "
                    "assert os.environ['YOKE_OWNED_ENV_CANARY']=='retained-child-data'"
                )
                return real_run(
                    [*runner, sys.executable, "-I", "-c", expression],
                    env=injected, capture_output=True, text=True, timeout=30,
                )

            with mock.patch.object(RUNNER.subprocess, "run", side_effect=cargo):
                result = RUNNER.main(["--controlled-test-runner", *arguments])
            self.assertEqual(result, 0)
            self.assertEqual(dict(os.environ), before)

    def test_controlled_mode_refuses_runner_authority_and_target_overrides(self):
        def cargo(command, **options):
            self.assertEqual(command, ["rustc", "-vV"])
            return subprocess.CompletedProcess(command, 0, "host: x86_64-pc-windows-msvc\n", "")

        with tempfile.TemporaryDirectory(prefix="ci-runner-policy-") as directory:
            root = Path(directory)
            (root / ".cargo").mkdir()
            config = root / ".cargo/config.toml"
            for text in [
                '[target.x86_64-pc-windows-msvc]\nrunner=["owned-sandbox"]\n',
                '[env]\nCARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUNNER="owned-sandbox"\n',
                '[env]\nCARGO_BUILD_TARGET="owned-host"\n',
                'include="owned-policy.toml"\n',
                'owned="' + 'x' * 65_536 + '"\n',
            ]:
                config.write_text(text, encoding="utf8")
                with mock.patch.object(Path, "cwd", return_value=root), mock.patch.object(
                    RUNNER.subprocess, "run", side_effect=cargo
                ):
                    self.assertEqual(RUNNER.main(["--controlled-test-runner", "--lib"]), 2)
            for arguments in [["--target", "owned-host"], ["--config=owned-policy.toml"]]:
                with mock.patch.object(RUNNER.subprocess, "run") as child:
                    self.assertEqual(RUNNER.main(["--controlled-test-runner", *arguments]), 2)
                    child.assert_not_called()
            with mock.patch.dict(os.environ, {"CARGO_TARGET_OWNED_HOST_RUNNER": "owned-sandbox"}):
                with mock.patch.object(RUNNER.subprocess, "run") as child:
                    self.assertEqual(RUNNER.main(["--controlled-test-runner", "--lib"]), 2)
                    child.assert_not_called()

    def test_real_cargo_reinjection_reproduces_legacy_failure_and_controlled_success(self):
        self.assertIsNotNone(shutil.which("cargo"), "Rust is required for this live Cargo contract")
        with tempfile.TemporaryDirectory(prefix="ci-cargo-tls-") as directory:
            root = Path(directory)
            (root / "src").mkdir()
            (root / ".cargo").mkdir()
            (root / "Cargo.toml").write_text(
                '[package]\nname="owned-tls-fixture"\nversion="0.0.0"\nedition="2024"\n'
                '[workspace]\n[profile.dev]\ndebug=0\nincremental=false\n', encoding="utf8"
            )
            (root / "src/lib.rs").write_text(
                '#[test]\nfn no_ambient_tls() {\n'
                'for key in ["SSL_CERT_FILE","SSL_CERT_DIR","SSLKEYLOGFILE"] {'
                'assert!(std::env::var_os(key).is_none(), "{key}");}\n'
                'assert_eq!(std::env::var("YOKE_OWNED_ENV_CANARY").unwrap(), "retained-child-data");\n'
                '}\n', encoding="utf8"
            )
            (root / ".cargo/config.toml").write_text(
                '[env]\n' + "".join(
                    f'{name}={{value="owned-post-cargo-injection",force=true}}\n'
                    for name in RUNNER.TLS_OVERRIDES
                ), encoding="utf8"
            )
            environment = self.environment()
            environment["CARGO_TARGET_DIR"] = str(root / "target")
            script = root / "runner owned ;$ e\u0301-\U0001f9ed.py"
            shutil.copyfile(SPEC.origin, script)
            common = [sys.executable, "-I", str(script)]
            options = dict(cwd=root, env=environment, capture_output=True, text=True, timeout=90)
            legacy = subprocess.run([*common, "--offline", "--lib"], **options)
            self.assertNotEqual(legacy.returncode, 0)
            self.assertIn("no_ambient_tls", legacy.stdout)
            controlled = subprocess.run(
                [*common, "--controlled-test-runner", "--offline", "--lib"], **options
            )
            self.assertEqual(controlled.returncode, 0, controlled.stdout + controlled.stderr)
            self.assertIn("1 passed; 0 failed", controlled.stdout)


if __name__ == "__main__":
    unittest.main()
