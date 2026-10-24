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
struct SignalFailureOnce {
    inner: Box<dyn StdChildWrapper>,
    error: Option<i32>,
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
    ManagedChild {
        inner: Box::new(SignalFailureOnce {
            inner: wrapper.spawn().unwrap(),
            error: Some(error),
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
