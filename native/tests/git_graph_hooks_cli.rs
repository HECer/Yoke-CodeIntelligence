use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};
use tempfile::TempDir;

fn git_environment(command: &mut Command) -> &mut Command {
    command
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
}

fn git(root: &Path, arguments: &[&str]) -> Output {
    let result = git_environment(&mut Command::new("git"))
        .current_dir(root)
        .args(arguments)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{arguments:?}: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        !stderr.lines().any(|line| line.starts_with("yoke:")),
        "Git hook failed during {arguments:?}: {stderr}"
    );
    result
}

fn root() -> TempDir {
    let root = TempDir::new().unwrap();
    seed_repository(root.path());
    root
}

fn seed_repository(path: &Path) {
    git(path, &["init", "-q", "--initial-branch=main"]);
    git(path, &["config", "--local", "user.name", "Owned Fixture"]);
    git(
        path,
        &["config", "--local", "user.email", "fixture@example.invalid"],
    );
    git(path, &["config", "--local", "core.autocrlf", "false"]);
    fs::write(path.join("original.rs"), b"pub fn obsolete() {}\r\n").unwrap();
    git(path, &["add", "--", "original.rs"]);
    git(path, &["commit", "-qm", "baseline"]);
}

fn hooks(root: &Path, arguments: &[&str]) -> Output {
    git_environment(&mut Command::new(env!("CARGO_BIN_EXE_yoke")))
        .current_dir(root)
        .args(["git", "graph-hooks"])
        .args(arguments)
        .output()
        .unwrap()
}

fn success(result: &Output) -> Value {
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    serde_json::from_slice(&result.stdout).unwrap()
}

fn install(root: &Path) -> Value {
    success(&hooks(
        root,
        &["install", "--trust-git", "--allow-hook-write"],
    ))
}

fn graph_path(root: &Path) -> PathBuf {
    root.join(".git/yoke-graph-hooks/graph.json")
}

fn head_graph(root: &Path, event: &str) -> Value {
    let path = graph_path(root);
    let bytes = fs::read(&path).unwrap_or_else(|error| panic!("{event}: {path:?}: {error}"));
    let graph: Value = serde_json::from_slice(&bytes).unwrap();
    let head = git(root, &["rev-parse", "HEAD"]);
    assert_eq!(
        graph["graph"]["sourceCommit"],
        String::from_utf8_lossy(&head.stdout).trim()
    );
    assert_eq!(graph["graph"]["event"], event);
    assert!(
        graph["graph"]["snapshotId"]
            .as_str()
            .is_some_and(yoke::model::valid_id)
    );
    assert!(
        graph["graph"]["nativeGraphSha256"]
            .as_str()
            .is_some_and(yoke::model::valid_id)
    );
    assert_eq!(graph["directed"], true);
    assert_eq!(graph["multigraph"], true);
    graph
}

