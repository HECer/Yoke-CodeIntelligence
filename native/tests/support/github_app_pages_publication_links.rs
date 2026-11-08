//! Exact native page approval must survive the existing owned publication journal.
use super::*;

fn completed_page() -> (Fixture, Value, Value, Value) {
    let f = Fixture::new();
    let done = completed(&f);
    let o = options(&f);
    let p = preview(&f, &done, &o);
    let stored = pages::store_page(&context(&f, &o), &request(&p), &f.d).unwrap();
    (f, done, p, stored)
}
fn reference(p: &Value) -> pages::LinkReference {
    pages::LinkReference {
        page_id: id(p).into(),
        expected_page_version: 1,
    }
}
fn publication_options(f: &Fixture) -> publication::Options {
    let mut o = super::super::options(f);
    o.allow_page_links = true;
    o
}
fn pctx<'a>(f: &'a Fixture, o: &'a publication::Options) -> publication::Context<'a> {
    publication::Context {
        store: &f.engine.store,
        host: &f.host,
        options: o,
        clock: &f.clock,
    }
}
fn preview_request(done: &Value, p: &Value) -> publication::PreviewRequest {
    publication::PreviewRequest {
        schema_version: 1,
        completion_id: done["completionId"].as_str().unwrap().into(),
        expected_queue_revision: 2,
        page: Some(reference(p)),
    }
}
fn linked_preview(
    f: &Fixture,
    done: &Value,
    p: &Value,
    o: &publication::Options,
) -> anyhow::Result<Value> {
    publication::preview_with_context(&pctx(f, o), &preview_request(done, p), &f.d)
}
fn publish_request(done: &Value, preview: &Value, p: &Value) -> publication::PublishRequest {
    let mut r = super::super::publish_request(done, preview);
    r.page = Some(reference(p));
    r
}
fn publish_native(
    f: &Fixture,
    r: &publication::PublishRequest,
    o: &publication::Options,
    provider: &Provider,
) -> anyhow::Result<Value> {
    let mut client = fixture_client(
        provider.endpoint,
        f.time.clone(),
        &f.d,
        json!({"pull_requests":"write"}),
    );
    publication::publish(&pctx(f, o), &mut client, r, &remote_publish(), &f.d)
}
fn remove(f: &Fixture, p: &Value, stored: &Value) {
    pages::remove_page(
        &context(f, &options(f)),
        &pages::RemoveRequest {
            schema_version: 1,
            page_id: id(p).into(),
            expected_page_version: 1,
            expected_revision: stored["pageRevision"].as_u64().unwrap(),
            approval: "remove_exact_page".into(),
        },
        &f.d,
    )
    .unwrap();
}
fn posts(provider: &Provider) -> usize {
    provider
        .seen
        .lock()
        .unwrap()
        .iter()
        .filter(|s| s.method == "POST" && s.path == "/repos/acme/demo/issues/7/comments")
        .count()
}

#[test]
fn linked_preview_uses_the_exact_native_page_and_preserves_plain_v1_contract() {
    let (f, done, p, _) = completed_page();
    let o = publication_options(&f);
    let before = rows(&f);
    let plain = publication::preview(
        &f.engine.store,
        &f.host,
        &super::super::preview_request(&done),
        &o,
        &f.d,
    )
    .unwrap();
    let linked = linked_preview(&f, &done, &p, &o).unwrap();
    assert_eq!(plain["rendererVersion"], "yoke-pr-comment-v1");
    assert!(plain.get("pageLink").is_none());
    assert_eq!(linked["rendererVersion"], "yoke-pr-comment-v2");
    let native = serde_json::to_value(
        pages::approved_link(&context(&f, &options(&f)), &reference(&p), &f.d).unwrap(),
    )
    .unwrap();
    assert_eq!(linked["pageLink"], native);
    assert!(linked["body"].as_str().unwrap().contains(&format!(
        "<a href=\"{}\">View captured structural graph</a>",
        p["url"].as_str().unwrap()
    )));
    assert_ne!(linked["proposalId"], plain["proposalId"]);
    assert_ne!(linked["bodyHash"], plain["bodyHash"]);
    assert_eq!(
        linked["bodyHash"],
        yoke::model::hash(linked["body"].as_str().unwrap().as_bytes())
    );
    assert_eq!(linked["publicationAuthority"], false);
    assert_eq!(linked["complete"], false);
    assert_eq!(linked["currentHeadVerified"], false);
    assert_eq!(linked["networkRequests"], 0);
    assert_eq!(rows(&f), before);
    assert_eq!(
        std::fs::read(f.root.path().join(PATH)).unwrap(),
        SOURCE.as_bytes()
    );
}

