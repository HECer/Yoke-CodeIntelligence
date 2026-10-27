use serde_json::json;
use std::{fs, path::PathBuf};
use yoke::{
    Engine,
    capture::Deadline,
    debug_adapter::{DebugAdapter, DebugBreakpoint, HostDebugProfile},
};
fn profile(log: &std::path::Path, mode: &str) -> HostDebugProfile {
    HostDebugProfile {
        key: "owned".into(),
        adapter_id: "independent-fixture".into(),
        command: std::env::split_paths(&std::env::var_os("PATH").unwrap())
            .filter(|directory| directory.is_absolute())
            .map(|directory| directory.join(if cfg!(windows) { "node.exe" } else { "node" }))
            .find(|path| path.is_file())
            .expect("The owned DAP fixture requires host-installed Node")
            .canonicalize()
            .unwrap(),
        arguments: vec![
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/debug-adapter.mjs")
                .display()
                .to_string(),
            log.display().to_string(),
            mode.into(),
        ],
        trust_adapter: true,
        allow_target_execution: true,
        allow_evaluation: true,
        request: "launch".into(),
        configuration: json!({"program":"${capturedEntry}"}),
        entry: Some("main.py".into()),
        breakpoints: vec![DebugBreakpoint {
            path: "main.py".into(),
            line: 2,
        }],
    }
}
#[test]
fn persistent_dap_evaluates_only_stopped_source_bound_frames_and_closes_owned_target() {
    let root = tempfile::tempdir().unwrap();
    let log = tempfile::NamedTempFile::new().unwrap();
    fs::write(
        root.path().join("main.py"),
        "value = 40\nresult = value + 2\n",
    )
    .unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let deadline = Deadline::new(20000);
    let snapshot = engine.index(&deadline).unwrap().0;
    let mut adapter = DebugAdapter::start(
        &engine.store,
        &snapshot,
        &profile(log.path(), "healthy"),
        &deadline,
    )
    .unwrap();
    assert_eq!(adapter.info()["frame"]["path"], "main.py");
    assert_eq!(adapter.info()["snapshotId"], snapshot.id);
    assert_eq!(
        adapter.evaluate("value + 2", &deadline).unwrap()["result"],
        "42"
    );
    assert_eq!(
        adapter.evaluate("value", &deadline).unwrap()["result"],
        "40"
    );
    adapter.close(&deadline).unwrap();
    let events = fs::read_to_string(log.path()).unwrap();
    assert!(events.contains("disconnect"));
    assert_eq!(
        fs::read_to_string(root.path().join("main.py")).unwrap(),
        "value = 40\nresult = value + 2\n"
    );
}

