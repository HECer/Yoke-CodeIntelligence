use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};

const HUMAN: &str = "\u{feff}# Human instructions\r\nPrivate fixture wording stays here.\r\n";

struct Fixture {
    directory: tempfile::TempDir,
}
impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        for name in ["target", "state", "working"] {
            fs::create_dir(directory.path().join(name)).unwrap();
        }
        fs::write(directory.path().join("target/AGENTS.md"), HUMAN).unwrap();
        fs::write(
            directory.path().join("selected.exe"),
            b"Owned fixture; never executed",
        )
        .unwrap();
        fs::write(
            directory.path().join("policy.json"),
            b"{\"modes\":[\"planning\"]}",
        )
        .unwrap();
        let selection = json!({
            "installation":"fixture-cli", "targetRoots":[directory.path().join("target")],
            "executable":directory.path().join("selected.exe"),
            "policy":directory.path().join("policy.json"),
            "stateRoot":directory.path().join("state"),
        });
        fs::write(
            directory.path().join("selection.json"),
            serde_json::to_vec(&selection).unwrap(),
        )
        .unwrap();
        Self { directory }
    }
    fn path(&self, relative: &str) -> std::path::PathBuf {
        self.directory.path().join(relative)
    }
    fn command(&self, operation: &str, extra: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_yoke"));
        command
            .current_dir(self.path("working"))
            .args(["host-install", operation, "--selection"])
            .arg(self.path("selection.json"))
            .args(extra);
        command
    }
    fn call(&self, operation: &str, extra: &[&str]) -> Output {
        self.command(operation, extra).output().unwrap()
    }
    fn assert_source_store_absent(&self) {
        for path in [
            self.path("working/.yoke"),
            self.path("target/.yoke"),
            self.path("state/.yoke"),
        ] {
            assert!(
                !path.exists(),
                "Host lifecycle opened source state at {}",
                path.display()
            );
        }
    }
}
fn success(output: Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn save_plan(path: &Path, plan: &Value) {
    fs::write(path, serde_json::to_vec(plan).unwrap()).unwrap();
}

fn close_stdout_and_wait(mut command: Command) -> Output {
    let (reader, writer) = std::io::pipe().unwrap();
    drop(reader);
    let mut child = command
        .stdin(Stdio::null())
        .stdout(writer)
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(command);
    let started = Instant::now();
    let completed = loop {
        if child.try_wait().unwrap().is_some() {
            break true;
        }
        if started.elapsed() >= Duration::from_secs(45) {
            child.kill().unwrap();
            break false;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let output = child.wait_with_output().unwrap();
    assert!(
        completed,
        "Owned host installation child exceeded its bound: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn interrupted_initialization(state_root: &Path) -> PathBuf {
    let scaffold = state_root.join(yoke::host_install::STATE_DIRECTORY);
    fs::create_dir(&scaffold).unwrap();
    fs::write(scaffold.join("writer.lock"), []).unwrap();
    for name in ["plans", "receipts", "journals"] {
        fs::create_dir(scaffold.join(name)).unwrap();
    }
    scaffold
}

fn state_bytes(root: &Path) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
    fn visit(root: &Path, directory: &Path, result: &mut BTreeMap<PathBuf, Option<Vec<u8>>>) {
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            let metadata = fs::symlink_metadata(&path).unwrap();
            assert!(!metadata.file_type().is_symlink());
            let name = path.strip_prefix(root).unwrap().to_path_buf();
            if metadata.is_dir() {
                result.insert(name, None);
                visit(root, &path, result);
            } else {
                assert!(metadata.is_file());
                result.insert(name, Some(fs::read(path).unwrap()));
            }
        }
    }
    let mut result = BTreeMap::new();
    visit(root, root, &mut result);
    result
}

#[test]
fn host_install_cli_planning_and_status_are_pure_and_keep_human_text_private() {
    let fixture = Fixture::new();
    let plan = success(fixture.call("plan", &[]));
    assert_eq!(plan["profile"], "generic-agents-v1");
    assert!(yoke::model::valid_id(plan["id"].as_str().unwrap()));
    assert!(
        !serde_json::to_string(&plan)
            .unwrap()
            .contains("Private fixture wording")
    );
    assert_eq!(success(fixture.call("status", &[]))["state"], "absent");
    assert_eq!(
        fs::read(fixture.path("target/AGENTS.md")).unwrap(),
        HUMAN.as_bytes()
    );
    assert_eq!(fs::read_dir(fixture.path("state")).unwrap().count(), 0);
    fixture.assert_source_store_absent();
}

#[test]
fn host_install_cli_installs_and_uninstalls_only_its_owned_fragment() {
    let fixture = Fixture::new();
    let plan = success(fixture.call("plan", &[]));
    let plan_path = fixture.path("plan.json");
    save_plan(&plan_path, &plan);
    let receipt = success(fixture.call(
        "apply",
        &["--plan", plan_path.to_str().unwrap(), "--allow-host-writes"],
    ));
    assert_eq!(receipt["status"], "installed");
    assert_eq!(success(fixture.call("status", &[]))["state"], "installed");
    let mut installed = fs::read(fixture.path("target/AGENTS.md")).unwrap();
    assert!(installed.starts_with(HUMAN.as_bytes()));
    installed.extend_from_slice(b"\r\n# Later human instructions\r\n");
    fs::write(fixture.path("target/AGENTS.md"), installed).unwrap();
    let uninstall = success(fixture.call("uninstall-plan", &[]));
    save_plan(&plan_path, &uninstall);
    let result = success(fixture.call(
        "apply",
        &["--plan", plan_path.to_str().unwrap(), "--allow-host-writes"],
    ));
    assert_eq!(result["status"], "uninstalled");
    assert_eq!(
        fs::read(fixture.path("target/AGENTS.md")).unwrap(),
        [HUMAN.as_bytes(), b"\r\n# Later human instructions\r\n"].concat()
    );
    fixture.assert_source_store_absent();
}

#[test]
fn host_install_cli_write_permission_precedes_selection_or_plan_reads() {
    for operation in ["apply", "recover"] {
        let fixture = Fixture::new();
        fs::remove_file(fixture.path("selection.json")).unwrap();
        let extra = if operation == "apply" {
            vec!["--plan", "absent.json", "--allow-writes"]
        } else {
            vec![
                "--journal",
                "not-an-id",
                "--mode",
                "rollback",
                "--allow-writes",
            ]
        };
        let output = fixture.call(operation, &extra);
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("--allow-host-writes"),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(fs::read_dir(fixture.path("state")).unwrap().count(), 0);
        assert_eq!(
            fs::read(fixture.path("target/AGENTS.md")).unwrap(),
            HUMAN.as_bytes()
        );
        fixture.assert_source_store_absent();
    }
}

#[test]
fn host_install_cli_rejects_ambiguous_selection_and_small_write_output_before_mutation() {
    let fixture = Fixture::new();
    let original = fs::read(fixture.path("selection.json")).unwrap();
    fs::write(
        fixture.path("selection.json"),
        b"{\"installation\":\"a\",\"installation\":\"b\"}",
    )
    .unwrap();
    let duplicate = fixture.call("plan", &[]);
    assert!(!duplicate.status.success());
    assert!(String::from_utf8_lossy(&duplicate.stderr).contains("Duplicate JSON key"));
    fs::write(fixture.path("selection.json"), original).unwrap();
    let plan = success(fixture.call("plan", &[]));
    let path = fixture.path("plan.json");
    save_plan(&path, &plan);
    let small = fixture.call(
        "apply",
        &[
            "--plan",
            path.to_str().unwrap(),
            "--allow-host-writes",
            "--max-bytes",
            "1",
        ],
    );
    assert!(!small.status.success());
    assert_eq!(
        fs::read(fixture.path("target/AGENTS.md")).unwrap(),
        HUMAN.as_bytes()
    );
    assert_eq!(fs::read_dir(fixture.path("state")).unwrap().count(), 0);
    fixture.assert_source_store_absent();
}

#[test]
fn host_install_cli_output_failure_identifies_the_committed_result_and_can_be_retried() {
    let fixture = Fixture::new();
    let plan = success(fixture.call("plan", &[]));
    let plan_path = fixture.path("plan.json");
    save_plan(&plan_path, &plan);
    let output = close_stdout_and_wait(fixture.command(
        "apply",
        &[
            "--plan",
            plan_path.to_str().unwrap(),
            "--allow-host-writes",
            "--timeout",
            "30000",
        ],
    ));
    assert!(!output.status.success());
    let installed = success(fixture.call("status", &[]));
    assert_eq!(installed["state"], "installed");
    let receipt_id = installed["receiptId"].as_str().unwrap();
    let receipt: Value = serde_json::from_slice(
        &fs::read(
            fixture
                .path("state")
                .join(yoke::host_install::STATE_DIRECTORY)
                .join("receipts")
                .join(format!("{receipt_id}.json")),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(receipt["planId"], plan["id"]);
    assert_eq!(receipt["id"], receipt_id);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("durable"), "{stderr}");
    assert!(stderr.contains("output"), "{stderr}");
    for field in ["id", "planId", "journalId"] {
        assert!(
            stderr.contains(receipt[field].as_str().unwrap()),
            "Missing durable {field}: {stderr}"
        );
    }
    assert!(!stderr.contains("Private fixture wording"));
    let committed_bytes = fs::read(fixture.path("target/AGENTS.md")).unwrap();
    assert!(committed_bytes.starts_with(HUMAN.as_bytes()));
    assert!(String::from_utf8_lossy(&committed_bytes).contains("generic-agents-v1"));
    let retry = success(fixture.call(
        "apply",
        &["--plan", plan_path.to_str().unwrap(), "--allow-host-writes"],
    ));
    assert_eq!(retry["id"], receipt["id"]);
    assert_eq!(retry["journalId"], receipt["journalId"]);
    assert_eq!(
        fs::read(fixture.path("target/AGENTS.md")).unwrap(),
        committed_bytes
    );
    fixture.assert_source_store_absent();
}

#[test]
fn host_install_cli_valid_output_budget_is_admitted_before_any_host_state_or_file_write() {
    let fixture = Fixture::new();
    let mut selection: Value =
        serde_json::from_slice(&fs::read(fixture.path("selection.json")).unwrap()).unwrap();
    let mut roots = Vec::new();
    for index in 0..16 {
        let root = fixture.path(&format!(
            "receipt-budget-root-{index:02}-{}",
            "x".repeat(32)
        ));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("AGENTS.md"), HUMAN).unwrap();
        roots.push(root);
    }
    selection["targetRoots"] = json!(roots);
    fs::write(
        fixture.path("selection.json"),
        serde_json::to_vec(&selection).unwrap(),
    )
    .unwrap();
    let plan = success(fixture.call("plan", &["--max-bytes", "4000000"]));
    assert!(
        plan["bindings"]["roots"]
            .as_array()
            .unwrap()
            .iter()
            .map(|root| root["path"].as_str().unwrap().len())
            .sum::<usize>()
            > 1024,
        "The selected paths alone must exceed the valid output budget"
    );
    let plan_path = fixture.path("plan.json");
    save_plan(&plan_path, &plan);
    let output = fixture.call(
        "apply",
        &[
            "--plan",
            plan_path.to_str().unwrap(),
            "--allow-host-writes",
            "--max-bytes",
            "1024",
        ],
    );
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("JSON output exceeds byte capacity"),
        "{stderr}"
    );
    assert!(!stderr.contains("Private fixture wording"));
    assert_eq!(fs::read_dir(fixture.path("state")).unwrap().count(), 0);
    for root in roots {
        assert_eq!(fs::read(root.join("AGENTS.md")).unwrap(), HUMAN.as_bytes());
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        assert!(!root.join(".yoke").exists());
    }
    fixture.assert_source_store_absent();
}

#[test]
fn host_install_cli_initialization_permission_precedes_selection_and_plan_reads() {
    let fixture = Fixture::new();
    fs::remove_file(fixture.path("selection.json")).unwrap();
    let output = fixture.call(
        "recover-initialization",
        &["--plan", "absent.json", "--allow-writes"],
    );
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--allow-host-writes"), "{stderr}");
    assert_eq!(fs::read_dir(fixture.path("state")).unwrap().count(), 0);
    assert_eq!(
        fs::read(fixture.path("target/AGENTS.md")).unwrap(),
        HUMAN.as_bytes()
    );
    fixture.assert_source_store_absent();
}

#[test]
fn host_install_cli_initialization_repairs_an_owned_scaffold_and_replays_without_source_changes() {
    let fixture = Fixture::new();
    let plan = success(fixture.call("plan", &[]));
    let plan_path = fixture.path("plan.json");
    save_plan(&plan_path, &plan);
    let state = fixture.path("state");
    let scaffold = interrupted_initialization(&state);
    let before = state_bytes(&state);
    for operation in ["plan", "status"] {
        assert!(!fixture.call(operation, &[]).status.success());
        assert_eq!(state_bytes(&state), before);
    }
    let extra = ["--plan", plan_path.to_str().unwrap(), "--allow-host-writes"];
    let receipt = success(fixture.call("recover-initialization", &extra));
    assert_eq!(receipt["status"], "initialized");
    assert_eq!(receipt["planId"], plan["id"]);
    assert_eq!(receipt["installationId"], plan["installationId"]);
    assert_eq!(receipt["stateRoot"], plan["bindings"]["stateRoot"]);
    assert!(yoke::model::valid_id(receipt["id"].as_str().unwrap()));
    assert!(yoke::model::valid_id(receipt["token"].as_str().unwrap()));
    assert!(scaffold.join("schema.json").is_file());
    let persisted: Value =
        serde_json::from_slice(&fs::read(scaffold.join("initialization.json")).unwrap()).unwrap();
    assert_eq!(persisted, receipt);
    let after = state_bytes(&state);
    assert_eq!(
        success(fixture.call("recover-initialization", &extra)),
        receipt
    );
    assert_eq!(success(fixture.call("status", &[]))["state"], "absent");
    assert_eq!(state_bytes(&state), after);
    assert!(
        !serde_json::to_string(&receipt)
            .unwrap()
            .contains("Private fixture wording")
    );
    assert_eq!(
        fs::read(fixture.path("target/AGENTS.md")).unwrap(),
        HUMAN.as_bytes()
    );
    fixture.assert_source_store_absent();
}

#[test]
fn host_install_cli_initialization_refuses_foreign_scaffold_bytes_without_disclosing_them() {
    let fixture = Fixture::new();
    let plan = success(fixture.call("plan", &[]));
    let plan_path = fixture.path("plan.json");
    save_plan(&plan_path, &plan);
    let state = fixture.path("state");
    let scaffold = interrupted_initialization(&state);
    fs::write(
        scaffold.join("foreign.txt"),
        b"Private foreign fixture contents",
    )
    .unwrap();
    let before = state_bytes(&state);
    let output = fixture.call(
        "recover-initialization",
        &["--plan", plan_path.to_str().unwrap(), "--allow-host-writes"],
    );
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("unrecognized subcommand"), "{stderr}");
    for bytes in [&output.stdout, &output.stderr] {
        let text = String::from_utf8_lossy(bytes);
        assert!(!text.contains("Private foreign fixture contents"));
        assert!(!text.contains("Private fixture wording"));
    }
    assert_eq!(state_bytes(&state), before);
    assert_eq!(
        fs::read(fixture.path("target/AGENTS.md")).unwrap(),
        HUMAN.as_bytes()
    );
    fixture.assert_source_store_absent();
}

