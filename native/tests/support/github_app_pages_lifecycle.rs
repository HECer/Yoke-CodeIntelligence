//! Real completed-source page lifecycle contracts; no external publication.
use super::*;
use yoke::github_app::pages;
#[path = "github_app_pages_http.rs"]
mod http;
#[path = "github_app_pages_links.rs"]
mod links;
#[path = "github_app_pages_oracle.rs"]
mod oracle;
#[path = "github_app_pages_publication_links.rs"]
mod publication_links;

fn options(f: &Fixture) -> pages::Options {
    pages::Options {
        allow_pages: true,
        worker: f.options.clone(),
        ttl_seconds: 60,
        ..Default::default()
    }
}
fn context<'a>(f: &'a Fixture, o: &'a pages::Options) -> pages::Context<'a> {
    pages::Context {
        store: &f.engine.store,
        host: &f.host,
        options: o,
        clock: &f.clock,
    }
}
fn preview(f: &Fixture, completion: &Value, o: &pages::Options) -> Value {
    pages::preview(
        &f.engine.store,
        &f.host,
        &pages::PreviewRequest {
            schema_version: 1,
            completion_id: completion["completionId"].as_str().unwrap().into(),
            expected_queue_revision: 2,
            public_base_url: "https://reports.example.invalid".into(),
        },
        o,
        &f.d,
    )
    .unwrap()
}
fn request(p: &Value) -> pages::StoreRequest {
    pages::StoreRequest {
        schema_version: 1,
        completion_id: p["completionId"].as_str().unwrap().into(),
        expected_queue_revision: p["queueRevision"].as_u64().unwrap() as usize,
        public_base_url: p["publicBaseUrl"].as_str().unwrap().into(),
        proposal_id: p["proposalId"].as_str().unwrap().into(),
        html_hash: p["htmlHash"].as_str().unwrap().into(),
        approval: "store_exact_page".into(),
    }
}
fn token(p: &Value) -> &str {
    p["url"].as_str().unwrap().split_once("?t=").unwrap().1
}
fn id(p: &Value) -> &str {
    p["pageId"].as_str().unwrap()
}
fn rows(f: &Fixture) -> Vec<(String, String, String)> {
    f.engine
        .store
        .db
        .prepare("SELECT kind,id,value FROM records ORDER BY kind,id")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap()
}
fn read(f: &Fixture, p: &Value, o: &pages::Options) -> anyhow::Result<Option<pages::ReadPage>> {
    pages::read_page(&context(f, o), id(p), token(p), &f.d)
}

#[test]
fn stored_exact_page_survives_reopen_and_exact_retry_does_not_refresh_expiry() {
    let f = Fixture::new();
    let done = completed(&f);
    let o = options(&f);
    let before = rows(&f);
    let blobs = f.blobs();
    let p = preview(&f, &done, &o);
    assert_eq!(rows(&f), before);
    let stored = pages::store_page(&context(&f, &o), &request(&p), &f.d).unwrap();
    assert_eq!(stored["persisted"], true);
    assert_eq!(stored["historical"], false);
    assert_eq!(stored["pageVersion"], 1);
    assert_eq!(stored["expiresAtUnix"], NOW + 60);
    assert_eq!(stored["networkRequests"], 0);
    assert_eq!(stored["publicationAuthority"], false);
    let after = rows(&f);
    assert_eq!(after.len(), before.len() + 2);
    let page = read(&f, &p, &o).unwrap().unwrap();
    assert_eq!(page.html(), p["html"].as_str().unwrap());
    assert_eq!(page.html_hash(), p["htmlHash"].as_str().unwrap());
    f.time.store(NOW + 30, Ordering::SeqCst);
    let retry = pages::store_page(&context(&f, &o), &request(&p), &f.d).unwrap();
    assert_eq!(retry["historical"], true);
    assert_eq!(retry["expiresAtUnix"], stored["expiresAtUnix"]);
    assert_eq!(rows(&f), after);
    let reopened = Engine::open(f.root.path()).unwrap();
    let clock = worker::Clock::for_fixture(f.time.clone());
    let c = pages::Context {
        store: &reopened.store,
        host: &f.host,
        options: &o,
        clock: &clock,
    };
    assert_eq!(
        pages::read_page(&c, id(&p), token(&p), &f.d)
            .unwrap()
            .unwrap()
            .html(),
        page.html()
    );
    assert_eq!(f.blobs(), blobs);
    assert_eq!(
        std::fs::read(f.root.path().join(PATH)).unwrap(),
        SOURCE.as_bytes()
    );
}

