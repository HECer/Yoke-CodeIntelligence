//! Ignored public integration drafts. No feature execution or acceptance claim.
//! Promote only after root releases the next wave and freezes its source inputs.

use serde_json::{Value, json};
use std::{collections::BTreeSet, fs, path::Path};
use tempfile::TempDir;
use yoke::{
    Engine,
    capture::Deadline,
    model::{Graph, Snapshot, hash},
    retrieval,
};

struct Fixture {
    engine: Engine,
    snapshot: Snapshot,
    graph: Graph,
    root: TempDir,
}
fn write(root: &Path, path: &str, text: &str) {
    let file = root.join(path);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(file, text).unwrap();
}
fn fixture() -> Fixture {
    let root = TempDir::new().unwrap();
    for (path, text) in [
        ("package.json", r#"{"private":true}"#),
        (
            "pnpm-workspace.yaml",
            "packages:\n - 'packages/*'\n - '!packages/excluded'\n",
        ),
        ("packages/a/package.json", r#"{"name":"same"}"#),
        ("packages/a/Cargo.toml", "[package]\nname='mixed'\n"),
        ("packages/excluded/package.json", r#"{"name":"same"}"#),
        ("packages/a/main.rs", "fn alpha(){ payment(); }\n"),
        ("packages/excluded/main.rs", "fn excluded(){ payment(); }\n"),
    ] {
        write(root.path(), path, text);
    }
    let mut engine = Engine::open(root.path()).unwrap();
    let (snapshot, graph, _, _) = engine.index(&Deadline::new(30_000)).unwrap();
    Fixture {
        engine,
        snapshot,
        graph,
        root,
    }
}
fn args() -> Value {
    json!({"query":"payment","scopeMode":"manifest_balanced_v1","limit":20,
        "perFileLimit":20,"graphWeight":0,"pathGlob":"**/*.rs","includeBody":true})
}
fn balanced(f: &Fixture) -> Value {
    retrieval::find(
        &f.engine.store,
        &f.snapshot,
        &f.graph,
        &args(),
        &Deadline::new(20_000),
    )
    .unwrap()
}

#[test]
fn public_scope_summaries_keep_one_share_per_physical_root_and_excluded_items() {
    let f = fixture();
    let value = balanced(&f);
    let metadata = &value["retrievalScope"];
    assert_eq!(metadata["policy"], "yoke-retrieval-scopes-1");
    assert_eq!(metadata["requestedMode"], "manifest_balanced_v1");
    assert_eq!(metadata["effectiveMode"], "manifest_balanced_v1");
    assert_eq!(metadata["selectionReason"], "explicit_balanced_request");
    assert!(metadata["fallback"].is_null());
    assert_eq!(metadata["actualPackageRootCount"], 2);
    assert_eq!(metadata["queryEligibleScopeCount"], 2);
    let scopes = metadata["scopes"].as_array().unwrap();
    let a = scopes.iter().find(|s| s["root"] == "packages/a").unwrap();
    let excluded = scopes
        .iter()
        .find(|s| s["root"] == "packages/excluded")
        .unwrap();
    assert_eq!(a["kind"], "package");
    assert_ne!(
        a["id"], excluded["id"],
        "duplicate names are distinct roots"
    );
    assert_eq!(
        scopes.iter().filter(|s| s["root"] == "packages/a").count(),
        1
    );
    assert_eq!(a["labels"].as_array().unwrap().len(), 2);
    assert!(
        excluded["memberships"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["reason"] == "excluded")
    );
    let items = value["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    let ids: BTreeSet<_> = items
        .iter()
        .map(|row| row["scopeId"].as_str().unwrap())
        .collect();
    assert_eq!(ids.len(), 2);
    assert!(
        items.iter().any(|row| row["name"] == "excluded"),
        "membership exclusion cannot remove retrieval source"
    );
    for row in items {
        let original = f
            .graph
            .entities
            .iter()
            .find(|e| e.id == row["id"].as_str().unwrap())
            .unwrap();
        let path = row["path"].as_str().unwrap();
        let body = fs::read_to_string(f.root.path().join(path)).unwrap();
        assert_eq!(row["startByte"], original.start_byte);
        assert_eq!(row["endByte"], original.end_byte);
        assert_eq!(row["contentHash"], hash(body.as_bytes()));
        assert_eq!(row["body"], &body[original.start_byte..original.end_byte]);
    }
}

#[test]
fn public_partition_and_anchors_use_captured_bytes_after_live_tree_changes() {
    let f = fixture();
    let before = balanced(&f);
    write(f.root.path(), "pnpm-workspace.yaml", "packages: [\n");
    write(
        f.root.path(),
        "packages/a/package.json",
        r#"{"name":"new-live-name"}"#,
    );
    write(
        f.root.path(),
        "packages/a/main.rs",
        "fn replaced_live(){}\n",
    );
    let after = balanced(&f);
    assert_eq!(
        serde_json::to_vec(&before).unwrap(),
        serde_json::to_vec(&after).unwrap()
    );
    let alpha = after["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["name"] == "alpha")
        .unwrap();
    assert!(alpha["body"].as_str().unwrap().contains("payment()"));
    let bindings = after["retrievalScope"]["declarationBindings"]
        .as_array()
        .unwrap();
    let marker = bindings
        .iter()
        .find(|b| b["path"] == "packages/a/package.json")
        .unwrap();
    assert_eq!(marker["contentHash"], hash(br#"{"name":"same"}"#));
}

#[test]
fn malformed_captured_manifest_is_explicit_policy_error_without_source_contents() {
    let root = TempDir::new().unwrap();
    write(
        root.path(),
        "package.json",
        r#"{"name":"SOURCE_SECRET_DO_NOT_ECHO","broken":"#,
    );
    write(root.path(), "main.rs", "fn useful(){ payment(); }\n");
    let mut engine = Engine::open(root.path()).unwrap();
    let (snapshot, graph, _, _) = engine.index(&Deadline::new(30_000)).unwrap();
    let error = retrieval::find(
        &engine.store,
        &snapshot,
        &graph,
        &args(),
        &Deadline::new(20_000),
    )
    .unwrap_err();
    let message = format!("{error:#}");
    assert!(message.contains("source_syntax"), "{message}");
    assert!(!message.contains("SOURCE_SECRET_DO_NOT_ECHO"), "{message}");
    assert!(!message.contains("\"broken\""), "{message}");
}
