use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpListener},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};
use yoke::{
    capture::Deadline,
    github::{
        self,
        discussions::{Capture, Profile, Request},
    },
};

struct Fixture {
    endpoint: SocketAddr,
    calls: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Fixture {
    fn new(mode: &str) -> Self {
        Self::with_head(mode, "b".repeat(40))
    }
    fn with_head(mode: &str, reported_head: String) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = listener.local_addr().unwrap();
        let origin = format!("http://{endpoint}");
        let calls = Arc::new(Mutex::new(vec![]));
        let seen = Arc::clone(&calls);
        let stop = Arc::new(AtomicBool::new(false));
        let done = Arc::clone(&stop);
        let mode = mode.to_owned();
        let worker = thread::spawn(move || {
            let mut pulls = 0;
            while !done.load(Ordering::SeqCst) {
                let (mut stream, _) = match listener.accept() {
                    Ok(value) => value,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                        continue;
                    }
                    Err(e) => panic!("owned discussion fixture: {e}"),
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut raw = vec![];
                let mut byte = [0];
                while raw.len() < 16384 && !raw.ends_with(b"\r\n\r\n") {
                    if stream.read(&mut byte).unwrap_or(0) == 0 {
                        break;
                    }
                    raw.push(byte[0]);
                }
                let head = String::from_utf8(raw).unwrap();
                let target = head
                    .lines()
                    .next()
                    .unwrap()
                    .split_whitespace()
                    .nth(1)
                    .unwrap()
                    .to_owned();
                seen.lock().unwrap().push(target.clone());
                let path = target.split('?').next().unwrap();
                let date = "2026-10-18T10:00:00Z";
                let mut pull = json!({"id":100,"number":1,"title":"owned title 🌍","body":"owned description\r\n","state":"closed","merged":true,
                    "created_at":date,"updated_at":date,"closed_at":date,"url":format!("{origin}/repos/org/repo/pulls/1"),"user":{"login":"owned"},
                    "head":{"sha":reported_head.clone()},"base":{"sha":"a".repeat(40),"repo":{"id":9,"full_name":"org/repo"}}});
                if mode == "null-body" {
                    pull["body"] = Value::Null;
                }
                if mode == "unsafe-text" {
                    pull["body"] = json!(format!("ghp_{}", "A".repeat(40)));
                }
                let mut link = None;
                let body = match path {
                    "/repos/org/repo" => json!({"id":9,"full_name":"org/repo"}),
                    "/repos/org/repo/pulls" => {
                        if mode == "lost-filter" {
                            link = Some(format!(
                                "<{origin}/repos/org/repo/pulls?per_page=100&page=2>; rel=\"next\""
                            ));
                        }
                        json!([pull])
                    }
                    "/repos/org/repo/pulls/1" => {
                        pulls += 1;
                        if mode == "changed" && pulls > 1 {
                            pull["head"]["sha"] = json!("c".repeat(40));
                        }
                        pull
                    }
                    "/repos/org/repo/issues/1/comments" => {
                        json!([{"id":101,"body":"why 🌍\r\n","created_at":date,"updated_at":date,"user":{"login":"owned"},"issue_url":format!("{origin}/repos/org/repo/issues/{}",if mode=="foreign-comment" {2}else{1})}])
                    }
                    "/repos/org/repo/pulls/1/reviews" => {
                        json!([{"id":102,"body":"owned review","state":"APPROVED","submitted_at":date,"commit_id":"b".repeat(40),"user":{"login":"owned"},"pull_request_url":format!("{origin}/repos/org/repo/pulls/1")}])
                    }
                    "/repos/org/repo/pulls/1/comments" => {
                        json!([{"id":103,"body":"owned reply","path":"café.rs","diff_hunk":"@@ -1 +1 @@\r\n+🌍","line":3,"original_line":2,"side":"RIGHT","in_reply_to_id":104,"pull_request_review_id":102,"created_at":date,"updated_at":date,"commit_id":"b".repeat(40),"original_commit_id":"a".repeat(40),"user":{"login":"owned"},"pull_request_url":format!("{origin}/repos/org/repo/pulls/1")}])
                    }
                    _ => panic!("unexpected owned endpoint {path}"),
                };
                let body = serde_json::to_vec(&body).unwrap();
                write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n",body.len()).unwrap();
                if let Some(link) = link {
                    write!(stream, "Link: {link}\r\n").unwrap();
                }
                stream.write_all(b"\r\n").unwrap();
                stream.write_all(&body).unwrap();
            }
        });
        Self {
            endpoint,
            calls,
            stop,
            worker: Some(worker),
        }
    }
    fn client(&self, profile: &Profile) -> github::ReadClient {
        github::ReadClient::for_fixture(
            &profile.read,
            &github::HostTestProfile::loopback(self.endpoint).unwrap(),
            &Deadline::new(20000),
        )
        .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.worker.take().unwrap().join().unwrap();
    }
}
fn profile() -> Profile {
    Profile {
        read: github::HostReadProfile {
            allow_network: true,
            allowed_repos: vec!["org/repo".into()],
            ..Default::default()
        },
        allow_discussion_text: true,
        ..Default::default()
    }
}
fn input() -> Request {
    Request {
        schema_version: 1,
        repository: "org/repo".into(),
        repository_id: 9,
        pull_requests: vec![1],
    }
}
fn captured(f: &Fixture) -> Capture {
    let p = profile();
    github::discussions::capture(&f.client(&p), &p, &input(), &Deadline::new(20000)).unwrap()
}
fn rehash(value: &mut Value) {
    let payload: github::discussions::Payload =
        serde_json::from_value(value["payload"].clone()).unwrap();
    value["id"] = json!(yoke::model::hash(serde_json::to_vec(&payload).unwrap()));
}

