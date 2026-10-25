use super::*;
use std::path::PathBuf;

fn python() -> PathBuf {
    let names: &[&str] = if cfg!(windows) {
        &["python.exe", "python3.exe"]
    } else {
        &["python3", "python"]
    };
    let paths = std::env::var_os("PATH").expect("Actual host Python requires PATH");
    for directory in std::env::split_paths(&paths).filter(|path| path.is_absolute()) {
        for name in names {
            let path = directory.join(name);
            if path.is_file() {
                return path.canonicalize().unwrap();
            }
        }
    }
    panic!("Actual host Python required for owned process cleanup tests");
}

fn command(script: &str, directory: &std::path::Path) -> Command {
    let mut command = Command::new(python());
    command
        .args(["-I", "-B", "-c", script])
        .current_dir(directory)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}

fn owned(script: &str, directory: &std::path::Path) -> ManagedChild {
    ManagedChild::spawn(command(script, directory)).unwrap()
}

fn observe_exit(child: &mut ManagedChild) -> std::process::ExitStatus {
    let deadline = crate::capture::Deadline::new(30_000);
    loop {
        if let Some(status) = child.inner_mut().try_wait().unwrap() {
            return status;
        }
        deadline.check().unwrap();
        thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn naturally_exited_owned_root_keeps_its_actual_status_during_checked_cleanup() {
    let directory = tempfile::tempdir().unwrap();
    let mut child = owned("raise SystemExit(17)", directory.path());
    assert_eq!(observe_exit(&mut child).code(), Some(17));
    assert_eq!(
        child
            .terminate_checked(Duration::from_secs(2))
            .unwrap()
            .code(),
        Some(17)
    );
}

#[test]
fn checked_cleanup_terminates_a_live_owned_root_and_returns_an_exit_receipt() {
    let directory = tempfile::tempdir().unwrap();
    let child = owned("import time; time.sleep(30)", directory.path());
    assert!(
        !child
            .terminate_checked(Duration::from_secs(2))
            .unwrap()
            .success()
    );
}

#[test]
fn invalid_cleanup_budgets_never_become_successful_exit_receipts() {
    for budget in [Duration::ZERO, Duration::from_secs(3)] {
        let directory = tempfile::tempdir().unwrap();
        let mut child = owned("raise SystemExit(0)", directory.path());
        assert!(observe_exit(&mut child).success());
        let error = child.terminate_checked(budget).unwrap_err();
        assert!(cleanup_unproven(&error));
    }
}

#[cfg(unix)]
#[derive(Debug)]
struct SignalFailureOnce {
    inner: Box<dyn StdChildWrapper>,
    error: Option<i32>,
    #[cfg(target_os = "macos")]
    exit_before_signal: Option<mpsc::Sender<DarwinRaceObservation>>,
}

#[cfg(target_os = "macos")]
#[derive(Debug)]
struct DarwinRaceObservation {
    code: i32,
    status: i32,
    signal: std::result::Result<(), Option<i32>>,
}

#[cfg(unix)]
impl StdChildWrapper for SignalFailureOnce {
    fn inner(&self) -> &std::process::Child {
        self.inner.inner()
    }
    fn inner_mut(&mut self) -> &mut std::process::Child {
        self.inner.inner_mut()
    }
    fn into_inner(self: Box<Self>) -> std::process::Child {
        self.inner.into_inner()
    }
    fn start_kill(&mut self) -> std::io::Result<()> {
        #[cfg(target_os = "macos")]
        if let Some(observed) = self.exit_before_signal.take() {
            let mut release = self
                .inner
                .stdin()
                .take()
                .ok_or_else(|| std::io::Error::other("Owned race release pipe unavailable"))?;
            release.write_all(b"x")?;
            drop(release);
            // This wait runs inside cleanup's original absolute deadline.
            // The race test also measures the entire two-second cleanup call.
            let info = try_observe_unreaped(self.inner.inner().id(), Duration::from_secs(2))?;
            // Exercise the real Darwin signal error after cleanup's initial
            // raw try_wait observed a live root. WNOWAIT has not reaped it.
            let result = self.inner.start_kill();
            let _ = observed.send(DarwinRaceObservation {
                code: info.si_code,
                status: info.si_status,
                signal: result
                    .as_ref()
                    .map(|()| ())
                    .map_err(|error| error.raw_os_error()),
            });
            return result;
        }
        if let Some(error) = self.error.take() {
            Err(std::io::Error::from_raw_os_error(error))
        } else {
            self.inner.start_kill()
        }
    }
}

#[cfg(unix)]
fn injected_child(directory: &std::path::Path, error: i32) -> ManagedChild {
    let mut wrapper = StdCommandWrap::from(command("import time; time.sleep(30)", directory));
    wrapper.wrap(process_wrap::std::ProcessGroup::leader());
    let inner = wrapper.spawn().unwrap();
    ManagedChild {
        owned_pgid: inner.inner().id(),
        inner: Box::new(SignalFailureOnce {
            inner,
            error: Some(error),
            #[cfg(target_os = "macos")]
            exit_before_signal: None,
        }),
        drop_wait: true,
    }
}

#[cfg(unix)]
#[test]
fn missing_group_without_owned_root_exit_is_not_cleanup_proof() {
    let directory = tempfile::tempdir().unwrap();
    let mut child = injected_child(directory.path(), libc::ESRCH);
    let result = child.cleanup_checked(Duration::from_millis(10));
    // Restore the guard before asserting the intentionally failed cleanup.
    // The injected signal error is one-shot; real cleanup still owns the root.
    child.drop_wait = true;
    assert!(cleanup_unproven(&result.unwrap_err()));
    assert!(child.inner_mut().try_wait().unwrap().is_none());
    child.terminate_checked(Duration::from_secs(2)).unwrap();
}

#[cfg(unix)]
#[test]
fn a_different_signal_failure_never_counts_as_an_absent_group() {
    let directory = tempfile::tempdir().unwrap();
    let mut child = injected_child(directory.path(), libc::EPERM);
    let result = child.cleanup_checked(Duration::from_secs(2));
    child.drop_wait = true;
    assert!(cleanup_unproven(&result.unwrap_err()));
    assert!(child.inner_mut().try_wait().unwrap().is_none());
    child.terminate_checked(Duration::from_secs(2)).unwrap();
}

#[cfg(target_os = "macos")]
fn observe_unreaped(pid: u32, budget: Duration) -> libc::siginfo_t {
    try_observe_unreaped(pid, budget).expect("Owned root exit observation failed")
}

#[cfg(target_os = "macos")]
fn try_observe_unreaped(pid: u32, budget: Duration) -> std::io::Result<libc::siginfo_t> {
    let end = std::time::Instant::now() + budget;
    loop {
        // Darwin leaves siginfo untouched for WNOHANG with no matching event.
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        let result = unsafe {
            libc::waitid(
                libc::P_PID,
                pid as libc::id_t,
                &mut info,
                libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
            )
        };
        if result == 0 && info.si_pid == pid as libc::pid_t {
            return Ok(info);
        }
        if result != 0 {
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() != Some(libc::EINTR) {
                return Err(error);
            }
        }
        let remaining = end.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "Owned root never became waitable",
            ));
        }
        thread::sleep(remaining.min(Duration::from_millis(5)));
    }
}

