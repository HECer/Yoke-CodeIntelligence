//! Digest-pinned, offline Docker execution with exact owned-container cleanup.
use crate::{
    Engine,
    capture::Deadline,
    changes,
    model::valid_id,
    paths::is_link,
    process::{self, CaptureLimits, CapturedOutput},
};
use anyhow::{Context, Result, ensure};
use regex::Regex;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
#[derive(Clone, Debug)]
pub enum Isolation {
    Linux,
}
#[derive(Clone, Debug)]
pub struct DockerSpec {
    pub docker_command: String,
    pub docker_args: Vec<String>,
    pub daemon_endpoint: String,
    pub image: String,
    pub command: Vec<String>,
    pub isolation: Isolation,
    pub user: String,
    pub memory_mb: u32,
    pub cpus: u32,
    pub pids: u32,
    pub tmpfs_mb: u32,
    pub test_count_pattern: String,
}
fn validate(spec: &DockerSpec) -> Result<Regex> {
    #[cfg(windows)]
    let endpoint = Regex::new(r"^npipe:////\./pipe/[A-Za-z0-9_.-]+$")?;
    #[cfg(unix)]
    let endpoint = Regex::new(r"^unix:///[A-Za-z0-9_./-]+$")?;
    ensure!(
        endpoint.is_match(&spec.daemon_endpoint),
        "Docker requires an explicit local daemon endpoint; remote contexts are unavailable"
    );
    ensure!(
        !spec.docker_args.iter().any(|arg| arg == "--host"
            || arg.starts_with("--host=")
            || arg.starts_with("-H")
            || arg == "--context"
            || arg.starts_with("--context=")),
        "Docker host/context prefix conflicts with the explicit local daemon endpoint"
    );
    ensure!(
        Regex::new(r"^[A-Za-z0-9][A-Za-z0-9._:/-]*@sha256:[a-f0-9]{64}$")?.is_match(&spec.image),
        "Docker image must be a locally installed digest-pinned name@sha256:digest"
    );
    ensure!(
        !spec.docker_command.is_empty()
            && spec.docker_args.len() <= 32
            && spec.command.len() <= 64
            && !spec.command.is_empty()
            && spec
                .command
                .iter()
                .all(|arg| arg.len() <= 64_000 && !arg.contains('\0'))
            && !spec.command[0].is_empty(),
        "Invalid host Docker command argv"
    );
    let fields: Vec<_> = spec.user.split(':').collect();
    ensure!(
        fields.len() == 2
            && fields.iter().all(|field| field
                .parse::<u32>()
                .is_ok_and(|id| id > 0 && id <= i32::MAX as u32)),
        "Docker requires a numeric nonroot uid:gid"
    );
    ensure!(
        (64..=16_384).contains(&spec.memory_mb)
            && (1..=16).contains(&spec.cpus)
            && (8..=512).contains(&spec.pids)
            && (8..=2048).contains(&spec.tmpfs_mb)
            && spec.tmpfs_mb <= spec.memory_mb,
        "Docker resource limits are invalid or unbounded"
    );
    ensure!(
        !spec.test_count_pattern.is_empty() && spec.test_count_pattern.len() <= 1000,
        "A bounded host-selected test count pattern is required"
    );
    let pattern = Regex::new(&spec.test_count_pattern)?;
    ensure!(
        pattern.captures_len() == 2,
        "Test count pattern requires exactly one numeric capture"
    );
    match spec.isolation {
        Isolation::Linux => {}
    }
    Ok(pattern)
}
fn executable(command: &str) -> Result<PathBuf> {
    let path = Path::new(command);
    if path.is_absolute() {
        let path = path
            .canonicalize()
            .context("Host Docker executable unavailable")?;
        ensure!(path.is_file(), "Host Docker executable is not a file");
        return Ok(path);
    }
    ensure!(
        path.components().count() == 1,
        "Docker executable must be absolute or selected from host PATH"
    );
    let directories = std::env::var_os("PATH").context("Host PATH unavailable")?;
    #[cfg(windows)]
    let suffixes: Vec<String> = std::iter::once(String::new())
        .chain(
            std::env::var("PATHEXT")
                .unwrap_or_else(|_| ".EXE;.COM;.BAT;.CMD".into())
                .split(';')
                .map(str::to_owned),
        )
        .collect();
    #[cfg(not(windows))]
    let suffixes = vec![String::new()];
    for dir in std::env::split_paths(&directories) {
        if !dir.is_absolute() {
            continue;
        }
        for suffix in &suffixes {
            let candidate = dir.join(format!("{command}{suffix}"));
            if candidate.is_file() {
                return candidate
                    .canonicalize()
                    .context("Host Docker executable unavailable");
            }
        }
    }
    anyhow::bail!("Host Docker executable unavailable")
}
fn invoke(
    exe: &Path,
    spec: &DockerSpec,
    cwd: &Path,
    args: &[String],
    deadline: &Deadline,
) -> Result<CapturedOutput> {
    let mut command = Command::new(exe);
    command
        .args(&spec.docker_args)
        .arg(format!("--host={}", spec.daemon_endpoint))
        .args(args)
        .current_dir(cwd);
    process::run_capture(
        command,
        &[],
        deadline,
        CaptureLimits {
            stdin_bytes: 0,
            stdout_bytes: 8_000_000,
            stderr_bytes: 8_000_000,
        },
    )
}
fn passed(output: &CapturedOutput) -> bool {
    output.exit_code == Some(0)
        && output.stdin_complete
        && output.output_complete
        && !output.timed_out
        && !output.output_exceeded
        && !output.io_error
}
fn cid(path: &Path) -> Result<String> {
    let meta = fs::symlink_metadata(path)?;
    ensure!(
        meta.is_file() && !is_link(&meta) && meta.len() <= 65,
        "Invalid owned container cidfile"
    );
    let id = fs::read_to_string(path)?;
    let id = id.trim();
    ensure!(valid_id(id), "Invalid full owned container ID");
    Ok(id.to_owned())
}
fn cleanup(exe: &Path, spec: &DockerSpec, cwd: &Path, id: &str) -> bool {
    // This separate finite budget survives request cancellation. No name or
    // command stdout can select a cleanup target: only the owned cidfile can.
    let deadline = Deadline::new(5000);
    if !valid_id(id) {
        return false;
    }
    let stop = invoke(
        exe,
        spec,
        cwd,
        &["stop".into(), "--time".into(), "1".into(), id.into()],
        &deadline,
    );
    let remove = invoke(
        exe,
        spec,
        cwd,
        &["rm".into(), "--force".into(), id.into()],
        &deadline,
    );
    let absent = invoke(
        exe,
        spec,
        cwd,
        &[
            "ps".into(),
            "--all".into(),
            "--no-trunc".into(),
            "--filter".into(),
            format!("id={id}"),
            "--format".into(),
            "{{.ID}}".into(),
        ],
        &deadline,
    );
    stop.is_ok_and(|out| passed(&out))
        && remove.is_ok_and(|out| passed(&out))
        && absent.is_ok_and(|out| passed(&out) && out.stdout.iter().all(u8::is_ascii_whitespace))
}
fn tests_executed(pattern: &Regex, output: &[u8]) -> Result<u64> {
    let text = std::str::from_utf8(output).context("Test output is not UTF-8")?;
    let mut count = 0u64;
    for captures in pattern.captures_iter(text) {
        let value = captures[1]
            .parse::<u64>()
            .context("Test count is not numeric")?;
        count = count.checked_add(value).context("Test count overflow")?;
        ensure!(count <= 1_000_000, "Test count exceeds capacity");
    }
    Ok(count)
}
pub fn execute_container(
    engine: &Engine,
    plan_id: &str,
    spec: &DockerSpec,
    trust: bool,
    deadline: &Deadline,
) -> Result<Value> {
    ensure!(
        trust,
        "Container verification requires explicit host --trust-project permission"
    );
    let pattern = validate(spec)?;
    deadline.check()?;
    let plan = changes::load_checked_plan(&engine.store, plan_id, deadline)?;
    ensure!(
        plan.id == plan_id
            && plan.workspace_id == engine.store.workspace_id
            && plan.expires_at > changes::now(),
        "Invalid or expired change plan"
    );
    let snapshot = engine.store.snapshot(&plan.result_snapshot_id)?;
    engine.store.set_deadline(deadline)?;
    engine.store.record("verification",plan_id,&json!({"status":"running","level":"executed","isolation":"docker-linux","snapshotId":snapshot.id}))?;
    let tree = tempfile::Builder::new()
        .prefix("yoke-container-source-")
        .tempdir()?;
    changes::materialize(&engine.store, &snapshot, tree.path(), deadline)?;
    let control = tempfile::Builder::new()
        .prefix("yoke-container-control-")
        .tempdir()?;
    let cidfile = control.path().join("owned.cid");
    let mut nonce = [0; 32];
    getrandom::fill(&mut nonce).map_err(|_| anyhow::anyhow!("System randomness unavailable"))?;
    let owner = hex::encode(nonce);
    let mut selected_id = None;
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut exit_code = None;
    let mut timed_out = false;
    let mut output_complete = false;
    let mut output_exceeded = false;
    let mut cleanup_confirmed = false;
    let mut executed_tests = 0;
    let mut error = None;
    let exe = executable(&spec.docker_command);
    let execution = (|| -> Result<()> {
        let exe = exe.as_ref().map_err(|e| anyhow::anyhow!("{e:#}"))?;
        let image = invoke(
            exe,
            spec,
            control.path(),
            &[
                "image".into(),
                "inspect".into(),
                spec.image.clone(),
                "--format".into(),
                "{{.Os}}".into(),
            ],
            deadline,
        )?;
        ensure!(
            passed(&image) && std::str::from_utf8(&image.stdout)?.trim() == "linux",
            "Digest-pinned Linux image unavailable locally; no pull fallback"
        );
        let source = tree
            .path()
            .canonicalize()?
            .to_str()
            .context("Snapshot mount path is not UTF-8")?
            .to_owned();
        // Rust canonical paths use Windows' verbatim prefix; Docker's Windows
        // bind parser requires the equivalent drive-qualified DOS spelling.
        #[cfg(windows)]
        let source = source.strip_prefix(r"\\?\").unwrap_or(&source).to_owned();
        #[cfg(windows)]
        ensure!(
            Regex::new(r"^[A-Za-z]:\\")?.is_match(&source),
            "Docker snapshot mount requires a local Windows drive path"
        );
        ensure!(
            !source.contains([',', '\n', '\r']),
            "Snapshot mount path cannot be represented safely"
        );
        let size = format!("{}m", spec.tmpfs_mb);
        let mut args = vec![
            "create".into(),
            "--cidfile".into(),
            cidfile
                .to_str()
                .context("Owned cidfile is not UTF-8")?
                .into(),
            "--pull=never".into(),
            "--network=none".into(),
            "--cap-drop=ALL".into(),
            "--security-opt=no-new-privileges".into(),
            "--read-only".into(),
            "--ipc=private".into(),
            "--cgroupns=private".into(),
            "--user".into(),
            spec.user.clone(),
            "--memory".into(),
            format!("{}m", spec.memory_mb),
            "--memory-swap".into(),
            format!("{}m", spec.memory_mb),
            "--cpus".into(),
            spec.cpus.to_string(),
            "--pids-limit".into(),
            spec.pids.to_string(),
            "--ulimit".into(),
            "nofile=1024:1024".into(),
            "--ulimit".into(),
            "core=0:0".into(),
            "--tmpfs".into(),
            format!("/tmp:rw,noexec,nosuid,nodev,size={size},mode=1777"),
            "--tmpfs".into(),
            format!("/work:rw,nosuid,nodev,size={size},mode=1777"),
            "--workdir".into(),
            "/work".into(),
            "--mount".into(),
            format!("type=bind,source={source},target=/source,readonly"),
            "--label".into(),
            format!("yoke.verification-owner={owner}"),
            "--env".into(),
            "CI=1".into(),
            "--env".into(),
            "DO_NOT_TRACK=1".into(),
            "--env".into(),
            "NO_COLOR=1".into(),
            "--entrypoint".into(),
            spec.command[0].clone(),
            spec.image.clone(),
        ];
        args.extend_from_slice(&spec.command[1..]);
        let create = invoke(exe, spec, control.path(), &args, deadline)?;
        // A failed/timed-out create can still have created a container. Read its
        // own cidfile before deciding whether creation succeeded.
        if cidfile.try_exists()? {
            selected_id = Some(cid(&cidfile)?);
        }
        timed_out |= create.timed_out;
        output_exceeded |= create.output_exceeded;
        if !passed(&create) {
            exit_code = create.exit_code;
            output_complete = create.output_complete && create.stdin_complete && !create.io_error;
            stdout = create.stdout.clone();
            stderr = create.stderr.clone();
        }
        ensure!(
            passed(&create),
            "Container creation failed or exceeded its bounds"
        );
        let id = selected_id
            .as_ref()
            .context("Docker did not write the owned cidfile")?;
        let run = invoke(
            exe,
            spec,
            control.path(),
            &["start".into(), "--attach".into(), id.clone()],
            deadline,
        )?;
        stdout = run.stdout;
        stderr = run.stderr;
        exit_code = run.exit_code;
        timed_out |= run.timed_out;
        output_complete = run.output_complete && run.stdin_complete && !run.io_error;
        output_exceeded |= run.output_exceeded;
        ensure!(
            exit_code == Some(0) && output_complete && !timed_out && !output_exceeded,
            "Container command failed or exceeded its bounds"
        );
        executed_tests = tests_executed(&pattern, &stdout)?;
        ensure!(
            executed_tests > 0,
            "Unknown or zero executed tests cannot authorize workspace application"
        );
        Ok(())
    })();
    if let Err(e) = execution {
        error = Some(format!("{e:#}"));
    }
    // A CLI cancellation may race cidfile creation. Re-read only this exact
    // private file after the owned CLI process group has been terminated.
    if selected_id.is_none() && cidfile.try_exists().unwrap_or(false) {
        match cid(&cidfile) {
            Ok(id) => selected_id = Some(id),
            Err(e) => error = Some(format!("Owned container identity unavailable: {e:#}")),
        }
    }
    if let (Ok(exe), Some(id)) = (&exe, &selected_id) {
        cleanup_confirmed = cleanup(exe, spec, control.path(), id);
    }
    timed_out |= deadline.check().is_err();
    let source_integrity =
        crate::verify::snapshot_integrity(&snapshot, tree.path(), deadline).unwrap_or(false);
    let approved = error.is_none()
        && exit_code == Some(0)
        && executed_tests > 0
        && output_complete
        && !output_exceeded
        && !timed_out
        && source_integrity
        && cleanup_confirmed;
    let retained_source = if selected_id.is_some() && !cleanup_confirmed {
        Some(tree.keep())
    } else {
        None
    };
    let retained_control = if selected_id.is_some() && !cleanup_confirmed {
        Some(control.keep())
    } else {
        None
    };
    let value = json!({"status":if approved{"passed"}else{"failed"},"level":"executed","isolation":"docker-linux","daemonEndpoint":spec.daemon_endpoint,"planId":plan_id,"snapshotId":snapshot.id,"image":spec.image,"command":spec.command,"exitCode":exit_code,"executedTests":executed_tests,"testCountEvidence":"host_selected_test_runner_output_assertion","timedOut":timed_out,"outputComplete":output_complete,"outputExceeded":output_exceeded,"sourceIntegrity":source_integrity,"cleanupConfirmed":cleanup_confirmed,"ownedContainerId":selected_id,"retainedSourcePath":retained_source,"retainedControlPath":retained_control,"error":error,"stdout":String::from_utf8_lossy(&stdout).chars().take(128_000).collect::<String>(),"stderr":String::from_utf8_lossy(&stderr).chars().take(128_000).collect::<String>(),"limitations":["The local daemon, image and host-selected command remain trusted; fixture checks are not a live daemon audit.","Positive test counts are runner assertions, not proof of exhaustive behavioral coverage."]});
    // Cleanup has its own budget; persisting a failure must also survive the
    // request deadline, without ever promoting incomplete execution to passed.
    let receipt_deadline = Deadline::new(1000);
    engine.store.set_deadline(&receipt_deadline)?;
    engine.store.record("verification", plan_id, &value)?;
    Ok(value)
}