#[test]
fn captures_all_channels_exact_unicode_reply_edges_and_rechecks_the_closed_head() {
    let f = Fixture::new("normal");
    let c = captured(&f);
    assert!(c.payload.coverage_complete);
    assert_eq!(c.payload.observations, 4);
    assert_eq!(
        c.payload.pulls[0].observations[1].body.as_deref(),
        Some("why 🌍\r\n")
    );
    assert_eq!(c.payload.pulls[0].observations[3].in_reply_to_id, Some(104));
    assert_eq!(
        c.payload.pulls[0].observations[2].submitted_at.as_deref(),
        Some("2026-10-18T10:00:00Z")
    );
    assert_eq!(
        c.payload.pulls[0].observations[3]
            .anchor
            .as_ref()
            .unwrap()
            .original_line,
        Some(2)
    );
    assert_eq!(
        c.payload.pulls[0].observations[3]
            .original_commit_id
            .as_deref(),
        Some("a".repeat(40).as_str())
    );
    let raw = serde_json::to_vec(&c).unwrap();
    assert_eq!(
        Capture::parse(&raw, &Deadline::new(20000)).unwrap().id,
        c.id
    );
    assert_eq!(
        f.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|p| p.split('?').next() == Some("/repos/org/repo/pulls/1"))
            .count(),
        2
    );
}
#[test]
fn missing_text_grant_and_zero_budget_refuse_before_network() {
    for mode in 0..3 {
        let f = Fixture::new("normal");
        let mut p = profile();
        match mode {
            0 => p.allow_discussion_text = false,
            1 => p.max_observations = 0,
            _ => p.max_text_bytes = 0,
        };
        assert!(
            github::discussions::capture(&f.client(&p), &p, &input(), &Deadline::new(20000))
                .is_err()
        );
        assert!(f.calls.lock().unwrap().is_empty());
    }
}
#[test]
fn changed_head_and_cross_pull_comment_never_become_a_capture() {
    for mode in ["changed", "foreign-comment"] {
        let f = Fixture::new(mode);
        let p = profile();
        assert!(
            github::discussions::capture(&f.client(&p), &p, &input(), &Deadline::new(20000))
                .is_err()
        );
    }
}
#[test]
fn closed_listing_cannot_lose_its_state_filter_in_pagination() {
    let f = Fixture::new("lost-filter");
    let p = profile();
    let mut r = input();
    r.pull_requests.clear();
    assert!(github::discussions::capture(&f.client(&p), &p, &r, &Deadline::new(20000)).is_err());
    assert_eq!(f.calls.lock().unwrap().len(), 2);
}
#[test]
fn provider_null_description_is_known_absence_not_missing_page_coverage() {
    let f = Fixture::new("null-body");
    let c = captured(&f);
    assert!(c.payload.coverage_complete);
    assert!(c.payload.pulls[0].observations[0].body.is_none());
}
#[test]
fn replay_rejects_rehashed_foreign_receipts_and_omitted_requested_pulls() {
    let f = Fixture::new("normal");
    let c = captured(&f);
    for mutation in 0..2 {
        let mut value = serde_json::to_value(&c).unwrap();
        if mutation == 0 {
            value["payload"]["receipts"][0]["url"] = json!("https://foreign.invalid/stolen");
        } else {
            value["payload"]["pulls"] = json!([]);
            value["payload"]["observations"] = json!(0);
            value["payload"]["textBytes"] = json!(0);
        }
        rehash(&mut value);
        assert!(
            Capture::parse(&serde_json::to_vec(&value).unwrap(), &Deadline::new(20000)).is_err(),
            "mutation {mutation}"
        );
    }
}