#[cfg(target_os = "macos")]
struct OwnedRootReapGuard {
    child: Option<ManagedChild>,
    teardown_end: Option<std::time::Instant>,
}

#[cfg(target_os = "macos")]
impl OwnedRootReapGuard {
    fn new(child: ManagedChild) -> Self {
        Self {
            child: Some(child),
            teardown_end: None,
        }
    }

    fn reap(&mut self) -> std::io::Result<std::process::ExitStatus> {
        // Test-only teardown has its own original 30-second fixture budget.
        // Retries during unwinding share it; ManagedChild adds no Drop wait.
        let end = *self
            .teardown_end
            .get_or_insert_with(|| std::time::Instant::now() + Duration::from_secs(30));
        let child = self
            .child
            .as_mut()
            .ok_or_else(|| std::io::Error::other("Owned root guard already released"))?;
        child.drop_wait = false;
        drop(child.inner.inner_mut().stdin.take());
        let raw = child.inner.inner_mut();
        if let Some(status) = raw.try_wait()? {
            return Ok(status);
        }
        if std::time::Instant::now() >= end {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "Owned root teardown budget exhausted",
            ));
        }
        // Only a still-unreaped owned root may be killed. A cached exit above
        // returns immediately; the numeric PID/PGID is never used for a late kill.
        let signal = raw.kill();
        loop {
            if let Some(status) = raw.try_wait()? {
                return Ok(status);
            }
            let remaining = end.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    format!("Owned root remained unreaped after teardown signal: {signal:?}"),
                ));
            }
            thread::sleep(remaining.min(Duration::from_millis(5)));
        }
    }
}

