//! Controlled JSON transport fixtures; these do not establish neural ASR quality.
use serde_json::{Value, json};
use std::{fs, path::PathBuf};
use tempfile::TempDir;
use yoke::{
    Engine,
    capture::Deadline,
    media,
    media_asr::{self, Host, Limits, ModelFile, Spec},
    model::hash,
    retention,
};

fn wav() -> Vec<u8> {
    let mut out = Vec::new();
    out.extend(b"RIFF");
    out.extend(32036u32.to_le_bytes());
    out.extend(b"WAVEfmt ");
    out.extend(16u32.to_le_bytes());
    out.extend(1u16.to_le_bytes());
    out.extend(1u16.to_le_bytes());
    out.extend(16000u32.to_le_bytes());
    out.extend(32000u32.to_le_bytes());
    out.extend(2u16.to_le_bytes());
    out.extend(16u16.to_le_bytes());
    out.extend(b"data");
    out.extend(32000u32.to_le_bytes());
    out.resize(32044, 0);
    out
}
fn node() -> PathBuf {
    std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|p| p.join(if cfg!(windows) { "node.exe" } else { "node" }))
        .find(|p| p.is_file())
        .map(|p| {
            if cfg!(windows) {
                p
            } else {
                p.canonicalize().unwrap()
            }
        })
        .expect("explicit Node fixture runtime required")
}
fn fixture(mode: &str, limits: Limits) -> (TempDir, TempDir, Engine, String, Spec) {
    let source = TempDir::new().unwrap();
    let host = TempDir::new().unwrap();
    fs::write(source.path().join("spoken-İ-π.wav"), wav()).unwrap();
    let engine = Engine::open(source.path()).unwrap();
    let asset =
        media::inspect(&engine, "spoken-İ-π.wav", &Deadline::new(30000)).unwrap()["assetId"]
            .as_str()
            .unwrap()
            .to_owned();
    let files = [
        "config.json",
        "model.bin",
        "tokenizer.json",
        "vocabulary.txt",
    ]
    .into_iter()
    .map(|name| {
        fs::write(host.path().join(name), b"owned fixture").unwrap();
        ModelFile {
            name: name.into(),
            bytes: 13,
            sha256: hash(b"owned fixture"),
        }
    })
    .collect();
    let code = r#"let s='';process.stdin.on('data',b=>s+=b);process.stdin.on('end',()=>{const r=JSON.parse(s);const fs=require('fs');let v={schema:1,jobId:r.jobId,libraryVersion:r.model.libraryVersion,audioSha256:r.audio.sha256,audioFrames:r.audio.frames,sampleRate:16000,language:r.language,durationSeconds:r.audio.frames/16000,requestedComputeType:'int8',segments:[{start:0.25,end:1.5,text:' π model observation'}]};const mode=MODE;if(mode==='forged')v.audioSha256='0'.repeat(64);if(mode==='duplicate'){process.stdout.write('{"schema":1,'+JSON.stringify(v).slice(1));return;}if(mode==='audio')fs.appendFileSync(r.audio.path,'changed');if(mode==='model')fs.appendFileSync(require('path').join(r.model.directory,'model.bin'),'changed');if(mode==='parallel')v.segments.push({...v.segments[0]});if(mode==='inert')v.segments[0].text=' '+fs.readFileSync(require('path').join(r.model.directory,'tokenizer.json'),'utf8');process.stdout.write(JSON.stringify(v));});"#.replace("MODE", &serde_json::to_string(mode).unwrap());
    let executable = node();
    let spec = Spec {
        executable_sha256: hash(fs::read(&executable).unwrap()),
        executable,
        args: vec!["-e".into(), code, "--".into()],
        model_root: host.path().to_owned(),
        model_files: files,
        library_version: "1.2.1".into(),
        language: "en".into(),
        limits,
    };
    (source, host, engine, asset, spec)
}
fn select(spec: Spec) -> Host {
    Host::select(spec, true, &Deadline::new(30000)).unwrap()
}
fn controls(engine: &Engine) -> Vec<Value> {
    let mut query = engine
        .store
        .db
        .prepare("SELECT value FROM records WHERE kind='native_media_asr_control_v1' ORDER BY id")
        .unwrap();
    query
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .map(|r| serde_json::from_str(&r.unwrap()).unwrap())
        .collect()
}

#[test]
fn immutable_unicode_audio_parallel_observations_and_read_without_runtime() {
    let (source, host, engine, asset, spec) = fixture("parallel", Limits::default());
    let selected = select(spec);
    fs::remove_file(source.path().join("spoken-İ-π.wav")).unwrap();
    let result =
        media_asr::transcribe(&engine.store, &asset, &selected, &Deadline::new(30000)).unwrap();
    let id = result["jobId"].as_str().unwrap();
    fs::rename(
        host.path().join("model.bin"),
        host.path().join("unavailable.bin"),
    )
    .unwrap();
    let read = media_asr::read(&engine.store, id, &Deadline::new(30000)).unwrap();
    assert_eq!(read["segments"].as_array().unwrap().len(), 2);
    assert_ne!(read["segments"][0]["id"], read["segments"][1]["id"]);
    assert_eq!(read["segments"][0]["reportedEndSeconds"], 1.5);
    assert_eq!(
        read["segments"][0]["capturedSampleEnvelope"]["endByte"],
        32044
    );
    assert_eq!(read["exactSpeechAlignmentEstablished"], false);
    assert_eq!(
        media_asr::status(&engine.store, id, &Deadline::new(30000)).unwrap()["state"],
        "completed"
    );
    let page = media_asr::jobs(&engine.store, &asset, 0, 0, &Deadline::new(30000)).unwrap();
    assert_eq!(page["jobs"], json!([]));
    assert_eq!(page["hasMore"], true);
    assert_eq!(controls(&engine).len(), 1);
    assert_eq!(
        retention::purge_plan(&engine.store, 0, &Deadline::new(30000)).unwrap()["unknownDependencyKinds"],
        json!([])
    );
}

