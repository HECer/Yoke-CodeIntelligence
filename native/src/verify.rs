use crate::{Engine, capture::Deadline, changes, paths::scoped};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
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

/// Check the complete copied source tree, including unexpected files and links.
/// Existing host execution keeps its previous semantics; containers use this
/// stronger inventory check before an executed verification can be approved.
pub(crate) fn snapshot_integrity(
    snapshot: &crate::model::Snapshot,
    root: &std::path::Path,
    deadline: &Deadline,
) -> Result<bool> {
    let mut expected = std::collections::BTreeMap::new();
    for file in &snapshot.files {
        ensure!(
            expected.insert(file.path.clone(), file).is_none(),
            "Duplicate snapshot path"
        );
    }
    let mut queue = vec![root.to_path_buf()];
    let mut seen = std::collections::BTreeSet::new();
    let mut entries = 0usize;
    while let Some(dir) = queue.pop() {
        deadline.check()?;
        let metadata = fs::symlink_metadata(&dir)?;
        if crate::paths::is_link(&metadata) || !metadata.is_dir() {
            return Ok(false);
        }
        for entry in fs::read_dir(&dir)? {
            deadline.check()?;
            entries += 1;
            ensure!(
                entries <= 200_000,
                "Source integrity inventory capacity exceeded"
            );
            let entry = entry?;
            let path = entry.path();
            let meta = fs::symlink_metadata(&path)?;
            if crate::paths::is_link(&meta) {
                return Ok(false);
            }
            if meta.is_dir() {
                queue.push(path);
                continue;
            }
            if !meta.is_file() {
                return Ok(false);
            }
            let relative = path
                .strip_prefix(root)?
                .to_str()
                .context("Source path is not UTF-8")?
                .replace('\\', "/");
            let Some(expected_file) = expected.get(&relative) else {
                return Ok(false);
            };
            if !file_integrity(expected_file, &path, deadline)? {
                return Ok(false);
            }
            seen.insert(relative);
        }
    }
    Ok(seen.len() == expected.len())
}

pub(crate) fn file_integrity(
    file: &crate::model::SourceFile,
    path: &std::path::Path,
    deadline: &Deadline,
) -> Result<bool> {
    deadline.check()?;
    let expected_size = u64::try_from(file.size)?;
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_file() || crate::paths::is_link(&meta) || meta.len() != expected_size {
        return Ok(false);
    }
    let mut input = fs::File::open(path)?;
    if input.metadata()?.len() != expected_size {
        return Ok(false);
    }
    let mut digest = Sha256::new();
    let mut bytes = 0u64;
    let mut buffer = [0u8; 65_536];
    loop {
        deadline.check()?;
        let count = input.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        bytes = bytes
            .checked_add(count as u64)
            .context("Source integrity byte count overflow")?;
        if bytes > expected_size {
            return Ok(false);
        }
        digest.update(&buffer[..count]);
    }
    deadline.check()?;
    Ok(bytes == expected_size && hex::encode(digest.finalize()) == file.hash)
}

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
    let plan = changes::load_checked_plan(&engine.store, plan_id, deadline)?;
    ensure!(plan.expires_at > changes::now(), "Plan expired");
    let snapshot = engine.store.snapshot(&plan.result_snapshot_id)?;
    engine.store.record(
        "verification",
        plan_id,
        &json!({"status":"running","level":"executed","snapshotId":snapshot.id}),
    )?;
    let tree = tempfile::Builder::new().prefix("yoke-verify-").tempdir()?;
    changes::materialize_plan(&engine.store, &plan, tree.path(), deadline)?;
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
        "SystemDrive",
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
    let integrity =
        snapshot.files.iter().all(|file| {
            scoped(tree.path(), &file.path)
                .and_then(|path| file_integrity(file, &path, deadline))
                .unwrap_or(false)
        }) && changes::plan_directory_integrity(&plan, &snapshot, tree.path(), deadline).is_ok();
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

#[cfg(test)]
mod integrity_tests {
    use super::*;
    #[test]
    fn oversized_copied_source_is_rejected_within_the_hash_budget() {
        let original = tempfile::TempDir::new().unwrap();
        fs::write(original.path().join("a.rs"), "fn original() {}\n").unwrap();
        let mut engine = Engine::open(original.path()).unwrap();
        let snapshot = engine.index(&Deadline::new(10_000)).unwrap().0;
        let copy = tempfile::TempDir::new().unwrap();
        changes::materialize(
            &engine.store,
            &snapshot,
            copy.path(),
            &Deadline::new(10_000),
        )
        .unwrap();
        fs::OpenOptions::new()
            .write(true)
            .open(copy.path().join("a.rs"))
            .unwrap()
            .set_len(128_000_000)
            .unwrap();
        let deadline = Deadline::new(300);
        assert!(!snapshot_integrity(&snapshot, copy.path(), &deadline).unwrap());
        assert!(
            deadline.check().is_ok(),
            "Source hashing consumed an oversized file beyond its deadline"
        );
    }
}
