import ctypes as c
import importlib.util
import json
import os
from pathlib import Path
import sys
import tempfile
import types
import unittest
from unittest import mock

SPEC = importlib.util.spec_from_file_location(
    "host_powershell_probe", Path(__file__).resolve().parents[1] / "host_powershell_probe.py"
)
PROBE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PROBE)

FIXTURE_BOOTSTRAP = (Path(__file__).resolve().parents[2] / "native/tests/fixtures/windows-powershell-utility.ps1").read_bytes().decode("utf8")
FIXTURE_MODULE = "($PSHOME+'\\Modules\\Microsoft.PowerShell.Utility\\Microsoft.PowerShell.Utility.psd1')"


def initialized_owned_fixture(definition):
    """Declare this test payload's dependency; never alter the raw probe plan."""
    name, script, count, resume = definition
    return ("initialized-" + name, FIXTURE_BOOTSTRAP + script, count, resume)


def simulated_trial(assigned=False, termination=True, leaking=False, close=True,
                    observation=None, clock=None, wait=None):
    class Basic(c.Structure):
        _fields_ = [("flags", c.c_uint32)]

    class Extended(c.Structure):
        _fields_ = [("basic", Basic)]

    state = {"terminated": False}
    api = mock.Mock()
    api.CreateJobObjectW.return_value = 501
    api.SetInformationJobObject.return_value = 1
    api.AssignProcessToJobObject.return_value = int(assigned)
    api.ResumeThread.return_value = 1
    api.CloseHandle.return_value = int(close)

    def accounting(_job, _kind, buffer, _size, _returned):
        count = int(leaking and not state["terminated"])
        c.memmove(c.addressof(buffer) + 40, count.to_bytes(4, "little"), 4)
        return 1

    def terminate(_job, _code):
        state["terminated"] = termination
        return int(termination)

    api.QueryInformationJobObject.side_effect = accounting
    api.TerminateJobObject.side_effect = terminate
    win = types.SimpleNamespace(CreateProcess=mock.Mock(return_value=(101, 102, 103, 104)),
                                TerminateProcess=mock.Mock(),
                                WaitForSingleObject=mock.Mock(return_value=0, side_effect=wait),
                                GetExitCodeProcess=mock.Mock(return_value=0))
    error = None
    with mock.patch.dict(sys.modules, {"_winapi": win, "msvcrt": types.SimpleNamespace(get_osfhandle=lambda fd: fd)}), \
         mock.patch.object(PROBE.shutil, "disk_usage", return_value=types.SimpleNamespace(free=500_000_000)), \
         mock.patch.object(PROBE, "windows_api", return_value=(api, Extended, None, None)), \
         mock.patch.object(PROBE, "observe_members", side_effect=observation,
                           return_value={"complete": True, "members": [], "unavailable": []}), \
         mock.patch.object(PROBE.time, "monotonic", side_effect=clock or PROBE.time.monotonic), \
         mock.patch.object(PROBE.os, "set_handle_inheritable", create=True), \
         mock.patch.object(PROBE.subprocess, "STARTUPINFO", return_value=types.SimpleNamespace(), create=True), \
         mock.patch.object(PROBE.subprocess, "STARTF_USESTDHANDLES", 256, create=True), \
         mock.patch.object(PROBE.subprocess, "CREATE_NO_WINDOW", 0x08000000, create=True):
        try:
            row = PROBE.trial(Path("owned.exe"), Path("."), {}, ("simulated", "owned", 1, "primary"))
        except PROBE.CleanupUnproven as exception:
            error, row = exception, exception.observation
    return row, api, win, error


