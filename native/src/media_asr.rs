//! Explicit local ASR observations over registered immutable PCM audio.
use crate::{
    capture::Deadline,
    media::{self, Asset},
    model::{hash, identity, valid_id},
    process::{self, CaptureLimits},
    store::Store,
};
use anyhow::{Context, Result, ensure};
use rusqlite::{TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{fs, io::Read, path::Path, process::Command};

#[path = "media_asr_host.rs"]
mod host;
#[path = "media_asr_report.rs"]
mod report;
pub use host::{Host, Limits, ModelFile, Spec, read_host_spec};

pub(crate) const REQUEST: &str = "native_media_asr_request_v1";
pub(crate) const CONTROL: &str = "native_media_asr_control_v1";
const WORKER: &[u8] = include_bytes!("media_asr_worker.py");

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Request {
    schema: u32,
    workspace_id: String,
    asset: Asset,
    nonce: String,
    frames: u64,
    data_offset: u64,
    host_binding: String,
    worker_hash: String,
    model_files: Vec<ModelFile>,
    library_version: String,
    language: String,
    limits: Limits,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Control {
    request_id: String,
    workspace_id: String,
    state: String,
    private_root: Option<String>,
    error: Option<String>,
    raw_output: Option<String>,
    raw_bytes: Option<usize>,
    transcript: Option<String>,
    transcript_bytes: Option<usize>,
}

struct DeadlineScope<'a> {
    store: &'a Store,
    previous: Option<Deadline>,
}
impl Drop for DeadlineScope<'_> {
    fn drop(&mut self) {
        self.store.replace_deadline(self.previous.take());
    }
}
impl<'a> DeadlineScope<'a> {
    fn enter(store: &'a Store, requested: &Deadline) -> Result<(Deadline, Self)> {
        let bounded = store.inherited_deadline(requested)?;
        let scope = Self {
            store,
            previous: store.replace_deadline(Some(bounded.clone())),
        };
        store.check_deadline()?;
        Ok((bounded, scope))
    }
}

fn private_unchanged(path: &Path, expected: &[u8], d: &Deadline) -> Result<()> {
    d.check()?;
    let before = fs::symlink_metadata(path)?;
    ensure!(
        before.is_file()
            && !crate::paths::is_link(&before)
            && before.len() == expected.len() as u64,
        "ASR private input changed"
    );
    let mut file = fs::File::open(path)?;
    let mut cursor = 0usize;
    let mut buffer = [0u8; 64_000];
    loop {
        d.check()?;
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        ensure!(
            count <= expected.len().saturating_sub(cursor)
                && expected[cursor..cursor + count] == buffer[..count],
            "ASR private input changed"
        );
        cursor += count;
    }
    ensure!(
        cursor == expected.len() && fs::symlink_metadata(path)?.len() == before.len(),
        "ASR private input changed"
    );
    Ok(())
}

fn publish_artifact(store: &Store, bytes: &[u8], d: &Deadline) -> Result<String> {
    let id = hash(bytes);
    let existing: Option<Vec<u8>> = {
        use rusqlite::OptionalExtension;
        store.db.query_row("SELECT substr(CAST(value AS BLOB),1,64001) FROM records WHERE kind='artifact' AND id=?1", [&id], |row| row.get(0)).optional()?
    };
    let registration = json!({"artifactId":id,"totalBytes":bytes.len()});
    if let Some(existing) = existing {
        let parsed = crate::json::parse(&existing, 64_000)
            .map_err(|_| anyhow::anyhow!("ASR artifact registration invalid"))?;
        ensure!(
            parsed == registration && store.read_blob_exact(&id, bytes.len(), 4_000_000)? == bytes,
            "ASR artifact collision or changed registration"
        );
    } else {
        ensure!(store.blob(bytes)? == id, "ASR artifact hash mismatch");
        store.db.execute(
            "INSERT INTO records VALUES('artifact',?1,?2)",
            params![id, serde_json::to_string(&registration)?],
        )?;
    }
    d.check()?;
    Ok(id)
}

