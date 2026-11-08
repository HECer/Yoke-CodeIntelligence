use std::process::Command;
use yoke::{Engine, capture::source_scope_policy};

#[test]
fn host_cli_can_explicitly_capture_an_ignored_directory_as_read_only() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("hidden")).unwrap();
    std::fs::write(root.path().join(".gitignore"), "hidden/\n").unwrap();
    std::fs::write(root.path().join("main.rs"), "fn visible(){}\n").unwrap();
    std::fs::write(
        root.path().join("hidden/private.rs"),
        "fn captured_read_only(){}\n",
    )
    .unwrap();
    for include in [false, true] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_yoke"));
        command.arg("--root").arg(root.path());
        if include {
            command.args(["--include-ignored-dir", "hidden"]);
        }
        let output = command.arg("index").output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let engine = Engine::open(root.path()).unwrap();
        let snapshot = engine
            .store
            .snapshot(&engine.store.latest().unwrap().unwrap())
            .unwrap();
        assert_eq!(
            snapshot
                .files
                .iter()
                .any(|file| file.path == "hidden/private.rs"),
            include,
            "The explicit host capture option was not applied"
        );
        if include {
            let scope = source_scope_policy(&engine.store, &snapshot).unwrap();
            assert!(scope.ignored_at_capture("hidden/private.rs").unwrap());
            assert!(scope.check_host_write_scope("hidden/future.rs").is_err());
        }
    }
}