class PowerShellDiagnostic(unittest.TestCase):
    def test_console_control_keeps_nested_snapshot_inputs_and_last_original_trial(self):
        definitions = PROBE.commands()
        self.assertEqual([row[0] for row in definitions],
                         ["single-primary", "nested-primary-folder", "nested-snapshot",
                          "nested-snapshot-console", "nested-snapshot-folder"])
        plain, console = definitions[2:4]
        self.assertEqual(plain[2:], console[2:])
        self.assertEqual(console[1], plain[1]
                         .replace("Write-Output 'owned-shell-test'",
                                  "[Console]::Out.WriteLine('owned-shell-test')")
                         .replace("owned-before-write-output", "owned-before-console-out")
                         .replace("owned-after-write-output", "owned-after-console-out"))
        self.assertEqual(PROBE.TRIAL_SECONDS, 20)
        self.assertEqual(PROBE.CLEANUP_SECONDS, 2)
        # Member sampling shares the trial deadline, not an extra post-timeout wait.
        waits = len(definitions) * (PROBE.TRIAL_SECONDS + PROBE.CLEANUP_SECONDS
                                    + 2 * PROBE.READER_JOIN_SECONDS)
        self.assertEqual(waits, 112.5)
        self.assertLess(waits, 120)

    def test_captured_write_output_is_one_separate_equivalent_nested_snapshot_trial(self):
        canonical = PROBE.commands()
        definitions = PROBE.captured_write_output_commands()
        self.assertEqual(len(definitions), 1)
        captured = definitions[0]
        self.assertEqual(captured[0], "nested-snapshot-captured-write-output")
        self.assertEqual(captured[2:], canonical[2][2:])
        self.assertEqual(captured[1], canonical[2][1]
                         .replace("Write-Output 'owned-shell-test'",
                                  "$value = Write-Output 'owned-shell-test'")
                         .replace("owned-before-write-output", "owned-before-captured-write-output")
                         .replace("owned-after-write-output')",
                                  "owned-after-captured-write-output'); [Console]::Out.WriteLine($value)"))
        self.assertEqual(captured[1].count("Write-Output 'owned-shell-test'"), 1)
        self.assertEqual(captured[1].count("[Console]::Out.WriteLine($value)"), 1)
        self.assertLess(captured[1].index("owned-before-captured-write-output"),
                        captured[1].index("$value = Write-Output"))
        self.assertLess(captured[1].index("$value = Write-Output"),
                        captured[1].index("owned-after-captured-write-output"))
        self.assertLess(captured[1].index("owned-after-captured-write-output"),
                        captured[1].index("[Console]::Out.WriteLine($value)"))
        self.assertEqual(PROBE.commands(), canonical)
        self.assertEqual(len(canonical), 5)
        self.assertEqual(canonical[-1][0], "nested-snapshot-folder")
        waits = len(definitions) * (PROBE.TRIAL_SECONDS + PROBE.CLEANUP_SECONDS
                                    + 2 * PROBE.READER_JOIN_SECONDS)
        self.assertEqual(waits, 22.5)
        self.assertLess(waits, 120)

    def test_captured_output_cli_dispatch_does_not_run_the_canonical_plan(self):
        canonical = PROBE.commands()
        definitions = PROBE.captured_write_output_commands()
        for supplemental in (False, True):
            with self.subTest(supplemental=supplemental), tempfile.TemporaryDirectory() as raw:
                root = Path(raw)
                output = (root / "new-output").resolve()
                environment = {"GITHUB_SHA": "a" * 40, "SystemRoot": str(root),
                               "PSModulePath": "unapproved-modules"}
                row = {"diagnosticComplete": False, "nativeAcceptance": False}
                arguments = ["--output", str(output)]
                if supplemental:
                    arguments.append("--captured-write-output")
                with mock.patch.object(PROBE, "os", types.SimpleNamespace(name="nt", environ=environment)), \
                     mock.patch.object(PROBE.shutil, "disk_usage", return_value=types.SimpleNamespace(free=500_000_000)), \
                     mock.patch.object(PROBE.Path, "is_file", return_value=True), \
                     mock.patch.object(PROBE, "trial", return_value=row) as trial, \
                     mock.patch("builtins.print"):
                    self.assertEqual(PROBE.main(arguments), 0)
                selected = definitions if supplemental else canonical
                self.assertEqual([call.args[3] for call in trial.call_args_list], selected)
                self.assertTrue(all(call.args[1] == output / "fixture" for call in trial.call_args_list))
                self.assertTrue(all(call.args[2] == PROBE.selected_environment(environment)
                                    for call in trial.call_args_list))
                report = json.loads((output / "result.json").read_text(encoding="utf8"))
                self.assertEqual(report["diagnosticMode"], "captured-write-output" if supplemental else "canonical")
                self.assertEqual(report["trialDeadlineSeconds"], 20)
                self.assertEqual(len(report["trials"]), len(selected))
                self.assertFalse(report["nativeAcceptance"] or report["diagnosticComplete"])

    def test_captured_output_flag_rejects_values_and_unknown_modes_before_output_or_process(self):
        for option in ("--captured-write-output=false", "--captured-write-output=owned", "--unapproved-control"):
            with self.subTest(option=option), \
                 mock.patch.object(PROBE.Path, "mkdir") as mkdir, \
                 mock.patch.object(PROBE, "trial") as trial, \
                 mock.patch("sys.stderr"):
                with self.assertRaises(SystemExit) as refused:
                    PROBE.main(["--output", "unused-owned-output", option])
                self.assertEqual(refused.exception.code, 2)
                mkdir.assert_not_called()
                trial.assert_not_called()

    def test_captured_output_mode_preserves_existing_output_before_any_trial(self):
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            marker = root / "foreign-marker"
            marker.write_bytes(b"preserve")
            with mock.patch.object(PROBE, "os", types.SimpleNamespace(name="nt", environ={"GITHUB_SHA": "a" * 40})), \
                 mock.patch.object(PROBE.shutil, "disk_usage", return_value=types.SimpleNamespace(free=500_000_000)), \
                 mock.patch.object(PROBE, "trial") as trial:
                with self.assertRaises(FileExistsError):
                    PROBE.main(["--output", str(root), "--captured-write-output"])
            self.assertEqual(marker.read_bytes(), b"preserve")
            trial.assert_not_called()

    def test_local_app_data_control_keeps_the_exact_original_final_command(self):
        original = PROBE.commands()
        selected = PROBE.local_app_data_commands()
        self.assertEqual(selected, [("nested-snapshot-folder-local-app-data", *original[-1][1:])])
        self.assertEqual(PROBE.commands(), original)
        self.assertEqual(len(selected), 1)
        self.assertEqual(PROBE.TRIAL_SECONDS, 20)
        self.assertEqual(PROBE.CLEANUP_SECONDS, 2)
        self.assertLess(PROBE.TRIAL_SECONDS + PROBE.CLEANUP_SECONDS
                        + 2 * PROBE.READER_JOIN_SECONDS, 60)

    def test_local_app_data_changes_only_one_explicit_environment_key(self):
        source = {"PATH": "owned-tools", "SystemRoot": "C:\\Windows",
                  "LOCALAPPDATA": "C:\\Owned Host\\Local",
                  "APPDATA": "unselected", "PSModulePath": "unselected",
                  "OPENAI_API_KEY": "owned-not-a-secret"}
        with mock.patch.object(PROBE.Path, "is_dir", return_value=True):
            selected = PROBE.local_app_data_environment(source)
        baseline = PROBE.selected_environment(source)
        self.assertNotIn("LOCALAPPDATA", baseline)
        self.assertEqual(selected, {**baseline, "LOCALAPPDATA": source["LOCALAPPDATA"]})
        self.assertNotIn("APPDATA", selected)
        self.assertNotIn("PSModulePath", selected)
        self.assertNotIn("OPENAI_API_KEY", selected)

    def test_local_app_data_refuses_missing_relative_remote_and_control_values_before_lookup(self):
        for value in (None, "", "relative", "C:relative", "\\\\server\\share",
                      "C:\\owned\\..\\Local", "C:\\owned\\.\\Local",
                      "C:\\owned\nLocal", "C:\\owned\x00Local", "x" * 32768):
            with self.subTest(value=value), mock.patch.object(PROBE.Path, "is_dir") as directory:
                source = {} if value is None else {"LOCALAPPDATA": value}
                with self.assertRaises(RuntimeError):
                    PROBE.local_app_data_environment(source)
                directory.assert_not_called()
        with mock.patch.object(PROBE.Path, "is_dir", return_value=False):
            with self.assertRaisesRegex(RuntimeError, "directory is unavailable"):
                PROBE.local_app_data_environment({"LOCALAPPDATA": "C:\\missing"})

    def test_local_app_data_invalid_host_value_refuses_before_output_creation(self):
        with mock.patch.object(PROBE, "os", types.SimpleNamespace(name="nt", environ={})), \
             mock.patch.object(PROBE.shutil, "disk_usage", return_value=types.SimpleNamespace(free=500_000_000)), \
             mock.patch.object(PROBE.Path, "mkdir") as mkdir, \
             mock.patch.object(PROBE, "trial") as trial:
            with self.assertRaises(RuntimeError):
                PROBE.main(["--output", "unused-owned-output", "--local-app-data"])
            mkdir.assert_not_called()
            trial.assert_not_called()

    def test_local_app_data_cli_is_separate_and_does_not_transcribe_the_host_value(self):
        with tempfile.TemporaryDirectory() as raw:
            output = (Path(raw) / "new-output").resolve()
            value = "C:\\Owned Host\\Private Local"
            environment = {"SystemRoot": raw, "LOCALAPPDATA": value, "GITHUB_SHA": "a" * 40}
            row = {"diagnosticComplete": True, "nativeAcceptance": False}
            with mock.patch.object(PROBE, "os", types.SimpleNamespace(name="nt", environ=environment)), \
                 mock.patch.object(PROBE.shutil, "disk_usage", return_value=types.SimpleNamespace(free=500_000_000)), \
                 mock.patch.object(PROBE.Path, "is_file", return_value=True), \
                 mock.patch.object(PROBE.Path, "is_dir", return_value=True), \
                 mock.patch.object(PROBE, "trial", return_value=row) as trial, \
                 mock.patch("builtins.print"):
                self.assertEqual(PROBE.main(["--output", str(output), "--local-app-data"]), 0)
            self.assertEqual(trial.call_count, 1)
            self.assertEqual(trial.call_args.args[3], PROBE.local_app_data_commands()[0])
            self.assertEqual(trial.call_args.args[2]["LOCALAPPDATA"], value)
            report = json.loads((output / "result.json").read_text(encoding="utf8"))
            self.assertEqual(report["diagnosticMode"], "local-app-data")
            self.assertIn("LOCALAPPDATA", report["environmentNames"])
            self.assertNotIn(value, json.dumps(report))
            self.assertFalse(report["nativeAcceptance"])

    def test_supplemental_controls_are_mutually_exclusive(self):
        with mock.patch.object(PROBE.Path, "mkdir") as mkdir, \
             mock.patch.object(PROBE, "trial") as trial, mock.patch("sys.stderr"):
            with self.assertRaises(SystemExit) as refused:
                PROBE.main(["--output", "unused-owned-output", "--captured-write-output", "--local-app-data"])
            self.assertEqual(refused.exception.code, 2)
            mkdir.assert_not_called()
            trial.assert_not_called()

    def test_command_boundaries_use_two_fresh_fixed_commands_without_changing_prior_controls(self):
        canonical = PROBE.commands()
        captured = PROBE.captured_write_output_commands()
        local = PROBE.local_app_data_commands()
        selected = PROBE.command_boundary_commands()
        self.assertEqual([row[0] for row in selected],
                         ["nested-snapshot-command-lookup", "nested-snapshot-fixed-module-import"])
        for _, script, count, resume in selected:
            self.assertEqual((count, resume), (2, "snapshot"))
            self.assertEqual(script.count("$ExecutionContext.InvokeCommand.GetCommand("), 1)
            self.assertEqual(script.count("$value = & $command 'owned-shell-test'"), 1)
            self.assertEqual(script.count("[Console]::Out.WriteLine($value)"), 1)
            self.assertIn("$command -isnot [System.Management.Automation.CmdletInfo]", script)
            self.assertIn("$command.ModuleName -cne 'Microsoft.PowerShell.Utility'", script)
            self.assertIn("$boundaryClock=[Diagnostics.Stopwatch]::StartNew()", script)
            for phase in ("before-command-lookup", "after-command-lookup",
                          "before-command-invoke", "after-command-invoke"):
                self.assertIn("owned-" + phase + "-ms:", script)
            self.assertLess(script.index("before-command-lookup"), script.index(".GetCommand("))
            self.assertLess(script.index(".GetCommand("), script.index("after-command-lookup"))
            self.assertLess(script.index("after-command-lookup"), script.index("before-command-invoke"))
            self.assertLess(script.index("before-command-invoke"), script.index("$value = & $command"))
            self.assertLess(script.index("$value = & $command"), script.index("after-command-invoke"))
            self.assertNotIn("Get-Command", script)
            self.assertNotIn("Get-Module", script)
            self.assertNotIn("Out-String", script)
            self.assertNotIn(";;", script.replace(" ", ""))
        self.assertNotIn("Import-Module", selected[0][1])
        imported = selected[1][1]
        self.assertIn("Import-Module -Name ($PSHOME+'\\Modules\\Microsoft.PowerShell.Utility"
                      "\\Microsoft.PowerShell.Utility.psd1') -ErrorAction Stop", imported)
        self.assertLess(imported.index("before-fixed-module-import"), imported.index("Import-Module"))
        self.assertLess(imported.index("Import-Module"), imported.index("after-fixed-module-import"))
        self.assertLess(imported.index("after-fixed-module-import"), imported.index("before-command-lookup"))
        self.assertEqual(PROBE.commands(), canonical)
        self.assertEqual(PROBE.captured_write_output_commands(), captured)
        self.assertEqual(PROBE.local_app_data_commands(), local)

    def test_command_boundary_keeps_each_original_deadline_and_fits_separate_step(self):
        self.assertEqual(PROBE.TRIAL_SECONDS, 20)
        self.assertEqual(PROBE.CLEANUP_SECONDS, 2)
        waits = len(PROBE.command_boundary_commands()) * (
            PROBE.TRIAL_SECONDS + PROBE.CLEANUP_SECONDS + 2 * PROBE.READER_JOIN_SECONDS)
        self.assertEqual(waits, 45)
        self.assertLess(waits, 60)

    def test_command_boundary_refuses_changed_captured_command_before_execution(self):
        with mock.patch.object(PROBE, "captured_write_output_commands",
                               return_value=[("owned", "changed-command", 2, "snapshot")]):
            with self.assertRaisesRegex(RuntimeError, "Captured Write-Output control changed"):
                PROBE.command_boundary_commands()

    def test_command_boundary_cli_uses_baseline_environment_and_reports_both_trials(self):
        with tempfile.TemporaryDirectory() as raw:
            output = (Path(raw) / "new-output").resolve()
            environment = {"SystemRoot": raw, "LOCALAPPDATA": "unselected-private-value",
                           "PSModulePath": "unselected-modules", "GITHUB_SHA": "a" * 40}
            row = {"diagnosticComplete": False, "nativeAcceptance": False}
            with mock.patch.object(PROBE, "os", types.SimpleNamespace(name="nt", environ=environment)), \
                 mock.patch.object(PROBE.shutil, "disk_usage", return_value=types.SimpleNamespace(free=500_000_000)), \
                 mock.patch.object(PROBE.Path, "is_file", return_value=True), \
                 mock.patch.object(PROBE, "trial", return_value=row) as trial, mock.patch("builtins.print"):
                self.assertEqual(PROBE.main(["--output", str(output), "--command-boundary"]), 0)
            self.assertEqual([call.args[3] for call in trial.call_args_list], PROBE.command_boundary_commands())
            self.assertTrue(all(call.args[2] == PROBE.selected_environment(environment)
                                for call in trial.call_args_list))
            report = json.loads((output / "result.json").read_text(encoding="utf8"))
            self.assertEqual(report["diagnosticMode"], "command-boundary")
            self.assertEqual(len(report["trials"]), 2)
            self.assertFalse(report["nativeAcceptance"] or report["diagnosticComplete"])
            self.assertNotIn("unselected-private-value", json.dumps(report))

    def test_command_boundary_refuses_values_and_other_modes_before_output(self):
        for extra in (["--command-boundary=true"],
                      ["--command-boundary", "--captured-write-output"],
                      ["--command-boundary", "--local-app-data"]):
            with self.subTest(extra=extra), mock.patch.object(PROBE.Path, "mkdir") as mkdir, \
                 mock.patch.object(PROBE, "trial") as trial, mock.patch("sys.stderr"):
                with self.assertRaises(SystemExit) as refused:
                    PROBE.main(["--output", "unused-owned-output", *extra])
                self.assertEqual(refused.exception.code, 2)
                mkdir.assert_not_called()
                trial.assert_not_called()

    def test_command_boundary_preserves_existing_output_and_never_starts_a_trial(self):
        with tempfile.TemporaryDirectory() as raw:
            marker = Path(raw) / "foreign-marker"
            marker.write_bytes(b"preserve")
            with mock.patch.object(PROBE, "os", types.SimpleNamespace(name="nt", environ={})), \
                 mock.patch.object(PROBE.shutil, "disk_usage", return_value=types.SimpleNamespace(free=500_000_000)), \
                 mock.patch.object(PROBE, "trial") as trial:
                with self.assertRaises(FileExistsError):
                    PROBE.main(["--output", raw, "--command-boundary"])
            self.assertEqual(marker.read_bytes(), b"preserve")
            trial.assert_not_called()

    def test_fixed_import_keeps_the_exact_original_script_and_normal_name_resolution(self):
        canonical = PROBE.commands()
        row = PROBE.fixed_import_original_commands()[0]
        self.assertEqual(row[0], "nested-snapshot-fixed-import-original")
        self.assertEqual(row[2:], canonical[-1][2:])
        self.assertTrue(row[1].endswith(canonical[-1][1]))
        self.assertEqual(row[1].count("Write-Output 'owned-windows-runtime'"), 1)
        self.assertNotIn(".GetCommand(", row[1])
        self.assertNotIn("& $command", row[1])
        self.assertIn("$originalImportClock=[Diagnostics.Stopwatch]::StartNew()", row[1])
        self.assertIn("Import-Module -Name ($PSHOME+'\\Modules\\Microsoft.PowerShell.Utility"
                      "\\Microsoft.PowerShell.Utility.psd1') -ErrorAction Stop", row[1])
        self.assertLess(row[1].index("owned-before-original-module-import-ms:"), row[1].index("Import-Module"))
        self.assertLess(row[1].index("Import-Module"), row[1].index("owned-after-original-module-import-ms:"))
        self.assertLess(row[1].index("owned-after-original-module-import-ms:"), row[1].index("owned-script-entry"))
        self.assertEqual(PROBE.commands(), canonical)
        self.assertEqual(PROBE.TRIAL_SECONDS, 20)
        self.assertEqual(PROBE.CLEANUP_SECONDS, 2)
        self.assertEqual(len(PROBE.fixed_import_original_commands()), 1)

    def test_fixed_import_refuses_a_changed_original_job_or_resume_definition(self):
        for count, resume in ((1, "snapshot"), (2, "primary")):
            with self.subTest(count=count, resume=resume), mock.patch.object(
                    PROBE, "commands", return_value=[("nested-snapshot-folder", "owned", count, resume)]):
                with self.assertRaisesRegex(RuntimeError, "Canonical folder control changed"):
                    PROBE.fixed_import_original_commands()

    def test_fixed_import_cli_uses_only_the_new_control_and_baseline_environment(self):
        with tempfile.TemporaryDirectory() as raw:
            output = (Path(raw) / "new-output").resolve()
            environment = {"SystemRoot": raw, "PSModulePath": "unselected-module-value"}
            row = {"diagnosticComplete": True, "nativeAcceptance": False}
            with mock.patch.object(PROBE, "os", types.SimpleNamespace(name="nt", environ=environment)), \
                 mock.patch.object(PROBE.shutil, "disk_usage", return_value=types.SimpleNamespace(free=500_000_000)), \
                 mock.patch.object(PROBE.Path, "is_file", return_value=True), \
                 mock.patch.object(PROBE, "trial", return_value=row) as trial, mock.patch("builtins.print"):
                self.assertEqual(PROBE.main(["--output", str(output), "--fixed-import-original"]), 0)
            self.assertEqual([call.args[3] for call in trial.call_args_list], PROBE.fixed_import_original_commands())
            self.assertEqual(trial.call_args.args[2], PROBE.selected_environment(environment))
            report = json.loads((output / "result.json").read_text(encoding="utf8"))
            self.assertEqual(report["diagnosticMode"], "fixed-import-original")
            self.assertFalse(report["nativeAcceptance"])
            self.assertEqual(len(report["trials"]), 1)
            self.assertNotIn("unselected-module-value", json.dumps(report))

    def test_fixed_import_refuses_other_modes_and_values_before_output(self):
        for extra in (["--fixed-import-original=true"],
                      *[["--fixed-import-original", flag] for flag in
                        ("--captured-write-output", "--local-app-data", "--command-boundary")]):
            with self.subTest(extra=extra), mock.patch.object(PROBE.Path, "mkdir") as mkdir, \
                 mock.patch.object(PROBE, "trial") as trial, mock.patch("sys.stderr"):
                with self.assertRaises(SystemExit) as refused:
                    PROBE.main(["--output", "unused-owned-output", *extra])
                self.assertEqual(refused.exception.code, 2)
                mkdir.assert_not_called()
                trial.assert_not_called()

    def test_fixed_import_preserves_existing_evidence_before_execution(self):
        with tempfile.TemporaryDirectory() as raw:
            marker = Path(raw) / "foreign-marker"
            marker.write_bytes(b"preserve")
            with mock.patch.object(PROBE, "os", types.SimpleNamespace(name="nt", environ={})), \
                 mock.patch.object(PROBE.shutil, "disk_usage", return_value=types.SimpleNamespace(free=500_000_000)), \
                 mock.patch.object(PROBE, "trial") as trial:
                with self.assertRaises(FileExistsError):
                    PROBE.main(["--output", raw, "--fixed-import-original"])
            self.assertEqual(marker.read_bytes(), b"preserve")
            trial.assert_not_called()

    def test_environment_excludes_credentials_profiles_and_unapproved_host_input(self):
        source = {"SystemDrive": "C:", "PATH": "owned-tools", "TEMP": "owned-temp",
                  "OPENAI_API_KEY": "owned-not-a-credential", "GOOGLE_APPLICATION_CREDENTIALS": "owned-not-a-path",
                  "PSModulePath": "unapproved-modules", "YOKE_UNKNOWN": "unapproved"}
        self.assertEqual(PROBE.selected_environment(source),
                         {"SystemDrive": "C:", "PATH": "owned-tools", "TEMP": "owned-temp",
                          "CI": "1", "DO_NOT_TRACK": "1", "NO_COLOR": "1"})
        self.assertEqual(source["PSModulePath"], "unapproved-modules")

    def test_bad_source_revision_refuses_before_output_or_process_creation(self):
        with mock.patch.object(PROBE, "os", types.SimpleNamespace(name="nt", environ={"GITHUB_SHA": "bad\nrevision"})), \
             mock.patch.object(PROBE.Path, "mkdir") as mkdir, mock.patch.object(PROBE, "trial") as trial:
            with self.assertRaisesRegex(RuntimeError, "source revision invalid"):
                PROBE.main(["--output", "unused-owned-output"])
        mkdir.assert_not_called()
        trial.assert_not_called()

    def test_low_disk_refuses_before_output_or_process_creation(self):
        with mock.patch.object(PROBE, "os", types.SimpleNamespace(name="nt", environ={"GITHUB_SHA": "a" * 40})), \
             mock.patch.object(PROBE.shutil, "disk_usage", return_value=types.SimpleNamespace(free=499_999_999)), \
             mock.patch.object(PROBE.Path, "mkdir") as mkdir, mock.patch.object(PROBE, "trial") as trial:
            with self.assertRaisesRegex(RuntimeError, "disk reserve unavailable"):
                PROBE.main(["--output", "unused-owned-output"])
        mkdir.assert_not_called()
        trial.assert_not_called()

    def test_existing_output_is_preserved_without_trial_execution(self):
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            marker = root / "foreign-marker"
            marker.write_bytes(b"preserve")
            with mock.patch.object(PROBE, "os", types.SimpleNamespace(name="nt", environ={"GITHUB_SHA": "a" * 40})), \
                 mock.patch.object(PROBE.shutil, "disk_usage", return_value=types.SimpleNamespace(free=500_000_000)), \
                 mock.patch.object(PROBE, "trial") as trial:
                with self.assertRaises(FileExistsError):
                    PROBE.main(["--output", str(root)])
            self.assertEqual(marker.read_bytes(), b"preserve")
            trial.assert_not_called()

    def test_assignment_failure_terminates_only_the_owned_root_and_closes_handles(self):
        row, api, win, error = simulated_trial()
        self.assertIsNone(error)
        self.assertFalse(row["diagnosticComplete"])
        self.assertFalse(row["assignedBeforeResume"])
        self.assertEqual(row["activeAfterCleanup"], [0])
        self.assertTrue(row["rootExitedAfterCleanup"])
        self.assertFalse(row["nativeAcceptance"])
        win.TerminateProcess.assert_called_once_with(101, 124)
        api.TerminateJobObject.assert_called_once_with(501, 125)
        self.assertEqual([call.args[0] for call in api.CloseHandle.call_args_list], [102, 101, 501])
        api.ResumeThread.assert_not_called()
        self.assertEqual(row["membersBeforeCleanup"]["unavailable"], ["not-assigned"])

    def test_failed_termination_remains_unproven_even_when_accounting_is_zero(self):
        for options, expected in [({"termination": False}, "job-termination"), ({"close": False}, "handle-close")]:
            row, api, _win, error = simulated_trial(**options)
            self.assertIsInstance(error, PROBE.CleanupUnproven)
            self.assertFalse(row["diagnosticComplete"])
            self.assertTrue(row["cleanupUnproven"])
            self.assertEqual(row["activeAfterCleanup"], [0])
            self.assertIn(expected, row["cleanupErrors"])
            self.assertEqual([call.args[0] for call in api.CloseHandle.call_args_list], [102, 101, 501])

    def test_zero_root_exit_does_not_hide_descendants_killed_by_cleanup(self):
        row, _api, _win, error = simulated_trial(assigned=True, leaking=True)
        self.assertIsNone(error)
        self.assertEqual(row["exitCode"], 0)
        self.assertEqual(row["activeBeforeCleanup"], [1])
        self.assertEqual(row["activeAfterCleanup"], [0])
        self.assertFalse(row["treeExitedBeforeCleanup"])
        self.assertFalse(row["diagnosticComplete"])

    def test_member_sampling_keeps_the_original_deadline_and_runs_before_termination(self):
        now, samples = [0.0], []

        def wait(_handle, milliseconds):
            if milliseconds == 10:
                now[0] = 19.8 if now[0] == 0 else 20.0
                return 258
            return 0

        def observe(api, _jobs, _root, _pid, _handles, end):
            api.TerminateJobObject.assert_not_called()
            samples.append((now[0], end))
            return {"complete": True, "members": [], "unavailable": []}

        row, api, win, error = simulated_trial(
            assigned=True, clock=lambda: now[0], wait=wait, observation=observe)
        self.assertIsNone(error)
        self.assertEqual(samples, [(19.8, 20.0)])
        self.assertTrue(row["timedOut"])
        self.assertFalse(row["diagnosticComplete"])
        api.TerminateJobObject.assert_called_once_with(501, 125)
        win.GetExitCodeProcess.assert_not_called()

    def test_expired_trial_does_not_start_late_member_queries(self):
        now = [0.0]
        original_observer = PROBE.observe_members

        def wait(_handle, milliseconds):
            if milliseconds == 10:
                now[0] = 20.0
                return 258
            return 0

        row, api, _win, error = simulated_trial(
            assigned=True, clock=lambda: now[0], wait=wait, observation=original_observer)
        self.assertIsNone(error)
        self.assertEqual(row["membersBeforeCleanup"]["unavailable"], ["deadline"])
        self.assertIsNone(row["membersBeforeCleanup"]["members"])
        api.OpenProcess.assert_not_called()
        self.assertTrue(all(call.args[1] == 1 for call in api.QueryInformationJobObject.call_args_list))
        api.TerminateJobObject.assert_called_once_with(501, 125)

    def test_observation_exception_retains_handles_and_never_skips_cleanup(self):
        def observe(_api, _jobs, _root, _pid, handles, _end):
            handles.append(601)
            raise RuntimeError("fixed observation failure")

        row, api, _win, error = simulated_trial(assigned=True, observation=observe)
        self.assertIsNone(error)
        self.assertFalse(row["diagnosticComplete"])
        self.assertEqual(row["membersBeforeCleanup"]["unavailable"], ["observation-error"])
        self.assertEqual(row["activeAfterCleanup"], [0])
        api.TerminateJobObject.assert_called_once_with(501, 125)
        self.assertEqual([call.args[0] for call in api.CloseHandle.call_args_list], [102, 101, 601, 501])

    def test_unavailable_metadata_never_masks_cleanup_failure_or_claims_completion(self):
        def observe(_api, _jobs, _root, _pid, handles, _end):
            handles.append(601)
            return {"complete": False, "members": None, "unavailable": ["membership-query"]}

        for close in (True, False):
            with self.subTest(close=close):
                row, api, _win, error = simulated_trial(assigned=True, close=close, observation=observe)
                self.assertFalse(row["diagnosticComplete"])
                self.assertEqual(row["activeAfterCleanup"], [0])
                api.TerminateJobObject.assert_called_once_with(501, 125)
                self.assertEqual([call.args[0] for call in api.CloseHandle.call_args_list],
                                 [102, 101, 601, 501])
                if close:
                    self.assertIsNone(error)
                else:
                    self.assertIsInstance(error, PROBE.CleanupUnproven)
                    self.assertTrue(row["cleanupUnproven"])
                    self.assertIn("handle-close", row["cleanupErrors"])

    def test_cleanup_uncertainty_is_saved_and_stops_further_trials(self):
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            output = root / "new-output"
            environment = {"GITHUB_SHA": "a" * 40, "SystemRoot": str(root)}
            failed = {"name": "owned", "diagnosticComplete": False, "cleanupUnproven": True,
                      "nativeAcceptance": False}
            with mock.patch.object(PROBE, "os", types.SimpleNamespace(name="nt", environ=environment)), \
                 mock.patch.object(PROBE.shutil, "disk_usage", return_value=types.SimpleNamespace(free=500_000_000)), \
                 mock.patch.object(PROBE.Path, "is_file", return_value=True), \
                 mock.patch.object(PROBE, "trial", side_effect=PROBE.CleanupUnproven(failed)) as trial:
                with self.assertRaises(PROBE.CleanupUnproven):
                    PROBE.main(["--output", str(output)])
            report = json.loads((output / "result.json").read_text(encoding="utf8"))
            self.assertFalse(report["nativeAcceptance"] or report["diagnosticComplete"])
            self.assertEqual(report["trials"], [failed])
            self.assertEqual(trial.call_count, 1)

    @unittest.skipUnless(os.name == "nt", "Actual owned PowerShell requires Windows")
    def test_actual_nested_snapshot_resume_records_host_and_both_empty_jobs(self):
        with tempfile.TemporaryDirectory() as raw:
            shell = Path(os.environ["SystemRoot"]) / "System32/WindowsPowerShell/v1.0/powershell.exe"
            row = PROBE.trial(shell, Path(raw), PROBE.selected_environment(os.environ),
                              initialized_owned_fixture(PROBE.commands()[-1]))
        self.assertTrue(row["diagnosticComplete"], row)
        self.assertTrue(row["startedSuspended"] and row["assignedBeforeResume"])
        self.assertTrue(row["primaryObservedResumed"])
        self.assertTrue(row["rootExitedAfterCleanup"])
        self.assertEqual(row["activeAfterCleanup"], [0, 0])
        self.assertEqual(row["stdout"].strip(), "owned-windows-runtime")
        self.assertIn("owned-script-entry", row["stderr"])
        self.assertIn("owned-after-folder", row["stderr"])
        self.assertIn("owned-before-write-output", row["stderr"])
        self.assertIn("owned-after-write-output", row["stderr"])
        self.assertIn("owned-host-version:", row["stderr"])
        self.assertFalse(row["nativeAcceptance"])

    @unittest.skipUnless(os.name == "nt", "Actual owned PowerShell requires Windows")
    def test_actual_owned_phase_paths_keep_unicode_quotes_and_semicolon_as_data(self):
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            marker = root / "phase ' İ π ; owned"
            python_marker = root / "python ' İ π ; owned"
            # Use the same JSON string boundary as the owned REPL test cell.
            namespace = {}
            exec("with open(" + json.dumps(str(python_marker), ensure_ascii=False)
                 + ",'w',encoding='ascii') as phase:\n phase.write('entered')", namespace)
            quoted = str(marker).replace("'", "''")
            command = ("[IO.File]::WriteAllText('" + quoted + "','entered'); "
                       "[Console]::Error.WriteLine('owned-before-write-output'); "
                       "Write-Output owned-shell-test; "
                       "[Console]::Error.WriteLine('owned-after-write-output'); "
                       "[IO.File]::WriteAllText('" + quoted + "','finished')")
            shell = Path(os.environ["SystemRoot"]) / "System32/WindowsPowerShell/v1.0/powershell.exe"
            row = PROBE.trial(shell, root, PROBE.selected_environment(os.environ),
                              initialized_owned_fixture(("owned-phase-path", command, 2, "snapshot")))
            self.assertTrue(row["diagnosticComplete"], row)
            self.assertEqual(marker.read_bytes(), b"finished")
            self.assertEqual(python_marker.read_bytes(), b"entered")
            self.assertEqual(row["activeAfterCleanup"], [0, 0])
            self.assertEqual(row["stdout"].strip(), "owned-shell-test")
            self.assertIn("owned-before-write-output", row["stderr"])
            self.assertIn("owned-after-write-output", row["stderr"])

    def test_fixture_initialization_preserves_scripts_jobs_and_every_raw_control(self):
        plans = [PROBE.commands(), PROBE.captured_write_output_commands(),
                 PROBE.local_app_data_commands(), PROBE.command_boundary_commands(),
                 PROBE.fixed_import_original_commands()]
        for definition in (PROBE.commands()[-1], ("owned", "[Console]::Out.WriteLine('owned')", 2, "snapshot")):
            prepared = initialized_owned_fixture(definition)
            self.assertTrue(prepared[1].endswith(definition[1]))
            self.assertEqual(prepared[1], FIXTURE_BOOTSTRAP + definition[1])
            self.assertEqual(prepared[2:], definition[2:])
        self.assertEqual(FIXTURE_BOOTSTRAP.count(FIXTURE_MODULE), 1)
        self.assertIn("-ErrorAction Stop", FIXTURE_BOOTSTRAP)
        self.assertIn("exit 125", FIXTURE_BOOTSTRAP)
        self.assertEqual(plans, [PROBE.commands(), PROBE.captured_write_output_commands(),
                                PROBE.local_app_data_commands(), PROBE.command_boundary_commands(),
                                PROBE.fixed_import_original_commands()])

    @unittest.skipUnless(os.name == "nt", "Actual owned PowerShell requires Windows")
    def test_missing_fixture_import_exits_before_the_original_payload(self):
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            missing = root / "missing ' π ; module.psd1"
            marker = root / "payload-entered"
            bootstrap = FIXTURE_BOOTSTRAP.replace(FIXTURE_MODULE, "'" + str(missing).replace("'", "''") + "'", 1)
            script = bootstrap + "[IO.File]::WriteAllText($env:YOKE_OWNED_PAYLOAD_MARKER,'entered'); [Console]::Out.WriteLine('unexpected-payload')"
            environment = PROBE.selected_environment(os.environ)
            environment["YOKE_OWNED_PAYLOAD_MARKER"] = str(marker)
            shell = Path(os.environ["SystemRoot"]) / "System32/WindowsPowerShell/v1.0/powershell.exe"
            row = PROBE.trial(shell, root, environment, ("missing-fixture-import", script, 2, "snapshot"))
            self.assertEqual(row["exitCode"], 125, row)
            self.assertFalse(row["diagnosticComplete"] or row["nativeAcceptance"])
            self.assertFalse(row["timedOut"])
            self.assertTrue(row["treeExitedBeforeCleanup"] and row["rootExitedAfterCleanup"])
            self.assertEqual(row["activeBeforeCleanup"], [0, 0])
            self.assertEqual(row["activeAfterCleanup"], [0, 0])
            self.assertEqual(row["stdout"], "")
            self.assertEqual(row["stderr"].strip(), "Owned PowerShell Utility module unavailable")
            self.assertFalse(marker.exists() or missing.exists())

    @unittest.skipUnless(os.name == "nt", "Actual owned PowerShell requires Windows")
    def test_fixture_fixed_system_import_does_not_execute_foreign_module_path(self):
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            marker = root / "foreign ' π ; loaded"
            modules = root / "foreign-modules"
            module = modules / "Microsoft.PowerShell.Utility"
            module.mkdir(parents=True)
            (module / "Microsoft.PowerShell.Utility.psd1").write_text(
                "@{RootModule='owned.psm1';ModuleVersion='1.0.0';FunctionsToExport=@('Write-Output')}", encoding="utf8")
            (module / "owned.psm1").write_text(
                "[IO.File]::WriteAllText($env:YOKE_OWNED_FOREIGN_MARKER,'loaded'); "
                "function Write-Output { param($value) [Console]::Out.WriteLine('foreign-output') }", encoding="utf8")
            environment = PROBE.selected_environment(os.environ)
            environment.update(PSModulePath=str(modules), YOKE_OWNED_FOREIGN_MARKER=str(marker))
            shell = Path(os.environ["SystemRoot"]) / "System32/WindowsPowerShell/v1.0/powershell.exe"
            row = PROBE.trial(shell, root, environment, initialized_owned_fixture(PROBE.commands()[-1]))
            self.assertTrue(row["diagnosticComplete"], row)
            self.assertEqual(row["stdout"].strip(), "owned-windows-runtime")
            self.assertEqual(row["activeBeforeCleanup"], [0, 0])
            self.assertEqual(row["activeAfterCleanup"], [0, 0])
            self.assertFalse(row["nativeAcceptance"] or marker.exists())


