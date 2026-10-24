use serde_json::json;
use std::{collections::BTreeMap, fs};
use tempfile::TempDir;
use yoke::{
    Engine,
    capture::Deadline,
    decision_records::{DecisionStore, Scope},
    model::Snapshot,
    store::Store,
};

fn fixture() -> (TempDir, Engine, Snapshot) {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("config.rs"),
        "// Gebühren 🌍\npub const MODE: &str = \"old\";\npub const REQUIRED: bool = true;\n",
    )
    .unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let (snapshot, _, _, _) = engine.index(&Deadline::new(10_000)).unwrap();
    (dir, engine, snapshot)
}
fn draft() -> serde_json::Value {
    json!({"key":"storage-policy","kind":"decision","title":"Keep source bytes authoritative","body":"Use immutable snapshots.\n<script>alert(1)</script>\n```\n# malicious heading","author":"Ada","date":"2026-10-15","paths":["config.rs"],"cost":{"currency":"EUR","microunits":1200000,"basis":"authored estimate"},"rules":[{"kind":"follows","path":"config.rs","literal":"REQUIRED: bool = true"},{"kind":"ruled_out","path":"config.rs","literal":"eval("},{"kind":"changed_value","path":"config.rs","before":"\"old\"","after":"\"new\""}]})
}
fn invoke(
    store: &DecisionStore<'_>,
    snapshot: &Snapshot,
    operation: &str,
    args: serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    store.invoke(snapshot, operation, &args, &Deadline::new(10_000))
}

#[test]
fn revisions_require_exact_cas_and_preserve_authored_attribution_and_originals() {
    let (dir, engine, snapshot) = fixture();
    let source = fs::read(dir.path().join("config.rs")).unwrap();
    let store = DecisionStore::new(
        &engine.store,
        &engine.store,
        Scope::Repository,
        &Deadline::new(10_000),
    )
    .unwrap();
    let first = invoke(&store, &snapshot, "write", json!({"draft":draft()})).unwrap();
    assert_eq!(first["revision"]["number"], 1);
    assert_eq!(first["revision"]["draft"]["author"], "Ada");
    assert_eq!(first["revision"]["draft"]["cost"]["microunits"], 1200000);
    assert!(invoke(&store, &snapshot, "write", json!({"draft":draft()})).is_err());
    let mut updated = draft();
    updated["body"] = json!("Updated policy");
    let second = invoke(
        &store,
        &snapshot,
        "write",
        json!({"draft":updated,"expectedRevision":first["revision"]["id"]}),
    )
    .unwrap();
    assert_eq!(second["revision"]["number"], 2);
    assert_eq!(
        second["revision"]["parentRevision"],
        first["revision"]["id"]
    );
    assert!(
        invoke(
            &store,
            &snapshot,
            "write",
            json!({"draft":draft(),"expectedRevision":first["revision"]["id"]})
        )
        .is_err()
    );
    assert_eq!(store.list(&Deadline::new(10_000)).unwrap().len(), 1);
    assert_eq!(fs::read(dir.path().join("config.rs")).unwrap(), source);
}

