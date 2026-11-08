//! A completed root must not leave a null-stdio descendant running.
use super::*;

#[derive(Clone, Copy)]
enum Completion {
    Capture,
    Terminate,
    Drop,
}

fn assert_group_exit(completion: Completion) {
    let folder = tempfile::tempdir().unwrap();
    let host = HostGit::discover().unwrap();
    let executable = blocking_git_host(folder.path(), &host);
    std::fs::write(folder.path().join("mode.txt"), "exit-parent").unwrap();
    let root = folder.path().to_path_buf();
    let (ready, wait_ready) = mpsc::channel();
    let (returned, wait_returned) = mpsc::channel();
    let observer = std::thread::spawn(move || {
        let end = Instant::now() + Duration::from_secs(4);
        let pids = loop {
            if let Ok(text) = std::fs::read_to_string(root.join("processes.ready")) {
                let pids: Vec<u32> = text.lines().filter_map(|s| s.parse().ok()).collect();
                if pids.len() == 2 {
                    break pids;
                }
            }
            assert!(
                Instant::now() < end,
                "Owned process pair never became ready"
            );
            std::thread::sleep(Duration::from_millis(2));
        };
        let handles: Vec<_> = pids
            .into_iter()
            .map(|pid| {
                let handle = ProcessHandle(unsafe { OpenProcess(0x0010_0000, 0, pid) });
                assert!(!handle.0.is_null());
                assert_eq!(unsafe { WaitForSingleObject(handle.0, 0) }, 258);
                (pid, handle)
            })
            .collect();
        ready.send(()).unwrap();
        if matches!(completion, Completion::Capture) {
            std::fs::write(root.join("release-root"), b"exit").unwrap();
        }
        wait_returned.recv_timeout(Duration::from_secs(5)).unwrap();
        for (pid, handle) in handles {
            assert_eq!(
                unsafe { WaitForSingleObject(handle.0, 0) },
                0,
                "Owned process {pid} survived a checked return"
            );
        }
    });
    let mut command = Command::new(executable);
    command.current_dir(folder.path());
    let started = Instant::now();
    if !matches!(completion, Completion::Capture) {
        let child = crate::process::ManagedChild::spawn(command).unwrap();
        wait_ready.recv_timeout(Duration::from_secs(4)).unwrap();
        match completion {
            Completion::Terminate => {
                child.terminate_checked(Duration::from_secs(2)).unwrap();
            }
            Completion::Drop => drop(child),
            Completion::Capture => unreachable!(),
        }
    } else {
        let output = crate::process::run_capture(
            command,
            &[],
            &Deadline::new(4000),
            crate::process::CaptureLimits {
                stdin_bytes: 0,
                stdout_bytes: 8192,
                stderr_bytes: 8192,
            },
        )
        .unwrap();
        wait_ready.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(output.exit_code, Some(0));
        assert!(output.output_complete && !output.timed_out);
    }
    returned.send(()).unwrap();
    observer.join().unwrap();
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[test]
fn successful_capture_reaps_the_owned_null_stdio_descendant() {
    assert_group_exit(Completion::Capture);
}

#[test]
fn explicit_termination_proves_the_owned_descendant_has_exited() {
    assert_group_exit(Completion::Terminate);
}

#[test]
fn dropping_the_owned_child_reaps_its_descendant_within_the_cleanup_budget() {
    assert_group_exit(Completion::Drop);
}