#[cfg(target_os = "macos")]
impl Drop for OwnedRootReapGuard {
    fn drop(&mut self) {
        if self.child.is_some()
            && let Err(error) = self.reap()
        {
            if thread::panicking() {
                eprintln!("Owned root teardown failed during test unwind: {error}");
            } else {
                panic!("Owned root teardown failed: {error}");
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn waiting_child(
    script: &str,
    directory: &std::path::Path,
    race: Option<mpsc::Sender<DarwinRaceObservation>>,
) -> ManagedChild {
    let mut host = command(script, directory);
    host.stdin(Stdio::piped()).stdout(Stdio::piped());
    let mut wrapper = StdCommandWrap::from(host);
    wrapper.wrap(process_wrap::std::ProcessGroup::leader());
    let inner = wrapper.spawn().unwrap();
    let guard_race = race.is_some();
    let mut child = ManagedChild {
        owned_pgid: inner.inner().id(),
        inner: Box::new(SignalFailureOnce {
            inner,
            error: None,
            exit_before_signal: race,
        }),
        drop_wait: true,
    };
    let ready = |child: &mut ManagedChild| {
        let mut stdout = child.stdout().take().unwrap();
        let (done, ready) = mpsc::sync_channel(1);
        thread::spawn(move || {
            let mut byte = [0];
            let _ = done.send(stdout.read_exact(&mut byte).map(|()| byte[0]));
        });
        assert_eq!(
            ready
                .recv_timeout(Duration::from_secs(30))
                .unwrap()
                .unwrap(),
            b'R'
        );
    };
    if guard_race {
        // A setup panic must not invoke the race hook from ManagedChild::drop
        // without retaining the raw root for fallible teardown.
        let mut guard = OwnedRootReapGuard::new(child);
        ready(guard.child.as_mut().unwrap());
        guard.child.take().unwrap()
    } else {
        ready(&mut child);
        child
    }
}

#[cfg(unix)]
fn await_group_absence(pgid: u32) {
    let deadline = crate::capture::Deadline::new(30_000);
    while !owned_group_absent(pgid).unwrap() {
        deadline.check().unwrap();
        thread::sleep(Duration::from_millis(5));
    }
}

#[cfg(unix)]
#[test]
fn complete_capture_cannot_publish_success_while_an_owned_descendant_survives() {
    use std::os::{
        fd::AsRawFd,
        unix::{net::UnixStream, process::CommandExt},
    };
    let directory = tempfile::tempdir().unwrap();
    let (mut release, inherited) = UnixStream::pair().unwrap();
    let descriptor = inherited.as_raw_fd();
    let mut host = command(
        &format!(
            r#"
import os, sys
if os.fork() == 0:
    for descriptor in (0, 1, 2):
        os.close(descriptor)
    try:
        os.read({descriptor}, 1)
    finally:
        os._exit(0)
with open('owned-pgid', 'w', encoding='ascii') as receipt:
    receipt.write(str(os.getpgrp()))
sys.stdout.buffer.write(b'owned-output')
sys.stdout.buffer.flush()
os._exit(0)
"#
        ),
        directory.path(),
    );
    // Make only this child's copy inheritable. Other concurrent test children
    // cannot inherit the release socket and delay fixture teardown.
    unsafe {
        host.pre_exec(move || {
            let flags = libc::fcntl(descriptor, libc::F_GETFD);
            if flags == -1
                || libc::fcntl(descriptor, libc::F_SETFD, flags & !libc::FD_CLOEXEC) == -1
            {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let began = std::time::Instant::now();
    let result = run_capture(
        host,
        b"",
        &crate::capture::Deadline::new(30_000),
        CaptureLimits {
            stdin_bytes: 0,
            stdout_bytes: 1024,
            stderr_bytes: 1024,
        },
    );
    let elapsed = began.elapsed();
    // Release the fixture before any assertion; closing the socket also releases
    // it during unwinding. The numeric PGID below is used only for observation.
    let released = release.write_all(b"x");
    drop(release);
    drop(inherited);
    let pgid: u32 = std::fs::read_to_string(directory.path().join("owned-pgid"))
        .unwrap()
        .parse()
        .unwrap();
    await_group_absence(pgid);
    assert!(cleanup_unproven(&result.unwrap_err()));
    assert!(released.is_ok());
    assert!(elapsed < Duration::from_secs(30), "{elapsed:?}");
}

#[cfg(target_os = "macos")]
#[test]
fn exited_but_unreaped_owned_root_preserves_actual_status_and_proves_group_absence() {
    let directory = tempfile::tempdir().unwrap();
    let child = owned("raise SystemExit(17)", directory.path());
    let pgid = child.owned_pgid;
    for _ in 0..2 {
        let info = observe_unreaped(child.inner.inner().id(), Duration::from_secs(30));
        assert_eq!(info.si_code, libc::CLD_EXITED);
        assert_eq!(info.si_status, 17);
    }
    // Only cleanup may reap this fixture; both observations used WNOWAIT.
    assert_eq!(
        child
            .terminate_checked(Duration::from_secs(2))
            .unwrap()
            .code(),
        Some(17)
    );
    assert!(owned_group_absent(pgid).unwrap());
}

#[cfg(target_os = "macos")]
#[test]
fn darwin_exit_between_initial_poll_and_group_signal_requires_both_receipts() {
    let directory = tempfile::tempdir().unwrap();
    let (observed, observations) = mpsc::channel();
    let mut guard = OwnedRootReapGuard::new(waiting_child(
        "import sys; sys.stdout.buffer.write(b'R'); sys.stdout.buffer.flush(); sys.stdin.buffer.read(1); raise SystemExit(17)",
        directory.path(),
        Some(observed),
    ));
    let child = guard.child.as_mut().unwrap();
    let pgid = child.owned_pgid;
    // The readiness budget is setup only. The injected exit and WNOWAIT
    // observation above must fit inside the original cleanup call's budget.
    let began = std::time::Instant::now();
    let result = child.cleanup_checked(Duration::from_secs(2));
    let elapsed = began.elapsed();
    let group_absent = owned_group_absent(pgid);
    // Retain and reap the raw root before inspecting any hook or cleanup result,
    // including a failed observation or an unexpectedly successful signal.
    let reaped = guard.reap();
    let observation = observations.try_recv();
    assert_eq!(reaped.unwrap().code(), Some(17));
    let observation = observation.expect("Darwin race hook did not record a signal observation");
    assert_eq!(observation.code, libc::CLD_EXITED);
    assert_eq!(observation.status, 17);
    assert_eq!(observation.signal, Err(Some(libc::EPERM)));
    assert_eq!(result.unwrap().code(), Some(17));
    assert!(elapsed < Duration::from_secs(2), "{elapsed:?}");
    assert!(group_absent.unwrap());
}

#[cfg(target_os = "macos")]
#[test]
fn live_owned_root_and_its_descendant_are_killed_before_cleanup_succeeds() {
    use std::os::unix::process::ExitStatusExt;
    let directory = tempfile::tempdir().unwrap();
    let child = waiting_child(
        r#"
import os, sys, time
if os.fork() == 0:
    sys.stdout.buffer.write(b'R')
    sys.stdout.buffer.flush()
    sys.stdin.buffer.read(1)
    os._exit(0)
time.sleep(30)
"#,
        directory.path(),
        None,
    );
    let pgid = child.owned_pgid;
    assert!(!owned_group_absent(pgid).unwrap());
    let status = child.terminate_checked(Duration::from_secs(2)).unwrap();
    assert_eq!(status.signal(), Some(libc::SIGKILL));
    assert!(owned_group_absent(pgid).unwrap());
}

#[cfg(target_os = "macos")]
#[test]
fn reaped_root_with_a_surviving_descendant_refuses_without_an_extra_drop_wait() {
    let directory = tempfile::tempdir().unwrap();
    let mut child = waiting_child(
        r#"
import os, sys
if os.fork() == 0:
    sys.stdout.buffer.write(b'R')
    sys.stdout.buffer.flush()
    sys.stdin.buffer.read(1)
    os._exit(0)
os._exit(17)
"#,
        directory.path(),
        None,
    );
    let pgid = child.owned_pgid;
    let mut release = child.stdin().take().unwrap();
    let info = observe_unreaped(child.inner.inner().id(), Duration::from_secs(30));
    assert_eq!(info.si_code, libc::CLD_EXITED);
    assert_eq!(info.si_status, 17);
    assert!(!owned_group_absent(pgid).unwrap());
    let began = std::time::Instant::now();
    let result = child.terminate_checked(Duration::from_millis(20));
    let elapsed = began.elapsed();
    // Release and observe fixture teardown before asserting the refusal.
    // Closing this owned pipe also releases the descendant during unwinding.
    let released = release.write_all(b"x");
    drop(release);
    await_group_absence(pgid);
    assert!(cleanup_unproven(&result.unwrap_err()));
    assert!(released.is_ok());
    assert!(elapsed < Duration::from_secs(2), "{elapsed:?}");
}

#[cfg(target_os = "macos")]
#[test]
fn independently_absent_group_without_a_raw_child_receipt_is_unproven() {
    let directory = tempfile::tempdir().unwrap();
    let child = owned("raise SystemExit(17)", directory.path());
    let pid = child.inner.inner().id();
    observe_unreaped(pid, Duration::from_secs(30));
    let mut discarded = 0;
    // Deliberately lose the raw Child's receipt, unlike the natural-exit test.
    assert_eq!(
        unsafe { libc::waitpid(pid as libc::pid_t, &mut discarded, libc::WNOHANG) },
        pid as libc::pid_t
    );
    assert!(owned_group_absent(child.owned_pgid).unwrap());
    let error = child.terminate_checked(Duration::from_secs(2)).unwrap_err();
    assert!(cleanup_unproven(&error));
}

#[cfg(unix)]
#[test]
fn invalid_group_observation_never_turns_a_cached_exit_into_cleanup_proof() {
    for pgid in [0, 1, u32::MAX] {
        let directory = tempfile::tempdir().unwrap();
        let mut child = owned("raise SystemExit(0)", directory.path());
        assert!(observe_exit(&mut child).success());
        child.owned_pgid = pgid;
        let error = child.terminate_checked(Duration::from_secs(2)).unwrap_err();
        assert!(cleanup_unproven(&error));
    }
}
