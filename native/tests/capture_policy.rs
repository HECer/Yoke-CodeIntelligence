use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::Path, process::Command};
use tempfile::TempDir;
use yoke::{
    Engine,
    analysis::AnalyzerEpoch,
    capture::{self, CaptureOptions, Deadline, DerivedCaptureOptionsRecord},
    changes, editor,
    git_workflows::{self, HostGit},
    model::{Snapshot, identity},
    store::Store,
};

const SOURCE: &str = "fn original() {}\n";
const REPLACEMENT: &str = "fn replaced() {}\n";

fn fixture(epoch: AnalyzerEpoch) -> (TempDir, Engine, Snapshot) {
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("main.custom"), SOURCE).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    engine.capture_epoch = epoch;
    engine.capture_options = CaptureOptions {
        language_overrides: BTreeMap::from([("custom".into(), "rust".into())]),
        ..Default::default()
    };
    let snapshot = engine.index(&Deadline::new(30000)).unwrap().0;
    (root, engine, snapshot)
}

fn persist(store: &Store, snapshot: &Snapshot) {
    store
        .db
        .execute(
            "INSERT INTO snapshots(id,value) VALUES(?1,?2)",
            rusqlite::params![snapshot.id, serde_json::to_string(snapshot).unwrap()],
        )
        .unwrap();
}

fn scope_only(store: &Store, origin: &Snapshot) -> Snapshot {
    let mut derived = origin.clone();
    derived.id = "a".repeat(64);
    derived.kind = "derived".into();
    derived.capture_consistency = "host_custom_derived_producer".into();
    derived.files[0].hash = store.blob(REPLACEMENT.as_bytes()).unwrap();
    derived.files[0].size = REPLACEMENT.len();
    persist(store, &derived);
    capture::inherit_options(store, origin, &derived).unwrap();
    derived
}

fn stored_source(store: &Store) -> Vec<(String, String, String)> {
    store.db.prepare(
        "SELECT 'snapshot',id,value FROM snapshots UNION ALL SELECT 'graph',id,value FROM graphs ORDER BY 1,2",
    ).unwrap().query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .unwrap().map(Result::unwrap).collect()
}

fn recapture(root: &Path, store: &Store, source: &Snapshot) -> Snapshot {
    capture::capture_for_snapshot(root, store, source, &Deadline::new(30000)).unwrap()
}

#[test]
fn arbitrary_derived_id_preserves_proven_v9_capture_policy_without_claiming_extraction() {
    let (root, engine, origin) = fixture(AnalyzerEpoch::V9);
    let derived = scope_only(&engine.store, &origin);
    let before = stored_source(&engine.store);
    assert!(
        capture::analyzer_epoch_for_snapshot_with_deadline(
            &engine.store,
            &derived,
            &Deadline::new(10000),
        )
        .is_err(),
        "scope inheritance must not invent an extractor for a custom producer"
    );
    assert_eq!(
        recapture(root.path(), &engine.store, &derived).id,
        origin.id
    );
    assert_eq!(stored_source(&engine.store), before);
    assert_eq!(
        fs::read_to_string(root.path().join("main.custom")).unwrap(),
        SOURCE
    );
}

#[test]
fn canonical_derived_v9_from_disk_v8_retains_v8_source_acquisition_policy() {
    let (root, mut engine, origin) = fixture(AnalyzerEpoch::V8);
    let graph = engine.store.graph(&origin.id).unwrap();
    let plan = changes::prepare_at(
        AnalyzerEpoch::V9, &mut engine.store, &origin, &graph,
        &json!({"edits":[{"kind":"replace","path":"main.custom",
            "expectedHash":origin.files[0].hash,"startByte":0,"endByte":SOURCE.len(),"text":REPLACEMENT}]}),
        &Deadline::new(30000),
    ).unwrap();
    let derived = engine.store.snapshot(&plan.result_snapshot_id).unwrap();
    assert_eq!(
        capture::analyzer_epoch_for_snapshot_with_deadline(
            &engine.store,
            &derived,
            &Deadline::new(10000),
        )
        .unwrap(),
        AnalyzerEpoch::V9
    );
    let before = stored_source(&engine.store);
    assert_eq!(
        recapture(root.path(), &engine.store, &derived).id,
        origin.id
    );
    assert_eq!(stored_source(&engine.store), before);
    assert_eq!(
        fs::read_to_string(root.path().join("main.custom")).unwrap(),
        SOURCE
    );
}