fn has_label(graph: &Value, label: &str) -> bool {
    graph["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|node| node["label"] == label)
}

#[test]
fn real_commit_checkout_and_merge_refresh_only_the_exact_committed_graph() {
    let root = root();
    let path = root.path();
    git(path, &["branch", "topic"]);
    fs::write(
        path.join(".git/hooks/pre-push"),
        b"owned unrelated host fixture\n",
    )
    .unwrap();
    let installed = install(path);
    assert_eq!(installed["state"], "active");
    assert_eq!(installed["ownedHooks"], 3);
    assert_eq!(
        install(path)["originSnapshotId"],
        installed["originSnapshotId"]
    );

    fs::write(path.join("main.rs"), b"pub fn main_only() {}\r\n").unwrap();
    fs::write(path.join("untracked.rs"), b"pub fn untracked_only() {}\r\n").unwrap();
    git(path, &["add", "--", "main.rs"]);
    git(path, &["commit", "-qm", "main change"]);
    let graph = head_graph(path, "post-commit");
    assert!(has_label(&graph, "obsolete") && has_label(&graph, "main_only"));
    assert!(!has_label(&graph, "untracked_only"));

    git(path, &["switch", "-q", "topic"]);
    let graph = head_graph(path, "post-checkout");
    assert!(has_label(&graph, "obsolete"));
    assert!(!has_label(&graph, "main_only"));
    let previous = fs::read(graph_path(path)).unwrap();
    fs::write(path.join("original.rs"), b"pub fn dirty() {}\r\n").unwrap();
    git(path, &["checkout", "--", "original.rs"]);
    assert_eq!(fs::read(graph_path(path)).unwrap(), previous);

    git(path, &["rm", "--", "original.rs"]);
    fs::write(path.join("topic.rs"), b"pub fn topic_only() {}\r\n").unwrap();
    git(path, &["add", "--", "topic.rs"]);
    git(path, &["commit", "-qm", "topic change"]);
    assert!(has_label(&head_graph(path, "post-commit"), "topic_only"));
    git(path, &["switch", "-q", "main"]);
    assert!(has_label(&head_graph(path, "post-checkout"), "obsolete"));
    git(path, &["merge", "--no-edit", "topic"]);
    let graph = head_graph(path, "post-merge");
    assert!(has_label(&graph, "main_only") && has_label(&graph, "topic_only"));
    assert!(!has_label(&graph, "obsolete") && !has_label(&graph, "untracked_only"));
    let previous = fs::read(graph_path(path)).unwrap();
    let disabled = success(&hooks(path, &["uninstall", "--allow-hook-write"]));
    assert_eq!(disabled["state"], "inactive");
    git(path, &["commit", "--allow-empty", "-qm", "disabled hook"]);
    assert_eq!(previous, fs::read(graph_path(path)).unwrap());
    assert_eq!(
        fs::read(path.join(".git/hooks/pre-push")).unwrap(),
        b"owned unrelated host fixture\n"
    );
    assert_eq!(
        fs::read(path.join("main.rs")).unwrap(),
        b"pub fn main_only() {}\r\n"
    );
    assert_eq!(
        fs::read(path.join("topic.rs")).unwrap(),
        b"pub fn topic_only() {}\r\n"
    );
}

#[test]
fn explicit_grants_and_foreign_hook_content_precede_installation_writes() {
    let root = root();
    let path = root.path();
    let denied = hooks(path, &["install", "--trust-git"]);
    assert!(!denied.status.success());
    assert!(!path.join(".git/yoke-graph-hooks").exists());
    let foreign = path.join(".git/hooks/post-checkout");
    fs::write(&foreign, "#!/bin/sh\n# unrelated host hook\n").unwrap();
    let refused = hooks(path, &["install", "--trust-git", "--allow-hook-write"]);
    assert!(!refused.status.success());
    assert!(!path.join(".git/yoke-graph-hooks").exists());
    assert!(!path.join(".git/hooks/post-commit").exists());
    assert!(!path.join(".yoke").exists());
    assert_eq!(
        fs::read(foreign).unwrap(),
        b"#!/bin/sh\n# unrelated host hook\n"
    );
}

#[test]
fn staged_install_resumes_and_tampering_refuses_removal() {
    let root = root();
    let path = root.path();
    let installed = install(path);
    let active = path.join(".git/yoke-graph-hooks/active");
    let checkout = path.join(".git/hooks/post-checkout");
    let checkout_bytes = fs::read(&checkout).unwrap();
    fs::remove_file(&active).unwrap();
    fs::remove_file(&checkout).unwrap();
    assert_eq!(success(&hooks(path, &["status"]))["state"], "incomplete");
    git(path, &["commit", "--allow-empty", "-qm", "staging hook"]);
    assert!(!graph_path(path).exists());
    assert_eq!(
        install(path)["originSnapshotId"],
        installed["originSnapshotId"]
    );
    assert_eq!(fs::read(&checkout).unwrap(), checkout_bytes);
    fs::write(&checkout, b"foreign replacement\n").unwrap();
    let attempted = hooks(path, &["uninstall", "--allow-hook-write"]);
    assert!(!attempted.status.success());
    assert!(active.exists() && path.join(".git/hooks/post-commit").exists());
    assert_eq!(fs::read(&checkout).unwrap(), b"foreign replacement\n");
    fs::write(&checkout, checkout_bytes).unwrap();
    let removed = success(&hooks(path, &["uninstall", "--allow-hook-write"]));
    assert_eq!(removed["ownedHooks"], 0);
    assert!(!active.exists());
}

#[test]
fn a_custom_hook_directory_needs_matching_explicit_selection() {
    let root = root();
    let path = root.path();
    let custom = path.join("owned hooks π");
    fs::create_dir(&custom).unwrap();
    git(
        path,
        &["config", "--local", "core.hooksPath", "owned hooks π"],
    );
    let refused = hooks(path, &["install", "--trust-git", "--allow-hook-write"]);
    assert!(!refused.status.success());
    assert!(!path.join(".git/yoke-graph-hooks").exists());
    let installed = hooks(
        path,
        &[
            "install",
            "--trust-git",
            "--allow-hook-write",
            "--hooks-dir",
            custom.to_str().unwrap(),
        ],
    );
    assert_eq!(success(&installed)["state"], "active");
    assert!(custom.join("post-commit").is_file());
    git(path, &["commit", "--allow-empty", "-qm", "custom hook"]);
    assert!(has_label(&head_graph(path, "post-commit"), "obsolete"));
    git(path, &["config", "--local", "--unset", "core.hooksPath"]);
    assert_eq!(
        success(&hooks(path, &["status"]))["state"],
        "hook-directory-changed"
    );
    assert_eq!(
        success(&hooks(path, &["uninstall", "--allow-hook-write"]))["ownedHooks"],
        0
    );
    assert!(!custom.join("post-commit").exists());
}

#[test]
fn local_registration_drives_a_real_merge_and_preserves_unrelated_config() {
    let root = tempfile::Builder::new()
        .prefix("merge %P ' ")
        .tempdir()
        .unwrap();
    let path = root.path();
    seed_repository(path);
    fs::create_dir(path.join("graphs")).unwrap();
    let graph = "graphs/bill [Q]' π.json";
    let sibling = "graphs/bill Q' π.json";
    fs::write(path.join(graph), br#"{"nodes":[],"links":[]}"#).unwrap();
    fs::write(path.join(sibling), br#"{"nodes":[],"links":[]}"#).unwrap();
    git(path, &["add", "--", "graphs"]);
    git(path, &["commit", "-qm", "graph base"]);
    git(path, &["branch", "topic"]);
    let attrs = path.join(".git/info/attributes");
    let original_attrs = b"# existing local policy\r\n*.txt text";
    fs::write(&attrs, original_attrs).unwrap();
    git(path, &["config", "--local", "yoke.fixture", "keep-me"]);
    let install = [
        "install",
        "--trust-git",
        "--allow-hook-write",
        "--union-graph",
        graph,
    ];
    let installed = success(&hooks(path, &install));
    assert_eq!(
        installed["unionDriver"],
        serde_json::json!({"path":graph,"attributeOwned":true,"driverOwned":true})
    );
    assert_eq!(
        success(&hooks(path, &install))["unionDriver"],
        installed["unionDriver"]
    );
    let sibling_attribute = git(path, &["check-attr", "-z", "merge", "--", sibling]);
    assert_eq!(
        sibling_attribute.stdout,
        format!("{sibling}\0merge\0unspecified\0").as_bytes()
    );

    let receipt: Value = serde_json::from_slice(
        &fs::read(path.join(".git/yoke-graph-hooks/installation.json")).unwrap(),
    )
    .unwrap();
    let scratch = [
        ".merge_file_aaaaaa",
        ".merge_file_bbbbbb",
        ".merge_file_cccccc",
    ];
    for file in scratch {
        fs::write(path.join(file), br#"{"nodes":[],"links":[]}"#).unwrap();
    }
    let unauthorized = hooks(
        path,
        &[
            "merge-union",
            receipt["installationId"].as_str().unwrap(),
            scratch[0],
            scratch[1],
            scratch[2],
            "--",
            sibling,
        ],
    );
    assert!(!unauthorized.status.success());
    assert!(String::from_utf8_lossy(&unauthorized.stderr).contains("unselected path"));
    assert_eq!(
        fs::read(path.join(scratch[1])).unwrap(),
        br#"{"nodes":[],"links":[]}"#
    );
    for file in scratch {
        fs::remove_file(path.join(file)).unwrap();
    }

    fs::write(path.join(graph), br#"{"nodes":[{"id":"ours"}],"links":[]}"#).unwrap();
    git(path, &["commit", "-qam", "ours"]);
    git(path, &["switch", "-q", "topic"]);
    fs::write(
        path.join(graph),
        br#"{"nodes":[{"id":"theirs"}],"edges":[]}"#,
    )
    .unwrap();
    git(path, &["commit", "-qam", "theirs"]);
    git(path, &["switch", "-q", "main"]);
    git(path, &["merge", "--no-edit", "topic"]);
    let merged: Value = serde_json::from_slice(&fs::read(path.join(graph)).unwrap()).unwrap();
    assert_eq!(
        merged["nodes"],
        serde_json::json!([{"id":"ours"},{"id":"theirs"}])
    );
    assert_eq!(head_graph(path, "post-merge")["directed"], true);
    let removed = success(&hooks(path, &["uninstall", "--allow-hook-write"]));
    assert!(removed.get("unionDriver").is_none());
    assert_eq!(fs::read(&attrs).unwrap(), original_attrs);
    assert!(!path.join(".gitattributes").exists());
    assert_eq!(
        git(path, &["config", "--local", "--get", "yoke.fixture"]).stdout,
        b"keep-me\n"
    );
    let attribute = git(path, &["check-attr", "-z", "merge", "--", graph]);
    assert_eq!(
        attribute.stdout,
        format!("{graph}\0merge\0unspecified\0").as_bytes()
    );
}

#[test]
fn foreign_driver_setting_prevents_registration_and_hook_activation() {
    let root = root();
    let path = root.path();
    fs::write(path.join("graph.json"), br#"{"nodes":[],"links":[]}"#).unwrap();
    git(path, &["add", "--", "graph.json"]);
    git(path, &["commit", "-qm", "graph"]);
    git(
        path,
        &["config", "--local", "merge.yoke-graphify.driver", "false"],
    );
    let refused = hooks(
        path,
        &[
            "install",
            "--trust-git",
            "--allow-hook-write",
            "--union-graph",
            "graph.json",
        ],
    );
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("foreign or duplicated"));
    assert!(!path.join(".git/yoke-graph-hooks/active").exists());
    assert!(!path.join(".git/hooks/post-merge").exists());
    assert!(!path.join(".git/info/attributes").exists());
    assert_eq!(
        git(
            path,
            &["config", "--local", "--get", "merge.yoke-graphify.driver"]
        )
        .stdout,
        b"false\n"
    );
}
