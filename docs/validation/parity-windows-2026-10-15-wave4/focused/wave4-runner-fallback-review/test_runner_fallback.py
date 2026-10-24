"""Synthetic batching proof only. No Cargo/Node or candidate runner execution.

The native reader runs against coherent in-memory raw fixtures. This verifies
its partition/coverage rules, not OS resource capacity or fresh real execution.
"""
import ast
import copy
import hashlib
from pathlib import Path
import sys
import unittest
from unittest import mock

sys.dont_write_bytecode = True
STAGING = Path(__file__).resolve().parent
EVIDENCE = STAGING.parent
sys.path.insert(0, str(EVIDENCE / "next-wave4-preservation"))
import receipt_core as core

OLD_SHA = "0d7ed0fa533d55c6518da3dfdc322d6e9e227361f1e88070edbaf2b7067ba45e"
NEW_SHA = "b51bbffbddd07be0aa6ef019fc950d772c628d77b3567f646d7a837726aa3d15"
READER_SHA = "35e097ea51dee414c88c68d645ad43ca6bc2d71c6335152f05107e99a22f99f3"
REPLACEMENTS = (
    (b'for offset in range(0, len(by_kind["test"]), 6):',
     b'for offset in range(0, len(by_kind["test"]), 2):'),
    (b'names = by_kind["test"][offset:offset + 6]',
     b'names = by_kind["test"][offset:offset + 2]'),
    (b'label = "integration-" + str(offset // 6).zfill(2)',
     b'label = "integration-" + str(offset // 2).zfill(2)'),
)


def partition():
    """Read only the literal batch step from the checked new runner AST."""
    syntax = ast.parse((EVIDENCE / "acceptance_wave4.py").read_bytes())
    loops = [node for node in ast.walk(syntax)
             if isinstance(node, ast.For) and isinstance(node.target, ast.Name)
             and node.target.id == "offset"]
    if len(loops) != 1:
        raise AssertionError("Unexpected integration loop")
    step = ast.literal_eval(loops[0].iter.args[2])
    if type(step) is not int or step != 2:
        raise AssertionError("Expected explicit batch step two")
    targets = [f"fixture_{index:03d}" for index in range(209)]
    return targets, [targets[offset:offset + step] for offset in range(0, len(targets), step)]