#[test]
fn forged_duplicate_and_changed_inputs_fail_with_no_transcript_and_no_automatic_retry() {
    for mode in ["forged", "duplicate", "audio", "model"] {
        let (_source, _host, engine, asset, spec) = fixture(mode, Limits::default());
        assert!(
            media_asr::transcribe(&engine.store, &asset, &select(spec), &Deadline::new(30000))
                .is_err(),
            "{mode}"
        );
        let rows = controls(&engine);
        assert_eq!(rows.len(), 1, "{mode}");
        assert_eq!(rows[0]["state"], "failed", "{mode}");
        assert_eq!(rows[0]["transcript"], Value::Null);
        let id = rows[0]["requestId"].as_str().unwrap();
        assert!(media_asr::read(&engine.store, id, &Deadline::new(30000)).is_err());
        assert_eq!(
            media_asr::status(&engine.store, id, &Deadline::new(30000)).unwrap()["automaticRetry"],
            false
        );
        assert_eq!(controls(&engine).len(), 1);
    }
}

#[test]
fn explicit_trust_stale_model_and_caller_transaction_refuse_before_admission() {
    let (_source, host, engine, asset, spec) = fixture("normal", Limits::default());
    assert!(Host::select(spec.clone(), false, &Deadline::new(30000)).is_err());
    let selected = select(spec.clone());
    engine.store.db.execute_batch("BEGIN").unwrap();
    assert!(
        media_asr::transcribe(&engine.store, &asset, &selected, &Deadline::new(30000)).is_err()
    );
    engine.store.db.execute_batch("ROLLBACK").unwrap();
    assert!(controls(&engine).is_empty());
    fs::write(host.path().join("model.bin"), b"changed").unwrap();
    assert!(
        media_asr::transcribe(&engine.store, &asset, &selected, &Deadline::new(30000)).is_err()
    );
    assert!(controls(&engine).is_empty());
    let mut bad = serde_json::to_value(spec).unwrap();
    bad["trustWorker"] = json!(true);
    assert!(Spec::from_host_input(&serde_json::to_vec(&bad).unwrap()).is_err());
}

#[test]
fn true_zero_output_budgets_and_job_capacity_are_enforced() {
    for text in [false, true] {
        let limits = if text {
            Limits {
                max_transcript_bytes: 0,
                ..Default::default()
            }
        } else {
            Limits {
                max_segments: 0,
                ..Default::default()
            }
        };
        let (_source, _host, engine, asset, spec) = fixture("normal", limits);
        assert!(
            media_asr::transcribe(&engine.store, &asset, &select(spec), &Deadline::new(30000))
                .is_err()
        );
        assert_eq!(controls(&engine)[0]["state"], "failed");
    }
    let (_source, _host, engine, asset, spec) = fixture(
        "normal",
        Limits {
            max_jobs: 1,
            ..Default::default()
        },
    );
    let selected = select(spec);
    media_asr::transcribe(&engine.store, &asset, &selected, &Deadline::new(30000)).unwrap();
    assert!(
        media_asr::transcribe(&engine.store, &asset, &selected, &Deadline::new(30000)).is_err()
    );
    assert_eq!(controls(&engine).len(), 1);
}

#[test]
fn corrupted_transcript_or_orphaned_journal_blocks_read_and_retention() {
    let (_source, _host, engine, asset, spec) = fixture("normal", Limits::default());
    let result =
        media_asr::transcribe(&engine.store, &asset, &select(spec), &Deadline::new(30000)).unwrap();
    let id = result["jobId"].as_str().unwrap();
    let transcript = result["artifactId"].as_str().unwrap();
    fs::write(engine.store.dir.join("blobs").join(transcript), b"{}").unwrap();
    assert!(media_asr::read(&engine.store, id, &Deadline::new(30000)).is_err());
    assert!(retention::purge_plan(&engine.store, 0, &Deadline::new(30000)).is_err());
    engine
        .store
        .db
        .execute(
            "DELETE FROM records WHERE kind='native_media_asr_request_v1' AND id=?1",
            [id],
        )
        .unwrap();
    assert!(retention::purge_plan(&engine.store, 0, &Deadline::new(30000)).is_err());
}

#[test]
fn inferred_hash_text_never_grants_blob_retention_and_deadline_is_not_extended() {
    let (_source, host, engine, asset, mut spec) = fixture("inert", Limits::default());
    let inert = "c".repeat(64);
    fs::write(host.path().join("tokenizer.json"), &inert).unwrap();
    let file = spec
        .model_files
        .iter_mut()
        .find(|f| f.name == "tokenizer.json")
        .unwrap();
    file.bytes = 64;
    file.sha256 = hash(inert.as_bytes());
    let result =
        media_asr::transcribe(&engine.store, &asset, &select(spec), &Deadline::new(30000)).unwrap();
    assert_eq!(
        retention::purge_plan(&engine.store, 0, &Deadline::new(30000)).unwrap()["unknownDependencyKinds"],
        json!([])
    );
    let short = Deadline::new(1);
    let _ = engine.store.set_deadline(&short);
    std::thread::sleep(std::time::Duration::from_millis(5));
    assert!(
        media_asr::status(
            &engine.store,
            result["jobId"].as_str().unwrap(),
            &Deadline::new(30000)
        )
        .is_err()
    );
    assert!(
        engine
            .store
            .artifact_size(result["artifactId"].as_str().unwrap())
            .is_err()
    );
}
