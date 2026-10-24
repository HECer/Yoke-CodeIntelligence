"""Owned staging tests; these never preserve actual evidence or run Cargo."""
import importlib.util
import copy
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

STAGING = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("staged_wave5_preservation", STAGING / "preserve_wave5.py")
preserver = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = preserver
spec.loader.exec_module(preserver)


class PrimitiveContractTests(unittest.TestCase):
    def test_portable_evidence_paths_allow_unicode(self):
        self.assertEqual(preserver.relative_path("accepted/Unicode-🌍/run.log").as_posix(), "accepted/Unicode-🌍/run.log")

    def test_paths_reject_aliases_traversal_and_windows_devices(self):
        for path in ("../outside", "/root", "a//b", "a/./b", "C:/root", "a\\b", "a/NUL.log", "a/x. ", "a/x:y", "a\x00b"):
            with self.subTest(path=path), self.assertRaises(RuntimeError):
                preserver.relative_path(path)

    def test_python_counts_are_dynamic_and_names_are_exact(self):
        raw = b"test_a (suite.Case.test_a) ... ok\ntest_b (suite.Case.test_b) ... ok\nRan 2 tests in 0.01s\n\nOK\n"
        self.assertEqual(preserver.parse_python(raw), ["test_a (suite.Case.test_a)", "test_b (suite.Case.test_b)"])

    def test_duplicate_python_names_are_rejected(self):
        raw = b"test_a (suite.Case.test_a) ... ok\ntest_a (suite.Case.test_a) ... ok\nRan 2 tests in 0.01s\nOK\n"
        with self.assertRaises(RuntimeError):
            preserver.parse_python(raw)

    def test_runner_literals_are_data_and_never_execute(self):
        code = b"raise Exception('must never execute')\nBASE=['cargo','test']\nACL_TESTS=('a','b')\nIGNORED_WORKER='worker'\n"
        self.assertEqual(preserver.literal_contract(code), {"BASE": ["cargo", "test"], "ACL_TESTS": ["a", "b"], "IGNORED_WORKER": "worker"})

    def test_dynamic_runner_expression_is_not_authority(self):
        with self.assertRaises(RuntimeError):
            preserver.literal_contract(b"BASE=__import__('os').system('never')\nACL_TESTS=('a',)\nIGNORED_WORKER='w'\n")

    def test_disjoint_acl_partition_covers_ignored_worker_without_passing_it(self):
        self.assertEqual(preserver.library_partition({"a", "acl", "worker"}, {"worker"}, ["acl"], {"a": "ok", "worker": "ignored"}, [{"acl": "ok"}]), {"a": "ok", "worker": "ignored", "acl": "ok"})

    def test_acl_overlap_omission_or_ignored_cannot_pass(self):
        for ordinary, exact in [({"a": "ok", "acl": "ok", "worker": "ignored"}, [{"acl": "ok"}]), ({"worker": "ignored"}, [{"acl": "ok"}]), ({"a": "ok", "worker": "ignored"}, [{"acl": "ignored"}])]:
            with self.subTest(ordinary=ordinary), self.assertRaises(RuntimeError):
                preserver.library_partition({"a", "acl", "worker"}, {"worker"}, ["acl"], ordinary, exact)

    def test_duplicate_json_keys_are_rejected(self):
        with self.assertRaises(RuntimeError):
            preserver.load_json_bytes(b'{"accepted":true,"accepted":false}')


