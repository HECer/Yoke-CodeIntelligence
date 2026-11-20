use serde_json::{Value, json};
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};
use tempfile::TempDir;

fn yoke(root: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_yoke"))
        .current_dir(root)
        .args(["graph", "union-merge"])
        .args(arguments)
        .output()
        .unwrap()
}

fn git(root: &Path, arguments: &[&str]) -> Output {
    let result = Command::new("git")
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .env("GIT_CONFIG_COUNT", "0")
        .env_remove("GIT_CONFIG_PARAMETERS")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .args(arguments)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{arguments:?}: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    result
}

fn write_graph(root: &Path, filename: &str, value: &Value) {
    fs::write(root.join(filename), serde_json::to_vec(value).unwrap()).unwrap();
}

fn branches() -> (Value, Value) {
    let calls =
        json!({"source":"a","target":"b","relation":"calls","key":0,"source_location":"L2"});
    let ours = json!({
        "directed":false,
        "graph":{"left":"owned","shared":"left"},
        "nodes":[{"id":"a","label":"α"},{"id":"b","language":"rust"}],
        "edges":[calls,calls,{"source":"a","target":"b","_src":"b","_tgt":"a","relation":"imports","key":0}],
        "hyperedges":[{"id":"shared","nodes":["a","b"]}]
    });
    let theirs = json!({
        "directed":false,
        "graph":{"shared":"right"},
        "nodes":[{"id":"a","label":"β"},{"id":"b"},{"id":"c"}],
        "links":[calls,{"source":"a","target":"b","relation":"contains","key":0},
            {"source":"a","target":"ref_unresolved","relation":"references"}],
        "hyperedges":[{"id":"shared","nodes":["a","b"]},{"id":"additional","nodes":["a","b","c"]}]
    });
    (ours, theirs)
}

fn assert_union(value: &Value) {
    assert_eq!(value["directed"], false);
    assert_eq!(value["multigraph"], true);
    assert_eq!(value["graph"]["left"], "owned");
    assert_eq!(value["graph"]["shared"], "right");
    assert_eq!(
        value["nodes"],
        json!([{"id":"a","label":"β"},{"id":"b","language":"rust"},{"id":"c"}])
    );
    assert_eq!(value["hyperedges"].as_array().unwrap().len(), 2);
    let links = value["links"].as_array().unwrap();
    assert_eq!(links.len(), 5);
    assert_eq!(links[0]["relation"], "calls");
    assert_eq!(links[1]["relation"], "calls");
    assert_eq!(links[2]["source"], "b");
    assert_eq!(links[2]["target"], "a");
    assert_eq!(links[3]["relation"], "contains");
    assert_eq!(links[4]["target"], "ref_unresolved");
    for (index, edge) in links.iter().enumerate() {
        assert_eq!(edge["key"], index);
        if index < 4 {
            assert_eq!(edge["yokeOriginalEdgeKey"], 0);
        }
    }
}

#[test]
fn preview_preserves_multiplicity_and_remerges_idempotently() {
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("base.json"), []).unwrap();
    let (ours, theirs) = branches();
    write_graph(root.path(), "ours.json", &ours);
    write_graph(root.path(), "theirs.json", &theirs);
    let result = yoke(root.path(), &["base.json", "ours.json", "theirs.json"]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let graph: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_union(&graph);
    write_graph(root.path(), "merged.json", &graph);
    let repeated = yoke(root.path(), &["base.json", "merged.json", "theirs.json"]);
    assert!(
        repeated.status.success(),
        "{}",
        String::from_utf8_lossy(&repeated.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&repeated.stdout).unwrap(),
        graph
    );
    assert_eq!(
        fs::read(root.path().join("ours.json")).unwrap(),
        serde_json::to_vec(&ours).unwrap()
    );
    assert!(!root.path().join(".yoke").exists());
}

#[test]
fn git_calls_the_driver_and_commits_the_union() {
    let root = TempDir::new().unwrap();
    let path = root.path();
    git(path, &["init", "-q", "--initial-branch=main"]);
    git(path, &["config", "--local", "user.name", "Owned Fixture"]);
    git(
        path,
        &["config", "--local", "user.email", "fixture@example.invalid"],
    );
    git(path, &["config", "--local", "core.autocrlf", "false"]);
    let binary = env!("CARGO_BIN_EXE_yoke")
        .replace('\\', "/")
        .replace('\'', "'\"'\"'");
    let driver = format!("'{binary}' graph union-merge --write-git-current %O %A %B");
    git(
        path,
        &["config", "--local", "merge.yoke-graphify.driver", &driver],
    );
    fs::write(
        path.join(".gitattributes"),
        "graph.json merge=yoke-graphify\n",
    )
    .unwrap();
    fs::write(path.join("source.rs"), b"fn owned() {}\r\n").unwrap();
    write_graph(
        path,
        "graph.json",
        &json!({"directed":false,"nodes":[],"links":[]}),
    );
    git(
        path,
        &["add", "--", ".gitattributes", "source.rs", "graph.json"],
    );
    git(path, &["commit", "-qm", "base"]);
    git(path, &["branch", "other"]);
    let (ours, theirs) = branches();
    write_graph(path, "graph.json", &ours);
    git(path, &["commit", "-qam", "current"]);
    git(path, &["switch", "-q", "other"]);
    write_graph(path, "graph.json", &theirs);
    git(path, &["commit", "-qam", "other"]);
    git(path, &["switch", "-q", "main"]);
    git(path, &["merge", "--no-edit", "other"]);
    let result = git(path, &["show", "HEAD:graph.json"]);
    assert_union(&serde_json::from_slice(&result.stdout).unwrap());
    let parents = git(path, &["rev-list", "--parents", "-1", "HEAD"]);
    assert_eq!(
        String::from_utf8_lossy(&parents.stdout)
            .split_whitespace()
            .count(),
        3
    );
    assert_eq!(
        fs::read(path.join("source.rs")).unwrap(),
        b"fn owned() {}\r\n"
    );
    assert!(!path.join(".yoke").exists());
}

#[test]
fn invalid_graph_or_regular_current_path_leaves_every_input_byte_unchanged() {
    let root = TempDir::new().unwrap();
    let path = root.path();
    let base = ".merge_file_AAAAAA";
    let current = ".merge_file_BBBBBB";
    let other = ".merge_file_CCCCCC";
    let original = br#"{"nodes":[{"id":"a"}],"edges":[]}"#;
    fs::write(path.join(base), []).unwrap();
    fs::write(path.join(current), original).unwrap();
    fs::write(path.join("graph.json"), original).unwrap();
    fs::write(path.join(other), b"invalid graph").unwrap();
    let invalid = yoke(path, &["--write-git-current", base, current, other]);
    assert!(!invalid.status.success() && invalid.stdout.is_empty());
    let regular = yoke(path, &["--write-git-current", base, "graph.json", other]);
    assert!(!regular.status.success() && regular.stdout.is_empty());
    assert_eq!(fs::read(path.join(current)).unwrap(), original);
    assert_eq!(fs::read(path.join("graph.json")).unwrap(), original);
    assert_eq!(fs::read(path.join(other)).unwrap(), b"invalid graph");
    assert_eq!(fs::read_dir(path).unwrap().count(), 4);
}
