import importlib.util
import os
from pathlib import Path
import subprocess
import sys
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


if __name__ == "__main__":
    unittest.main()