class NativeFixture:
    def __init__(self, batches=None):
        self.workspace = Path("G:/synthetic-wave4/Code-Intelligence")
        self.bundle = Path("G:/synthetic-wave4/evidence")
        self.directory = self.bundle / "native"
        self.contract = core.literal_contract((EVIDENCE / "acceptance_wave4.py").read_bytes())
        self.base = self.contract["BASE"]
        self.targets, expected = partition()
        self.batches = copy.deepcopy(expected if batches is None else batches)
        self.files, self.results, self.manifest = {}, [], {}
        self.ordinary, self.acl = "D:/synthetic-fixtures", "C:/synthetic-acl"
        self.library_names = sorted(["ordinary_control", self.contract["IGNORED_WORKER"], *self.contract["ACL_TESTS"]])
        self.ignored = {self.contract["IGNORED_WORKER"]}
        targets = []
        for kind, names in (("lib", ["yoke"]), ("test", self.targets)):
            for name in names:
                source = self.workspace / "native" / ("src/lib.rs" if kind == "lib" else f"tests/{name}.rs")
                self.manifest[source.relative_to(self.workspace).as_posix()] = "0" * 64
                targets.append(dict(name=name, kind=[kind], src_path=str(source)))
        metadata = dict(target_directory=str(self.workspace / "target"), workspace_members=["fixture-package"],
                        packages=[dict(id="fixture-package", name="yoke-code-intelligence", targets=targets)])
        self.add("cargo-metadata", ["cargo", "metadata", "--no-deps", "--locked", "--format-version", "1"], core.encoded(metadata), True)
        self.put("target-inventory.json", targets)
        library = self.inventory("library-inventory", ["--lib"], ["yoke"])
        ordinary = set(self.library_names).difference(self.contract["ACL_TESTS"])
        self.execute("library-d", ["--lib"], library, {"yoke": ordinary},
                     [part for name in self.contract["ACL_TESTS"] for part in ("--skip", name)])
        partitions = [dict(label="library-d", names=sorted(ordinary))]
        for index, name in enumerate(self.contract["ACL_TESTS"]):
            label = f"library-c-acl-{index}"
            self.execute(label, ["--lib", name], library, {"yoke": {name}}, ["--exact"], True)
            partitions.append(dict(label=label, names=[name]))
        doc_raw = b"Doc-tests yoke\ntest doc_control ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out;\n"
        doc_groups = core.harness_groups(doc_raw.replace(b"Doc-tests yoke", b" Running doctests (yoke.exe)"), False)
        self.add("doctests", self.base + ["--doc", "--", "--test-threads=4"], doc_raw, groups=doc_groups)
        selected = []
        for index, names in enumerate(self.batches):
            label = f"integration-{index:02d}"
            selectors = [part for name in names for part in ("--test", name)]
            records = self.inventory(label + "-inventory", selectors, names)
            self.execute(label, selectors, records)
            selected.extend(names)
        kinds = dict(lib=["yoke"], bin=[], test=self.targets, example=[], bench=[])
        coverage = dict(libraryRegistered=len(self.library_names), libraryIgnored=1,
                        libraryIgnoredNames=sorted(self.ignored), ignoredWorker=self.contract["IGNORED_WORKER"],
                        libraryPartitions=partitions, libraryDisjointCoverageAccepted=True,
                        binTargets=[], exampleTargets=[], benchTargets=[], integrationTargets=len(selected),
                        integrationTargetNames=selected, integrationCoverageAccepted=True, targetNamesByKind=kinds)
        self.receipt = dict(results=self.results, tempRoot=self.ordinary, aclTempRoot=self.acl, matrixCoverage=coverage)

    def put(self, name, value):
        self.files[str(self.directory / name)] = core.encoded(value)

    def add(self, label, command, raw, inventory=False, acl=False, groups=None, names=None):
        self.files[str(self.directory / (label + ".log"))] = raw
        groups = [] if groups is None else groups
        row = dict(label=label, command=command, inventoryOnly=inventory,
                   tempRoot=self.acl if acl else self.ordinary,
                   groups=len(groups), passed=sum(group["passed"] for group in groups),
                   failed=0, ignored=sum(group["ignored"] for group in groups))
        if not inventory:
            row["testGroups"] = [dict(target=name, **group) for name, group in zip(names, groups)] if names is not None else groups
        self.results.append(row)

    def registered(self, name):
        return self.library_names if name == "yoke" else [f"case::{name}"]

    @staticmethod
    def header(name):
        return f" Running tests/{name}.rs (G:/synthetic-artifacts/{name}-0123.exe)\n"

    def inventory(self, prefix, selectors, names):
        records = {}
        for suffix, ignored_only in (("all", False), ("ignored", True)):
            raw = ""
            for name in names:
                rows = [row for row in self.registered(name) if not ignored_only or row in self.ignored]
                raw += self.header(name) + "".join(row + ": test\n" for row in rows) + f"{len(rows)} tests, 0 benchmarks\n"
                records[name] = dict(registered=self.registered(name),
                                     ignored=sorted(set(self.registered(name)).intersection(self.ignored)),
                                     executable=f"G:/synthetic-artifacts/{name}-0123.exe")
            self.add(prefix + "-" + suffix, self.base + selectors + ["--", *(["--ignored"] if ignored_only else []), "--list"], raw.encode(), True)
        self.put(prefix + ".json", records)
        return records

    def execute(self, label, selectors, records, selected=None, extra=(), acl=False):
        raw = ""
        for name, record in records.items():
            rows = record["registered"] if selected is None else sorted(selected[name])
            ignored = sum(row in self.ignored for row in rows)
            raw += self.header(name)
            raw += "".join(f"test {row} ... {'ignored' if row in self.ignored else 'ok'}\n" for row in rows)
            raw += f"test result: ok. {len(rows) - ignored} passed; 0 failed; {ignored} ignored; 0 measured; {len(record['registered']) - len(rows)} filtered out;\n"
        groups = core.harness_groups(raw.encode(), False)
        self.add(label, self.base + selectors + ["--", "--format", "pretty", "--test-threads=4", *extra], raw.encode(), acl=acl, groups=groups, names=list(records))

    def row(self, label):
        return next(row for row in self.results if row["label"] == label)

    def replay(self):
        def read(path, _boundary):
            try:
                return self.files[str(path)]
            except KeyError as error:
                raise RuntimeError("Missing synthetic raw file: " + str(path)) from error
        with mock.patch.object(core, "read", side_effect=read):
            return core.replay_native(self.bundle, self.receipt, self.contract, self.workspace, self.manifest)

    def phase(self, duplicate_label=False, failed_inventory=False):
        freeze = dict(sourceManifestSha256="a" * 64, runnerSha256=NEW_SHA, sourceFiles=len(self.manifest))
        self.put("source-before.json", self.manifest)
        self.put("source-after.json", self.manifest)
        self.put("cleaned-before.json", [])
        for index in range(len(self.batches)):
            self.put(f"integration-{index:02d}-cleaned.json", [])
        for row in self.results:
            row.update(origin="fresh_execution", exitCode=0,
                       sourceManifestSha256=freeze["sourceManifestSha256"], runnerSha256=NEW_SHA,
                       logSha256=hashlib.sha256(self.files[str(self.directory / (row["label"] + ".log"))]).hexdigest())
        if failed_inventory:
            self.row("integration-00-inventory-all")["exitCode"] = 1
        if duplicate_label:
            self.results.append(copy.deepcopy(self.row("integration-00")))
        receipt = dict(self.receipt, phase="native", phases=list(core.PHASES), accepted=True,
                       sourceChanges=[], validationErrors=[], **freeze,
                       **{key: sum(row[key] for row in self.results) for key in ("groups", "passed", "failed", "ignored")},
                       artifactSha256={Path(path).name: hashlib.sha256(raw).hexdigest() for path, raw in self.files.items()})
        self.put("receipt.json", receipt)
        with mock.patch.object(core, "plain", side_effect=lambda path, _boundary: Path(path)), \
                mock.patch.object(core, "read", side_effect=lambda path, _boundary: self.files[str(path)]):
            return core.phase_receipt(self.bundle, "native", freeze, self.manifest)