#[test]
fn linked_preview_requires_distinct_consent_exact_version_and_an_unexpired_page() {
    let (f, done, p, _) = completed_page();
    let mut o = publication_options(&f);
    let before = rows(&f);
    assert!(linked_preview(&f, &done, &p, &o).is_ok());
    o.allow_page_links = false;
    fail_marker(
        linked_preview(&f, &done, &p, &o),
        "github_app_page_links_permission_required",
    );
    o.allow_page_links = true;
    let mut r = preview_request(&done, &p);
    r.page.as_mut().unwrap().expected_page_version = 2;
    fail_marker(
        publication::preview_with_context(&pctx(&f, &o), &r, &f.d),
        "github_app_pages_link_stale",
    );
    f.time.store(NOW + 60, Ordering::SeqCst);
    fail_marker(
        linked_preview(&f, &done, &p, &o),
        "github_app_pages_link_stale",
    );
    assert_eq!(rows(&f), before);
}

#[test]
fn linked_preview_rejects_a_genuine_stored_page_from_another_completed_pr() {
    let (mut f, _, p, _) = completed_page();
    let other = complete_another_pr(&mut f, 8);
    let o = publication_options(&f);
    let before = rows(&f);
    fail_marker(
        linked_preview(&f, &other, &p, &o),
        "github_app_page_link_source_mismatch",
    );
    assert_eq!(rows(&f), before);
}

#[test]
fn approved_linked_post_is_exact_and_history_survives_page_removal_and_key_rotation() {
    let (mut f, done, p, stored) = completed_page();
    let mut o = publication_options(&f);
    o.allow_publication = true;
    let preview = linked_preview(&f, &done, &p, &o).unwrap();
    let r = publish_request(&done, &preview, &p);
    let provider = Provider::new(Mode::Publishing);
    let published = publish_native(&f, &r, &o, &provider).unwrap();
    assert_eq!(published["status"], "published");
    assert_eq!(posts(&provider), 1);
    let seen = provider.seen.lock().unwrap();
    let post = seen
        .iter()
        .find(|s| s.method == "POST" && s.path == "/repos/acme/demo/issues/7/comments")
        .unwrap();
    assert!(post.installation_token);
    assert_eq!(post.comment_body.as_deref(), preview["body"].as_str());
    drop(seen);
    let before_retry = rows(&f);
    let requests = provider.seen.lock().unwrap().len();
    assert_eq!(
        publish_native(&f, &r, &o, &provider).unwrap()["historical"],
        true
    );
    assert_eq!(provider.seen.lock().unwrap().len(), requests);
    assert_eq!(rows(&f), before_retry);
    remove(&f, &p, &stored);
    f.time.store(NOW + 60, Ordering::SeqCst);
    f.host = app::Host::with_signing_keys(
        app::HostProfile::parse(
            &serde_json::to_vec(&json!({"schemaVersion":1,"appId":123,
        "secretCredential":"new-not-read","priorSecretCredentials":["old-not-read"],
        "grants":[{"installationId":42,"repositoryId":9,"repository":"acme/demo"}]}))
            .unwrap(),
        )
        .unwrap(),
        vec![
            app::SigningKey::from_host_bytes(b"public replacement linked-publication fixture key")
                .unwrap(),
            app::SigningKey::from_host_bytes(KEY).unwrap(),
        ],
    )
    .unwrap();
    let after_removal = rows(&f);
    let historical = publish_native(&f, &r, &o, &provider).unwrap();
    assert_eq!(historical["historical"], true);
    assert_eq!(historical["networkRequests"], 0);
    assert_eq!(provider.seen.lock().unwrap().len(), requests);
    o.allow_page_links = false;
    let status = publication::status(
        &pctx(&f, &o),
        &publication::StatusRequest {
            schema_version: 1,
            request_id: r.request_id,
        },
        &f.d,
    )
    .unwrap();
    assert_eq!(status["status"], "published");
    assert_eq!(status["bodyHash"], preview["bodyHash"]);
    assert_eq!(rows(&f), after_removal);
    let plan =
        yoke::retention::plan(&f.engine.store, &yoke::retention::Policy::default(), &f.d).unwrap();
    assert_eq!(plan["unknownDependencyKinds"], json!([]));
    assert!(
        !plan["snapshotIds"]
            .as_array()
            .unwrap()
            .contains(&json!(f.snapshot))
    );
    assert_eq!(
        std::fs::read(f.root.path().join(PATH)).unwrap(),
        SOURCE.as_bytes()
    );
}

