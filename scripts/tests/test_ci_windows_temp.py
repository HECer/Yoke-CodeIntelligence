import importlib.util
import os
from pathlib import Path
import tempfile
import unittest
from unittest import mock

SPEC = importlib.util.spec_from_file_location("ci_windows_temp", Path(__file__).resolve().parents[1] / "ci_windows_temp.py")
CI_TEMP = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CI_TEMP)


class CanonicalTemporaryFixture(unittest.TestCase):
    def test_ordinary_ascii_drive_paths_are_kept_exact(self):
        for value in [r"C:\Users\runneradmin\AppData\Local\Temp", "D:/a/_temp", "C:\\"]:
            self.assertEqual(CI_TEMP.ascii_root(value), value)

    def test_aliases_metacharacters_unicode_relative_and_traversal_are_not_emitted(self):
        for value in [r"C:\Users\RUNNER~1\Temp", r"C:\with space\Temp", r"C:\π\Temp", "relative",
                      r"\\server\share\Temp", r"\\?\C:\Temp", "C:Temp", r"C:\a\..\Temp",
                      r"C:\a\.\Temp", r"C:\a\\Temp", "C:\\Temp\nINJECT=value", r"C:\a&b", r"C:\a%VAR%"]:
            with self.subTest(value=value), self.assertRaises(RuntimeError):
                CI_TEMP.ascii_root(value)

    def test_actual_directory_identity_and_same_object_are_observed_without_mutation(self):
        with tempfile.TemporaryDirectory(prefix="yoke-owned-canonical-fixture-") as directory:
            path = Path(directory)
            before = CI_TEMP.directory_identity(path)
            # Filesystem identity is tested on every host; the Windows path
            # spelling policy has independent exact-string tests above.
            with mock.patch.object(CI_TEMP, "ascii_root", side_effect=lambda value: value):
                result = CI_TEMP.canonical_temp(path)
            self.assertTrue(result["sameDirectoryIdentityObserved"])
            self.assertEqual(CI_TEMP.directory_identity(path), before)
            self.assertEqual(list(path.iterdir()), [])

    def test_changed_identity_refuses_instead_of_selecting_another_root(self):
        with tempfile.TemporaryDirectory(prefix="yoke-owned-canonical-fixture-") as directory:
            with mock.patch.object(CI_TEMP, "ascii_root", side_effect=lambda value: value), \
                 mock.patch.object(CI_TEMP, "directory_identity", side_effect=[(1, 2), (1, 3)]):
                with self.assertRaisesRegex(RuntimeError, "identity changed"):
                    CI_TEMP.canonical_temp(Path(directory))

    def test_unknown_directory_identity_is_not_same_object_evidence(self):
        value = mock.Mock(st_mode=0o040700, st_file_attributes=0, st_dev=1, st_ino=0)
        with mock.patch.object(Path, "stat", return_value=value):
            with self.assertRaisesRegex(RuntimeError, "identity unavailable"):
                CI_TEMP.directory_identity(Path("unused"))

    @unittest.skipUnless(os.name == "nt", "Windows-only CI environment writer")
    def test_ci_writer_emits_only_the_observed_root_and_preserves_existing_entries(self):
        with tempfile.TemporaryDirectory(prefix="yoke-owned-canonical-fixture-") as directory:
            root = Path(directory)
            env_file = root / "environment"
            env_file.write_bytes(b"OWNED=before")
            with mock.patch.dict(CI_TEMP.os.environ, {"GITHUB_ACTIONS": "true", "GITHUB_ENV": str(env_file)}), \
                 mock.patch.object(CI_TEMP, "runtime_temp", return_value=root), \
                 mock.patch("builtins.print"):
                self.assertEqual(CI_TEMP.main(), 0)
            canonical = str(root.resolve())
            self.assertEqual(env_file.read_bytes(),
                             ("OWNED=before\nTEMP=" + canonical + "\nTMP=" + canonical + "\n").encode())

    def test_linked_ancestor_refuses_before_resolution_or_environment_write(self):
        with tempfile.TemporaryDirectory(prefix="yoke-owned-canonical-fixture-") as directory:
            with mock.patch.object(Path, "is_junction", return_value=True), \
                 mock.patch.object(Path, "resolve", side_effect=AssertionError("Must not follow link")):
                with self.assertRaisesRegex(RuntimeError, "Linked CI"):
                    CI_TEMP.canonical_temp(Path(directory))

    def test_file_and_missing_root_are_not_directory_fixtures(self):
        with tempfile.TemporaryDirectory(prefix="yoke-owned-canonical-fixture-") as directory:
            file = Path(directory) / "inert.txt"
            file.write_bytes(b"owned fixture")
            with self.assertRaises(RuntimeError):
                CI_TEMP.directory_identity(file)
            with self.assertRaises(FileNotFoundError):
                CI_TEMP.canonical_temp(Path(directory) / "missing")

    def test_non_ci_entry_refuses_before_any_environment_file_access(self):
        with mock.patch.dict(CI_TEMP.os.environ, {"GITHUB_ACTIONS": "false"}), \
             mock.patch.object(CI_TEMP, "canonical_temp", side_effect=AssertionError("Must not inspect")), \
             mock.patch.object(Path, "open", side_effect=AssertionError("Must not write")):
            with self.assertRaises(RuntimeError):
                CI_TEMP.main()

    def test_native_runtime_api_failure_and_truncation_refuse_without_fallback(self):
        for returned in [0, 32768, 32769, 8]:
            api = mock.Mock()

            def read(capacity, output):
                self.assertEqual(capacity, 32768)
                output.value = "C:\\Temp\\"
                return returned

            api.GetTempPath2W.side_effect = read
            with self.subTest(returned=returned), mock.patch.object(CI_TEMP.ctypes, "WinDLL", create=True, return_value=api):
                if returned == 8:
                    self.assertEqual(str(CI_TEMP.runtime_temp()), "C:\\Temp\\" if os.name != "nt" else "C:\\Temp")
                else:
                    with self.assertRaises(RuntimeError):
                        CI_TEMP.runtime_temp()

    @unittest.skipUnless(os.name == "nt", "Actual Windows long-path observation")
    def test_actual_windows_long_temp_spelling_keeps_the_same_object(self):
        with tempfile.TemporaryDirectory(prefix="yoke-owned-canonical-fixture-") as directory:
            result = CI_TEMP.canonical_temp(directory)
            self.assertTrue(result["sameDirectoryIdentityObserved"])
            self.assertEqual(Path(result["canonical"]).stat().st_ino, Path(directory).stat().st_ino)

    @unittest.skipUnless(os.name == "nt", "Actual Rust-compatible Windows temporary root")
    def test_actual_windows_runtime_api_selects_an_existing_directory(self):
        root = CI_TEMP.runtime_temp()
        self.assertTrue(root.is_absolute())
        self.assertEqual(len(CI_TEMP.directory_identity(root)), 2)

    @unittest.skipUnless(os.name == "nt", "Actual DOS short-path observation requires Windows")
    def test_actual_dos_alias_expands_to_the_same_long_directory(self):
        with tempfile.TemporaryDirectory(prefix="yoke-owned-canonical-fixture-") as directory:
            api = CI_TEMP.ctypes.WinDLL("kernel32", use_last_error=True)
            function = api.GetShortPathNameW
            function.argtypes = [CI_TEMP.ctypes.c_wchar_p, CI_TEMP.ctypes.c_wchar_p, CI_TEMP.ctypes.c_uint32]
            function.restype = CI_TEMP.ctypes.c_uint32
            buffer = CI_TEMP.ctypes.create_unicode_buffer(32768)
            size = function(directory, buffer, len(buffer))
            self.assertTrue(0 < size < len(buffer))
            if "~" not in buffer.value:
                self.skipTest("Volume supplies no DOS short alias for this owned fixture")
            with self.assertRaises(RuntimeError):
                CI_TEMP.ascii_root(buffer.value)
            result = CI_TEMP.canonical_temp(buffer.value)
            self.assertEqual(Path(result["canonical"]), Path(directory).resolve())
            self.assertTrue(result["sameDirectoryIdentityObserved"])


if __name__ == "__main__":
    unittest.main()