#[test]
fn host_install_cli_initialization_admits_a_valid_output_budget_before_writing_state() {
    let fixture = Fixture::new();
    let mut state = fixture.path("long-state");
    fs::create_dir(&state).unwrap();
    let mut index = 0;
    while state.to_str().unwrap().len() <= 1200 {
        state = state.join(format!(
            "owned-budget-component-{index:02}-{}",
            "x".repeat(64)
        ));
        fs::create_dir(&state).unwrap();
        index += 1;
    }
    let mut selection: Value =
        serde_json::from_slice(&fs::read(fixture.path("selection.json")).unwrap()).unwrap();
    selection["stateRoot"] = json!(state);
    fs::write(
        fixture.path("selection.json"),
        serde_json::to_vec(&selection).unwrap(),
    )
    .unwrap();
    let plan = success(fixture.call("plan", &["--max-bytes", "4000000"]));
    assert!(
        plan["bindings"]["stateRoot"]["path"]
            .as_str()
            .unwrap()
            .len()
            > 1024
    );
    let plan_path = fixture.path("plan.json");
    save_plan(&plan_path, &plan);
    let scaffold = interrupted_initialization(&state);
    let before = state_bytes(&state);
    let output = fixture.call(
        "recover-initialization",
        &[
            "--plan",
            plan_path.to_str().unwrap(),
            "--allow-host-writes",
            "--max-bytes",
            "1024",
        ],
    );
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("JSON output exceeds byte capacity"),
        "{stderr}"
    );
    assert_eq!(state_bytes(&state), before);
    assert!(!scaffold.join("schema.json").exists());
    assert!(!scaffold.join("initialization.json").exists());
    assert!(!state.join(".yoke").exists());
    assert_eq!(
        fs::read(fixture.path("target/AGENTS.md")).unwrap(),
        HUMAN.as_bytes()
    );
    fixture.assert_source_store_absent();
}

