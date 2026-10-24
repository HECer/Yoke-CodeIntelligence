import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest import mock

SPEC = importlib.util.spec_from_file_location("native_shards", Path(__file__).resolve().parents[1] / "native_shards.py")
SHARD = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(SHARD)


class WholeTargetCoverage(unittest.TestCase):
    def metadata(self, root):
        targets = []
        for ordinal, (kind, tested, documented) in enumerate([
            ("lib", True, True), ("bin", True, False),
            ("example", False, False), ("example", False, False), ("example", True, False),
            *[("test", True, False)] * 8, ("test", False, False), ("bench", False, False),
        ]):
            path = root / f"owned-{ordinal}-π.rs"
            path.write_text("// exact e\u0301 🦀\r\n" * (ordinal + 1), encoding="utf-8", newline="")
            targets.append({"name": f"target_{ordinal}", "kind": [kind], "test": tested,
                            "doctest": documented, "src_path": str(path), "required-features": []})
        return {"workspace_members": ["owned-id"], "packages": [{"id": "owned-id", "name": "owned", "targets": targets}]}

    def fixture(self, root):
        return SHARD.inventory(self.metadata(root), root)

    def receipts(self, groups):
        inputs = {group["source"]: "a" * 64 for group in groups}
        receipts = []
        for index, selected in enumerate(SHARD.partitions(groups)):
            receipts.append({"schemaVersion": 1, "platform": "Windows", "sourceRevision": "b" * 40,
                "index": index, "shards": 8, "complete": True, "wholeCargoTargets": True,
                "inventory": copy.deepcopy(groups), "inventorySha256": SHARD.digest(groups), "sourceInputs": inputs,
                "groups": [{"group": group["id"], "exitCode": 0, "logs": [
                    {"name": f"{ordinal:04d}.{suffix}", "bytes": 0, "sha256": "c" * 64}
                    for suffix in ("stdout", "stderr")]} for ordinal, group in enumerate(selected)]})
        return receipts, inputs

    def test_default_target_kinds_keep_whole_tests_docs_and_compile_only_examples(self):
        with tempfile.TemporaryDirectory() as directory:
            groups = self.fixture(Path(directory))
            expected = {f"owned:{kind}:target_{index}:{mode}" for index, kind, mode in [
                (0, "lib", "test"), (0, "lib", "doc"), (1, "bin", "test"),
                (2, "example", "build"), (3, "example", "build"), (4, "example", "test"),
                *[(index, "test", "test") for index in range(5, 13)]]}
            self.assertEqual({row["id"] for row in groups}, expected)
            self.assertTrue(all("π" in row["source"] for row in groups))
            partitions = SHARD.partitions(groups)
            flattened = [row["id"] for part in partitions for row in part]
            self.assertEqual(set(flattened), expected)
            self.assertEqual(len(flattened), len(expected))
            self.assertEqual(partitions, SHARD.partitions(list(reversed(groups))))

    def test_missing_duplicate_and_unknown_target_metadata_never_yields_partial_success(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            original = self.metadata(root)
            changes = []
            for mutate in [lambda d: d["workspace_members"].append("missing"),
                           lambda d: d["packages"].append(copy.deepcopy(d["packages"][0])),
                           lambda d: d["packages"][0]["targets"].append(copy.deepcopy(d["packages"][0]["targets"][0])),
                           lambda d: d["packages"][0]["targets"][0].update(kind=["unknown"]),
                           lambda d: d["packages"][0]["targets"][0].update(test=None),
                           lambda d: d["packages"][0]["targets"][0].update({"required-features": ["unreconciled"]}),
                           lambda d: d["packages"][0]["targets"][-1].update(test=True),
                           lambda d: d["packages"][0]["targets"][0].update(name="*"),
                           lambda d: d["packages"][0]["targets"][0].update(name="--config")]:
                data = copy.deepcopy(original)
                mutate(data)
                changes.append(data)
            for data in changes:
                with self.subTest(data=data), self.assertRaises(RuntimeError):
                    SHARD.inventory(data, root)

    def test_external_target_source_refuses_even_with_an_existing_file(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            selected = root / "selected"
            selected.mkdir()
            data = self.metadata(selected)
            outside = root / "outside.rs"
            outside.write_text("// inert", encoding="utf-8")
            data["packages"][0]["targets"][0]["src_path"] = str(outside)
            with self.assertRaises(RuntimeError):
                SHARD.inventory(data, selected)

    def test_empty_duplicate_boolean_negative_and_oversized_partition_inputs_refuse(self):
        good = [{"id": str(index), "weight": index + 1} for index in range(8)]
        for rows in [[], good[:7], good + [good[0]], [{"id": "x", "weight": True}] * 8,
                     [{"id": str(i), "weight": -1} for i in range(8)],
                     [{"id": str(i), "weight": 16_000_001} for i in range(8)], good * 300]:
            with self.subTest(rows=len(rows)), self.assertRaises(RuntimeError):
                SHARD.partitions(rows)

    def test_commands_have_only_target_selection_and_original_platform_thread_counts(self):
        with tempfile.TemporaryDirectory() as directory:
            groups = self.fixture(Path(directory))
            for group in groups:
                for platform, threads in [("Windows", "1"), ("Linux", "2"), ("macOS", "2")]:
                    argv = SHARD.command(group, platform)
                    self.assertIn("--locked", argv)
                    self.assertEqual(argv[argv.index("-j") + 1], "2")
                    if group["mode"] == "build":
                        self.assertEqual(argv[:2], ["cargo", "build"])
                        self.assertNotIn("--", argv)
                    else:
                        self.assertIn("--controlled-test-runner", argv)
                        self.assertEqual(argv[argv.index("--") + 1:], ["--test-threads=" + threads])
                        self.assertNotIn("--ignored", argv)
                        self.assertNotIn("--include-ignored", argv)
                    if group["mode"] == "doc":
                        self.assertIn("--doc", argv)
                        self.assertNotIn("--lib", argv)

    def test_all_eight_same_source_receipts_are_required_before_platform_success(self):
        with tempfile.TemporaryDirectory() as directory:
            groups = self.fixture(Path(directory))
            receipts, inputs = self.receipts(groups)
            proof = SHARD.check_receipts(receipts, groups, inputs, "Windows", "b" * 40)
            self.assertTrue(proof["complete"])
            self.assertEqual(proof["wholeGroups"], len(groups))
            for changes in [receipts[:-1], receipts + [receipts[0]], [receipts[0]] * 8]:
                with self.assertRaises(RuntimeError):
                    SHARD.check_receipts(changes, groups, inputs, "Windows", "b" * 40)

    def test_failed_missing_substituted_or_changed_scope_receipts_refuse(self):
        with tempfile.TemporaryDirectory() as directory:
            groups = self.fixture(Path(directory))
            receipts, inputs = self.receipts(groups)
            for mutate in [lambda r: r.update(sourceRevision="d" * 40), lambda r: r.update(platform="Linux"),
                           lambda r: r.update(complete=False), lambda r: r.update(wholeCargoTargets=False),
                           lambda r: r.update(index=True), lambda r: r.update(sourceInputs={}),
                           lambda r: r.update(inventorySha256="d" * 64), lambda r: r["groups"].pop(),
                           lambda r: r["groups"][0].update(group="other"),
                           lambda r: r["groups"][0].update(exitCode=1),
                           lambda r: r["groups"][0].update(exitCode=False),
                           lambda r: r["groups"][0].update(logs=[]),
                           lambda r: r["groups"][0]["logs"][0].update(name="../foreign"),
                           lambda r: r["groups"][0]["logs"][0].update(bytes=SHARD.MAX_LOG + 1)]:
                changed = copy.deepcopy(receipts)
                mutate(changed[0])
                with self.subTest(receipt=changed[0]), self.assertRaises(RuntimeError):
                    SHARD.check_receipts(changed, groups, inputs, "Windows", "b" * 40)

    def test_execution_refuses_non_ci_before_metadata_build_or_native_process_use(self):
        with mock.patch.dict(SHARD.os.environ, {"GITHUB_ACTIONS": "false"}), \
             mock.patch.object(SHARD, "selected_groups", side_effect=AssertionError("No metadata")), \
             mock.patch.object(SHARD.subprocess, "run", side_effect=AssertionError("No execution")):
            with self.assertRaises(RuntimeError):
                SHARD.main(["run", "--index", "0"])

    def test_failed_group_still_runs_remaining_whole_groups_and_cannot_publish_success(self):
        # Only this synthetic coordinator fixture simulates CI/disk state. The
        # subprocess boundary is replaced completely; no compiler or native
        # test image can be read or executed by this test.
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            groups = self.fixture(root)
            receipts, inputs = self.receipts(groups)
            parts = SHARD.partitions(groups)
            index = next(i for i, part in enumerate(parts) if len(part) > 1)
            calls = []

            def execute(argv, **kwargs):
                if argv[0] == "git":
                    return mock.Mock(returncode=0)
                calls.append(argv)
                self.assertFalse(any(key in kwargs["env"] for key in SHARD.TESTS.TLS_OVERRIDES))
                kwargs["stdout"].write(b"owned synthetic command output\n")
                return mock.Mock(returncode=1 if len(calls) == 1 else 0)

            with mock.patch.object(SHARD, "ROOT", root), \
                 mock.patch.object(SHARD, "ci_context", return_value=("Windows", "b" * 40)), \
                 mock.patch.object(SHARD, "selected_groups", return_value=groups), \
                 mock.patch.object(SHARD.OWNED, "source_inputs", return_value=inputs), \
                 mock.patch.object(SHARD.shutil, "disk_usage", return_value=mock.Mock(free=SHARD.OWNED.MIN_FREE)), \
                 mock.patch.dict(SHARD.os.environ, {key: "owned-inert-fixture" for key in SHARD.TESTS.TLS_OVERRIDES}), \
                 mock.patch.object(SHARD.subprocess, "run", side_effect=execute):
                self.assertEqual(SHARD.run(index), 1)
                self.assertTrue(all(key in SHARD.os.environ for key in SHARD.TESTS.TLS_OVERRIDES))
            self.assertEqual(len(calls), len(parts[index]))
            result = json.loads((root / "validation/ci-native-shard/result.json").read_text())
            self.assertFalse(result["complete"])
            self.assertEqual([row["group"] for row in result["groups"]], [row["id"] for row in parts[index]])
            receipts[index] = result
            with self.assertRaises(RuntimeError):
                SHARD.check_receipts(receipts, groups, inputs, "Windows", "b" * 40)

    def test_actual_artifact_log_tampering_refuses_before_coverage_publication(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            groups = self.fixture(root)
            receipts, inputs = self.receipts(groups)
            artifacts = root / "validation/ci-native-shards-download"
            artifacts.mkdir(parents=True)
            payload = b"owned original group log\n"
            first = None
            for receipt in receipts:
                target = artifacts / ("native-shard-Windows-" + str(receipt["index"]))
                target.mkdir()
                for row in receipt["groups"]:
                    for log in row["logs"]:
                        path = target / log["name"]
                        path.write_bytes(payload)
                        first = first or path
                        log.update(bytes=len(payload), sha256=hashlib.sha256(payload).hexdigest())
                (target / "result.json").write_text(json.dumps(receipt), encoding="utf-8")
            first.write_bytes(b"X" + payload[1:])
            with mock.patch.object(SHARD, "ROOT", root), \
                 mock.patch.object(SHARD, "ci_context", return_value=("Windows", "b" * 40)), \
                 mock.patch.object(SHARD, "selected_groups", return_value=groups), \
                 mock.patch.object(SHARD.OWNED, "source_inputs", return_value=inputs):
                with self.assertRaisesRegex(RuntimeError, "artifact namespace"):
                    SHARD.check(artifacts.parent)
                with self.assertRaisesRegex(RuntimeError, "Shard log bytes differ"):
                    SHARD.check(artifacts)
                self.assertFalse((root / "validation/ci-native-coverage.json").exists())
                first.write_bytes(payload)
                self.assertEqual(SHARD.check(artifacts), 0)
                self.assertTrue((root / "validation/ci-native-coverage.json").is_file())

    @unittest.skipUnless(shutil.which("cargo"), "Cargo metadata host unavailable")
    def test_actual_current_cargo_inventory_is_covered_without_any_build_or_execution(self):
        metadata = SHARD.FMT.cargo_metadata(SHARD.ROOT / "Cargo.toml")
        groups = SHARD.inventory(metadata, SHARD.ROOT)
        declared = sum(1 + int(t["kind"] == ["lib"] and t["doctest"])
                       for p in metadata["packages"] if p["id"] in metadata["workspace_members"]
                       for t in p["targets"] if t["test"] or t["kind"] == ["example"])
        self.assertEqual(len(groups), declared)
        parts = SHARD.partitions(groups)
        self.assertEqual(len(parts), 8)
        self.assertEqual(sorted(row["id"] for part in parts for row in part), sorted(row["id"] for row in groups))


if __name__ == "__main__":
    unittest.main()
