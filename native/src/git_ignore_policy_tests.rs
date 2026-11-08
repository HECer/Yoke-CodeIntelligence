use super::*;
fn persist(store: &Store, snapshot: &Snapshot) {
    store
        .db
        .execute(
            "INSERT INTO snapshots VALUES(?1,?2)",
            rusqlite::params![snapshot.id, serde_json::to_string(snapshot).unwrap()],
        )
        .unwrap();
}
#[test]
fn archive_retention_and_derived_proofs_preserve_original_ignored_members() {
    let root = tempfile::tempdir().unwrap();
    for (name, body) in [
        (".gitignore", "ignored.rs\n"),
        ("main.rs", "fn original() {}\n"),
        ("ignored.rs", "fn ignored() {}\r\n"),
    ] {
        fs::write(root.path().join(name), body).unwrap();
    }
    let store = Store::open(root.path()).unwrap();
    let deadline = Deadline::new(30000);
    let options = CaptureOptions {
        include_git_ignored: true,
        ..Default::default()
    };
    let base = capture_with_options(root.path(), &store, &options, &deadline).unwrap();
    persist(&store, &base);
    let original_proof = archived_capture_scope(&store, &base, &deadline).unwrap();
    assert_eq!(
        archived_capture_options(&base, &original_proof, &deadline).unwrap(),
        options
    );
    ignored::retention(
        &store,
        &base.id,
        &original_proof["nodes"][0]["proof"]["record"],
    )
    .unwrap();
    let mut derived = base.clone();
    derived.kind = "derived".into();
    let ordinary = derived
        .files
        .iter_mut()
        .find(|file| file.path == "main.rs")
        .unwrap();
    let after = b"fn changed() {}\n";
    ordinary.hash = store.blob(after).unwrap();
    ordinary.size = after.len();
    derived.id = identity(&("git-ignore-derived", &base.id, &derived.files));
    persist(&store, &derived);
    inherit_options(&store, &base, &derived).unwrap();
    let proof = archived_capture_scope(&store, &derived, &deadline).unwrap();
    assert_eq!(
        archived_capture_options(&derived, &proof, &deadline).unwrap(),
        options
    );
    let mut forged = derived.clone();
    let member = forged
        .files
        .iter_mut()
        .find(|file| file.path == "ignored.rs")
        .unwrap();
    let changed = b"fn forged() {}\n";
    member.hash = store.blob(changed).unwrap();
    member.size = changed.len();
    forged.id = identity(&("git-ignore-forged", &base.id, &forged.files));
    persist(&store, &forged);
    let before = store.db.total_changes();
    let error = inherit_options(&store, &base, &forged).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("Read-only ignored source changed"),
        "{error}"
    );
    assert_eq!(store.db.total_changes(), before);
    let mut forged_proof = proof.clone();
    forged_proof["nodes"][0]["snapshot"] = serde_json::to_value(&forged).unwrap();
    forged_proof["nodes"][0]["proof"]["record"]["snapshotId"] = serde_json::json!(forged.id);
    forged_proof["nodes"][0]["proof"]["record"]["derivedSnapshotHash"] =
        serde_json::json!(identity(&forged));
    drop(store);
    validate_archived_capture_scope(&derived, &proof, &deadline).unwrap();
    let error = validate_archived_capture_scope(&forged, &forged_proof, &deadline).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("Read-only ignored source changed"),
        "{error}"
    );
    let mut changed_proof = original_proof;
    changed_proof["nodes"][0]["proof"]["record"]["options"]["includeGitIgnored"] =
        serde_json::json!(false);
    assert!(validate_archived_capture_scope(&base, &changed_proof, &deadline).is_err());
}
