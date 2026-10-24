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


def simulated_trial(assigned=False, termination=True, leaking=False, close=True):
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
                                TerminateProcess=mock.Mock(), WaitForSingleObject=mock.Mock(return_value=0),
                                GetExitCodeProcess=mock.Mock(return_value=0))
    error = None
    with mock.patch.dict(sys.modules, {"_winapi": win, "msvcrt": types.SimpleNamespace(get_osfhandle=lambda fd: fd)}), \
         mock.patch.object(PROBE.shutil, "disk_usage", return_value=types.SimpleNamespace(free=500_000_000)), \
         mock.patch.object(PROBE, "windows_api", return_value=(api, Extended, None, None)), \
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
            row = PROBE.trial(shell, Path(raw), PROBE.selected_environment(os.environ), PROBE.commands()[-1])
        self.assertTrue(row["diagnosticComplete"], row)
        self.assertTrue(row["startedSuspended"] and row["assignedBeforeResume"])
        self.assertTrue(row["primaryObservedResumed"])
        self.assertTrue(row["rootExitedAfterCleanup"])
        self.assertEqual(row["activeAfterCleanup"], [0, 0])
        self.assertEqual(row["stdout"].strip(), "owned-windows-runtime")
        self.assertIn("owned-script-entry", row["stderr"])
        self.assertIn("owned-after-folder", row["stderr"])
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
                       "Write-Output owned-shell-test; "
                       "[IO.File]::WriteAllText('" + quoted + "','finished')")
            shell = Path(os.environ["SystemRoot"]) / "System32/WindowsPowerShell/v1.0/powershell.exe"
            row = PROBE.trial(shell, root, PROBE.selected_environment(os.environ),
                              ("owned-phase-path", command, 2, "snapshot"))
            self.assertTrue(row["diagnosticComplete"], row)
            self.assertEqual(marker.read_bytes(), b"finished")
            self.assertEqual(python_marker.read_bytes(), b"entered")
            self.assertEqual(row["activeAfterCleanup"], [0, 0])
            self.assertEqual(row["stdout"].strip(), "owned-shell-test")


if __name__ == "__main__":
    unittest.main()
