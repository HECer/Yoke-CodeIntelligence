//! Typed self-contained Meaning-View collections and legacy preservation.
#[path = "support/meaning_archive_fixture.rs"]
mod support;
use serde_json::{Value, json};
use std::fs;
use support::{d, fixture, registry_state, remove_source_workspace, store_state};
use tempfile::TempDir;
use yoke::{
    analysis::AnalyzerEpoch,
    graph_collections::Registry,
    graph_view::{self, BuildOptions},
    graph_view_archive::{self as archive, Limits},
};

fn checked(f: &support::Fixture) -> archive::CheckedMeaningArchive {
    let output = archive::prepare(&f.engine.store, &f.view, Limits::default(), &d()).unwrap();
    archive::parse(output.bytes(), Limits::default(), &d()).unwrap()
}

#[test]
fn typed_registration_reopens_and_replays_after_the_original_store_tree_and_worker_are_gone() {
    // Break caught: Registry copies only data.snapshot.files, retains a Store handle, or loses typed provenance.
    let f = fixture(AnalyzerEpoch::V9);
    let archive = checked(&f);
    let original_view = serde_json::to_value(&f.view).unwrap();
    let original_archive_id = archive.archive_id().to_owned();
    let directory = TempDir::new().unwrap();
    let registry = Registry::open(directory.path()).unwrap();
    let baseline = store_state(&f);
    let registration = registry
        .register_meaning_archive_cas("typed", &archive, None, &d())
        .unwrap();
    let alias_registration = registry
        .register_meaning_archive_cas("typed_alias", &archive, None, &d())
        .unwrap();
    assert_ne!(
        registration, alias_registration,
        "The registration identity includes its host name"
    );
    assert_eq!(store_state(&f), baseline);
    let state = registry_state(directory.path());
    for (name, returned) in [
        ("typed", &registration),
        ("typed_alias", &alias_registration),
    ] {
        let row = state["registrations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row[0] == name)
            .unwrap();
        let persisted_id = row[1].as_str().unwrap();
        let raw_payload = row[2].as_str().unwrap();
        assert_eq!(returned.as_str(), persisted_id);
        assert_eq!(
            persisted_id,
            yoke::model::hash(raw_payload.as_bytes()),
            "Registration ID must be identity of exact compact persisted payload, not a view/archive hash"
        );
        assert_eq!(
            serde_json::from_str::<Value>(raw_payload).unwrap()["name"],
            name
        );
    }
    let payload: Value =
        serde_json::from_str(state["registrations"][0][2].as_str().unwrap()).unwrap();
    assert_eq!(payload["archive"]["view"], original_view);
    assert!(payload.get("sourceAuthority").is_none());
    assert!(
        payload["graph"]["entities"]
            .as_array()
            .unwrap()
            .iter()
            .all(|n| !n["kind"].as_str().unwrap().starts_with("meaning_"))
    );
    let calls = fs::read(&f.marker).unwrap();
    let marker = f.marker.clone();
    let _retained_fixture = remove_source_workspace(f);
    drop(registry);
    let before = fs::read(directory.path().join("yoke-collection.sqlite")).unwrap();
    let readonly = Registry::open_existing(directory.path()).unwrap();
    let replay = readonly
        .load_meaning_archive("typed", Limits::default(), &d())
        .unwrap();
    assert_eq!(replay.archive_id(), original_archive_id);
    assert_eq!(serde_json::to_value(replay.view()).unwrap(), original_view);
    assert_eq!(registry_state(directory.path()), state);
    drop(readonly);
    assert_eq!(
        fs::read(directory.path().join("yoke-collection.sqlite")).unwrap(),
        before
    );
    assert_eq!(fs::read(marker).unwrap(), calls);
    assert_eq!(registration, state["registrations"][0][1].as_str().unwrap());
}

#[test]
fn typed_collection_cas_and_invalid_name_refusals_leave_no_registration_or_source_blobs() {
    // Break caught: source blobs escape the registration transaction on stale/malformed admission.
    let f = fixture(AnalyzerEpoch::V8);
    let archive = checked(&f);
    let directory = TempDir::new().unwrap();
    let registry = Registry::open(directory.path()).unwrap();
    let empty = registry_state(directory.path());
    for (name, expected) in [
        ("../outside", None),
        ("typed", Some("bad-id")),
        (
            "typed",
            Some("0000000000000000000000000000000000000000000000000000000000000000"),
        ),
    ] {
        assert!(
            registry
                .register_meaning_archive_cas(name, &archive, expected, &d())
                .is_err()
        );
        assert_eq!(registry_state(directory.path()), empty);
    }
    let current = registry
        .register_meaning_archive_cas("typed", &archive, None, &d())
        .unwrap();
    let occupied = registry_state(directory.path());
    assert!(
        registry
            .register_meaning_archive_cas("typed", &archive, None, &d())
            .is_err()
    );
    assert_eq!(registry_state(directory.path()), occupied);
    assert!(
        registry
            .register_meaning_archive_cas("typed", &archive, Some(&"f".repeat(64)), &d())
            .is_err()
    );
    assert_eq!(registry_state(directory.path()), occupied);
    assert_eq!(
        registry
            .register_meaning_archive_cas("typed", &archive, Some(&current), &d())
            .unwrap(),
        current
    );
}

#[test]
fn source_only_collection_entry_points_never_return_a_partial_typed_registration() {
    // Break caught: generic join/query/read silently drops the Meaning sidecar after typed registration.
    let f = fixture(AnalyzerEpoch::V9);
    let archive = checked(&f);
    let directory = TempDir::new().unwrap();
    let registry = Registry::open(directory.path()).unwrap();
    assert!(
        registry
            .register_view_cas("legacy", &f.engine.store, &f.view, None, &d())
            .is_err()
    );
    registry
        .register_meaning_archive_cas("typed", &archive, None, &d())
        .unwrap();
    let before = registry_state(directory.path());
    assert!(registry.join(&["typed".into()], &d()).is_err());
    assert!(
        registry
            .query(
                &["typed".into()],
                &json!({"operation":"find","query":"Storage"}),
                &d()
            )
            .is_err()
    );
    assert!(registry.read_source("typed", "a.rs", 0..1, &d()).is_err());
    assert_eq!(registry_state(directory.path()), before);
}

#[test]
fn adding_a_typed_archive_preserves_released_source_registration_payload_ids_and_queries() {
    // Break caught: optional typed metadata serializes into old rows or alters the source-only projection.
    let f = fixture(AnalyzerEpoch::V8);
    let legacy = graph_view::build(
        &f.engine.store,
        &f.snapshot,
        &f.graph,
        &BuildOptions::default(),
        &d(),
    )
    .unwrap();
    assert!(
        serde_json::to_value(&legacy)
            .unwrap()
            .get("meaning")
            .is_none()
    );
    let directory = TempDir::new().unwrap();
    let registry = Registry::open(directory.path()).unwrap();
    let legacy_id = registry
        .register_view_cas("legacy", &f.engine.store, &legacy, None, &d())
        .unwrap();
    let original_row = registry_state(directory.path())["registrations"][0].clone();
    assert_eq!(
        legacy_id,
        yoke::model::hash(original_row[2].as_str().unwrap().as_bytes())
    );
    let before_query = registry
        .query(
            &["legacy".into()],
            &json!({"operation":"find","query":"alpha"}),
            &d(),
        )
        .unwrap();
    let payload: Value = serde_json::from_str(original_row[2].as_str().unwrap()).unwrap();
    assert!(payload["archive"].get("meaningArchive").is_none());
    assert!(payload["archive"]["view"].get("meaning").is_none());
    let archive = checked(&f);
    registry
        .register_meaning_archive_cas("typed", &archive, None, &d())
        .unwrap();
    assert_eq!(
        registry_state(directory.path())["registrations"][0],
        original_row
    );
    assert_eq!(original_row[1], legacy_id);
    assert_eq!(
        registry
            .query(
                &["legacy".into()],
                &json!({"operation":"find","query":"alpha"}),
                &d()
            )
            .unwrap(),
        before_query
    );
    assert!(
        registry
            .join(&["legacy".into(), "typed".into()], &d())
            .is_err()
    );
}

#[test]
fn typed_collection_load_rejects_deleted_historical_source_without_repairing_the_database() {
    // Break caught: loader consults only current manifest or skips raw historical replay.
    let f = fixture(AnalyzerEpoch::V8);
    let archive = checked(&f);
    let directory = TempDir::new().unwrap();
    let registry = Registry::open(directory.path()).unwrap();
    registry
        .register_meaning_archive_cas("typed", &archive, None, &d())
        .unwrap();
    drop(registry);
    let db = rusqlite::Connection::open(directory.path().join("yoke-collection.sqlite")).unwrap();
    assert_eq!(
        db.execute(
            "DELETE FROM source_blobs WHERE hash=?1",
            [yoke::model::hash(support::HISTORY_OLD)]
        )
        .unwrap(),
        1
    );
    drop(db);
    let before = registry_state(directory.path());
    let registry = Registry::open_existing(directory.path()).unwrap();
    assert!(
        registry
            .load_meaning_archive("typed", Limits::default(), &d())
            .is_err()
    );
    assert_eq!(registry_state(directory.path()), before);
}

#[test]
fn collection_metadata_reports_the_complete_typed_layer_without_quotes_or_false_accounting() {
    // Break caught: source-only counts masquerade as complete typed metadata, or unknown billing becomes zero.
    let f = fixture(AnalyzerEpoch::V9);
    let archive = checked(&f);
    let directory = TempDir::new().unwrap();
    let registry = Registry::open(directory.path()).unwrap();
    registry
        .register_meaning_archive_cas("typed", &archive, None, &d())
        .unwrap();
    let before = registry_state(directory.path());
    let metadata = registry.describe("typed", &d()).unwrap();
    assert_eq!(metadata["kind"], "typed_meaning_view");
    assert_eq!(metadata["viewId"], f.view.id);
    assert_eq!(metadata["workspaceId"], f.snapshot.workspace_id);
    assert_eq!(metadata["snapshotId"], f.snapshot.id);
    assert_eq!(metadata["archivedScopeCount"], 2);
    assert_eq!(metadata["archivedFileEntries"], 8);
    assert_eq!(
        metadata["sourceBytes"],
        support::A.len()
            + support::B.len()
            + support::HISTORY_OLD.len()
            + support::HISTORY_NEW.len()
            + "# Archive\n`a.rs::alpha`\n".len()
    );
    assert_eq!(metadata["meaning"]["selectionCount"], 1);
    assert_eq!(metadata["meaning"]["packetCount"], 3);
    assert_eq!(metadata["meaning"]["conceptCount"], 5);
    assert_eq!(metadata["meaning"]["relationCount"], 3);
    assert_eq!(metadata["meaning"]["hyperedgeCount"], 1);
    assert_eq!(metadata["meaning"]["unknownCostPacketCount"], 3);
    assert_eq!(metadata["meaning"]["unknownUsagePacketCount"], 3);
    assert_eq!(metadata["meaning"]["providerAttestation"], "unverified");
    assert!(metadata["meaning"].get("reportedCostMicros").is_none());
    assert!(metadata.get("sourceObjects").is_none());
    assert!(metadata.get("archive").is_none());
    assert!(
        !serde_json::to_string(&metadata)
            .unwrap()
            .contains("exact café evidence")
    );
    assert_eq!(registry.list(&d()).unwrap()["count"], 1);
    assert_eq!(registry_state(directory.path()), before);
}

#[test]
fn complete_archive_bytes_are_explicit_input_but_protected_foreign_cas_is_never_a_fallback() {
    // Break caught: the new bridge turns a bare foreign CAS reference into readable native source.
    let f = fixture(AnalyzerEpoch::V8);
    let output = archive::prepare(&f.engine.store, &f.view, Limits::default(), &d()).unwrap();
    let (_foreign_root, receiver, foreign) = support::foreign_only_source(support::A);
    let content = yoke::model::hash(support::A);
    assert!(receiver.store.dir.join("blobs").join(&content).is_file());
    assert!(receiver.store.read_artifact(&content).is_err());
    assert!(yoke::query::source(&receiver.store, &foreign, "a.rs").is_err());
    let changes = || {
        receiver
            .store
            .db
            .query_row("SELECT total_changes()", [], |row| row.get::<_, i64>(0))
            .unwrap()
    };
    let before_receiver = changes();
    let before_receiver_cas = support::blob_inventory(&receiver.store);
    assert!(
        archive::prepare(&receiver.store, &f.view, Limits::default(), &d()).is_err(),
        "A published foreign-store view ID is not a grant to receiver CAS"
    );
    // The original archive already supplies these bytes from a separate authorized native scope.
    let explicit = archive::parse(output.bytes(), Limits::default(), &d()).unwrap();
    let directory = TempDir::new().unwrap();
    let registry = Registry::open(directory.path()).unwrap();
    registry
        .register_meaning_archive_cas("explicit", &explicit, None, &d())
        .unwrap();
    let before_registry = registry_state(directory.path());
    let mut missing: Value = serde_json::from_slice(output.bytes()).unwrap();
    missing["sourceObjects"]
        .as_object_mut()
        .unwrap()
        .remove(&content)
        .unwrap();
    assert!(
        archive::parse(
            &serde_json::to_vec(&missing).unwrap(),
            Limits::default(),
            &d()
        )
        .is_err()
    );
    assert_eq!(registry_state(directory.path()), before_registry);
    assert_eq!(changes(), before_receiver);
    assert_eq!(
        support::blob_inventory(&receiver.store),
        before_receiver_cas
    );
    assert!(receiver.store.read_artifact(&content).is_err());
}