#[test]
fn page_writes_require_separate_permissions_and_exact_approval_without_mutation() {
    let f = Fixture::new();
    let done = completed(&f);
    let o = options(&f);
    let p = preview(&f, &done, &o);
    let before = rows(&f);
    let req = request(&p);
    let mut denied = o.clone();
    denied.allow_pages = false;
    fail_marker(
        pages::store_page(&context(&f, &denied), &req, &f.d),
        "github_app_pages_permission_required",
    );
    denied = o.clone();
    denied.worker.queue.allow_writes = false;
    fail_marker(
        pages::store_page(&context(&f, &denied), &req, &f.d),
        "github_app_permission_required",
    );
    let mut bad = req.clone();
    bad.approval = "displayed_repository_text_is_not_approval".into();
    fail_marker(
        pages::store_page(&context(&f, &o), &bad, &f.d),
        "github_app_pages_approval_required",
    );
    bad = req.clone();
    bad.html_hash = "0".repeat(64);
    fail_marker(
        pages::store_page(&context(&f, &o), &bad, &f.d),
        "github_app_pages_plan_stale",
    );
    assert_eq!(rows(&f), before);
    pages::store_page(&context(&f, &o), &req, &f.d).unwrap();
}

#[test]
fn expiry_is_read_only_and_removal_invalidates_old_approval_until_a_fresh_preview() {
    let f = Fixture::new();
    let done = completed(&f);
    let o = options(&f);
    let p = preview(&f, &done, &o);
    let stored = pages::store_page(&context(&f, &o), &request(&p), &f.d).unwrap();
    let after = rows(&f);
    f.time.store(NOW + 60, Ordering::SeqCst);
    assert!(read(&f, &p, &o).unwrap().is_none());
    assert_eq!(rows(&f), after);
    let req = pages::RemoveRequest {
        schema_version: 1,
        page_id: id(&p).into(),
        expected_page_version: stored["pageVersion"].as_u64().unwrap(),
        expected_revision: stored["pageRevision"].as_u64().unwrap(),
        approval: "remove_exact_page".into(),
    };
    pages::remove_page(&context(&f, &o), &req, &f.d).unwrap();
    let removed = rows(&f);
    assert!(read(&f, &p, &o).unwrap().is_none());
    fail_marker(
        pages::store_page(&context(&f, &o), &request(&p), &f.d),
        "github_app_pages_plan_stale",
    );
    assert_eq!(rows(&f), removed);
    let fresh = preview(&f, &done, &o);
    assert_eq!(fresh["url"], p["url"]);
    assert_ne!(fresh["proposalId"], p["proposalId"]);
    let recreated = pages::store_page(&context(&f, &o), &request(&fresh), &f.d).unwrap();
    assert_eq!(recreated["expiresAtUnix"], NOW + 120);
    assert!(read(&f, &fresh, &o).unwrap().is_some());
}

#[test]
fn replacement_refreshes_only_after_fresh_approval_and_eviction_keeps_stale_plans_dead() {
    let mut f = Fixture::new();
    let first = completed(&f);
    let mut o = options(&f);
    o.max_pages = 1;
    let p = preview(&f, &first, &o);
    pages::store_page(&context(&f, &o), &request(&p), &f.d).unwrap();
    f.time.store(NOW + 10, Ordering::SeqCst);
    let fresh = preview(&f, &first, &o);
    assert_eq!(fresh["htmlHash"], p["htmlHash"]);
    let updated = pages::store_page(&context(&f, &o), &request(&fresh), &f.d).unwrap();
    assert_eq!(updated["expiresAtUnix"], NOW + 70);
    assert_eq!(updated["historical"], false);
    let second = complete_another_pr(&mut f, 8);
    let other = preview(&f, &second, &o);
    pages::store_page(&context(&f, &o), &request(&other), &f.d).unwrap();
    assert!(read(&f, &p, &o).unwrap().is_none());
    assert!(read(&f, &other, &o).unwrap().is_some());
    fail_marker(
        pages::store_page(&context(&f, &o), &request(&fresh), &f.d),
        "github_app_pages_plan_stale",
    );
    assert_eq!(
        f.engine
            .store
            .db
            .query_row(
                "SELECT count(*) FROM records WHERE kind=?1",
                [pages::RECORD_KIND],
                |r| r.get::<_, usize>(0)
            )
            .unwrap(),
        1
    );
}