#[test]
fn bundle_roundtrip_is_deterministic_strict_and_never_overwrites_markdown() {
    let (_dir, engine, snapshot) = fixture();
    let personal_dir = TempDir::new().unwrap();
    let personal = Store::open(personal_dir.path()).unwrap();
    let repository = DecisionStore::new(
        &engine.store,
        &engine.store,
        Scope::Repository,
        &Deadline::new(10_000),
    )
    .unwrap();
    let first = invoke(&repository, &snapshot, "write", json!({"draft":draft()})).unwrap();
    let exports = TempDir::new().unwrap();
    let a = exports.path().join("a");
    let b = exports.path().join("b");
    repository
        .export_bundle(&a, &Deadline::new(10_000))
        .unwrap();
    repository
        .export_bundle(&b, &Deadline::new(10_000))
        .unwrap();
    assert_eq!(
        fs::read(a.join("manifest.json")).unwrap(),
        fs::read(b.join("manifest.json")).unwrap()
    );
    assert_eq!(
        fs::read(a.join("storage-policy.md")).unwrap(),
        fs::read(b.join("storage-policy.md")).unwrap()
    );
    assert!(
        !fs::read_to_string(a.join("storage-policy.md"))
            .unwrap()
            .contains("<script>")
    );
    assert!(
        repository
            .export_bundle(&a, &Deadline::new(10_000))
            .is_err()
    );
    let personal = DecisionStore::new(
        &engine.store,
        &personal,
        Scope::Personal {
            host_id: "windows-test-host".into(),
        },
        &Deadline::new(10_000),
    )
    .unwrap();
    let expected = BTreeMap::from([("storage-policy".into(), None)]);
    personal
        .import_bundle(&a, &expected, &Deadline::new(10_000))
        .unwrap();
    assert_eq!(
        personal
            .get("storage-policy", &Deadline::new(10_000))
            .unwrap()
            .draft
            .author,
        "Ada"
    );
    assert!(
        personal
            .import_bundle(&a, &expected, &Deadline::new(10_000))
            .is_err()
    );
    fs::write(a.join("storage-policy.md"), "tampered prose").unwrap();
    assert!(
        personal
            .import_bundle(
                &a,
                &BTreeMap::from([(
                    "storage-policy".into(),
                    Some(first["revision"]["id"].as_str().unwrap().into())
                )]),
                &Deadline::new(10_000)
            )
            .is_err()
    );
}

#[test]
fn source_bound_literal_checks_show_utf8_byte_evidence_and_truthful_limits() {
    let (dir, mut engine, snapshot) = fixture();
    {
        let store = DecisionStore::new(
            &engine.store,
            &engine.store,
            Scope::Repository,
            &Deadline::new(10_000),
        )
        .unwrap();
        invoke(&store, &snapshot, "write", json!({"draft":draft()})).unwrap();
    }
    fs::write(dir.path().join("config.rs"), "// Gebühren 🌍\npub const MODE: &str = \"new\";\npub const REQUIRED: bool = false;\neval();\n").unwrap();
    let (candidate, _, _, _) = engine.index(&Deadline::new(10_000)).unwrap();
    let store = DecisionStore::new(
        &engine.store,
        &engine.store,
        Scope::Repository,
        &Deadline::new(10_000),
    )
    .unwrap();
    let check = store
        .check("storage-policy", &candidate, &Deadline::new(10_000))
        .unwrap();
    assert_eq!(
        check["classification"],
        "literal_heuristic_not_semantic_proof"
    );
    assert_eq!(check["rules"][0]["outcome"], "violated");
    assert_eq!(check["rules"][1]["outcome"], "violated");
    assert_eq!(check["rules"][2]["outcome"], "literal_transition");
    let evidence = &check["rules"][2]["afterEvidence"][0];
    let start = evidence["startByte"].as_u64().unwrap() as usize;
    let end = evidence["endByte"].as_u64().unwrap() as usize;
    let text = fs::read_to_string(dir.path().join("config.rs")).unwrap();
    assert_eq!(&text[start..end], "\"new\"");
    assert_eq!(evidence["snapshotId"], candidate.id);
    assert_eq!(check["rules"][2]["beforeEvidence"][0]["text"], "\"old\"");
}