#[test]
fn partial_discovery_keeps_unacquired_pull_numbers_explicit() {
    let f = Fixture::new("normal");
    let mut p = profile();
    p.read.max_requests = 6;
    let mut r = input();
    r.pull_requests.clear();
    let c = github::discussions::capture(&f.client(&p), &p, &r, &Deadline::new(20000)).unwrap();
    assert!(!c.payload.coverage_complete);
    assert!(c.payload.pulls.is_empty());
    assert_eq!(c.payload.unacquired_pulls, vec![1]);
    assert_eq!(c.payload.requests, 2);
    assert_eq!(f.calls.lock().unwrap().len(), 2);
}
#[test]
fn replay_rejects_rehashed_coverage_and_text_claims() {
    let f = Fixture::new("normal");
    let c = captured(&f);
    for mutation in 0..3 {
        let mut value = serde_json::to_value(&c).unwrap();
        match mutation {
            0 => {
                value["payload"]["pulls"][0]["coverage"]["review"]["complete"] = json!(false);
                value["payload"]["pulls"][0]["coverage"]["review"]["reason"] =
                    json!("page_budget_exhausted");
            }
            1 => {
                value["payload"]["pulls"][0]["observations"][1]["bodyHash"] = json!("0".repeat(64))
            }
            _ => value["payload"]["receipts"][0]["bodyRetained"] = json!(true),
        };
        rehash(&mut value);
        assert!(
            Capture::parse(&serde_json::to_vec(&value).unwrap(), &Deadline::new(20000)).is_err(),
            "mutation {mutation}"
        );
    }
}

