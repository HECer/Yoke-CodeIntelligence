use serde_json::{Value, json};
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};
use tempfile::TempDir;

fn run(root: &Path, input: &str, output_bytes: u64, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_yoke"))
        .current_dir(root)
        .args([
            "--max-bytes",
            &output_bytes.to_string(),
            "graph",
            "diagnose",
            "--input",
            input,
        ])
        .args(args)
        .output()
        .unwrap()
}

fn json_output(result: &Output) -> Value {
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    serde_json::from_slice(&result.stdout).unwrap()
}

#[test]
fn diagnoses_raw_parallel_reverse_self_and_out_of_corpus_edges() {
    let root = TempDir::new().unwrap();
    let input = json!({
        "directed": true,
        "nodes": [{"id":"a","verification":"unverified"},{"id":"b"},{"id":7},{"id":"7"},null],
        "links": [
            {"source":"a","target":"b","relation":"calls","source_file":"a.py","source_location":"L2","context":"π"},
            {"from":"a","to":"b","relation":"calls","source_file":"a.py","source_location":"L2","context":"π"},
            {"source":"a","target":"b","relation":"calls","source_file":"b.py","source_location":"L4","context":"😀"},
            {"source":"a","target":"b","relation":"references"},
            {"source":"b","target":"a","relation":"calls"},
            {"source":"a","target":"a","relation":"contains"},
            {"source":7,"target":"b","relation":"calls"},
            {"source":"a","target":"ref_external"},
            {"source":"absent","target":"a"},
            {"source":"a","target":"absent"},
            {"source":"","target":"a"},
            {"source":"a","from":"b","target":"b"},
            42
        ]
    });
    let raw = serde_json::to_vec(&input).unwrap();
    fs::write(root.path().join("graph.json"), &raw).unwrap();
    let output = run(root.path(), "graph.json", 32000, &[]);
    let value = json_output(&output);
    assert_eq!(value["inputSha256"], yoke::model::hash(&raw));
    assert_eq!(value["declaredDirected"], true);
    assert_eq!(
        value["counts"],
        json!({
            "nodeRows":5,"uniqueNodes":3,"invalidNodeRows":1,"duplicateNodeIds":1,"unverifiedNodeRows":1,
            "edgeRows":13,"nonObjectEdges":1,"invalidEndpointEdges":2,"danglingEndpointEdges":2,
            "externalReferenceEdges":1,"boundEdges":7,"exactDuplicateEdges":1,"selfLoops":1,
            "directedEndpointPairs":4,"directedParallelGroups":1,"directedCollapseLoss":3,
            "reverseDirectionGroups":1,"undirectedEndpointPairs":3,"undirectedCollapseLoss":4,
            "relationVariantGroups":1,"sourceFileVariantGroups":1,"sourceLocationVariantGroups":1,
            "contextVariantGroups":1
        })
    );
    assert_eq!(
        value["parallelExamples"],
        json!([{"source":"a","target":"b","observations":4}])
    );
    assert!(value["producerSuppression"].is_null());
    assert_eq!(fs::read(root.path().join("graph.json")).unwrap(), raw);
    assert!(!root.path().join(".yoke").exists());
}

#[test]
fn scans_only_the_selected_producer_and_retains_counts_without_examples() {
    let root = TempDir::new().unwrap();
    let raw = b"{\"nodes\":[],\"edges\":[]}";
    fs::write(root.path().join("graph.json"), raw).unwrap();
    let producer = "\u{feff}seen_first: set[tuple[str, str]] = set()\r\n\
        # seen_comment = set()\r\nraise RuntimeError('not an execution fixture')\r\n\
        seen_second = set()\r\nseen_ = set()\r\n";
    fs::write(root.path().join("extract.py"), producer).unwrap();
    let args = ["--producer-source", "extract.py", "--examples", "1"];
    let output = run(root.path(), "graph.json", 32000, &args);
    let value = json_output(&output);
    assert_eq!(
        value["producerSuppression"],
        json!({"sourceSha256":yoke::model::hash(producer),"kind":"heuristic_seen_declarations",
            "matchingLines":2,"examples":[{"line":1,"name":"seen_first"}]})
    );
    assert_eq!(
        fs::read(root.path().join("extract.py")).unwrap(),
        producer.as_bytes()
    );
    let without_examples = run(
        root.path(),
        "graph.json",
        32000,
        &["--producer-source", "extract.py", "--examples", "0"],
    );
    let value = json_output(&without_examples);
    assert_eq!(value["producerSuppression"]["matchingLines"], 2);
    assert_eq!(value["producerSuppression"]["examples"], json!([]));
    assert!(!root.path().join(".yoke").exists());
}

#[test]
fn refuses_ambiguous_or_over_budget_reports_before_publishing_any_output() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("graph.json");
    fs::write(&path, b"{\"nodes\":[],\"nodes\":[],\"edges\":[]}").unwrap();
    let duplicate = run(root.path(), "graph.json", 32000, &[]);
    assert!(!duplicate.status.success() && duplicate.stdout.is_empty());
    assert!(String::from_utf8_lossy(&duplicate.stderr).contains("Duplicate JSON key"));

    fs::write(&path, b"{\"nodes\":[],\"edges\":[]}").unwrap();
    let full = run(root.path(), "graph.json", 32000, &[]);
    json_output(&full);
    let exact = run(root.path(), "graph.json", full.stdout.len() as u64, &[]);
    assert_eq!(json_output(&exact), json_output(&full));
    let small = run(root.path(), "graph.json", full.stdout.len() as u64 - 1, &[]);
    assert!(!small.status.success() && small.stdout.is_empty());
    assert!(!root.path().join(".yoke").exists());
}