#[test]
fn hostile_schema_paths_dates_bounds_and_foreign_snapshots_are_rejected_without_state() {
    let (_dir, engine, snapshot) = fixture();
    let store = DecisionStore::new(
        &engine.store,
        &engine.store,
        Scope::Repository,
        &Deadline::new(10_000),
    )
    .unwrap();
    for key in ["../escape", "CON", "a/b", "a.", "file:stream", "MixedCase"] {
        let mut bad = draft();
        bad["key"] = json!(key);
        assert!(
            invoke(&store, &snapshot, "write", json!({"draft":bad})).is_err(),
            "{key}"
        );
    }
    for path in [
        "../config.rs",
        ".env",
        "C:/config.rs",
        "config.rs:stream",
        "missing.rs",
    ] {
        let mut bad = draft();
        bad["paths"] = json!([path]);
        assert!(
            invoke(&store, &snapshot, "write", json!({"draft":bad})).is_err(),
            "{path}"
        );
    }
    for date in ["2026-02-29", "2026-13-01", "2026-10-15\n", "0000-01-01"] {
        let mut bad = draft();
        bad["date"] = json!(date);
        assert!(invoke(&store, &snapshot, "write", json!({"draft":bad})).is_err());
    }
    let mut bad = draft();
    bad["execute"] = json!("cmd.exe");
    assert!(invoke(&store, &snapshot, "write", json!({"draft":bad})).is_err());
    let mut bad = draft();
    bad["body"] = json!("x".repeat(64001));
    assert!(invoke(&store, &snapshot, "write", json!({"draft":bad})).is_err());
    let mut tampered = snapshot.clone();
    tampered.files[0].size += 1;
    assert!(invoke(&store, &tampered, "write", json!({"draft":draft()})).is_err());
    let (_foreign_dir, _foreign, foreign_snapshot) = fixture();
    assert!(invoke(&store, &foreign_snapshot, "write", json!({"draft":draft()})).is_err());
    assert!(store.list(&Deadline::new(10_000)).unwrap().is_empty());
}

#[test]
fn personal_storage_is_explicit_and_durably_host_pinned() {
    let (_dir, engine, _snapshot) = fixture();
    let personal_dir = TempDir::new().unwrap();
    let personal = Store::open(personal_dir.path()).unwrap();
    assert!(
        DecisionStore::new(
            &engine.store,
            &personal,
            Scope::Repository,
            &Deadline::new(10_000)
        )
        .is_err()
    );
    assert!(
        DecisionStore::new(
            &engine.store,
            &engine.store,
            Scope::Personal {
                host_id: "host-a".into()
            },
            &Deadline::new(10_000)
        )
        .is_err()
    );
    DecisionStore::new(
        &engine.store,
        &personal,
        Scope::Personal {
            host_id: "host-a".into(),
        },
        &Deadline::new(10_000),
    )
    .unwrap();
    assert!(
        DecisionStore::new(
            &engine.store,
            &personal,
            Scope::Personal {
                host_id: "host-b".into()
            },
            &Deadline::new(10_000)
        )
        .is_err()
    );
}

#[test]
fn bundles_refuse_foreign_sources_unknown_entries_duplicate_json_and_invalid_utf8() {
    let (_dir, engine, snapshot) = fixture();
    let store = DecisionStore::new(
        &engine.store,
        &engine.store,
        Scope::Repository,
        &Deadline::new(10_000),
    )
    .unwrap();
    invoke(&store, &snapshot, "write", json!({"draft":draft()})).unwrap();
    let exports = TempDir::new().unwrap();
    let path = exports.path().join("bundle");
    store.export_bundle(&path, &Deadline::new(10_000)).unwrap();
    let (_foreign_dir, foreign, _foreign_snapshot) = fixture();
    let other = DecisionStore::new(
        &foreign.store,
        &foreign.store,
        Scope::Repository,
        &Deadline::new(10_000),
    )
    .unwrap();
    let expected = BTreeMap::from([("storage-policy".into(), None)]);
    assert!(
        other
            .import_bundle(&path, &expected, &Deadline::new(10_000))
            .is_err()
    );
    let manifest = fs::read(path.join("manifest.json")).unwrap();
    fs::write(path.join("extra.md"), "not part of bundle").unwrap();
    assert!(
        store
            .import_bundle(&path, &expected, &Deadline::new(10_000))
            .is_err()
    );
    fs::remove_file(path.join("extra.md")).unwrap();
    fs::write(
        path.join("manifest.json"),
        b"{\"schemaVersion\":1,\"schemaVersion\":1}",
    )
    .unwrap();
    assert!(
        store
            .import_bundle(&path, &expected, &Deadline::new(10_000))
            .is_err()
    );
    fs::write(path.join("manifest.json"), &manifest).unwrap();
    fs::write(path.join("storage-policy.md"), [0xff, 0xfe]).unwrap();
    assert!(
        store
            .import_bundle(&path, &expected, &Deadline::new(10_000))
            .is_err()
    );
    assert!(other.list(&Deadline::new(10_000)).unwrap().is_empty());
}

