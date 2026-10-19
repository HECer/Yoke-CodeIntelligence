//! Actual owned Git histories; the mapped digest itself remains read-only.
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    process::{Command, Output},
};
use yoke::{
    Engine,
    analysis::AnalyzerEpoch,
    capture::Deadline,
    git_workflows::{self, HostGit},
    github_app::{HostProfile, digest},
    model::Snapshot,
};

const FIRST: &str = "// café\r\npub fn stable() -> u32 { 1 }\r\npub fn changed() -> u32 { 1 }\r\npub mod nested { pub fn same(){} }\r\npub mod other { pub fn same(){} }\r\npub static LIMIT: u32 = 1;\r\n";
fn git(host: &HostGit, root: &Path, args: &[&str]) -> String {
    let null = if cfg!(windows) { "NUL" } else { "/dev/null" };
    let out = Command::new(&host.executable)
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", null)
        .env("GIT_CONFIG_SYSTEM", null)
        .args([
            "-c",
            "init.templateDir=",
            "-c",
            "core.autocrlf=false",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "user.name=Mapping Fixture",
            "-c",
            "user.email=fixture@example.invalid",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "owned Git setup: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().into()
}
fn profile() -> HostProfile {
    HostProfile::parse(&serde_json::to_vec(&profile_value()).unwrap()).unwrap()
}
fn profile_value() -> Value {
    json!({"schemaVersion":1,"appId":123,"secretCredential":"must-not-be-read",
        "grants":[{"installationId":42,"repositoryId":9,"repository":"acme/demo"}]})
}
struct Fixture {
    root: tempfile::TempDir,
    engine: Engine,
    git: HostGit,
    origin: Snapshot,
    snapshots: Vec<Snapshot>,
    commits: Vec<String>,
}
impl Fixture {
    fn new() -> Self {
        Self::with_format("sha1")
    }
    fn with_format(format: &str) -> Self {
        let root = tempfile::tempdir().unwrap();
        let host = HostGit::discover().unwrap();
        git(
            &host,
            root.path(),
            &["init", "-q", &format!("--object-format={format}")],
        );
        fs::write(root.path().join(".gitignore"), ".yoke/\n").unwrap();
        fs::write(root.path().join("café.rs"), FIRST).unwrap();
        fs::write(root.path().join("gone.rs"), "pub fn gone() {}\n").unwrap();
        fs::write(root.path().join("move.rs"), "pub fn moved() -> u32 { 7 }\n").unwrap();
        git(&host, root.path(), &["add", "--all"]);
        git(&host, root.path(), &["commit", "-q", "-m", "first"]);
        let first = git(&host, root.path(), &["rev-parse", "HEAD"]);
        let second_source = FIRST.replace("changed() -> u32 { 1 }", "changed() -> u32 { 2 }")
            + "pub fn added() {}\r\n";
        fs::write(root.path().join("café.rs"), &second_source).unwrap();
        fs::remove_file(root.path().join("gone.rs")).unwrap();
        fs::rename(root.path().join("move.rs"), root.path().join("new.rs")).unwrap();
        git(&host, root.path(), &["add", "--all"]);
        git(&host, root.path(), &["commit", "-q", "-m", "second"]);
        let second = git(&host, root.path(), &["rev-parse", "HEAD"]);
        fs::write(
            root.path().join("café.rs"),
            format!(
                "// shifted café bytes\r\n{}",
                second_source.replace("LIMIT: u32 = 1", "LIMIT: u32 = 2")
            ),
        )
        .unwrap();
        git(&host, root.path(), &["add", "--all"]);
        git(
            &host,
            root.path(),
            &["update-index", "--chmod=+x", "--", "café.rs"],
        );
        git(&host, root.path(), &["commit", "-q", "-m", "third"]);
        let third = git(&host, root.path(), &["rev-parse", "HEAD"]);
        let mut engine = Engine::open(root.path()).unwrap();
        let d = Deadline::new(30_000);
        let origin = engine.index(&d).unwrap().0;
        let commits = vec![first, second, third];
        let snapshots = commits
            .iter()
            .map(|id| {
                git_workflows::capture_commit_at(
                    AnalyzerEpoch::V8,
                    &mut engine,
                    &host,
                    &origin,
                    id,
                    &d,
                )
                .unwrap()
                .0
            })
            .collect();
        Self {
            root,
            engine,
            git: host,
            origin,
            snapshots,
            commits,
        }
    }
    fn request(&self) -> digest::PreviewRequest {
        digest::PreviewRequest {
            schema_version: 1,
            snapshot_id: self.snapshots[2].id.clone(),
            installation_id: 42,
            repository_id: 9,
            repository: "acme/demo".into(),
        }
    }
    fn selection(&self) -> digest::MappingSelection {
        digest::MappingSelection {
            snapshot_ids: self.snapshots[..2].iter().map(|s| s.id.clone()).collect(),
            limits: Default::default(),
        }
    }
    fn run(
        &self,
        options: &digest::Options,
        selection: &digest::MappingSelection,
    ) -> anyhow::Result<Value> {
        digest::preview_mapped_history_with_profile(
            &self.engine.store,
            &profile(),
            options,
            &self.request(),
            &self.git,
            &Default::default(),
            selection,
            &Deadline::new(30_000),
        )
    }
    fn capture_current(&mut self, epoch: AnalyzerEpoch) -> Snapshot {
        let id = git(&self.git, self.root.path(), &["rev-parse", "HEAD"]);
        git_workflows::capture_commit_at(
            epoch,
            &mut self.engine,
            &self.git,
            &self.origin,
            &id,
            &Deadline::new(30_000),
        )
        .unwrap()
        .0
    }
}
fn refused(value: anyhow::Result<Value>, marker: &str) {
    let Err(error) = value else {
        panic!("operation unexpectedly succeeded");
    };
    let error = format!("{error:#}");
    assert!(error.contains(marker), "{error}");
    assert!(!error.contains("UNTRUSTED_BODY_CANARY"));
}
fn cas(f: &Fixture) -> BTreeMap<String, String> {
    fs::read_dir(f.engine.store.dir.join("blobs"))
        .unwrap()
        .map(|e| {
            let e = e.unwrap();
            (
                e.file_name().into_string().unwrap(),
                yoke::model::hash(fs::read(e.path()).unwrap()),
            )
        })
        .collect()
}
fn commit<'a>(value: &'a Value, sha: &str) -> &'a Value {
    value["commits"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["sha"] == sha)
        .unwrap()
}

