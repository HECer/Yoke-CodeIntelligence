//! Authenticate an actual native page before exposing a link approval binding.
use super::*;

fn reference(p: &Value) -> pages::LinkReference {
    pages::LinkReference {
        page_id: id(p).into(),
        expected_page_version: 1,
    }
}
fn linked(f: &Fixture, p: &Value, o: &pages::Options) -> anyhow::Result<Value> {
    Ok(serde_json::to_value(pages::approved_link(
        &context(f, o),
        &reference(p),
        &f.d,
    )?)?)
}
fn cas_hashes(f: &Fixture) -> std::collections::BTreeMap<String, String> {
    f.blobs()
        .into_iter()
        .map(|name| {
            let hash = yoke::model::hash(
                std::fs::read(f.engine.store.dir.join("blobs").join(&name)).unwrap(),
            );
            (name, hash)
        })
        .collect()
}
#[test]
fn page_link_factory_binds_the_actual_stored_page_and_original_source_without_writes() {
    let f = Fixture::new();
    let done = completed(&f);
    let mut o = options(&f);
    let p = preview(&f, &done, &o);
    pages::store_page(&context(&f, &o), &request(&p), &f.d).unwrap();
    o.worker.queue.allow_writes = false;
    let before = rows(&f);
    let blobs = cas_hashes(&f);
    let link = linked(&f, &p, &o).unwrap();
    assert_eq!(link["schemaVersion"], 1);
    assert_eq!(link["pageId"], p["pageId"]);
    assert_eq!(link["pageVersion"], 1);
    assert_eq!(link["pageProposalId"], p["proposalId"]);
    assert_eq!(link["htmlHash"], p["htmlHash"]);
    assert_eq!(link["graphHash"], yoke::model::identity(&p["graph"]));
    assert_eq!(link["publicBaseUrl"], "https://reports.example.invalid");
    assert_eq!(link["url"], p["url"]);
    assert_eq!(link["storedAtUnix"], NOW);
    assert_eq!(link["expiresAtUnix"], NOW + 60);
    let source = json!({"domain":"yoke-github-page-link-source-v1",
        "workspaceId":f.engine.store.workspace_id,"appId":123,"repositoryId":9,"repository":"acme/demo","number":7,
        "jobId":done["jobId"],"completionId":done["completionId"],"queueRevision":2,"queueCheckpoint":p["queueCheckpoint"],
        "headSha":"a".repeat(40),"baseSha":"b".repeat(40),"headSnapshotId":f.snapshot,"baseSnapshotId":null,
        "reviewHash":p["reviewHash"],"graphHash":yoke::model::identity(&p["graph"]),"completedAtUnix":NOW});
    assert_eq!(link["sourceBindingHash"], yoke::model::identity(&source));
    assert_eq!(link["sourceQueueRevision"], 2);
    assert_eq!(link["sourceQueueCheckpoint"], p["queueCheckpoint"]);
    assert!(link.get("html").is_none() && link.get("graph").is_none());
    assert_eq!(rows(&f), before);
    assert_eq!(cas_hashes(&f), blobs);
    assert_eq!(
        std::fs::read(f.root.path().join(PATH)).unwrap(),
        SOURCE.as_bytes()
    );
}

#[test]
fn page_link_factory_refuses_missing_consent_wrong_version_and_expiry() {
    let f = Fixture::new();
    let done = completed(&f);
    let o = options(&f);
    let p = preview(&f, &done, &o);
    pages::store_page(&context(&f, &o), &request(&p), &f.d).unwrap();
    let before = rows(&f);
    let mut denied = o.clone();
    denied.allow_pages = false;
    fail_marker(
        linked(&f, &p, &denied),
        "github_app_pages_permission_required",
    );
    fail_marker(
        pages::approved_link(
            &context(&f, &o),
            &pages::LinkReference {
                page_id: id(&p).into(),
                expected_page_version: 2,
            },
            &f.d,
        ),
        "github_app_pages_link_stale",
    );
    fail_marker(
        pages::approved_link(
            &context(&f, &o),
            &pages::LinkReference {
                page_id: "f".repeat(64),
                expected_page_version: 1,
            },
            &f.d,
        ),
        "github_app_pages_link_stale",
    );
    fail_marker(
        pages::approved_link(
            &context(&f, &o),
            &pages::LinkReference {
                page_id: "../outside".into(),
                expected_page_version: 1,
            },
            &f.d,
        ),
        "github_app_pages_input_invalid",
    );
    assert_eq!(rows(&f), before);
    f.time.store(NOW + 60, Ordering::SeqCst);
    fail_marker(linked(&f, &p, &o), "github_app_pages_link_stale");
    assert_eq!(rows(&f), before);
}