#[test]
fn import_cas_is_atomic_across_multiple_keys_and_includes_revision_history() {
    let (_dir, engine, snapshot) = fixture();
    let store = DecisionStore::new(
        &engine.store,
        &engine.store,
        Scope::Repository,
        &Deadline::new(10_000),
    )
    .unwrap();
    let a = invoke(&store, &snapshot, "write", json!({"draft":draft()})).unwrap();
    let mut updated = draft();
    updated["body"] = json!("Second revision");
    invoke(
        &store,
        &snapshot,
        "write",
        json!({"draft":updated,"expectedRevision":a["revision"]["id"]}),
    )
    .unwrap();
    let mut second = draft();
    second["key"] = json!("standing-preference");
    second["kind"] = json!("preference");
    invoke(&store, &snapshot, "write", json!({"draft":second})).unwrap();
    let exports = TempDir::new().unwrap();
    let path = exports.path().join("bundle");
    store.export_bundle(&path, &Deadline::new(10_000)).unwrap();
    let personal_dir = TempDir::new().unwrap();
    let personal = Store::open(personal_dir.path()).unwrap();
    let target = DecisionStore::new(
        &engine.store,
        &personal,
        Scope::Personal {
            host_id: "test-host".into(),
        },
        &Deadline::new(10_000),
    )
    .unwrap();
    let expected = BTreeMap::from([
        ("standing-preference".into(), None),
        ("storage-policy".into(), Some("0".repeat(64))),
    ]);
    assert!(
        target
            .import_bundle(&path, &expected, &Deadline::new(10_000))
            .is_err()
    );
    assert!(target.list(&Deadline::new(10_000)).unwrap().is_empty());
    let expected = BTreeMap::from([
        ("standing-preference".into(), None),
        ("storage-policy".into(), None),
    ]);
    target
        .import_bundle(&path, &expected, &Deadline::new(10_000))
        .unwrap();
    assert_eq!(
        target
            .get("storage-policy", &Deadline::new(10_000))
            .unwrap()
            .number,
        2
    );
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(path.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["revisions"].as_array().unwrap().len(), 3);
}

#[test]
fn explicit_import_cannot_roll_back_or_replace_an_existing_revision_lineage() {
    let (_dir, engine, snapshot) = fixture();
    let store = DecisionStore::new(
        &engine.store,
        &engine.store,
        Scope::Repository,
        &Deadline::new(10_000),
    )
    .unwrap();
    let first = invoke(&store, &snapshot, "write", json!({"draft":draft()})).unwrap();
    let exports = TempDir::new().unwrap();
    let original = exports.path().join("original");
    store
        .export_bundle(&original, &Deadline::new(10_000))
        .unwrap();
    let mut updated = draft();
    updated["body"] = json!("New authoritative policy");
    let current = invoke(
        &store,
        &snapshot,
        "write",
        json!({"draft":updated,"expectedRevision":first["revision"]["id"]}),
    )
    .unwrap();
    let current_id = current["revision"]["id"].as_str().unwrap().to_owned();
    assert!(
        store
            .import_bundle(
                &original,
                &BTreeMap::from([("storage-policy".into(), Some(current_id.clone()))]),
                &Deadline::new(10_000)
            )
            .is_err()
    );
    assert_eq!(
        store
            .get("storage-policy", &Deadline::new(10_000))
            .unwrap()
            .id,
        current_id
    );
}

