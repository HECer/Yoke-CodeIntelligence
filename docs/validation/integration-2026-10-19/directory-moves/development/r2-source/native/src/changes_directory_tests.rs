//! Owned filesystem fixtures and explicit host prerequisites, without provider execution.
use super::*;
use crate::{Engine, analysis::AnalyzerEpoch, changes};
use serde_json::json;
fn d() -> Deadline {
    Deadline::new(30_000)
}
const BEFORE: &str = "pub fn original(){ /* café 🦀 */ }\r\n";
const AFTER: &str = "pub fn moved(){ /* café 🦀 */ }\r\n";
struct Fixture {
    engine: Engine,
    plan: Plan,
    root: tempfile::TempDir,
}
fn fixture(epoch: AnalyzerEpoch) -> Fixture {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("src/tree/sub")).unwrap();
    fs::create_dir_all(root.path().join("src/tree/empty/nested")).unwrap();
    fs::create_dir(root.path().join("pkg")).unwrap();
    fs::write(root.path().join("src/tree/a.rs"), BEFORE).unwrap();
    fs::write(
        root.path().join("src/tree/sub/café.rs"),
        "pub fn retained(){}\r\n",
    )
    .unwrap();
    fs::write(
        root.path().join("src/use.rs"),
        "// original folder reference\r\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            root.path().join("src/tree/a.rs"),
            fs::Permissions::from_mode(0o750),
        )
        .unwrap();
    }
    let mut engine = Engine::open(root.path()).unwrap();
    engine.capture_epoch = epoch;
    let (snapshot, graph, _, _) = engine.index(&d()).unwrap();
    let selected = DirectoryMove::select(
        root.path(),
        &engine.store,
        &snapshot,
        "src/tree",
        "pkg/tree",
        &d(),
    )
    .unwrap();
    let mut edits = Vec::new();
    for file in selected.record().files() {
        let old = String::from_utf8(engine.store.read_blob(&file.hash).unwrap()).unwrap();
        edits.push(json!({"kind":"delete","path":file.path,"expectedHash":file.hash}));
        edits.push(json!({"kind":"create","path":selected.record().destination_path(&file.path).unwrap(),"text":if file.path.ends_with("/a.rs"){AFTER}else{&old}}));
    }
    let file = snapshot
        .files
        .iter()
        .find(|file| file.path == "src/use.rs")
        .unwrap();
    edits.push(json!({"kind":"replace","path":file.path,"expectedHash":file.hash,"startByte":0,"endByte":file.size,"text":"// moved folder reference\r\n"}));
    let plan = changes::prepare_directory(
        &mut engine.store,
        &snapshot,
        &graph,
        &json!({"edits":edits}),
        &selected,
        root.path(),
        &d(),
    )
    .unwrap();
    Fixture { root, engine, plan }
}
fn prerequisites(f: &Fixture) {
    changes::approve(&f.engine.store, &f.plan.id, true).unwrap();
    f.engine.store.record("verification",&f.plan.id,&json!({"status":"passed","level":"executed","snapshotId":f.plan.result_snapshot_id,"fixtureProvenance":"Owned host prerequisite fixture; no compiler or provider execution asserted"})).unwrap();
}
fn apply(f: &Fixture, key: &str, deadline: &Deadline) -> Result<changes::Receipt> {
    changes::apply(
        &f.engine.store,
        f.root.path(),
        &json!({"planId":f.plan.id,"idempotencyKey":key,"mode":"workspace"}),
        true,
        deadline,
    )
}
#[test]
fn directory_plan_preserves_empty_dirs_exact_bytes_modes_and_identity_through_apply_and_recovery() {
    for epoch in [AnalyzerEpoch::V8, AnalyzerEpoch::V9] {
        let f = fixture(epoch);
        let record = f.plan.directory_move.as_ref().unwrap();
        let original = directory_binding(f.root.path(), "src/tree", &d()).unwrap();
        assert_eq!(record.files().len(), 2);
        assert!(!f.root.path().join("pkg/tree").exists());
        changes::load_checked_plan(&f.engine.store, &f.plan.id, &d()).unwrap();
        prerequisites(&f);
        let receipt = apply(&f, "owned-directory-roundtrip", &d()).unwrap();
        assert!(!f.root.path().join("src/tree").exists());
        assert!(f.root.path().join("pkg/tree/empty/nested").is_dir());
        assert_eq!(
            fs::read(f.root.path().join("pkg/tree/a.rs")).unwrap(),
            AFTER.as_bytes()
        );
        assert_eq!(
            directory_binding(f.root.path(), "pkg/tree", &d())
                .unwrap()
                .identity,
            original.identity
        );
        record
            .check_after(
                f.root.path(),
                &f.engine.store.snapshot(&f.plan.result_snapshot_id).unwrap(),
                &d(),
            )
            .unwrap();
        changes::recover(&f.engine.store, f.root.path(), &receipt.id, true).unwrap();
        assert!(f.root.path().join("src/tree/empty/nested").is_dir());
        assert!(!f.root.path().join("pkg/tree").exists());
        assert_eq!(
            fs::read(f.root.path().join("src/tree/a.rs")).unwrap(),
            BEFORE.as_bytes()
        );
        assert_eq!(
            fs::read(f.root.path().join("src/use.rs")).unwrap(),
            b"// original folder reference\r\n"
        );
        assert_eq!(
            directory_binding(f.root.path(), "src/tree", &d()).unwrap(),
            original
        );
    }
}
#[test]
fn stale_empty_directory_or_uncaptured_file_refuses_before_any_workspace_change() {
    for mutation in [
        "extra-directory",
        "missing-empty-directory",
        "uncaptured-file",
    ] {
        let f = fixture(AnalyzerEpoch::V9);
        prerequisites(&f);
        match mutation {
            "extra-directory" => fs::create_dir(f.root.path().join("src/tree/added")).unwrap(),
            "missing-empty-directory" => {
                fs::remove_dir(f.root.path().join("src/tree/empty/nested")).unwrap()
            }
            _ => fs::write(
                f.root.path().join("src/tree/.env"),
                "OWNED_FIXTURE=not-a-secret\n",
            )
            .unwrap(),
        }
        assert!(apply(&f, mutation, &d()).is_err(), "{mutation}");
        assert!(!f.root.path().join("pkg/tree").exists());
        assert_eq!(
            fs::read(f.root.path().join("src/tree/a.rs")).unwrap(),
            BEFORE.as_bytes()
        );
    }
}
#[test]
fn recovery_preserves_foreign_directory_members_and_can_retry_after_manual_reconciliation() {
    let f = fixture(AnalyzerEpoch::V8);
    prerequisites(&f);
    let receipt = apply(&f, "foreign-directory-member", &d()).unwrap();
    let foreign = f.root.path().join("pkg/tree/foreign.txt");
    fs::write(&foreign, "owned foreign write\r\n").unwrap();
    assert!(changes::recover(&f.engine.store, f.root.path(), &receipt.id, true).is_err());
    assert_eq!(fs::read(&foreign).unwrap(), b"owned foreign write\r\n");
    assert_eq!(
        fs::read(f.root.path().join("pkg/tree/a.rs")).unwrap(),
        AFTER.as_bytes()
    );
    fs::remove_file(&foreign).unwrap();
    changes::recover(&f.engine.store, f.root.path(), &receipt.id, true).unwrap();
    assert_eq!(
        fs::read(f.root.path().join("src/tree/a.rs")).unwrap(),
        BEFORE.as_bytes()
    );
}
#[test]
fn cancellation_after_actual_rename_keeps_the_durable_intent_and_recovers_unrecorded_paths() {
    use std::sync::{Arc, atomic::AtomicBool};
    struct RestoreHook;
    impl Drop for RestoreHook {
        fn drop(&mut self) {
            CANCEL_AFTER_RENAME.with(|slot| slot.borrow_mut().take());
        }
    }
    let f = fixture(AnalyzerEpoch::V9);
    prerequisites(&f);
    let flag = Arc::new(AtomicBool::new(false));
    let deadline = Deadline::run_bounded(&d(), Arc::clone(&flag), || Ok(d())).unwrap();
    CANCEL_AFTER_RENAME.with(|slot| slot.replace(Some(Arc::clone(&flag))));
    let hook = RestoreHook;
    let error = apply(&f, "cancelled-directory-rename", &deadline)
        .err()
        .expect("Cancellation must fail");
    drop(hook);
    let recovery = error.downcast_ref::<changes::RecoveryRequired>().unwrap();
    let id = recovery.journal_id.clone();
    assert!(!recovery.emergency_persisted);
    assert!(!f.root.path().join("src/tree").exists());
    assert_eq!(
        fs::read(f.root.path().join("pkg/tree/a.rs")).unwrap(),
        BEFORE.as_bytes()
    );
    f.engine.store.clear_deadline();
    let journal: changes::Receipt = f.engine.store.get_record("journal", &id).unwrap();
    assert_eq!(journal.status, "intent");
    assert!(journal.applied_paths.is_empty());
    changes::recover(&f.engine.store, f.root.path(), &id, true).unwrap();
    assert!(f.root.path().join("src/tree/empty/nested").is_dir());
    assert_eq!(
        fs::read(f.root.path().join("src/tree/a.rs")).unwrap(),
        BEFORE.as_bytes()
    );
}
#[test]
fn isolated_result_contains_the_complete_declared_directory_shape() {
    let f = fixture(AnalyzerEpoch::V8);
    let receipt = changes::apply(
        &f.engine.store,
        f.root.path(),
        &json!({"planId":f.plan.id,"idempotencyKey":"isolated-directory-shape","mode":"isolated"}),
        false,
        &d(),
    )
    .unwrap();
    let copied = Path::new(receipt.sandbox_path.as_ref().unwrap());
    assert!(copied.join("pkg/tree/empty/nested").is_dir());
    assert_eq!(
        fs::read(copied.join("pkg/tree/a.rs")).unwrap(),
        AFTER.as_bytes()
    );
    assert!(f.root.path().join("src/tree/empty/nested").is_dir());
    assert!(!f.root.path().join("pkg/tree").exists());
}
