use serde_json::{Value, json};
use std::{fs, path::Path};
use tempfile::TempDir;
use yoke::{
    Engine,
    capture::Deadline,
    changes,
    container::{self, DockerSpec, Isolation},
};
#[cfg(windows)]
static PROCESS_FIXTURE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
fn plan() -> (TempDir, Engine, String) {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("a.rs"), "fn source() {}\n").unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let (snapshot, graph, _, _) = engine.index(&Deadline::new(10_000)).unwrap();
    let plan = changes::prepare(
        &mut engine.store,
        &snapshot,
        &graph,
        &json!({"edits":[{"kind":"create","path":"b.rs","text":"fn derived() {}\n"}]}),
        &Deadline::new(10_000),
    )
    .unwrap();
    (dir, engine, plan.id)
}
fn spec() -> DockerSpec {
    DockerSpec {
        docker_command: "docker".into(),
        docker_args: vec![],
        daemon_endpoint: if cfg!(windows) {
            "npipe:////./pipe/docker_engine".into()
        } else {
            "unix:///var/run/docker.sock".into()
        },
        image: format!("local-test@sha256:{}", "a".repeat(64)),
        command: vec![
            "/bin/sh".into(),
            "-c".into(),
            "printf 'tests executed: 2\\n'".into(),
        ],
        isolation: Isolation::Linux,
        user: "10001:10001".into(),
        memory_mb: 256,
        cpus: 1,
        pids: 32,
        tmpfs_mb: 64,
        test_count_pattern: "tests executed: ([0-9]+)".into(),
    }
}
#[cfg(windows)]
fn fixture(dir: &Path, mode: &str) -> DockerSpec {
    // A labeled Docker CLI process fixture; this does not claim a live daemon.
    let source = dir.join("docker-process-fixture.rs");
    let executable = dir.join("docker-process-fixture.exe");
    let log = dir.join("docker-arguments.jsonl");
    let mode_file = dir.join("docker-mode.txt");
    fs::write(&mode_file, mode).unwrap();
    let code = r#"use std::{fs,io::Write,process,time::Duration};
fn quote(text:&str)->String {
    let mut out=String::from("\"");
    for ch in text.chars() {match ch {
        '"'=>out.push_str("\\\""),'\\'=>out.push_str("\\\\"),'\n'=>out.push_str("\\n"),'\r'=>out.push_str("\\r"),'\t'=>out.push_str("\\t"),
        ch if ch<' '=>out.push_str(&format!("\\u{:04x}",ch as u32)),ch=>out.push(ch),
    }}out.push('"');out
}
fn main() {
    let mut args:Vec<String>=std::env::args().skip(1).collect();
    if args.first().map(String::as_str)!=Some("--host=npipe:////./pipe/docker_engine") {process::exit(92);}
    args.remove(0);
    let own=std::env::current_exe().unwrap();let base=own.parent().unwrap();
    let mode=fs::read_to_string(base.join("docker-mode.txt")).unwrap();
    let json=format!("[{}]",args.iter().map(|arg|quote(arg)).collect::<Vec<_>>().join(","));
    let mut log=fs::OpenOptions::new().create(true).append(true).open(base.join("docker-arguments.jsonl")).unwrap();
    writeln!(log,"{json}").unwrap();drop(log);
    let id="a".repeat(64);
    match args[0].as_str() {
        "image"=>println!("linux"),
        "create"=>{
            if mode=="create-failure" {eprintln!("labeled create failure");process::exit(125);}
            let at=args.iter().position(|arg|arg=="--cidfile").unwrap();
            fs::write(&args[at+1],&id).unwrap();
            let mount=args[args.iter().position(|arg|arg=="--mount").unwrap()+1].strip_prefix("type=bind,source=").unwrap();
            let source=mount.strip_suffix(",target=/source,readonly").unwrap();
            fs::write(base.join("fixture-source-path.txt"),source).unwrap();println!("{id}");
        },
        "start"=>{
            if mode=="timeout" {std::thread::sleep(Duration::from_secs(30));}
            if mode=="mutate"||mode=="extra-file" {
                let source=std::path::PathBuf::from(fs::read_to_string(base.join("fixture-source-path.txt")).unwrap());
                if mode=="mutate" {fs::write(source.join("a.rs"),"changed").unwrap();}
                else {fs::write(source.join("unexpected.rs"),"added").unwrap();}
            }
            println!("tests executed: {}",if mode=="zero" {0}else{2});
        },
        "stop"=>println!("{id}"),
        "rm"=>{if mode=="cleanup-failure" {process::exit(1);}println!("{id}");},
        "ps"=>{if mode=="cleanup-failure" {println!("{id}");}},
        _=>process::exit(91),
    }
}
"#;
    fs::write(&source, code).unwrap();
    let mut compiler = std::process::Command::new("rustc");
    compiler
        .args([
            "--edition=2024",
            "--crate-name",
            "yoke_docker_process_fixture",
            "-C",
            "debuginfo=0",
        ])
        .arg(&source)
        .arg("-o")
        .arg(&executable);
    {
        use std::os::windows::process::CommandExt;
        compiler.creation_flags(0x0800_0000);
    }
    let compiled = compiler.output().unwrap();
    assert!(
        compiled.status.success(),
        "Fixture compilation failed: {}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let mut result = spec();
    result.docker_command = executable.to_str().unwrap().into();
    result.docker_args = Vec::new();
    assert!(!log.exists());
    result
}
#[test]
fn missing_trust_mutable_images_root_users_and_unavailable_docker_never_create_approval() {
    let (_dir, engine, id) = plan();
    let mut config = spec();
    assert!(
        container::execute_container(&engine, &id, &config, false, &Deadline::new(10_000)).is_err()
    );
    config.image = "local-test:latest".into();
    assert!(
        container::execute_container(&engine, &id, &config, true, &Deadline::new(10_000)).is_err()
    );
    config = spec();
    config.user = "0:0".into();
    assert!(
        container::execute_container(&engine, &id, &config, true, &Deadline::new(10_000)).is_err()
    );
    config = spec();
    config.docker_command = "definitely-unavailable-docker-executable".into();
    let result = container::execute_container(&engine, &id, &config, true, &Deadline::new(10_000));
    assert!(result.is_err() || result.unwrap()["status"] == "failed");
    assert!(
        engine
            .store
            .get_record::<Value>("verification", &id)
            .is_err()
            || engine
                .store
                .get_record::<Value>("verification", &id)
                .unwrap()["status"]
                != "passed"
    );
}
#[cfg(windows)]
#[test]
fn labeled_cli_fixture_requires_isolation_and_owned_cleanup_before_passing_receipt() {
    let _fixture_guard = PROCESS_FIXTURE_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (dir, engine, id) = plan();
    let config = fixture(dir.path(), "pass");
    let result =
        container::execute_container(&engine, &id, &config, true, &Deadline::new(20_000)).unwrap();
    assert_eq!(result["status"], "passed", "{result}");
    assert_eq!(result["executedTests"], 2);
    assert_eq!(result["cleanupConfirmed"], true);
    let lines = fs::read_to_string(dir.path().join("docker-arguments.jsonl")).unwrap();
    let commands: Vec<Vec<String>> = lines
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let create = commands.iter().find(|args| args[0] == "create").unwrap();
    for required in [
        "--pull=never",
        "--network=none",
        "--cap-drop=ALL",
        "--security-opt=no-new-privileges",
        "--read-only",
        "--pids-limit",
        "--memory",
        "--cpus",
        "--user",
        "--tmpfs",
    ] {
        assert!(
            create.iter().any(|arg| arg == required),
            "missing {required}: {create:?}"
        );
    }
    let mount = &create[create.iter().position(|v| v == "--mount").unwrap() + 1];
    assert!(mount.ends_with(",target=/source,readonly"));
    let stop = commands.iter().find(|args| args[0] == "stop").unwrap();
    let remove = commands.iter().find(|args| args[0] == "rm").unwrap();
    assert_eq!(stop.last().unwrap(), &"a".repeat(64));
    assert_eq!(remove.last().unwrap(), &"a".repeat(64));
    assert_eq!(
        engine
            .store
            .get_record::<Value>("verification", &id)
            .unwrap()["level"],
        "executed"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("a.rs")).unwrap(),
        "fn source() {}\n"
    );
}
#[cfg(windows)]
#[test]
fn labeled_cli_zero_tests_integrity_failure_and_cleanup_failure_are_failed_receipts() {
    let _fixture_guard = PROCESS_FIXTURE_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    for mode in ["zero", "mutate", "extra-file", "cleanup-failure"] {
        let (dir, engine, id) = plan();
        let config = fixture(dir.path(), mode);
        let result =
            container::execute_container(&engine, &id, &config, true, &Deadline::new(20_000))
                .unwrap();
        assert_eq!(result["status"], "failed", "{mode}");
        assert_ne!(
            engine
                .store
                .get_record::<Value>("verification", &id)
                .unwrap()["status"],
            "passed"
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("a.rs")).unwrap(),
            "fn source() {}\n"
        );
    }
}