#[test]
fn digest_replays_captured_source_and_keeps_discussion_claims_separate() {
    let f = Fixture::new("normal");
    let capture = captured(&f);
    let root = tempfile::tempdir().unwrap();
    let text = "pub fn visible(){ let _ = \"owned implementation body\"; }\npub const FLAG:&str=\"owned initializer body\";\n";
    std::fs::write(root.path().join("main.rs"), text).unwrap();
    let mut engine = yoke::Engine::open(root.path()).unwrap();
    let d = Deadline::new(20000);
    let snapshot = engine.index(&d).unwrap().0;
    std::fs::write(root.path().join("main.rs"), "pub fn live_changed(){}\n").unwrap();
    let profile=yoke::github_app::HostProfile::parse(&serde_json::to_vec(&json!({"schemaVersion":1,"appId":123,"secretCredential":"must-not-be-read","grants":[{"installationId":42,"repositoryId":9,"repository":"org/repo"}]})).unwrap()).unwrap();
    let request = yoke::github_app::digest::PreviewRequest {
        schema_version: 1,
        snapshot_id: snapshot.id.clone(),
        installation_id: 42,
        repository_id: 9,
        repository: "org/repo".into(),
    };
    let options = yoke::github_app::digest::Options::default();
    let permission = yoke::github_app::digest::discussions::Permission {
        allow_discussions: true,
        allow_fixture: true,
        handoff_preview: false,
    };
    let before = engine.store.db.total_changes();
    let calls = f.calls.lock().unwrap().len();
    let value = yoke::github_app::digest::discussions::preview(
        &yoke::github_app::digest::discussions::Context {
            store: &engine.store,
            profile: &profile,
            options: &options,
            permission: &permission,
            source: yoke::github_app::digest::discussions::Source::Current,
        },
        &request,
        &capture,
        &d,
    )
    .unwrap();
    assert_eq!(value["snapshotId"], snapshot.id);
    assert_eq!(
        value["threads"][0]["sourceAssociation"]["status"],
        "unavailable"
    );
    assert_eq!(value["threads"][0]["observations"][1]["body"], "why 🌍\r\n");
    assert_eq!(value["networkRequests"], 0);
    assert_eq!(value["modelCalls"], 0);
    assert_eq!(value["providerVerified"], false);
    let raw = serde_json::to_string(&value).unwrap();
    assert!(raw.contains("visible"));
    assert!(!raw.contains("live_changed"));
    assert_eq!(engine.store.db.total_changes(), before);
    assert_eq!(f.calls.lock().unwrap().len(), calls);
    let handoff_permission = yoke::github_app::digest::discussions::Permission {
        handoff_preview: true,
        ..permission
    };
    let handoff = yoke::github_app::digest::discussions::preview(
        &yoke::github_app::digest::discussions::Context {
            store: &engine.store,
            profile: &profile,
            options: &options,
            permission: &handoff_permission,
            source: yoke::github_app::digest::discussions::Source::Current,
        },
        &request,
        &capture,
        &d,
    )
    .unwrap();
    let raw = serde_json::to_string(&handoff).unwrap();
    for excluded in [
        "owned implementation body",
        "owned initializer body",
        "owned description",
        "why 🌍",
        "owned reply",
        "@@ -1",
    ] {
        assert!(
            !raw.contains(excluded),
            "raw field escaped the structured exclusion: {excluded}"
        );
    }
    assert!(raw.contains("visible"));
    assert_eq!(handoff["payload"]["discussionTextIncluded"], false);
    assert_eq!(handoff["serviceSend"], "unavailable");
    assert_eq!(
        handoff["id"],
        yoke::model::hash(serde_json::to_vec(&handoff["payload"]).unwrap())
    );
    assert_eq!(engine.store.db.total_changes(), before);
    assert_eq!(f.calls.lock().unwrap().len(), calls);
    let denied = yoke::github_app::digest::discussions::Permission {
        allow_discussions: true,
        ..Default::default()
    };
    assert!(
        yoke::github_app::digest::discussions::preview(
            &yoke::github_app::digest::discussions::Context {
                store: &engine.store,
                profile: &profile,
                options: &options,
                permission: &denied,
                source: yoke::github_app::digest::discussions::Source::Current
            },
            &request,
            &capture,
            &d
        )
        .is_err()
    );
    let blob = engine.store.dir.join("blobs").join(&snapshot.files[0].hash);
    std::fs::write(&blob, b"tampered immutable source").unwrap();
    assert!(
        yoke::github_app::digest::discussions::preview(
            &yoke::github_app::digest::discussions::Context {
                store: &engine.store,
                profile: &profile,
                options: &options,
                permission: &permission,
                source: yoke::github_app::digest::discussions::Source::Current
            },
            &request,
            &capture,
            &d
        )
        .is_err()
    );
}

#[test]
fn cli_permission_denials_do_not_create_source_state_or_read_missing_host_files() {
    let root = tempfile::tempdir().unwrap();
    let bin = env!("CARGO_BIN_EXE_yoke");
    for args in [
        vec![
            "github-discussions",
            "capture",
            "--profile",
            "missing-profile.json",
            "--request",
            "missing-request.json",
        ],
        vec![
            "--allow-writes",
            "github-discussions",
            "inspect",
            "--capture",
            "missing-capture.json",
        ],
    ] {
        let out = std::process::Command::new(bin)
            .current_dir(root.path())
            .args(args)
            .output()
            .unwrap();
        assert!(!out.status.success());
        assert!(out.stdout.is_empty());
        assert!(!root.path().join(".yoke").exists());
    }
}

