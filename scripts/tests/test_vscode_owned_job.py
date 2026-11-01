"""Pure admission tests; no controller import, process launch or image read."""
import ast
import json
from pathlib import Path, PureWindowsPath
import re
import unittest


def pure_functions():
    path = Path(__file__).resolve().parents[1] / "vscode_owned_job.py"
    if not path.is_file():
        return {}
    tree = ast.parse(path.read_text(encoding="utf-8"))
    names = {"require", "unique_object", "reject_constant", "decode", "windows_path", "admit_spec", "code_arguments", "admit_receipt", "close_handles"}
    selected = [node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name in names]
    namespace = {"json": json, "PureWindowsPath": PureWindowsPath, "re": re}
    exec(compile(ast.Module(body=selected, type_ignores=[]), "<pure-owned-fixtures>", "exec"), namespace)
    return namespace


class Admission(unittest.TestCase):
    def setUp(self):
        self.functions = pure_functions()

    def call(self, name, *args):
        self.assertTrue(name in self.functions, "Owned VS Code process-proof admission is missing: " + name)
        return self.functions[name](*args)

    def receipt(self):
        return {"schemaVersion": 1, "startedSuspended": True, "assignedBeforeResume": True,
                "exitCode": 0, "activeAtRootExit": 0, "activeBeforeCleanup": 0,
                "treeExitedBeforeCleanup": True, "activeAfterCleanup": 0,
                "handlesClosed": True, "cleanupComplete": True, "complete": True,
                "reason": "success"}

    def spec(self):
        return {"schemaVersion": 1, "fixtureRoot": r"C:\owned fixture π",
                "program": r"C:\selected editor\Code.exe", "repositoryRoot": r"C:\own source",
                "extensionDirectory": r"C:\selected package\extension", "timeoutSeconds": 180}

    def test_passed_marker_cannot_override_surviving_child(self):
        value = self.receipt()
        value.update(activeBeforeCleanup=1, treeExitedBeforeCleanup=False)
        with self.assertRaises(ValueError):
            self.call("admit_receipt", value)

    def test_natural_success_requires_complete_independent_proofs(self):
        self.call("admit_receipt", self.receipt())
        for key in ("startedSuspended", "assignedBeforeResume", "treeExitedBeforeCleanup",
                    "handlesClosed", "cleanupComplete", "complete"):
            for invalid in (False, 1, None):
                with self.subTest(key=key, invalid=invalid), self.assertRaises(ValueError):
                    value = self.receipt()
                    value[key] = invalid
                    self.call("admit_receipt", value)

    def test_nonzero_exit_and_forced_empty_tree_do_not_pass(self):
        for key, invalid in (("exitCode", 7), ("exitCode", True), ("activeBeforeCleanup", True),
                             ("activeAfterCleanup", 1), ("activeAtRootExit", 0x100000000), ("reason", "deadline")):
            with self.subTest(key=key, invalid=invalid), self.assertRaises(ValueError):
                value = self.receipt()
                value[key] = invalid
                self.call("admit_receipt", value)

    def test_transient_accounting_at_root_exit_may_settle_before_cleanup(self):
        value = self.receipt()
        value["activeAtRootExit"] = 1
        self.call("admit_receipt", value)

    def test_receipts_refuse_missing_and_unknown_fields(self):
        for key in self.receipt():
            value = self.receipt()
            del value[key]
            with self.subTest(key=key), self.assertRaises(ValueError):
                self.call("admit_receipt", value)
        value = self.receipt()
        value["resultMarkerPassed"] = True
        with self.assertRaises(ValueError):
            self.call("admit_receipt", value)

    def test_unicode_space_paths_remain_separate_fixed_arguments(self):
        spec = self.call("admit_spec", self.spec())
        arguments = self.call("code_arguments", spec)
        self.assertEqual(arguments, ["--new-window", "--wait", "--disable-extensions", "--disable-updates",
            "--skip-welcome", "--skip-release-notes", "--user-data-dir", r"C:\owned fixture π\profile",
            "--extensions-dir", r"C:\owned fixture π\extensions", "--extensionDevelopmentPath",
            r"C:\selected package\extension", "--extensionTestsPath",
            r"C:\own source\extensions\vscode\test-host.cjs", r"C:\owned fixture π\workspace"])

    def test_cleanup_attempts_remaining_handles_after_a_failure(self):
        seen = []
        def close(handle):
            seen.append(handle)
            return handle != 2
        self.assertFalse(self.call("close_handles", (1, None, 2, 3), close))
        self.assertEqual(seen, [1, 2, 3])

    def test_cleanup_attempts_remaining_handles_after_an_exception(self):
        seen = []
        def close(handle):
            seen.append(handle)
            if handle == 2:
                raise OSError("owned synthetic close refusal")
            return True
        self.assertFalse(self.call("close_handles", (1, 2, 3), close))
        self.assertEqual(seen, [1, 2, 3])

    def test_successful_cleanup_does_not_close_absent_handles(self):
        seen = []
        self.assertTrue(self.call("close_handles", (None, 1, 2), lambda handle: seen.append(handle) is None))
        self.assertEqual(seen, [1, 2])

    def test_spec_refuses_expanded_deadlines_bools_and_schema_changes(self):
        for key, invalid in (("timeoutSeconds", 0), ("timeoutSeconds", 181), ("timeoutSeconds", True),
                             ("timeoutSeconds", 0.5), ("schemaVersion", True), ("schemaVersion", 2)):
            with self.subTest(key=key, invalid=invalid), self.assertRaises(ValueError):
                value = self.spec()
                value[key] = invalid
                self.call("admit_spec", value)

    def test_spec_refuses_unknown_or_omitted_selection(self):
        value = self.spec()
        value["arguments"] = ["--disable-sandbox"]
        with self.assertRaises(ValueError):
            self.call("admit_spec", value)
        for key in self.spec():
            value = self.spec()
            del value[key]
            with self.subTest(key=key), self.assertRaises(ValueError):
                self.call("admit_spec", value)

    def test_spec_requires_selected_code_exe_outside_private_fixture(self):
        for program in (r"C:\editor\cmd.exe", r"C:\owned fixture π\Code.exe",
                        r"C:\OWNED FIXTURE Π\sub\Code.exe"):
            value = self.spec()
            value["program"] = program
            with self.subTest(program=program), self.assertRaises(ValueError):
                self.call("admit_spec", value)

    def test_windows_paths_refuse_traversal_unc_ads_devices_aliases_and_controls(self):
        cases = ["relative", r"\\server\share\Code.exe", r"C:relative", r"C:\x\..\Code.exe",
                 r"C:\x\.\Code.exe", r"C:\x\Code.exe:stream", r"C:\x\CON.txt", r"C:\x\LPT¹.txt",
                 "C:\\x\\Code.exe.", "C:\\x\\Code.exe ", "C:\\x\\bad\nname", "C:\\x\\bad\ud800"]
        for value in cases:
            with self.subTest(value=ascii(value)), self.assertRaises(ValueError):
                self.call("windows_path", value)

    def test_json_refuses_duplicate_fields_constants_and_capacity(self):
        for raw in (b'{"a":1,"a":2}', b'{"a":NaN}', b'{"a":Infinity}', b' ' * 32769):
            with self.subTest(raw=raw[:30]), self.assertRaises(ValueError):
                self.call("decode", raw)
        self.assertEqual(self.call("decode", b'{"a":1}'), {"a": 1})


if __name__ == "__main__":
    unittest.main()
