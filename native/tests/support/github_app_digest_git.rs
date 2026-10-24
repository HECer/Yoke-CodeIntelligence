//! Owned local Git setup followed by source-only preview; no provider traffic.
use super::{SOURCE, cas, host, marker, request, run};
use std::{
    path::{Path, PathBuf},
    process::Command,
};
use yoke::{
    Engine,
    analysis::AnalyzerEpoch,
    capture::Deadline,
    git_workflows::{self, HostGit},
    github_app::digest,
};

fn executable() -> PathBuf {
    std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|path| path.join(if cfg!(windows) { "git.exe" } else { "git" }))
        .find(|path| path.is_absolute() && path.is_file())
        .unwrap()
        .canonicalize()
        .unwrap()
}
fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new(executable())
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .env(
            "GIT_CONFIG_SYSTEM",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .args([
            "-c",
            "init.templateDir=",
            "-c",
            "core.autocrlf=false",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "user.name=Digest Fixture",
            "-c",
            "user.email=fixture@example.invalid",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "owned Git setup failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}
struct Fixture {
    root: tempfile::TempDir,
    engine: Engine,
    id: String,
    commit: String,
    tree: String,
    d: Deadline,
}
fn fixture(epoch: AnalyzerEpoch) -> Fixture {
    let root = tempfile::tempdir().unwrap();
    git(root.path(), &["init", "-q"]);
    std::fs::write(root.path().join("café.rs"), SOURCE).unwrap();
    git(root.path(), &["add", "--", "café.rs"]);
    git(root.path(), &["commit", "-q", "-m", "owned digest source"]);
    let commit = git(root.path(), &["rev-parse", "HEAD"]);
    let tree = git(root.path(), &["rev-parse", "HEAD^{tree}"]);
    let d = Deadline::new(30_000);
    let mut engine = Engine::open(root.path()).unwrap();
    engine.capture_epoch = epoch;
    let origin = engine.index(&d).unwrap().0;
    let selected = git_workflows::capture_commit_at(
        epoch,
        &mut engine,
        &HostGit {
            executable: executable(),
        },
        &origin,
        &commit,
        &d,
    )
    .unwrap()
    .0;
    Fixture {
        root,
        engine,
        id: selected.id,
        commit,
        tree,
        d,
    }
}

#[test]
fn captured_git_preview_verifies_commit_without_live_checkout_or_git_metadata() {
    for epoch in [AnalyzerEpoch::V8, AnalyzerEpoch::V9] {
        let f = fixture(epoch);
        std::fs::rename(
            f.root.path().join(".git"),
            f.root.path().join("unavailable_git_metadata"),
        )
        .unwrap();
        std::fs::write(
            f.root.path().join("café.rs"),
            b"pub fn changed_after_commit() {}\n",
        )
        .unwrap();
        let changes = f.engine.store.db.total_changes();
        let blobs = cas(&f.engine);
        let value = run(
            &f.engine,
            &host(),
            &request(&f.id),
            &digest::Options::default(),
            &f.d,
        )
        .unwrap();
        assert_eq!(value["sourceCommitVerified"], true);
        assert_eq!(value["sourceCommitId"], f.commit);
        assert_eq!(value["sourceTreeId"], f.tree);
        assert_eq!(value["analyzer"], epoch.producer());
        assert_eq!(value["providerVerified"], false);
        assert_eq!(value["networkRequests"], 0);
        assert_eq!(value["complete"], false);
        assert!(
            value["symbols"]
                .as_array()
                .unwrap()
                .iter()
                .any(|symbol| symbol["name"] == "café_api")
        );
        let raw = serde_json::to_string(&value).unwrap();
        assert!(!raw.contains("PRIVATE_IMPLEMENTATION_CANARY"));
        assert!(!raw.contains("changed_after_commit"));
        assert_eq!(f.engine.store.db.total_changes(), changes);
        assert_eq!(cas(&f.engine), blobs);
        assert_eq!(
            std::fs::read(f.root.path().join("café.rs")).unwrap(),
            b"pub fn changed_after_commit() {}\n"
        );
    }
}

#[test]
fn captured_git_preview_refuses_forged_commit_tree_blob_and_analyzer_facts() {
    let f = fixture(AnalyzerEpoch::V8);
    let o = digest::Options::default();
    assert!(run(&f.engine, &host(), &request(&f.id), &o, &f.d).is_ok());
    let raw: String = f
        .engine
        .store
        .db
        .query_row(
            "SELECT value FROM records WHERE kind=?1 AND id=?2",
            rusqlite::params![git_workflows::SOURCE_KIND, f.id],
            |row| row.get(0),
        )
        .unwrap();
    let original: serde_json::Value = serde_json::from_str(&raw).unwrap();
    for field in ["commitId", "treeId", "blob", "analyzer"] {
        let mut proof = original.clone();
        match field {
            "blob" => proof["blobs"][0]["contentHash"] = serde_json::json!("b".repeat(64)),
            "analyzer" => proof[field] = serde_json::json!("yoke-tree-sitter-999"),
            _ => proof[field] = serde_json::json!("b".repeat(f.commit.len())),
        }
        f.engine
            .store
            .db
            .execute(
                "UPDATE records SET value=?1 WHERE kind=?2 AND id=?3",
                rusqlite::params![
                    serde_json::to_string(&proof).unwrap(),
                    git_workflows::SOURCE_KIND,
                    f.id
                ],
            )
            .unwrap();
        let changes = f.engine.store.db.total_changes();
        let blobs = cas(&f.engine);
        marker(
            run(&f.engine, &host(), &request(&f.id), &o, &f.d),
            "github_app_digest_source_unavailable",
        );
        assert_eq!(f.engine.store.db.total_changes(), changes);
        assert_eq!(cas(&f.engine), blobs);
    }
}

#[test]
fn captured_git_source_budget_precedes_commit_blob_validation_without_repair() {
    let f = fixture(AnalyzerEpoch::V8);
    assert!(
        run(
            &f.engine,
            &host(),
            &request(&f.id),
            &digest::Options::default(),
            &f.d
        )
        .is_ok()
    );
    let proof = git_workflows::validate_source(&f.engine.store, &f.id, &f.d).unwrap();
    let blob = f
        .engine
        .store
        .dir
        .join("blobs")
        .join(proof["commitArtifactId"].as_str().unwrap());
    std::fs::write(&blob, b"CORRUPT_COMMIT_CANARY").unwrap();
    let changes = f.engine.store.db.total_changes();
    let blobs = cas(&f.engine);
    marker(
        run(
            &f.engine,
            &host(),
            &request(&f.id),
            &digest::Options::default(),
            &f.d,
        ),
        "github_app_digest_source_unavailable",
    );
    let o = digest::Options {
        max_source_bytes: 1,
        ..Default::default()
    };
    marker(
        run(&f.engine, &host(), &request(&f.id), &o, &f.d),
        "github_app_digest_capacity_exceeded",
    );
    assert_eq!(f.engine.store.db.total_changes(), changes);
    assert_eq!(cas(&f.engine), blobs);
}

#[test]
fn git_preview_refuses_unbounded_derived_origin_chains_before_scope_replay() {
    let mut f = fixture(AnalyzerEpoch::V8);
    assert!(
        run(
            &f.engine,
            &host(),
            &request(&f.id),
            &digest::Options::default(),
            &f.d
        )
        .is_ok()
    );
    let origin = f.engine.store.snapshot(&f.id).unwrap();
    let nested = git_workflows::capture_commit_at(
        AnalyzerEpoch::V8,
        &mut f.engine,
        &HostGit {
            executable: executable(),
        },
        &origin,
        &f.commit,
        &f.d,
    )
    .unwrap()
    .0;
    let changes = f.engine.store.db.total_changes();
    let blobs = cas(&f.engine);
    marker(
        run(
            &f.engine,
            &host(),
            &request(&nested.id),
            &digest::Options::default(),
            &f.d,
        ),
        "github_app_digest_source_unavailable",
    );
    assert_eq!(f.engine.store.db.total_changes(), changes);
    assert_eq!(cas(&f.engine), blobs);
}

#[test]
fn git_lineage_metadata_has_an_independent_host_budget_before_commit_reads() {
    let f = fixture(AnalyzerEpoch::V8);
    assert!(
        run(
            &f.engine,
            &host(),
            &request(&f.id),
            &digest::Options::default(),
            &f.d
        )
        .is_ok()
    );
    let changes = f.engine.store.db.total_changes();
    let blobs = cas(&f.engine);
    let o = digest::Options {
        max_lineage_metadata_bytes: 1,
        ..Default::default()
    };
    marker(
        run(&f.engine, &host(), &request(&f.id), &o, &f.d),
        "github_app_digest_capacity_exceeded",
    );
    assert_eq!(f.engine.store.db.total_changes(), changes);
    assert_eq!(cas(&f.engine), blobs);
}

#[test]
fn small_git_selection_refuses_oversized_origin_metadata_before_commit_reads() {
    let f = fixture(AnalyzerEpoch::V8);
    assert!(
        run(
            &f.engine,
            &host(),
            &request(&f.id),
            &digest::Options::default(),
            &f.d
        )
        .is_ok()
    );
    let proof = git_workflows::validate_source(&f.engine.store, &f.id, &f.d).unwrap();
    let origin = proof["originSnapshotId"].as_str().unwrap();
    let raw: String = f
        .engine
        .store
        .db
        .query_row("SELECT value FROM snapshots WHERE id=?1", [origin], |row| {
            row.get(0)
        })
        .unwrap();
    let padded = format!("{raw}{}", " ".repeat(16_000_001));
    // Valid JSON whitespace changes stored byte work without changing native
    // manifest facts or allocating a large ancestry fixture.
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&padded).unwrap(),
        serde_json::from_str::<serde_json::Value>(&raw).unwrap()
    );
    f.engine
        .store
        .db
        .execute(
            "UPDATE snapshots SET value=?1 WHERE id=?2",
            rusqlite::params![padded, origin],
        )
        .unwrap();
    let artifact = proof["commitArtifactId"].as_str().unwrap();
    std::fs::write(
        f.engine.store.dir.join("blobs").join(artifact),
        b"corrupt commit",
    )
    .unwrap();
    let changes = f.engine.store.db.total_changes();
    let blobs = cas(&f.engine);
    marker(
        run(
            &f.engine,
            &host(),
            &request(&f.id),
            &digest::Options::default(),
            &f.d,
        ),
        "github_app_digest_capacity_exceeded",
    );
    assert_eq!(f.engine.store.db.total_changes(), changes);
    assert_eq!(cas(&f.engine), blobs);
}

#[test]
fn git_lineage_aggregate_exact_boundary_includes_every_proof_row() {
    let f = fixture(AnalyzerEpoch::V9);
    let proof = git_workflows::validate_source(&f.engine.store, &f.id, &f.d).unwrap();
    let origin = proof["originSnapshotId"].as_str().unwrap();
    let bytes: usize = f
        .engine
        .store
        .db
        .query_row(
            "SELECT (SELECT length(CAST(value AS BLOB)) FROM snapshots WHERE id=?1) +
        (SELECT coalesce(sum(length(CAST(value AS BLOB))),0) FROM records WHERE
        (id=?2 AND kind IN (?3,?4)) OR (id=?1 AND kind IN (?5,?6)))",
            rusqlite::params![
                origin,
                f.id,
                git_workflows::SOURCE_KIND,
                yoke::capture::DERIVED_OPTIONS_RECORD_KIND,
                yoke::capture::OPTIONS_RECORD_KIND,
                yoke::capture::IGNORED_RECORD_KIND
            ],
            |row| row.get(0),
        )
        .unwrap();
    assert!(bytes < digest::Options::default().max_lineage_metadata_bytes);
    let o = digest::Options {
        max_lineage_metadata_bytes: bytes,
        ..Default::default()
    };
    assert!(run(&f.engine, &host(), &request(&f.id), &o, &f.d).is_ok());
    let changes = f.engine.store.db.total_changes();
    let blobs = cas(&f.engine);
    let o = digest::Options {
        max_lineage_metadata_bytes: bytes - 1,
        ..o
    };
    marker(
        run(&f.engine, &host(), &request(&f.id), &o, &f.d),
        "github_app_digest_capacity_exceeded",
    );
    assert_eq!(f.engine.store.db.total_changes(), changes);
    assert_eq!(cas(&f.engine), blobs);
}

#[test]
fn git_disk_origin_refuses_hidden_selector_rows_without_decoding_them() {
    let f = fixture(AnalyzerEpoch::V8);
    let proof = git_workflows::validate_source(&f.engine.store, &f.id, &f.d).unwrap();
    let origin = proof["originSnapshotId"].as_str().unwrap();
    for kind in [
        yoke::capture::DERIVED_OPTIONS_RECORD_KIND,
        "overlay-snapshot",
        git_workflows::SOURCE_KIND,
    ] {
        f.engine
            .store
            .db
            .execute(
                "INSERT INTO records(kind,id,value) VALUES(?1,?2,'not-json-origin-canary')",
                rusqlite::params![kind, origin],
            )
            .unwrap();
        let changes = f.engine.store.db.total_changes();
        let blobs = cas(&f.engine);
        let error = run(
            &f.engine,
            &host(),
            &request(&f.id),
            &digest::Options::default(),
            &f.d,
        )
        .unwrap_err();
        assert_eq!(error.to_string(), "github_app_digest_source_unavailable");
        assert_eq!(f.engine.store.db.total_changes(), changes);
        assert_eq!(cas(&f.engine), blobs);
        f.engine
            .store
            .db
            .execute(
                "DELETE FROM records WHERE kind=?1 AND id=?2",
                rusqlite::params![kind, origin],
            )
            .unwrap();
        assert!(
            run(
                &f.engine,
                &host(),
                &request(&f.id),
                &digest::Options::default(),
                &f.d
            )
            .is_ok()
        );
    }
}

#[test]
fn selected_metadata_budget_does_not_reduce_the_independent_lineage_row_allowance() {
    let f = fixture(AnalyzerEpoch::V8);
    let before = run(
        &f.engine,
        &host(),
        &request(&f.id),
        &digest::Options::default(),
        &f.d,
    )
    .unwrap();
    let proof = git_workflows::validate_source(&f.engine.store, &f.id, &f.d).unwrap();
    let origin = proof["originSnapshotId"].as_str().unwrap();
    let raw: String = f
        .engine
        .store
        .db
        .query_row("SELECT value FROM snapshots WHERE id=?1", [origin], |row| {
            row.get(0)
        })
        .unwrap();
    f.engine
        .store
        .db
        .execute(
            "UPDATE snapshots SET value=?1 WHERE id=?2",
            rusqlite::params![format!("{raw}{}", " ".repeat(128_000)), origin],
        )
        .unwrap();
    let selected_bytes: usize = f
        .engine
        .store
        .db
        .query_row(
            "SELECT (SELECT length(CAST(value AS BLOB)) FROM snapshots WHERE id=?1) +
        (SELECT length(CAST(value AS BLOB)) FROM graphs WHERE id=?1)",
            [&f.id],
            |row| row.get(0),
        )
        .unwrap();
    let o = digest::Options {
        max_metadata_bytes: selected_bytes,
        ..Default::default()
    };
    let changes = f.engine.store.db.total_changes();
    let blobs = cas(&f.engine);
    let value = run(&f.engine, &host(), &request(&f.id), &o, &f.d).unwrap();
    assert_eq!(value, before);
    assert_eq!(f.engine.store.db.total_changes(), changes);
    assert_eq!(cas(&f.engine), blobs);
}

#[test]
fn lowered_metadata_budgets_preserve_the_separate_commit_registration_row_allowance() {
    let f = fixture(AnalyzerEpoch::V8);
    let before = run(
        &f.engine,
        &host(),
        &request(&f.id),
        &digest::Options::default(),
        &f.d,
    )
    .unwrap();
    let proof = git_workflows::validate_source(&f.engine.store, &f.id, &f.d).unwrap();
    let artifact = proof["commitArtifactId"].as_str().unwrap();
    let (kind, raw): (String, String) = f
        .engine
        .store
        .db
        .query_row(
            "SELECT kind,value FROM records WHERE id=?1",
            [artifact],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    f.engine
        .store
        .db
        .execute(
            "UPDATE records SET value=?1 WHERE kind=?2 AND id=?3",
            rusqlite::params![format!("{raw}{}", " ".repeat(32_000)), kind, artifact],
        )
        .unwrap();
    let o = digest::Options {
        max_metadata_bytes: 10_000,
        max_lineage_metadata_bytes: 10_000,
        ..Default::default()
    };
    let changes = f.engine.store.db.total_changes();
    let blobs = cas(&f.engine);
    let value = run(&f.engine, &host(), &request(&f.id), &o, &f.d).unwrap();
    assert_eq!(value, before);
    assert_eq!(f.engine.store.db.total_changes(), changes);
    assert_eq!(cas(&f.engine), blobs);
}
