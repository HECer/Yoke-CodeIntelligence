#![cfg(windows)]
use std::{
    fs,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use yoke::{
    Engine,
    agent_repl::{AgentRepl, ReplSpec},
    capture::Deadline,
    project_session::{HostProfile, Session},
};

fn python() -> PathBuf {
    let output = std::process::Command::new("python")
        .args(["-I", "-c", "import sys;print(sys.executable)"])
        .output()
        .unwrap();
    assert!(output.status.success());
    PathBuf::from(String::from_utf8(output.stdout).unwrap().trim())
}
fn spec() -> ReplSpec {
    ReplSpec {
        python: python(),
        trust_python: true,
        max_output_bytes: 100_000,
        shell: None,
    }
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn OpenProcess(access: u32, inherit: i32, id: u32) -> *mut std::ffi::c_void;
    fn WaitForSingleObject(handle: *mut std::ffi::c_void, milliseconds: u32) -> u32;
    fn CloseHandle(handle: *mut std::ffi::c_void) -> i32;
}
fn exited(pid: u32) -> bool {
    unsafe {
        let handle = OpenProcess(0x0010_0000, 0, pid);
        if handle.is_null() {
            return true;
        }
        let exited = WaitForSingleObject(handle, 0) == 0;
        CloseHandle(handle);
        exited
    }
}

#[test]
fn cancelled_python_cell_stops_owned_process_before_original_request_timeout() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("main.rs"), "fn target(){}\n").unwrap();
    let setup = Deadline::new(20_000);
    let mut session = Session::open(
        Engine::open(root.path()).unwrap(),
        HostProfile::default(),
        Some("lifecycle"),
        &setup,
    )
    .unwrap();
    let mut repl = AgentRepl::spawn(spec(), &setup).unwrap();
    let marker = root.path().join("worker.pid");
    let cancel = Arc::new(AtomicBool::new(false));
    let cancelled = Arc::clone(&cancel);
    let observed = marker.clone();
    let observer = std::thread::spawn(move || {
        let end = Instant::now() + Duration::from_secs(3);
        while fs::read_to_string(&observed)
            .ok()
            .and_then(|text| text.parse::<u32>().ok())
            .is_none()
        {
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(10));
        }
        cancelled.store(true, Ordering::SeqCst);
    });
    let code = format!(
        "import os,time\nwith open({},'w') as output:\n output.write(str(os.getpid()))\ntime.sleep(30)",
        serde_json::to_string(&marker.to_string_lossy()).unwrap()
    );
    let started = Instant::now();
    let result = Deadline::run_bounded(&Deadline::new(5000), cancel, || {
        repl.execute(&mut session, &code, &Deadline::new(600_000))
    });
    observer.join().unwrap();
    assert!(result.is_err());
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "Cancellation waited {:?} for original deadline",
        started.elapsed()
    );
    let pid = fs::read_to_string(marker).unwrap().parse().unwrap();
    assert!(exited(pid), "Cancelled owned Python process is still alive");
}

#[test]
fn checked_owned_termination_proves_windows_child_and_descendant_exit() {
    let root = tempfile::tempdir().unwrap();
    let marker = root.path().join("descendant.pid");
    let code = format!(
        "import subprocess,sys,time\np=subprocess.Popen([sys.executable,'-I','-c','import time;time.sleep(30)'])\nwith open({},'w') as output:\n output.write(str(p.pid))\ntime.sleep(30)",
        serde_json::to_string(&marker.to_string_lossy()).unwrap()
    );
    let mut command = std::process::Command::new(python());
    command.args(["-I", "-c", &code]);
    let child = yoke::process::ManagedChild::spawn(command).unwrap();
    let parent_pid = child.id();
    let end = Instant::now() + Duration::from_secs(3);
    while fs::read_to_string(&marker)
        .ok()
        .and_then(|text| text.parse::<u32>().ok())
        .is_none()
    {
        assert!(Instant::now() < end);
        std::thread::sleep(Duration::from_millis(10));
    }
    let descendant_pid = fs::read_to_string(marker).unwrap().parse().unwrap();
    assert!(!exited(parent_pid));
    assert!(!exited(descendant_pid));
    let started = Instant::now();
    let status = child.terminate_checked(Duration::from_millis(500)).unwrap();
    assert!(!status.success());
    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(exited(parent_pid));
    let end = Instant::now() + Duration::from_secs(1);
    while !exited(descendant_pid) {
        assert!(
            Instant::now() < end,
            "Owned Windows descendant survived job termination"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn connection_state_cannot_attach_or_detach_runtime_after_profile_binding() {
    let root = tempfile::tempdir().unwrap();
    let d = Deadline::new(20_000);
    let mut session = Session::open(
        Engine::open(root.path()).unwrap(),
        HostProfile::default(),
        Some("binding"),
        &d,
    )
    .unwrap();
    let mut runtime = AgentRepl::spawn(spec(), &d).unwrap();
    let init=serde_json::to_vec(&serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25"}})).unwrap();
    let ping = br#"{"jsonrpc":"2.0","id":2,"method":"ping"}"#;
    let mut state = yoke::protocol::ConnectionState::default();
    state
        .message_serena_with_repl(&mut session, &mut runtime, &init)
        .unwrap();
    assert!(
        state
            .message_session(&mut session, yoke::protocol::Profile::Serena, ping)
            .is_err()
    );
    assert!(
        state
            .message_session(&mut session, yoke::protocol::Profile::Native, ping)
            .is_err()
    );
    let mut plain = yoke::protocol::ConnectionState::default();
    plain
        .message_session(&mut session, yoke::protocol::Profile::Serena, &init)
        .unwrap();
    assert!(
        plain
            .message_serena_with_repl(&mut session, &mut runtime, ping)
            .is_err()
    );
    runtime.close().unwrap();
    assert!(!runtime.is_alive());
}