#[test]
fn active_debugger_evaluation_observes_host_cancellation_before_its_original_deadline() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    use std::time::{Duration, Instant};
    let root = tempfile::tempdir().unwrap();
    let log = tempfile::NamedTempFile::new().unwrap();
    fs::write(
        root.path().join("main.py"),
        "value = 40\nresult = value + 2\n",
    )
    .unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let snapshot = engine.index(&Deadline::new(20000)).unwrap().0;
    let mut adapter = DebugAdapter::start(
        &engine.store,
        &snapshot,
        &profile(log.path(), "stalled-evaluate"),
        &Deadline::new(20000),
    )
    .unwrap();
    let cancelled = Arc::new(AtomicBool::new(false));
    let signal = Arc::clone(&cancelled);
    let log_path = log.path().to_owned();
    let watcher = std::thread::spawn(move || {
        let limit = Instant::now() + Duration::from_secs(5);
        loop {
            if fs::read_to_string(&log_path)
                .unwrap_or_default()
                .contains("\"command\":\"evaluate\"")
            {
                signal.store(true, Ordering::SeqCst);
                return;
            }
            assert!(Instant::now() < limit, "Fixture never received evaluation");
            std::thread::sleep(Duration::from_millis(5));
        }
    });
    let start = Instant::now();
    let result = Deadline::run_bounded(&Deadline::new(3000), cancelled, || {
        adapter.evaluate("value", &Deadline::new(600000))
    });
    watcher.join().unwrap();
    assert!(result.is_err());
    assert!(!adapter.info()["alive"].as_bool().unwrap());
    assert_eq!(adapter.info()["ownedProcessCleanupProven"], true);
    assert_eq!(adapter.info()["ownedIoWorkers"], 0);
    assert!(
        start.elapsed() < Duration::from_secs(1),
        "Cancelled evaluation waited for the original request deadline: {:?}",
        start.elapsed()
    );
    assert!(
        adapter.evaluate("value", &Deadline::new(1000)).is_err(),
        "An interrupted debugger must not silently restart"
    );
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn OpenProcess(access: u32, inherit: i32, id: u32) -> *mut std::ffi::c_void;
            fn WaitForSingleObject(handle: *mut std::ffi::c_void, milliseconds: u32) -> u32;
            fn CloseHandle(handle: *mut std::ffi::c_void) -> i32;
            fn GetLastError() -> u32;
        }
        let event: serde_json::Value = serde_json::from_str(
            fs::read_to_string(log.path())
                .unwrap()
                .lines()
                .next()
                .unwrap(),
        )
        .unwrap();
        let pid = u32::try_from(event["pid"].as_u64().unwrap()).unwrap();
        // SYNCHRONIZE permits an exit observation, never process modification.
        let handle = unsafe { OpenProcess(0x0010_0000, 0, pid) };
        if handle.is_null() {
            assert_eq!(
                unsafe { GetLastError() },
                87,
                "Could not verify owned adapter exit"
            );
        } else {
            let status = unsafe { WaitForSingleObject(handle, 0) };
            unsafe { CloseHandle(handle) };
            assert_eq!(
                status, 0,
                "Cancelled owned adapter process is still running"
            );
        }
    }
}
#[test]
fn runtime_and_evaluation_permissions_are_checked_before_adapter_or_target_start() {
    let root = tempfile::tempdir().unwrap();
    let log = tempfile::NamedTempFile::new().unwrap();
    fs::write(root.path().join("main.py"), "value = 40\n").unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let deadline = Deadline::new(20000);
    let snapshot = engine.index(&deadline).unwrap().0;
    for flag in 0..3 {
        let mut p = profile(log.path(), "healthy");
        match flag {
            0 => p.trust_adapter = false,
            1 => p.allow_target_execution = false,
            _ => p.allow_evaluation = false,
        };
        assert!(DebugAdapter::start(&engine.store, &snapshot, &p, &deadline).is_err());
    }
    assert!(fs::read_to_string(log.path()).unwrap().is_empty());
}
#[test]
fn reverse_launch_authority_and_foreign_stale_or_unstopped_frames_are_refused() {
    let root = tempfile::tempdir().unwrap();
    let log = tempfile::NamedTempFile::new().unwrap();
    fs::write(
        root.path().join("main.py"),
        "value = 40\nresult = value + 2\n",
    )
    .unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let deadline = Deadline::new(20000);
    let snapshot = engine.index(&deadline).unwrap().0;
    for mode in [
        "reverse-run",
        "foreign-frame",
        "changed-source",
        "not-stopped",
    ] {
        let error = DebugAdapter::start(
            &engine.store,
            &snapshot,
            &profile(log.path(), mode),
            &Deadline::new(2000),
        )
        .err()
        .expect("unsafe debugger mode accepted")
        .to_string();
        let expected = match mode {
            "reverse-run" => "reverse",
            "foreign-frame" => "outside",
            "changed-source" => "source",
            _ => "deadline",
        };
        assert!(
            error.to_ascii_lowercase().contains(expected),
            "wrong {mode} boundary: {error}"
        );
        assert!(
            fs::read_to_string(log.path())
                .unwrap()
                .contains("initialize")
        );
    }
}

#[test]
fn session_facades_use_only_host_profiles_preserve_namespaces_and_reject_changed_source() {
    use yoke::project_session::{HostProfile, Session};
    let root = tempfile::tempdir().unwrap();
    let log = tempfile::NamedTempFile::new().unwrap();
    fs::write(
        root.path().join("main.py"),
        "value = 40\nresult = value + 2\n",
    )
    .unwrap();
    let mut host = json!({"debugProfiles":[profile(log.path(),"healthy")]});
    host["debugProfiles"][0]["configuration"]["env"] = json!({"TOKEN":"private-test-token"});
    let host: HostProfile = serde_json::from_value(host).unwrap();
    let deadline = Deadline::new(20000);
    let mut session = Session::open(
        Engine::open(root.path()).unwrap(),
        host,
        Some("debug-facade"),
        &deadline,
    )
    .unwrap();
    let info = yoke::agent_repl::invoke_facade(
        &mut session,
        "jb.debug_eval_info",
        json!({}),
        None,
        &deadline,
    )
    .unwrap();
    assert!(!info.to_string().contains("private-test-token"));
    assert!(fs::read_to_string(log.path()).unwrap().is_empty());
    let first = yoke::agent_repl::invoke_facade(
        &mut session,
        "jb.debug_eval",
        json!({"expression":"value + 2","repl_key":"owned"}),
        None,
        &deadline,
    )
    .unwrap();
    assert_eq!(first["result"], "42");
    assert_eq!(
        yoke::agent_repl::invoke_facade(
            &mut session,
            "jb.debug_eval",
            json!({"expression":"value","repl_key":"owned"}),
            None,
            &deadline
        )
        .unwrap()["result"],
        "40"
    );
    fs::write(
        root.path().join("main.py"),
        "value = 99\nresult = value + 2\n",
    )
    .unwrap();
    assert!(
        yoke::agent_repl::invoke_facade(
            &mut session,
            "jb.debug_eval",
            json!({"expression":"value","repl_key":"owned"}),
            None,
            &deadline
        )
        .is_err()
    );
    let events = fs::read_to_string(log.path()).unwrap();
    assert_eq!(
        events
            .lines()
            .filter(|line| line.contains("evaluate"))
            .count(),
        2
    );
    assert!(
        yoke::agent_repl::invoke_facade(
            &mut session,
            "jb.debug_eval",
            json!({"expression":"value","repl_key":"missing"}),
            None,
            &deadline
        )
        .is_err()
    );
}