fn audio(bytes: &[u8], d: &Deadline) -> Result<(u64, u64)> {
    let metadata = media::wav(bytes, d)?;
    ensure!(
        metadata["encoding"] == "pcm"
            && metadata["sampleRate"] == 16_000
            && metadata["channels"] == 1
            && metadata["bitsPerSample"] == 16,
        "ASR supports captured mono16kHz PCM16 WAV only"
    );
    let frames = metadata["frames"]
        .as_u64()
        .context("ASR audio frame count missing")?;
    ensure!(
        (1..=9_600_000).contains(&frames),
        "ASR audio empty or over capacity"
    );
    let mut cursor = 12usize;
    let mut data = None;
    while cursor < bytes.len() {
        d.check()?;
        let length = u32::from_le_bytes(bytes[cursor + 4..cursor + 8].try_into()?) as usize;
        if &bytes[cursor..cursor + 4] == b"data" {
            ensure!(
                data.is_none() && length as u64 == frames * 2,
                "ASR requires one complete PCM data chunk"
            );
            data = Some((cursor + 8) as u64);
        }
        cursor += 8 + length + length % 2;
    }
    Ok((frames, data.context("ASR audio data missing")?))
}

/// Existing asset admission without a host runtime, model, mutation or new Store.
pub fn validate_audio(store: &Store, asset: &str, limits: &Limits, d: &Deadline) -> Result<()> {
    let (bounded, _scope) = DeadlineScope::enter(store, d)?;
    let (_, bytes) = media::checked_asset(store, asset, &bounded)?;
    let (frames, _) = audio(&bytes, &bounded)?;
    ensure!(
        limits.max_audio_milliseconds <= 600_000
            && frames * 1000 <= 16_000 * limits.max_audio_milliseconds,
        "ASR selected audio duration over capacity"
    );
    Ok(())
}

