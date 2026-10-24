//! Actual owned Git objects and SQLite/CAS publication; no provider/network/key fixtures.
use serde_json::{Value, json};
use std::{path::Path, process::Command};
use yoke::{
    Engine,
    analysis::AnalyzerEpoch,
    capture::Deadline,
    git_workflows::{self, HostGit},
    github_app::{
        HostProfile,
        digest::{self, acquisition},
    },
    model::Snapshot,
};
fn git(g: &HostGit, root: &Path, args: &[&str]) -> String {
    let null = if cfg!(windows) { "NUL" } else { "/dev/null" };
    let out = Command::new(&g.executable)
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
            "user.name=Acquisition Fixture",
            "-c",
            "user.email=fixture@example.invalid",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().into()
}
fn profile_value() -> Value {
    json!({"schemaVersion":1,"appId":123,"secretCredential":"acquisition-no-such-key","grants":[{"installationId":42,"repositoryId":9,"repository":"org/repo"}]})
}
struct Fixture {
    root: tempfile::TempDir,
    engine: Engine,
    git: HostGit,
    origin: Snapshot,
    head: Snapshot,
    commits: Vec<String>,
    profile: HostProfile,
    request: digest::PreviewRequest,
}
impl Fixture {
    fn new() -> Self {
        Self::with("sha1", AnalyzerEpoch::V8)
    }
    fn with(format: &str, epoch: AnalyzerEpoch) -> Self {
        let root = tempfile::tempdir().unwrap();
        let g = HostGit::discover().unwrap();
        git(
            &g,
            root.path(),
            &["init", "-q", &format!("--object-format={format}")],
        );
        std::fs::write(root.path().join(".gitignore"), ".yoke/\n").unwrap();
        let mut commits = vec![];
        for (i, label) in ["first", "second", "third"].iter().enumerate() {
            std::fs::write(root.path().join("café.rs"),format!("// café CRLF\r\npub fn value()->u32 {{ {} }}\r\npub mod nested {{ pub fn same(){{}} }}\r\n",i+1)).unwrap();
            git(&g, root.path(), &["add", "--all"]);
            git(&g, root.path(), &["commit", "-q", "-m", label]);
            commits.push(git(&g, root.path(), &["rev-parse", "HEAD"]));
        }
        let d = Deadline::new(30000);
        let mut engine = Engine::open(root.path()).unwrap();
        engine.capture_epoch = epoch;
        let origin = engine.index(&d).unwrap().0;
        let head =
            git_workflows::capture_commit_at(epoch, &mut engine, &g, &origin, &commits[2], &d)
                .unwrap()
                .0;
        let profile = HostProfile::parse(&serde_json::to_vec(&profile_value()).unwrap()).unwrap();
        let request = digest::PreviewRequest {
            schema_version: 1,
            snapshot_id: head.id.clone(),
            installation_id: 42,
            repository_id: 9,
            repository: "org/repo".into(),
        };
        Self {
            root,
            engine,
            git: g,
            origin,
            head,
            commits,
            profile,
            request,
        }
    }
    fn run(&mut self, o: &acquisition::Options) -> anyhow::Result<Value> {
        acquisition::acquire(
            &mut self.engine,
            &self.profile,
            &digest::Options::default(),
            &self.request,
            &self.git,
            &digest::HistoryOptions::default(),
            o,
            &Deadline::new(30000),
        )
    }
    fn facts(&self) -> Value {
        let count = |t: &str| {
            self.engine
                .store
                .db
                .query_row(&format!("SELECT count(*) FROM {t}"), [], |r| {
                    r.get::<_, usize>(0)
                })
                .unwrap()
        };
        json!({"snapshots":count("snapshots"),"graphs":count("graphs"),"records":count("records"),"symbols":count("symbols"),"documents":count("documents"),"latest":self.engine.store.latest().unwrap()})
    }
}
fn allowed() -> acquisition::Options {
    acquisition::Options {
        allow_capture: true,
        allow_writes: true,
        ..Default::default()
    }
}
#[test]
fn real_sha1_and_sha256_history_is_captured_mapped_and_idempotent_with_dirty_checkout_and_epoch_inheritance()
 {
    for (format, epoch) in [("sha1", AnalyzerEpoch::V8), ("sha256", AnalyzerEpoch::V9)] {
        let mut f = Fixture::with(format, epoch);
        assert_eq!(f.head.id, f.request.snapshot_id);
        let latest = f.engine.store.latest().unwrap();
        std::fs::write(
            f.root.path().join("café.rs"),
            b"dirty checkout must not supply history",
        )
        .unwrap();
        std::fs::write(f.root.path().join("foreign.rs"), b"pub fn foreign(){}\n").unwrap();
        let index = std::fs::read(f.root.path().join(".git/index")).unwrap();
        let status = git(&f.git, f.root.path(), &["status", "--porcelain"]);
        let r = f.run(&allowed()).unwrap();
        assert_eq!(r["acquiredCaptures"].as_array().unwrap().len(), 2);
        assert_eq!(r["historyMapping"]["mappedCommits"], 3);
        assert_eq!(r["historyMapping"]["unavailableCommits"], 0);
        assert_eq!(r["analyzer"], epoch.producer());
        assert_eq!(r["sourceWritten"], false);
        assert_eq!(r["latestChanged"], false);
        assert_eq!(r["complete"], false);
        assert_eq!(r["networkRequests"], 0);
        assert_eq!(r["modelCalls"], 0);
        assert_eq!(f.engine.store.latest().unwrap(), latest);
        for capture in r["acquiredCaptures"].as_array().unwrap() {
            let id = capture["snapshotId"].as_str().unwrap();
            let snapshot = f.engine.store.snapshot(id).unwrap();
            let file = snapshot.files.iter().find(|p| p.path == "café.rs").unwrap();
            let raw = f
                .engine
                .store
                .read_blob_exact(&file.hash, file.size, 4_000_000)
                .unwrap();
            assert!(
                std::str::from_utf8(&raw)
                    .unwrap()
                    .contains("// café CRLF\r\n")
            );
            assert!(!snapshot.files.iter().any(|p| p.path == "foreign.rs"));
        }
        let ids = r["capturedSnapshotIds"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect();
        let preview = digest::preview_mapped_history_with_profile(
            &f.engine.store,
            &f.profile,
            &digest::Options::default(),
            &f.request,
            &f.git,
            &digest::HistoryOptions::default(),
            &digest::MappingSelection {
                snapshot_ids: ids,
                limits: Default::default(),
            },
            &Deadline::new(30000),
        )
        .unwrap();
        assert_eq!(preview["historyMapping"], r["historyMapping"]);
        assert_eq!(preview["commits"], r["commits"]);
        let before = f.facts();
        assert_eq!(f.run(&allowed()).unwrap(), r);
        assert_eq!(f.facts(), before);
        assert_eq!(
            std::fs::read(f.root.path().join(".git/index")).unwrap(),
            index
        );
        assert_eq!(
            git(&f.git, f.root.path(), &["status", "--porcelain"]),
            status
        );
        assert_eq!(
            std::fs::read(f.root.path().join("café.rs")).unwrap(),
            b"dirty checkout must not supply history"
        );
    }
}
#[test]
fn acquisition_default_permissions_scope_and_disk_heads_do_not_publish_partial_history() {
    let mut f = Fixture::new();
    let before = f.facts();
    assert!(
        f.run(&Default::default())
            .unwrap_err()
            .to_string()
            .contains("permission_required")
    );
    let mut o = allowed();
    o.allow_writes = false;
    assert!(f.run(&o).is_err());
    assert_eq!(f.facts(), before);
    f.request.repository_id = 10;
    assert!(
        f.run(&allowed())
            .unwrap_err()
            .to_string()
            .contains("grant_denied")
    );
    assert_eq!(f.facts(), before);
    f.request.repository_id = 9;
    f.request.snapshot_id = f.origin.id.clone();
    assert!(f.run(&allowed()).is_err());
    assert_eq!(f.facts(), before);
}
#[test]
fn aggregate_snapshot_file_source_tree_and_proof_budgets_never_reset_per_commit() {
    for fault in [
        "snapshots",
        "files",
        "source",
        "tree_bytes",
        "tree_entries",
        "commit_bytes",
    ] {
        let mut f = Fixture::new();
        let before = f.facts();
        let full = f.run(&allowed()).unwrap();
        let a = &full["acquisition"];
        // Repeating the same deterministic request revalidates all objects and
        // consumes the same aggregate allowance, even when snapshots exist.
        let mut o = allowed();
        match fault {
            "snapshots" => o.max_snapshots = 2,
            "files" => o.max_files = a["candidateFilesAdmitted"].as_u64().unwrap() as usize - 1,
            "source" => {
                o.max_source_bytes = a["sourceBytesAdmitted"].as_u64().unwrap() as usize - 1
            }
            "tree_bytes" => o.max_tree_bytes = a["treeListingBytes"].as_u64().unwrap() as usize - 1,
            "tree_entries" => o.max_tree_entries = a["treeEntries"].as_u64().unwrap() as usize - 1,
            "commit_bytes" => o.max_commit_bytes = a["commitBytes"].as_u64().unwrap() as usize - 1,
            _ => unreachable!(),
        }
        let published = f.facts();
        assert!(f.run(&o).is_err(), "{fault}");
        assert_eq!(f.facts(), published);
        assert_ne!(before, published);
    }
}
#[test]
fn late_output_or_sql_failure_rolls_back_every_snapshot_graph_fts_scope_and_receipt() {
    for fault in [
        "output",
        "source_receipt",
        "artifact",
        "inheritance",
        "graphs",
    ] {
        let mut f = Fixture::new();
        let before = f.facts();
        let mut o = allowed();
        match fault {
            "output" => o.max_output_bytes = 1024,
            "source_receipt" => {
                // The first target is fully prepared before the second target
                // fails. Its pending graph/FTS/scope/receipt must also roll back.
                f.engine.store.db.execute_batch(&format!("CREATE TRIGGER refuse_history BEFORE INSERT ON records WHEN NEW.kind='native_git_source_v1' AND json_extract(NEW.value,'$.commitId')='{}' BEGIN SELECT RAISE(ABORT,'private rollback marker'); END;",f.commits[1])).unwrap();
            }
            "artifact" => {
                f.engine.store.db.execute_batch("CREATE TRIGGER refuse_history BEFORE INSERT ON records WHEN NEW.kind='artifact' BEGIN SELECT RAISE(ABORT,'private rollback marker'); END;").unwrap();
            }
            "inheritance" => {
                f.engine.store.db.execute_batch(&format!("CREATE TRIGGER refuse_history BEFORE INSERT ON records WHEN NEW.kind='{}' BEGIN SELECT RAISE(ABORT,'private rollback marker'); END;",yoke::capture::DERIVED_OPTIONS_RECORD_KIND)).unwrap();
            }
            "graphs" => {
                f.engine.store.db.execute_batch("CREATE TRIGGER refuse_history BEFORE INSERT ON graphs BEGIN SELECT RAISE(ABORT,'private rollback marker'); END;").unwrap();
            }
            _ => unreachable!(),
        }
        let error = f.run(&o).unwrap_err();
        assert!(!format!("{error:#}").contains("private rollback marker"));
        assert_eq!(f.facts(), before, "{fault}");
        assert!(f.engine.store.db.is_autocommit());
    }
}
#[test]
fn exact_output_capacity_is_admitted_before_commit_and_corrupt_existing_graph_is_never_replaced() {
    let mut f = Fixture::new();
    let r = f.run(&allowed()).unwrap();
    let size = yoke::json::encode_bounded(&r, 4_000_000, &Deadline::new(30000))
        .unwrap()
        .len()
        + 1;
    let mut o = allowed();
    o.max_output_bytes = size;
    assert_eq!(f.run(&o).unwrap(), r);
    o.max_output_bytes = size - 1;
    assert!(f.run(&o).is_err());
    let id = r["acquiredCaptures"][0]["snapshotId"].as_str().unwrap();
    let mut graph = serde_json::to_value(f.engine.store.graph(id).unwrap()).unwrap();
    graph["entities"][0]["name"] = json!("foreign-private-claim");
    let corrupt = serde_json::to_string(&graph).unwrap();
    f.engine
        .store
        .db
        .execute(
            "UPDATE graphs SET value=?1 WHERE id=?2",
            rusqlite::params![corrupt, id],
        )
        .unwrap();
    let before = f.facts();
    let error = f.run(&allowed()).unwrap_err();
    assert!(!format!("{error:#}").contains("foreign-private-claim"));
    assert_eq!(f.facts(), before);
    let retained: String = f
        .engine
        .store
        .db
        .query_row("SELECT value FROM graphs WHERE id=?1", [id], |r| r.get(0))
        .unwrap();
    assert_eq!(retained, corrupt);
}
#[test]
fn caller_transaction_expiry_durability_and_sql_policy_are_preserved() {
    use rusqlite::limits::Limit;
    let mut f = Fixture::new();
    let before = f.facts();
    f.engine.store.db.execute_batch("BEGIN").unwrap();
    assert!(
        f.run(&allowed())
            .unwrap_err()
            .to_string()
            .contains("caller_transaction")
    );
    assert!(!f.engine.store.db.is_autocommit());
    assert!(
        git_workflows::capture_commit_at(
            AnalyzerEpoch::V8,
            &mut f.engine,
            &f.git,
            &f.origin,
            &f.commits[0],
            &Deadline::new(30000)
        )
        .is_err()
    );
    f.engine.store.db.execute_batch("ROLLBACK").unwrap();
    assert_eq!(f.facts(), before);
    f.engine
        .store
        .db
        .pragma_update(None, "synchronous", 0)
        .unwrap();
    assert!(f.run(&allowed()).is_err());
    assert_eq!(f.facts(), before);
    f.engine
        .store
        .db
        .pragma_update(None, "synchronous", 2)
        .unwrap();
    let length = f
        .engine
        .store
        .db
        .set_limit(Limit::SQLITE_LIMIT_LENGTH, 1024)
        .unwrap();
    f.engine
        .store
        .db
        .busy_timeout(std::time::Duration::from_millis(123))
        .unwrap();
    assert!(f.run(&allowed()).is_err());
    assert_eq!(
        f.engine.store.db.limit(Limit::SQLITE_LIMIT_LENGTH).unwrap(),
        1024
    );
    assert_eq!(
        f.engine
            .store
            .db
            .query_row::<u64, _, _>("PRAGMA busy_timeout", [], |r| r.get(0))
            .unwrap(),
        123
    );
    f.engine
        .store
        .db
        .set_limit(Limit::SQLITE_LIMIT_LENGTH, length)
        .unwrap();
    assert_eq!(f.facts(), before);
    let d = Deadline::new(10);
    std::thread::sleep(std::time::Duration::from_millis(15));
    assert!(
        acquisition::acquire(
            &mut f.engine,
            &f.profile,
            &digest::Options::default(),
            &f.request,
            &f.git,
            &digest::HistoryOptions::default(),
            &allowed(),
            &d
        )
        .is_err()
    );
    assert_eq!(f.facts(), before);
}
#[test]
fn public_single_commit_capture_also_rolls_back_late_proof_failure() {
    let mut f = Fixture::new();
    let before = f.facts();
    f.engine.store.db.execute_batch("CREATE TRIGGER refuse_source BEFORE INSERT ON records WHEN NEW.kind='native_git_source_v1' BEGIN SELECT RAISE(ABORT,'owned failure'); END;").unwrap();
    assert!(
        git_workflows::capture_commit_at(
            AnalyzerEpoch::V8,
            &mut f.engine,
            &f.git,
            &f.origin,
            &f.commits[0],
            &Deadline::new(30000)
        )
        .is_err()
    );
    assert_eq!(f.facts(), before);
    assert!(f.engine.store.db.is_autocommit());
}
#[test]
fn zero_selected_history_reports_its_frontier_without_publishing_or_claiming_complete() {
    let mut f = Fixture::new();
    let before = f.facts();
    let options = digest::HistoryOptions {
        max_commits: 0,
        ..Default::default()
    };
    let result = acquisition::acquire(
        &mut f.engine,
        &f.profile,
        &digest::Options::default(),
        &f.request,
        &f.git,
        &options,
        &allowed(),
        &Deadline::new(30000),
    )
    .unwrap();
    assert!(result["acquiredCaptures"].as_array().unwrap().is_empty());
    assert_eq!(result["stateWritten"], false);
    assert_eq!(result["complete"], false);
    assert_eq!(
        result["historyMapping"]["filesStatus"],
        "no_selected_commits"
    );
    assert_eq!(f.facts(), before);
}
#[test]
fn cli_denials_precede_host_files_and_store_initialization() {
    let root = tempfile::tempdir().unwrap();
    for flags in [
        vec![],
        vec!["--allow-capture", "--allow-history", "--trust-git"],
        vec!["--allow-writes", "--allow-capture", "--trust-git"],
    ] {
        let out = Command::new(env!("CARGO_BIN_EXE_yoke"))
            .arg("--root")
            .arg(root.path())
            .arg("digest-history-capture")
            .args(flags)
            .args(["--profile", "missing.json", "--request", "missing.json"])
            .output()
            .unwrap();
        assert!(!out.status.success());
        assert!(out.stdout.is_empty());
        assert!(String::from_utf8_lossy(&out.stderr).contains("permission_required"));
        assert!(!root.path().join(".yoke").exists());
    }
    let host = tempfile::tempdir().unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(root.path())
        .args([
            "--allow-writes",
            "digest-history-capture",
            "--allow-capture",
            "--allow-history",
            "--trust-git",
            "--profile",
            "../not-a-host-profile.json",
            "--request",
        ])
        .arg(host.path().join("missing.json"))
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("input_invalid"));
    assert!(!root.path().join(".yoke").exists());
}
#[test]
fn actual_cli_acquires_history_without_app_keys_or_checkout_writes() {
    let f = Fixture::new();
    let host = tempfile::tempdir().unwrap();
    for (name, value) in [
        ("profile.json", profile_value()),
        ("request.json", serde_json::to_value(&f.request).unwrap()),
    ] {
        std::fs::write(host.path().join(name), serde_json::to_vec(&value).unwrap()).unwrap();
    }
    let root = f.root.path().to_owned();
    let before = std::fs::read(root.join("café.rs")).unwrap();
    drop(f.engine);
    let out = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(&root)
        .args([
            "--allow-writes",
            "--max-bytes",
            "4000000",
            "digest-history-capture",
            "--allow-capture",
            "--allow-history",
            "--trust-git",
            "--profile",
        ])
        .arg(host.path().join("profile.json"))
        .arg("--request")
        .arg(host.path().join("request.json"))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["historyMapping"]["mappedCommits"], 3);
    assert_eq!(v["acquiredCaptures"].as_array().unwrap().len(), 2);
    assert_eq!(std::fs::read(root.join("café.rs")).unwrap(), before);
    assert_eq!(v["modelCalls"], 0);
    assert_eq!(v["networkRequests"], 0);
}
