"""Inert source-extracted admission checks; no controller/native/editor run."""
import ast
import copy
import json
from pathlib import Path, PureWindowsPath
import re
import stat
import struct
import unittest
import zipfile


def functions():
    scripts = Path(__file__).resolve().parents[1]
    namespace = {"json": json, "PureWindowsPath": PureWindowsPath, "re": re, "stat": stat, "struct": struct}
    sources = [(scripts / "vscode_owned_job.py", {"require", "admit_receipt"}),
               (scripts / "host_vscode.py", {"require", "unique_object", "reject_constant", "decode", "admit_lifecycle", "producer_role", "safe_members", "admit_host_report", "admit_state", "zip_directory", "admit_stage_result"})]
    for path, names in sources:
        if not path.is_file():
            continue
        tree = ast.parse(path.read_bytes())
        selected = [n for n in tree.body if isinstance(n, ast.FunctionDef) and n.name in names]
        exec(compile(ast.Module(body=selected, type_ignores=[]), "<inert-vsix-admission>", "exec"), namespace)
        namespace["admit_job"] = namespace["admit_receipt"]
    return namespace


class Admission(unittest.TestCase):
    def setUp(self):
        self.functions = functions()

    def call(self, name, *args):
        self.assertTrue(name in self.functions, "Fresh VSIX admission is missing: " + name)
        return self.functions[name](*args)

    def job(self):
        return {"schemaVersion": 1, "startedSuspended": True, "assignedBeforeResume": True,
                "exitCode": 0, "activeAtRootExit": 0, "activeBeforeCleanup": 0,
                "treeExitedBeforeCleanup": True, "activeAfterCleanup": 0,
                "handlesClosed": True, "cleanupComplete": True, "complete": True, "reason": "success"}

    def lifecycle(self):
        rows = []
        for scenario in ("success", "nonzero", "orphan", "deadline"):
            proof = self.job()
            if scenario != "success":
                proof.update(complete=False, reason={"nonzero": "root_exit", "orphan": "tree_alive", "deadline": "deadline"}[scenario])
            if scenario == "nonzero":
                proof["exitCode"] = 7
            if scenario == "orphan":
                proof.update(activeBeforeCleanup=1, treeExitedBeforeCleanup=False)
            if scenario == "deadline":
                proof.update(exitCode=None, activeAtRootExit=None, activeBeforeCleanup=None, treeExitedBeforeCleanup=False)
            rows.append({"scenario": scenario, "processProof": proof})
        return {"schemaVersion": 1, "sourceRevision": "a" * 40, "complete": True,
                "scope": "Owned Python Job lifecycle probes only", "cases": rows,
                "VSCodeOrYokeExecuted": False, "VSIXOrReleaseAccepted": False}

    def producer(self):
        row = {"reason": "compiler-artifact", "package_id": "native-package",
               "target": {"name": "yoke", "kind": ["bin"], "src_path": r"C:\owned\native\src\main.rs"},
               "profile": {"test": False}, "fresh": False,
               "executable": r"C:\owned\target\release\yoke.exe",
               "filenames": [r"C:\owned\target\release\yoke.exe"]}
        return [row, {"reason": "build-finished", "success": True}]

    def producer_bytes(self, rows):
        return b"\n".join(json.dumps(r).encode() for r in rows) + b"\n"

    def report(self):
        return {"kind": "actual-vscode-extension-host", "passed": True, "runtime": "rust",
                "bundledRuntime": False, "vscodeVersion": "1.143.1", "unsavedSourceBound": True,
                "workspaceExecutableIgnored": True, "protectedOpenBufferExcluded": True, "savedSourceUnchanged": True}

    def test_stale_lifecycle_cannot_authorize_new_source(self):
        with self.assertRaises(ValueError):
            self.call("admit_lifecycle", self.lifecycle(), "b" * 40)

    def test_complete_matching_four_case_lifecycle_is_required(self):
        self.call("admit_lifecycle", self.lifecycle(), "a" * 40)
        for scenario in range(4):
            value = self.lifecycle()
            value["cases"][scenario]["processProof"]["cleanupComplete"] = False
            with self.subTest(scenario=scenario), self.assertRaises(ValueError):
                self.call("admit_lifecycle", value, "a" * 40)

    def test_duplicate_missing_and_forged_probe_success_refuse(self):
        values = []
        value = self.lifecycle(); value["cases"][1] = copy.deepcopy(value["cases"][0]); values.append(value)
        value = self.lifecycle(); value["cases"].pop(); values.append(value)
        value = self.lifecycle(); value["cases"][2]["processProof"]["complete"] = True; values.append(value)
        value = self.lifecycle(); value["VSIXOrReleaseAccepted"] = True; values.append(value)
        for value in values:
            with self.assertRaises(ValueError):
                self.call("admit_lifecycle", value, "a" * 40)

    def test_negative_lifecycle_schema_unknown_fields_and_forged_deadline_refuse(self):
        for index, key, invalid in ((1, "schemaVersion", 2), (2, "forged", True), (3, "exitCode", 0)):
            value = self.lifecycle()
            value["cases"][index]["processProof"][key] = invalid
            with self.subTest(index=index, key=key), self.assertRaises(ValueError):
                self.call("admit_lifecycle", value, "a" * 40)

    def test_producer_partial_fields_and_wrong_test_profile_refuse(self):
        for key, invalid in (("target", None), ("profile", {"test": True}), ("filenames", None)):
            rows = self.producer()
            rows[0][key] = invalid
            with self.subTest(key=key), self.assertRaises(ValueError):
                self.call("producer_role", self.producer_bytes(rows), PureWindowsPath(r"C:\owned"), "native-package")

    def test_native_host_result_cannot_drop_assertions_or_add_authority(self):
        for key in self.report():
            value = self.report()
            del value[key]
            with self.subTest(key=key), self.assertRaises(ValueError):
                self.call("admit_host_report", value, False)
        value = self.report(); value["allowExecution"] = True
        with self.assertRaises(ValueError):
            self.call("admit_host_report", value, False)

    def state(self):
        return {"schemaVersion": 1, "sourceRevision": "a" * 40, "toolRoot": r"D:\temporary\yvs-aaaaaaaaaaaaaaaa",
                "compilerHomes": {"CARGO_HOME": r"C:\tools\cargo", "RUSTUP_HOME": r"C:\tools\rustup"},
                "workerRoots": {"tool": r"C:\owned\validation\ci-vscode-workers\tool", "package": r"C:\owned\validation\ci-vscode-workers\package",
                                "explicit": r"D:\temporary\yvh-explicit-aaaaaaaaaaaaaaaa", "bundled": r"D:\temporary\yvh-bundled-bbbbbbbbbbbbbbbb"}}

    def test_real_host_fixture_roots_cannot_inherit_the_checkout(self):
        value = self.state()
        value["workerRoots"]["explicit"] = r"C:\owned\validation\ci-vscode-workers\explicit"
        with self.assertRaises(ValueError):
            self.call("admit_state", value, "a" * 40, PureWindowsPath(r"C:\owned"), PureWindowsPath(r"D:\temporary"))

    def test_state_binds_fresh_fixed_roles_and_host_selected_compiler_homes(self):
        self.call("admit_state", self.state(), "a" * 40, PureWindowsPath(r"C:\owned"), PureWindowsPath(r"D:\temporary"))
        for role in ("tool", "package", "explicit", "bundled"):
            value = self.state(); value["workerRoots"][role] = r"D:\foreign\role"
            with self.subTest(role=role), self.assertRaises(ValueError):
                self.call("admit_state", value, "a" * 40, PureWindowsPath(r"C:\owned"), PureWindowsPath(r"D:\temporary"))

    def test_fresh_successful_fixed_release_role_is_admitted(self):
        row = self.call("producer_role", self.producer_bytes(self.producer()), PureWindowsPath(r"C:\owned"), "native-package")
        self.assertEqual(row["path"], r"C:\owned\target\release\yoke.exe")

    def test_stale_foreign_debug_or_duplicate_native_roles_refuse(self):
        values = []
        for key, value in (("fresh", True), ("executable", r"C:\owned\target\debug\yoke.exe"), ("package_id", "foreign")):
            rows = self.producer(); rows[0][key] = value; values.append(rows)
        rows = self.producer(); rows.insert(0, copy.deepcopy(rows[0])); values.append(rows)
        rows = self.producer(); rows[-1]["success"] = False; values.append(rows)
        rows = self.producer(); rows.append({"reason": "compiler-artifact"}); values.append(rows)
        for rows in values:
            with self.assertRaises(ValueError):
                self.call("producer_role", self.producer_bytes(rows), PureWindowsPath(r"C:\owned"), "native-package")

    def member(self, name, size=1):
        member = zipfile.ZipInfo(name)
        member.file_size = size
        return member

    def test_zip_inventory_refuses_traversal_aliases_links_and_conflicts(self):
        values = [[self.member("../outside")], [self.member("C:/outside")], [self.member("a\\b")],
                  [self.member("Code.exe"), self.member("code.exe")], [self.member("a"), self.member("a/b")],
                  [self.member("CON.txt")], [self.member("a/../b")], [self.member("trailing. ")]]
        linked = self.member("linked"); linked.external_attr = (stat.S_IFLNK | 0o777) << 16; values.append([linked])
        encrypted = self.member("encrypted"); encrypted.flag_bits = 1; values.append([encrypted])
        unbounded_codec = self.member("dictionary-codec"); unbounded_codec.compress_type = 14; values.append([unbounded_codec])
        for members in values:
            with self.assertRaises(ValueError):
                self.call("safe_members", members, 100, 100, 20)

    def test_zip64_directory_override_refuses_before_zip_decoder(self):
        locator = b"PK\x06\x07" + bytes(16)
        end = struct.pack("<4s4H2LH", b"PK\x05\x06", 0, 0, 1, 1, 20, 0, 0)
        with self.assertRaises(ValueError):
            self.call("zip_directory", locator + end, 0, 42, 20)

    def test_zip_directory_honors_comment_framing_and_count_bounds(self):
        end = struct.pack("<4s4H2LH", b"PK\x05\x06", 0, 0, 1, 1, 0, 0, 3) + b"own"
        self.assertEqual(self.call("zip_directory", end, 0, len(end), 20), 1)
        for raw, count in ((end + b"x", 20), (end, 0), (b"missing", 20)):
            with self.assertRaises(ValueError):
                self.call("zip_directory", raw, 0, len(raw), count)

    def test_ambiguous_comment_footer_cannot_override_checked_directory_budget(self):
        fake = struct.pack("<4s4H2LH", b"PK\x05\x06", 0, 0, 1, 1, 0x7fffffff, 0, 1)
        outer = struct.pack("<4s4H2LH", b"PK\x05\x06", 0, 0, 1, 1, 0, 0, len(fake))
        with self.assertRaises(ValueError):
            self.call("zip_directory", outer + fake, 0, len(outer + fake), 20)

    def test_zip_inventory_binds_count_entry_and_cumulative_limits(self):
        for members, total, entry, count in (([self.member("a", 2)], 1, 10, 2),
                ([self.member("a", 2)], 10, 1, 2), ([self.member("a"), self.member("b")], 10, 10, 1)):
            with self.assertRaises(ValueError):
                self.call("safe_members", members, total, entry, count)
        self.assertEqual(set(self.call("safe_members", [self.member("owned π/file.txt")], 100, 100, 20)), {"owned π/file.txt"})

    def test_host_report_requires_original_scope_and_all_flags(self):
        self.call("admit_host_report", self.report(), False)
        for key, value in (("passed", "true"), ("vscodeVersion", "1.143.1-insider"), ("bundledRuntime", True),
                           ("savedSourceUnchanged", False), ("runtime", "typescript")):
            report = self.report(); report[key] = value
            with self.subTest(key=key), self.assertRaises(ValueError):
                self.call("admit_host_report", report, False)

    def test_json_capacity_duplicate_and_nonfinite_values_refuse(self):
        for raw in (b'{"source":1,"source":2}', b'{"number":NaN}', b' ' * 1000001):
            with self.assertRaises(ValueError):
                self.call("decode", raw)

    def stage_result(self):
        return {"schemaVersion": 1, "sourceRevision": "a" * 40, "phase": "tool", "complete": True,
                "workerAdmission": {"specificJobMembership": True, "queryHandleClosedBeforePayloadRead": True},
                "nativeSHA256": "b" * 64, "sourceInputsUnchanged": True, "builtInputsUnchanged": True, "observation": {}}

    def test_stage_result_requires_matching_source_phase_and_closed_query_handle(self):
        self.call("admit_stage_result", self.stage_result(), "tool", "a" * 40)
        for key, invalid in (("sourceRevision", "c" * 40), ("phase", "bundled"), ("complete", 1),
                             ("nativeSHA256", "b" * 63), ("sourceInputsUnchanged", False), ("schemaVersion", True)):
            value = self.stage_result(); value[key] = invalid
            with self.subTest(key=key), self.assertRaises(ValueError):
                self.call("admit_stage_result", value, "tool", "a" * 40)

    def test_stage_result_cannot_omit_or_forge_worker_admission(self):
        for key in self.stage_result():
            value = self.stage_result(); del value[key]
            with self.subTest(key=key), self.assertRaises(ValueError):
                self.call("admit_stage_result", value, "tool", "a" * 40)
        for invalid in (False, 1, None):
            value = self.stage_result(); value["workerAdmission"]["queryHandleClosedBeforePayloadRead"] = invalid
            with self.subTest(invalid=invalid), self.assertRaises(ValueError):
                self.call("admit_stage_result", value, "tool", "a" * 40)


if __name__ == "__main__":
    unittest.main()