#[test]
fn overlay_v9_from_disk_v8_retains_the_base_capture_policy() {
    let (root, mut engine, origin) = fixture(AnalyzerEpoch::V8);
    let d = Deadline::new(30000);
    let session = editor::open(&mut engine, &d).unwrap();
    editor::update(
        &engine,
        &session.id,
        &json!({"path":"main.custom","version":1,"text":REPLACEMENT}),
        &d,
    )
    .unwrap();
    let overlay = editor::capture_at(&mut engine, &session.id, AnalyzerEpoch::V9, &d).unwrap();
    assert_eq!(
        capture::analyzer_epoch_for_snapshot_with_deadline(&engine.store, &overlay, &d).unwrap(),
        AnalyzerEpoch::V9
    );
    let before = stored_source(&engine.store);
    assert_eq!(
        recapture(root.path(), &engine.store, &overlay).id,
        origin.id
    );
    assert_eq!(stored_source(&engine.store), before);
    assert_eq!(
        fs::read_to_string(root.path().join("main.custom")).unwrap(),
        SOURCE
    );
}

fn git(host: &HostGit, root: &Path, args: &[&str]) -> String {
    let out = Command::new(&host.executable)
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .args([
            "-c",
            "init.templateDir=",
            "-c",
            "core.hooksPath=",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.autocrlf=false",
            "-c",
            "user.name=Capture Fixture",
            "-c",
            "user.email=capture@example.invalid",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().into()
}

#[test]
fn exact_git_capture_receipts_select_their_own_policy_and_never_fall_back_on_corruption() {
    let (root, mut engine, _) = fixture(AnalyzerEpoch::V8);
    let host = HostGit::discover().unwrap();
    git(&host, root.path(), &["init", "-b", "main"]);
    git(&host, root.path(), &["add", "main.custom"]);
    git(&host, root.path(), &["commit", "-m", "source"]);
    let commit = git(&host, root.path(), &["rev-parse", "HEAD"]);
    let d = Deadline::new(30000);
    let origin = engine.index(&d).unwrap().0;
    for epoch in [AnalyzerEpoch::V8, AnalyzerEpoch::V9] {
        let (historical, _) =
            git_workflows::capture_commit_at(epoch, &mut engine, &host, &origin, &commit, &d)
                .unwrap();
        let expected = capture::capture_with_options_at(
            epoch,
            root.path(),
            &engine.store,
            &engine.capture_options,
            &d,
        )
        .unwrap();
        let before = stored_source(&engine.store);
        assert_eq!(
            recapture(root.path(), &engine.store, &historical).id,
            expected.id
        );
        assert_eq!(stored_source(&engine.store), before);
        assert_eq!(
            git_workflows::validated_source_epoch(&engine.store, &historical, &d).unwrap(),
            epoch
        );
        let mut child = historical.clone();
        child.id = identity(&("scope-only-git-descendant", &historical.id));
        child.capture_consistency = "host_custom_derived_producer".into();
        persist(&engine.store, &child);
        capture::inherit_options(&engine.store, &historical, &child).unwrap();
        assert!(
            capture::analyzer_epoch_for_snapshot_with_deadline(&engine.store, &child, &d).is_err()
        );
        assert_eq!(
            recapture(root.path(), &engine.store, &child).id,
            expected.id
        );
        let proof = git_workflows::validate_source(&engine.store, &historical.id, &d).unwrap();
        for key in ["analyzer", "snapshotHash", "commitId"] {
            let mut corrupt = proof.clone();
            corrupt[key] = if key == "analyzer" {
                json!("yoke-tree-sitter-10")
            } else {
                json!("0".repeat(64))
            };
            engine
                .store
                .record(git_workflows::SOURCE_KIND, &historical.id, &corrupt)
                .unwrap();
            assert!(
                capture::capture_for_snapshot(root.path(), &engine.store, &historical, &d).is_err(),
                "invalid Git {key} fell back to the inherited origin policy"
            );
        }
        engine
            .store
            .record(git_workflows::SOURCE_KIND, &historical.id, &proof)
            .unwrap();
    }
    assert_eq!(
        fs::read_to_string(root.path().join("main.custom")).unwrap(),
        SOURCE
    );
}

#[test]
fn foreign_scope_receipts_and_their_native_children_do_not_grant_disk_acquisition() {
    let (root, engine, origin) = fixture(AnalyzerEpoch::V9);
    let mut foreign = origin.clone();
    foreign.id = "f".repeat(64);
    foreign.kind = "federated".into();
    persist(&engine.store, &foreign);
    capture::inherit_options(&engine.store, &origin, &foreign).unwrap();
    let mut child = foreign.clone();
    child.id = "e".repeat(64);
    child.kind = "derived".into();
    persist(&engine.store, &child);
    capture::inherit_options(&engine.store, &foreign, &child).unwrap();
    for source in [&foreign, &child] {
        assert_eq!(
            capture::options_for_snapshot(&engine.store, source).unwrap(),
            engine.capture_options
        );
        assert!(
            capture::capture_for_snapshot(
                root.path(),
                &engine.store,
                source,
                &Deadline::new(30000)
            )
            .is_err()
        );
    }
}

#[test]
fn resealed_scope_proof_cannot_widen_only_dirs_before_following_its_valid_origin() {
    let root = TempDir::new().unwrap();
    fs::create_dir(root.path().join("src")).unwrap();
    fs::write(root.path().join("src/main.rs"), SOURCE).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    engine.capture_options.only_dirs = vec!["src".into()];
    engine.capture_epoch = AnalyzerEpoch::V9;
    let origin = engine.index(&Deadline::new(30000)).unwrap().0;
    let mut widened = origin.clone();
    widened.id = "d".repeat(64);
    widened.kind = "derived".into();
    let mut outside = widened.files[0].clone();
    outside.path = "outside.rs".into();
    widened.files.push(outside);
    persist(&engine.store, &widened);
    engine
        .store
        .record(
            capture::DERIVED_OPTIONS_RECORD_KIND,
            &widened.id,
            &DerivedCaptureOptionsRecord {
                snapshot_id: widened.id.clone(),
                workspace_id: widened.workspace_id.clone(),
                origin_snapshot_id: origin.id.clone(),
                origin_snapshot_hash: identity(&origin),
                derived_snapshot_hash: identity(&widened),
                options_hash: identity(&engine.capture_options),
            },
        )
        .unwrap();
    assert!(
        capture::capture_for_snapshot(root.path(), &engine.store, &widened, &Deadline::new(30000))
            .is_err()
    );
}

#[test]
fn ignored_capture_v9_lineage_preserves_members_and_rejects_resealed_member_changes() {
    let root = TempDir::new().unwrap();
    fs::create_dir(root.path().join("ignored")).unwrap();
    fs::write(root.path().join(".gitignore"), "ignored/\n").unwrap();
    fs::write(root.path().join("ignored/hidden.rs"), SOURCE).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    engine.capture_epoch = AnalyzerEpoch::V9;
    engine.capture_options.include_ignored_dirs = vec!["ignored".into()];
    let origin = engine.index(&Deadline::new(30000)).unwrap().0;
    let mut derived = origin.clone();
    derived.id = "c".repeat(64);
    derived.kind = "derived".into();
    persist(&engine.store, &derived);
    capture::inherit_options(&engine.store, &origin, &derived).unwrap();
    assert_eq!(
        recapture(root.path(), &engine.store, &derived).id,
        origin.id
    );
    let hidden = derived
        .files
        .iter_mut()
        .find(|file| file.path == "ignored/hidden.rs")
        .unwrap();
    hidden.hash = engine.store.blob(REPLACEMENT.as_bytes()).unwrap();
    hidden.size = REPLACEMENT.len();
    engine
        .store
        .db
        .execute(
            "UPDATE snapshots SET value=?1 WHERE id=?2",
            rusqlite::params![serde_json::to_string(&derived).unwrap(), derived.id],
        )
        .unwrap();
    let mut proof: DerivedCaptureOptionsRecord = engine
        .store
        .get_record(capture::DERIVED_OPTIONS_RECORD_KIND, &derived.id)
        .unwrap();
    proof.derived_snapshot_hash = identity(&derived);
    engine
        .store
        .record(capture::DERIVED_OPTIONS_RECORD_KIND, &derived.id, &proof)
        .unwrap();
    assert!(
        capture::capture_for_snapshot(root.path(), &engine.store, &derived, &Deadline::new(30000))
            .is_err()
    );
    assert_eq!(
        fs::read_to_string(root.path().join("ignored/hidden.rs")).unwrap(),
        SOURCE
    );
}

#[test]
fn unproven_or_tampered_capture_lineage_cannot_authorize_disk_recapture() {
    let (root, engine, origin) = fixture(AnalyzerEpoch::V9);
    let derived = scope_only(&engine.store, &origin);
    let proof: Value = engine
        .store
        .get_record(capture::DERIVED_OPTIONS_RECORD_KIND, &derived.id)
        .unwrap();
    for key in [
        "originSnapshotHash",
        "derivedSnapshotHash",
        "optionsHash",
        "originSnapshotId",
    ] {
        let mut corrupt = proof.clone();
        corrupt[key] = json!("0".repeat(64));
        engine
            .store
            .record(capture::DERIVED_OPTIONS_RECORD_KIND, &derived.id, &corrupt)
            .unwrap();
        let before = stored_source(&engine.store);
        assert!(
            capture::capture_for_snapshot(
                root.path(),
                &engine.store,
                &derived,
                &Deadline::new(30000)
            )
            .is_err(),
            "accepted corrupt {key}"
        );
        assert_eq!(stored_source(&engine.store), before);
    }
    engine
        .store
        .record(capture::DERIVED_OPTIONS_RECORD_KIND, &derived.id, &proof)
        .unwrap();
    let mut altered = derived.clone();
    altered.files[0].language = "python".into();
    assert!(
        capture::capture_for_snapshot(root.path(), &engine.store, &altered, &Deadline::new(30000))
            .is_err()
    );
    let mut custom_root = origin.clone();
    custom_root.id = "b".repeat(64);
    custom_root.kind = "derived".into();
    custom_root.files[0].language = "text".into();
    persist(&engine.store, &custom_root);
    assert_eq!(
        capture::options_for_snapshot(&engine.store, &custom_root).unwrap(),
        CaptureOptions::default()
    );
    assert!(
        capture::capture_for_snapshot(
            root.path(),
            &engine.store,
            &custom_root,
            &Deadline::new(30000)
        )
        .is_err()
    );
    let mut cycle = proof.clone();
    cycle["originSnapshotId"] = json!(derived.id);
    cycle["originSnapshotHash"] = json!(identity(&derived));
    engine
        .store
        .record(capture::DERIVED_OPTIONS_RECORD_KIND, &derived.id, &cycle)
        .unwrap();
    assert!(
        capture::capture_for_snapshot(root.path(), &engine.store, &derived, &Deadline::new(30000))
            .is_err()
    );
}

#[test]
fn capture_policy_traversal_is_bounded_and_honors_the_callers_expired_deadline() {
    let (root, engine, origin) = fixture(AnalyzerEpoch::V8);
    let mut parent = origin.clone();
    let options_hash = identity(&engine.capture_options);
    for depth in 1..=16 {
        let mut derived = parent.clone();
        derived.id = identity(&("scope-only-capture-policy", depth));
        derived.kind = "derived".into();
        persist(&engine.store, &derived);
        engine
            .store
            .record(
                capture::DERIVED_OPTIONS_RECORD_KIND,
                &derived.id,
                &DerivedCaptureOptionsRecord {
                    snapshot_id: derived.id.clone(),
                    workspace_id: derived.workspace_id.clone(),
                    origin_snapshot_id: parent.id.clone(),
                    origin_snapshot_hash: identity(&parent),
                    derived_snapshot_hash: identity(&derived),
                    options_hash: options_hash.clone(),
                },
            )
            .unwrap();
        if depth == 15 {
            assert_eq!(
                recapture(root.path(), &engine.store, &derived).id,
                origin.id
            );
        } else if depth == 16 {
            assert!(
                capture::capture_for_snapshot(
                    root.path(),
                    &engine.store,
                    &derived,
                    &Deadline::new(30000)
                )
                .is_err()
            );
        }
        parent = derived;
    }
    let before = stored_source(&engine.store);
    assert!(
        capture::capture_for_snapshot(root.path(), &engine.store, &origin, &Deadline::new(0))
            .is_err()
    );
    assert_eq!(stored_source(&engine.store), before);
}