class BatchFallbackTests(unittest.TestCase):
    def test_exact_three_byte_runner_delta_and_unchanged_reader(self):
        old = (EVIDENCE / "acceptance_wave3.py").read_bytes()
        new = (EVIDENCE / "acceptance_wave4.py").read_bytes()
        self.assertEqual(hashlib.sha256(old).hexdigest(), OLD_SHA)
        self.assertEqual(hashlib.sha256(new).hexdigest(), NEW_SHA)
        expected = old
        for before, after in REPLACEMENTS:
            self.assertEqual(expected.count(before), 1)
            expected = expected.replace(before, after, 1)
        self.assertEqual(new, expected)
        self.assertEqual(sum(left != right for left, right in zip(old, new)), 3)
        self.assertEqual(hashlib.sha256(Path(core.__file__).read_bytes()).hexdigest(), READER_SHA)

    def test_209_targets_use_105_consecutive_batches_with_singleton_tail(self):
        targets, batches = partition()
        self.assertEqual(len(batches), 105)
        self.assertEqual([len(batch) for batch in batches], [2] * 104 + [1])
        self.assertEqual(batches[-1], [targets[-1]])
        fixture = NativeFixture()
        self.assertEqual([row["label"] for row in fixture.results if row["label"].startswith("integration-") and "-inventory" not in row["label"]], [f"integration-{index:02d}" for index in range(105)])
        result = fixture.replay()
        self.assertEqual(result["integrationTargets"], 209)
        self.assertEqual(result["cargoTargets"], 210)
        self.assertTrue(result["disjointLibraryCoverage"])

    def test_missing_tail_target_is_rejected_against_full_metadata(self):
        _, batches = partition()
        fixture = NativeFixture(batches[:-1])
        with self.assertRaisesRegex(RuntimeError, "Integration targets omitted or duplicated"):
            fixture.replay()

    def test_repeated_target_in_coherent_batch_is_rejected(self):
        _, batches = partition()
        batches[1][0] = batches[0][0]
        fixture = NativeFixture(batches)
        with self.assertRaisesRegex(RuntimeError, "Integration targets omitted or duplicated"):
            fixture.replay()

    def test_tail_inventory_cannot_disappear(self):
        fixture = NativeFixture()
        label = "integration-104-inventory-all"
        fixture.results.remove(fixture.row(label))
        with self.assertRaisesRegex(RuntimeError, "Missing/repeated native result"):
            fixture.replay()

    def test_tail_runtime_group_cannot_disappear(self):
        fixture = NativeFixture()
        fixture.files[str(fixture.directory / "integration-104.log")] = b""
        with self.assertRaisesRegex(RuntimeError, "Cargo harness count differs"):
            fixture.replay()

    def test_selector_and_raw_group_mismatch_is_rejected(self):
        fixture = NativeFixture()
        fixture.row("integration-00")["command"][len(fixture.base) + 3] = "fixture_002"
        with self.assertRaisesRegex(RuntimeError, "Native command/role differs"):
            fixture.replay()

    def test_missing_middle_label_is_rejected(self):
        fixture = NativeFixture()
        for suffix in ("", "-inventory-all", "-inventory-ignored"):
            fixture.results.remove(fixture.row("integration-50" + suffix))
        with self.assertRaisesRegex(RuntimeError, "Integration batch labels are incomplete"):
            fixture.replay()

    def test_duplicate_within_batch_is_rejected(self):
        fixture = NativeFixture()
        row = fixture.row("integration-00")
        row["command"][len(fixture.base) + 3] = "fixture_000"
        with self.assertRaisesRegex(RuntimeError, "Invalid integration target batch"):
            fixture.replay()

    def test_failed_inventory_is_not_accepted_as_success(self):
        self.assertTrue(NativeFixture().phase()["accepted"])
        with self.assertRaisesRegex(RuntimeError, "Failed/retained execution"):
            NativeFixture().phase(failed_inventory=True)

    def test_duplicate_result_label_is_not_hidden_by_result_dictionary(self):
        with self.assertRaisesRegex(RuntimeError, "Duplicate/unsafe result labels"):
            NativeFixture().phase(duplicate_label=True)


if __name__ == "__main__":
    unittest.main(verbosity=2)