#[test]
fn obvious_credential_text_and_unknown_schema_keys_never_echo_remote_values() {
    let f = Fixture::new("unsafe-text");
    let p = profile();
    let error = github::discussions::capture(&f.client(&p), &p, &input(), &Deadline::new(20000))
        .unwrap_err();
    assert!(!format!("{error:#}").contains("ghp_"));
    let f = Fixture::new("normal");
    let c = captured(&f);
    let mut value = serde_json::to_value(&c).unwrap();
    let key = format!("ghp_{}", "B".repeat(40));
    value
        .as_object_mut()
        .unwrap()
        .insert(key.clone(), json!(true));
    let error =
        Capture::parse(&serde_json::to_vec(&value).unwrap(), &Deadline::new(20000)).unwrap_err();
    assert!(!format!("{error:#}").contains(&key));
}

#[test]
fn cli_offline_digest_and_header_handoff_use_explicit_capture_and_fixture_grants() {
    let f = Fixture::new("normal");
    let capture = captured(&f);
    let root = tempfile::tempdir().unwrap();
    let host = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("main.rs"),
        "pub fn visible() { let _ = \"must stay in implementation\"; }\n",
    )
    .unwrap();
    let mut engine = yoke::Engine::open(root.path()).unwrap();
    let snapshot = engine.index(&Deadline::new(20000)).unwrap().0;
    let profile = host.path().join("profile.json");
    let request = host.path().join("request.json");
    let captured = host.path().join("capture.json");
    std::fs::write(&profile,serde_json::to_vec(&json!({"schemaVersion":1,"appId":123,"secretCredential":"must-not-be-read","grants":[{"installationId":42,"repositoryId":9,"repository":"org/repo"}]})).unwrap()).unwrap();
    std::fs::write(&request,serde_json::to_vec(&json!({"schemaVersion":1,"snapshotId":snapshot.id,"installationId":42,"repositoryId":9,"repository":"org/repo"})).unwrap()).unwrap();
    std::fs::write(&captured, serde_json::to_vec(&capture).unwrap()).unwrap();
    let before = f.calls.lock().unwrap().len();
    for handoff in [false, true] {
        let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_yoke"));
        cmd.arg("--root")
            .arg(root.path())
            .args(["--max-bytes", "4000000", "github-app", "--profile"])
            .arg(&profile)
            .args([
                "digest",
                "--allow-digest",
                "--allow-discussions",
                "--allow-fixture-discussions",
                "--discussion-capture",
            ])
            .arg(&captured)
            .args(["preview", "--request"])
            .arg(&request);
        if handoff {
            cmd.arg("--handoff-preview");
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000);
        }
        let out = cmd.output().unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let value: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(value["networkRequests"], 0);
        if handoff {
            assert_eq!(value["serviceSend"], "unavailable");
            assert_eq!(value["payload"]["discussionTextIncluded"], false);
        } else {
            assert_eq!(value["discussionCaptureId"], capture.id);
            assert_eq!(value["threads"][0]["observations"][1]["body"], "why 🌍\r\n");
        }
    }
    assert_eq!(f.calls.lock().unwrap().len(), before);
}

#[test]
fn exact_output_capacity_never_silently_clips_discussion_observations() {
    let f = Fixture::new("normal");
    let c = captured(&f);
    let exact = serde_json::to_vec(&c).unwrap().len();
    let mut p = profile();
    p.max_output_bytes = exact;
    let c =
        github::discussions::capture(&f.client(&p), &p, &input(), &Deadline::new(20000)).unwrap();
    assert_eq!(serde_json::to_vec(&c).unwrap().len(), exact);
    p.max_output_bytes = exact - 1;
    assert!(
        github::discussions::capture(&f.client(&p), &p, &input(), &Deadline::new(20000)).is_err()
    );
}

