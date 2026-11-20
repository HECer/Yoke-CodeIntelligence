use serde_json::{Value, json};
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};
use tempfile::TempDir;
use yoke::{
    Engine,
    capture::Deadline,
    graph_view::{BuildOptions, build},
};

fn call(root: &Path, view: &str, operation: &str, options: Value, flags: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(root)
        .args(flags)
        .args([
            "graph",
            "view",
            view,
            operation,
            "--args",
            &options.to_string(),
        ])
        .output()
        .unwrap()
}

fn response(output: Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn pins_historic_view_and_preserves_parallel_call_observations_under_context_filter() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("app.py");
    let source = "\
def helper():
    return 1

def branch():
    return helper()

def entrypoint():
    return branch() + helper() + helper()

def caller():
    return entrypoint()
";
    fs::write(&path, source).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let d = Deadline::new(45_000);
    let (snapshot, graph, _, _) = engine.index(&d).unwrap();
    let view = build(
        &engine.store,
        &snapshot,
        &graph,
        &BuildOptions::default(),
        &d,
    )
    .unwrap();
    let entity = |name: &str| {
        view.nodes.iter().find(|row| row["name"] == name).unwrap()["id"]
            .as_str()
            .unwrap()
    };
    fs::write(&path, b"def changed(): return 9\n").unwrap();

    let report = response(call(
        root.path(),
        &view.id,
        "query",
        json!({"seeds":["entrypoint"],"mode":"dfs","direction":"out","contexts":["call"],"depth":2}),
        &[],
    ));
    assert_eq!(report["graphViewId"], view.id);
    assert_eq!(report["snapshotId"], snapshot.id);
    assert_eq!(report["mode"], "dfs");
    assert_eq!(report["seeds"][0]["id"], entity("entrypoint"));
    let nodes = report["entities"].as_array().unwrap();
    assert_eq!(nodes.len(), 3);
    assert!(nodes.iter().any(|node| node["id"] == entity("helper")));
    assert!(!nodes.iter().any(|node| node["id"] == entity("caller")));
    let observations = report["observations"].as_array().unwrap();
    assert_eq!(
        observations.iter().filter(|row|
            row["source"] == entity("entrypoint") && row["target"] == entity("helper")
        ).count(),
        2
    );
    assert!(observations.iter().all(|row| row["relation"] == "calls"));
    assert!(
        observations
            .iter()
            .all(|row| view.observations.contains(row))
    );
    let evidence = report["evidence"].as_array().unwrap();
    assert!(observations.iter().all(|row| {
        row["evidenceIds"]
            .as_array()
            .unwrap()
            .iter()
            .all(|id| evidence.iter().any(|anchor| anchor["id"] == *id))
    }));
    let helper = report["visits"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == entity("helper"))
        .unwrap();
    assert_eq!(helper["depth"], 1);
    assert_eq!(helper["via"]["from"], entity("entrypoint"));
    assert_eq!(helper["via"]["observation"]["target"], entity("helper"));

    let explained = response(call(
        root.path(),
        &view.id,
        "explain",
        json!({"query":"what calls EntryPoint","direction":"in"}),
        &[],
    ));
    assert_eq!(explained["seeds"].as_array().unwrap().len(), 1);
    assert_eq!(explained["filters"]["contexts"], json!(["call"]));
    assert_eq!(explained["filters"]["contextSource"], "query_intent");
    assert_eq!(explained["seeds"][0]["id"], entity("entrypoint"));
    assert!(explained["seeds"][0]["score"].as_u64().unwrap() > 0);
    assert!(
        explained["entities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|node| node["id"] == entity("caller"))
    );
    assert_eq!(fs::read(&path).unwrap(), b"def changed(): return 9\n");
    assert_eq!(
        engine.store.latest().unwrap().as_deref(),
        Some(snapshot.id.as_str())
    );

    let capped = response(call(
        root.path(),
        &view.id,
        "query",
        json!({"seeds":[entity("entrypoint")],"maxVisited":1,"contexts":["call"]}),
        &[],
    ));
    assert_eq!(capped["entities"].as_array().unwrap().len(), 1);
    assert_eq!(capped["truncated"], true);
    let too_small = call(
        root.path(),
        &view.id,
        "query",
        json!({"seeds":["entrypoint"]}),
        &["--max-bytes", "100"],
    );
    assert!(!too_small.status.success());
    assert!(too_small.stdout.is_empty());
    let wrong_snapshot = "a".repeat(64);
    let mismatched = call(
        root.path(),
        &view.id,
        "query",
        json!({"seeds":["entrypoint"]}),
        &["--snapshot", &wrong_snapshot],
    );
    assert!(!mismatched.status.success());
}

#[test]
fn rejects_ambiguous_selectors_and_requires_existing_store() {
    let root = TempDir::new().unwrap();
    let missing = "a".repeat(64);
    let rejected = call(
        root.path(),
        &missing,
        "query",
        json!({"seeds":["ambiguous"]}),
        &[],
    );
    assert!(!rejected.status.success());
    assert!(!root.path().join(".yoke").exists());
    for file in ["first.py", "second.py"] {
        fs::write(root.path().join(file), b"def ambiguous(): return 1\n").unwrap();
    }
    let mut engine = Engine::open(root.path()).unwrap();
    let d = Deadline::new(45_000);
    let (snapshot, graph, _, _) = engine.index(&d).unwrap();
    let view = build(
        &engine.store,
        &snapshot,
        &graph,
        &BuildOptions::default(),
        &d,
    )
    .unwrap();
    let exact = response(call(
        root.path(),
        &view.id,
        "query",
        json!({"seeds":["ambiguous","missing","first.py"],"depth":0}),
        &[],
    ));
    assert_eq!(
        exact["unresolvedSeeds"],
        json!([
            {"seed":"ambiguous","candidateCount":2},
            {"seed":"missing","candidateCount":0}
        ])
    );
    assert_eq!(exact["entities"].as_array().unwrap().len(), 1);
    assert_eq!(exact["entities"][0]["path"], "first.py");
    let conflict = call(
        root.path(),
        &view.id,
        "query",
        json!({"seeds":["first.py"],"query":"ambiguous"}),
        &[],
    );
    assert!(!conflict.status.success());
    let authority = call(
        root.path(),
        &view.id,
        "query",
        json!({"seeds":["first.py"]}),
        &["--allow-writes"],
    );
    assert!(!authority.status.success());
}
