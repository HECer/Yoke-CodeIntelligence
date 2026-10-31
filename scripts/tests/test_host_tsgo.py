"""Pure source-extracted vendor admission tests; no controller or native import."""
import ast
import copy
import hashlib
import json
from pathlib import Path, PureWindowsPath
import re
import unittest
import tempfile
from types import SimpleNamespace


def functions():
    path = Path(__file__).resolve().parents[1] / "host_tsgo.py"
    namespace = {"hashlib": hashlib, "json": json, "PureWindowsPath": PureWindowsPath, "re": re}
    if path.is_file():
        tree = ast.parse(path.read_bytes())
        names = {"require", "admit_members", "admit_package", "fixture_sources", "token_spans",
                 "utf16_range", "admit_reply", "admit_state", "admit_result", "query"}
        selected = [n for n in tree.body if isinstance(n, ast.FunctionDef) and n.name in names]
        exec(compile(ast.Module(body=selected, type_ignores=[]), "<inert-tsgo-admission>", "exec"), namespace)
    return namespace


class VendorAdmission(unittest.TestCase):
    def test_actual_query_orchestration_uses_scoped_lsp_route_and_native_byte_budget(self):
        namespace = functions()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            fixture, tool = root / "fixture", root / "tool"
            fixture.mkdir()
            tool.mkdir()

            def write_json(path, value):
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(json.dumps(value), encoding="utf-8")

            def command(argv, workspace, logs, name, timeout):
                self.assertEqual(timeout, 120)
                self.assertEqual(argv[argv.index("--max-bytes") + 1], "64000")
                self.assertEqual(argv[argv.index("--timeout") + 1], "100000")
                requests = json.loads((fixture / "requests.json").read_text(encoding="utf-8"))
                self.assertEqual(len(requests), 3)
                self.assertEqual([r["args"]["arguments"]["languageServer"]["operation"] for r in requests],
                                 ["hover", "definition", "references"])
                for request in requests:
                    self.assertEqual(request["operation"], "queryProject")
                    self.assertEqual(set(request["args"]), {"project", "tool", "arguments"})
                    self.assertEqual(request["args"]["project"], "default")
                    self.assertEqual(request["args"]["tool"], "ci_find")
                    arguments = request["args"]["arguments"]
                    self.assertEqual(set(arguments), {"languageServer", "maxBytes"})
                    self.assertEqual(arguments["maxBytes"], 64000)
                return b"{}"

            namespace.update(V=SimpleNamespace(write_json=write_json, decode=json.loads,
                read_bytes=lambda path, limit: path.read_bytes()), command=command,
                OUTPUT=root / "output", admit_reply=lambda *args: {"hoverRangeChecked": True})
            result = namespace["query"](tool, fixture, root / "inert-native.exe", "a" * 64)
            self.assertTrue(result["savedFixtureSourceUnchanged"])

    def call(self, name, *args):
        self.assertIn(name, functions(), "Real TSGo admission is not implemented")
        return functions()[name](*args)

    def members(self):
        return [{"name": "package/package.json", "bytes": 20, "kind": "file"},
                {"name": "package/lib/tsgo.exe", "bytes": 40, "kind": "file"},
                {"name": "package/lib/lib.d.ts", "bytes": 30, "kind": "file"}]

    def test_complete_regular_package_inventory_is_required(self):
        self.assertEqual(len(self.call("admit_members", self.members(), 3, 90)), 3)
        for count, size in [(2, 90), (3, 91), (True, 90), (3, False)]:
            with self.assertRaises(ValueError):
                self.call("admit_members", self.members(), count, size)

    def test_archive_paths_devices_unicode_aliases_and_escapes_refuse(self):
        for name in ["../tsgo.exe", "/package/file", "package/../file", "package/x:ads", "package/CON.txt",
                     "package/a\\b", "package/a. ", "package//file", "package/lib/LPT1", "package/x\0y"]:
            value = self.members(); value[0]["name"] = name
            with self.subTest(name=name), self.assertRaises(ValueError):
                self.call("admit_members", value, 3, 90)

    def test_links_special_entries_duplicates_and_parent_file_refuse(self):
        for kind in ["symlink", "hardlink", "device", "fifo"]:
            value = self.members(); value[0]["kind"] = kind
            with self.assertRaises(ValueError):
                self.call("admit_members", value, 3, 90)
        for name in ["package/LIB/tsgo.exe", "package/lib", "package/lib/other.exe"]:
            value = self.members(); value[0]["name"] = name
            with self.assertRaises(ValueError):
                self.call("admit_members", value, 3, 90)

    def test_package_identity_is_platform_and_version_specific(self):
        valid = {"name": "@typescript/native-preview-win32-x64", "version": "7.0.0-dev.20260707.2", "os": ["win32"], "cpu": ["x64"]}
        self.call("admit_package", valid)
        for key, invalid in [("name", "native-preview"), ("version", "latest"), ("os", ["linux"]), ("cpu", ["arm64"])]:
            value = dict(valid); value[key] = invalid
            with self.assertRaises(ValueError):
                self.call("admit_package", value)

    def reply(self):
        sources = self.call("fixture_sources")
        entry, helper = "entry.ts", "src/Gr\u00fc\u00dfe e\u0301.ts"
        spans = self.call("token_spans", sources)
        snapshots = "owned-snapshot"
        def common():
            return {"snapshotId": snapshots, "producer": r"C:\runner\tsgo.exe", "complete": False,
                    "selectedBackendId": "typescript_tsgo", "sourceExecutionAuthorized": True,
                    "backendSelection": {"policy": "first_supporting_file_then_first_fallback", "matchedDeclaredExtension": True,
                                         "fallback": False, "runtimeCertification": False},
                    "serverProfileBinding": {"kind": "selected_command_bytes", "contentHash": "a" * 64}}
        hover = common(); hover["response"] = {"contents": {"kind": "markdown", "value": "function greet(name: string): string"},
            "range": self.call("utf16_range", sources[entry], spans[entry][-1][0], spans[entry][-1][1])}
        def location(path, span):
            return {"path": path, "startByte": span[0], "endByte": span[1], "contentHash": hashlib.sha256(sources[path]).hexdigest(),
                    "text": "greet", "producer": r"C:\runner\tsgo.exe", "resolution": "exact_binding", "trust": "source-data"}
        definition = common(); definition["locations"] = [location(helper, spans[helper][0])]
        references = common(); references["locations"] = [location(path, span) for path in (helper, entry) for span in spans[path]]
        return {"results": [{"status": "ok", "data": row} for row in (hover, definition, references)], "sourceWritten": False, "atomicBatch": False}

    def admit(self, value):
        return self.call("admit_reply", value, self.call("fixture_sources"), r"C:\runner\tsgo.exe", "a" * 64)

    def test_real_protocol_reply_requires_nonempty_hover_definition_and_references(self):
        self.admit(self.reply())
        for index, key in [(0, "response"), (1, "locations"), (2, "locations")]:
            value = self.reply(); value["results"][index]["data"][key] = None if index == 0 else []
            with self.assertRaises(ValueError):
                self.admit(value)

    def test_unicode_crlf_ranges_and_raw_source_hashes_are_checked(self):
        for key, invalid in [("startByte", 0), ("endByte", 0), ("contentHash", "b" * 64), ("path", "../outside.ts"), ("text", "other")]:
            value = self.reply(); value["results"][1]["data"]["locations"][0][key] = invalid
            with self.subTest(key=key), self.assertRaises(ValueError):
                self.admit(value)
        value = self.reply(); value["results"][0]["data"]["response"]["range"]["start"]["character"] = 0
        with self.assertRaises(ValueError):
            self.admit(value)

    def test_partial_reference_set_duplicate_and_foreign_source_refuse(self):
        value = self.reply(); value["results"][2]["data"]["locations"].pop()
        with self.assertRaises(ValueError):
            self.admit(value)
        value = self.reply(); value["results"][2]["data"]["locations"].append(copy.deepcopy(value["results"][2]["data"]["locations"][0]))
        with self.assertRaises(ValueError):
            self.admit(value)

    def test_provider_hash_snapshot_trust_and_runtime_scope_cannot_be_forged(self):
        for key, invalid in [("snapshotId", "stale"), ("producer", "other.exe"), ("complete", True),
                             ("selectedBackendId", "typescript"), ("sourceExecutionAuthorized", False)]:
            value = self.reply(); value["results"][1]["data"][key] = invalid
            with self.subTest(key=key), self.assertRaises(ValueError):
                self.admit(value)
        value = self.reply(); value["results"][1]["data"]["serverProfileBinding"]["contentHash"] = "b" * 64
        with self.assertRaises(ValueError):
            self.admit(value)
        value = self.reply(); value["results"][0]["status"] = "error"
        with self.assertRaises(ValueError):
            self.admit(value)

    def test_state_must_select_fresh_runner_temp_roles_outside_source(self):
        value = {"schemaVersion": 1, "sourceRevision": "a" * 40, "toolRoot": r"C:\temp\yts-0123456789abcdef",
                 "workerRoots": {"tool": r"C:\temp\ytw-tool-0123456789abcdef", "query": r"C:\temp\ytw-query-0123456789abcdef"}}
        self.call("admit_state", value, "a" * 40, PureWindowsPath(r"C:\source"), PureWindowsPath(r"C:\temp"))
        for key in ["toolRoot", "workerRoots"]:
            wrong = copy.deepcopy(value)
            if key == "toolRoot":
                wrong[key] = r"C:\source\package"
            else:
                wrong[key]["query"] = wrong[key]["tool"]
            with self.assertRaises(ValueError):
                self.call("admit_state", wrong, "a" * 40, PureWindowsPath(r"C:\source"), PureWindowsPath(r"C:\temp"))

    def result(self):
        return {"schemaVersion": 1, "sourceRevision": "a" * 40, "phase": "query", "complete": True,
                "workerAdmission": {"specificJobMembership": True, "queryHandleClosedBeforePayloadRead": True},
                "nativeSHA256": "b" * 64, "vendorVersion": "7.0.0-dev.20260707.2", "sourceInputsUnchanged": True,
                "builtInputsUnchanged": True, "observation": {}}

    def test_worker_admission_requires_actual_booleans_not_integer_aliases(self):
        self.call("admit_result", self.result(), "query", "a" * 40)
        for key in self.result()["workerAdmission"]:
            value = self.result(); value["workerAdmission"][key] = 1
            with self.assertRaises(ValueError):
                self.call("admit_result", value, "query", "a" * 40)

    def test_stale_incomplete_changed_producer_and_cross_phase_results_refuse(self):
        for key, invalid in [("sourceRevision", "c" * 40), ("phase", "tool"), ("complete", False), ("schemaVersion", True),
                             ("nativeSHA256", "short"), ("sourceInputsUnchanged", False), ("builtInputsUnchanged", False),
                             ("vendorVersion", "latest"), ("observation", None)]:
            value = self.result(); value[key] = invalid
            with self.subTest(key=key), self.assertRaises(ValueError):
                self.call("admit_result", value, "query", "a" * 40)


if __name__ == "__main__":
    unittest.main()