#[test]
#[ignore = "requires host-selected absolute Python and actual isolated debugpy adapter paths"]
fn actual_windows_debugpy_persistent_evaluation_has_exact_captured_source() {
    let python = std::env::var("YOKE_DEBUG_PYTHON").expect("Set actual host Python path");
    let adapter_path =
        std::env::var("YOKE_DEBUG_ADAPTER").expect("Set actual debugpy adapter directory");
    assert!(PathBuf::from(&python).is_absolute() && PathBuf::from(&adapter_path).is_absolute());
    let root = tempfile::tempdir().unwrap();
    let source = "value = 40\nresult = value + 2\nprint(result)\n";
    fs::write(root.path().join("main.py"), source).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let deadline = Deadline::new(90000);
    let snapshot = engine.index(&deadline).unwrap().0;
    let profile = HostDebugProfile {
        key: "actual-python".into(),
        adapter_id: "debugpy".into(),
        command: PathBuf::from(&python),
        arguments: vec![adapter_path],
        trust_adapter: true,
        allow_target_execution: true,
        allow_evaluation: true,
        request: "launch".into(),
        configuration: json!({"program":"${capturedEntry}","cwd":"${capturedRoot}","python":[python],"console":"internalConsole","justMyCode":true,"subProcess":false}),
        entry: Some("main.py".into()),
        breakpoints: vec![DebugBreakpoint {
            path: "main.py".into(),
            line: 2,
        }],
    };
    let mut adapter = DebugAdapter::start(&engine.store, &snapshot, &profile, &deadline).unwrap();
    assert_eq!(
        adapter.info()["frame"]["contentHash"],
        yoke::model::hash(source)
    );
    assert_eq!(
        adapter.evaluate("value + 2", &deadline).unwrap()["result"],
        "42"
    );
    assert_eq!(
        adapter.evaluate("value", &deadline).unwrap()["result"],
        "40"
    );
    adapter.close(&deadline).unwrap();
    assert_eq!(
        fs::read_to_string(root.path().join("main.py")).unwrap(),
        source
    );
}

#[test]
fn dap_accepts_no_configuration_phase_and_discovers_optional_stopped_thread_ids() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("main.py"),
        "value = 40\nresult = value + 2\n",
    )
    .unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let snapshot = engine.index(&Deadline::new(20000)).unwrap().0;
    for mode in ["no-configuration-phase", "no-thread-id"] {
        let log = tempfile::NamedTempFile::new().unwrap();
        let mut p = profile(log.path(), mode);
        if mode == "no-configuration-phase" {
            p.breakpoints.clear();
        }
        let deadline = Deadline::new(3000);
        let started = std::time::Instant::now();
        let mut adapter = DebugAdapter::start(&engine.store, &snapshot, &p, &deadline)
            .unwrap_or_else(|error| {
                use std::io::Read;
                // Sample failure return (including checked startup cleanup)
                // before diagnostic I/O; this is not protocol-only timing.
                let elapsed = started.elapsed();
                let remaining = deadline.remaining();
                // This owned fixture records only request type, command, sequence
                // and its PID. Read it only after failure, with a fixed byte cap.
                let mut bytes = Vec::new();
                let log_read = fs::File::open(log.path())
                    .and_then(|file| file.take(4097).read_to_end(&mut bytes));
                let truncated = bytes.len() > 4096;
                bytes.truncate(4096);
                let commands = std::str::from_utf8(&bytes);
                panic!(
                    "optional {mode} rejected: {error:#}; elapsed={elapsed:?}; remaining={remaining:?}; fixture_read={log_read:?}; fixture_truncated={truncated}; fixture_commands={commands:?}"
                )
            });
        assert_eq!(
            adapter.evaluate("value + 2", &deadline).unwrap()["result"],
            "42"
        );
        adapter.close(&deadline).unwrap();
    }
}
#[test]
fn invalid_evaluation_input_is_rejected_before_launching_an_authorized_target() {
    use yoke::project_session::{HostProfile, Session};
    let root = tempfile::tempdir().unwrap();
    let log = tempfile::NamedTempFile::new().unwrap();
    fs::write(
        root.path().join("main.py"),
        "value = 40\nresult = value + 2\n",
    )
    .unwrap();
    let host: HostProfile =
        serde_json::from_value(json!({"debugProfiles":[profile(log.path(),"healthy")]})).unwrap();
    let deadline = Deadline::new(20000);
    let mut session = Session::open(
        Engine::open(root.path()).unwrap(),
        host,
        Some("input-admission"),
        &deadline,
    )
    .unwrap();
    assert!(
        yoke::agent_repl::invoke_facade(
            &mut session,
            "jb.debug_eval",
            json!({"expression":"","repl_key":"owned"}),
            None,
            &deadline
        )
        .is_err()
    );
    assert!(
        fs::read_to_string(log.path()).unwrap().is_empty(),
        "invalid evaluation launched a target before input admission"
    );
}