#[test]
fn reads_authenticate_before_replay_and_recheck_current_grant_and_captured_bytes() {
    let mut f = Fixture::new();
    let done = completed(&f);
    let o = options(&f);
    let p = preview(&f, &done, &o);
    pages::store_page(&context(&f, &o), &request(&p), &f.d).unwrap();
    let before = rows(&f);
    for bad in [
        "".into(),
        "0".repeat(64),
        "../page".into(),
        "a".repeat(4097),
    ] {
        assert!(
            pages::read_page(&context(&f, &o), id(&p), &bad, &f.d)
                .unwrap()
                .is_none()
        );
    }
    assert!(
        pages::read_page(&context(&f, &o), "../page", token(&p), &f.d)
            .unwrap()
            .is_none()
    );
    f.host = host_for(43);
    assert!(read(&f, &p, &o).unwrap().is_some());
    assert_eq!(
        pages::store_page(&context(&f, &o), &request(&p), &f.d).unwrap()["historical"],
        true
    );
    fail_marker(
        pages::preview(
            &f.engine.store,
            &f.host,
            &pages::PreviewRequest {
                schema_version: 1,
                completion_id: done["completionId"].as_str().unwrap().into(),
                expected_queue_revision: 2,
                public_base_url: "https://reports.example.invalid".into(),
            },
            &o,
            &f.d,
        ),
        "github_app_worker_grant_denied",
    );
    std::fs::remove_file(
        f.engine
            .store
            .dir
            .join("blobs")
            .join(yoke::model::hash(SOURCE.as_bytes())),
    )
    .unwrap();
    fail_marker(read(&f, &p, &o), "github_app_worker_source_unavailable");
    assert!(
        pages::read_page(&context(&f, &o), id(&p), &"0".repeat(64), &f.d)
            .unwrap()
            .is_none()
    );
    assert_eq!(rows(&f), before);
}

#[test]
fn request_parsers_reject_duplicate_unknown_and_oversized_input_before_storage() {
    let valid = json!({"schemaVersion":1,"completionId":"a".repeat(64),
        "expectedQueueRevision":2,"publicBaseUrl":"https://reports.example.invalid",
        "proposalId":"b".repeat(64),"htmlHash":"c".repeat(64),"approval":"store_exact_page"});
    let raw = serde_json::to_vec(&valid).unwrap();
    assert!(pages::StoreRequest::parse(&raw).is_ok());
    let mut bad = valid.clone();
    bad["html"] = json!("caller HTML is never authoritative");
    fail_marker(
        pages::StoreRequest::parse(&serde_json::to_vec(&bad).unwrap()),
        "github_app_pages_input_invalid",
    );
    let duplicate =
        String::from_utf8(raw.clone())
            .unwrap()
            .replacen("{", "{\"schemaVersion\":1,", 1);
    fail_marker(
        pages::StoreRequest::parse(duplicate.as_bytes()),
        "github_app_pages_input_invalid",
    );
    let mut padded = raw.clone();
    assert!(padded.len() < 4096);
    padded.resize(4096, b' ');
    assert!(pages::StoreRequest::parse(&padded).is_ok());
    padded.push(b' ');
    fail_marker(
        pages::StoreRequest::parse(&padded),
        "github_app_pages_input_invalid",
    );
    let remove = json!({"schemaVersion":1,"pageId":"a".repeat(64),"expectedPageVersion":1,
        "expectedRevision":2,"approval":"remove_exact_page"});
    assert!(pages::RemoveRequest::parse(&serde_json::to_vec(&remove).unwrap()).is_ok());
    let mut bad = remove;
    bad["path"] = json!("../other");
    fail_marker(
        pages::RemoveRequest::parse(&serde_json::to_vec(&bad).unwrap()),
        "github_app_pages_input_invalid",
    );
}

