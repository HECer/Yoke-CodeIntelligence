"""Inert source-inventory regression: Git index/text fixtures, no controller import."""
import ast
import hashlib
from pathlib import Path
import subprocess
import tempfile
from types import SimpleNamespace
import unittest


def inventory(root):
    source = Path(__file__).resolve().parents[1] / "host_vscode.py"
    tree = ast.parse(source.read_bytes())
    selected = [node for node in tree.body if isinstance(node, ast.FunctionDef)
                and node.name in {"require", "source_inputs"}]
    namespace = {"Path": Path, "ROOT": root, "subprocess": subprocess}

    def ordinary(path):
        if not path.is_file() or path.is_symlink():
            raise ValueError("Fixture must be an ordinary text file")

    def text_hash(path, maximum):
        raw = path.read_bytes()
        if len(raw) > maximum:
            raise ValueError("Fixture byte capacity")
        raw.decode("utf-8")
        return hashlib.sha256(raw).hexdigest()

    namespace.update(JOB=SimpleNamespace(ordinary=ordinary), hash_file=text_hash)
    exec(compile(ast.Module(body=selected, type_ignores=[]), "<inert-source-inventory>", "exec"), namespace)
    return namespace


class SourceInventory(unittest.TestCase):
    def git(self, root, *args, data=None):
        return subprocess.check_output(["git", "-C", str(root), *args], input=data, timeout=10)

    def test_historical_evidence_is_excluded_before_the_original_inventory_capacity(self):
        with tempfile.TemporaryDirectory(prefix="owned-inventory-") as directory:
            root = Path(directory)
            self.git(root, "init", "-q")
            blob = self.git(root, "hash-object", "-w", "--stdin", data=b"owned text\n").strip()
            expected = {"Cargo.toml", "native/src/lib.rs", "native/src/owned-\u03c0-e\u0301.rs", "scripts/owned.py",
                        "docs/ARCHITECTURE.md", "docs/validation/owned/README.md",
                        "docs/validation/native-owned.json", "docs/validation/native-owned.txt"}
            excluded = {"docs/validation/native-distribution-owned.json",
                        "docs/validation/native-distribution-owned.txt",
                        "docs/validation/deep/native-owned.json", "docs/owned.json"}
            # Index-only history has no payload files. Admitting it would fail
            # even if the controller's original count/byte guards were weakened.
            history = {f"docs/validation/owned-{'x' * 60}-{i:05d}/output.log" for i in range(19000)}
            records = b"".join(b"100644 " + blob + b"\t" + name.encode() + b"\n"
                               for name in sorted(expected | excluded | history))
            self.git(root, "update-index", "--index-info", data=records)
            for name in expected | excluded:
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(b"owned text\n")
            legacy = self.git(root, "ls-files", "-z", "--", "native", "scripts", "docs", "Cargo.toml")
            self.assertGreater(len(legacy), 1_000_000)
            self.assertGreater(len(legacy.split(b"\0")), 4097)
            namespace = inventory(root)
            actual = namespace["source_inputs"]()
            self.assertEqual(set(actual), expected)
            self.assertEqual(set(actual.values()), {hashlib.sha256(b"owned text\n").hexdigest()})

    def test_original_raw_inventory_and_path_refusals_precede_payload_access(self):
        for raw, message in ((b"x" * 1_000_001, "Source inventory capacity"),
                             (b"a\0" * 4097, "Source inventory cardinality"),
                             (b"missing-terminator", "Source inventory cardinality"),
                             (b"/outside\0", "Unsafe source input"),
                             (b"native/../outside\0", "Unsafe source input"),
                             (b"native\\outside\0", "Unsafe source input"),
                             (b"\0", "Unsafe source input")):
            with self.subTest(rawBytes=len(raw)), tempfile.TemporaryDirectory() as directory:
                namespace = inventory(Path(directory))
                namespace["subprocess"] = SimpleNamespace(check_output=lambda *a, **k: raw)
                namespace["JOB"] = SimpleNamespace(ordinary=lambda *a: self.fail("Unexpected filesystem admission"))
                namespace["hash_file"] = lambda *a: self.fail("Unexpected payload read")
                with self.assertRaises(ValueError) as rejected:
                    namespace["source_inputs"]()
                self.assertEqual(str(rejected.exception), message)


if __name__ == "__main__":
    unittest.main()
