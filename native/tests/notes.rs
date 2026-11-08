use serde_json::{Value, json};
use std::fs;
use tempfile::TempDir;
#[test]
fn global_address_namespace_is_reserved_and_reference_boundaries_are_exact() {
    let (_dir, e, s) = fixture();
    let p = writable();
    assert!(
        call(
            &e,
            &s,
            "write",
            json!({"name":"global/topic","content":"ambiguous"}),
            &p
        )
        .is_err()
    );
    call(&e, &s, "write", json!({"name":"a","content":"Choice"}), &p).unwrap();
    call(
        &e,
        &s,
        "write",
        json!({"name":"ref","content":"notmem:a mem:a [choice](mem:a)"}),
        &p,
    )
    .unwrap();
    call(&e, &s, "rename", json!({"oldName":"a","newName":"b"}), &p).unwrap();
    assert_eq!(
        call(&e, &s, "read", json!({"name":"ref"}), &p).unwrap()["note"]["content"],
        "notmem:a mem:b [choice](mem:b)"
    );
}
use yoke::{
    Engine,
    capture::Deadline,
    model::Snapshot,
    notes::{self, Policy, Scope},
};
fn fixture() -> (TempDir, Engine, Snapshot) {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("main.rs"), "fn alpha() {}\n").unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let (s, _, _, _) = engine.index(&Deadline::new(20000)).unwrap();
    (dir, engine, s)
}
fn call(e: &Engine, s: &Snapshot, op: &str, args: Value, p: &Policy) -> anyhow::Result<Value> {
    notes::invoke(
        &e.store,
        &e.store,
        s,
        Scope::Project,
        op,
        &args,
        p,
        &Deadline::new(20000),
    )
}
fn writable() -> Policy {
    Policy {
        writable: true,
        ..Default::default()
    }
}
#[test]
fn named_note_crud_retains_authored_scope_revisions_and_exact_source() {
    let (dir, e, s) = fixture();
    let original = fs::read(dir.path().join("main.rs")).unwrap();
    let p = writable();
    let a = call(
        &e,
        &s,
        "write",
        json!({"name":"architecture/choice","content":"Use 🌍 memory","paths":["main.rs"]}),
        &p,
    )
    .unwrap();
    assert_eq!(a["note"]["content"], "Use 🌍 memory");
    assert_eq!(a["note"]["snapshotId"], s.id);
    assert_eq!(a["note"]["anchors"][0]["contentHash"], s.files[0].hash);
    let id = a["note"]["id"].clone();
    let b=call(&e,&s,"edit",json!({"name":"architecture/choice","needle":"🌍","replacement":"Rust","expectedCount":1,"expectedRevision":id}),&p).unwrap();
    assert_eq!(b["note"]["content"], "Use Rust memory");
    assert_ne!(a["note"]["id"], b["note"]["id"]);
    assert!(call(&e,&s,"edit",json!({"name":"architecture/choice","needle":"Rust","replacement":"lost","expectedRevision":id}),&p).is_err());
    assert_eq!(
        call(&e, &s, "list", json!({"topic":"architecture"}), &p).unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    call(
        &e,
        &s,
        "delete",
        json!({"name":"architecture/choice","expectedRevision":b["note"]["id"]}),
        &p,
    )
    .unwrap();
    assert!(call(&e, &s, "read", json!({"name":"architecture/choice"}), &p).is_err());
    assert_eq!(
        e.store
            .get_record::<Value>(notes::REVISION_KIND, a["note"]["id"].as_str().unwrap())
            .unwrap()["content"],
        "Use 🌍 memory"
    );
    assert_eq!(fs::read(dir.path().join("main.rs")).unwrap(), original);
}
#[test]
fn rename_rewrites_exact_mem_references_atomically_except_read_only_notes() {
    let (_dir, e, s) = fixture();
    let p = writable();
    for (name, content) in [
        ("topic/a", "Choice"),
        (
            "topic/ref",
            "Read [the choice](mem:topic/a) and mem:topic/a-more.",
        ),
        ("locked/ref", "Read mem:topic/a"),
    ] {
        call(&e, &s, "write", json!({"name":name,"content":content}), &p).unwrap();
    }
    let policy = Policy {
        read_only_patterns: vec!["locked/**".into()],
        ..writable()
    };
    let renamed = call(
        &e,
        &s,
        "rename",
        json!({"oldName":"topic/a","newName":"topic/b"}),
        &policy,
    )
    .unwrap();
    assert_eq!(renamed["referencesUpdated"], 1);
    assert_eq!(
        renamed["readOnlyReferencesUnchanged"],
        json!(["locked/ref"])
    );
    let reference = call(&e, &s, "read", json!({"name":"topic/ref"}), &policy).unwrap();
    assert_eq!(
        reference["note"]["content"],
        "Read [the choice](mem:topic/b) and mem:topic/a-more."
    );
    assert_eq!(
        call(&e, &s, "read", json!({"name":"locked/ref"}), &policy).unwrap()["note"]["content"],
        "Read mem:topic/a"
    );
    assert!(
        call(
            &e,
            &s,
            "write",
            json!({"name":"locked/ref","content":"overwrite"}),
            &policy
        )
        .is_err()
    );
    assert!(
        call(
            &e,
            &s,
            "rename",
            json!({"oldName":"topic/b","newName":"topic/ref"}),
            &policy
        )
        .is_err()
    );
    assert_eq!(
        call(&e, &s, "read", json!({"name":"topic/b"}), &policy).unwrap()["note"]["content"],
        "Choice"
    );
}
#[test]
fn global_storage_is_host_selected_and_missing_or_ignored_notes_do_not_leak() {
    let (_dir, e, s) = fixture();
    let global = TempDir::new().unwrap();
    let global_engine = Engine::open(global.path()).unwrap();
    let p = writable();
    let note = notes::invoke(
        &e.store,
        &global_engine.store,
        &s,
        Scope::Global,
        "write",
        &json!({"name":"preferences/rust","content":"Prefer Rust","paths":["main.rs"]}),
        &p,
        &Deadline::new(20000),
    )
    .unwrap();
    assert_eq!(note["note"]["scope"], "global");
    assert_eq!(note["note"]["sourceWorkspaceId"], s.workspace_id);
    assert!(call(&e, &s, "read", json!({"name":"preferences/rust"}), &p).is_err());
    assert!(
        notes::invoke(
            &e.store,
            &global_engine.store,
            &s,
            Scope::Project,
            "read",
            &json!({"name":"preferences/rust"}),
            &p,
            &Deadline::new(20000)
        )
        .is_err()
    );
    call(
        &e,
        &s,
        "write",
        json!({"name":"private/topic","content":"Authored private note"}),
        &p,
    )
    .unwrap();
    let hidden = Policy {
        ignored_patterns: vec!["private/**".into()],
        ..writable()
    };
    assert!(call(&e, &s, "read", json!({"name":"private/topic"}), &hidden).is_err());
    assert!(
        call(&e, &s, "list", json!({}), &hidden).unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    for name in ["../escape", "C:/escape", ".yoke/state", "topic//a"] {
        assert!(call(&e, &s, "write", json!({"name":name,"content":"x"}), &p).is_err());
    }
    assert!(
        call(
            &e,
            &s,
            "write",
            json!({"name":"x","content":"x","trust":true}),
            &p
        )
        .is_err()
    );
}
#[test]
fn regex_edits_count_guard_and_read_only_policy_do_not_partially_mutate_notes() {
    let (_dir, e, s) = fixture();
    let p = writable();
    call(&e, &s, "write", json!({"name":"a","content":"x1 x2"}), &p).unwrap();
    for spec in [
        json!({"name":"a","needle":"x([0-9])","replacement":"v$1","regex":true,"expectedCount":1}),
        json!({"name":"a","needle":"x","replacement":"v"}),
    ] {
        assert!(call(&e, &s, "edit", spec, &p).is_err());
    }
    assert_eq!(
        call(&e, &s, "read", json!({"name":"a"}), &p).unwrap()["note"]["content"],
        "x1 x2"
    );
    assert!(call(&e, &s, "delete", json!({"name":"a"}), &Policy::default()).is_err());
    let edited=call(&e,&s,"edit",json!({"name":"a","needle":"x([0-9])","replacement":"v$1","regex":true,"allowMultiple":true,"expectedCount":2}),&p).unwrap();
    assert_eq!(edited["note"]["content"], "v1 v2");
}

#[test]
fn edited_note_keeps_historical_anchor_and_reports_changed_source() {
    let (dir, mut e, s) = fixture();
    let p = writable();
    call(
        &e,
        &s,
        "write",
        json!({"name":"choice","content":"Old choice","paths":["main.rs"]}),
        &p,
    )
    .unwrap();
    fs::write(dir.path().join("main.rs"), "fn beta() {}\n").unwrap();
    let (fresh, _, _, _) = e.index(&Deadline::new(20000)).unwrap();
    let edited = call(
        &e,
        &fresh,
        "edit",
        json!({"name":"choice","needle":"Old","replacement":"Reviewed"}),
        &p,
    )
    .unwrap();
    assert_eq!(edited["note"]["anchors"][0]["snapshotId"], s.id);
    assert_eq!(
        edited["note"]["anchors"][0]["sourceWorkspaceId"],
        s.workspace_id
    );
    assert_eq!(edited["note"]["anchors"][0]["contentHash"], s.files[0].hash);
    let read = call(&e, &fresh, "read", json!({"name":"choice"}), &p).unwrap();
    assert_eq!(read["anchorCurrentness"][0]["state"], "changed");
}

#[test]
fn hidden_and_read_only_references_do_not_leak_or_block_renames() {
    let (_dir, e, s) = fixture();
    let p = writable();
    call(&e, &s, "write", json!({"name":"a","content":"Choice"}), &p).unwrap();
    call(
        &e,
        &s,
        "write",
        json!({"name":"hidden/ref","content":"mem:a"}),
        &p,
    )
    .unwrap();
    call(
        &e,
        &s,
        "write",
        json!({"name":"locked/ref","content":"mem:a ".repeat(10000)}),
        &p,
    )
    .unwrap();
    let policy = Policy {
        ignored_patterns: vec!["hidden/**".into()],
        read_only_patterns: vec!["locked/**".into()],
        ..writable()
    };
    let renamed = call(
        &e,
        &s,
        "rename",
        json!({"oldName":"a","newName":"longer-reference-name"}),
        &policy,
    )
    .unwrap();
    assert_eq!(
        renamed["readOnlyReferencesUnchanged"],
        json!(["locked/ref"])
    );
    assert!(
        !serde_json::to_string(&renamed)
            .unwrap()
            .contains("hidden/ref")
    );
}
#[test]
fn regex_gap_growth_fails_before_remaining_budget_underflow_and_keeps_the_revision() {
    let (_dir, e, s) = fixture();
    let p = writable();
    let original = format!("x{}x", "a".repeat(40000));
    let written = call(
        &e,
        &s,
        "write",
        json!({"name":"bounded","content":original}),
        &p,
    )
    .unwrap();
    let revision = written["note"]["id"].clone();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        call(
            &e,
            &s,
            "edit",
            json!({"name":"bounded","needle":"x","replacement":"r".repeat(30000),"regex":true,"allowMultiple":true,"expectedCount":2}),
            &p,
        )
    }));
    assert!(
        result.is_ok(),
        "Bounded note edit panicked instead of rejecting excessive growth"
    );
    assert!(result.unwrap().is_err());
    let read = call(&e, &s, "read", json!({"name":"bounded"}), &p).unwrap();
    assert_eq!(read["note"]["content"], original);
    assert_eq!(read["note"]["id"], revision);
}
#[test]
fn global_note_policy_matches_its_canonical_global_namespace() {
    let (_dir, e, s) = fixture();
    let p = writable();
    notes::invoke(
        &e.store,
        &e.store,
        &s,
        Scope::Global,
        "write",
        &json!({"name":"preferences/rust","content":"choice"}),
        &p,
        &Deadline::new(20000),
    )
    .unwrap();
    let policy = Policy {
        read_only_patterns: vec!["global/**".into()],
        ..writable()
    };
    assert!(
        notes::invoke(
            &e.store,
            &e.store,
            &s,
            Scope::Global,
            "write",
            &json!({"name":"preferences/rust","content":"overwrite"}),
            &policy,
            &Deadline::new(20000)
        )
        .is_err()
    );
    let read = notes::invoke(
        &e.store,
        &e.store,
        &s,
        Scope::Global,
        "read",
        &json!({"name":"preferences/rust"}),
        &policy,
        &Deadline::new(20000),
    )
    .unwrap();
    assert_eq!(read["readOnly"], true);
    assert_eq!(read["note"]["content"], "choice");
    let hidden = Policy {
        ignored_patterns: vec!["global/**".into()],
        ..writable()
    };
    assert!(
        notes::invoke(
            &e.store,
            &e.store,
            &s,
            Scope::Global,
            "read",
            &json!({"name":"preferences/rust"}),
            &hidden,
            &Deadline::new(20000)
        )
        .is_err()
    );
}
#[test]
fn memory_reference_left_boundaries_exclude_larger_uri_schemes_and_identifiers() {
    let (_dir, e, s) = fixture();
    let p = writable();
    call(&e, &s, "write", json!({"name":"a","content":"choice"}), &p).unwrap();
    let original =
        "notmem:a αmem:a other+mem:a other.mem:a other-mem:a mem:a mem:a-more [choice](mem:a)";
    call(
        &e,
        &s,
        "write",
        json!({"name":"reference","content":original}),
        &p,
    )
    .unwrap();
    let renamed = call(&e, &s, "rename", json!({"oldName":"a","newName":"b"}), &p).unwrap();
    assert_eq!(renamed["referencesUpdated"], 2);
    assert_eq!(
        call(&e, &s, "read", json!({"name":"reference"}), &p).unwrap()["note"]["content"],
        "notmem:a αmem:a other+mem:a other.mem:a other-mem:a mem:b mem:a-more [choice](mem:b)"
    );
}
#[test]
fn maximally_encoded_accepted_note_remains_readable_after_commit() {
    let (_dir, e, s) = fixture();
    let p = writable();
    let text = "\"".repeat(63800);
    call(
        &e,
        &s,
        "write",
        json!({"name":"encoded","content":text,"paths":["main.rs"]}),
        &p,
    )
    .unwrap();
    assert_eq!(
        call(&e, &s, "read", json!({"name":"encoded"}), &p).unwrap()["note"]["content"],
        text
    );
}
fn seed_retired_heads(e: &Engine, s: &Snapshot, count: usize, content: &str) {
    let p = writable();
    call(
        e,
        s,
        "write",
        json!({"name":"retired/template","content":content}),
        &p,
    )
    .unwrap();
    let retired = call(e, s, "delete", json!({"name":"retired/template"}), &p).unwrap();
    let id = retired["revisionId"].as_str().unwrap();
    let raw: String = e
        .store
        .db
        .query_row(
            "SELECT value FROM records WHERE kind=?1 AND id=?2",
            rusqlite::params![notes::REVISION_KIND, id],
            |row| row.get(0),
        )
        .unwrap();
    // Restore a bounded historical-state fixture from a real canonical revision.
    // The production reader still validates every cloned record and head hash.
    let transaction = e.store.db.unchecked_transaction().unwrap();
    for i in 0..count - 1 {
        let name = format!("retired/{i}");
        let empty = raw
            .replacen(&format!("\"id\":\"{id}\""), "\"id\":\"\"", 1)
            .replacen(
                "\"name\":\"retired/template\"",
                &format!("\"name\":\"{name}\""),
                1,
            );
        let revision_id = yoke::model::hash(empty.as_bytes());
        let value = empty.replacen("\"id\":\"\"", &format!("\"id\":\"{revision_id}\""), 1);
        transaction
            .execute(
                "INSERT INTO records VALUES(?1,?2,?3)",
                rusqlite::params![notes::REVISION_KIND, revision_id, value],
            )
            .unwrap();
        let head_id = yoke::model::identity(&(
            notes::HEAD_KIND,
            &e.store.workspace_id,
            Scope::Project,
            &name,
        ));
        transaction
            .execute(
                "INSERT INTO records VALUES(?1,?2,?3)",
                rusqlite::params![
                    notes::HEAD_KIND,
                    head_id,
                    json!({"name":name,"scope":"project","revisionId":revision_id}).to_string()
                ],
            )
            .unwrap();
    }
    transaction.commit().unwrap();
}
#[test]
fn retired_heads_remain_bounded_without_consuming_active_note_content_quota() {
    let (_dir, e, s) = fixture();
    let p = writable();
    seed_retired_heads(&e, &s, 501, &"x".repeat(64000));
    call(
        &e,
        &s,
        "write",
        json!({"name":"active","content":"new authored note"}),
        &p,
    )
    .unwrap();
    assert_eq!(
        call(&e, &s, "list", json!({}), &p).unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}
#[test]
fn host_note_policy_defaults_read_only_and_rejects_unknown_authority_fields() {
    let policy: Policy = serde_json::from_str("{}").unwrap();
    assert!(!policy.writable);
    assert!(
        serde_json::from_str::<Policy>(r#"{"writable":true,"allowSourceWrites":true}"#).is_err()
    );
    let encoded = serde_json::to_value(Policy {
        read_only_patterns: vec!["global/**".into()],
        ..writable()
    })
    .unwrap();
    assert_eq!(encoded["readOnlyPatterns"], json!(["global/**"]));
}
#[test]
fn authored_revision_timestamp_uses_unix_milliseconds() {
    let (_dir, e, s) = fixture();
    let start = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let result = call(
        &e,
        &s,
        "write",
        json!({"name":"clock","content":"timestamp"}),
        &writable(),
    )
    .unwrap();
    let end = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let created = result["note"]["createdMs"].as_u64().unwrap();
    assert!(
        (start..=end).contains(&created),
        "{created} outside {start}..={end}"
    );
}

#[test]
fn full_retired_head_quota_rejects_publication_atomically_and_allows_reuse() {
    let (_dir, e, s) = fixture();
    let p = writable();
    seed_retired_heads(&e, &s, 2000, "");
    assert!(
        call(
            &e,
            &s,
            "write",
            json!({"name":"overflow","content":"new"}),
            &p
        )
        .is_err()
    );
    assert_eq!(
        call(&e, &s, "list", json!({}), &p).unwrap()["items"],
        json!([])
    );
    call(
        &e,
        &s,
        "write",
        json!({"name":"retired/template","content":"restored"}),
        &p,
    )
    .unwrap();
    let before = call(&e, &s, "read", json!({"name":"retired/template"}), &p).unwrap();
    assert!(
        call(
            &e,
            &s,
            "rename",
            json!({"oldName":"retired/template","newName":"overflow"}),
            &p
        )
        .is_err()
    );
    assert_eq!(
        call(&e, &s, "read", json!({"name":"retired/template"}), &p).unwrap(),
        before
    );
    assert_eq!(
        call(&e, &s, "list", json!({}), &p).unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let heads: u64 = e
        .store
        .db
        .query_row(
            "SELECT count(*) FROM records WHERE kind=?1",
            [notes::HEAD_KIND],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(heads, 2000);
}