#[test]
fn storage_preserves_caller_transactions_sql_policy_and_refuses_weak_durability() {
    use rusqlite::limits::Limit;
    let f = Fixture::new();
    let done = completed(&f);
    let o = options(&f);
    let p = preview(&f, &done, &o);
    let before = rows(&f);
    f.engine.store.db.execute_batch("CREATE TEMP TABLE owned_page_tx(v INTEGER); BEGIN; INSERT INTO owned_page_tx VALUES(7)").unwrap();
    fail_marker(
        pages::store_page(&context(&f, &o), &request(&p), &f.d),
        "github_app_pages_caller_transaction_refused",
    );
    assert!(!f.engine.store.db.is_autocommit());
    assert_eq!(
        f.engine
            .store
            .db
            .query_row("SELECT sum(v) FROM owned_page_tx", [], |r| r
                .get::<_, usize>(0))
            .unwrap(),
        7
    );
    f.engine
        .store
        .db
        .execute_batch("ROLLBACK; PRAGMA synchronous=NORMAL")
        .unwrap();
    fail_marker(
        pages::store_page(&context(&f, &o), &request(&p), &f.d),
        "github_app_pages_sql_durability_required",
    );
    assert_eq!(rows(&f), before);
    f.engine
        .store
        .db
        .execute_batch("PRAGMA synchronous=FULL; PRAGMA busy_timeout=937")
        .unwrap();
    f.engine
        .store
        .db
        .set_limit(Limit::SQLITE_LIMIT_LENGTH, 90_000_000)
        .unwrap();
    pages::store_page(&context(&f, &o), &request(&p), &f.d).unwrap();
    assert_eq!(
        f.engine.store.db.limit(Limit::SQLITE_LIMIT_LENGTH).unwrap(),
        90_000_000
    );
    assert_eq!(
        f.engine
            .store
            .db
            .query_row("PRAGMA busy_timeout", [], |r| r.get::<_, u64>(0))
            .unwrap(),
        937
    );
    assert!(f.engine.store.db.is_autocommit());
    let after = rows(&f);
    let expired = Deadline::new(0);
    assert!(pages::store_page(&context(&f, &o), &request(&p), &expired).is_err());
    assert_eq!(rows(&f), after);
}

#[test]
fn page_clocks_reject_in_process_regression_and_a_fresh_clock_below_the_persisted_floor() {
    let f = Fixture::new();
    let done = completed(&f);
    let o = options(&f);
    let p = preview(&f, &done, &o);
    pages::store_page(&context(&f, &o), &request(&p), &f.d).unwrap();
    let after = rows(&f);
    f.time.store(NOW + 10, Ordering::SeqCst);
    assert!(read(&f, &p, &o).unwrap().is_some());
    f.time.store(NOW + 9, Ordering::SeqCst);
    fail_marker(read(&f, &p, &o), "github_app_worker_clock_regressed");
    f.time.store(NOW - 1, Ordering::SeqCst);
    let clock = worker::Clock::for_fixture(f.time.clone());
    let reopened = Engine::open(f.root.path()).unwrap();
    let c = pages::Context {
        store: &reopened.store,
        host: &f.host,
        options: &o,
        clock: &clock,
    };
    fail_marker(
        pages::read_page(&c, id(&p), token(&p), &f.d),
        "github_app_pages_clock_regressed",
    );
    assert_eq!(rows(&f), after);
}

#[test]
fn preview_names_the_conditional_bearer_read_capability_without_granting_a_write() {
    let f = Fixture::new();
    let done = completed(&f);
    let o = options(&f);
    let p = preview(&f, &done, &o);
    assert!(
        p.get("pageAuthority").is_none(),
        "ambiguous authority flag must be replaced"
    );
    assert_eq!(p["pageWriteAuthority"], false);
    assert_eq!(
        p["bearerReadCapability"],
        "conditional_on_stored_unexpired_page_current_key_and_repository_grant"
    );
    assert_eq!(p["publicationAuthority"], false);
}