#[test]
fn page_link_factory_refuses_caller_transaction_and_obeys_output_budget_without_mutation() {
    let f = Fixture::new();
    let done = completed(&f);
    let mut o = options(&f);
    let p = preview(&f, &done, &o);
    pages::store_page(&context(&f, &o), &request(&p), &f.d).unwrap();
    let before = rows(&f);
    f.engine.store.db.execute_batch("CREATE TEMP TABLE owned_link_transaction(v INTEGER); BEGIN; INSERT INTO owned_link_transaction VALUES(7)").unwrap();
    fail_marker(
        linked(&f, &p, &o),
        "github_app_pages_caller_transaction_refused",
    );
    assert!(!f.engine.store.db.is_autocommit());
    assert_eq!(
        f.engine
            .store
            .db
            .query_row("SELECT sum(v) FROM owned_link_transaction", [], |r| r
                .get::<_, u64>(0))
            .unwrap(),
        7
    );
    f.engine.store.db.execute_batch("ROLLBACK").unwrap();
    o.max_output_bytes = 1;
    fail_marker(linked(&f, &p, &o), "github_app_pages_capacity_exceeded");
    assert_eq!(rows(&f), before);
}

#[test]
fn page_link_factory_derives_current_key_url_without_changing_historical_page() {
    const NEXT_KEY: &[u8] = b"public owned next page-link fixture key";
    let mut f = Fixture::new();
    let done = completed(&f);
    let o = options(&f);
    let p = preview(&f, &done, &o);
    pages::store_page(&context(&f, &o), &request(&p), &f.d).unwrap();
    let before = rows(&f);
    f.host = app::Host::with_signing_keys(
        app::HostProfile::parse(
            &serde_json::to_vec(&json!({"schemaVersion":1,"appId":123,
        "secretCredential":"current-not-read","priorSecretCredentials":["old-not-read"],
        "grants":[{"installationId":42,"repositoryId":9,"repository":"acme/demo"}]}))
            .unwrap(),
        )
        .unwrap(),
        vec![
            app::SigningKey::from_host_bytes(NEXT_KEY).unwrap(),
            app::SigningKey::from_host_bytes(KEY).unwrap(),
        ],
    )
    .unwrap();
    let link = linked(&f, &p, &o).unwrap();
    assert_ne!(link["url"], p["url"]);
    let mut mac = Hmac::<sha2::Sha256>::new_from_slice(NEXT_KEY).unwrap();
    mac.update(b"yoke-code-intelligence/github-app-page-access/v1\n");
    mac.update(
        &serde_json::to_vec(
            &json!({"workspaceId":f.engine.store.workspace_id,"appId":123,"pageId":p["pageId"]}),
        )
        .unwrap(),
    );
    let expected_token = hex::encode(mac.finalize().into_bytes());
    assert_eq!(
        link["url"],
        format!(
            "https://reports.example.invalid/p/{}?t={expected_token}",
            id(&p)
        )
    );
    assert_eq!(link["htmlHash"], p["htmlHash"]);
    assert_eq!(link["pageProposalId"], p["proposalId"]);
    assert!(
        pages::read_page(&context(&f, &o), id(&p), &expected_token, &f.d)
            .unwrap()
            .is_some()
    );
    assert!(
        pages::read_page(&context(&f, &o), id(&p), token(&p), &f.d)
            .unwrap()
            .is_none()
    );
    assert_eq!(rows(&f), before);
}