class ReceiptReaderTests(unittest.TestCase):
    def setUp(self):
        self.fixture = tempfile.TemporaryDirectory(prefix="synthetic-receipt-", dir=STAGING)
        self.addCleanup(self.fixture.cleanup)
        self.bundle = Path(self.fixture.name)
        self.directory = self.bundle / "format"
        self.directory.mkdir()
        self.manifest = {"native/src/lib.rs": "a" * 64}
        self.freeze = dict(sourceManifestSha256=preserver.sha(preserver.encoded(self.manifest)), runnerSha256="b" * 64, sourceFiles=1)
        payloads = {"source-before.json": preserver.encoded(self.manifest), "source-after.json": preserver.encoded(self.manifest), "format.log": b"synthetic successful format fixture; no tool was run\n"}
        for name, data in payloads.items():
            (self.directory / name).write_bytes(data)
        row = dict(label="format", command=["cargo", "fmt", "--all", "--", "--check"], origin="fresh_execution", exitCode=0,
                   inventoryOnly=False, groups=0, passed=0, failed=0, ignored=0, tempRoot="D:/synthetic-owned-fixture", logSha256=preserver.sha(payloads["format.log"]),
                   sourceManifestSha256=self.freeze["sourceManifestSha256"], runnerSha256=self.freeze["runnerSha256"])
        self.receipt = dict(phase="format", phases=list(preserver.PHASES), accepted=True, sourceChanges=[], validationErrors=[], results=[row], groups=0, passed=0, failed=0, ignored=0,
                            tempRoot=row["tempRoot"], aclTempRoot=None, artifactSha256={name:preserver.sha(data) for name,data in payloads.items()}, **self.freeze)

    def check(self, receipt):
        (self.directory / "receipt.json").write_bytes(preserver.encoded(receipt))
        from receipt_core import phase_receipt
        return phase_receipt(self.bundle, "format", self.freeze, self.manifest)

    def test_valid_synthetic_reader_fixture_is_accepted(self):
        self.assertEqual(self.check(self.receipt), self.receipt)

    def test_wrong_empty_change_or_error_type_is_rejected(self):
        for field, value in [("sourceChanges", {}), ("validationErrors", None)]:
            changed = copy.deepcopy(self.receipt)
            changed[field] = value
            with self.subTest(field=field), self.assertRaises(RuntimeError):
                self.check(changed)

    def test_boolean_receipt_counts_are_not_integers(self):
        changed = copy.deepcopy(self.receipt)
        changed["groups"] = False
        with self.assertRaises(RuntimeError):
            self.check(changed)

    def test_retained_execution_or_mixed_runner_is_rejected(self):
        for field, value in [("origin", "retained_execution"), ("runnerSha256", "c" * 64)]:
            changed = copy.deepcopy(self.receipt)
            changed["results"][0][field] = value
            with self.subTest(field=field), self.assertRaises(RuntimeError):
                self.check(changed)

    def test_changed_raw_bytes_are_rejected_even_with_accepted_true(self):
        (self.directory / "format.log").write_bytes(b"changed")
        with self.assertRaises(RuntimeError):
            self.check(self.receipt)

    def test_raw_malformed_receipt_is_preserved_as_unverified(self):
        for value in ([], None, True, 7, "not an object"):
            raw = preserver.encoded(value)
            (self.directory / "receipt.json").write_bytes(raw)
            with self.subTest(value=value):
                result = preserver.classify_raw(self.bundle, STAGING, True)
                self.assertFalse(result["acceptedFullRun"])
                self.assertTrue(result["parseDiagnostics"])
                self.assertEqual(result["recordedOutcomes"], {})
                self.assertEqual((self.directory / "receipt.json").read_bytes(), raw)

    def test_boolean_exit_status_is_not_an_integer_exit_zero(self):
        changed = copy.deepcopy(self.receipt)
        changed["results"][0]["exitCode"] = False
        with self.assertRaises(RuntimeError):
            self.check(changed)

    def test_exclusive_plan_write_never_replaces_an_existing_file(self):
        path = self.bundle / "already-present.json"
        path.write_bytes(b"original fixture bytes")
        with self.assertRaises(FileExistsError):
            preserver.create_json(path, {"replacement": True}, STAGING)
        self.assertEqual(path.read_bytes(), b"original fixture bytes")