fn expected<'a>(id: &'a str, request: &'a Request) -> report::Expected<'a> {
    report::Expected {
        job_id: id,
        source_hash: &request.asset.source_hash,
        frames: request.frames,
        data_offset: request.data_offset,
        library_version: &request.library_version,
        language: &request.language,
        max_segments: request.limits.max_segments,
        max_text_bytes: request.limits.max_transcript_bytes,
    }
}
fn request_id(request: &Request) -> String {
    identity(&(REQUEST, request))
}
fn body(id: &str, request: &Request, checked: &report::Checked) -> Value {
    json!({"schema":1,"kind":"media_asr_transcript","producer":"yoke-native-asr-1","jobId":id,"workspaceId":request.workspace_id,"assetId":request.asset.id,"path":request.asset.path,"sourceHash":request.asset.source_hash,"blobId":request.asset.blob_id,"audioFrames":request.frames,"sampleRate":16000,"durationSeconds":request.frames as f64/16000.0,"hostBinding":request.host_binding,"workerSha256":request.worker_hash,"modelFiles":request.model_files,"libraryVersion":checked.raw.library_version,"language":checked.raw.language,"segments":checked.segments,"textBytes":checked.text_bytes,"nonFullNominalOverlaps":checked.uncertain_temporal_segments,"origin":"selected_local_asr_model_observations","semanticCompletenessEstablished":false,"exactSpeechAlignmentEstablished":false,"sourceApplicationPerformed":false,"localTranscriptionAttemptsAuthorized":1})
}
fn reference(id: &str, request: &Request, control: &Control) -> Value {
    json!({"jobId":id,"assetId":request.asset.id,"sourceHash":request.asset.source_hash,"state":control.state,"artifactId":control.transcript,"error":control.error,"privateCleanupRequired":control.state=="cleanup_unproven","automaticRetry":false,"sourceApplicationPerformed":false,"semanticCompletenessEstablished":false,"localTranscriptionAttemptsAuthorized":1})
}
fn read_record<T: serde::de::DeserializeOwned>(
    store: &Store,
    kind: &str,
    id: &str,
    d: &Deadline,
) -> Result<T> {
    ensure!(valid_id(id), "ASR job ID invalid");
    d.check()?;
    let raw: Vec<u8> = store.db.query_row(
        "SELECT substr(CAST(value AS BLOB),1,64001) FROM records WHERE kind=?1 AND id=?2",
        params![kind, id],
        |row| row.get(0),
    )?;
    (|| crate::json::decode(crate::json::parse(&raw, 64_000)?))()
        .map_err(|_: anyhow::Error| anyhow::anyhow!("ASR journal protocol invalid"))
}
fn load(store: &Store, id: &str, d: &Deadline) -> Result<(Request, Control)> {
    store.set_deadline(d)?;
    let request: Request = read_record(store, REQUEST, id, d)?;
    let control: Control = read_record(store, CONTROL, id, d)?;
    media::validate_asset(&request.asset)?;
    host::validate_recorded_profile(
        &request.library_version,
        &request.language,
        &request.limits,
        &request.model_files,
    )?;
    ensure!(
        (1..=9_600_000).contains(&request.frames)
            && request.data_offset <= request.asset.size as u64
            && request.frames * 2 <= request.asset.size as u64 - request.data_offset
            && request.frames * 1000 <= 16_000 * request.limits.max_audio_milliseconds,
        "ASR recorded audio capacity invalid"
    );
    ensure!(
        request.schema == 1
            && request_id(&request) == id
            && valid_id(&request.nonce)
            && valid_id(&request.host_binding)
            && valid_id(&request.worker_hash)
            && request.workspace_id == store.workspace_id
            && request.asset.workspace_id == store.workspace_id
            && control.request_id == id
            && control.workspace_id == store.workspace_id,
        "ASR journal binding mismatch"
    );
    let no_output = control.raw_output.is_none()
        && control.raw_bytes.is_none()
        && control.transcript.is_none()
        && control.transcript_bytes.is_none();
    match control.state.as_str() {
        "completed" => ensure!(
            control.raw_output.as_ref().is_some_and(|id| valid_id(id))
                && control.transcript.as_ref().is_some_and(|id| valid_id(id))
                && control
                    .raw_bytes
                    .is_some_and(|n| (1..=2_000_000).contains(&n))
                && control
                    .transcript_bytes
                    .is_some_and(|n| (1..=4_000_000).contains(&n))
                && control.private_root.is_none()
                && control.error.is_none(),
            "ASR completed journal invalid"
        ),
        "admitted" => ensure!(
            no_output && control.private_root.is_some() && control.error.is_none(),
            "ASR admitted journal invalid"
        ),
        "failed" | "cleanup_unproven" => ensure!(
            no_output
                && (control.private_root.is_some() == (control.state == "cleanup_unproven"))
                && control.error.as_deref().is_some_and(|code| [
                    "owned_worker_cleanup_unproven",
                    "worker_transport_failed",
                    "worker_output_or_binding_refused",
                    "private_input_cleanup_unproven"
                ]
                .contains(&code)),
            "ASR failure journal invalid"
        ),
        _ => anyhow::bail!("ASR journal state invalid"),
    }
    Ok((request, control))
}