#[test]
fn invalid_page_mac_is_rejected_but_an_invalid_bearer_never_reads_the_corrupt_row() {
    let f = Fixture::new();
    let done = completed(&f);
    let o = options(&f);
    let p = preview(&f, &done, &o);
    pages::store_page(&context(&f, &o), &request(&p), &f.d).unwrap();
    let raw: String = f
        .engine
        .store
        .db
        .query_row(
            "SELECT value FROM records WHERE kind=?1 AND id=?2",
            rusqlite::params![pages::RECORD_KIND, id(&p)],
            |r| r.get(0),
        )
        .unwrap();
    let mut corrupt: Value = serde_json::from_str(&raw).unwrap();
    corrupt["signature256"] = json!(format!("sha256={}", "0".repeat(64)));
    f.engine
        .store
        .db
        .execute(
            "UPDATE records SET value=?3 WHERE kind=?1 AND id=?2",
            rusqlite::params![
                pages::RECORD_KIND,
                id(&p),
                serde_json::to_string(&corrupt).unwrap()
            ],
        )
        .unwrap();
    let after = rows(&f);
    assert!(
        pages::read_page(&context(&f, &o), id(&p), &"0".repeat(64), &f.d)
            .unwrap()
            .is_none()
    );
    fail_marker(read(&f, &p, &o), "github_app_pages_binding_invalid");
    assert_eq!(rows(&f), after);
}

#[test]
fn adding_an_app_revision_respects_the_global_hard_bound_before_committing() {
    for foreign_count in [999, 1000] {
        let f = Fixture::new();
        let done = completed(&f);
        let o = options(&f);
        for index in 0..foreign_count {
            let app_id = 1000 + index;
            let key = yoke::model::identity(&json!({"domain":"yoke-page-revision-v1",
                "workspaceId":f.engine.store.workspace_id,"appId":app_id}));
            let mut revision = json!({"schemaVersion":1,"workspaceId":f.engine.store.workspace_id,
                "appId":app_id,"revision":2,"atUnix":NOW});
            let mut mac = Hmac::<sha2::Sha256>::new_from_slice(KEY).unwrap();
            mac.update(b"yoke-code-intelligence/github-app-page-cache/v1\n");
            mac.update(
                &serde_json::to_vec(&json!({"kind":"github_app_page_revision_v1","data":revision}))
                    .unwrap(),
            );
            revision["signature256"] = json!(format!(
                "sha256={}",
                hex::encode(mac.finalize().into_bytes())
            ));
            f.engine
                .store
                .db
                .execute(
                    "INSERT INTO records(kind,id,value) VALUES(?1,?2,?3)",
                    rusqlite::params!["github_app_page_revision_v1", key, revision.to_string()],
                )
                .unwrap();
        }
        let p = preview(&f, &done, &o);
        let before = rows(&f);
        let result = pages::store_page(&context(&f, &o), &request(&p), &f.d);
        if foreign_count == 999 {
            assert_eq!(result.unwrap()["persisted"], true);
            assert!(read(&f, &p, &o).unwrap().is_some());
        } else {
            fail_marker(result, "github_app_pages_capacity_exceeded");
            assert_eq!(rows(&f), before);
        }
    }
}

