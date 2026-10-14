use crate::{
    Engine,
    capture::Deadline,
    changes::{self, Plan},
    model::hash,
    paths::scoped,
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    fs,
    io::Read,
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

pub fn execute(
    engine: &Engine,
    plan_id: &str,
    command: &str,
    args: &[String],
    trust: bool,
    deadline: &Deadline,
) -> Result<Value> {
    ensure!(
        trust,
        "Project verification requires explicit host --trust-project permission"
    );
    ensure!(
        !command.is_empty() && args.len() <= 100,
        "Invalid verification command"
    );
    let plan: Plan = engine.store.get_record("plan", plan_id)?;
    ensure!(plan.expires_at > changes::now(), "Plan expired");
    let snapshot = engine.store.snapshot(&plan.result_snapshot_id)?;
    engine.store.record(
        "verification",
        plan_id,
        &json!({"status":"running","level":"executed","snapshotId":snapshot.id}),
    )?;
    let tree = tempfile::Builder::new().prefix("yoke-verify-").tempdir()?;
    changes::materialize(&engine.store, &snapshot, tree.path(), deadline)?;
    let mut process = Command::new(command);
    process
        .args(args)
        .current_dir(tree.path())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_clear();
    for name in [
        "PATH",
        "SystemRoot",
        "WINDIR",
        "USERPROFILE",
        "HOME",
        "TEMP",
        "TMP",
        "LANG",
        "RUSTUP_HOME",
        "CARGO_HOME",
    ] {
        if let Some(value) = std::env::var_os(name) {
            process.env(name, value);
        }
    }
    process
        .env("CI", "1")
        .env("DO_NOT_TRACK", "1")
        .env("NO_COLOR", "1");
    let mut child =
        crate::process::ManagedChild::spawn(process).context("Verification command unavailable")?;
    let overflow = Arc::new(AtomicBool::new(false));
    let (completed, results) = mpsc::sync_channel(2);
    let reader = |mut input: Box<dyn Read + Send>,
                  overflow: Arc<AtomicBool>,
                  stream: usize,
                  completed: mpsc::SyncSender<(usize, Result<Vec<u8>>)>| {
        thread::spawn(move || {
            let result = (|| -> Result<Vec<u8>> {
                let mut output = Vec::new();
                let mut chunk = [0; 8192];
                loop {
                    let count = input.read(&mut chunk)?;
                    if count == 0 {
                        break;
                    }
                    if output.len() + count <= 8_000_000 {
                        output.extend_from_slice(&chunk[..count]);
                    } else {
                        overflow.store(true, Ordering::SeqCst);
                    }
                }
                Ok(output)
            })();
            let _ = completed.send((stream, result));
        })
    };
    reader(
        Box::new(child.stdout().take().unwrap()),
        overflow.clone(),
        0,
        completed.clone(),
    );
    reader(
        Box::new(child.stderr().take().unwrap()),
        overflow.clone(),
        1,
        completed,
    );
    let mut timed_out = false;
    let mut output: [Option<Vec<u8>>; 2] = [None, None];
    let mut status = None;
    let mut read_error = false;
    loop {
        while let Ok((stream, result)) = results.try_recv() {
            match result {
                Ok(bytes) => output[stream] = Some(bytes),
                Err(_) => {
                    read_error = true;
                    output[stream] = Some(Vec::new());
                }
            }
        }
        if status.is_none() {
            status = child.inner_mut().try_wait()?;
        }
        if status.is_some() && output.iter().all(Option::is_some) {
            break;
        }
        if deadline.check().is_err() || overflow.load(Ordering::SeqCst) {
            timed_out = deadline.check().is_err();
            child
                .start_kill()
                .context("Unable to terminate verification process group")?;
            if status.is_none() {
                status = Some(child.inner_mut().wait()?);
            }
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    let output_complete = output.iter().all(Option::is_some) && !read_error;
    let stdout = output[0].take().unwrap_or_default();
    let stderr = output[1].take().unwrap_or_default();
    let integrity = snapshot.files.iter().all(|file| {
        scoped(tree.path(), &file.path)
            .and_then(|path| Ok(hash(fs::read(path)?) == file.hash))
            .unwrap_or(false)
    });
    timed_out |= deadline.check().is_err();
    let passed = status.is_some_and(|s| s.success())
        && integrity
        && output_complete
        && !timed_out
        && !overflow.load(Ordering::SeqCst);
    let value = json!({"status":if passed{"passed"}else{"failed"},"level":"executed","planId":plan_id,"snapshotId":snapshot.id,"command":command,"args":args,"exitCode":status.and_then(|s| s.code()),"timedOut":timed_out,"outputComplete":output_complete,"outputExceeded":overflow.load(Ordering::SeqCst),"sourceIntegrity":integrity,"stdout":String::from_utf8_lossy(&stdout).chars().take(128000).collect::<String>(),"stderr":String::from_utf8_lossy(&stderr).chars().take(128000).collect::<String>(),"limitations":["Host-selected command ran in a copied tree under the host account; this is not an OS sandbox or proof of exhaustive test coverage."]});
    engine.store.record("verification", plan_id, &value)?;
    Ok(value)
}