#[test]
fn exact_git_file_relations_and_qualified_symbol_candidates_keep_original_anchors() {
    let f = Fixture::new();
    let changes = f.engine.store.db.total_changes();
    let before = cas(&f);
    fs::write(f.root.path().join("café.rs"), "not the captured source").unwrap();
    let value = f.run(&Default::default(), &f.selection()).unwrap();
    assert_eq!(
        value["kind"],
        "native_captured_repository_digest_mapped_history_preview"
    );
    assert_eq!(value["historyMapping"]["mappedCommits"], 3);
    assert_eq!(value["historyMapping"]["unavailableCommits"], 0);
    assert_eq!(value["historyMapping"]["complete"], false);
    assert_eq!(value["historyMapping"]["allTreeChangesEnumerated"], false);
    assert_eq!(value["sourceWritten"], false);
    assert_eq!(value["networkRequests"], 0);
    assert_eq!(value["modelCalls"], 0);
    let row_bytes: usize = value["commits"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|commit| {
            commit["files"]
                .as_array()
                .unwrap()
                .iter()
                .chain(commit["symbols"].as_array().unwrap())
        })
        .map(|row| serde_json::to_vec(row).unwrap().len())
        .sum();
    assert_eq!(value["historyMapping"]["relationRowBytes"], row_bytes);
    let middle = commit(&value, &f.commits[1]);
    let files = middle["files"].as_array().unwrap();
    for (path, kind) in [
        ("gone.rs", "deleted"),
        ("move.rs", "deleted"),
        ("new.rs", "added"),
        ("café.rs", "modified"),
    ] {
        assert!(
            files
                .iter()
                .any(|v| v["path"] == path && v["change"] == kind),
            "{files:?}"
        );
    }
    let moved = middle["symbols"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| {
            r["before"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v["name"] == "moved")
        })
        .unwrap();
    assert_eq!(moved["before"][0]["path"], "move.rs");
    assert_eq!(moved["after"][0]["path"], "new.rs");
    assert_eq!(moved["basis"], "captured_declaration_fingerprint");
    assert_eq!(moved["declarationBytesEqual"], true);
    assert_eq!(moved["semanticIdentityEstablished"], false);
    let changed = middle["symbols"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["before"][0]["name"] == "changed")
        .unwrap();
    assert_eq!(changed["declarationBytesEqual"], false);
    assert_eq!(
        changed["before"][0]["signature"],
        changed["after"][0]["signature"]
    );
    let names: Vec<_> = middle["symbols"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|r| r["after"].as_array().unwrap())
        .filter(|v| v["name"] == "same")
        .map(|v| v["qualifiedName"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"nested::same") && names.contains(&"other::same"));
    let current = commit(&value, &f.commits[2]);
    assert!(
        current["files"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v["path"] != "new.rs")
    );
    let file = current["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["path"] == "café.rs")
        .unwrap();
    assert_eq!(file["before"]["gitMode"], "100644");
    assert_eq!(file["after"]["gitMode"], "100755");
    let stable = current["symbols"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["before"][0]["name"] == "stable")
        .unwrap();
    assert_eq!(stable["declarationBytesEqual"], true);
    assert_ne!(
        stable["before"][0]["startByte"],
        stable["after"][0]["startByte"]
    );
    assert_eq!(stable["before"][0]["snapshotId"], f.snapshots[1].id);
    assert_eq!(stable["after"][0]["snapshotId"], f.snapshots[2].id);
    assert_eq!(cas(&f), before);
    assert_eq!(f.engine.store.db.total_changes(), changes);
    assert_eq!(
        fs::read_to_string(f.root.path().join("café.rs")).unwrap(),
        "not the captured source"
    );
}

#[test]
fn missing_captures_are_explicit_and_legacy_history_does_not_infer_them() {
    let f = Fixture::new();
    let selection = digest::MappingSelection {
        snapshot_ids: vec![],
        limits: Default::default(),
    };
    let value = f.run(&Default::default(), &selection).unwrap();
    assert_eq!(
        commit(&value, &f.commits[2])["filesStatus"],
        "unavailable_parent_snapshot"
    );
    assert_eq!(
        commit(&value, &f.commits[0])["symbolsStatus"],
        "unavailable_commit_snapshot"
    );
    assert_eq!(value["historyMapping"]["unavailableCommits"], 3);
    let legacy = digest::preview_history_with_profile(
        &f.engine.store,
        &profile(),
        &Default::default(),
        &f.request(),
        &f.git,
        &Default::default(),
        &Deadline::new(30_000),
    )
    .unwrap();
    assert!(legacy.get("historyMapping").is_none());
    assert!(
        legacy["commits"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v["filesStatus"] == "unavailable" && v["files"] == json!([]))
    );
}

#[test]
fn sha256_git_objects_and_mixed_supported_capture_epochs_retain_their_provenance() {
    let mut f = Fixture::with_format("sha256");
    assert!(f.commits.iter().all(|id| id.len() == 64));
    git(
        &f.git,
        f.root.path(),
        &["checkout", "-q", "--detach", &f.commits[1]],
    );
    f.snapshots[1] = f.capture_current(AnalyzerEpoch::V9);
    let value = f.run(&Default::default(), &f.selection()).unwrap();
    assert_eq!(value["historyMapping"]["mappedCommits"], 3);
    let snapshots = value["historyMapping"]["snapshots"].as_array().unwrap();
    assert!(
        snapshots
            .iter()
            .any(|s| s["analyzer"] == AnalyzerEpoch::V9.producer())
    );
    assert!(
        snapshots
            .iter()
            .any(|s| s["analyzer"] == AnalyzerEpoch::V8.producer())
    );
    for row in value["commits"].as_array().unwrap()[1]["symbols"]
        .as_array()
        .unwrap()
    {
        assert_eq!(row["semanticIdentityEstablished"], false);
    }
}

#[test]
fn aggregate_source_metadata_ast_and_export_budgets_cannot_reset_per_snapshot() {
    let f = Fixture::new();
    let full = f.run(&Default::default(), &f.selection()).unwrap();
    let m = &full["historyMapping"];
    for field in [
        "sourceBytesVerified",
        "capturedFilesVerified",
        "selectedMetadataBytesAdmitted",
        "lineageMetadataBytesAdmitted",
        "astCapacityCharged",
        "entityCapacityCharged",
        "replayNameBytes",
    ] {
        let limit = m[field].as_u64().unwrap() as usize - 1;
        let mut options = digest::Options::default();
        match field {
            "sourceBytesVerified" => options.max_source_bytes = limit,
            "capturedFilesVerified" => options.max_files = limit,
            "selectedMetadataBytesAdmitted" => options.max_metadata_bytes = limit,
            "lineageMetadataBytesAdmitted" => options.max_lineage_metadata_bytes = limit,
            "astCapacityCharged" => options.max_ast_nodes = limit,
            "entityCapacityCharged" => options.max_entities = limit,
            "replayNameBytes" => options.max_replay_name_bytes = limit,
            _ => unreachable!(),
        }
        digest::preview_with_profile(
            &f.engine.store,
            &profile(),
            &options,
            &f.request(),
            &Deadline::new(30_000),
        )
        .expect("the head alone must fit this aggregate boundary");
        refused(
            f.run(&options, &f.selection()),
            "github_app_digest_capacity_exceeded",
        );
    }
    let total: usize = f
        .snapshots
        .iter()
        .map(|s| {
            let r = digest::PreviewRequest {
                snapshot_id: s.id.clone(),
                ..f.request()
            };
            digest::preview_with_profile(
                &f.engine.store,
                &profile(),
                &Default::default(),
                &r,
                &Deadline::new(30_000),
            )
            .unwrap()["symbols"]
                .as_array()
                .unwrap()
                .len()
        })
        .sum();
    let options = digest::Options {
        max_symbols: total - 1,
        ..Default::default()
    };
    refused(
        f.run(&options, &f.selection()),
        "github_app_digest_capacity_exceeded",
    );
}

#[test]
fn relation_work_and_commit_proof_limits_refuse_without_partial_results() {
    let f = Fixture::new();
    let full = f.run(&Default::default(), &f.selection()).unwrap();
    for key in [
        "fileRelations",
        "symbolRelations",
        "comparisons",
        "commitProofBytesAdmitted",
    ] {
        let mut selection = f.selection();
        let limit = full["historyMapping"][key].as_u64().unwrap() as usize - 1;
        match key {
            "fileRelations" => selection.limits.max_file_relations = limit,
            "symbolRelations" => selection.limits.max_symbol_relations = limit,
            "comparisons" => selection.limits.max_comparisons = limit,
            "commitProofBytesAdmitted" => selection.limits.max_proof_bytes = limit,
            _ => unreachable!(),
        }
        refused(
            f.run(&Default::default(), &selection),
            "github_app_digest_capacity_exceeded",
        );
    }
    let mut selection = f.selection();
    selection.limits.max_proof_bytes = 0;
    refused(
        f.run(&Default::default(), &selection),
        "github_app_digest_capacity_exceeded",
    );
    selection = f.selection();
    selection.limits.max_snapshots = 2;
    refused(
        f.run(&Default::default(), &selection),
        "github_app_digest_capacity_exceeded",
    );
}

#[test]
fn revoked_grants_invalid_duplicate_and_head_selectors_are_rejected_before_git_execution() {
    let f = Fixture::new();
    let bad = HostGit {
        executable: f.root.path().join("must-not-run.exe"),
    };
    for ids in [
        vec![f.snapshots[0].id.clone(), f.snapshots[0].id.clone()],
        vec![f.snapshots[2].id.clone()],
        vec!["../UNTRUSTED_BODY_CANARY".into()],
    ] {
        let selection = digest::MappingSelection {
            snapshot_ids: ids,
            limits: Default::default(),
        };
        refused(
            digest::preview_mapped_history_with_profile(
                &f.engine.store,
                &profile(),
                &Default::default(),
                &f.request(),
                &bad,
                &Default::default(),
                &selection,
                &Deadline::new(30_000),
            ),
            "github_app_digest_input_invalid",
        );
    }
    let mut revoked = profile_value();
    revoked["grants"][0]["repositoryId"] = json!(10);
    let revoked = HostProfile::parse(&serde_json::to_vec(&revoked).unwrap()).unwrap();
    refused(
        digest::preview_mapped_history_with_profile(
            &f.engine.store,
            &revoked,
            &Default::default(),
            &f.request(),
            &bad,
            &Default::default(),
            &f.selection(),
            &Deadline::new(30_000),
        ),
        "github_app_digest_grant_denied",
    );
}

#[test]
fn unrelated_commits_disk_sources_and_ambiguous_captures_never_enter_the_history() {
    let mut f = Fixture::new();
    git(
        &f.git,
        f.root.path(),
        &["checkout", "-q", "-b", "other", &f.commits[0]],
    );
    fs::write(f.root.path().join("other.rs"), "pub fn unrelated() {}\n").unwrap();
    git(&f.git, f.root.path(), &["add", "--all"]);
    git(&f.git, f.root.path(), &["commit", "-q", "-m", "other"]);
    let other = f.capture_current(AnalyzerEpoch::V8);
    let mut selection = f.selection();
    selection.snapshot_ids.push(other.id);
    refused(
        f.run(&Default::default(), &selection),
        "github_app_digest_source_unavailable",
    );
    selection = f.selection();
    selection.snapshot_ids.push(f.origin.id.clone());
    refused(
        f.run(&Default::default(), &selection),
        "github_app_digest_source_unavailable",
    );
    git(
        &f.git,
        f.root.path(),
        &["checkout", "-q", "--detach", &f.commits[0]],
    );
    let duplicate = f.capture_current(AnalyzerEpoch::V9);
    selection = f.selection();
    selection.snapshot_ids.push(duplicate.id);
    refused(
        f.run(&Default::default(), &selection),
        "github_app_digest_source_unavailable",
    );
}

#[test]
fn corrupt_additional_proofs_and_cancelled_work_restore_the_callers_sql_policy() {
    let f = Fixture::new();
    let limit = rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH;
    f.engine.store.db.set_limit(limit, 120_000_000).unwrap();
    f.engine
        .store
        .db
        .busy_timeout(std::time::Duration::from_millis(71))
        .unwrap();
    let raw: String = f
        .engine
        .store
        .db
        .query_row(
            "SELECT value FROM graphs WHERE id=?1",
            [&f.snapshots[0].id],
            |r| r.get(0),
        )
        .unwrap();
    let mut graph: Value = serde_json::from_str(&raw).unwrap();
    let entity = graph["entities"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|e| e["path"] == "café.rs" && e["kind"] == "function")
        .unwrap();
    entity["name"] = json!("UNTRUSTED_BODY_CANARY");
    f.engine
        .store
        .db
        .execute(
            "UPDATE graphs SET value=?1 WHERE id=?2",
            rusqlite::params![serde_json::to_string(&graph).unwrap(), &f.snapshots[0].id],
        )
        .unwrap();
    let before = cas(&f);
    let changes = f.engine.store.db.total_changes();
    refused(
        f.run(&Default::default(), &f.selection()),
        "github_app_digest_source_unavailable",
    );
    assert_eq!(f.engine.store.db.limit(limit).unwrap(), 120_000_000);
    assert_eq!(
        f.engine
            .store
            .db
            .query_row("PRAGMA busy_timeout", [], |r| r.get::<_, u64>(0))
            .unwrap(),
        71
    );
    assert!(f.engine.store.db.is_autocommit());
    assert_eq!(cas(&f), before);
    assert_eq!(f.engine.store.db.total_changes(), changes);
    refused(
        digest::preview_mapped_history_with_profile(
            &f.engine.store,
            &profile(),
            &Default::default(),
            &f.request(),
            &f.git,
            &Default::default(),
            &f.selection(),
            &Deadline::new(0),
        ),
        "deadline exceeded",
    );
    let canceled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    assert!(
        Deadline::run_bounded(&Deadline::new(30_000), canceled, || f
            .run(&Default::default(), &f.selection()))
        .is_err()
    );
}

#[test]
fn caller_transactions_are_refused_without_commit_or_rollback() {
    let f = Fixture::new();
    f.engine.store.db.execute_batch("BEGIN").unwrap();
    refused(
        f.run(&Default::default(), &f.selection()),
        "github_app_digest_caller_transaction_refused",
    );
    assert!(!f.engine.store.db.is_autocommit());
    f.engine.store.db.execute_batch("ROLLBACK").unwrap();
}

#[test]
fn duplicate_declarations_preserve_every_original_candidate_without_a_forced_identity() {
    let mut f = Fixture::new();
    let duplicate = "pub fn repeated() {}\npub fn repeated() {}\n";
    fs::write(f.root.path().join("duplicates.rs"), duplicate).unwrap();
    git(&f.git, f.root.path(), &["add", "--all"]);
    git(
        &f.git,
        f.root.path(),
        &["commit", "-q", "-m", "duplicate exports"],
    );
    f.snapshots[1] = f.capture_current(AnalyzerEpoch::V8);
    f.commits[1] = git(&f.git, f.root.path(), &["rev-parse", "HEAD"]);
    fs::write(
        f.root.path().join("duplicates.rs"),
        format!("// shifted café\n{duplicate}"),
    )
    .unwrap();
    git(&f.git, f.root.path(), &["add", "--all"]);
    git(
        &f.git,
        f.root.path(),
        &["commit", "-q", "-m", "shift duplicates"],
    );
    f.snapshots[2] = f.capture_current(AnalyzerEpoch::V8);
    f.commits[2] = git(&f.git, f.root.path(), &["rev-parse", "HEAD"]);
    let value = f.run(&Default::default(), &f.selection()).unwrap();
    let row = commit(&value, &f.commits[2])["symbols"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["before"][0]["name"] == "repeated")
        .unwrap();
    assert_eq!(row["relation"], "ambiguous_syntax_candidates");
    for side in ["before", "after"] {
        let values = row[side].as_array().unwrap();
        assert_eq!(values.len(), 2);
        assert_ne!(values[0]["id"], values[1]["id"]);
        assert_ne!(values[0]["startByte"], values[1]["startByte"]);
    }
    assert_eq!(row["semanticIdentityEstablished"], false);
    assert_eq!(row["declarationBytesEqual"], Value::Null);
}

#[test]
fn a_path_still_present_in_git_but_outside_the_new_capture_is_unavailable_not_deleted() {
    let mut f = Fixture::new();
    let selected = f.root.path().join("host-selected");
    fs::create_dir(&selected).unwrap();
    fs::write(selected.join("local.rs"), "pub fn local_only() {}\n").unwrap();
    f.engine.capture_options.only_dirs = vec!["host-selected".into()];
    let d = Deadline::new(30_000);
    let restricted = f.engine.index(&d).unwrap().0;
    f.snapshots[2] = git_workflows::capture_commit_at(
        AnalyzerEpoch::V8,
        &mut f.engine,
        &f.git,
        &restricted,
        &f.commits[2],
        &d,
    )
    .unwrap()
    .0;
    assert!(f.snapshots[2].files.is_empty());
    let value = f.run(&Default::default(), &f.selection()).unwrap();
    let current = commit(&value, &f.commits[2]);
    assert!(
        current["files"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["change"] != "deleted")
    );
    let stable = current["symbols"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["before"][0]["name"] == "stable")
        .unwrap();
    assert_eq!(stable["relation"], "unavailable_after_export_projection");
    assert_eq!(
        stable["before"][0]["counterpartSourceStatus"],
        "uncaptured_source"
    );
    assert!(stable["after"].as_array().unwrap().is_empty());
}

fn cli(f: &Fixture, profile: &Path, request: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_yoke"));
    command
        .arg("--root")
        .arg(f.root.path())
        .args(["github-app", "--profile"])
        .arg(profile)
        .args([
            "digest",
            "--allow-digest",
            "history",
            "--allow-history",
            "--trust-git",
            "--request",
        ])
        .arg(request);
    command
}
fn output_refused(output: Output, marker: &str) {
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let text = String::from_utf8(output.stderr).unwrap();
    assert!(text.contains(marker), "{text}");
    assert!(!text.contains("UNTRUSTED_BODY_CANARY"));
}
#[test]
fn actual_cli_maps_only_explicit_selectors_and_honors_exact_output_capacity() {
    let f = Fixture::new();
    let host = tempfile::tempdir().unwrap();
    let p = host.path().join("profile.json");
    let r = host.path().join("request.json");
    fs::write(&p, serde_json::to_vec(&profile_value()).unwrap()).unwrap();
    fs::write(&r, serde_json::to_vec(&f.request()).unwrap()).unwrap();
    let before = cas(&f);
    let mut command = cli(&f, &p, &r);
    command.args(["--map-captured-history", "--max-bytes", "4000000"]);
    for s in &f.snapshots[..2] {
        command.args(["--captured-snapshot", &s.id]);
    }
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["historyMapping"]["mappedCommits"], 3);
    let raw = output.stdout.len();
    let mut exact = cli(&f, &p, &r);
    exact.args(["--max-bytes", &raw.to_string(), "--map-captured-history"]);
    for s in &f.snapshots[..2] {
        exact.args(["--captured-snapshot", &s.id]);
    }
    let exact = exact.output().unwrap();
    assert!(
        exact.status.success(),
        "{}",
        String::from_utf8_lossy(&exact.stderr)
    );
    assert_eq!(exact.stdout.len(), raw);
    let mut small = cli(&f, &p, &r);
    small.args([
        "--max-bytes",
        &(raw - 1).to_string(),
        "--map-captured-history",
    ]);
    for s in &f.snapshots[..2] {
        small.args(["--captured-snapshot", &s.id]);
    }
    output_refused(
        small.output().unwrap(),
        "github_app_digest_capacity_exceeded",
    );
    output_refused(
        cli(&f, &p, &r)
            .args(["--captured-snapshot", &f.snapshots[0].id])
            .output()
            .unwrap(),
        "--map-captured-history",
    );
    assert_eq!(cas(&f), before);
    let mut request = serde_json::to_value(f.request()).unwrap();
    request["capturedSnapshotIds"] = json!([f.snapshots[0].id]);
    fs::write(&r, serde_json::to_vec(&request).unwrap()).unwrap();
    output_refused(
        cli(&f, &p, &r)
            .arg("--map-captured-history")
            .output()
            .unwrap(),
        "github_app_digest_input_invalid",
    );
    assert_eq!(cas(&f), before);
}

#[test]
fn mapped_cli_never_turns_its_selection_flag_into_digest_history_or_git_permission() {
    let host = tempfile::tempdir().unwrap();
    let root = host.path().join("must-stay-absent");
    for (flags, marker) in [
        (vec![], "github_app_digest_permission_required"),
        (
            vec!["--allow-digest"],
            "github_app_digest_history_permission_required",
        ),
        (
            vec!["--allow-digest", "--allow-history"],
            "github_app_digest_history_trust_required",
        ),
    ] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_yoke"));
        command
            .arg("--root")
            .arg(&root)
            .args(["github-app", "digest", "history", "--map-captured-history"])
            .args(flags)
            .arg("--profile")
            .arg(host.path().join("missing-profile.json"))
            .arg("--request")
            .arg(host.path().join("missing-request.json"));
        output_refused(command.output().unwrap(), marker);
        assert!(!root.exists());
        assert_eq!(fs::read_dir(host.path()).unwrap().count(), 0);
    }
}