#[test]
fn retention_keeps_all_source_revisions_and_rejects_head_alias_tampering() {
    let (dir, mut engine, baseline) = fixture();
    let first = {
        let store = DecisionStore::new(
            &engine.store,
            &engine.store,
            Scope::Repository,
            &Deadline::new(10_000),
        )
        .unwrap();
        invoke(&store, &baseline, "write", json!({"draft":draft()})).unwrap()
    };
    fs::write(
        dir.path().join("config.rs"),
        "pub const MODE: &str = \"new\";\n",
    )
    .unwrap();
    let (candidate, _, _, _) = engine.index(&Deadline::new(10_000)).unwrap();
    let store = DecisionStore::new(
        &engine.store,
        &engine.store,
        Scope::Repository,
        &Deadline::new(10_000),
    )
    .unwrap();
    invoke(
        &store,
        &candidate,
        "write",
        json!({"draft":draft(),"expectedRevision":first["revision"]["id"]}),
    )
    .unwrap();
    let roots =
        yoke::decision_records::validated_roots(&engine.store, &Deadline::new(10_000)).unwrap();
    assert!(roots.snapshot_ids.contains(&baseline.id));
    assert!(roots.snapshot_ids.contains(&candidate.id));
    assert!(roots.blob_ids.contains(&baseline.files[0].hash));
    assert!(roots.blob_ids.contains(&candidate.files[0].hash));
    engine
        .store
        .db
        .execute(
            "UPDATE records SET value=json_set(value,'$.key','foreign-alias') WHERE kind=?1",
            [yoke::decision_records::HEAD_KIND],
        )
        .unwrap();
    assert!(
        yoke::decision_records::validated_roots(&engine.store, &Deadline::new(10_000)).is_err()
    );
}

#[test]
fn missing_candidate_files_are_inconclusive_and_absence_is_file_scoped() {
    let (dir, mut engine, baseline) = fixture();
    {
        let store = DecisionStore::new(
            &engine.store,
            &engine.store,
            Scope::Repository,
            &Deadline::new(10_000),
        )
        .unwrap();
        invoke(&store, &baseline, "write", json!({"draft":draft()})).unwrap();
        let check = store
            .check("storage-policy", &baseline, &Deadline::new(10_000))
            .unwrap();
        assert_eq!(check["rules"][0]["outcome"], "followed");
        assert_eq!(check["rules"][1]["outcome"], "followed");
        assert_eq!(check["rules"][1]["absenceScope"], "complete_captured_file");
        assert_eq!(check["rules"][2]["outcome"], "unchanged");
    }
    fs::remove_file(dir.path().join("config.rs")).unwrap();
    fs::write(dir.path().join("other.rs"), "fn other() {}\n").unwrap();
    let (candidate, _, _, _) = engine.index(&Deadline::new(10_000)).unwrap();
    let store = DecisionStore::new(
        &engine.store,
        &engine.store,
        Scope::Repository,
        &Deadline::new(10_000),
    )
    .unwrap();
    let check = store
        .check("storage-policy", &candidate, &Deadline::new(10_000))
        .unwrap();
    assert_eq!(check["rules"][0]["outcome"], "inconclusive_missing_file");
    assert_eq!(check["rules"][1]["outcome"], "inconclusive_missing_file");
    assert_eq!(check["rules"][2]["outcome"], "inconclusive_missing_file");
}