class OwnedMemberObservations(unittest.TestCase):
    def api(self, pids=(103, 105), assigned=None, returned=None, membership=True):
        api = mock.Mock(spec=["QueryInformationJobObject", "OpenProcess", "IsProcessInJob",
                              "GetProcessTimes", "QueryFullProcessImageNameW",
                              "WaitForSingleObject", "GetExitCodeProcess"])
        api.OpenProcess.return_value = 601

        def query(job, kind, buffer, _size, _returned):
            self.assertEqual((job, kind), (501, 3))
            data = c.cast(buffer, c.POINTER(PROBE.JobProcessIds)).contents
            data.assigned = len(pids) if assigned is None else assigned
            data.returned = len(pids) if returned is None else returned
            data.ids[:len(pids)] = pids
            return 1

        def owned(_handle, job, output):
            self.assertEqual(job, 501)
            if membership is None:
                return 0
            c.cast(output, c.POINTER(c.c_int32))[0] = int(membership)
            return 1

        def times(handle, created, *_rest):
            c.cast(created, c.POINTER(c.c_uint64))[0] = 12345 if handle == 101 else 67890
            return 1

        def image(_handle, flags, output, length):
            self.assertEqual(flags, 0)
            output.value = "C:\\owned\\İ π ' ; probe.exe"
            c.cast(length, c.POINTER(c.c_uint32))[0] = len(output.value)
            return 1

        def exit_code(_handle, output):
            c.cast(output, c.POINTER(c.c_uint32))[0] = 259
            return 1

        api.QueryInformationJobObject.side_effect = query
        api.IsProcessInJob.side_effect = owned
        api.GetProcessTimes.side_effect = times
        api.QueryFullProcessImageNameW.side_effect = image
        api.WaitForSingleObject.side_effect = lambda handle, _timeout: 0 if handle == 101 else 258
        api.GetExitCodeProcess.side_effect = exit_code
        return api

    def observe(self, api):
        handles = []
        with mock.patch.object(PROBE.time, "monotonic", return_value=0):
            result = PROBE.observe_members(api, [501, 502], 101, 103, handles, 1)
        return result, handles

    def test_nested_jobs_use_one_outer_list_and_held_membership_checked_identities(self):
        api = self.api()
        result, handles = self.observe(api)
        self.assertTrue(result["complete"])
        self.assertEqual((result["assignedAtQuery"], result["returnedAtQuery"]), (2, 2))
        self.assertEqual([member["pid"] for member in result["members"]], [103, 105])
        self.assertEqual(handles, [601])
        api.OpenProcess.assert_called_once_with(0x00101000, False, 105)
        api.QueryInformationJobObject.assert_called_once()
        self.assertEqual([call.args[:2] for call in api.IsProcessInJob.call_args_list],
                         [(101, 501), (601, 501)])
        root, child = result["members"]
        self.assertEqual((root["creationTime100ns"], child["creationTime100ns"]), (12345, 67890))
        self.assertEqual(child["imagePath"], "C:\\owned\\İ π ' ; probe.exe")
        self.assertTrue(root["exited"])
        self.assertEqual(root["exitCode"], 259)  # Signaled handle, even with STILL_ACTIVE's numeric code.
        self.assertFalse(child["exited"])
        self.assertIsNone(child["exitCode"])
        api.GetExitCodeProcess.assert_called_once()

    def test_unavailable_list_is_not_an_empty_complete_observation(self):
        api = self.api()
        api.QueryInformationJobObject.side_effect = lambda *_args: 0
        result, handles = self.observe(api)
        self.assertFalse(result["complete"])
        self.assertIsNone(result["members"])
        self.assertEqual(result["unavailable"], ["job-process-list"])
        self.assertEqual(handles, [])
        api.OpenProcess.assert_not_called()
        api.IsProcessInJob.assert_not_called()

    def test_malformed_pid_lists_are_rejected_before_opening_or_inspecting_members(self):
        for options in ({"pids": (0,)}, {"pids": (0x1_0000_0000,)}, {"pids": (103, 103)},
                        {"returned": PROBE.MAX_MEMBERS + 1},
                        {"assigned": 1, "returned": 2}):
            with self.subTest(options=options):
                api = self.api(**options)
                result, handles = self.observe(api)
                self.assertFalse(result["complete"])
                self.assertIsNone(result["members"])
                self.assertEqual(result["unavailable"], ["invalid-job-process-list"])
                self.assertEqual(handles, [])
                api.OpenProcess.assert_not_called()
                api.IsProcessInJob.assert_not_called()

    def test_truncated_list_is_explicitly_partial_without_unbounded_retry(self):
        api = self.api(pids=(103,), assigned=PROBE.MAX_MEMBERS + 1)
        result, _handles = self.observe(api)
        self.assertFalse(result["complete"])
        self.assertEqual(result["unavailable"], ["job-process-list-capacity"])
        self.assertEqual(result["returnedAtQuery"], 1)
        self.assertEqual(len(result["members"]), 1)
        api.QueryInformationJobObject.assert_called_once()

    def test_pid_reuse_or_membership_failure_blocks_all_process_metadata(self):
        for membership, reason in ((False, "not-in-owned-job"), (None, "membership-query")):
            with self.subTest(membership=membership):
                api = self.api(pids=(105,), membership=membership)
                result, handles = self.observe(api)
                self.assertFalse(result["complete"])
                self.assertEqual(handles, [601])
                member = result["members"][0]
                self.assertEqual(member["unavailable"], [reason])
                for field in ("creationTime100ns", "imagePath", "exited", "exitCode"):
                    self.assertIsNone(member[field])
                api.GetProcessTimes.assert_not_called()
                api.QueryFullProcessImageNameW.assert_not_called()
                api.WaitForSingleObject.assert_not_called()
                api.GetExitCodeProcess.assert_not_called()

    def test_denied_handle_is_unavailable_without_process_metadata_access(self):
        api = self.api(pids=(105,))
        api.OpenProcess.return_value = None
        result, handles = self.observe(api)
        self.assertFalse(result["complete"])
        self.assertEqual(result["members"][0]["unavailable"], ["process-open"])
        self.assertEqual(handles, [])
        api.IsProcessInJob.assert_not_called()
        api.GetProcessTimes.assert_not_called()
        api.QueryFullProcessImageNameW.assert_not_called()
        api.WaitForSingleObject.assert_not_called()

    def test_failed_metadata_queries_do_not_manufacture_identity_or_exit_state(self):
        api = self.api(pids=(105,))
        api.GetProcessTimes.side_effect = lambda *_args: 0
        api.QueryFullProcessImageNameW.side_effect = lambda *_args: 0
        api.WaitForSingleObject.side_effect = None
        api.WaitForSingleObject.return_value = 0xffffffff
        result, handles = self.observe(api)
        self.assertFalse(result["complete"])
        self.assertEqual(handles, [601])
        member = result["members"][0]
        self.assertEqual(member["unavailable"], ["creation-time", "image-name", "exit-state"])
        self.assertTrue(member["inOuterOwnedJob"])
        for field in ("creationTime100ns", "imagePath", "exited", "exitCode"):
            self.assertIsNone(member[field])
        api.GetExitCodeProcess.assert_not_called()

    def test_observation_deadline_prevents_new_queries_and_retains_opened_handles(self):
        for clocks, opened in (([1], []), ([0, 0, 1, 1], [601])):
            with self.subTest(clocks=clocks):
                api, handles = self.api(pids=(105,)), []
                with mock.patch.object(PROBE.time, "monotonic", side_effect=clocks):
                    result = PROBE.observe_members(api, [501, 502], 101, 103, handles, 1)
                self.assertFalse(result["complete"])
                self.assertIn("deadline", result["unavailable"])
                self.assertEqual(handles, opened)
                if not opened:
                    api.QueryInformationJobObject.assert_not_called()
                    api.OpenProcess.assert_not_called()
                api.IsProcessInJob.assert_not_called()
                api.GetProcessTimes.assert_not_called()
                api.QueryFullProcessImageNameW.assert_not_called()
                api.WaitForSingleObject.assert_not_called()


if __name__ == "__main__":
    unittest.main()
