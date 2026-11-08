import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock

SPEC = importlib.util.spec_from_file_location('host_memory_probe', Path(__file__).resolve().parents[1] / 'host_memory_probe.py')
PROBE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PROBE)


def refusal():
    return {'schema': 1, 'mode': 'self', 'requestedBytes': PROBE.MEMORY,
            'vmObservationAvailable': True, 'virtualBytesBefore': 2**40,
            'setLimitAttempted': True, 'setLimitSucceeded': False, 'setLimitErrno': 22,
            'limitObservationAvailable': True, 'observedSoftBytes': 2**63 - 1,
            'observedHardBytes': 2**63 - 1, 'exactLimitInstalled': False,
            'allocationProbeAttempted': False, 'mallocDenied': None, 'mallocErrno': 0,
            'mmapDenied': None, 'mmapErrno': 0, 'nativeAcceptance': False}


class HostMemoryDiagnostic(unittest.TestCase):
    def test_actual_api_refusal_is_data_without_allocation_or_native_acceptance(self):
        value = PROBE.decode(json.dumps(refusal()).encode(), 'self')
        self.assertFalse(value['setLimitSucceeded'])
        self.assertFalse(value['allocationProbeAttempted'])
        self.assertFalse(value['nativeAcceptance'])

    def test_exact_header_and_observed_allocation_are_independent(self):
        value = refusal()
        value.update(setLimitSucceeded=True, setLimitErrno=0, observedSoftBytes=PROBE.MEMORY,
                     observedHardBytes=PROBE.MEMORY, exactLimitInstalled=True,
                     allocationProbeAttempted=True, mallocDenied=False, mmapDenied=True, mmapErrno=12)
        decoded = PROBE.decode(json.dumps(value).encode(), 'self')
        self.assertFalse(decoded['mallocDenied'])
        self.assertTrue(decoded['mmapDenied'])
        self.assertFalse(decoded['nativeAcceptance'])

    def test_inherited_limit_cannot_claim_a_self_installation(self):
        value = refusal()
        value.update(mode='inherited', setLimitAttempted=False, setLimitSucceeded=None, setLimitErrno=0)
        self.assertIsNone(PROBE.decode(json.dumps(value).encode(), 'inherited')['setLimitSucceeded'])
        value['setLimitAttempted'] = True
        with self.assertRaises(RuntimeError):
            PROBE.decode(json.dumps(value).encode(), 'inherited')

    def test_forged_acceptance_allocation_admission_and_numeric_booleans_refuse(self):
        for key, bad in [('nativeAcceptance', True), ('allocationProbeAttempted', True),
                         ('exactLimitInstalled', True), ('requestedBytes', True),
                         ('schema', True), ('virtualBytesBefore', -1), ('observedHardBytes', 2**64),
                         ('mmapDenied', False), ('setLimitSucceeded', None), ('mallocErrno', -1)]:
            value = refusal()
            value[key] = bad
            with self.subTest(key=key), self.assertRaises(RuntimeError):
                PROBE.decode(json.dumps(value).encode(), 'self')

    def test_duplicate_unknown_and_oversized_reports_never_become_complete(self):
        raw = json.dumps(refusal()).encode()
        for malformed in [b'{"schema":1,' + raw[1:], raw[:-1] + b',"unexpected":true}', b' ' * 4097]:
            with self.assertRaises((RuntimeError, ValueError)):
                PROBE.decode(malformed, 'self')

    def test_unicode_quoted_binary_path_is_an_argument_not_shell_source(self):
        binary = Path(tempfile.gettempdir()).resolve() / "owned ' \u03c0" / 'probe'
        linux = PROBE.plan(binary, 'linux')
        mac = PROBE.plan(binary, 'darwin')
        self.assertEqual(len(linux), 2)
        self.assertEqual(len(mac), 3)
        self.assertNotIn(str(binary), linux[-1]['argv'][2])
        self.assertEqual(linux[-1]['argv'][-2:], [str(binary), 'inherited'])
        self.assertIn('exec "$@"', linux[-1]['argv'][2])
        self.assertFalse(linux[-1]['nanoDisabled'])
        self.assertTrue(mac[-1]['nanoDisabled'])

    def test_non_ci_and_windows_refuse_before_git_or_filesystem_mutation(self):
        for platform, ci in [('linux', 'false'), ('win32', 'true')]:
            with mock.patch.object(PROBE.sys, 'platform', platform), \
                 mock.patch.dict(PROBE.os.environ, {'GITHUB_ACTIONS': ci}), \
                 mock.patch.object(PROBE.subprocess, 'check_output', side_effect=AssertionError('Must not run git')), \
                 mock.patch.object(Path, 'mkdir', side_effect=AssertionError('Must not create output')):
                with self.assertRaisesRegex(RuntimeError, 'Unix CI'):
                    PROBE.main()

    def test_stale_checkout_refuses_before_probe_or_output(self):
        with mock.patch.object(PROBE.sys, 'platform', 'linux'), \
             mock.patch.dict(PROBE.os.environ, {'GITHUB_ACTIONS': 'true', 'GITHUB_SHA': 'a' * 40}), \
             mock.patch.object(PROBE.subprocess, 'check_output', return_value=('b' * 40).encode()), \
             mock.patch.object(Path, 'mkdir', side_effect=AssertionError('Must not create output')):
            with self.assertRaisesRegex(RuntimeError, 'Exact CI checkout'):
                PROBE.main()

    def test_capture_low_disk_refuses_before_spawning(self):
        with tempfile.TemporaryDirectory(prefix='yoke-owned-memory-control-') as temporary:
            root = Path(temporary)
            with mock.patch.object(PROBE.shutil, 'disk_usage', return_value=mock.Mock(free=PROBE.MIN_FREE - 1)), \
                 mock.patch.object(PROBE.subprocess, 'Popen', side_effect=AssertionError('Must not spawn')):
                with self.assertRaisesRegex(RuntimeError, 'disk reserve'):
                    PROBE.capture(['never-executed'], root, {}, root, 'probe', 1)

    def test_capture_retained_output_has_a_bounded_read(self):
        with tempfile.TemporaryDirectory(prefix='yoke-owned-memory-control-') as temporary:
            root = Path(temporary)
            process = mock.Mock(returncode=0)
            process.poll.return_value = 0

            def fake_spawn(*_args, **kwargs):
                kwargs['stdout'].write(b'x' * (PROBE.MAX_LOG + 1))
                kwargs['stdout'].flush()
                return process

            with mock.patch.object(PROBE.shutil, 'disk_usage', return_value=mock.Mock(free=PROBE.MIN_FREE * 2)), \
                 mock.patch.object(PROBE.subprocess, 'Popen', side_effect=fake_spawn), \
                 mock.patch.object(PROBE, 'retire') as retire:
                with self.assertRaisesRegex(RuntimeError, 'retained-log capacity'):
                    PROBE.capture(['never-executed'], root, {}, root, 'probe', 1)
                retire.assert_called_once_with(process)

    def test_process_group_absence_and_root_wait_are_both_required(self):
        process = mock.Mock(pid=12345)
        with mock.patch.object(PROBE.os, 'killpg', create=True, side_effect=ProcessLookupError), \
             mock.patch.object(PROBE.signal, 'SIGKILL', 9, create=True), \
             mock.patch.object(PROBE.time, 'monotonic', side_effect=[10, 10.5]):
            PROBE.retire(process)
        process.wait.assert_called_once()
        self.assertGreater(process.wait.call_args.kwargs['timeout'], 0)
        self.assertLessEqual(process.wait.call_args.kwargs['timeout'], 2)

    def test_unproven_group_cleanup_refuses_instead_of_reporting_observations(self):
        process = mock.Mock(pid=12345)
        with mock.patch.object(PROBE.os, 'killpg', create=True), \
             mock.patch.object(PROBE.signal, 'SIGKILL', 9, create=True), \
             mock.patch.object(PROBE.time, 'monotonic', side_effect=[10, 10.5, 13]):
            with self.assertRaisesRegex(RuntimeError, 'cleanup unproven'):
                PROBE.retire(process)

    def test_group_signal_refusal_still_attempts_root_cleanup_without_claiming_success(self):
        process = mock.Mock(pid=12345)
        with mock.patch.object(PROBE.os, 'killpg', create=True, side_effect=PermissionError), \
             mock.patch.object(PROBE.signal, 'SIGKILL', 9, create=True), \
             mock.patch.object(PROBE.time, 'monotonic', side_effect=[10, 10.5]):
            with self.assertRaisesRegex(RuntimeError, 'termination refused'):
                PROBE.retire(process)
        process.kill.assert_called_once()
        process.wait.assert_called_once()

    def test_linked_or_oversized_evidence_refuses_before_payload_acceptance(self):
        with tempfile.TemporaryDirectory(prefix='yoke-owned-memory-control-') as temporary:
            path = Path(temporary) / 'data.json'
            path.write_bytes(b'{}')
            with mock.patch.object(Path, 'is_symlink', return_value=True):
                with self.assertRaisesRegex(RuntimeError, 'Ordinary diagnostic'):
                    PROBE.bounded(path, 2)
            path.write_bytes(b'{}x')
            with self.assertRaisesRegex(RuntimeError, 'capacity exceeded'):
                PROBE.bounded(path, 2)


if __name__ == '__main__':
    unittest.main()