#[test]
fn host_install_cli_initialization_output_failure_identifies_its_durable_result() {
    let fixture = Fixture::new();
    let plan = success(fixture.call("plan", &[]));
    let plan_path = fixture.path("plan.json");
    save_plan(&plan_path, &plan);
    let scaffold = interrupted_initialization(&fixture.path("state"));
    let output = close_stdout_and_wait(fixture.command(
        "recover-initialization",
        &[
            "--plan",
            plan_path.to_str().unwrap(),
            "--allow-host-writes",
            "--timeout",
            "30000",
        ],
    ));
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        scaffold.join("initialization.json").is_file() && scaffold.join("schema.json").is_file(),
        "Expected a committed initialization before output failure: {stderr}"
    );
    let receipt: Value =
        serde_json::from_slice(&fs::read(scaffold.join("initialization.json")).unwrap()).unwrap();
    assert_eq!(receipt["status"], "initialized");
    assert_eq!(receipt["planId"], plan["id"]);
    assert!(
        stderr.contains("durable") && stderr.contains("output"),
        "{stderr}"
    );
    for field in ["id", "planId", "token"] {
        assert!(
            stderr.contains(receipt[field].as_str().unwrap()),
            "Missing durable initialization {field}: {stderr}"
        );
    }
    assert!(!stderr.contains("Private fixture wording"));
    let before_retry = state_bytes(&fixture.path("state"));
    assert_eq!(
        success(fixture.call(
            "recover-initialization",
            &["--plan", plan_path.to_str().unwrap(), "--allow-host-writes"],
        )),
        receipt
    );
    assert_eq!(state_bytes(&fixture.path("state")), before_retry);
    assert_eq!(success(fixture.call("status", &[]))["state"], "absent");
    assert_eq!(
        fs::read(fixture.path("target/AGENTS.md")).unwrap(),
        HUMAN.as_bytes()
    );
    fixture.assert_source_store_absent();
}