#[test]
fn page_expiry_during_owned_preflight_prevents_any_comment_send_or_journal_intent() {
    let (f, done, p, _) = completed_page();
    let mut o = publication_options(&f);
    o.allow_publication = true;
    let preview = linked_preview(&f, &done, &p, &o).unwrap();
    let r = publish_request(&done, &preview, &p);
    let time = f.time.clone();
    let observed = Arc::new(AtomicBool::new(false));
    let seen = observed.clone();
    let provider = Provider::with_hook(
        Mode::PreflightMutation,
        Some(Box::new(move || {
            time.store(NOW + 60, Ordering::SeqCst);
            seen.store(true, Ordering::SeqCst);
        })),
    );
    let before = rows(&f);
    fail_marker(
        publish_native(&f, &r, &o, &provider),
        "github_app_pages_link_stale",
    );
    assert!(
        observed.load(Ordering::SeqCst),
        "actual remote preflight must expire the page"
    );
    assert_eq!(posts(&provider), 0);
    assert_eq!(rows(&f), before);
}

#[test]
fn successful_post_reply_closes_from_captured_link_when_the_page_disappears_during_send() {
    for expire in [true, false] {
        let (f, done, p, stored) = completed_page();
        let mut o = publication_options(&f);
        o.allow_publication = true;
        let preview = linked_preview(&f, &done, &p, &o).unwrap();
        let r = publish_request(&done, &preview, &p);
        let observed = Arc::new(AtomicBool::new(false));
        let seen = observed.clone();
        let time = f.time.clone();
        let root = f.root.path().to_owned();
        let page_options = options(&f);
        let page_id = id(&p).to_owned();
        let revision = stored["pageRevision"].as_u64().unwrap();
        let provider = Provider::with_hook(
            Mode::Publishing,
            Some(Box::new(move || {
                let engine = Engine::open(&root).unwrap();
                let count: u64 = engine
                    .store
                    .db
                    .query_row(
                        "SELECT count(*) FROM records WHERE kind=?1",
                        [publication::RECORD_KIND],
                        |r| r.get(0),
                    )
                    .unwrap();
                assert_eq!(
                    count, 1,
                    "the native send intent must commit before the provider observes POST"
                );
                if expire {
                    time.store(NOW + 60, Ordering::SeqCst);
                } else {
                    let host = host();
                    let clock = worker::Clock::for_fixture(time);
                    pages::remove_page(
                        &pages::Context {
                            store: &engine.store,
                            host: &host,
                            options: &page_options,
                            clock: &clock,
                        },
                        &pages::RemoveRequest {
                            schema_version: 1,
                            page_id,
                            expected_page_version: 1,
                            expected_revision: revision,
                            approval: "remove_exact_page".into(),
                        },
                        &Deadline::new(5000),
                    )
                    .unwrap();
                }
                seen.store(true, Ordering::SeqCst);
            })),
        );
        let result = publish_native(&f, &r, &o, &provider).unwrap();
        assert!(observed.load(Ordering::SeqCst));
        assert_eq!(posts(&provider), 1);
        assert_eq!(result["status"], "published");
        assert_eq!(result["writeConfirmed"], true);
        let before = rows(&f);
        let requests = provider.seen.lock().unwrap().len();
        assert_eq!(
            publish_native(&f, &r, &o, &provider).unwrap()["historical"],
            true
        );
        assert_eq!(provider.seen.lock().unwrap().len(), requests);
        assert_eq!(rows(&f), before);
    }
}

