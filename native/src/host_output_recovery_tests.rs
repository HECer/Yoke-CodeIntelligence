//! Actual child-process interruptions of the installer, with test-only explicit
//! barriers. This module is never compiled into a production executable.

use super::*;
use std::{
    cell::RefCell,
    io::{BufRead, BufReader, Read, Write},
    process::{Child, Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

const PREFIX: &str = "YOKE_HOST_OUTPUT_RECOVERY_V1 ";
const TERMINAL_PREFIX: &str = "YOKE_HOST_OUTPUT_TERMINAL_V1 ";
const WORKER: &str = "host_install::output_recovery_tests::output_recovery_worker";
const SOURCE: &[u8] = b"Human-owned original: \xef\xbb\xbf \xe2\x98\x80\r\n";
const LINE_LIMIT: usize = 8192;

thread_local! {
    static STOP_AT: RefCell<Option<String>> = const { RefCell::new(None) };
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Request {
    root: PathBuf,
    nonce: String,
    phase: String,
    roots: usize,
    journal: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Observation {
    phase: String,
    plan_id: String,
    journal_id: String,
    root_index: usize,
    output_identity: Option<String>,
    output_length: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TerminalOutcome {
    plan_id: String,
    journal_id: String,
    success: bool,
    receipt_id: Option<String>,
    status: Option<String>,
    error: Option<String>,
}

enum WorkerMessage {
    Stopped(Observation),
    Finished(TerminalOutcome),
}

fn line(input: &mut impl BufRead) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    input
        .take((LINE_LIMIT + 1) as u64)
        .read_until(b'\n', &mut bytes)?;
    ensure!(
        !bytes.is_empty() && bytes.len() <= LINE_LIMIT && bytes.ends_with(b"\n"),
        "Host recovery fixture pipe line is absent or too large"
    );
    Ok(bytes)
}

/// Calls exist only behind cfg(test, windows). No environment variable, CLI
/// flag, analyzed source, or serialized plan can activate a barrier.
pub(super) fn barrier(
    phase: &str,
    plan: &Plan,
    root_index: usize,
    output: Option<(&str, usize)>,
) -> Result<()> {
    let enabled = STOP_AT.with(|slot| slot.borrow().as_deref() == Some(phase));
    if !enabled || root_index != 0 {
        return Ok(());
    }
    let observation = Observation {
        phase: phase.into(),
        plan_id: plan.id.clone(),
        journal_id: journal_id(plan),
        root_index,
        output_identity: output.map(|(id, _)| id.to_owned()),
        output_length: output
            .map(|(_, length)| u64::try_from(length))
            .transpose()?,
    };
    println!("\n{PREFIX}{}", serde_json::to_string(&observation)?);
    std::io::stdout().flush()?;
    let mut input = BufReader::new(std::io::stdin().lock());
    match line(&mut input)?.as_slice() {
        b"exit\n" => std::process::exit(23),
        b"continue\n" => {
            STOP_AT.with(|slot| *slot.borrow_mut() = None);
            Ok(())
        }
        _ => bail!("Unexpected recovery worker command"),
    }
}

fn fixture_target(root: &Path, index: usize) -> PathBuf {
    root.join(if index == 0 {
        "target".to_owned()
    } else {
        format!("target-{index}")
    })
}

fn selection_for_count(root: &Path, count: usize, d: &Deadline) -> Result<Selection> {
    ensure!((1..=16).contains(&count), "Invalid fixture target count");
    // Canonical paths are retained for fixture identity checks, while the
    // public constructor must still receive ordinary drive-rooted user paths.
    let root = match root.components().next() {
        Some(std::path::Component::Prefix(prefix))
            if matches!(prefix.kind(), std::path::Prefix::VerbatimDisk(_)) =>
        {
            PathBuf::from(
                root.to_str()
                    .context("Fixture path is not UTF-8")?
                    .strip_prefix(r"\\?\")
                    .context("Fixture verbatim drive prefix is invalid")?,
            )
        }
        _ => root.to_path_buf(),
    };
    Selection::new(
        "actual-output-interruption",
        &(0..count)
            .map(|index| fixture_target(&root, index))
            .collect::<Vec<_>>(),
        &root.join("native.exe"),
        &root.join("policy.json"),
        &root.join("state"),
        d,
    )
}

#[test]
#[ignore = "spawned only with an explicitly selected parent-owned temp fixture"]
fn output_recovery_worker() -> Result<()> {
    let request: Request = {
        let mut input = BufReader::new(std::io::stdin().lock());
        serde_json::from_slice(&line(&mut input)?)?
    };
    ensure!(
        request.nonce.starts_with("yoke-output-recovery-") && request.nonce.len() <= 128,
        "Unrecognized recovery worker fixture"
    );
    ensure!(
        request.root.is_absolute()
            && request.root.canonicalize()? == request.root
            && request.root.file_name().and_then(|name| name.to_str())
                == Some(request.nonce.as_str())
            && fs::read(request.root.join("permit"))? == request.nonce.as_bytes(),
        "Recovery worker requires its exact explicit owned fixture"
    );
    ensure!(
        [
            "allocating",
            "allocated",
            "registered",
            "disarmed",
            "populating",
            "partial",
            "flushed",
            "prepared",
        ]
        .contains(&request.phase.as_str())
            && (1..=16).contains(&request.roots),
        "Unknown recovery fixture checkpoint"
    );
    STOP_AT.with(|slot| *slot.borrow_mut() = Some(request.phase));
    let d = Deadline::new(30_000);
    let selection = selection_for_count(&request.root, request.roots, &d)?;
    let (plan_id, selected_journal, result) = if let Some(id) = request.journal {
        ensure!(valid_id(&id), "Invalid explicit fixture recovery journal");
        let summary = load_journal_summary(&selection, &id, &d)?;
        let result = recover(
            &selection,
            &WriteAuthority::new(&selection),
            &id,
            RecoveryMode::Resume,
            1002,
            &d,
        );
        (summary.plan_id, id, result)
    } else {
        let plan = plan_install(&selection, 1000, &d)?;
        let result = apply(
            &selection,
            &WriteAuthority::new(&selection),
            &plan,
            1001,
            &d,
        );
        (plan.id.clone(), journal_id(&plan), result)
    };
    ensure!(
        STOP_AT.with(|slot| slot.borrow().is_none()),
        "Installer did not reach/continue its requested test-only barrier: {result:?}"
    );
    let outcome = match result {
        Ok(receipt) => TerminalOutcome {
            plan_id,
            journal_id: selected_journal,
            success: true,
            receipt_id: Some(receipt.id),
            status: Some(receipt.status),
            error: None,
        },
        Err(error) => TerminalOutcome {
            plan_id,
            journal_id: selected_journal,
            success: false,
            receipt_id: None,
            status: None,
            error: Some(format!("{error:#}").chars().take(512).collect()),
        },
    };
    let bytes = serde_json::to_vec(&outcome)?;
    ensure!(
        bytes.len() <= 4096,
        "Worker terminal outcome exceeds pipe budget"
    );
    println!("\n{TERMINAL_PREFIX}{}", std::str::from_utf8(&bytes)?);
    std::io::stdout().flush()?;
    Ok(())
}

#[derive(Clone, Copy)]
enum Termination {
    Exit,
    Kill,
}

struct Interrupted {
    fixture: Option<tempfile::TempDir>,
    child: Option<Child>,
    selection: Selection,
    plan: Plan,
    observed: Option<Observation>,
    source: Vec<u8>,
    messages: Option<mpsc::Receiver<std::result::Result<WorkerMessage, String>>>,
}

impl Drop for Interrupted {
    fn drop(&mut self) {
        let Some(child) = self.child.as_mut() else {
            return;
        };
        if child.try_wait().ok().flatten().is_some() {
            return;
        }
        let _ = child.kill();
        let until = Instant::now() + Duration::from_secs(2);
        while Instant::now() < until {
            if child.try_wait().ok().flatten().is_some() {
                return;
            }
            thread::sleep(Duration::from_millis(5));
        }
        if let Some(fixture) = self.fixture.take() {
            eprintln!(
                "Recovery child {} did not stop; retained owned fixture {}",
                child.id(),
                fixture.keep().display()
            );
        }
    }
}

impl Interrupted {
    fn after(&self) -> Result<PathBuf> {
        auxiliary(&self.selection, &self.plan, 0, "after")
    }
    fn target(&self) -> Result<PathBuf> {
        target(&self.selection, 0)
    }
    fn recover(&self, mode: RecoveryMode) -> Result<Receipt> {
        recover(
            &self.selection,
            &WriteAuthority::new(&self.selection),
            &journal_id(&self.plan),
            mode,
            1002,
            &Deadline::new(30_000),
        )
    }
}

fn interrupt(phase: &str, termination: Termination) -> Result<Interrupted> {
    interrupt_with(phase, termination, SOURCE, 1)
}

fn new_fixture(source: &[u8], roots: usize) -> Result<Interrupted> {
    ensure!((1..=16).contains(&roots), "Invalid fixture root count");
    ensure!(
        source.len() <= MAX_FILE,
        "Fixture source exceeds its file budget"
    );
    std::str::from_utf8(source)?;
    let fixture = tempfile::Builder::new()
        .prefix("yoke-output-recovery-")
        .tempdir()?;
    let root = fixture.path().canonicalize()?;
    let nonce = root
        .file_name()
        .and_then(|name| name.to_str())
        .context("Fixture name is not UTF-8")?
        .to_owned();
    for index in 0..roots {
        let target = fixture_target(&root, index);
        fs::create_dir(&target)?;
        fs::write(target.join("AGENTS.md"), source)?;
    }
    fs::create_dir(root.join("state"))?;
    fs::write(
        root.join("native.exe"),
        b"owned test executable binding; never executed",
    )?;
    fs::write(root.join("policy.json"), b"{}")?;
    fs::write(root.join("permit"), nonce.as_bytes())?;
    let d = Deadline::new(30_000);
    let selection = selection_for_count(&root, roots, &d)?;
    let plan = plan_install(&selection, 1000, &d)?;
    Ok(Interrupted {
        fixture: Some(fixture),
        child: None,
        selection,
        plan,
        observed: None,
        source: source.to_vec(),
        messages: None,
    })
}

fn interrupt_with(
    phase: &str,
    termination: Termination,
    source: &[u8],
    roots: usize,
) -> Result<Interrupted> {
    let mut interrupted = new_fixture(source, roots)?;
    interrupt_worker(&mut interrupted, phase, termination, false)?;
    Ok(interrupted)
}

fn interrupt_worker(
    interrupted: &mut Interrupted,
    phase: &str,
    termination: Termination,
    resume: bool,
) -> Result<()> {
    start_worker(interrupted, phase, resume)?;
    terminate_worker(interrupted, termination)
}

fn terminate_worker(interrupted: &mut Interrupted, termination: Termination) -> Result<()> {
    let child = interrupted
        .child
        .as_mut()
        .context("Recovery worker missing")?;
    match termination {
        Termination::Exit => {
            let stdin = child
                .stdin
                .as_mut()
                .context("Recovery worker stdin missing")?;
            stdin.write_all(b"exit\n")?;
            stdin.flush()?;
        }
        Termination::Kill => child
            .kill()
            .context("Could not kill owned installer worker")?,
    }
    let status = wait_worker(child)?;
    match termination {
        Termination::Exit => ensure!(
            status.code() == Some(23),
            "Installer did not exit at its barrier: {status}"
        ),
        Termination::Kill => ensure!(
            !status.success(),
            "Killed installer unexpectedly returned success"
        ),
    }
    Ok(())
}

fn start_worker(interrupted: &mut Interrupted, phase: &str, resume: bool) -> Result<()> {
    let root = interrupted
        .fixture
        .as_ref()
        .context("Owned fixture missing")?
        .path()
        .canonicalize()?;
    let nonce = root
        .file_name()
        .and_then(|name| name.to_str())
        .context("Fixture name is not UTF-8")?
        .to_owned();
    interrupted.observed = None;
    let mut child = Command::new(std::env::current_exe()?)
        .args([
            "--ignored",
            "--exact",
            WORKER,
            "--nocapture",
            "--test-threads=1",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let stdout = child
        .stdout
        .take()
        .context("Recovery worker stdout missing")?;
    interrupted.child = Some(child);
    let (tx, rx) = mpsc::sync_channel(2);
    let _reader = thread::spawn(move || {
        let result = (|| -> Result<()> {
            let mut input = BufReader::new(stdout.take(65_536));
            let mut terminal = false;
            loop {
                let bytes = match line(&mut input) {
                    Ok(bytes) => bytes,
                    Err(_) if terminal => return Ok(()),
                    Err(error) => return Err(error),
                };
                if let Some(json) = bytes.strip_prefix(PREFIX.as_bytes()) {
                    ensure!(!terminal, "Worker stopped after its terminal outcome");
                    tx.send(Ok(WorkerMessage::Stopped(serde_json::from_slice(json)?)))
                        .map_err(|_| anyhow::anyhow!("Fixture parent closed its pipe"))?;
                } else if let Some(json) = bytes.strip_prefix(TERMINAL_PREFIX.as_bytes()) {
                    ensure!(!terminal, "Worker returned more than one terminal outcome");
                    terminal = true;
                    tx.send(Ok(WorkerMessage::Finished(serde_json::from_slice(json)?)))
                        .map_err(|_| anyhow::anyhow!("Fixture parent closed its pipe"))?;
                }
            }
        })();
        if let Err(error) = result {
            let _ = tx.send(Err(format!("{error:#}")));
        }
    });
    let request = Request {
        root,
        nonce,
        phase: phase.into(),
        roots: interrupted.plan.files.len(),
        journal: resume.then(|| journal_id(&interrupted.plan)),
    };
    let child = interrupted
        .child
        .as_mut()
        .context("Recovery worker missing")?;
    let stdin = child
        .stdin
        .as_mut()
        .context("Recovery worker stdin missing")?;
    serde_json::to_writer(&mut *stdin, &request)?;
    stdin.write_all(b"\n")?;
    stdin.flush()?;
    let message = rx
        .recv_timeout(Duration::from_secs(15))
        .context("Installer did not reach its test-only interruption barrier")?
        .map_err(anyhow::Error::msg)?;
    let WorkerMessage::Stopped(observation) = message else {
        bail!("Worker returned before the requested interruption barrier");
    };
    ensure!(
        observation.phase == phase
            && observation.plan_id == interrupted.plan.id
            && observation.journal_id == journal_id(&interrupted.plan)
            && observation.root_index == 0,
        "Interruption observation does not bind the actual reviewed plan"
    );
    interrupted.observed = Some(observation);
    interrupted.messages = Some(rx);
    Ok(())
}

fn wait_worker(child: &mut Child) -> Result<std::process::ExitStatus> {
    let until = Instant::now() + Duration::from_secs(5);
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        ensure!(
            Instant::now() < until,
            "Owned installer process did not terminate"
        );
        thread::sleep(Duration::from_millis(5));
    };
    Ok(status)
}

fn continue_worker(interrupted: &mut Interrupted) -> Result<TerminalOutcome> {
    let child = interrupted
        .child
        .as_mut()
        .context("Live recovery worker missing")?;
    let stdin = child
        .stdin
        .as_mut()
        .context("Live recovery worker stdin missing")?;
    stdin.write_all(b"continue\n")?;
    stdin.flush()?;
    let message = interrupted
        .messages
        .as_ref()
        .context("Live worker output pipe missing")?
        .recv_timeout(Duration::from_secs(35))
        .context("Continued installer did not report a bounded terminal outcome")?
        .map_err(anyhow::Error::msg)?;
    let WorkerMessage::Finished(outcome) = message else {
        bail!("Continued worker stopped instead of returning its typed outcome");
    };
    ensure!(
        wait_worker(child)?.success(),
        "Worker did not finish its test-only outcome protocol"
    );
    ensure!(
        outcome.plan_id == interrupted.plan.id
            && outcome.journal_id == journal_id(&interrupted.plan)
            && outcome.success == outcome.receipt_id.is_some()
            && outcome.success == outcome.status.is_some()
            && outcome.success != outcome.error.is_some(),
        "Worker terminal outcome differs from the exact reviewed operation"
    );
    Ok(outcome)
}

fn absent(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).context("Could not establish strict output absence"),
        Ok(_) => bail!("Unexpected output remains at {}", path.display()),
    }
}

fn state_manifest(selection: &Selection) -> Result<BTreeMap<PathBuf, String>> {
    state_manifest_with_live_lock(selection, false)
}

fn state_manifest_with_live_lock(
    selection: &Selection,
    live_lock: bool,
) -> Result<BTreeMap<PathBuf, String>> {
    let root = state(selection);
    let mut todo = vec![root.clone()];
    let mut found = BTreeMap::new();
    while let Some(directory) = todo.pop() {
        for entry in fs::read_dir(&directory)? {
            let path = entry?.path();
            let relative = path.strip_prefix(&root)?.to_path_buf();
            if live_lock && relative == Path::new("writer.lock") {
                let info = io::entry_info(&path, &Deadline::new(10_000))?
                    .context("Live fixture lock disappeared")?;
                ensure!(
                    !info.directory && info.length == 0,
                    "Live fixture lock changed shape"
                );
                // The child's held share-zero lock cannot be read. Its empty
                // shape is verified through the metadata-only namespace path.
                found.insert(relative, hash([]));
                continue;
            }
            let metadata = fs::symlink_metadata(&path)?;
            ensure!(
                !metadata.file_type().is_symlink(),
                "Unexpected fixture link"
            );
            ensure!(
                found.len() < 1024 && metadata.len() <= MAX_PLAN as u64,
                "Recovery fixture manifest budget exceeded"
            );
            if metadata.is_dir() {
                todo.push(path);
            } else {
                found.insert(relative, hash(fs::read(path)?));
            }
        }
    }
    Ok(found)
}

#[test]
fn actual_apply_exposes_no_human_bytes_before_durable_output_identity() -> Result<()> {
    let interrupted = interrupt("allocated", Termination::Kill)?;
    let observed = interrupted
        .observed
        .as_ref()
        .context("No interruption observation")?;
    let journal = load_journal_summary(
        &interrupted.selection,
        &journal_id(&interrupted.plan),
        &Deadline::new(10_000),
    )?;
    ensure!(
        journal.files[0].output_identity.is_none(),
        "Fixture did not stop before durable output ownership"
    );
    ensure!(
        observed.output_length == Some(0),
        "Installer wrote human bytes before its output identity was durably journaled"
    );
    absent(&interrupted.after()?)
}

#[test]
fn actual_apply_pre_identity_kill_resumes_after_os_removes_unregistered_empty_output() -> Result<()>
{
    let interrupted = interrupt("allocated", Termination::Kill)?;
    let receipt = interrupted.recover(RecoveryMode::Resume)?;
    ensure!(
        receipt.status == "installed",
        "Interrupted installation did not resume"
    );
    let bytes = fs::read(interrupted.target()?)?;
    ensure!(
        bytes.starts_with(SOURCE)
            && bytes
                .windows(b"yoke:managed".len())
                .any(|window| window == b"yoke:managed"),
        "Resumed installation did not preserve the exact original and managed block"
    );
    absent(&interrupted.after()?)
}

#[test]
fn actual_apply_pre_identity_process_exit_rolls_back_without_unidentified_output() -> Result<()> {
    let interrupted = interrupt("allocated", Termination::Exit)?;
    let receipt = interrupted.recover(RecoveryMode::Rollback)?;
    ensure!(
        receipt.status == "rolled_back",
        "Interrupted installation did not roll back"
    );
    ensure!(
        fs::read(interrupted.target()?)? == SOURCE,
        "Rollback changed the human original"
    );
    absent(&interrupted.after()?)
}

fn foreign_pre_identity_output(bytes: Option<&[u8]>) -> Result<()> {
    let interrupted = interrupt("allocated", Termination::Kill)?;
    let bytes = match bytes {
        Some(bytes) => bytes.to_vec(),
        None => host_config::apply_patch(
            SOURCE,
            &interrupted.plan.files[0].patch,
            &Deadline::new(10_000),
        )?,
    };
    match fs::remove_file(interrupted.after()?) {
        Ok(()) => (),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
        Err(error) => {
            return Err(error).context("Could not replace the owned interrupted fixture output");
        }
    }
    fs::write(interrupted.after()?, &bytes)?;
    let d = Deadline::new(10_000);
    let foreign = io::read_file(&interrupted.after()?, MAX_FILE, false, &d)?
        .context("Foreign fixture output missing")?;
    let foreign_id = foreign.identity.clone();
    drop(foreign);
    let before = state_manifest(&interrupted.selection)?;
    for mode in [RecoveryMode::Resume, RecoveryMode::Rollback] {
        ensure!(
            interrupted.recover(mode).is_err(),
            "Recovery adopted an unidentified foreign occupant"
        );
        ensure!(
            state_manifest(&interrupted.selection)? == before,
            "Foreign refusal mutated the durable journal"
        );
        let actual = io::read_file(&interrupted.after()?, MAX_FILE, false, &d)?
            .context("Recovery deleted a foreign occupant")?;
        ensure!(
            actual.identity == foreign_id && actual.bytes == bytes,
            "Recovery replaced or edited the unknown foreign output"
        );
        ensure!(
            fs::read(interrupted.target()?)? == SOURCE,
            "Foreign refusal changed the original target"
        );
    }
    Ok(())
}

#[test]
fn actual_pre_identity_crash_preserves_foreign_empty_occupant() -> Result<()> {
    foreign_pre_identity_output(Some(&[]))
}

#[test]
fn actual_pre_identity_crash_preserves_foreign_canonical_bytes_occupant() -> Result<()> {
    foreign_pre_identity_output(None)
}

#[test]
fn actual_apply_post_prepared_kill_still_resumes_as_a_positive_harness_control() -> Result<()> {
    let interrupted = interrupt("prepared", Termination::Kill)?;
    let observed = interrupted
        .observed
        .as_ref()
        .context("No prepared interruption observation")?;
    let journal = load_journal_summary(
        &interrupted.selection,
        &journal_id(&interrupted.plan),
        &Deadline::new(10_000),
    )?;
    ensure!(
        journal.files[0].output_identity == observed.output_identity,
        "Prepared barrier did not follow durable identity publication"
    );
    let receipt = interrupted.recover(RecoveryMode::Resume)?;
    ensure!(
        receipt.status == "installed",
        "Prepared output did not resume"
    );
    ensure!(
        fs::read(interrupted.target()?)?.starts_with(SOURCE),
        "Resumed prepared output lost human bytes"
    );
    Ok(())
}

fn observed(interrupted: &Interrupted) -> Result<&Observation> {
    interrupted
        .observed
        .as_ref()
        .context("No interruption observation")
}

fn raw_identity(path: &Path) -> Result<(String, usize)> {
    // This binding reader hashes raw bytes without interpreting a partial UTF-8
    // sequence as a complete instruction file. It never executes its input.
    let file = io::read_executable_binding(path, MAX_FILE, &Deadline::new(10_000))?
        .context("Expected owned fixture object is missing")?;
    Ok((file.identity, file.byte_length))
}

fn large_source() -> Vec<u8> {
    let mut source = SOURCE.to_vec();
    source.resize(65_535, b'x');
    source.extend_from_slice("🌍".as_bytes());
    source.extend(std::iter::repeat_n(b'x', 4096));
    source
}

fn finish(interrupted: &Interrupted, mode: RecoveryMode, retained: bool) -> Result<()> {
    let receipt = interrupted.recover(mode)?;
    check_finished(interrupted, mode, retained, receipt)
}

fn check_finished(
    interrupted: &Interrupted,
    mode: RecoveryMode,
    retained: bool,
    receipt: Receipt,
) -> Result<()> {
    let target = io::read_file(
        &interrupted.target()?,
        MAX_FILE,
        false,
        &Deadline::new(10_000),
    )?
    .context("Recovery lost the human target")?;
    match mode {
        RecoveryMode::Resume => {
            ensure!(
                receipt.status == "installed",
                "Resume did not commit installation"
            );
            ensure!(
                Some(&target.hash) == interrupted.plan.files[0].after_hash.as_ref()
                    && target.bytes.starts_with(&interrupted.source)
                    && target
                        .bytes
                        .windows(b"yoke:managed".len())
                        .any(|bytes| bytes == b"yoke:managed"),
                "Resume lost original bytes or published bytes outside the reviewed plan"
            );
            if retained {
                ensure!(
                    observed(interrupted)?.output_identity.as_ref() == Some(&target.identity),
                    "Recovery replaced the durably registered output instead of completing it"
                );
            }
        }
        RecoveryMode::Rollback => {
            ensure!(receipt.status == "rolled_back", "Rollback did not finish");
            ensure!(
                target.bytes == interrupted.source
                    && Some(&target.identity) == interrupted.plan.files[0].before_identity.as_ref(),
                "Rollback did not restore the exact original object and bytes"
            );
        }
    }
    drop(target);
    for index in 0..interrupted.plan.files.len() {
        absent(&auxiliary(
            &interrupted.selection,
            &interrupted.plan,
            index,
            "before",
        )?)?;
        absent(&auxiliary(
            &interrupted.selection,
            &interrupted.plan,
            index,
            "after",
        )?)?;
    }
    let summary = load_journal_summary(
        &interrupted.selection,
        &journal_id(&interrupted.plan),
        &Deadline::new(10_000),
    )?;
    ensure!(
        summary.schema_version == 2
            && summary.phase
                == match mode {
                    RecoveryMode::Resume => "committed",
                    RecoveryMode::Rollback => "rolled_back",
                },
        "Recovery did not publish a validated terminal v2 journal"
    );
    Ok(())
}

fn complete_case(
    phase: &str,
    termination: Termination,
    mode: RecoveryMode,
    retained: bool,
) -> Result<Interrupted> {
    let source = if matches!(phase, "partial" | "flushed") {
        large_source()
    } else {
        SOURCE.to_vec()
    };
    let interrupted = interrupt_with(phase, termination, &source, 1)?;
    let journal = journal_v2::load(
        &interrupted.selection,
        &journal_id(&interrupted.plan),
        &Deadline::new(10_000),
    )?;
    ensure!(
        journal.schema_version == 2,
        "Actual apply did not write a v2 journal"
    );
    if matches!(phase, "allocating" | "allocated" | "registered") {
        absent(&interrupted.after()?)?;
    } else {
        let observation = observed(&interrupted)?;
        let (id, length) = raw_identity(&interrupted.after()?)?;
        ensure!(
            observation.output_identity.as_ref() == Some(&id)
                && observation.output_length == Some(length as u64)
                && journal.files[0].latest_identity() == Some(id.as_str()),
            "Retained physical output is not bound to its durable allocation attempt"
        );
        if matches!(phase, "disarmed" | "populating") {
            ensure!(length == 0, "Human bytes appeared before population");
        }
        if phase == "partial" {
            let prefix = fs::read(interrupted.after()?)?;
            ensure!(
                prefix.len() == 65_536
                    && prefix == source[..65_536]
                    && std::str::from_utf8(&prefix).is_err(),
                "Fixture must stop on the exact canonical raw prefix inside a UTF-8 code point"
            );
        }
        if phase == "flushed" {
            let bytes = fs::read(interrupted.after()?)?;
            ensure!(
                bytes.len() > 65_536
                    && Some(hash(&bytes)) == interrupted.plan.files[0].after_hash
                    && journal.files[0].stage == "populating"
                    && journal.files[0].output_identity.is_none(),
                "Flushed barrier must precede the durable prepared event"
            );
        }
    }
    finish(&interrupted, mode, retained)?;
    Ok(interrupted)
}

#[test]
fn actual_allocating_kill_resumes_without_creating_unowned_output() -> Result<()> {
    complete_case("allocating", Termination::Kill, RecoveryMode::Resume, false).map(|_| ())
}

#[test]
fn actual_allocating_process_exit_rolls_back_without_output() -> Result<()> {
    complete_case(
        "allocating",
        Termination::Exit,
        RecoveryMode::Rollback,
        false,
    )
    .map(|_| ())
}

#[test]
fn actual_registered_kill_resumes_with_a_new_bounded_attempt() -> Result<()> {
    let interrupted = complete_case("registered", Termination::Kill, RecoveryMode::Resume, false)?;
    let journal = journal_v2::load(
        &interrupted.selection,
        &journal_id(&interrupted.plan),
        &Deadline::new(10_000),
    )?;
    ensure!(
        journal.files[0].attempts.len() == 2
            && journal.files[0].attempts[0].abandoned
            && journal.files[0].attempts[0].identity == observed(&interrupted)?.output_identity
            && !journal.files[0].attempts[1].abandoned,
        "Resume did not preserve the deleted armed attempt before allocating its successor"
    );
    Ok(())
}

#[test]
fn actual_registered_process_exit_rolls_back_after_os_removes_armed_output() -> Result<()> {
    complete_case(
        "registered",
        Termination::Exit,
        RecoveryMode::Rollback,
        false,
    )
    .map(|_| ())
}

#[test]
fn actual_disarmed_kill_resumes_the_exact_empty_registered_object() -> Result<()> {
    complete_case("disarmed", Termination::Kill, RecoveryMode::Resume, true).map(|_| ())
}

#[test]
fn actual_disarmed_process_exit_rolls_back_the_exact_empty_registered_object() -> Result<()> {
    complete_case("disarmed", Termination::Exit, RecoveryMode::Rollback, false).map(|_| ())
}

#[test]
fn actual_populating_kill_resumes_the_exact_registered_object() -> Result<()> {
    complete_case("populating", Termination::Kill, RecoveryMode::Resume, true).map(|_| ())
}

#[test]
fn actual_populating_process_exit_rolls_back_without_copying_human_bytes() -> Result<()> {
    complete_case(
        "populating",
        Termination::Exit,
        RecoveryMode::Rollback,
        false,
    )
    .map(|_| ())
}

#[test]
fn actual_partial_kill_resumes_a_raw_prefix_ending_inside_utf8() -> Result<()> {
    complete_case("partial", Termination::Kill, RecoveryMode::Resume, true).map(|_| ())
}

#[test]
fn actual_partial_process_exit_rolls_back_the_same_raw_prefix_object() -> Result<()> {
    complete_case("partial", Termination::Exit, RecoveryMode::Rollback, false).map(|_| ())
}

#[test]
fn actual_flushed_kill_resumes_before_prepared_identity_event() -> Result<()> {
    complete_case("flushed", Termination::Kill, RecoveryMode::Resume, true).map(|_| ())
}

#[test]
fn actual_flushed_process_exit_rolls_back_before_prepared_identity_event() -> Result<()> {
    complete_case("flushed", Termination::Exit, RecoveryMode::Rollback, false).map(|_| ())
}

fn replace_output(interrupted: &Interrupted, bytes: &[u8]) -> Result<String> {
    let after = interrupted.after()?;
    let (old, _) = raw_identity(&after)?;
    let replacement = after
        .parent()
        .context("Fixture output parent missing")?
        .join(".foreign-replacement");
    fs::write(&replacement, bytes)?;
    let (id, _) = raw_identity(&replacement)?;
    ensure!(
        id != old,
        "Foreign replacement must be a distinct live file object"
    );
    fs::remove_file(&after)?;
    fs::rename(&replacement, &after)?;
    ensure!(
        raw_identity(&after)?.0 == id,
        "Replacement rename changed object identity"
    );
    Ok(id)
}

fn refuses_output(interrupted: &Interrupted, bytes: &[u8], id: &str) -> Result<()> {
    let before = state_manifest(&interrupted.selection)?;
    for mode in [RecoveryMode::Resume, RecoveryMode::Rollback] {
        ensure!(
            interrupted.recover(mode).is_err(),
            "Recovery accepted a foreign/noncanonical output"
        );
        ensure!(
            state_manifest(&interrupted.selection)? == before
                && fs::read(interrupted.after()?)? == bytes
                && raw_identity(&interrupted.after()?)?.0 == id
                && fs::read(interrupted.target()?)? == interrupted.source,
            "Refused recovery changed output bytes, identity, original or durable history"
        );
    }
    Ok(())
}

#[test]
fn actual_disarmed_recovery_preserves_a_replacement_empty_file() -> Result<()> {
    let interrupted = interrupt("disarmed", Termination::Kill)?;
    let id = replace_output(&interrupted, &[])?;
    refuses_output(&interrupted, &[], &id)
}

#[test]
fn actual_partial_recovery_preserves_replacement_identical_raw_prefix() -> Result<()> {
    let interrupted = interrupt_with("partial", Termination::Kill, &large_source(), 1)?;
    let bytes = fs::read(interrupted.after()?)?;
    let id = replace_output(&interrupted, &bytes)?;
    refuses_output(&interrupted, &bytes, &id)
}

#[test]
fn actual_flushed_recovery_preserves_replacement_identical_canonical_bytes() -> Result<()> {
    let interrupted = interrupt_with("flushed", Termination::Kill, &large_source(), 1)?;
    let bytes = fs::read(interrupted.after()?)?;
    let id = replace_output(&interrupted, &bytes)?;
    refuses_output(&interrupted, &bytes, &id)
}

#[test]
fn actual_partial_recovery_preserves_noncanonical_bytes_on_registered_identity() -> Result<()> {
    let interrupted = interrupt_with("partial", Termination::Kill, &large_source(), 1)?;
    let (id, _) = raw_identity(&interrupted.after()?)?;
    let mut bytes = fs::read(interrupted.after()?)?;
    ensure!(!bytes.is_empty(), "Actual partial output is empty");
    bytes[0] ^= 1;
    fs::write(interrupted.after()?, &bytes)?;
    ensure!(
        raw_identity(&interrupted.after()?)?.0 == id,
        "Fixture replaced rather than edited output"
    );
    refuses_output(&interrupted, &bytes, &id)
}

#[test]
fn actual_partial_recovery_preflights_second_root_before_first_append() -> Result<()> {
    let interrupted = interrupt_with("partial", Termination::Kill, &large_source(), 2)?;
    let prefix = fs::read(interrupted.after()?)?;
    let id = raw_identity(&interrupted.after()?)?.0;
    let second = target(&interrupted.selection, 1)?;
    let foreign = b"Foreign second-root instructions must survive.\n";
    fs::write(&second, foreign)?;
    let before = state_manifest(&interrupted.selection)?;
    for mode in [RecoveryMode::Resume, RecoveryMode::Rollback] {
        ensure!(
            interrupted.recover(mode).is_err(),
            "Recovery ignored a changed later target"
        );
        ensure!(
            state_manifest(&interrupted.selection)? == before
                && fs::read(interrupted.after()?)? == prefix
                && raw_identity(&interrupted.after()?)?.0 == id
                && fs::read(interrupted.target()?)? == interrupted.source
                && fs::read(&second)? == foreign,
            "A later-root conflict was discovered only after mutating the first output or journal"
        );
    }
    Ok(())
}

#[test]
fn actual_registered_retry_limit_refuses_fourth_allocation_but_allows_rollback() -> Result<()> {
    let mut interrupted = interrupt("registered", Termination::Kill)?;
    for _ in 0..2 {
        interrupt_worker(&mut interrupted, "registered", Termination::Kill, true)?;
        absent(&interrupted.after()?)?;
    }
    let journal = journal_v2::load(
        &interrupted.selection,
        &journal_id(&interrupted.plan),
        &Deadline::new(10_000),
    )?;
    ensure!(
        journal.files[0].attempts.len() == 3
            && journal.files[0].attempts[..2]
                .iter()
                .all(|attempt| attempt.abandoned)
            && !journal.files[0].attempts[2].abandoned,
        "Three real registered crashes did not retain bounded attempt lineage"
    );
    let before = state_manifest(&interrupted.selection)?;
    ensure!(
        interrupted.recover(RecoveryMode::Resume).is_err(),
        "Recovery allocated a fourth attempt"
    );
    ensure!(
        state_manifest(&interrupted.selection)? == before
            && fs::read(interrupted.target()?)? == SOURCE,
        "Attempt exhaustion changed the journal or original"
    );
    absent(&interrupted.after()?)?;
    finish(&interrupted, RecoveryMode::Rollback, false)
}

#[test]
fn actual_populating_lineage_cannot_restart_when_registered_output_disappears() -> Result<()> {
    let interrupted = interrupt_with("partial", Termination::Kill, &large_source(), 1)?;
    fs::remove_file(interrupted.after()?)?;
    let before = state_manifest(&interrupted.selection)?;
    ensure!(
        interrupted.recover(RecoveryMode::Resume).is_err(),
        "Population restarted on a fresh identity"
    );
    ensure!(
        state_manifest(&interrupted.selection)? == before
            && fs::read(interrupted.target()?)? == interrupted.source,
        "Missing populated output caused an attempt or original mutation"
    );
    absent(&interrupted.after()?)?;
    finish(&interrupted, RecoveryMode::Rollback, false)
}

#[test]
fn actual_retained_reader_blocks_prefix_recovery_without_mutating_state() -> Result<()> {
    use std::os::windows::fs::OpenOptionsExt;
    let interrupted = interrupt_with("partial", Termination::Kill, &large_source(), 1)?;
    let prefix = fs::read(interrupted.after()?)?;
    let id = raw_identity(&interrupted.after()?)?.0;
    let reader = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(interrupted.after()?)?;
    let before = state_manifest(&interrupted.selection)?;
    for mode in [RecoveryMode::Resume, RecoveryMode::Rollback] {
        ensure!(
            interrupted.recover(mode).is_err(),
            "Recovery bypassed a retained foreign reader"
        );
        ensure!(
            state_manifest(&interrupted.selection)? == before
                && fs::read(interrupted.after()?)? == prefix
                && fs::read(interrupted.target()?)? == interrupted.source,
            "Sharing refusal mutated bytes or durable history"
        );
    }
    drop(reader);
    ensure!(
        raw_identity(&interrupted.after()?)?.0 == id,
        "Sharing refusal replaced registered output"
    );
    finish(&interrupted, RecoveryMode::Resume, true)
}

mod fixture_acl {
    use anyhow::{Result, ensure};
    use std::{
        ffi::c_void,
        fs::{File, OpenOptions},
        io::Read,
        mem::size_of,
        os::windows::{
            ffi::OsStrExt,
            fs::{MetadataExt, OpenOptionsExt},
            io::AsRawHandle,
        },
        path::Path,
        ptr,
    };

    #[link(name = "Advapi32")]
    unsafe extern "system" {
        fn ConvertStringSecurityDescriptorToSecurityDescriptorW(
            text: *const u16,
            revision: u32,
            descriptor: *mut *mut c_void,
            size: *mut u32,
        ) -> i32;
        fn GetSecurityInfo(
            handle: *mut c_void,
            object: i32,
            information: u32,
            owner: *mut *mut c_void,
            group: *mut *mut c_void,
            dacl: *mut *mut c_void,
            sacl: *mut *mut c_void,
            descriptor: *mut *mut c_void,
        ) -> u32;
        fn GetFileSecurityW(
            path: *const u16,
            information: u32,
            descriptor: *mut c_void,
            length: u32,
            needed: *mut u32,
        ) -> i32;
        fn SetSecurityInfo(
            handle: *mut c_void,
            object: i32,
            information: u32,
            owner: *mut c_void,
            group: *mut c_void,
            dacl: *mut c_void,
            sacl: *mut c_void,
        ) -> u32;
        fn GetSecurityDescriptorLength(descriptor: *mut c_void) -> u32;
        fn GetSecurityDescriptorControl(
            descriptor: *mut c_void,
            control: *mut u16,
            revision: *mut u32,
        ) -> i32;
        fn GetSecurityDescriptorDacl(
            descriptor: *mut c_void,
            present: *mut i32,
            dacl: *mut *mut c_void,
            defaulted: *mut i32,
        ) -> i32;
        fn GetSecurityDescriptorSacl(
            descriptor: *mut c_void,
            present: *mut i32,
            sacl: *mut *mut c_void,
            defaulted: *mut i32,
        ) -> i32;
        fn GetSecurityDescriptorOwner(
            descriptor: *mut c_void,
            owner: *mut *mut c_void,
            defaulted: *mut i32,
        ) -> i32;
        fn GetSecurityDescriptorGroup(
            descriptor: *mut c_void,
            group: *mut *mut c_void,
            defaulted: *mut i32,
        ) -> i32;
        fn GetLengthSid(sid: *mut c_void) -> u32;
    }
    #[link(name = "Kernel32")]
    unsafe extern "system" {
        fn LocalAlloc(flags: u32, bytes: usize) -> *mut c_void;
        fn LocalFree(memory: *mut c_void) -> *mut c_void;
        fn GetFileInformationByHandleEx(
            handle: *mut c_void,
            class: i32,
            output: *mut c_void,
            bytes: u32,
        ) -> i32;
    }

    #[repr(C)]
    #[derive(Default)]
    struct NativeId {
        volume: u64,
        id: [u8; 16],
    }

    fn identity(file: &File) -> Result<String> {
        let mut value = NativeId::default();
        ensure!(
            unsafe {
                GetFileInformationByHandleEx(
                    file.as_raw_handle(),
                    18,
                    ptr::from_mut(&mut value).cast(),
                    size_of::<NativeId>() as u32,
                )
            } != 0,
            "Fixture native identity query failed: {}",
            std::io::Error::last_os_error()
        );
        Ok(format!("{:016x}:{}", value.volume, hex::encode(value.id)))
    }

    struct Descriptor(*mut c_void);
    impl Drop for Descriptor {
        fn drop(&mut self) {
            unsafe { LocalFree(self.0) };
        }
    }

    fn stored_dacl_descriptor(path: &Path) -> Result<Descriptor> {
        // Keep the oracle independent from production's held-handle query. The
        // legacy named API returns stored ACE flags, unlike GetSecurityInfo.
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut needed = 0;
        let result = unsafe { GetFileSecurityW(wide.as_ptr(), 7, ptr::null_mut(), 0, &mut needed) };
        ensure!(
            result == 0 && std::io::Error::last_os_error().raw_os_error() == Some(122),
            "Fixture descriptor length query failed"
        );
        ensure!(
            (20..=1_048_576).contains(&needed),
            "Fixture descriptor exceeds bounds"
        );
        let capacity = needed;
        let raw = unsafe { LocalAlloc(0x40, capacity as usize) };
        ensure!(!raw.is_null(), "Fixture descriptor allocation failed");
        let descriptor = Descriptor(raw);
        ensure!(
            unsafe { GetFileSecurityW(wide.as_ptr(), 7, raw, capacity, &mut needed) } != 0,
            "Fixture stored descriptor query failed"
        );
        ensure!(
            (20..=capacity).contains(&needed),
            "Fixture descriptor changed size"
        );
        ensure!(
            (20..=needed).contains(&unsafe { GetSecurityDescriptorLength(raw) }),
            "Fixture stored descriptor length invalid"
        );
        Ok(descriptor)
    }

    fn descriptor(file: &File, path: &Path, information: u32) -> Result<(Descriptor, usize, u16)> {
        let mut raw = ptr::null_mut();
        let stored = (information == 7)
            .then(|| stored_dacl_descriptor(path))
            .transpose()?;
        let result = if let Some(stored) = &stored {
            raw = stored.0;
            0
        } else {
            unsafe {
                GetSecurityInfo(
                    file.as_raw_handle(),
                    1,
                    information,
                    ptr::null_mut(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                    &mut raw,
                )
            }
        };
        ensure!(
            result == 0,
            "Fixture security query failed: {}",
            std::io::Error::from_raw_os_error(result as i32)
        );
        ensure!(!raw.is_null(), "Fixture security descriptor absent");
        let descriptor = stored.unwrap_or_else(|| Descriptor(raw));
        let length = unsafe { GetSecurityDescriptorLength(raw) } as usize;
        ensure!(
            (20..=1_048_576).contains(&length),
            "Fixture descriptor exceeds bounds"
        );
        let (mut control, mut revision) = (0, 0);
        ensure!(
            unsafe { GetSecurityDescriptorControl(raw, &mut control, &mut revision) } != 0,
            "Fixture descriptor control query failed"
        );
        Ok((descriptor, length, control))
    }

    fn within(
        descriptor: &Descriptor,
        length: usize,
        value: *mut c_void,
        bytes: usize,
    ) -> Result<()> {
        let base = descriptor.0 as usize;
        let start = value as usize;
        ensure!(
            start >= base
                && start
                    .checked_add(bytes)
                    .zip(base.checked_add(length))
                    .is_some_and(|(end, limit)| end <= limit),
            "Fixture security pointer exceeds descriptor bounds"
        );
        Ok(())
    }

    fn acl_bytes(descriptor: &Descriptor, length: usize, label: bool) -> Result<Option<Vec<u8>>> {
        let (mut present, mut defaulted, mut acl) = (0, 0, ptr::null_mut());
        let result = unsafe {
            if label {
                GetSecurityDescriptorSacl(descriptor.0, &mut present, &mut acl, &mut defaulted)
            } else {
                GetSecurityDescriptorDacl(descriptor.0, &mut present, &mut acl, &mut defaulted)
            }
        };
        ensure!(result != 0, "Fixture ACL query failed");
        if present == 0 || acl.is_null() {
            return Ok(None);
        }
        within(descriptor, length, acl, 8)?;
        let size = unsafe { ptr::read_unaligned(acl.cast::<u8>().add(2).cast::<u16>()) } as usize;
        ensure!((8..=65_535).contains(&size), "Fixture ACL length invalid");
        within(descriptor, length, acl, size)?;
        Ok(Some(
            unsafe { std::slice::from_raw_parts(acl.cast::<u8>(), size) }.to_vec(),
        ))
    }

    fn sid_bytes(descriptor: &Descriptor, length: usize, group: bool) -> Result<Option<Vec<u8>>> {
        let (mut sid, mut defaulted) = (ptr::null_mut(), 0);
        let result = unsafe {
            if group {
                GetSecurityDescriptorGroup(descriptor.0, &mut sid, &mut defaulted)
            } else {
                GetSecurityDescriptorOwner(descriptor.0, &mut sid, &mut defaulted)
            }
        };
        ensure!(result != 0, "Fixture owner/group query failed");
        if sid.is_null() {
            return Ok(None);
        }
        within(descriptor, length, sid, 8)?;
        let size = unsafe { GetLengthSid(sid) } as usize;
        ensure!((8..=68).contains(&size), "Fixture SID length invalid");
        within(descriptor, length, sid, size)?;
        Ok(Some(
            unsafe { std::slice::from_raw_parts(sid.cast::<u8>(), size) }.to_vec(),
        ))
    }

    fn security_hash(file: &File, path: &Path) -> Result<String> {
        let (security, length, control) = descriptor(file, path, 7)?;
        let owner = sid_bytes(&security, length, false)?;
        let group = sid_bytes(&security, length, true)?;
        let dacl = acl_bytes(&security, length, false)?;
        let (label, label_length, label_control) = descriptor(file, path, 0x10)?;
        let sacl = acl_bytes(&label, label_length, true)?;
        Ok(crate::model::identity(&(
            "host-output-security-v1",
            owner,
            group,
            control & 0x150c,
            dacl,
            label_control & 0x2a30,
            sacl,
        )))
    }

    pub(super) fn observe(path: &Path) -> Result<super::LiveFileState> {
        // A test observer requests no data-write/delete access. Its share mask
        // admits the paused child's existing writable/DELETE handles; using the
        // production SHARE_READ-only guard would conflict before the assertion.
        let mut file = OpenOptions::new()
            .read(true)
            .share_mode(7)
            .custom_flags(0x0020_0000)
            .open(path)?;
        let metadata = file.metadata()?;
        ensure!(
            metadata.is_file()
                && metadata.file_attributes() & 0x400 == 0
                && metadata.len() <= super::MAX_FILE as u64,
            "Fixture observer requires a bounded ordinary file"
        );
        let id = identity(&file)?;
        let mut bytes = Vec::new();
        (&mut file)
            .take(super::MAX_FILE as u64 + 1)
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() as u64 == metadata.len() && identity(&file)? == id,
            "Fixture object changed during bounded observation"
        );
        Ok(super::LiveFileState {
            identity: id,
            bytes,
            security_hash: security_hash(&file, path)?,
        })
    }

    pub(super) fn protect_owned_output(path: &Path, expected_id: &str) -> Result<()> {
        // Limit access to this owned fixture's owner and SYSTEM. Do not grant
        // Everyone access or modify any real user/configuration path.
        let text: Vec<u16> = "D:P(A;;FA;;;OW)(A;;FA;;;SY)"
            .encode_utf16()
            .chain(Some(0))
            .collect();
        // WRITE_DAC/READ_CONTROL/attributes/synchronize request no file-data
        // write or DELETE authority, and share7 permits the child's held object.
        let file = OpenOptions::new()
            .access_mode(0x0016_0080)
            .share_mode(7)
            .custom_flags(0x0020_0000)
            .open(path)?;
        ensure!(
            identity(&file)? == expected_id,
            "Fixture ACL target identity changed before mutation"
        );
        let mut raw = std::ptr::null_mut();
        ensure!(
            unsafe {
                ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    text.as_ptr(),
                    1,
                    &mut raw,
                    std::ptr::null_mut(),
                )
            } != 0,
            "Owned fixture security descriptor creation failed: {}",
            std::io::Error::last_os_error()
        );
        let descriptor = Descriptor(raw);
        let (mut present, mut defaulted, mut dacl) = (0, 0, ptr::null_mut());
        ensure!(
            unsafe {
                GetSecurityDescriptorDacl(descriptor.0, &mut present, &mut dacl, &mut defaulted)
            } != 0
                && present != 0
                && !dacl.is_null(),
            "Fixture replacement DACL absent"
        );
        let changed = unsafe {
            SetSecurityInfo(
                file.as_raw_handle(),
                1,
                4 | 0x8000_0000,
                ptr::null_mut(),
                ptr::null_mut(),
                dacl,
                ptr::null_mut(),
            )
        };
        ensure!(
            changed == 0,
            "Owned fixture DACL modification failed: {}",
            std::io::Error::from_raw_os_error(changed as i32)
        );
        Ok(())
    }
}

#[derive(PartialEq, Eq)]
struct LiveFileState {
    identity: String,
    bytes: Vec<u8>,
    security_hash: String,
}

fn live_file_state(path: &Path) -> Result<LiveFileState> {
    fixture_acl::observe(path)
}

fn live_prepared() -> Result<Interrupted> {
    let mut interrupted = new_fixture(SOURCE, 1)?;
    start_worker(&mut interrupted, "prepared", false)?;
    let original = live_file_state(&interrupted.target()?)?;
    let output = live_file_state(&interrupted.after()?)?;
    let journal = journal_v2::load(
        &interrupted.selection,
        &journal_id(&interrupted.plan),
        &Deadline::new(10_000),
    )?;
    ensure!(
        original.bytes == SOURCE
            && Some(&original.identity) == interrupted.plan.files[0].before_identity.as_ref()
            && Some(&output.identity) == observed(&interrupted)?.output_identity.as_ref()
            && Some(hash(&output.bytes)) == interrupted.plan.files[0].after_hash
            && journal.files[0].stage == "prepared"
            && journal.files[0].latest_identity() == Some(output.identity.as_str())
            && journal.files[0]
                .attempts
                .last()
                .and_then(|attempt| attempt.security_hash.as_ref())
                == Some(&output.security_hash),
        "Live barrier did not retain the exact original and durably prepared output"
    );
    absent(&auxiliary(
        &interrupted.selection,
        &interrupted.plan,
        0,
        "before",
    )?)?;
    Ok(interrupted)
}

fn live_prepared_control() -> Result<()> {
    let mut interrupted = live_prepared()?;
    let outcome = continue_worker(&mut interrupted)?;
    ensure!(
        outcome.success && outcome.status.as_deref() == Some("installed"),
        "Untampered real apply did not finish after explicit continue: {outcome:?}"
    );
    let receipt = head(&interrupted.selection, &Deadline::new(10_000))?
        .context("Continued apply returned without an ownership receipt")?;
    let actual = live_file_state(&interrupted.target()?)?;
    let summary = load_journal_summary(
        &interrupted.selection,
        &journal_id(&interrupted.plan),
        &Deadline::new(10_000),
    )?;
    ensure!(
        outcome.receipt_id.as_ref() == Some(&receipt.id)
            && receipt.plan_id == interrupted.plan.id
            && summary.phase == "committed"
            && Some(&actual.identity) == observed(&interrupted)?.output_identity.as_ref()
            && Some(hash(&actual.bytes)) == interrupted.plan.files[0].after_hash
            && actual.bytes.starts_with(SOURCE),
        "Continued apply did not commit the exact prepared object and reviewed bytes"
    );
    absent(&interrupted.after()?)?;
    absent(&auxiliary(
        &interrupted.selection,
        &interrupted.plan,
        0,
        "before",
    )?)
}

fn live_acl_refusal(change_original: bool) -> Result<()> {
    // A real successful continuation is required even when this test runs in
    // isolation; a missing worker/retain implementation is not a refusal oracle.
    live_prepared_control()?;
    let mut interrupted = live_prepared()?;
    let original_before = live_file_state(&interrupted.target()?)?;
    let output_before = live_file_state(&interrupted.after()?)?;
    let registered = journal_v2::load(
        &interrupted.selection,
        &journal_id(&interrupted.plan),
        &Deadline::new(10_000),
    )?;
    let history = state_manifest_with_live_lock(&interrupted.selection, true)?;
    let changed_path = if change_original {
        interrupted.target()?
    } else {
        interrupted.after()?
    };
    let changed_id = if change_original {
        &original_before.identity
    } else {
        &output_before.identity
    };
    fixture_acl::protect_owned_output(&changed_path, changed_id)?;
    let original_expected = live_file_state(&interrupted.target()?)?;
    let output_expected = live_file_state(&interrupted.after()?)?;
    if change_original {
        ensure!(
            original_expected.identity == original_before.identity
                && original_expected.bytes == original_before.bytes
                && original_expected.security_hash != original_before.security_hash
                && output_expected == output_before,
            "Live original ACL fixture changed bytes, identity or the prepared output"
        );
    } else {
        ensure!(
            output_expected.identity == output_before.identity
                && output_expected.bytes == output_before.bytes
                && output_expected.security_hash != output_before.security_hash
                && original_expected == original_before,
            "Live output ACL fixture changed bytes, identity or the original"
        );
    }
    let outcome = continue_worker(&mut interrupted)?;
    ensure!(
        !outcome.success,
        "Live apply accepted changed security and moved the original: {outcome:?}"
    );
    ensure!(
        live_file_state(&interrupted.target()?)? == original_expected
            && live_file_state(&interrupted.after()?)? == output_expected,
        "Refused live apply moved or edited the original/prepared object or its security"
    );
    absent(&auxiliary(
        &interrupted.selection,
        &interrupted.plan,
        0,
        "before",
    )?)?;
    let after_history = state_manifest(&interrupted.selection)?;
    ensure!(
        history
            .iter()
            .all(|(path, digest)| after_history.get(path) == Some(digest)),
        "Live security refusal rewrote or removed immutable history"
    );
    let latest = journal_v2::load(
        &interrupted.selection,
        &journal_id(&interrupted.plan),
        &Deadline::new(10_000),
    )?;
    ensure!(
        latest.files[0].stage == "prepared"
            && latest.files[0].output_identity == registered.files[0].output_identity
            && latest.files[0].attempts == registered.files[0].attempts
            && latest.receipt_id.is_none()
            && head(&interrupted.selection, &Deadline::new(10_000))?.is_none(),
        "Live security refusal advanced ownership or abandoned registered output evidence"
    );
    Ok(())
}

#[test]
fn actual_apply_rechecks_live_prepared_output_acl_before_moving_original() -> Result<()> {
    live_acl_refusal(false)
}

#[test]
fn actual_apply_rechecks_live_original_acl_before_moving_original() -> Result<()> {
    live_acl_refusal(true)
}

#[test]
fn actual_pre_identity_recovery_refuses_changed_original_acl_without_retry() -> Result<()> {
    let _control = complete_case("allocated", Termination::Kill, RecoveryMode::Resume, false)?;
    let mut interrupted = new_fixture(SOURCE, 1)?;
    start_worker(&mut interrupted, "allocated", false)?;
    let original = live_file_state(&interrupted.target()?)?;
    let journal = journal_v2::load(
        &interrupted.selection,
        &journal_id(&interrupted.plan),
        &Deadline::new(10_000),
    )?;
    ensure!(
        journal.files[0].stage == "allocating"
            && journal.files[0]
                .attempts
                .last()
                .is_some_and(|attempt| attempt.identity.is_none())
            && observed(&interrupted)?.output_length == Some(0),
        "Fixture did not pause before the first durable output identity"
    );
    let live_history = state_manifest_with_live_lock(&interrupted.selection, true)?;
    fixture_acl::protect_owned_output(&interrupted.target()?, &original.identity)?;
    let changed = live_file_state(&interrupted.target()?)?;
    ensure!(
        changed.identity == original.identity
            && changed.bytes == original.bytes
            && changed.security_hash != original.security_hash,
        "Pre-identity fixture must change only the original object's ACL"
    );
    terminate_worker(&mut interrupted, Termination::Kill)?;
    absent(&interrupted.after()?)?;
    let history = state_manifest(&interrupted.selection)?;
    ensure!(
        history == live_history,
        "Child death changed durable pre-identity history"
    );
    for mode in [RecoveryMode::Resume, RecoveryMode::Rollback] {
        ensure!(
            interrupted.recover(mode).is_err(),
            "Pre-identity recovery adopted the original's changed ACL as a fresh baseline"
        );
        ensure!(
            state_manifest(&interrupted.selection)? == history
                && live_file_state(&interrupted.target()?)? == changed,
            "Pre-identity source ACL refusal changed history, original bytes, identity or security"
        );
        absent(&interrupted.after()?)?;
        absent(&auxiliary(
            &interrupted.selection,
            &interrupted.plan,
            0,
            "before",
        )?)?;
    }
    Ok(())
}

#[test]
fn actual_prepared_recovery_refuses_acl_tampering_before_publication() -> Result<()> {
    // Require the real prepared-resume path to work first. An unavailable
    // recovery backend must not make the later refusal appear to be evidence.
    let control = interrupt("prepared", Termination::Kill)?;
    finish(&control, RecoveryMode::Resume, true)?;

    let interrupted = interrupt("prepared", Termination::Kill)?;
    let d = Deadline::new(30_000);
    let after = interrupted.after()?;
    let prepared =
        io::read_file(&after, MAX_FILE, false, &d)?.context("Actual prepared output is absent")?;
    let original_id = prepared.identity.clone();
    let original_bytes = prepared.bytes.clone();
    let registered_security = io::security_fingerprint(&prepared, &d)?;
    let journal = journal_v2::load(&interrupted.selection, &journal_id(&interrupted.plan), &d)?;
    ensure!(
        journal.files[0].stage == "prepared"
            && journal.files[0]
                .attempts
                .last()
                .and_then(|attempt| attempt.security_hash.as_ref())
                == Some(&registered_security),
        "Prepared fixture does not have durable security evidence"
    );
    drop(prepared);
    fixture_acl::protect_owned_output(&after, &original_id)?;
    let changed = io::read_file(&after, MAX_FILE, false, &d)?
        .context("ACL fixture modification lost its output")?;
    let changed_security = io::security_fingerprint(&changed, &d)?;
    ensure!(
        changed.identity == original_id
            && changed.bytes == original_bytes
            && changed_security != registered_security,
        "ACL fixture must change only security on the exact registered object"
    );
    drop(changed);
    let before = state_manifest(&interrupted.selection)?;
    for mode in [RecoveryMode::Resume, RecoveryMode::Rollback] {
        ensure!(
            interrupted.recover(mode).is_err(),
            "Prepared recovery accepted an object whose ACL no longer matches durable registration"
        );
        let actual = io::read_file(&after, MAX_FILE, false, &d)?
            .context("Refused prepared recovery removed the changed output")?;
        ensure!(
            state_manifest(&interrupted.selection)? == before
                && actual.identity == original_id
                && actual.bytes == original_bytes
                && io::security_fingerprint(&actual, &d)? == changed_security
                && fs::read(interrupted.target()?)? == SOURCE
                && raw_identity(&interrupted.target()?)?.0
                    == interrupted.plan.files[0]
                        .before_identity
                        .clone()
                        .context("Original identity absent")?,
            "Prepared security refusal changed journal, ACL, output or original target"
        );
        absent(&auxiliary(
            &interrupted.selection,
            &interrupted.plan,
            0,
            "before",
        )?)?;
    }
    Ok(())
}

fn persist_constructed_v2_event(
    selection: &Selection,
    journal: &mut journal_v2::Journal,
    d: &Deadline,
) -> Result<()> {
    d.check()?;
    if !journal.id.is_empty() {
        journal.previous_event = Some(journal.id.clone());
        journal.sequence = journal
            .sequence
            .checked_add(1)
            .context("Constructed fixture event sequence overflow")?;
    }
    ensure!(
        journal.sequence < MAX_EVENTS as u32,
        "Constructed fixture event capacity exceeded"
    );
    journal.id = journal_v2::event_id(journal);
    let directory = state(selection).join("journals").join(&journal.journal_id);
    if journal.sequence == 0 {
        io::create_directory(&directory, d)?;
    }
    let path = directory.join(format!("{:08}-{}.json", journal.sequence, journal.id));
    let bytes = crate::json::encode_bounded(journal, MAX_PLAN, d)?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    d.check()
}

#[test]
fn sixteen_root_v2_history_reserves_only_remaining_output_work() -> Result<()> {
    // Constructed strict history, deliberately separate from the real child
    // interruption cases: twelve roots used one abandoned registered attempt
    // before preparing their second. Persist that explicit fixture directly,
    // then prove its complete prefix with one strict load. Repeatedly calling
    // the validating event producer here would turn fixture construction into
    // quadratic history replay unrelated to the capacity admission regression.
    // The remaining four roots are still prepared through the real producer.
    let interrupted = new_fixture(SOURCE, 16)?;
    let d = Deadline::new(120_000);
    let prefix_started = Instant::now();
    let lock = write_state(&interrupted.selection, &d)?;
    persist_plan(&interrupted.selection, &interrupted.plan, &d)?;
    let source_security = (0..interrupted.plan.files.len())
        .map(|index| -> Result<Option<String>> {
            let original =
                io::read_file(&target(&interrupted.selection, index)?, MAX_FILE, false, &d)?;
            ensure!(
                original.is_some() == interrupted.plan.files[index].before_identity.is_some(),
                "Constructed source security lacks its reviewed original"
            );
            original
                .as_ref()
                .map(|file| io::security_fingerprint(file, &d))
                .transpose()
        })
        .collect::<Result<Vec<_>>>()?;
    let mut journal = journal_v2::Journal {
        schema_version: 2,
        id: String::new(),
        journal_id: journal_id(&interrupted.plan),
        plan_id: interrupted.plan.id.clone(),
        sequence: 0,
        previous_event: None,
        phase: "preparing".into(),
        files: interrupted
            .plan
            .files
            .iter()
            .enumerate()
            .map(|(index, _)| journal_v2::Progress {
                stage: "pending".into(),
                output_identity: None,
                source_security_hash: source_security[index].clone(),
                attempts: Vec::new(),
            })
            .collect(),
        receipt_id: None,
        result: None,
    };
    persist_constructed_v2_event(&interrupted.selection, &mut journal, &d)?;
    for index in 0..12 {
        journal.files[index].attempts.push(journal_v2::Attempt {
            ordinal: 0,
            identity: None,
            security_hash: None,
            abandoned: false,
        });
        journal.files[index].stage = "allocating".into();
        persist_constructed_v2_event(&interrupted.selection, &mut journal, &d)?;
        let abandoned_id = format!("{:016x}:{:032x}", 0xfeed_u64, index + 1);
        journal.files[index].attempts[0].identity = Some(abandoned_id);
        journal.files[index].attempts[0].security_hash =
            Some(hash("abandoned constructed fixture security"));
        journal.files[index].stage = "registered".into();
        persist_constructed_v2_event(&interrupted.selection, &mut journal, &d)?;
        journal.files[index].attempts[0].abandoned = true;
        journal.files[index].attempts.push(journal_v2::Attempt {
            ordinal: 1,
            identity: None,
            security_hash: None,
            abandoned: false,
        });
        journal.files[index].stage = "allocating".into();
        persist_constructed_v2_event(&interrupted.selection, &mut journal, &d)?;
        let original = io::read_file(&target(&interrupted.selection, index)?, MAX_FILE, false, &d)?
            .context("Constructed original is absent")?;
        let canonical =
            host_config::apply_patch(&original.bytes, &interrupted.plan.files[index].patch, &d)?;
        let output = io::prepare_file(
            &auxiliary(&interrupted.selection, &interrupted.plan, index, "after")?,
            Some(&original),
            &canonical,
            &d,
        )?;
        journal.files[index].attempts[1].identity = Some(output.identity.clone());
        journal.files[index].attempts[1].security_hash =
            Some(io::security_fingerprint(&output, &d)?);
        journal.files[index].stage = "registered".into();
        persist_constructed_v2_event(&interrupted.selection, &mut journal, &d)?;
        journal.files[index].stage = "populating".into();
        persist_constructed_v2_event(&interrupted.selection, &mut journal, &d)?;
        journal.files[index].output_identity = Some(output.identity.clone());
        journal.files[index].stage = "prepared".into();
        persist_constructed_v2_event(&interrupted.selection, &mut journal, &d)?;
    }
    let durable = journal_v2::load(&interrupted.selection, &journal_id(&interrupted.plan), &d)?;
    ensure!(
        durable.sequence == 72
            && durable.files[..12]
                .iter()
                .all(|file| file.stage == "prepared")
            && durable.files[12..]
                .iter()
                .all(|file| file.stage == "pending"),
        "Capacity fixture did not establish its 73-event validated prefix"
    );
    eprintln!(
        "host-output-capacity prefix_ms={}",
        prefix_started.elapsed().as_millis()
    );
    let preparation_started = Instant::now();
    for index in 12..16 {
        let original = io::read_file(&target(&interrupted.selection, index)?, MAX_FILE, false, &d)?
            .context("Remaining original is absent")?;
        let canonical =
            host_config::apply_patch(&original.bytes, &interrupted.plan.files[index].patch, &d)?;
        journal_v2::prepare_output(
            &interrupted.selection,
            &interrupted.plan,
            &mut journal,
            index,
            Some(&original),
            &canonical,
            &d,
        )
        .context(
            "Valid 73-event/16-root history must reserve only its remaining completion work",
        )?;
    }
    eprintln!(
        "host-output-capacity remaining_prepare_ms={}",
        preparation_started.elapsed().as_millis()
    );
    drop(lock);
    let recovery_started = Instant::now();
    let profile = journal_v2::profile::start();
    let recovered = interrupted.recover(RecoveryMode::Resume);
    drop(profile);
    eprintln!(
        "host-output-capacity profile={}",
        journal_v2::profile::report()
    );
    eprintln!(
        "host-output-capacity recover_ms={} success={}",
        recovery_started.elapsed().as_millis(),
        recovered.is_ok()
    );
    check_finished(&interrupted, RecoveryMode::Resume, false, recovered?)?;
    for (index, file) in interrupted.plan.files.iter().enumerate() {
        let actual = io::read_file(&target(&interrupted.selection, index)?, MAX_FILE, false, &d)?
            .context("Completed multi-root target is absent")?;
        ensure!(
            Some(&actual.hash) == file.after_hash.as_ref() && actual.bytes.starts_with(SOURCE),
            "Remaining-capacity completion did not publish every reviewed target"
        );
    }
    Ok(())
}

#[test]
fn strict_v2_rollback_rejects_an_output_identity_that_was_never_registered() -> Result<()> {
    let interrupted = new_fixture(SOURCE, 1)?;
    let d = Deadline::new(30_000);
    let lock = write_state(&interrupted.selection, &d)?;
    persist_plan(&interrupted.selection, &interrupted.plan, &d)?;
    let original = io::read_file(&interrupted.target()?, MAX_FILE, false, &d)?
        .context("Constructed rollback source is absent")?;
    let source_security_hash = io::security_fingerprint(&original, &d)?;
    drop(original);
    let mut journal = journal_v2::Journal {
        schema_version: 2,
        id: String::new(),
        journal_id: journal_id(&interrupted.plan),
        plan_id: interrupted.plan.id.clone(),
        sequence: 0,
        previous_event: None,
        phase: "preparing".into(),
        files: vec![journal_v2::Progress {
            stage: "pending".into(),
            output_identity: None,
            source_security_hash: Some(source_security_hash),
            attempts: Vec::new(),
        }],
        receipt_id: None,
        result: None,
    };
    journal_v2::append_event(&interrupted.selection, &mut journal, &d)?;
    journal.files[0].stage = "allocating".into();
    journal.files[0].attempts.push(journal_v2::Attempt {
        ordinal: 0,
        identity: None,
        security_hash: None,
        abandoned: false,
    });
    journal_v2::append_event(&interrupted.selection, &mut journal, &d)?;
    let registered = format!("{:016x}:{:032x}", 0xfeed_u64, 1_u8);
    let unregistered = format!("{:016x}:{:032x}", 0xfeed_u64, 2_u8);
    let _ = journal_v2::register_output(
        &interrupted.selection,
        &interrupted.plan,
        &mut journal,
        0,
        &registered,
        &hash("constructed rollback fixture security"),
        &d,
    )?;
    journal.phase = "rolling_back".into();
    journal_v2::append_event(&interrupted.selection, &mut journal, &d)?;
    // Write hostile, self-consistent metadata directly; using append_event for
    // this event would test its producer rather than the persisted reader.
    journal.previous_event = Some(journal.id.clone());
    journal.sequence += 1;
    journal.files[0].stage = "cleaned".into();
    journal.files[0].output_identity = Some(unregistered);
    journal.id = journal_v2::event_id(&journal);
    let path = state(&interrupted.selection)
        .join("journals")
        .join(&journal.journal_id)
        .join(format!("{:08}-{}.json", journal.sequence, journal.id));
    fs::write(path, serde_json::to_vec(&journal)?)?;
    drop(lock);
    let before = state_manifest(&interrupted.selection)?;
    ensure!(
        journal_v2::load(&interrupted.selection, &journal.journal_id, &d).is_err(),
        "Strict v2 rollback admitted an output identity absent from every allocation attempt"
    );
    ensure!(
        load_journal_summary(&interrupted.selection, &journal.journal_id, &d).is_err(),
        "The common journal view bypassed strict v2 ownership validation"
    );
    for mode in [RecoveryMode::Resume, RecoveryMode::Rollback] {
        ensure!(
            interrupted.recover(mode).is_err(),
            "Recovery accepted forged rollback ownership"
        );
        ensure!(
            state_manifest(&interrupted.selection)? == before
                && fs::read(interrupted.target()?)? == SOURCE,
            "Rejected rollback ownership changed original bytes or durable history"
        );
    }
    Ok(())
}

#[test]
fn strict_v2_persisted_history_rejects_changed_or_removed_source_security() -> Result<()> {
    for remove in [false, true] {
        // Start with genuine apply events and its durably prepared object, then
        // forge a later self-consistent row without invoking the event producer.
        let interrupted = interrupt("prepared", Termination::Kill)?;
        let d = Deadline::new(30_000);
        let lock = write_state(&interrupted.selection, &d)?;
        let mut journal =
            journal_v2::load(&interrupted.selection, &journal_id(&interrupted.plan), &d)?;
        ensure!(
            journal.schema_version == 2
                && journal.phase == "preparing"
                && journal.files[0].stage == "prepared"
                && journal.files[0].source_security_hash.is_some(),
            "Source-security fixture lacks its genuine prepared baseline"
        );
        let original_before = live_file_state(&interrupted.target()?)?;
        let output_before = live_file_state(&interrupted.after()?)?;
        ensure!(
            journal.files[0].source_security_hash.as_ref() == Some(&original_before.security_hash),
            "Genuine initial history did not bind the original security fingerprint"
        );
        journal.phase = "rolling_back".into();
        let changed = hash("hostile replacement of immutable source security");
        ensure!(
            journal.files[0].source_security_hash.as_ref() != Some(&changed),
            "Hostile source-security fixture accidentally preserved its baseline"
        );
        journal.files[0].source_security_hash = (!remove).then_some(changed);
        persist_constructed_v2_event(&interrupted.selection, &mut journal, &d)?;
        drop(lock);
        let history_before = state_manifest(&interrupted.selection)?;
        ensure!(
            journal_v2::load(&interrupted.selection, &journal.journal_id, &d).is_err(),
            "Strict v2 history admitted changed or removed immutable source security"
        );
        ensure!(
            load_journal_summary(&interrupted.selection, &journal.journal_id, &d).is_err(),
            "The common journal view bypassed immutable source-security validation"
        );
        for mode in [RecoveryMode::Resume, RecoveryMode::Rollback] {
            ensure!(
                interrupted.recover(mode).is_err(),
                "Recovery accepted a forged immutable source-security baseline"
            );
            ensure!(
                state_manifest(&interrupted.selection)? == history_before
                    && live_file_state(&interrupted.target()?)? == original_before
                    && live_file_state(&interrupted.after()?)? == output_before,
                "Rejected source-security history changed durable history or either owned object"
            );
            absent(&auxiliary(
                &interrupted.selection,
                &interrupted.plan,
                0,
                "before",
            )?)?;
        }
    }
    Ok(())
}

#[path = "host_journal_scope_tests.rs"]
mod journal_scope_tests;
