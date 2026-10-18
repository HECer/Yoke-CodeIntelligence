//! Positive Windows CLI operations with a public fixture key owned by an outer controller.
use super::*;

fn state(f: &Fixture) -> std::collections::BTreeMap<String, Vec<Vec<String>>> {
    use rusqlite::types::ValueRef;
    let db = &f.engine.store.db;
    let tables=db.prepare("SELECT name FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").unwrap()
        .query_map([],|r|r.get::<_,String>(0)).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
    let mut result = std::collections::BTreeMap::new();
    for table in tables {
        let mut query = db
            .prepare(&format!("SELECT * FROM \"{}\"", table.replace('"', "\"\"")))
            .unwrap();
        let columns = query.column_count();
        let mut cursor = query.query([]).unwrap();
        let mut values = Vec::new();
        while let Some(row) = cursor.next().unwrap() {
            let mut cells = Vec::new();
            for col in 0..columns {
                cells.push(match row.get_ref(col).unwrap() {
                    ValueRef::Null => "null".into(),
                    ValueRef::Integer(n) => format!("integer:{n}"),
                    ValueRef::Real(n) => format!("real:{:016x}", n.to_bits()),
                    ValueRef::Text(v) => format!("text:{}", hex::encode(v)),
                    ValueRef::Blob(v) => format!("blob:{}", hex::encode(v)),
                });
            }
            values.push(cells);
        }
        values.sort();
        result.insert(table, values);
    }
    result
}
fn cas(f: &Fixture) -> std::collections::BTreeMap<String, String> {
    f.blobs()
        .into_iter()
        .map(|n| {
            let hash = yoke::model::hash(
                std::fs::read(f.engine.store.dir.join("blobs").join(&n)).unwrap(),
            );
            (n, hash)
        })
        .collect()
}
#[test]
#[ignore = "Run with the outer Windows credential controller so guard termination cannot leak its owned key"]
fn owned_cli_preview_store_retry_remove_and_readonly_service() {
    let name = std::env::var("YOKE_PAGES_ORACLE_CREDENTIAL").expect("explicit outer key owner");
    let nonce = name
        .strip_prefix("test-pages-")
        .expect("owned fixture key name");
    assert!(
        nonce.len() == 32
            && nonce
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    );
    let loaded = yoke::host_credentials::read_github_webhook_secret(&name).unwrap();
    assert!(
        loaded.as_slice() == KEY,
        "owned fixture credential mismatch"
    );
    drop(loaded);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let f = Fixture::new_at(now);
    let provider = Provider::with_time(Mode::Exact, None, 42, None, None, 7, now);
    let done = worker::run(
        &f.context(),
        &mut f.client(&provider),
        &f.request(),
        &remote(),
        &f.d,
    )
    .unwrap();
    assert_eq!(provider.seen.lock().unwrap().len(), 8);
    drop(provider);
    let documents = tempfile::tempdir().unwrap();
    let profile = documents.path().join("profile.json");
    let input = documents.path().join("input.json");
    std::fs::write(
        &profile,
        serde_json::to_vec(
            &json!({"schemaVersion":1,"appId":123,"secretCredential":name,
        "grants":[{"installationId":42,"repositoryId":9,"repository":"acme/demo"}]}),
        )
        .unwrap(),
    )
    .unwrap();
    let binary_hash = yoke::model::hash(std::fs::read(env!("CARGO_BIN_EXE_yoke")).unwrap());
    let run = |operation: &str, writes: bool| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_yoke"));
        command.current_dir(documents.path()).args([
            "--root",
            f.root.path().to_str().unwrap(),
            "--timeout",
            "30000",
            "--max-bytes",
            "4000000",
        ]);
        if writes {
            command.arg("--allow-writes");
        }
        command.args(["github-app", "--profile", profile.to_str().unwrap()]);
        if operation != "serve" {
            command.arg("--allow-webhooks");
        }
        command.args(["pages", "--ttl-seconds", "60"]);
        if operation != "preview" {
            command.arg("--allow-pages");
        }
        if operation == "serve" {
            command.args(["--allow-page-serving", "serve", "--lifetime-ms", "150"]);
        } else {
            command.args([operation, "--request", input.to_str().unwrap()]);
        }
        let result = yoke::process::run_capture(
            command,
            &[],
            &Deadline::new(40_000),
            yoke::process::CaptureLimits {
                stdin_bytes: 0,
                stdout_bytes: 4_000_000,
                stderr_bytes: 64_000,
            },
        )
        .unwrap();
        assert!(
            !result.timed_out
                && !result.output_exceeded
                && !result.io_error
                && result.output_complete
        );
        assert!(
            !result.stdout.windows(KEY.len()).any(|w| w == KEY)
                && !result.stderr.windows(KEY.len()).any(|w| w == KEY)
        );
        result
    };
    let value = |operation: &str, writes: bool| {
        let result = run(operation, writes);
        assert_eq!(
            result.exit_code,
            Some(0),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(result.stderr.is_empty() && result.stdout.ends_with(b"\n"));
        serde_json::from_slice::<Value>(&result.stdout).unwrap()
    };
    let before = state(&f);
    let original_records = rows(&f);
    let unchanged_records = || {
        rows(&f)
            .into_iter()
            .filter(|(kind, _, _)| {
                kind != pages::RECORD_KIND && kind != "github_app_page_revision_v1"
            })
            .collect::<Vec<_>>()
    };
    let blobs = cas(&f);
    let preq = pages::PreviewRequest {
        schema_version: 1,
        completion_id: done["completionId"].as_str().unwrap().into(),
        expected_queue_revision: 2,
        public_base_url: "https://reports.example.invalid".into(),
    };
    std::fs::write(&input, serde_json::to_vec(&preq).unwrap()).unwrap();
    let p = value("preview", false);
    assert_eq!(p, preview(&f, &done, &options(&f)));
    assert_eq!(state(&f), before);
    std::fs::write(&input, serde_json::to_vec(&request(&p)).unwrap()).unwrap();
    let stored = value("store", true);
    assert_eq!(stored["pageId"], p["pageId"]);
    assert_eq!(stored["status"], "stored");
    assert_eq!(stored["persisted"], true);
    assert_eq!(stored["pageVersion"], 1);
    assert_eq!(
        stored["expiresAtUnix"].as_u64().unwrap() - stored["storedAtUnix"].as_u64().unwrap(),
        60
    );
    let after_store = state(&f);
    for (table, rows) in &before {
        if table != "records" {
            assert_eq!(after_store.get(table), Some(rows));
        }
    }
    assert_eq!(after_store.len(), before.len());
    let logical = rows(&f);
    assert_eq!(logical.len(), before["records"].len() + 2);
    assert_eq!(unchanged_records(), original_records);
    let read_options = options(&f);
    let read_clock = worker::Clock::system();
    let read_context = pages::Context {
        store: &f.engine.store,
        host: &f.host,
        options: &read_options,
        clock: &read_clock,
    };
    assert_eq!(
        pages::read_page(&read_context, id(&p), token(&p), &f.d)
            .unwrap()
            .unwrap()
            .html(),
        p["html"].as_str().unwrap()
    );
    let publication_input = documents.path().join("publication-preview.json");
    std::fs::write(
        &publication_input,
        serde_json::to_vec(&json!({
            "schemaVersion":1,"completionId":done["completionId"],"expectedQueueRevision":2,
            "page":{"pageId":p["pageId"],"expectedPageVersion":1}
        }))
        .unwrap(),
    )
    .unwrap();
    for allow in [false, true] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_yoke"));
        command.current_dir(documents.path()).args([
            "--root",
            f.root.path().to_str().unwrap(),
            "--timeout",
            "30000",
            "--max-bytes",
            "4000000",
            "github-app",
            "--allow-webhooks",
            "--profile",
            profile.to_str().unwrap(),
            "publication",
        ]);
        if allow {
            command.arg("--allow-page-links");
        }
        command.args(["preview", "--request", publication_input.to_str().unwrap()]);
        let result = yoke::process::run_capture(
            command,
            &[],
            &Deadline::new(40_000),
            yoke::process::CaptureLimits {
                stdin_bytes: 0,
                stdout_bytes: 4_000_000,
                stderr_bytes: 64_000,
            },
        )
        .unwrap();
        assert!(
            !result.timed_out
                && !result.output_exceeded
                && !result.io_error
                && result.output_complete
        );
        assert!(
            !result.stdout.windows(KEY.len()).any(|w| w == KEY)
                && !result.stderr.windows(KEY.len()).any(|w| w == KEY)
        );
        if allow {
            assert_eq!(
                result.exit_code,
                Some(0),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            assert!(result.stderr.is_empty() && result.stdout.ends_with(b"\n"));
            let linked: Value = serde_json::from_slice(&result.stdout).unwrap();
            assert_eq!(linked["rendererVersion"], "yoke-pr-comment-v2");
            assert_eq!(
                linked["pageLink"],
                serde_json::to_value(
                    pages::approved_link(
                        &read_context,
                        &pages::LinkReference {
                            page_id: id(&p).into(),
                            expected_page_version: 1
                        },
                        &f.d
                    )
                    .unwrap()
                )
                .unwrap()
            );
            assert!(
                linked["body"]
                    .as_str()
                    .unwrap()
                    .contains(p["url"].as_str().unwrap())
            );
            assert_eq!(linked["publicationAuthority"], false);
            assert_eq!(linked["networkRequests"], 0);
        } else {
            assert!(result.exit_code != Some(0) && result.stdout.is_empty());
            assert!(
                String::from_utf8_lossy(&result.stderr)
                    .contains("github_app_page_links_permission_required")
            );
        }
        assert_eq!(state(&f), after_store);
    }
    let retry = value("store", true);
    assert_eq!(retry["historical"], true);
    assert_eq!(retry["expiresAtUnix"], stored["expiresAtUnix"]);
    assert_eq!(state(&f), after_store);
    let remove = pages::RemoveRequest {
        schema_version: 1,
        page_id: id(&p).into(),
        expected_page_version: 1,
        expected_revision: stored["pageRevision"].as_u64().unwrap(),
        approval: "remove_exact_page".into(),
    };
    std::fs::write(&input, serde_json::to_vec(&remove).unwrap()).unwrap();
    let removed = value("remove", true);
    assert_eq!(removed["removed"], true);
    assert!(
        pages::read_page(&read_context, id(&p), token(&p), &f.d)
            .unwrap()
            .is_none()
    );
    let after_remove = state(&f);
    assert_eq!(unchanged_records(), original_records);
    for (table, rows) in &before {
        if table != "records" {
            assert_eq!(after_remove.get(table), Some(rows));
        }
    }
    assert_eq!(rows(&f).len(), original_records.len() + 1);
    std::fs::write(&input, serde_json::to_vec(&request(&p)).unwrap()).unwrap();
    let stale = run("store", true);
    assert!(stale.exit_code != Some(0) && stale.stdout.is_empty());
    assert!(String::from_utf8_lossy(&stale.stderr).contains("github_app_pages_plan_stale"));
    assert_eq!(state(&f), after_remove);
    let service = value("serve", false);
    assert_eq!(service["pageServing"], true);
    assert_eq!(service["webhookServing"], false);
    assert_eq!(service["durableQueue"], false);
    let url = url::Url::parse(service["endpoint"].as_str().unwrap()).unwrap();
    let address: SocketAddr = format!("{}:{}", url.host_str().unwrap(), url.port().unwrap())
        .parse()
        .unwrap();
    assert!(std::net::TcpStream::connect_timeout(&address, Duration::from_millis(200)).is_err());
    assert_eq!(state(&f), after_remove);
    assert_eq!(cas(&f), blobs);
    assert_eq!(
        std::fs::read(f.root.path().join(PATH)).unwrap(),
        SOURCE.as_bytes()
    );
    assert_eq!(
        yoke::model::hash(std::fs::read(env!("CARGO_BIN_EXE_yoke")).unwrap()),
        binary_hash
    );
    println!(
        "{}",
        json!({"oracle":"owned_pages_cli_v1","fixtureOnly":true,"binarySha256":binary_hash,
        "cliCalls":8,"linkedPreviewVerified":true,"systemClockOnly":true,"logicalTablesChecked":before.len(),"casFilesHashed":blobs.len(),
        "sourceBytesUnchanged":true,"readonlyServiceRetired":true,"liveGitHubValidated":false,"externallyPublished":false})
    );
}