#[test]
fn linked_and_plain_contracts_for_one_completion_have_independent_historical_render_bindings() {
    let (f, done, p, _) = completed_page();
    let mut o = publication_options(&f);
    o.allow_publication = true;
    let linked = linked_preview(&f, &done, &p, &o).unwrap();
    let r = publish_request(&done, &linked, &p);
    assert_eq!(
        publish_native(&f, &r, &o, &Provider::new(Mode::Publishing)).unwrap()["status"],
        "published"
    );
    let plain = publication::preview_with_context(
        &pctx(&f, &o),
        &super::super::preview_request(&done),
        &f.d,
    )
    .unwrap();
    let mut plain_request = super::super::publish_request(&done, &plain);
    plain_request.request_id = "89abcdef-0123-4abc-8000-000000000008".into();
    let provider = Provider::configured(
        Mode::Publishing,
        None,
        42,
        Some(linked["body"].as_str().unwrap().into()),
        None,
    );
    assert_eq!(
        publish_native(&f, &plain_request, &o, &provider).unwrap()["status"],
        "published"
    );
    assert_eq!(posts(&provider), 0);
    let seen = provider.seen.lock().unwrap();
    let patches: Vec<_> = seen.iter().filter(|s| s.method == "PATCH").collect();
    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].comment_body.as_deref(), plain["body"].as_str());
    drop(seen);
    for request in [r, plain_request] {
        let status = publication::status(
            &pctx(&f, &o),
            &publication::StatusRequest {
                schema_version: 1,
                request_id: request.request_id,
            },
            &f.d,
        )
        .unwrap();
        assert_eq!(status["status"], "published");
        assert_eq!(status["bodyHash"], request.body_hash);
    }
}

#[test]
fn linked_requests_are_closed_and_absent_links_preserve_exact_v1_request_bytes() {
    let id = "a".repeat(64);
    let plain = publication::PreviewRequest {
        schema_version: 1,
        completion_id: id.clone(),
        expected_queue_revision: 2,
        page: None,
    };
    assert_eq!(
        String::from_utf8(serde_json::to_vec(&plain).unwrap()).unwrap(),
        format!("{{\"schemaVersion\":1,\"completionId\":\"{id}\",\"expectedQueueRevision\":2}}")
    );
    let valid = json!({"schemaVersion":1,"completionId":id,"expectedQueueRevision":2,"page":{"pageId":"b".repeat(64),"expectedPageVersion":1}});
    assert!(publication::PreviewRequest::parse(&serde_json::to_vec(&valid).unwrap()).is_ok());
    for (key, value) in [
        ("url", json!("https://attacker.invalid")),
        ("html", json!("<script>caller</script>")),
        ("token", json!("c".repeat(64))),
    ] {
        let mut bad = valid.clone();
        bad["page"][key] = value;
        assert!(publication::PreviewRequest::parse(&serde_json::to_vec(&bad).unwrap()).is_err());
    }
    for version in [0, u64::MAX] {
        let mut bad = valid.clone();
        bad["page"]["expectedPageVersion"] = json!(version);
        assert!(publication::PreviewRequest::parse(&serde_json::to_vec(&bad).unwrap()).is_err());
    }
    let duplicate = format!(
        "{{\"schemaVersion\":1,\"completionId\":\"{id}\",\"expectedQueueRevision\":2,\"page\":{{\"pageId\":\"{}\",\"expectedPageVersion\":1,\"expectedPageVersion\":1}}}}",
        "b".repeat(64)
    );
    assert!(publication::PreviewRequest::parse(duplicate.as_bytes()).is_err());
}

