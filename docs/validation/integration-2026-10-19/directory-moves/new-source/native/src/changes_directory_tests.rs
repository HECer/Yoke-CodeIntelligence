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

#[test]
fn rehashed_incomplete_directory_membership_still_fails_before_the_durable_write_intent() {
    let f = fixture(AnalyzerEpoch::V9);
    let mut plan = f.plan.clone();
    plan.directory_move
        .as_mut()
        .unwrap()
        .directories
        .retain(|dir| dir.path != "src/tree/empty/nested");
    plan.id = changes::checked_plan::plan_identity(&plan);
    f.engine.store.record("plan", &plan.id, &plan).unwrap();
    changes::approve(&f.engine.store, &plan.id, false).unwrap();
    f.engine.store.record("verification",&plan.id,&json!({"status":"passed","level":"executed","snapshotId":plan.result_snapshot_id,"fixtureProvenance":"Owned host prerequisite fixture; no compiler execution asserted"})).unwrap();
    assert!(changes::apply(&f.engine.store,f.root.path(),&json!({"planId":plan.id,"idempotencyKey":"rehashed-incomplete-tree","mode":"workspace"}),true,&d()).is_err());
    assert_eq!(f.engine.store.db.query_row("SELECT COUNT(*) FROM records WHERE kind='journal' AND json_extract(value,'$.planId')=?1",[&plan.id],|r|r.get::<_,u64>(0)).unwrap(),0);
    assert!(f.root.path().join("src/tree/empty/nested").is_dir());
    assert!(!f.root.path().join("pkg/tree").exists());
    assert_eq!(
        fs::read(f.root.path().join("src/tree/a.rs")).unwrap(),
        BEFORE.as_bytes()
    );
}

#[test]
fn saved_directory_metadata_rejects_unknown_nested_fields_and_noncanonical_paths() {
    let f = fixture(AnalyzerEpoch::V8);
    let mut value = serde_json::to_value(&f.plan).unwrap();
    value["directoryMove"]["files"][0]["hostAuthority"] = json!(true);
    f.engine.store.record("plan", &f.plan.id, &value).unwrap();
    assert!(changes::load_checked_plan(&f.engine.store, &f.plan.id, &d()).is_err());
    for source in ["../outside", "src/./tree", "src/tree/", "C:/outside"] {
        let mut plan = f.plan.clone();
        plan.directory_move.as_mut().unwrap().source = source.into();
        plan.id = changes::checked_plan::plan_identity(&plan);
        f.engine.store.record("plan", &plan.id, &plan).unwrap();
        assert!(
            changes::approve(&f.engine.store, &plan.id, true).is_err(),
            "{source}"
        );
    }
    assert!(f.root.path().join("src/tree/empty/nested").is_dir());
    assert!(!f.root.path().join("pkg/tree").exists());
}

#[test]
fn complete_directory_file_quota_admits_exactly1000_and_refuses1001_without_provider_or_plan() {
    let mut f = fixture(AnalyzerEpoch::V9);
    for i in 0..998 {
        fs::write(f.root.path().join(format!("src/tree/f{i:04}.rs")), b"").unwrap();
    }
    let (snapshot, _, _, _) = f.engine.index(&d()).unwrap();
    let selected = DirectoryMove::select(
        f.root.path(),
        &f.engine.store,
        &snapshot,
        "src/tree",
        "pkg/tree",
        &d(),
    )
    .unwrap();
    assert_eq!(selected.record().files.len(), 1000);
    fs::write(f.root.path().join("src/tree/overflow.rs"), b"").unwrap();
    let (snapshot, _, _, _) = f.engine.index(&d()).unwrap();
    let error = DirectoryMove::select(
        f.root.path(),
        &f.engine.store,
        &snapshot,
        "src/tree",
        "pkg/tree",
        &d(),
    )
    .err()
    .unwrap();
    assert_eq!(error.to_string(), CAPACITY);
    assert!(!f.root.path().join("pkg/tree").exists());
}

#[test]
fn complete_empty_directory_quota_admits_exactly2000_and_refuses2001_without_a_move() {
    let f = fixture(AnalyzerEpoch::V9);
    for i in 0..1996 {
        fs::create_dir(f.root.path().join(format!("src/tree/d{i:04}"))).unwrap();
    }
    let base = f.engine.store.snapshot(&f.plan.base_snapshot_id).unwrap();
    let selected = DirectoryMove::select(
        f.root.path(),
        &f.engine.store,
        &base,
        "src/tree",
        "pkg/tree",
        &d(),
    )
    .unwrap();
    assert_eq!(selected.record().directories.len(), 2000);
    fs::create_dir(f.root.path().join("src/tree/overflow")).unwrap();
    let error = DirectoryMove::select(
        f.root.path(),
        &f.engine.store,
        &base,
        "src/tree",
        "pkg/tree",
        &d(),
    )
    .err()
    .unwrap();
    assert_eq!(error.to_string(), CAPACITY);
    assert!(!f.root.path().join("pkg/tree").exists());
}

#[test]
fn selected_source_byte_ceiling_counts_each_physical_file_even_when_cas_is_deduplicated() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("source")).unwrap();
    fs::create_dir(root.path().join("pkg")).unwrap();
    let bytes = vec![b'x'; 4_000_000];
    for i in 0..4 {
        fs::write(root.path().join(format!("source/{i}.txt")), &bytes).unwrap();
    }
    let mut engine = Engine::open(root.path()).unwrap();
    let (snapshot, _, _, _) = engine.index(&d()).unwrap();
    assert_eq!(
        snapshot
            .files
            .iter()
            .map(|file| file.hash.as_str())
            .collect::<BTreeSet<_>>()
            .len(),
        1
    );
    let selected = DirectoryMove::select(
        root.path(),
        &engine.store,
        &snapshot,
        "source",
        "pkg/source",
        &d(),
    )
    .unwrap();
    assert_eq!(selected.record().files.len(), 4);
    fs::write(root.path().join("source/one-byte.txt"), b"y").unwrap();
    let (snapshot, _, _, _) = engine.index(&d()).unwrap();
    let error = DirectoryMove::select(
        root.path(),
        &engine.store,
        &snapshot,
        "source",
        "pkg/source",
        &d(),
    )
    .err()
    .unwrap();
    assert_eq!(error.to_string(), CAPACITY);
    assert!(!root.path().join("pkg/source").exists());
    drop(engine);
}