/// Replays checked stored bytes only; starts no model and selects no host files.
pub fn read(store: &Store, id: &str, d: &Deadline) -> Result<Value> {
    let (bounded, _scope) = DeadlineScope::enter(store, d)?;
    let d = &bounded;
    let (request, control) = load(store, id, d)?;
    ensure!(
        control.state == "completed",
        "ASR job has no completed transcript; automatic retry is unavailable"
    );
    let (asset, bytes) = media::checked_asset(store, &request.asset.id, d)?;
    ensure!(
        asset == request.asset && audio(&bytes, d)? == (request.frames, request.data_offset),
        "ASR recorded audio binding mismatch"
    );
    let raw_id = control
        .raw_output
        .as_ref()
        .context("ASR raw response missing")?;
    let raw_size = control.raw_bytes.context("ASR raw response size missing")?;
    ensure!(
        store.artifact_size(raw_id)? == raw_size,
        "ASR raw response registration mismatch"
    );
    let raw = store.read_blob_exact(raw_id, raw_size, 2_000_000)?;
    let checked = report::check(&raw, &expected(id, &request), d)?;
    let expected = body(id, &request, &checked);
    let result_id = control
        .transcript
        .as_ref()
        .context("ASR transcript missing")?;
    let size = control
        .transcript_bytes
        .context("ASR transcript size missing")?;
    ensure!(
        store.artifact_size(result_id)? == size,
        "ASR transcript registration mismatch"
    );
    let saved = store.read_blob_exact(result_id, size, 4_000_000)?;
    ensure!(
        saved == serde_json::to_vec(&expected)?,
        "ASR transcript replay mismatch"
    );
    Ok(expected)
}
pub fn status(store: &Store, id: &str, d: &Deadline) -> Result<Value> {
    let (bounded, _scope) = DeadlineScope::enter(store, d)?;
    let d = &bounded;
    let (request, control) = load(store, id, d)?;
    if control.state == "completed" {
        read(store, id, d)?;
    }
    let mut result = reference(id, &request, &control);
    if control.state == "cleanup_unproven" {
        result["privateRootForManualHostRecovery"] = json!(control.private_root);
    }
    Ok(result)
}
pub fn jobs(
    store: &Store,
    asset: &str,
    offset: usize,
    limit: usize,
    d: &Deadline,
) -> Result<Value> {
    let (bounded, _scope) = DeadlineScope::enter(store, d)?;
    let d = &bounded;
    ensure!(
        valid_id(asset) && offset <= 1000 && limit <= 100,
        "ASR job page invalid"
    );
    store.set_deadline(d)?;
    let mut statement = store
        .db
        .prepare("SELECT id FROM records WHERE kind=?1 ORDER BY id LIMIT 1001")?;
    let ids = statement
        .query_map([CONTROL], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    ensure!(ids.len() <= 1000, "ASR job registry over capacity");
    let mut rows = Vec::new();
    for id in ids {
        d.check()?;
        let (request, control) = load(store, &id, d)?;
        if request.asset.id == asset {
            rows.push(reference(&id, &request, &control));
        }
    }
    let total = rows.len();
    let selected = rows
        .into_iter()
        .skip(offset)
        .take(limit)
        .collect::<Vec<_>>();
    Ok(
        json!({"assetId":asset,"total":total,"offset":offset,"limit":limit,"hasMore":offset.saturating_add(selected.len())<total,"jobs":selected,"modelStarted":false}),
    )
}

/// Typed retained roots exclude executable/model fingerprints and inferred text.
pub(crate) fn retained_roots(
    store: &Store,
    id: &str,
    d: &Deadline,
) -> Result<std::collections::BTreeSet<String>> {
    let (bounded, _scope) = DeadlineScope::enter(store, d)?;
    let (request, control) = load(store, id, &bounded)?;
    let (asset, bytes) = media::checked_asset(store, &request.asset.id, &bounded)?;
    ensure!(
        asset == request.asset && audio(&bytes, &bounded)? == (request.frames, request.data_offset),
        "ASR retained audio binding mismatch"
    );
    let inspected =
        serde_json::to_vec(&media::technical_inspection_data(&asset, &bytes, &bounded)?)?;
    let inspection_id = hash(&inspected);
    use rusqlite::OptionalExtension;
    let registered: Option<String> = store
        .db
        .query_row(
            "SELECT id FROM records WHERE kind='artifact' AND id=?1",
            [&inspection_id],
            |row| row.get(0),
        )
        .optional()?;
    let mut roots = std::collections::BTreeSet::from([asset.blob_id]);
    if registered.is_some() {
        ensure!(
            store.artifact_size(&inspection_id)? == inspected.len()
                && store.read_blob_exact(&inspection_id, inspected.len(), 4_000_000)? == inspected,
            "ASR retained technical inspection mismatch"
        );
        roots.insert(inspection_id);
    }
    if control.state == "completed" {
        read(store, id, &bounded)?;
        roots.extend(control.raw_output);
        roots.extend(control.transcript);
    }
    Ok(roots)
}

fn failure(store: &Store, id: &str, old: &Control, code: &str, retained: bool) -> Result<()> {
    // Finite metadata/cleanup recovery, never another inference attempt.
    let recovery = Deadline::new(2000);
    let _scope = DeadlineScope {
        store,
        previous: store.replace_deadline(Some(recovery.clone())),
    };
    store.check_deadline()?;
    let mut next = old.clone();
    next.state = if retained {
        "cleanup_unproven"
    } else {
        "failed"
    }
    .into();
    next.error = Some(code.into());
    if !retained {
        next.private_root = None;
    }
    ensure!(
        store.replace_record(CONTROL, id, old, &next)?,
        "ASR failure journal changed; host recovery required"
    );
    Ok(())
}

/// One explicit host-authorized attempt. No automatic package/model acquisition or retry.
pub fn transcribe(store: &Store, asset: &str, host: &Host, d: &Deadline) -> Result<Value> {
    let (bounded, _scope) = DeadlineScope::enter(store, d)?;
    let d = &bounded;
    d.check()?;
    ensure!(
        store.db.is_autocommit(),
        "ASR cannot invoke a worker inside a caller transaction"
    );
    store.set_deadline(d)?;
    let root = store
        .dir
        .parent()
        .context("ASR source root unavailable")?
        .canonicalize()?;
    ensure!(
        !host.executable.starts_with(&root),
        "ASR host executable must be outside analyzed source"
    );
    let (asset, bytes) = media::checked_asset(store, asset, d)?;
    let (frames, data_offset) = audio(&bytes, d)?;
    ensure!(
        frames * 1000 <= 16_000 * host.spec.limits.max_audio_milliseconds,
        "ASR selected audio duration over capacity"
    );
    host.unchanged(d)?;
    let temp_base = std::env::temp_dir().canonicalize()?;
    ensure!(
        !temp_base.starts_with(&root),
        "ASR private data root must be outside analyzed source"
    );
    let directory = tempfile::Builder::new()
        .prefix("yoke-asr-")
        .tempdir_in(temp_base)?;
    let private = directory.path().to_path_buf();
    let audio_path = private.join("audio.wav");
    let worker_path = private.join("worker.py");
    fs::write(&audio_path, &bytes)?;
    fs::write(&worker_path, WORKER)?;
    fs::create_dir(private.join("home"))?;
    fs::create_dir(private.join("cache"))?;
    let mut nonce = [0u8; 32];
    getrandom::fill(&mut nonce)?;
    let request = Request {
        schema: 1,
        workspace_id: store.workspace_id.clone(),
        asset,
        nonce: hex::encode(nonce),
        frames,
        data_offset,
        host_binding: host.binding.clone(),
        worker_hash: hash(WORKER),
        model_files: host.spec.model_files.clone(),
        library_version: host.spec.library_version.clone(),
        language: host.spec.language.clone(),
        limits: host.spec.limits.clone(),
    };
    let id = request_id(&request);
    let control = Control {
        request_id: id.clone(),
        workspace_id: store.workspace_id.clone(),
        state: "admitted".into(),
        private_root: Some(
            private
                .to_str()
                .context("ASR private path must be UTF-8")?
                .into(),
        ),
        error: None,
        raw_output: None,
        raw_bytes: None,
        transcript: None,
        transcript_bytes: None,
    };
    let worker_input = serde_json::to_vec(
        &json!({"schema":1,"jobId":id,"audio":{"path":audio_path,"sha256":request.asset.source_hash,"frames":frames},"model":{"directory":host.spec.model_root,"libraryVersion":request.library_version},"language":request.language,"limits":{"threads":request.limits.threads,"maxSegments":request.limits.max_segments,"maxTranscriptBytes":request.limits.max_transcript_bytes}}),
    )?;
    ensure!(
        worker_input.len() <= 16_384,
        "ASR worker input capacity exceeded"
    );
    {
        let tx = rusqlite::Transaction::new_unchecked(&store.db, TransactionBehavior::Immediate)?;
        let count: usize = tx.query_row(
            "SELECT count(*) FROM records WHERE kind=?1",
            [REQUEST],
            |row| row.get(0),
        )?;
        ensure!(
            count < request.limits.max_jobs,
            "ASR job registry capacity reached"
        );
        tx.execute(
            "INSERT INTO records VALUES(?1,?2,?3)",
            params![REQUEST, id, serde_json::to_string(&request)?],
        )?;
        tx.execute(
            "INSERT INTO records VALUES(?1,?2,?3)",
            params![CONTROL, id, serde_json::to_string(&control)?],
        )?;
        d.check()?;
        tx.commit()?;
    }
    let mut command = Command::new(&host.executable);
    command
        .args(&host.spec.args)
        .arg(&worker_path)
        .current_dir(&private)
        .env("HOME", private.join("home"))
        .env("USERPROFILE", private.join("home"))
        .env("HF_HOME", private.join("cache"))
        .env("HF_HUB_OFFLINE", "1")
        .env("HF_HUB_DISABLE_IMPLICIT_TOKEN", "1")
        .env("HF_HUB_DISABLE_TELEMETRY", "1")
        .env("TRANSFORMERS_OFFLINE", "1")
        .env("PYTHONNOUSERSITE", "1");
    let captured = process::run_capture(
        command,
        &worker_input,
        d,
        CaptureLimits {
            stdin_bytes: 16_384,
            stdout_bytes: 2_000_000,
            stderr_bytes: 4096,
        },
    );
    let captured = match captured {
        Ok(value) => value,
        Err(error) => {
            let retained = process::cleanup_unproven(&error);
            if retained {
                let _retained = directory.keep();
                failure(store, &id, &control, "owned_worker_cleanup_unproven", true)?;
            } else {
                let cleanup = directory.close();
                failure(
                    store,
                    &id,
                    &control,
                    "worker_transport_failed",
                    cleanup.is_err(),
                )?;
            }
            return Err(error.context(format!(
                "ASR attempt {id} failed; inspect its recorded state"
            )));
        }
    };
    let verified = (|| -> Result<(Vec<u8>, Value)> {
        ensure!(
            captured.exit_code == Some(0)
                && captured.stdin_complete
                && captured.output_complete
                && !captured.timed_out
                && !captured.output_exceeded
                && !captured.io_error,
            "ASR worker refused or did not complete within bounds"
        );
        private_unchanged(&worker_path, WORKER, d)?;
        private_unchanged(&audio_path, &bytes, d)?;
        host.unchanged(d)?;
        let (again, original) = media::checked_asset(store, &request.asset.id, d)?;
        ensure!(
            again == request.asset && original == bytes,
            "ASR registered input changed"
        );
        let checked = report::check(&captured.stdout, &expected(&id, &request), d)?;
        let data = body(&id, &request, &checked);
        let serialized = serde_json::to_vec(&data)?;
        ensure!(
            serialized.len() <= 4_000_000,
            "ASR transcript artifact over capacity"
        );
        Ok((serialized, data))
    })();
    let (serialized, _) = match verified {
        Ok(value) => value,
        Err(error) => {
            let cleanup = directory.close();
            failure(
                store,
                &id,
                &control,
                "worker_output_or_binding_refused",
                cleanup.is_err(),
            )?;
            return Err(error.context(format!(
                "ASR attempt {id} refused; inspect its recorded state"
            )));
        }
    };
    if let Err(error) = directory.close() {
        failure(store, &id, &control, "private_input_cleanup_unproven", true)?;
        return Err(error.into());
    }
    store.set_deadline(d)?;
    let tx = rusqlite::Transaction::new_unchecked(&store.db, TransactionBehavior::Immediate)?;
    let raw_id = publish_artifact(store, &captured.stdout, d)?;
    let result_id = publish_artifact(store, &serialized, d)?;
    let mut completed = control.clone();
    completed.state = "completed".into();
    completed.private_root = None;
    completed.raw_output = Some(raw_id);
    completed.raw_bytes = Some(captured.stdout.len());
    completed.transcript = Some(result_id);
    completed.transcript_bytes = Some(serialized.len());
    ensure!(
        store.replace_record(CONTROL, &id, &control, &completed)?,
        "ASR journal changed before result publication"
    );
    d.check()?;
    tx.commit()?;
    Ok(reference(&id, &request, &completed))
}
