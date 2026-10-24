use serde_json::{Value, json};
use std::fs;
use tempfile::TempDir;
use yoke::Engine;

fn fixture() -> (TempDir, Engine) {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("main.rs"),
        "// WHY: preserve UTF-8 🌍\nfn helper() -> u32 { 7 }\nfn main() { helper(); helper(); }\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("README.md"),
        "# Architecture\nSee [helper](main.rs).\n",
    )
    .unwrap();
    fs::write(dir.path().join(".env"), "SECRET_TOKEN=never-index-this").unwrap();
    let engine = Engine::open(dir.path()).unwrap();
    (dir, engine)
}
fn invoke(engine: &mut Engine, name: &str, args: Value) -> Value {
    let value = engine.invoke(name, args).unwrap();
    assert_ne!(value["status"], "error", "{value}");
    value
}

#[test]
fn immutable_capture_excludes_secrets_and_reuses_content_cache() {
    let (dir, mut engine) = fixture();
    let first = invoke(&mut engine, "ci_status", json!({"mode":"index"}));
    let sid = first["snapshotId"].as_str().unwrap();
    assert_eq!(
        first["data"]["snapshot"]["files"].as_array().unwrap().len(),
        2
    );
    assert_eq!(first["data"]["parsedFiles"], 2);
    let second = invoke(&mut engine, "ci_status", json!({"mode":"index"}));
    assert_eq!(second["snapshotId"], sid);
    assert_eq!(second["data"]["parsedFiles"], 0);
    fs::write(dir.path().join("main.rs"), "fn changed() {}\n").unwrap();
    let old = invoke(
        &mut engine,
        "ci_read",
        json!({"snapshotId":sid,"path":"main.rs"}),
    );
    assert!(old["data"]["text"].as_str().unwrap().contains("helper"));
    let fresh = invoke(&mut engine, "ci_find", json!({"query":"changed"}));
    assert_ne!(fresh["snapshotId"], sid);
    assert!(
        fresh["data"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["name"] == "changed")
    );
}

#[test]
fn directed_graph_preserves_parallel_calls_and_exact_unicode_anchors() {
    let (_dir, mut engine) = fixture();
    let found = invoke(
        &mut engine,
        "ci_find",
        json!({"query":"main","kind":"symbol"}),
    );
    let entity = found["data"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == "main")
        .unwrap();
    let traced = invoke(
        &mut engine,
        "ci_trace",
        json!({"snapshotId":found["snapshotId"],"entityIds":[entity["id"]],"direction":"out","relations":["calls"],"depth":1}),
    );
    let edges = traced["data"]["observations"].as_array().unwrap();
    assert_eq!(edges.len(), 2);
    assert_ne!(edges[0]["evidenceIds"], edges[1]["evidenceIds"]);
    assert!(edges.iter().all(|e| e["resolution"] != "exact_binding"));
    for evidence in traced["data"]["evidence"].as_array().unwrap() {
        let read = invoke(
            &mut engine,
            "ci_read",
            json!({"snapshotId":found["snapshotId"],"path":evidence["path"],"startByte":evidence["startByte"],"endByte":evidence["endByte"]}),
        );
        assert_eq!(read["data"]["text"], "helper()");
    }
}

#[test]
fn source_paths_and_utf8_boundaries_fail_closed() {
    let (_dir, mut engine) = fixture();
    invoke(&mut engine, "ci_status", json!({"mode":"index"}));
    for path in [
        "../outside",
        "C:/Windows/win.ini",
        "main.rs:stream",
        ".env",
        ".yoke/index.sqlite",
    ] {
        let result = engine.invoke("ci_read", json!({"path":path})).unwrap();
        assert_eq!(result["status"], "error", "{path}: {result}");
    }
    let result = engine
        .invoke(
            "ci_read",
            json!({"path":"main.rs","startByte":25,"endByte":26}),
        )
        .unwrap();
    assert_eq!(result["status"], "error");
}

#[test]
fn prepared_edits_are_isolated_and_reject_stale_hashes() {
    let (dir, mut engine) = fixture();
    let read = invoke(&mut engine, "ci_read", json!({"path":"main.rs"}));
    let plan = invoke(
        &mut engine,
        "ci_change_prepare",
        json!({"snapshotId":read["snapshotId"],"edits":[{"kind":"replace","path":"main.rs","expectedHash":read["data"]["contentHash"],"startByte":0,"endByte":0,"text":"// added\n"}]}),
    );
    let plan_id = &plan["data"]["plan"]["id"];
    assert!(
        !fs::read_to_string(dir.path().join("main.rs"))
            .unwrap()
            .starts_with("// added")
    );
    let applied = invoke(
        &mut engine,
        "ci_change_apply",
        json!({"planId":plan_id,"idempotencyKey":"test"}),
    );
    let isolated = applied["data"]["receipt"]["sandboxPath"].as_str().unwrap();
    assert!(
        fs::read_to_string(Path::new(isolated).join("main.rs"))
            .unwrap()
            .starts_with("// added")
    );
    let denied = engine
        .invoke(
            "ci_change_apply",
            json!({"planId":plan_id,"idempotencyKey":"workspace","mode":"workspace"}),
        )
        .unwrap();
    assert_eq!(denied["status"], "error");
    let stale = engine
        .invoke(
            "ci_change_prepare",
            json!({"edits":[{"kind":"delete","path":"main.rs","expectedHash":"0".repeat(64)}]}),
        )
        .unwrap();
    assert_eq!(stale["status"], "error");
}
use std::path::Path;

#[test]
fn bounded_results_keep_artifacts_and_reject_authority_fields() {
    let (dir, mut engine) = fixture();
    fs::write(dir.path().join("large.md"), "context ".repeat(4000)).unwrap();
    let result = invoke(
        &mut engine,
        "ci_read",
        json!({"path":"large.md","maxBytes":2048}),
    );
    assert_eq!(result["status"], "partial");
    assert!(serde_json::to_vec(&result).unwrap().len() <= 2048);
    assert!(result["artifactId"].as_str().unwrap().len() == 64);
    let artifact = invoke(
        &mut engine,
        "ci_read",
        json!({"artifactId":result["artifactId"],"maxBytes":2048}),
    );
    assert!(!artifact["data"]["chunk"].as_str().unwrap().is_empty());
    let denied = engine
        .invoke("ci_status", json!({"trustProviders":true}))
        .unwrap();
    assert_eq!(denied["status"], "error");
}

#[test]
fn context_and_graph_analytics_are_source_bound() {
    let (_dir, mut engine) = fixture();
    let context = invoke(&mut engine, "ci_context", json!({"query":"helper"}));
    assert!(
        context["data"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["text"].as_str().is_some_and(|t| t.contains("fn helper")))
    );
    let analytics = invoke(&mut engine, "ci_status", json!({"mode":"analyze"}));
    assert!(
        !analytics["data"]["analytics"]["communities"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        analytics["data"]["analytics"]["pageRank"]
            .as_array()
            .unwrap()
            .iter()
            .all(|i| i["score"].as_f64().unwrap().is_finite())
    );
}