#[cfg(windows)]
#[test]
fn labeled_cli_timeout_still_removes_exact_owned_container_and_never_passes() {
    let _fixture_guard = PROCESS_FIXTURE_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (dir, engine, id) = plan();
    let config = fixture(dir.path(), "timeout");
    let started = std::time::Instant::now();
    let result =
        container::execute_container(&engine, &id, &config, true, &Deadline::new(2500)).unwrap();
    assert_eq!(result["status"], "failed");
    assert_eq!(result["timedOut"], true);
    assert_eq!(result["cleanupConfirmed"], true, "{result}");
    assert!(started.elapsed() < std::time::Duration::from_secs(9));
    let logs = fs::read_to_string(dir.path().join("docker-arguments.jsonl")).unwrap();
    let commands: Vec<Vec<String>> = logs
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let remove = commands.iter().find(|args| args[0] == "rm").unwrap();
    assert_eq!(remove.last().unwrap(), &"a".repeat(64));
}

#[cfg(windows)]
#[test]
fn remote_daemon_or_conflicting_host_prefix_is_refused_before_any_cli_execution() {
    let _fixture_guard = PROCESS_FIXTURE_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    for change in ["remote", "conflicting-prefix"] {
        let (dir, engine, id) = plan();
        let mut config = fixture(dir.path(), "pass");
        if change == "remote" {
            config.daemon_endpoint = "tcp://remote.example:2376".into();
        } else {
            config.docker_args.push("--context=unexpected".into());
        }
        assert!(
            container::execute_container(&engine, &id, &config, true, &Deadline::new(20_000))
                .is_err(),
            "{change}"
        );
        assert!(!dir.path().join("docker-arguments.jsonl").exists());
    }
}
#[cfg(windows)]
#[test]
fn labeled_cli_creation_failure_preserves_daemon_diagnostics_without_a_passing_receipt() {
    let _fixture_guard = PROCESS_FIXTURE_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (dir, engine, id) = plan();
    let config = fixture(dir.path(), "create-failure");
    let result =
        container::execute_container(&engine, &id, &config, true, &Deadline::new(20_000)).unwrap();
    assert_eq!(result["status"], "failed");
    assert!(
        result["stderr"]
            .as_str()
            .unwrap()
            .contains("labeled create failure")
    );
    assert_eq!(result["ownedContainerId"], json!(null));
}
#[test]
#[ignore = "Requires an explicitly supplied locally installed digest-pinned Linux image and live Docker daemon"]
fn real_daemon_digest_pinned_offline_container_verification() {
    let (_dir, engine, id) = plan();
    let mut config = spec();
    config.image = std::env::var("YOKE_DOCKER_TEST_IMAGE")
        .expect("Set YOKE_DOCKER_TEST_IMAGE to a locally installed name@sha256:digest");
    config.command = vec![
        "/bin/sh".into(),
        "-c".into(),
        r#"set -eu
test "$(id -u)" = 10001
test "$(id -g)" = 10001
grep -qx 'fn source() {}' /source/a.rs
grep -qx 'fn derived() {}' /source/b.rs
grep ' /source ' /proc/mounts | grep -q ' ro,'
if (printf forbidden > /source/a.rs) 2>/dev/null; then exit 88; fi
grep -Eq '^CapEff:[[:space:]]+0000000000000000$' /proc/self/status
grep -Eq '^NoNewPrivs:[[:space:]]+1$' /proc/self/status
awk 'NR>1 && $2=="00000000" {bad=1} END {exit bad?1:0}' /proc/net/route
test "$(cat /sys/fs/cgroup/memory.max)" = 268435456
test "$(cat /sys/fs/cgroup/pids.max)" = 32
test "$(cat /sys/fs/cgroup/cpu.max)" = '100000 100000'
printf 'build scratch' > /work/scratch
printf 'tests executed: 13\n'
"#
        .into(),
    ];
    let result =
        container::execute_container(&engine, &id, &config, true, &Deadline::new(30_000)).unwrap();
    assert_eq!(result["status"], "passed", "{result}");
    assert_eq!(result["cleanupConfirmed"], true);
    assert_eq!(result["executedTests"], 13);
    config.command = vec!["/bin/sh".into(), "-c".into(), "exit 7".into()];
    let failed =
        container::execute_container(&engine, &id, &config, true, &Deadline::new(30_000)).unwrap();
    assert_eq!(failed["status"], "failed", "{failed}");
    assert_eq!(failed["exitCode"], 7);
    assert_eq!(failed["cleanupConfirmed"], true);
    assert_eq!(
        engine
            .store
            .get_record::<Value>("verification", &id)
            .unwrap()["status"],
        "failed"
    );
}
