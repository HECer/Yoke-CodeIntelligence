//! Exercise the real HostGit process boundary inside the digest transaction.
use super::*;
use std::{
    ffi::c_void,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

#[path = "github_app_digest_history_group_tests.rs"]
mod history_group_tests;

#[link(name = "kernel32")]
unsafe extern "system" {
    fn OpenProcess(access: u32, inherit: i32, id: u32) -> *mut c_void;
    fn WaitForSingleObject(handle: *mut c_void, milliseconds: u32) -> u32;
    fn CloseHandle(handle: *mut c_void) -> i32;
}
struct ProcessHandle(*mut c_void);
impl Drop for ProcessHandle {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

fn blocking_git_host(folder: &Path, host: &HostGit) -> PathBuf {
    // This owned host helper starts actual Git with a deliberately open stdin.
    // Both processes must be killed by the production Job Object, not by a test
    // cleanup action. No repository executable or hook is involved.
    let source = folder.join("history-host.rs");
    let executable = folder.join("history-host.exe");
    std::fs::write(
        &source,
        r#"
use std::{fs, process::{Command, Stdio}, os::windows::process::CommandExt};
fn main() {
    if std::env::args().nth(1).as_deref() == Some("--owned-waiter") {
        std::thread::sleep(std::time::Duration::from_secs(60));
        return;
    }
    let dir = std::env::current_exe().unwrap().parent().unwrap().to_path_buf();
    if fs::read_to_string(dir.join("mode.txt")).unwrap_or_default() == "exit-parent" {
        let child = Command::new(std::env::current_exe().unwrap()).arg("--owned-waiter")
            .creation_flags(0x08000000).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap();
        fs::write(dir.join("processes.ready"), format!("{}\n{}\n", std::process::id(), child.id())).unwrap();
        while !dir.join("release-root").exists() {
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        return;
    }
    let git = fs::read_to_string(dir.join("git.txt")).unwrap();
    let mut child = Command::new(git).args(["--no-replace-objects", "--no-lazy-fetch", "cat-file", "--batch-check"])
        .creation_flags(0x08000000).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap();
    let _pending_input = child.stdin.take().unwrap();
    fs::write(dir.join("processes.ready"), format!("{}\n{}\n", std::process::id(), child.id())).unwrap();
    child.wait().unwrap();
}
"#,
    )
    .unwrap();
    std::fs::write(folder.join("git.txt"), host.executable.to_str().unwrap()).unwrap();
    std::fs::write(
        folder.join("Cargo.toml"),
        "[package]\nname = \"owned-history-host\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[[bin]]\nname = \"history-host\"\npath = \"history-host.rs\"\n[workspace]\n",
    ).unwrap();
    let mut compiler = Command::new("cargo");
    compiler
        .current_dir(folder)
        .args(["build", "--offline", "--manifest-path"])
        .arg(folder.join("Cargo.toml"))
        .arg("--target-dir")
        .arg(folder.join("helper-target"));
    #[cfg(target_env = "msvc")]
    {
        // A dependency-free helper cannot rely on the parent workspace's
        // compiler discovery. Preserve only the discovered build-tool paths;
        // the production Git execution environment remains unchanged.
        let linker = find_msvc_tools::find_tool(std::env::consts::ARCH, "link.exe")
            .expect("The Windows MSVC test build requires its host linker");
        for (name, value) in linker.env() {
            compiler.env(name, value);
        }
    }
    let output = crate::process::run_capture(
        compiler,
        &[],
        &Deadline::new(20_000),
        crate::process::CaptureLimits {
            stdin_bytes: 0,
            stdout_bytes: 8192,
            stderr_bytes: 8192,
        },
    )
    .unwrap();
    assert_eq!(
        output.exit_code,
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.output_complete);
    std::fs::copy(
        folder.join("helper-target/debug/history-host.exe"),
        &executable,
    )
    .unwrap();
    executable
}

#[test]
fn cancellation_and_deadline_stop_owned_git_and_restore_the_same_store() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("source");
    std::fs::create_dir(&root).unwrap();
    let host = HostGit::discover().unwrap();
    git(&host, &root, &["init", "-q"]);
    std::fs::write(root.join("api.rs"), "pub fn api() {}\n").unwrap();
    git(&host, &root, &["add", "--", "api.rs"]);
    git(&host, &root, &["commit", "-q", "-m", "owned fixture"]);
    let head = git(&host, &root, &["rev-parse", "HEAD"]);
    let mut engine = Engine::open(&root).unwrap();
    let setup = Deadline::new(30_000);
    let origin = engine.index(&setup).unwrap().0;
    let snapshot = git_workflows::capture_commit_at(
        crate::analysis::AnalyzerEpoch::V8,
        &mut engine,
        &host,
        &origin,
        &head,
        &setup,
    )
    .unwrap()
    .0;
    let profile = HostProfile::parse(br#"{"schemaVersion":1,"appId":123,"secretCredential":"history-key-must-not-be-read","grants":[{"installationId":42,"repositoryId":9,"repository":"acme/demo"}]}"#).unwrap();
    let request = PreviewRequest {
        schema_version: 1,
        snapshot_id: snapshot.id,
        installation_id: 42,
        repository_id: 9,
        repository: "acme/demo".into(),
    };
    let blocking = HostGit {
        executable: blocking_git_host(folder.path(), &host),
    };
    let options = Options::default();
    let history_options = git_workflows::HistoryOptions::default();
    let baseline = super::super::preview_history_with_profile(
        &engine.store,
        &profile,
        &options,
        &request,
        &host,
        &history_options,
        &setup,
    )
    .unwrap();
    let blobs = || {
        let mut result = std::collections::BTreeMap::new();
        for entry in std::fs::read_dir(engine.store.dir.join("blobs")).unwrap() {
            let entry = entry.unwrap();
            result.insert(entry.file_name(), std::fs::read(entry.path()).unwrap());
        }
        result
    };
    let original_blobs = blobs();
    let changes = engine.store.db.total_changes();
    let length = engine
        .store
        .db
        .limit(rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH)
        .unwrap();
    engine
        .store
        .db
        .busy_timeout(Duration::from_millis(731))
        .unwrap();
    engine.store.replace_deadline(Some(Deadline::new(60_000)));
    for cancel in [true, false] {
        let ready = folder.path().join("processes.ready");
        let flag = Arc::new(AtomicBool::new(false));
        let worker_flag = Arc::clone(&flag);
        let (returned, wait_returned) = mpsc::channel();
        let worker_ready = ready.clone();
        let observer = std::thread::spawn(move || {
            let end = Instant::now() + Duration::from_secs(5);
            let pids = loop {
                if let Ok(text) = std::fs::read_to_string(&worker_ready) {
                    let ids: Vec<u32> = text.lines().filter_map(|s| s.parse().ok()).collect();
                    if ids.len() == 2 {
                        break ids;
                    }
                }
                assert!(Instant::now() < end, "Owned Git never became ready");
                std::thread::sleep(Duration::from_millis(2));
            };
            let handles: Vec<_> = pids
                .into_iter()
                .map(|pid| {
                    let handle = ProcessHandle(unsafe { OpenProcess(0x0010_0000, 0, pid) });
                    assert!(!handle.0.is_null(), "Cannot observe the owned process");
                    assert_eq!(unsafe { WaitForSingleObject(handle.0, 0) }, 258);
                    (pid, handle)
                })
                .collect();
            let started = Instant::now();
            if cancel {
                worker_flag.store(true, Ordering::SeqCst);
            }
            let outcome = wait_returned.recv_timeout(Duration::from_secs(4)).unwrap();
            for (pid, handle) in handles {
                assert_eq!(
                    unsafe { WaitForSingleObject(handle.0, 0) },
                    0,
                    "An owned Git process survived the digest return: pid={pid}, cancel={cancel}, elapsedMs={}, outcome={outcome}",
                    started.elapsed().as_millis()
                );
            }
            if cancel {
                assert!(
                    started.elapsed() < Duration::from_millis(750),
                    "Cancellation waited for the original deadline"
                );
            }
        });
        let started = Instant::now();
        let result = Deadline::run_bounded(&Deadline::new(5000), Arc::clone(&flag), || {
            let result = super::super::preview_history_with_profile(
                &engine.store,
                &profile,
                &options,
                &request,
                &blocking,
                &history_options,
                &Deadline::new(2000),
            );
            let outcome = match &result {
                Ok(_) => "unexpected_success".to_owned(),
                Err(error) => format!(
                    "{error:#}; cleanupUnproven={}",
                    crate::process::cleanup_unproven(error)
                ),
            };
            returned.send(outcome).unwrap();
            observer.join().unwrap();
            flag.store(false, Ordering::SeqCst);
            result
        });
        let error = result.unwrap_err().to_string();
        assert!(
            error.contains(if cancel { "cancelled" } else { "deadline" }),
            "{error}"
        );
        assert!(started.elapsed() < Duration::from_secs(3));
        assert!(engine.store.db.is_autocommit());
        assert_eq!(engine.store.db.total_changes(), changes);
        assert_eq!(
            engine
                .store
                .db
                .limit(rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH)
                .unwrap(),
            length
        );
        let busy: u64 = engine
            .store
            .db
            .query_row("PRAGMA busy_timeout", [], |r| r.get(0))
            .unwrap();
        assert_eq!(busy, 731);
        let original = engine.store.replace_deadline(None).unwrap();
        assert!(original.remaining() > Duration::from_secs(10));
        engine.store.replace_deadline(Some(original));
        assert_eq!(blobs(), original_blobs);
        assert_eq!(
            super::super::preview_history_with_profile(
                &engine.store,
                &profile,
                &options,
                &request,
                &host,
                &history_options,
                &Deadline::new(10_000),
            )
            .unwrap(),
            baseline
        );
        std::fs::remove_file(ready).unwrap();
    }
}