#[test]
fn authenticated_linked_intent_must_be_inside_the_captured_page_lifetime() {
    let mut refused = Vec::new();
    for invalid_at in [NOW + 4, NOW + 65] {
        let f = Fixture::new();
        let done = completed(&f);
        let po = options(&f);
        let p = preview(&f, &done, &po);
        f.time.store(NOW + 5, Ordering::SeqCst);
        pages::store_page(&context(&f, &po), &request(&p), &f.d).unwrap();
        let mut o = publication_options(&f);
        o.allow_publication = true;
        let preview = linked_preview(&f, &done, &p, &o).unwrap();
        let r = publish_request(&done, &preview, &p);
        let provider = Provider::new(Mode::LostReply);
        assert!(publish_native(&f, &r, &o, &provider).is_err());
        assert_eq!(posts(&provider), 1);
        let status_request = publication::StatusRequest {
            schema_version: 1,
            request_id: r.request_id,
        };
        f.time.store(NOW + 65, Ordering::SeqCst);
        assert_eq!(
            publication::status(&pctx(&f, &o), &status_request, &f.d).unwrap()["status"],
            "unknown"
        );
        let (old_id, raw): (String, String) = f
            .engine
            .store
            .db
            .query_row(
                "SELECT id,value FROM records WHERE kind=?1",
                [publication::RECORD_KIND],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        let mut value: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(value["event"]["data"]["pageLink"]["storedAtUnix"], NOW + 5);
        assert_eq!(
            value["event"]["data"]["pageLink"]["expiresAtUnix"],
            NOW + 65
        );
        value["event"]["atUnix"] = json!(invalid_at);
        let new_id = reseal_publication(&mut value);
        f.engine
            .store
            .db
            .execute(
                "UPDATE records SET id=?1,value=?2 WHERE kind=?3 AND id=?4",
                rusqlite::params![
                    new_id,
                    serde_json::to_string(&value).unwrap(),
                    publication::RECORD_KIND,
                    old_id
                ],
            )
            .unwrap();
        let before = rows(&f);
        let result = publication::status(&pctx(&f, &o), &status_request, &f.d);
        refused.push(result.is_err());
        if result.is_err() {
            fail_marker(result, "github_app_publication_binding_invalid");
        }
        assert_eq!(rows(&f), before);
    }
    assert_eq!(
        refused,
        vec![true, true],
        "both captured lifetime bounds must be enforced"
    );
}

#[test]
fn lost_linked_post_is_recovered_without_a_live_page_or_page_link_authority() {
    let (f, done, p, stored) = completed_page();
    let mut o = publication_options(&f);
    o.allow_publication = true;
    let preview = linked_preview(&f, &done, &p, &o).unwrap();
    let r = publish_request(&done, &preview, &p);
    let lost = Provider::new(Mode::LostReply);
    assert!(publish_native(&f, &r, &o, &lost).is_err());
    assert_eq!(posts(&lost), 1);
    let status_request = publication::StatusRequest {
        schema_version: 1,
        request_id: r.request_id.clone(),
    };
    let unknown = publication::status(&pctx(&f, &o), &status_request, &f.d).unwrap();
    assert_eq!(unknown["status"], "unknown");
    remove(&f, &p, &stored);
    f.time.store(NOW + 60, Ordering::SeqCst);
    o.allow_page_links = false;
    o.allow_publication = false;
    let provider = Provider::configured(
        Mode::RecoveryRead,
        None,
        42,
        Some(preview["body"].as_str().unwrap().into()),
        Some(9),
    );
    let mut client = fixture_client(
        provider.endpoint,
        f.time.clone(),
        &f.d,
        json!({"pull_requests":"read"}),
    );
    let recovery = super::super::recovery_request(&unknown);
    let result = publication::recover(
        &pctx(&f, &o),
        &mut client,
        &recovery,
        &remote_publish(),
        &f.d,
    )
    .unwrap();
    assert_eq!(result["status"], "published");
    assert_eq!(result["reconciled"], true);
    assert_eq!(result["commentId"], 501);
    assert_eq!(result["bodyHash"], preview["bodyHash"]);
    assert_eq!(posts(&provider), 0);
    assert!(
        !provider
            .seen
            .lock()
            .unwrap()
            .iter()
            .any(|s| s.method == "PATCH")
    );
    let before = rows(&f);
    let requests = provider.seen.lock().unwrap().len();
    let retry = publication::recover(
        &pctx(&f, &o),
        &mut client,
        &recovery,
        &remote_publish(),
        &f.d,
    )
    .unwrap();
    assert_eq!(retry["historical"], true);
    assert_eq!(retry["networkRequests"], 0);
    assert_eq!(provider.seen.lock().unwrap().len(), requests);
    assert_eq!(rows(&f), before);
}

#[test]
fn unrelated_pr_arrival_during_post_keeps_the_captured_link_and_closes_the_selected_job() {
    let (f, done, p, _) = completed_page();
    let mut o = publication_options(&f);
    o.allow_publication = true;
    let preview = linked_preview(&f, &done, &p, &o).unwrap();
    let r = publish_request(&done, &preview, &p);
    let observed = Arc::new(AtomicBool::new(false));
    let seen = observed.clone();
    let root = f.root.path().to_owned();
    let queue_options = f.options.queue.clone();
    let provider = Provider::with_hook(
        Mode::Publishing,
        Some(Box::new(move || {
            let engine = Engine::open(&root).unwrap();
            let mut event: Value = serde_json::from_slice(&raw_event()).unwrap();
            event["number"] = json!(8);
            event["pull_request"]["number"] = json!(8);
            let raw = serde_json::to_vec(&event).unwrap();
            let added = app::ingest(
                &engine.store,
                &host(),
                &delivery_headers(&raw, 2),
                &raw,
                &queue_options,
                &Deadline::new(5000),
            )
            .unwrap();
            assert_eq!(added["reused"], false);
            seen.store(true, Ordering::SeqCst);
        })),
    );
    let result = publish_native(&f, &r, &o, &provider);
    assert!(observed.load(Ordering::SeqCst));
    assert_eq!(posts(&provider), 1);
    // Plain rendering independently proves the original job is still selected at revision2.
    let plain = publication::preview_with_context(
        &pctx(&f, &o),
        &super::super::preview_request(&done),
        &f.d,
    )
    .unwrap();
    assert_eq!(plain["queueRevision"], 2);
    assert_eq!(plain["completionId"], done["completionId"]);
    assert_eq!(result.unwrap()["status"], "published");
    let before = rows(&f);
    let requests = provider.seen.lock().unwrap().len();
    assert_eq!(
        publish_native(&f, &r, &o, &provider).unwrap()["historical"],
        true
    );
    assert_eq!(provider.seen.lock().unwrap().len(), requests);
    assert_eq!(rows(&f), before);
}

#[test]
fn unrelated_pr_arrival_before_preview_keeps_a_genuine_page_for_the_same_selected_completion() {
    let (f, done, p, stored) = completed_page();
    let mut o = publication_options(&f);
    o.allow_publication = true;
    let approved_before = linked_preview(&f, &done, &p, &o).unwrap();
    let mut event: Value = serde_json::from_slice(&raw_event()).unwrap();
    event["number"] = json!(8);
    event["pull_request"]["number"] = json!(8);
    let raw = serde_json::to_vec(&event).unwrap();
    assert_eq!(
        app::ingest(
            &f.engine.store,
            &f.host,
            &delivery_headers(&raw, 2),
            &raw,
            &f.options.queue,
            &f.d
        )
        .unwrap()["reused"],
        false
    );
    let before = rows(&f);
    let native = serde_json::to_value(
        pages::approved_link(&context(&f, &options(&f)), &reference(&p), &f.d).unwrap(),
    )
    .unwrap();
    let plain = publication::preview_with_context(
        &pctx(&f, &o),
        &super::super::preview_request(&done),
        &f.d,
    )
    .unwrap();
    assert_eq!(plain["queueRevision"], 2);
    assert_eq!(plain["completionId"], done["completionId"]);
    let linked = linked_preview(&f, &done, &p, &o).unwrap();
    assert_eq!(linked["pageLink"], native);
    assert_eq!(linked["proposalId"], approved_before["proposalId"]);
    assert_eq!(linked["body"], approved_before["body"]);
    assert_eq!(rows(&f), before);
    let r = publish_request(&done, &linked, &p);
    let provider = Provider::new(Mode::Publishing);
    assert_eq!(
        publish_native(&f, &r, &o, &provider).unwrap()["status"],
        "published"
    );
    assert_eq!(posts(&provider), 1);
    let (intent_checkpoint, page_checkpoint): (String, String) = f.engine.store.db.query_row(
        "SELECT json_extract(value,'$.event.data.queueCheckpoint'),json_extract(value,'$.event.data.pageLink.sourceQueueCheckpoint') FROM records WHERE kind=?1 AND json_extract(value,'$.event.type')='intent'",
        [publication::RECORD_KIND], |row|Ok((row.get(0)?,row.get(1)?))).unwrap();
    assert_ne!(intent_checkpoint, page_checkpoint);
    assert_eq!(page_checkpoint, p["queueCheckpoint"]);
    remove(&f, &p, &stored);
    f.time.store(NOW + 60, Ordering::SeqCst);
    o.allow_page_links = false;
    let after = rows(&f);
    let status = publication::status(
        &pctx(&f, &o),
        &publication::StatusRequest {
            schema_version: 1,
            request_id: r.request_id,
        },
        &f.d,
    )
    .unwrap();
    assert_eq!(status["status"], "published");
    assert_eq!(status["bodyHash"], linked["bodyHash"]);
    assert_eq!(rows(&f), after);
    let plan =
        yoke::retention::plan(&f.engine.store, &yoke::retention::Policy::default(), &f.d).unwrap();
    assert_eq!(plan["unknownDependencyKinds"], json!([]));
    assert!(
        !plan["snapshotIds"]
            .as_array()
            .unwrap()
            .contains(&json!(f.snapshot))
    );
}

#[test]
fn authenticated_link_cannot_claim_a_genuine_checkpoint_before_its_job_was_selected() {
    let (f, done, p, _) = completed_page();
    let mut o = publication_options(&f);
    o.allow_publication = true;
    let mut preview = linked_preview(&f, &done, &p, &o).unwrap();
    let r = publish_request(&done, &preview, &p);
    assert!(publish_native(&f, &r, &o, &Provider::new(Mode::LostReply)).is_err());
    let status_request = publication::StatusRequest {
        schema_version: 1,
        request_id: r.request_id,
    };
    assert_eq!(
        publication::status(&pctx(&f, &o), &status_request, &f.d).unwrap()["status"],
        "unknown"
    );
    let checkpoint: String = f
        .engine
        .store
        .db
        .query_row(
            "SELECT id FROM records WHERE kind=?1 AND json_extract(value,'$.sequence')=1",
            [app::RECORD_KIND],
            |row| row.get(0),
        )
        .unwrap();
    assert_ne!(checkpoint, p["queueCheckpoint"]);
    let (old_id, raw): (String, String) = f
        .engine
        .store
        .db
        .query_row(
            "SELECT id,value FROM records WHERE kind=?1",
            [publication::RECORD_KIND],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    let mut record: Value = serde_json::from_str(&raw).unwrap();
    let mut link = record["event"]["data"]["pageLink"].clone();
    link["sourceQueueCheckpoint"] = json!(checkpoint);
    let source = json!({"domain":"yoke-github-page-link-source-v1",
        "workspaceId":f.engine.store.workspace_id,"appId":123,"repositoryId":9,"repository":"acme/demo","number":7,
        "jobId":done["jobId"],"completionId":done["completionId"],"queueRevision":2,"queueCheckpoint":checkpoint,
        "headSha":"a".repeat(40),"baseSha":"b".repeat(40),"headSnapshotId":f.snapshot,"baseSnapshotId":null,
        "reviewHash":p["reviewHash"],"graphHash":link["graphHash"],"completedAtUnix":NOW});
    link["sourceBindingHash"] = json!(yoke::model::identity(&source));
    preview["pageLink"] = link.clone();
    for key in [
        "proposalId",
        "published",
        "publicationAuthority",
        "currentHeadVerified",
        "installationTokenVerified",
        "sourceWritten",
        "complete",
        "networkRequests",
        "modelCalls",
    ] {
        preview.as_object_mut().unwrap().remove(key);
    }
    let proposal = yoke::model::identity(&preview);
    record["event"]["data"]["pageLink"] = link;
    record["event"]["data"]["proposalId"] = json!(proposal);
    record["event"]["request"]["proposalId"] = json!(proposal);
    let new_id = reseal_publication(&mut record);
    f.engine
        .store
        .db
        .execute(
            "UPDATE records SET id=?1,value=?2 WHERE kind=?3 AND id=?4",
            rusqlite::params![
                new_id,
                serde_json::to_string(&record).unwrap(),
                publication::RECORD_KIND,
                old_id
            ],
        )
        .unwrap();
    let before = rows(&f);
    fail_marker(
        publication::status(&pctx(&f, &o), &status_request, &f.d),
        "github_app_binding_invalid",
    );
    fail_marker(
        yoke::retention::plan(&f.engine.store, &yoke::retention::Policy::default(), &f.d),
        "github_app_binding_invalid",
    );
    assert_eq!(rows(&f), before);
}
