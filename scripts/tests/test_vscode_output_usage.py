"""Pure inventory fragments over owned text fixtures; no controller import or launch."""
import ast
import os
from pathlib import Path
from tempfile import TemporaryDirectory
from types import SimpleNamespace
import unittest
from unittest.mock import patch


SOURCE = Path(__file__).resolve().parents[1] / "vscode_owned_job.py"


def inventory_functions(walk=None):
    tree = ast.parse(SOURCE.read_text(encoding="utf8"))
    constants = {}
    for node in tree.body:
        if isinstance(node, ast.Assign):
            for target in node.targets:
                if isinstance(target, ast.Name) and target.id in {"MAX_ENTRIES", "MAX_OUTPUT", "MIN_FREE"}:
                    constants[target.id] = ast.literal_eval(node.value)
    namespace = {"Path": Path, "os": SimpleNamespace(walk=walk or os.walk), **constants}
    functions = [node for node in tree.body if isinstance(node, ast.FunctionDef)
                 and node.name in {"require", "ordinary", "output_usage"}]
    exec(compile(ast.Module(body=functions, type_ignores=[]), "<pure-inventory-fixtures>", "exec"), namespace)
    return namespace


class OutputInventory(unittest.TestCase):
    def stale_fixture(self, root):
        stale, retained = root / "removed.txt", root / "retained.txt"
        stale.write_bytes(b"removed")
        retained.write_bytes(b"retained")
        names = [stale.name, retained.name]
        stale.unlink()
        self.assertFalse(stale.exists())
        self.assertEqual(retained.read_bytes(), b"retained")

        def walk(selected, followlinks=False, onerror=None):
            self.assertEqual(selected, root)
            self.assertIs(followlinks, False)
            yield str(root), [], names

        return inventory_functions(walk), retained

    def test_ordinary_owned_text_inventory_counts_exact_bytes(self):
        with TemporaryDirectory(prefix="yoke-inventory-owned-") as directory:
            root = Path(directory)
            (root / "one.txt").write_bytes(b"one")
            (root / "two.txt").write_bytes(b"two!")
            functions = inventory_functions()
            self.assertEqual(functions["output_usage"](root), 7)

    def test_live_scan_accepts_a_listed_leaf_that_has_already_been_removed(self):
        # This is the observed CI race: enumeration succeeds before leaf stat.
        with TemporaryDirectory(prefix="yoke-inventory-owned-") as directory:
            root = Path(directory)
            functions, retained = self.stale_fixture(root)
            self.assertEqual(functions["output_usage"](root, allow_disappeared=True), 8)
            self.assertEqual(retained.read_bytes(), b"retained")

    def test_default_and_explicit_settled_scans_refuse_the_same_missing_leaf(self):
        with TemporaryDirectory(prefix="yoke-inventory-owned-") as directory:
            root = Path(directory)
            functions, _ = self.stale_fixture(root)
            for keywords in ({}, {"allow_disappeared": False}):
                with self.subTest(keywords=keywords), self.assertRaises(FileNotFoundError):
                    functions["output_usage"](root, **keywords)

    def test_removed_names_still_consume_the_live_entry_budget(self):
        with TemporaryDirectory(prefix="yoke-inventory-owned-") as directory:
            root = Path(directory)
            functions, _ = self.stale_fixture(root)
            # Reduced synthetic threshold exercises counting without 20,001 files.
            functions["MAX_ENTRIES"] = 1
            with self.assertRaisesRegex(ValueError, "entry capacity"):
                functions["output_usage"](root, allow_disappeared=True)

    def test_live_removal_does_not_excuse_an_existing_file_over_the_byte_budget(self):
        with TemporaryDirectory(prefix="yoke-inventory-owned-") as directory:
            root = Path(directory)
            functions, _ = self.stale_fixture(root)
            functions["MAX_OUTPUT"] = 7
            with self.assertRaisesRegex(ValueError, "output capacity"):
                functions["output_usage"](root, allow_disappeared=True)

    def test_live_scan_propagates_nonmissing_size_stat_failures(self):
        with TemporaryDirectory(prefix="yoke-inventory-owned-") as directory:
            root = Path(directory)
            retained = root / "retained.txt"
            retained.write_bytes(b"owned")
            original = Path.stat
            for error in (PermissionError("owned synthetic refusal"),
                          OSError("owned synthetic IO refusal")):
                attempts = []

                def denied(path, *args, **kwargs):
                    if path == retained and kwargs.get("follow_symlinks", True):
                        attempts.append("size-stat")
                        raise error
                    return original(path, *args, **kwargs)

                with self.subTest(error=type(error).__name__), patch.object(Path, "stat", denied):
                    with self.assertRaises(type(error)):
                        inventory_functions()["output_usage"](root, allow_disappeared=True)
                self.assertEqual(attempts, ["size-stat"])

    def test_live_scan_keeps_link_refusal(self):
        with TemporaryDirectory(prefix="yoke-inventory-owned-") as directory:
            root = Path(directory)
            retained = root / "retained.txt"
            retained.write_bytes(b"owned")
            original = Path.is_symlink

            def linked(path):
                return path == retained or original(path)

            with patch.object(Path, "is_symlink", linked):
                with self.assertRaisesRegex(ValueError, "Linked fixture entry"):
                    inventory_functions()["output_usage"](root, allow_disappeared=True)

    def test_live_scan_keeps_depth_refusal(self):
        with TemporaryDirectory(prefix="yoke-inventory-owned-") as directory:
            root = Path(directory)

            def walk(selected, followlinks=False, onerror=None):
                yield str(root.joinpath(*(["deep"] * 33))), [], []

            with self.assertRaisesRegex(ValueError, "depth capacity"):
                inventory_functions(walk)["output_usage"](root, allow_disappeared=True)

    def test_inventory_mode_requires_an_actual_boolean(self):
        with TemporaryDirectory(prefix="yoke-inventory-owned-") as directory:
            for value in (None, 0, 1, "live", [], {}):
                with self.subTest(value=value), self.assertRaises(ValueError):
                    inventory_functions()["output_usage"](Path(directory), allow_disappeared=value)

    def test_original_resource_limits_are_unchanged(self):
        functions = inventory_functions()
        self.assertEqual(functions["MIN_FREE"], 500_000_000)
        self.assertEqual(functions["MAX_OUTPUT"], 128_000_000)
        self.assertEqual(functions["MAX_ENTRIES"], 20_000)

    def test_only_the_live_monitor_call_enables_removed_leaf_handling(self):
        tree = ast.parse(SOURCE.read_text(encoding="utf8"))
        owned = next(node for node in tree.body if isinstance(node, ast.FunctionDef)
                     and node.name == "run_owned")
        calls = sorted((node for node in ast.walk(owned) if isinstance(node, ast.Call)
                        and isinstance(node.func, ast.Name) and node.func.id == "output_usage"),
                       key=lambda node: node.lineno)
        self.assertEqual(len(calls), 2)
        self.assertEqual([(keyword.arg, ast.literal_eval(keyword.value))
                          for keyword in calls[0].keywords], [("allow_disappeared", True)])
        self.assertEqual(calls[1].keywords, [])


if __name__ == "__main__":
    unittest.main()