#[test]
fn authored_markdown_cannot_create_active_links() {
    let (_dir, engine, snapshot) = fixture();
    let store = DecisionStore::new(
        &engine.store,
        &engine.store,
        Scope::Repository,
        &Deadline::new(10_000),
    )
    .unwrap();
    let mut authored = draft();
    authored["author"] = json!("[click](javascript:alert%281%29)");
    invoke(&store, &snapshot, "write", json!({"draft":authored})).unwrap();
    let exports = TempDir::new().unwrap();
    let path = exports.path().join("bundle");
    store.export_bundle(&path, &Deadline::new(10_000)).unwrap();
    let markdown = fs::read_to_string(path.join("storage-policy.md")).unwrap();
    let active_link = pulldown_cmark::Parser::new(&markdown).any(|event| {
        matches!(
            event,
            pulldown_cmark::Event::Start(pulldown_cmark::Tag::Link { .. })
        )
    });
    assert!(
        !active_link,
        "Authored data created an active Markdown link"
    );
    let personal_dir = TempDir::new().unwrap();
    let personal = Store::open(personal_dir.path()).unwrap();
    let target = DecisionStore::new(
        &engine.store,
        &personal,
        Scope::Personal {
            host_id: "test-host".into(),
        },
        &Deadline::new(10_000),
    )
    .unwrap();
    target
        .import_bundle(
            &path,
            &BTreeMap::from([("storage-policy".into(), None)]),
            &Deadline::new(10_000),
        )
        .unwrap();
    assert_eq!(
        target
            .get("storage-policy", &Deadline::new(10_000))
            .unwrap()
            .draft
            .author,
        "[click](javascript:alert%281%29)"
    );
}

#[test]
fn large_hostile_markdown_can_be_imported_without_losing_original_bytes() {
    let (_dir, engine, snapshot) = fixture();
    let store = DecisionStore::new(
        &engine.store,
        &engine.store,
        Scope::Repository,
        &Deadline::new(10_000),
    )
    .unwrap();
    let mut authored = draft();
    authored["body"] = json!("<".repeat(64_000));
    invoke(&store, &snapshot, "write", json!({"draft":authored})).unwrap();
    let exports = TempDir::new().unwrap();
    let path = exports.path().join("bundle");
    store.export_bundle(&path, &Deadline::new(10_000)).unwrap();
    let personal_dir = TempDir::new().unwrap();
    let personal = Store::open(personal_dir.path()).unwrap();
    let target = DecisionStore::new(
        &engine.store,
        &personal,
        Scope::Personal {
            host_id: "test-host".into(),
        },
        &Deadline::new(10_000),
    )
    .unwrap();
    target
        .import_bundle(
            &path,
            &BTreeMap::from([("storage-policy".into(), None)]),
            &Deadline::new(10_000),
        )
        .unwrap();
    assert_eq!(
        target
            .get("storage-policy", &Deadline::new(10_000))
            .unwrap()
            .draft
            .body,
        "<".repeat(64_000)
    );
}

#[cfg(windows)]
#[test]
fn windows_junction_parents_cannot_redirect_import_or_export() {
    let (_dir, engine, snapshot) = fixture();
    let store = DecisionStore::new(
        &engine.store,
        &engine.store,
        Scope::Repository,
        &Deadline::new(10_000),
    )
    .unwrap();
    invoke(&store, &snapshot, "write", json!({"draft":draft()})).unwrap();
    let exports = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    let junction = exports.path().join("junction");
    fs::write(outside.path().join("sentinel.md"), "preserve me").unwrap();
    let output = std::process::Command::new("cmd.exe")
        .args(["/c", "mklink", "/J"])
        .arg(&junction)
        .arg(outside.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "Windows junction fixture unavailable"
    );
    assert!(
        store
            .export_bundle(&junction.join("fresh"), &Deadline::new(10_000))
            .is_err()
    );
    assert!(
        store
            .import_bundle(&junction, &BTreeMap::new(), &Deadline::new(10_000))
            .is_err()
    );
    assert!(!outside.path().join("fresh").exists());
    assert_eq!(
        fs::read_to_string(outside.path().join("sentinel.md")).unwrap(),
        "preserve me"
    );
    fs::remove_dir(junction).unwrap();
}