class PreservationGuardTests(unittest.TestCase):
    """Only synthetic ignored files; admission mocks never authorize real evidence."""

    def setUp(self):
        self.fixture = tempfile.TemporaryDirectory(prefix="synthetic-preservation-", dir=STAGING)
        self.addCleanup(self.fixture.cleanup)
        self.root = Path(self.fixture.name)

    def test_advertised_historical_digest_sidecar_cannot_disappear(self):
        bundle = self.root / "prior"
        bundle.mkdir()
        payload = bundle / "raw.log"
        payload.write_bytes(b"historical fixture bytes")
        index = bundle / "index.json"
        raw = preserver.encoded(dict(sha256={"prior/raw.log": preserver.sha(payload.read_bytes())}, indexDigestFile="index.sha256"))
        index.write_bytes(raw)
        with mock.patch.object(preserver, "PRIOR", (("prior/index.json", 1, preserver.sha(raw)),)):
            with self.assertRaises(RuntimeError):
                preserver.verify_prior_pins(self.root)
            (bundle / "index.sha256").write_bytes((preserver.sha(raw) + "  index.json\n").encode("ascii"))
            self.assertTrue(preserver.verify_prior_pins(self.root)[0]["sidecarVerified"])

    def test_empty_directories_consume_the_raw_tree_node_budget(self):
        directory = self.root / "raw"
        directory.mkdir()
        for number in range(8):
            (directory / str(number)).mkdir()
        with mock.patch.object(preserver, "MAX_NODES", 6, create=True):
            with self.assertRaises(RuntimeError):
                preserver.tree_files(directory, self.root)

    def test_empty_directory_depth_is_bounded(self):
        directory = self.root / "raw"
        directory.mkdir()
        leaf = directory
        for _ in range(5):
            leaf = leaf / "child"
            leaf.mkdir()
        with mock.patch.object(preserver, "MAX_DEPTH", 3, create=True):
            with self.assertRaises(RuntimeError):
                preserver.tree_files(directory, self.root)

    def test_normal_inventory_is_dynamic_and_keeps_new_untracked_files(self):
        (self.root / "a.txt").write_bytes(b"tracked fixture")
        (self.root / "new.txt").write_bytes(b"normal untracked fixture")
        with mock.patch.object(preserver.subprocess, "check_output", return_value=b"a.txt\0new.txt\0"):
            manifest = preserver.source_manifest(self.root)
        self.assertEqual(manifest, {name: preserver.sha((self.root / name).read_bytes()) for name in ("a.txt", "new.txt")})

    def test_unknown_explicit_plan_input_cannot_expand_authority(self):
        with self.assertRaises(RuntimeError):
            preserver.build_plan({"implicitHomeDiscovery": True})

    def synthetic_plan_with_budget(self, bound, limit):
        """Exercise discovery/charging, with only phase admission mocked out."""
        workspace = self.root / "Code-Intelligence"
        evidence = workspace / "target/parity-validation-20261015"
        bundle = evidence / "wave5-frozen-synthetic"
        bundle.mkdir(parents=True)
        (workspace / "docs/validation").mkdir(parents=True)
        manifest = {"source.txt": "a" * 64}
        receipts = {phase: dict(artifactSha256={}, groups=0, passed=0, failed=0, ignored=0) for phase in preserver.PHASES}
        for name in (*preserver.TOP_FILES, "summary.json", *(phase + "/receipt.json" for phase in preserver.PHASES)):
            path = bundle / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(b"owned synthetic fixture\n")
        raw_paths = []
        for number in range(3):
            directory = evidence / ("wave5-raw-" + str(number))
            directory.mkdir()
            path = directory / "run.log"
            path.write_bytes(b"x" * 800)
            raw_paths.append(path)
        accepted = dict(manifest=manifest, receipts=receipts, freeze=dict(sourceManifestSha256="a" * 64, runnerSha256="b" * 64), nativeMatrix={})
        spec = dict(workspace=str(workspace), evidenceRoot=str(evidence), acceptedTag=bundle.name,
                    reviewedSourceSha256="a" * 64, reviewedRunnerSha256=preserver.REVIEWED_RUNNER_SHA256, destination="docs/validation/parity-windows-synthetic-wave5")
        seen = []
        real_read = preserver.read

        def counted_read(path, boundary):
            seen.append(path)
            return real_read(path, boundary)

        with mock.patch.object(preserver, "verify_prior_pins", return_value=[]), \
             mock.patch.object(preserver, "validate_bundle", return_value=accepted), \
             mock.patch.object(preserver, "source_manifest", return_value=manifest), \
             mock.patch.object(preserver, "read", side_effect=counted_read), \
             mock.patch.object(preserver, bound, limit):
            with self.assertRaises(RuntimeError):
                preserver.build_plan(spec)
        self.assertNotIn(raw_paths[2], seen, "Global budget must stop before traversing the next raw tree")

    def test_plan_file_budget_stops_discovery_before_another_tree(self):
        self.synthetic_plan_with_budget("MAX_FILES", 10)

    def test_plan_byte_budget_stops_discovery_before_another_tree(self):
        self.synthetic_plan_with_budget("MAX_TOTAL", 1200)

    def synthetic_apply(self, mutate_baseline=False, fail_admission=False, wrong_digest=False, existing_destination=False, changed_copy=False):
        workspace = self.root / "Code-Intelligence"
        workspace.mkdir()
        source = workspace / "source.txt"
        source.write_bytes(b"reviewed source")
        manifest = {"source.txt": preserver.sha(source.read_bytes())}
        evidence = workspace / "target/parity-validation-20261015"
        bundle = evidence / "wave5-frozen-synthetic"
        bundle.mkdir(parents=True)
        baseline = bundle / "source-freeze.json"
        baseline.write_bytes(preserver.encoded(manifest))
        payload = evidence / "fixture.log"
        payload.write_bytes(b"synthetic raw fixture, never actual evidence")
        destination = "docs/validation/parity-windows-synthetic-wave5"
        (workspace / "docs/validation").mkdir(parents=True)
        if existing_destination:
            (workspace / destination).mkdir()
            (workspace / destination / "foreign.txt").write_bytes(b"preserve existing fixture bytes")
        plan = dict(inputs=dict(workspace=str(workspace), evidenceRoot=str(evidence), acceptedTag=bundle.name, destination=destination, reviewedSourceSha256=preserver.sha(preserver.encoded(manifest))),
                    toolSha256={"mocked-only-for-owned-fixture": "b" * 64}, previousEvidence=[],
                    files=[dict(source=payload.relative_to(workspace).as_posix(), target="focused/fixture.log", bytes=payload.stat().st_size, sha256=preserver.sha(payload.read_bytes()))], sourceFiles=1,
                    sourceManifestSha256=preserver.sha(preserver.encoded(manifest)), runnerSha256="c" * 64,
                    phaseResults={}, nativeMatrix={}, rawAttempts=[])
        path = self.root / "synthetic-reviewed-plan.json"
        raw = preserver.encoded(plan)
        path.write_bytes(raw)
        if changed_copy:
            payload.write_bytes(b"changed after synthetic admission")
        calls = 0

        def historical_check(_):
            nonlocal calls
            calls += 1
            if mutate_baseline and calls == 2:
                source.write_bytes(b"foreign changed source")
                baseline.write_bytes(preserver.encoded({"source.txt": preserver.sha(source.read_bytes())}))
            return []

        def normal_files(_):
            files = [source, *(workspace / destination).rglob("*")]
            return {item.relative_to(workspace).as_posix(): preserver.sha(item.read_bytes()) for item in files if item.is_file()}

        with mock.patch.object(preserver, "tool_hashes", return_value=plan["toolSha256"]), \
             mock.patch.object(preserver, "build_plan", side_effect=RuntimeError("Synthetic admission refused") if fail_admission else None, return_value=plan), \
             mock.patch.object(preserver, "verify_prior_pins", side_effect=historical_check), \
             mock.patch.object(preserver, "source_manifest", side_effect=normal_files):
            return preserver.apply_plan(path, "d" * 64 if wrong_digest else preserver.sha(raw)), workspace / destination

    def test_final_check_cannot_adopt_a_replaced_source_baseline(self):
        with self.assertRaises(RuntimeError):
            self.synthetic_apply(mutate_baseline=True)

    def test_synthetic_copy_envelope_records_bounded_claims(self):
        result, destination = self.synthetic_apply(mutate_baseline=False)
        self.assertFalse(result["claims"]["universalParity"])
        self.assertFalse(result["claims"]["releaseCertified"])
        self.assertTrue((destination / "reviewed-plan.json").is_file())
        self.assertTrue((destination / "index.sha256").is_file())
        self.assertEqual((destination / "focused/fixture.log").read_bytes(), b"synthetic raw fixture, never actual evidence")

    def test_failed_readmission_refuses_before_destination_creation(self):
        with self.assertRaises(RuntimeError):
            self.synthetic_apply(fail_admission=True)
        self.assertFalse((self.root / "Code-Intelligence/docs/validation/parity-windows-synthetic-wave5").exists())

    def test_wrong_reviewed_digest_refuses_before_destination_creation(self):
        with self.assertRaises(RuntimeError):
            self.synthetic_apply(wrong_digest=True)
        self.assertFalse((self.root / "Code-Intelligence/docs/validation/parity-windows-synthetic-wave5").exists())

    def test_existing_destination_is_never_adopted_or_overwritten(self):
        with self.assertRaises(FileExistsError):
            self.synthetic_apply(existing_destination=True)
        self.assertEqual((self.root / "Code-Intelligence/docs/validation/parity-windows-synthetic-wave5/foreign.txt").read_bytes(), b"preserve existing fixture bytes")

    def test_changed_admitted_copy_preserves_partial_output_without_index(self):
        with self.assertRaises(RuntimeError):
            self.synthetic_apply(changed_copy=True)
        destination = self.root / "Code-Intelligence/docs/validation/parity-windows-synthetic-wave5"
        self.assertTrue(destination.is_dir())
        self.assertFalse((destination / "focused/fixture.log").exists())
        self.assertFalse((destination / "index.json").exists())


