use serde_json::{Value, json};
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};
use tempfile::TempDir;
use yoke::{Engine, capture::Deadline, model::hash, query};

fn cli(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(root)
        .args(args)
        .output()
        .unwrap()
}

fn report(output: Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn benchmarks_exact_native_context_data_and_historical_corpus() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("app.py");
    let original = "# Unicode text\u{a0}here \u{1f600}\r\ndef authentication():\r\n    return \"caf\u{e9} \u{1f600}\"\r\n";
    fs::write(&path, original).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let deadline = Deadline::new(30_000);
    let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
    let changed = "def changed_after_capture(): return 7\n";
    fs::write(&path, changed).unwrap();

    let response = report(cli(
        root.path(),
        &[
            "--snapshot",
            &snapshot.id,
            "graph",
            "benchmark",
            "--question",
            "authentication",
            "--question",
            "unlikelyunmatchedidentifier",
        ],
    ));
    let data = query::context(
        &engine.store,
        &snapshot,
        &graph,
        &json!({"query": "authentication", "limit": 5}),
        &deadline,
    )
    .unwrap();
    let encoded = serde_json::to_string(&data).unwrap();
    let chars = encoded.chars().count();
    assert!(encoded.len() > chars);
    let words = original.split_whitespace().count();
    assert_eq!(response["snapshotId"], snapshot.id);
    assert_eq!(response["capturedFiles"], 1);
    assert_eq!(response["corpusBaseline"], "captured_text_word_count");
    assert_eq!(response["corpusWords"], words);
    assert_eq!(response["estimatedCorpusTokens"], words * 4 / 3);
    assert_eq!(response["matchedQuestions"], 1);
    assert_eq!(
        response["questions"][0]["returnedItems"],
        data["items"].as_array().unwrap().len()
    );
    assert_eq!(response["questions"][0]["contextCharacters"], chars);
    assert_eq!(response["questions"][0]["contextSha256"], hash(encoded));
    assert_eq!(response["questions"][1]["returnedItems"], 0);
    assert_eq!(response["questions"][1]["estimatedQueryTokens"], 0);
    assert!(response["questions"][1]["reductionRatio"].is_null());
    assert_eq!(response["averageEstimatedQueryTokens"], chars / 4);
    assert_eq!(fs::read_to_string(&path).unwrap(), changed);
    assert_eq!(
        engine.store.latest().unwrap().as_deref(),
        Some(snapshot.id.as_str())
    );

    let missing = report(cli(
        root.path(),
        &[
            "--snapshot",
            &snapshot.id,
            "graph",
            "benchmark",
            "--corpus-words",
            "1000",
            "--question",
            "unlikelyunmatchedidentifier",
        ],
    ));
    assert_eq!(missing["corpusBaseline"], "host_supplied_word_count");
    assert_eq!(missing["estimatedCorpusTokens"], 1333);
    assert_eq!(missing["matchedQuestions"], 0);
    assert!(missing["averageEstimatedQueryTokens"].is_null());
    assert!(missing["reductionRatio"].is_null());

    let insufficient = cli(
        root.path(),
        &[
            "--snapshot",
            &snapshot.id,
            "--max-bytes",
            "100",
            "graph",
            "benchmark",
        ],
    );
    assert!(!insufficient.status.success());
    assert!(insufficient.stdout.is_empty());
}

#[test]
fn refuses_implicit_capture_and_provider_flags_without_initializing_state() {
    let root = TempDir::new().unwrap();
    let selected = "a".repeat(64);
    for args in [
        vec!["graph", "benchmark"],
        vec!["--snapshot", &selected, "graph", "benchmark"],
        vec![
            "--snapshot",
            &selected,
            "--trust-servers",
            "graph",
            "benchmark",
        ],
        vec![
            "--snapshot",
            &selected,
            "--allow-writes",
            "graph",
            "benchmark",
        ],
    ] {
        let output = cli(root.path(), &args);
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(!root.path().join(".yoke").exists());
    }
}
