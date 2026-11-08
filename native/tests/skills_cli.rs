use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};

fn call(
    repository: &Path,
    personal: &Path,
    operation: &str,
    args: Value,
    writable: bool,
) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_yoke"));
    command
        .arg("--root")
        .arg(repository)
        .args(["skills", operation, "--personal-root"])
        .arg(personal)
        .args([
            "--project",
            &yoke::model::hash(b"cli authored skills"),
            "--args",
            &args.to_string(),
        ]);
    if writable {
        command.args(["--allow-personal-writes", "--allow-writes"]);
    }
    command.output().unwrap()
}

fn result(output: std::process::Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn public_cli_preserves_authored_history_through_confirm_fold_and_publish() {
    let repository = tempfile::tempdir().unwrap();
    let personal = tempfile::tempdir().unwrap();
    let learn = json!({"skill":"Captured Unicode", "text":"Keep 世界 and the exact source hash.","author":"Ada","date":"2026-10-15","expectedRevision":null});
    assert!(
        !call(
            repository.path(),
            personal.path(),
            "learn",
            learn.clone(),
            false
        )
        .status
        .success()
    );
    let learned = result(call(
        repository.path(),
        personal.path(),
        "learn",
        learn,
        true,
    ));
    let confirmed = result(call(
        repository.path(),
        personal.path(),
        "confirm",
        json!({"skill":"captured-unicode","author":"Grace","expectedRevision":learned["revisionId"]}),
        true,
    ));
    let folded = result(call(
        repository.path(),
        personal.path(),
        "fold",
        json!({"skill":"captured-unicode","confirmed":true,"expectedRevision":confirmed["revisionId"],"expectedFileHash":confirmed["contentHash"]}),
        true,
    ));
    assert_eq!(folded["skill"]["version"], 2);
    let published = result(call(
        repository.path(),
        personal.path(),
        "publish",
        json!({"skill":"captured-unicode","expectedRevision":folded["revisionId"],"expectedFileHash":folded["contentHash"]}),
        true,
    ));
    assert_eq!(published["skill"]["where"], "repo");
    assert_eq!(
        published["skill"]["takeaways"][0]["confirmedBy"],
        json!(["Grace"])
    );
    let file = repository
        .path()
        .join(".claude/skills/captured-unicode/SKILL.md");
    let bytes = fs::read(&file).unwrap();
    assert!(String::from_utf8_lossy(&bytes).contains("Keep 世界 and the exact source hash."));
    let shown = result(call(
        repository.path(),
        personal.path(),
        "show",
        json!({"skill":"captured-unicode"}),
        false,
    ));
    assert_eq!(shown["revisionId"], published["revisionId"]);
    let list = result(call(
        repository.path(),
        personal.path(),
        "list",
        json!({}),
        false,
    ));
    assert_eq!(list["count"], 1);
    assert_eq!(fs::read(file).unwrap(), bytes);
    assert!(
        !repository.path().join(".yoke").exists(),
        "Skill operations must not capture or initialize the source repository"
    );
}

#[test]
fn cli_rejects_untrusted_roots_and_source_options_before_initializing_state() {
    let repository = tempfile::tempdir().unwrap();
    let personal = tempfile::tempdir().unwrap();
    for extra in [
        vec!["--snapshot", "invalid"],
        vec!["--include-ignored-dir", "vendor"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_yoke"))
            .arg("--root")
            .arg(repository.path())
            .args(["skills", "list", "--personal-root"])
            .arg(personal.path())
            .args(["--project", &yoke::model::hash(b"cli authored skills")])
            .args(extra)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(!repository.path().join(".yoke").exists());
        assert!(!personal.path().join(".yoke").exists());
    }
    let output = call(
        repository.path(),
        Path::new("relative-personal"),
        "list",
        json!({}),
        false,
    );
    assert!(!output.status.success());
    assert!(!repository.path().join(".yoke").exists());
}

#[test]
fn small_output_budget_retains_the_complete_mutation_receipt_in_the_personal_store() {
    let repository = tempfile::tempdir().unwrap();
    let personal = tempfile::tempdir().unwrap();
    let args = json!({"skill":"Long receipt", "text":"Keep the captured source and its exact hash. ".repeat(100), "author":"Ada", "expectedRevision":null});
    let output = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(repository.path())
        .args(["skills", "learn", "--personal-root"])
        .arg(personal.path())
        .args([
            "--project",
            &yoke::model::hash(b"cli authored skills"),
            "--args",
            &args.to_string(),
            "--allow-personal-writes",
            "--max-bytes",
            "1024",
        ])
        .output()
        .unwrap();
    let receipt = result(output);
    assert_eq!(receipt["status"], "partial");
    let store = yoke::store::Store::open(personal.path()).unwrap();
    let complete = store
        .read_artifact_limited(receipt["artifactId"].as_str().unwrap(), 4_000_000)
        .unwrap();
    let complete: Value = serde_json::from_slice(&complete).unwrap();
    assert_eq!(complete["skill"]["takeaways"][0]["text"], args["text"]);
    let shown = result(call(
        repository.path(),
        personal.path(),
        "show",
        json!({"skill":"long-receipt"}),
        false,
    ));
    assert_eq!(shown["revisionId"], complete["revisionId"]);
    assert!(!repository.path().join(".yoke").exists());
}

#[test]
fn folding_the_library_preflights_every_revision_before_changing_the_first_skill() {
    let repository = tempfile::tempdir().unwrap();
    let personal = tempfile::tempdir().unwrap();
    let mut revisions = serde_json::Map::new();
    let mut hashes = serde_json::Map::new();
    for name in ["alpha", "beta"] {
        let learned = result(call(
            repository.path(),
            personal.path(),
            "learn",
            json!({"skill":name,"text":"Keep the captured evidence.","author":"Ada","expectedRevision":null}),
            true,
        ));
        revisions.insert(name.into(), learned["revisionId"].clone());
        hashes.insert(name.into(), learned["contentHash"].clone());
    }
    let mut stale = revisions.clone();
    stale.insert("beta".into(), json!("f".repeat(64)));
    assert!(
        !call(
            repository.path(),
            personal.path(),
            "fold",
            json!({"expectedRevisions":stale,"expectedFileHashes":hashes}),
            true
        )
        .status
        .success()
    );
    for name in ["alpha", "beta"] {
        let shown = result(call(
            repository.path(),
            personal.path(),
            "show",
            json!({"skill":name}),
            false,
        ));
        assert_eq!(shown["skill"]["version"], 1);
        assert_eq!(shown["revisionId"], revisions[name]);
    }
    let folded = result(call(
        repository.path(),
        personal.path(),
        "fold",
        json!({"expectedRevisions":revisions,"expectedFileHashes":hashes}),
        true,
    ));
    assert_eq!(folded["count"], 2);
    assert_eq!(folded["atomicAcrossSkills"], false);
    for item in folded["results"].as_array().unwrap() {
        assert_eq!(item["from"], 1);
        assert_eq!(item["to"], 2);
        assert_eq!(item["folded"].as_array().unwrap().len(), 1);
    }
    assert!(!repository.path().join(".yoke").exists());
}

#[test]
fn host_cli_observes_missing_revision_guards_and_preserves_explicit_stale_guards() {
    let repository = tempfile::tempdir().unwrap();
    let personal = tempfile::tempdir().unwrap();
    let first = result(call(
        repository.path(),
        personal.path(),
        "learn",
        json!({"skill":"Convenient","text":"Keep source hashes.","author":"Ada"}),
        true,
    ));
    let next = result(call(
        repository.path(),
        personal.path(),
        "learn",
        json!({"skill":"Convenient","text":"Keep UTF-8 byte ranges.","author":"Grace"}),
        true,
    ));
    assert_ne!(first["revisionId"], next["revisionId"]);
    assert!(
        !call(
            repository.path(),
            personal.path(),
            "confirm",
            json!({"skill":"convenient","author":"Lin","expectedRevision":first["revisionId"]}),
            true
        )
        .status
        .success()
    );
    let shown = result(call(
        repository.path(),
        personal.path(),
        "show",
        json!({"skill":"convenient"}),
        false,
    ));
    assert_eq!(shown["revisionId"], next["revisionId"]);
    let folded = result(call(
        repository.path(),
        personal.path(),
        "fold",
        json!({"skill":"convenient"}),
        true,
    ));
    assert_eq!(folded["skill"]["version"], 2);
    assert_eq!(folded["folded"].as_array().unwrap().len(), 2);
    assert!(!repository.path().join(".yoke").exists());
}