#[test]
fn expiry_and_clock_regression_during_source_replay_are_checked_before_html_release() {
    use rusqlite::hooks::{AuthAction, AuthContext, Authorization};
    for next_time in [NOW + 60, NOW - 1] {
        let f = Fixture::new();
        let done = completed(&f);
        let o = options(&f);
        let p = preview(&f, &done, &o);
        pages::store_page(&context(&f, &o), &request(&p), &f.d).unwrap();
        let before = rows(&f);
        let observed = Arc::new(AtomicBool::new(false));
        let seen = observed.clone();
        let time = f.time.clone();
        f.engine
            .store
            .db
            .authorizer(Some(move |c: AuthContext<'_>| {
                if matches!(
                    c.action,
                    AuthAction::Read {
                        table_name: "graphs",
                        ..
                    }
                ) {
                    seen.store(true, Ordering::SeqCst);
                    time.store(next_time, Ordering::SeqCst);
                }
                Authorization::Allow
            }));
        let result = read(&f, &p, &o);
        f.engine
            .store
            .db
            .authorizer(None::<fn(AuthContext<'_>) -> Authorization>);
        assert!(
            observed.load(Ordering::SeqCst),
            "the real source graph must have been replayed"
        );
        if next_time == NOW + 60 {
            assert!(
                result.unwrap().is_none(),
                "HTML expired during replay must not be released"
            );
        } else {
            fail_marker(result, "github_app_worker_clock_regressed");
        }
        assert_eq!(rows(&f), before);
    }
}

#[test]
fn unchanged_foreign_revision_whitespace_counts_toward_exact_store_byte_admission() {
    let f = Fixture::new();
    let done = completed(&f);
    let mut o = options(&f);
    let p = preview(&f, &done, &o);
    pages::store_page(&context(&f, &o), &request(&p), &f.d).unwrap();
    let original_bytes: usize = f
        .engine
        .store
        .db
        .query_row(
            "SELECT sum(length(CAST(value AS BLOB))) FROM records WHERE kind IN (?1,?2)",
            rusqlite::params![pages::RECORD_KIND, "github_app_page_revision_v1"],
            |r| r.get(0),
        )
        .unwrap();
    let key = yoke::model::identity(&json!({"domain":"yoke-page-revision-v1",
        "workspaceId":f.engine.store.workspace_id,"appId":1000}));
    let mut revision = json!({"schemaVersion":1,"workspaceId":f.engine.store.workspace_id,
        "appId":1000,"revision":2,"atUnix":NOW});
    let mut mac = Hmac::<sha2::Sha256>::new_from_slice(KEY).unwrap();
    mac.update(b"yoke-code-intelligence/github-app-page-cache/v1\n");
    mac.update(
        &serde_json::to_vec(&json!({"kind":"github_app_page_revision_v1","data":revision}))
            .unwrap(),
    );
    revision["signature256"] = json!(format!(
        "sha256={}",
        hex::encode(mac.finalize().into_bytes())
    ));
    let mut raw = serde_json::to_vec(&revision).unwrap();
    assert!(raw.len() < 1024);
    raw.resize(4096, b' ');
    f.engine
        .store
        .db
        .execute(
            "INSERT INTO records(kind,id,value) VALUES(?1,?2,?3)",
            rusqlite::params![
                "github_app_page_revision_v1",
                key,
                std::str::from_utf8(&raw).unwrap()
            ],
        )
        .unwrap();
    o.max_store_bytes = original_bytes + 1024;
    let fresh = preview(&f, &done, &o);
    let before = rows(&f);
    fail_marker(
        pages::store_page(&context(&f, &o), &request(&fresh), &f.d),
        "github_app_pages_capacity_exceeded",
    );
    assert_eq!(rows(&f), before);
    o.max_store_bytes = original_bytes + 8192;
    let fresh = preview(&f, &done, &o);
    pages::store_page(&context(&f, &o), &request(&fresh), &f.d).unwrap();
    let stored: String = f
        .engine
        .store
        .db
        .query_row(
            "SELECT value FROM records WHERE kind=?1 AND id=?2",
            rusqlite::params!["github_app_page_revision_v1", key],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(stored.as_bytes(), raw);
}

#[test]
fn retention_recognizes_pages_and_keeps_only_the_original_completed_source_roots() {
    let mut f = Fixture::new();
    let done = completed(&f);
    let o = options(&f);
    let p = preview(&f, &done, &o);
    pages::store_page(&context(&f, &o), &request(&p), &f.d).unwrap();
    std::fs::write(f.root.path().join(PATH), "fn abandoned_page_context() {}\n").unwrap();
    let abandoned = f.engine.index(&f.d).unwrap().0.id;
    std::fs::write(f.root.path().join(PATH), "fn latest_page_context() {}\n").unwrap();
    f.engine.index(&f.d).unwrap();
    let before = rows(&f);
    let plan =
        yoke::retention::plan(&f.engine.store, &yoke::retention::Policy::default(), &f.d).unwrap();
    assert_eq!(plan["unknownDependencyKinds"], json!([]));
    let candidates = plan["snapshotIds"].as_array().unwrap();
    assert!(candidates.contains(&json!(abandoned)));
    assert!(!candidates.contains(&json!(f.snapshot)));
    let after = rows(&f);
    let plans: Vec<_> = after.iter().filter(|r| r.0 == "retention-plan").collect();
    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].1, plan["planId"].as_str().unwrap());
    assert_eq!(
        after
            .into_iter()
            .filter(|r| r.0 != "retention-plan")
            .collect::<Vec<_>>(),
        before
    );
}

