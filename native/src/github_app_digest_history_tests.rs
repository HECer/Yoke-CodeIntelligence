//! A private runner records whether any history acquisition was authorized.
use super::{Options, PreviewRequest, preview_with_history_reader};
use crate::{
    Engine,
    capture::Deadline,
    git_workflows::{self, HostGit},
    github_app::HostProfile,
};
use serde_json::{Value, json};
use std::{cell::Cell, path::Path, process::Command};

#[cfg(windows)]
#[path = "github_app_digest_history_process_tests.rs"]
mod history_owned_process_tests;

fn git(host: &HostGit, root: &Path, args: &[&str]) -> String {
    let null = if cfg!(windows) { "NUL" } else { "/dev/null" };
    let output = Command::new(&host.executable)
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
            "user.name=History Fixture",
            "-c",
            "user.email=fixture@example.invalid",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "Owned Git setup failed");
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}
#[test]
fn rejected_captured_source_never_invokes_a_history_runner() {
    for corrupt in ["commitId", "treeId", "sourceBlob", "commitArtifact"] {
        let root = tempfile::tempdir().unwrap();
        let host = HostGit::discover().unwrap();
        git(&host, root.path(), &["init", "-q"]);
        std::fs::write(root.path().join("api.rs"), "pub fn api() {}\n").unwrap();
        git(&host, root.path(), &["add", "--", "api.rs"]);
        git(&host, root.path(), &["commit", "-q", "-m", "owned fixture"]);
        let head = git(&host, root.path(), &["rev-parse", "HEAD"]);
        let d = Deadline::new(30_000);
        let mut engine = Engine::open(root.path()).unwrap();
        let origin = engine.index(&d).unwrap().0;
        let snapshot = git_workflows::capture_commit_at(
            crate::analysis::AnalyzerEpoch::V8,
            &mut engine,
            &host,
            &origin,
            &head,
            &d,
        )
        .unwrap()
        .0;
        let profile = HostProfile::parse(br#"{"schemaVersion":1,"appId":123,"secretCredential":"history-key-must-not-be-read","grants":[{"installationId":42,"repositoryId":9,"repository":"acme/demo"}]}"#).unwrap();
        let request = PreviewRequest {
            schema_version: 1,
            snapshot_id: snapshot.id.clone(),
            installation_id: 42,
            repository_id: 9,
            repository: "acme/demo".into(),
        };
        let calls = Cell::new(0);
        let mut reader = |_: &str, _: &Deadline| {
            calls.set(calls.get() + 1);
            Ok(git_workflows::history_read::History {
                commits: vec![],
                summary: json!({"complete":false}),
            })
        };
        preview_with_history_reader(
            &engine.store,
            &profile,
            &Options::default(),
            &request,
            &d,
            Some(&mut reader),
        )
        .unwrap();
        assert_eq!(
            calls.get(),
            1,
            "Healthy source must reach the selected runner"
        );
        calls.set(0);
        let raw: String = engine
            .store
            .db
            .query_row(
                "SELECT value FROM records WHERE kind=?1 AND id=?2",
                rusqlite::params![git_workflows::SOURCE_KIND, snapshot.id],
                |row| row.get(0),
            )
            .unwrap();
        let mut proof: Value = serde_json::from_str(&raw).unwrap();
        match corrupt {
            "commitId" | "treeId" => {
                proof[corrupt] = json!("b".repeat(head.len()));
                engine
                    .store
                    .db
                    .execute(
                        "UPDATE records SET value=?1 WHERE kind=?2 AND id=?3",
                        rusqlite::params![
                            serde_json::to_string(&proof).unwrap(),
                            git_workflows::SOURCE_KIND,
                            snapshot.id
                        ],
                    )
                    .unwrap();
            }
            "sourceBlob" => std::fs::write(
                engine.store.dir.join("blobs").join(&snapshot.files[0].hash),
                b"INPUT_BODY_CANARY",
            )
            .unwrap(),
            "commitArtifact" => std::fs::write(
                engine
                    .store
                    .dir
                    .join("blobs")
                    .join(proof["commitArtifactId"].as_str().unwrap()),
                b"INPUT_BODY_CANARY",
            )
            .unwrap(),
            _ => unreachable!(),
        }
        let changes = engine.store.db.total_changes();
        let error = preview_with_history_reader(
            &engine.store,
            &profile,
            &Options::default(),
            &request,
            &d,
            Some(&mut reader),
        )
        .unwrap_err();
        assert_eq!(error.to_string(), "github_app_digest_source_unavailable");
        assert_eq!(
            calls.get(),
            0,
            "Rejected {corrupt} reached history acquisition"
        );
        assert_eq!(engine.store.db.total_changes(), changes);
    }
}