#[test]
fn page_link_factory_refuses_removal_of_an_unexpired_previously_readable_page() {
    let f = Fixture::new();
    let done = completed(&f);
    let o = options(&f);
    let p = preview(&f, &done, &o);
    let stored = pages::store_page(&context(&f, &o), &request(&p), &f.d).unwrap();
    assert_eq!(linked(&f, &p, &o).unwrap()["url"], p["url"]);
    assert_eq!(f.time.load(Ordering::SeqCst), NOW);
    pages::remove_page(
        &context(&f, &o),
        &pages::RemoveRequest {
            schema_version: 1,
            page_id: id(&p).into(),
            expected_page_version: 1,
            expected_revision: stored["pageRevision"].as_u64().unwrap(),
            approval: "remove_exact_page".into(),
        },
        &f.d,
    )
    .unwrap();
    let after = rows(&f);
    fail_marker(linked(&f, &p, &o), "github_app_pages_link_stale");
    assert_eq!(rows(&f), after);
    assert_eq!(f.time.load(Ordering::SeqCst), NOW);
}
#[test]
fn page_link_factory_rechecks_current_grant_and_original_cas_bytes() {
    let mut f = Fixture::new();
    let done = completed(&f);
    let o = options(&f);
    let p = preview(&f, &done, &o);
    pages::store_page(&context(&f, &o), &request(&p), &f.d).unwrap();
    assert!(linked(&f, &p, &o).is_ok());
    let before = rows(&f);
    f.host=app::Host::with_signing_key(app::HostProfile::parse(&serde_json::to_vec(&json!({"schemaVersion":1,"appId":123,
        "secretCredential":"not-read","grants":[{"installationId":42,"repositoryId":10,"repository":"acme/other"}]})).unwrap()).unwrap(),
        app::SigningKey::from_host_bytes(KEY).unwrap()).unwrap();
    fail_marker(linked(&f, &p, &o), "github_app_worker_grant_denied");
    f.host = host();
    assert!(linked(&f, &p, &o).is_ok());
    std::fs::remove_file(
        f.engine
            .store
            .dir
            .join("blobs")
            .join(yoke::model::hash(SOURCE.as_bytes())),
    )
    .unwrap();
    fail_marker(linked(&f, &p, &o), "github_app_worker_source_unavailable");
    assert_eq!(rows(&f), before);
    assert_eq!(
        std::fs::read(f.root.path().join(PATH)).unwrap(),
        SOURCE.as_bytes()
    );
}
#[test]
fn page_link_factory_authenticates_the_mac_even_when_all_page_fields_are_intact() {
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
    let mut row: Value = serde_json::from_str(&raw).unwrap();
    let mut signature = row["signature256"].as_str().unwrap().as_bytes().to_vec();
    let last = signature.last_mut().unwrap();
    *last = if *last == b'0' { b'1' } else { b'0' };
    row["signature256"] = json!(String::from_utf8(signature).unwrap());
    f.engine
        .store
        .db
        .execute(
            "UPDATE records SET value=?1 WHERE kind=?2 AND id=?3",
            rusqlite::params![row.to_string(), pages::RECORD_KIND, id(&p)],
        )
        .unwrap();
    let before = rows(&f);
    fail_marker(linked(&f, &p, &o), "github_app_pages_binding_invalid");
    assert_eq!(rows(&f), before);
}

#[test]
fn page_link_factory_rechecks_expiry_and_clock_regression_after_real_source_replay() {
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
        let result = linked(&f, &p, &o);
        f.engine
            .store
            .db
            .authorizer(None::<fn(AuthContext<'_>) -> Authorization>);
        assert!(
            observed.load(Ordering::SeqCst),
            "actual source replay must advance the fixture clock"
        );
        fail_marker(
            result,
            if next_time > NOW {
                "github_app_pages_link_stale"
            } else {
                "github_app_worker_clock_regressed"
            },
        );
        assert_eq!(rows(&f), before);
    }
}
