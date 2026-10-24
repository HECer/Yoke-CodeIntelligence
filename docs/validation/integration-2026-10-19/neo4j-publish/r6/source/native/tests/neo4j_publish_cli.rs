use serde_json::{Value, json};
use std::{
    fs,
    path::Path,
    process::{Command, Output, Stdio},
};
#[cfg(windows)]
use yoke::neo4j_publish::Target;
use yoke::{Engine, capture::Deadline, neo4j_publish::Plan};
fn invoke(args: &[&str], cwd: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_yoke"))
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap()
}
fn config(dir: &Path) -> std::path::PathBuf {
    let path = dir.join("target.json");
    fs::write(&path,serde_json::to_vec(&json!({"baseUrl":"http://127.0.0.1:7474/","address":"127.0.0.1:7474","database":"neo4j","namespace":"owned \u{1f4a1}","user":"neo4j","useSystemRoots":false})).unwrap()).unwrap();
    path
}
#[test]
fn host_grants_and_foreign_source_flags_refuse_before_config_reads_and_store_creation() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("missing.json");
    let output = dir.path().join("output.json");
    for args in [
        vec![
            "neo4j-publish",
            "bootstrap-plan",
            "--config",
            missing.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
        ],
        vec![
            "neo4j-publish",
            "import-password",
            "--config",
            missing.to_str().unwrap(),
        ],
        vec![
            "neo4j-publish",
            "probe",
            "--plan",
            missing.to_str().unwrap(),
        ],
    ] {
        let p = invoke(&args, dir.path());
        assert!(!p.status.success());
        assert!(String::from_utf8_lossy(&p.stderr).contains("permission"));
        assert!(p.stdout.is_empty());
    }
    let p = invoke(
        &[
            "--root",
            dir.path().to_str().unwrap(),
            "neo4j-publish",
            "inspect",
            "--plan",
            missing.to_str().unwrap(),
        ],
        dir.path(),
    );
    assert!(!p.status.success());
    assert!(
        String::from_utf8_lossy(&p.stderr).contains("source/provider/write flags are unavailable")
    );
    assert!(!output.exists());
    assert!(!dir.path().join(".yoke").exists());
}
#[test]
fn bootstrap_offline_inspection_and_closed_configuration_never_connect_or_initialize_source() {
    let dir = tempfile::tempdir().unwrap();
    let config = config(dir.path());
    let output = dir.path().join("plan.json");
    let p = invoke(
        &[
            "neo4j-publish",
            "bootstrap-plan",
            "--config",
            config.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
            "--allow-output",
        ],
        dir.path(),
    );
    assert!(p.status.success(), "{}", String::from_utf8_lossy(&p.stderr));
    let value: Value = serde_json::from_slice(&p.stdout).unwrap();
    let bytes = fs::read(&output).unwrap();
    let plan = Plan::decode(&bytes, &Deadline::new(30000)).unwrap();
    assert_eq!(value["planId"], plan.id());
    let p = invoke(
        &[
            "neo4j-publish",
            "inspect",
            "--plan",
            output.to_str().unwrap(),
        ],
        dir.path(),
    );
    assert!(p.status.success());
    let value: Value = serde_json::from_slice(&p.stdout).unwrap();
    assert_eq!(value["networkConnections"], 0);
    assert_eq!(value["hostAuthorityGranted"], false);
    let mut target: Value = serde_json::from_slice(&fs::read(&config).unwrap()).unwrap();
    target["allowWrite"] = json!(true);
    fs::write(&config, serde_json::to_vec(&target).unwrap()).unwrap();
    let bad = dir.path().join("bad.json");
    let p = invoke(
        &[
            "neo4j-publish",
            "bootstrap-plan",
            "--config",
            config.to_str().unwrap(),
            "--output",
            bad.to_str().unwrap(),
            "--allow-output",
        ],
        dir.path(),
    );
    assert!(!p.status.success());
    assert!(!bad.exists());
    assert_eq!(fs::read(output).unwrap(), bytes);
    assert!(!dir.path().join(".yoke").exists());
}
#[test]
fn prepare_uses_the_existing_saved_snapshot_and_preserves_source_bytes_after_disk_changes() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    fs::create_dir(&source).unwrap();
    let original = "// owned \u{1f4a1}\r\nfn alpha(){beta();beta();}\r\nfn beta(){}\r\n";
    fs::write(source.join("owned.rs"), original).unwrap();
    let mut engine = Engine::open(&source).unwrap();
    let (snapshot, graph, _, _) = engine.index(&Deadline::new(30000)).unwrap();
    drop(engine);
    fs::write(
        source.join("owned.rs"),
        "changed disk bytes, not the saved source\n",
    )
    .unwrap();
    let config = config(dir.path());
    let output = dir.path().join("publish.json");
    let args = [
        "neo4j-publish",
        "prepare",
        "--config",
        config.to_str().unwrap(),
        "--source-root",
        source.to_str().unwrap(),
        "--snapshot-id",
        &snapshot.id,
        "--expected-revision",
        "absent",
        "--output",
        output.to_str().unwrap(),
        "--allow-output",
        "--allow-source-coordination",
    ];
    let p = invoke(&args, dir.path());
    assert!(p.status.success(), "{}", String::from_utf8_lossy(&p.stderr));
    let plan = Plan::decode(&fs::read(&output).unwrap(), &Deadline::new(30000)).unwrap();
    let payload = plan.payload().unwrap();
    assert_eq!(payload["entities"], json!(graph.entities));
    assert_eq!(payload["observations"], json!(graph.observations));
    assert_eq!(payload["sourceFiles"], json!(snapshot.files));
    assert_eq!(
        snapshot.files[0].hash,
        yoke::model::hash(original.as_bytes())
    );
    assert_eq!(
        fs::read(source.join("owned.rs")).unwrap(),
        b"changed disk bytes, not the saved source\n"
    );
    assert!(!dir.path().join(".yoke").exists());
}
#[cfg(windows)]
#[test]
fn lost_stdout_keeps_the_approved_journal_and_reports_its_plan_identity() {
    let dir = tempfile::tempdir().unwrap();
    let config = config(dir.path());
    let target: Target = serde_json::from_slice(&fs::read(config).unwrap()).unwrap();
    let plan = Plan::bootstrap(target, &Deadline::new(30000)).unwrap();
    let path = dir.path().join("plan.json");
    fs::write(&path, plan.encode(&Deadline::new(30000)).unwrap()).unwrap();
    let journal = dir.path().join(format!("neo4j-{}.jsonl", plan.id()));
    let mut child = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .args([
            "neo4j-publish",
            "approve",
            "--plan",
            path.to_str().unwrap(),
            "--expected-plan-id",
            plan.id(),
            "--journal",
            journal.to_str().unwrap(),
            "--allow-approval",
            "--allow-journal",
        ])
        .current_dir(dir.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    let out = child.wait_with_output().unwrap();
    assert!(!out.status.success());
    let error = String::from_utf8_lossy(&out.stderr);
    assert!(
        error.contains("neo4j_publish_delivery_failed_after_effect") && error.contains(plan.id())
    );
    let p = invoke(
        &[
            "neo4j-publish",
            "journal",
            "--plan",
            path.to_str().unwrap(),
            "--journal",
            journal.to_str().unwrap(),
            "--allow-journal",
        ],
        dir.path(),
    );
    assert!(p.status.success());
    let value: Value = serde_json::from_slice(&p.stdout).unwrap();
    assert_eq!(value["eventCount"], 1);
    assert_eq!(value["completedOperations"], 0);
    assert!(!dir.path().join(".yoke").exists());
}