#[test]
fn source_associations_distinguish_verified_history_from_selected_immutable_captures() {
    use yoke::{
        analysis::AnalyzerEpoch,
        git_workflows::{self, HostGit},
        github_app::digest::{self, discussions as combined},
    };
    let root = tempfile::tempdir().unwrap();
    let git = HostGit::discover().unwrap();
    let command = |args: &[&str]| {
        let null = if cfg!(windows) { "NUL" } else { "/dev/null" };
        let out = std::process::Command::new(&git.executable)
            .current_dir(root.path())
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
                "user.name=Owned Discussion Fixture",
                "-c",
                "user.email=fixture@example.invalid",
            ])
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "owned Git setup failed");
        String::from_utf8(out.stdout).unwrap().trim().to_owned()
    };
    command(&["init", "-q"]);
    std::fs::write(root.path().join(".gitignore"), ".yoke/\n").unwrap();
    std::fs::write(root.path().join("main.rs"), "pub fn prior(){}\n").unwrap();
    command(&["add", "--all"]);
    command(&["commit", "-q", "-m", "prior"]);
    let prior_sha = command(&["rev-parse", "HEAD"]);
    let mut engine = yoke::Engine::open(root.path()).unwrap();
    let d = Deadline::new(20000);
    let prior_disk = engine.index(&d).unwrap().0;
    let prior = git_workflows::capture_commit_at(
        AnalyzerEpoch::V8,
        &mut engine,
        &git,
        &prior_disk,
        &prior_sha,
        &d,
    )
    .unwrap()
    .0;
    std::fs::write(root.path().join("main.rs"), "pub fn newer(){}\n").unwrap();
    command(&["add", "--all"]);
    command(&["commit", "-q", "-m", "newer"]);
    let sha = command(&["rev-parse", "HEAD"]);
    let disk = engine.index(&d).unwrap().0;
    let snapshot =
        git_workflows::capture_commit_at(AnalyzerEpoch::V8, &mut engine, &git, &disk, &sha, &d)
            .unwrap()
            .0;
    let f = Fixture::with_head("normal", prior_sha.clone());
    let capture = captured(&f);
    let profile=yoke::github_app::HostProfile::parse(&serde_json::to_vec(&json!({"schemaVersion":1,"appId":123,"secretCredential":"must-not-be-read","grants":[{"installationId":42,"repositoryId":9,"repository":"org/repo"}]})).unwrap()).unwrap();
    let request = digest::PreviewRequest {
        schema_version: 1,
        snapshot_id: snapshot.id.clone(),
        installation_id: 42,
        repository_id: 9,
        repository: "org/repo".into(),
    };
    let options = digest::Options::default();
    let history = digest::HistoryOptions::default();
    let permission = combined::Permission {
        allow_discussions: true,
        allow_fixture: true,
        handoff_preview: false,
    };
    let selection = digest::MappingSelection {
        snapshot_ids: vec![prior.id.clone()],
        limits: Default::default(),
    };
    for (mode, expected) in [
        (combined::Source::Current, "unavailable"),
        (
            combined::Source::History {
                git: &git,
                options: &history,
                mapping: None,
            },
            "verified_commit_without_selected_source_capture",
        ),
        (
            combined::Source::History {
                git: &git,
                options: &history,
                mapping: Some(&selection),
            },
            "reported_head_matches_locally_verified_capture",
        ),
    ] {
        let value = combined::preview(
            &combined::Context {
                store: &engine.store,
                profile: &profile,
                options: &options,
                permission: &permission,
                source: mode,
            },
            &request,
            &capture,
            &d,
        )
        .unwrap();
        assert_eq!(value["threads"][0]["sourceAssociation"]["status"], expected);
        assert_eq!(
            value["threads"][0]["sourceAssociation"]["remoteRepositoryVerified"],
            false
        );
        if expected == "reported_head_matches_locally_verified_capture" {
            assert_eq!(
                value["threads"][0]["sourceAssociation"]["snapshotIds"],
                json!([prior.id])
            );
        }
    }
}