class Wave5AdaptationTests(unittest.TestCase):
    def test_all_four_prior_bundles_remain_pinned_as_1650_artifacts(self):
        self.assertEqual(len(preserver.PRIOR), 4)
        self.assertEqual(sum(row[1] for row in preserver.PRIOR), 1650)
        self.assertEqual(preserver.PRIOR[-1], (
            "docs/validation/parity-windows-2026-10-15-wave4/index.json",
            864,
            "1b829007c444e02be250626ca0e418830932d478a49ad1c88dc828eb3a72acbf",
        ))

    def test_receipt_core_is_the_unchanged_accepted_reader(self):
        self.assertEqual(preserver.sha((STAGING / "receipt_core.py").read_bytes()),
                         "35e097ea51dee414c88c68d645ad43ca6bc2d71c6335152f05107e99a22f99f3")

    def test_other_runner_is_rejected_before_historical_or_phase_reads(self):
        with tempfile.TemporaryDirectory(prefix="synthetic-runner-policy-", dir=STAGING) as fixture:
            workspace = Path(fixture) / "Code-Intelligence"
            evidence = workspace / "target/parity-validation-20261015"
            evidence.mkdir(parents=True)
            values = dict(workspace=str(workspace), evidenceRoot=str(evidence),
                          acceptedTag="wave5-frozen-synthetic",
                          reviewedSourceSha256="a" * 64, reviewedRunnerSha256="b" * 64,
                          destination="docs/validation/parity-windows-synthetic-wave5")
            with mock.patch.object(preserver, "verify_prior_pins") as prior:
                with self.assertRaisesRegex(RuntimeError, "Accepted runner changed"):
                    preserver.build_plan(values)
                prior.assert_not_called()

    def test_qualified_manual_results_remain_nonadditive_and_not_full_acceptance(self):
        with tempfile.TemporaryDirectory(prefix="synthetic-manual-", dir=STAGING) as fixture:
            directory = Path(fixture) / "wave5-qualified-example"
            directory.mkdir()
            receipt = dict(accepted=True, exitCode=0, passed=1, failed=0,
                           ignored=0, sourceChanges=[], validationErrors=[])
            (directory / "receipt.json").write_bytes(preserver.encoded(receipt))
            result = preserver.classify_raw(directory, STAGING, False)
            self.assertEqual(result["classification"], "raw_qualified_manual_gate_not_full_acceptance")
            self.assertFalse(result["acceptedFullRun"])
            self.assertEqual(result["recordedOutcomes"]["focused"]["passed"], 1)
            self.assertEqual(result["recordedOutcomes"]["focused"]["ignored"], 0)
            self.assertIn("never added", result["countsInterpretation"])

if __name__ == "__main__":
    unittest.main()