fn rewrite_page_fixture(f: &Fixture, page_id: &str, change: impl FnOnce(&mut Value)) {
    let raw: String = f
        .engine
        .store
        .db
        .query_row(
            "SELECT value FROM records WHERE kind=?1 AND id=?2",
            rusqlite::params![pages::RECORD_KIND, page_id],
            |r| r.get(0),
        )
        .unwrap();
    let mut row: Value = serde_json::from_str(&raw).unwrap();
    let p = &mut row["proposal"];
    change(p);
    let original = p["html"].as_str().unwrap();
    let (prefix, tail) = original
        .split_once("<script type=\"application/json\" id=\"data\">")
        .unwrap();
    let (packet, suffix) = tail.split_once("</script>").unwrap();
    let mut display: Value = serde_json::from_str(packet).unwrap();
    for (key, value) in display.as_object_mut().unwrap() {
        *value = p[key].clone();
    }
    let packet = serde_json::to_string(&display)
        .unwrap()
        .replace('&', "\\u0026")
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029");
    let html =
        format!("{prefix}<script type=\"application/json\" id=\"data\">{packet}</script>{suffix}");
    p["htmlHash"] = json!(yoke::model::hash(html.as_bytes()));
    p["html"] = json!(html);
    let mut contract = p.clone();
    for key in [
        "proposalId",
        "persisted",
        "published",
        "pageWriteAuthority",
        "bearerReadCapability",
        "publicationAuthority",
        "sourceWritten",
        "currentHeadVerified",
        "networkRequests",
        "modelCalls",
    ] {
        contract.as_object_mut().unwrap().remove(key);
    }
    p["proposalId"] = json!(yoke::model::identity(&contract));
    let mut unsigned = row.clone();
    unsigned.as_object_mut().unwrap().remove("signature256");
    let mut mac = Hmac::<sha2::Sha256>::new_from_slice(KEY).unwrap();
    mac.update(b"yoke-code-intelligence/github-app-page-cache/v1\n");
    mac.update(&serde_json::to_vec(&json!({"kind":pages::RECORD_KIND,"data":unsigned})).unwrap());
    row["signature256"] = json!(format!(
        "sha256={}",
        hex::encode(mac.finalize().into_bytes())
    ));
    f.engine
        .store
        .db
        .execute(
            "UPDATE records SET value=?3 WHERE kind=?1 AND id=?2",
            rusqlite::params![pages::RECORD_KIND, page_id, row.to_string()],
        )
        .unwrap();
}

#[test]
fn retention_rejects_resealed_orphan_completion_and_safe_html_with_a_forged_graph_label() {
    for case in ["completion", "graph"] {
        let f = Fixture::new();
        let done = completed(&f);
        let o = options(&f);
        let p = preview(&f, &done, &o);
        pages::store_page(&context(&f, &o), &request(&p), &f.d).unwrap();
        rewrite_page_fixture(&f, id(&p), |p| {
            if case == "completion" {
                p["completionId"] = json!("f".repeat(64));
            } else {
                p["graph"]["head"]["nodes"][0]["name"] = json!("e".repeat(64));
            }
        });
        let before = rows(&f);
        assert!(
            yoke::retention::plan(&f.engine.store, &yoke::retention::Policy::default(), &f.d)
                .is_err(),
            "resealed {case} must match the typed original completed report, not just safe HTML or its self-reported graph hash"
        );
        assert_eq!(rows(&f), before);
    }
}
