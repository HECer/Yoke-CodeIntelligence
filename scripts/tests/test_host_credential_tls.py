"""Synthetic source/receipt tests only; no Cargo, native image or credentials."""
import ast
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("host_credential_tls", ROOT / "scripts/host_credential_tls.py")
TLS = importlib.util.module_from_spec(spec)
spec.loader.exec_module(TLS)


class CredentialTLSCI(unittest.TestCase):
    def records(self, build):
        package = "path+file:///owned/native#yoke-code-intelligence@0.5.0"
        def record(name, kind, test, path):
            return {"reason":"compiler-artifact", "package_id":package,
                "target":{"name":name,"kind":[kind]}, "profile":{"test":test},
                "executable":str(path), "filenames":[str(path)], "fresh":False}
        return package, [
            record("yoke","bin",False,build / "debug/yoke.exe"),
            record("digest_receiver","test",True,build / "debug/deps/digest_receiver-0123456789abcdef.exe"),
            {"reason":"build-finished","success":True},
        ]

    def encoded(self, rows):
        return b"\n".join(json.dumps(row,ensure_ascii=False).encode("utf-8") for row in rows) + b"\n"

    def stdout(self):
        return (f"\nrunning 1 test\ntest {TLS.CASE} ... ok\n\n"
                "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 49 filtered out; finished in 1.23s\n\n").encode()

    def test_fresh_paired_roles_preserve_unicode_paths(self):
        build = ROOT / "owned-π🦀"
        package, rows = self.records(build)
        result = TLS.artifact_roles(self.encoded(rows),build,package)
        self.assertEqual(set(result),{"cli","test"})
        self.assertEqual(result["cli"]["path"],str(build / "debug/yoke.exe"))

    def test_stale_foreign_missing_duplicate_or_wrong_profile_roles_refuse(self):
        build = ROOT / "owned-build"
        package, rows = self.records(build)
        mutations = [
            lambda r:r[0].update(fresh=True),
            lambda r:r[0].update(fresh=0),
            lambda r:r[1].update(package_id="foreign"),
            lambda r:r[0]["profile"].update(test=True),
            lambda r:r[1]["profile"].update(test=1),
            lambda r:r[1]["target"].update(kind=["bin"]),
            lambda r:r.pop(0),
            lambda r:r.insert(1,copy.deepcopy(r[0])),
            lambda r:r[-1].update(success=False),
            lambda r:r[-1].update(success=1),
            lambda r:r.pop(),
            lambda r:r.append({"reason":"build-finished","success":True}),
            lambda r:r[0].update(filenames={r[0]["executable"]:True}),
        ]
        for mutate in mutations:
            changed = copy.deepcopy(rows)
            mutate(changed)
            with self.subTest(rows=changed),self.assertRaises(RuntimeError):
                TLS.artifact_roles(self.encoded(changed),build,package)

    def test_foreign_traversal_and_wrong_executable_names_refuse(self):
        build = ROOT / "owned-build"
        package, rows = self.records(build)
        for path in (ROOT / "foreign.exe", build / "debug/deps/other-0123456789abcdef.exe",
                     build / "../other/debug/deps/digest_receiver-0123456789abcdef.exe",
                     Path("relative/debug/deps/digest_receiver-0123456789abcdef.exe")):
            changed = copy.deepcopy(rows)
            changed[1].update(executable=str(path),filenames=[str(path)])
            with self.subTest(path=path),self.assertRaises(RuntimeError):
                TLS.artifact_roles(self.encoded(changed),build,package)

    def test_producer_json_capacity_duplicate_fields_and_invalid_payload_refuse(self):
        build = ROOT / "owned-build"
        for raw in (b"",b"not json\n",b'{"reason":"build-finished","reason":"other"}\n',b"x"*(TLS.MAX_PRODUCER+1)):
            with self.subTest(size=len(raw)),self.assertRaises((RuntimeError,ValueError)):
                TLS.artifact_roles(raw,build,"owned")

    def test_case_is_explicit_exact_serial_and_separate_from_default_coverage(self):
        command = TLS.test_command(Path("owned-test.exe"))
        self.assertEqual(command,["owned-test.exe","--exact",TLS.CASE,"--ignored","--test-threads=1"])
        result = TLS.case_result(self.stdout())
        self.assertFalse(result["wholeCargoTarget"])
        self.assertTrue(result["countsAreNotCombined"])
        self.assertEqual(result["filteredOut"],49)
        self.assertEqual(TLS.case_result(self.stdout().replace(b"\n",b"\r\n")),result)

    def test_missing_ignored_failed_filtered_or_substituted_case_cannot_pass(self):
        raw = self.stdout()
        variants = [
            raw.replace(TLS.CASE.encode(),b"foreign"),
            raw.replace(b" ... ok",b" ... ignored"),
            raw.replace(b" ... ok",b" ... FAILED"),
            raw.replace(b"1 passed",b"0 passed"),
            raw.replace(b"0 failed",b"1 failed"),
            raw.replace(b"0 ignored",b"1 ignored"),
            raw.replace(b"running 1 test",b"running 2 tests"),
            raw.replace(b"49 filtered",b"0 filtered"),
            raw.replace(b"49 filtered",b"4097 filtered"),
            raw + f"test {TLS.CASE} ... ok\n".encode(),
            raw + b"test foreign ... ok\n",
            raw + b"test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 49 filtered out; finished in 1.23s\n",
            b"x"*(TLS.MAX_CASE_LOG+1),
        ]
        for changed in variants:
            with self.subTest(raw=changed[:80]),self.assertRaises(RuntimeError):
                TLS.case_result(changed)

    def job(self):
        return {"startedSuspended":True,"assignedBeforeResume":True,"exitCode":0,"treeExitedBeforeCleanup":True,
                "activeBeforeCleanup":0,"activeAfterCleanup":0,"complete":True}

    def test_owned_tree_must_exit_before_forced_cleanup(self):
        TLS.admit_job(self.job())
        for key,value in (("startedSuspended",False),("assignedBeforeResume",False),("exitCode",False),
                          ("exitCode",1),("treeExitedBeforeCleanup",False),("activeBeforeCleanup",1),
                          ("activeAfterCleanup",1),("activeAfterCleanup",False),("complete",False)):
            changed = self.job()
            changed[key] = value
            with self.subTest(key=key,value=value),self.assertRaises(RuntimeError):
                TLS.admit_job(changed)

    def test_case_receipt_requires_exact_source_membership_and_both_log_hashes(self):
        stdout,stderr = self.stdout(),b""
        result = {"complete":True,"sourceRevision":"a"*40,
            "workerJobAdmission":{"specificJobMembershipConfirmed":True,"queryHandleClosedBeforeNativeRead":True},
            "observation":TLS.case_result(stdout),"imageSHA256":{"cli":"b"*64,"test":"c"*64},
            "logSHA256":{"case.stdout":hashlib.sha256(stdout).hexdigest(),"case.stderr":hashlib.sha256(stderr).hexdigest()}}
        TLS.admit_case(result,"a"*40,stdout,stderr)
        for mutate in (lambda r:r.update(complete=False),lambda r:r.update(sourceRevision="d"*40),
                       lambda r:r["workerJobAdmission"].update(specificJobMembershipConfirmed=False),
                       lambda r:r["observation"].update(wholeCargoTarget=True),
                       lambda r:r["imageSHA256"].pop("cli"),lambda r:r["imageSHA256"].update(cli="x"),
                       lambda r:r["logSHA256"].update({"case.stderr":"f"*64})):
            changed = copy.deepcopy(result)
            mutate(changed)
            with self.subTest(receipt=changed),self.assertRaises(RuntimeError):
                TLS.admit_case(changed,"a"*40,stdout,stderr)
        with self.assertRaises(RuntimeError):
            TLS.admit_case(result,"a"*40,stdout,b"altered")

    def test_non_ci_denial_precedes_compilation_namespace_or_native_access(self):
        with mock.patch.dict(TLS.os.environ,{"GITHUB_ACTIONS":"false"}), \
             mock.patch.object(TLS.subprocess,"run",side_effect=AssertionError("No compiler/process")), \
             mock.patch.object(TLS.OWNED,"sha",side_effect=AssertionError("No native payload")), \
             mock.patch.object(TLS.Path,"mkdir",side_effect=AssertionError("No namespace write")):
            for operation in ("prepare","run"):
                with self.assertRaises(RuntimeError):
                    TLS.main([operation])

    def test_worker_job_membership_precedes_image_admission_or_execution(self):
        spec = {"schemaVersion":1,"sourceRevision":"a"*40,"output":str(TLS.ROOT/TLS.OUTPUT_REL),
                "fixture":str(TLS.ROOT/TLS.FIXTURE_REL),"workerJobName":"owned-not-real"}
        with mock.patch.object(TLS,"ci_identity",return_value=("a"*40,{})), \
             mock.patch.object(TLS,"read_json",return_value=spec), \
             mock.patch.object(TLS.OWNED,"require_worker_job",side_effect=RuntimeError("Not assigned")), \
             mock.patch.object(TLS,"metadata",side_effect=AssertionError("No native metadata")), \
             mock.patch.object(TLS.OWNED,"sha",side_effect=AssertionError("No native payload")), \
             mock.patch.object(TLS.subprocess,"run",side_effect=AssertionError("No execution")):
            with self.assertRaises(RuntimeError):
                TLS.worker(TLS.ROOT/TLS.FIXTURE_REL/"spec.json")

    def test_worker_rejects_foreign_spec_before_reading_it(self):
        with mock.patch.object(TLS,"ci_identity",return_value=("a"*40,{})), \
             mock.patch.object(TLS,"read_json",side_effect=AssertionError("No foreign read")):
            with self.assertRaises(RuntimeError):
                TLS.worker(TLS.ROOT/"foreign.json")

    def test_copied_launcher_preserves_every_existing_job_guard(self):
        def function(path):
            return next(n for n in ast.parse(path.read_text(encoding="utf-8")).body
                        if isinstance(n,ast.FunctionDef) and n.name == "bounded_worker")
        self.assertEqual(ast.dump(function(ROOT/"scripts/host_history_live.py"),include_attributes=False),
                         ast.dump(function(ROOT/"scripts/host_credential_tls.py"),include_attributes=False))

    def test_fixed_producer_cannot_run_a_native_test(self):
        command = TLS.producer_command(ROOT/TLS.BUILD_REL)
        self.assertIn("--no-run",command)
        self.assertEqual(command[command.index("--test")+1],"digest_receiver")
        self.assertEqual(command[command.index("--target-dir")+1],str(ROOT/TLS.BUILD_REL))
        self.assertNotIn("--ignored",command)
        self.assertNotIn("--config",command)

    def test_preparation_orchestrates_only_fixed_fresh_compilation_and_retains_package_identity(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            package, rows = self.records(root/TLS.BUILD_REL)
            commands = []
            def execute(argv, **kwargs):
                commands.append(argv)
                if argv[1] == "metadata":
                    payload = {"packages":[{"name":"yoke-code-intelligence","id":package,
                        "manifest_path":str(root/"native/Cargo.toml"),"version":"0.5.0"}],
                        "workspace_members":[package]}
                    return mock.Mock(stdout=json.dumps(payload).encode(),returncode=0)
                self.assertEqual(argv,TLS.producer_command(root/TLS.BUILD_REL))
                kwargs["stdout"].write(self.encoded(rows))
                return mock.Mock(returncode=0)
            with mock.patch.object(TLS,"ROOT",root), \
                 mock.patch.object(TLS,"ci_identity",return_value=("a"*40,{})), \
                 mock.patch.object(TLS.OWNED,"source_inputs",return_value={"owned.rs":"b"*64}), \
                 mock.patch.object(TLS,"metadata",return_value={"bytes":1,"mtimeNs":1,"device":1,"inode":1}), \
                 mock.patch.object(TLS.subprocess,"run",side_effect=execute), \
                 mock.patch.object(TLS.OWNED,"sha",side_effect=AssertionError("No native payload")):
                self.assertEqual(TLS.prepare(),0)
                with self.assertRaises(RuntimeError):
                    TLS.prepare()
            self.assertEqual(len(commands),2)
            producer=json.loads((root/TLS.OUTPUT_REL/"producer.json").read_bytes())
            self.assertEqual(producer["packageId"],package)
            self.assertTrue(producer["freshTargetDirectory"])
            self.assertTrue(json.loads((root/TLS.OUTPUT_REL/"preparation-result.json").read_bytes())["complete"])

    def test_ci_job_keeps_preparation_runtime_and_artifact_scopes_separate(self):
        workflow=(ROOT/".github/workflows/rust.yml").read_text(encoding="utf-8")
        job=workflow.split("\n  windows-credential-tls:\n")
        self.assertEqual(len(job),2)
        self.assertIn("    runs-on: windows-latest\n",job[1])
        self.assertIn("    timeout-minutes: 45\n",job[1])
        self.assertIn("timeout-minutes: 35\n        run: python scripts/host_credential_tls.py prepare",job[1])
        self.assertIn("timeout-minutes: 4\n        run: python scripts/host_credential_tls.py run",job[1])
        self.assertIn("          path: validation/ci-host-credential-tls\n",job[1])
        self.assertNotIn("path: validation/ci-credential-build",job[1])
        self.assertNotIn("continue-on-error",job[1])
        self.assertNotIn("host_credential_tls.py",job[0])


if __name__ == "__main__":
    unittest.main()
