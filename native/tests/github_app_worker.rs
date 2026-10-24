//! Real own SQLite transitions, signed queue inputs and independently expected fencing.
use hmac::{Hmac, Mac};
use serde_json::{Value, json};
use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use yoke::{
    capture::Deadline,
    github_app::{self as app, worker},
    store::Store,
};
const KEY: &[u8] = b"public owned deterministic worker fixture key";
const NOW: u64 = 1_900_000_000;
fn host(repository_id: u64, repository: &str) -> app::Host {
    app::Host::with_signing_key(
        app::HostProfile::parse(
            &serde_json::to_vec(&json!({
        "schemaVersion":1,"appId":123,"secretCredential":"not-read-by-fixture",
        "grants":[{"installationId":42,"repositoryId":repository_id,"repository":repository}]}))
            .unwrap(),
        )
        .unwrap(),
        app::SigningKey::from_host_bytes(KEY).unwrap(),
    )
    .unwrap()
}
fn body(head: char, action: &str) -> Vec<u8> {
    let mut raw=serde_json::to_vec_pretty(&json!({"action":action,"number":7,"installation":{"id":42},
        "repository":{"id":9,"full_name":"acme/demo"},"pull_request":{"number":7,"state":if action=="closed" {"closed"} else {"open"},
        "draft":false,"title":"Owned Grüße 🌍","head":{"sha":head.to_string().repeat(40)},
        "base":{"sha":"b".repeat(40),"repo":{"id":9,"full_name":"acme/demo"}}}})).unwrap();
    raw.extend_from_slice(b"\r\n");
    raw
}
fn headers(raw: &[u8], n: u32) -> app::Headers {
    headers_with_key(raw, n, KEY)
}
fn headers_with_key(raw: &[u8], n: u32, key: &[u8]) -> app::Headers {
    let mut mac = Hmac::<sha2::Sha256>::new_from_slice(key).unwrap();
    mac.update(raw);
    app::Headers::parse(
        &serde_json::to_vec(&json!({"schemaVersion":1,"event":"pull_request",
        "deliveryId":format!("01234567-89ab-4cde-8000-{n:012x}"),
        "signature256":format!("sha256={}",hex::encode(mac.finalize().into_bytes()))}))
        .unwrap(),
    )
    .unwrap()
}
struct Fixture {
    store: Store,
    host: app::Host,
    o: worker::Options,
    clock: worker::Clock,
    time: Arc<AtomicU64>,
    d: Deadline,
    job: String,
    revision: usize,
    root: tempfile::TempDir,
}
impl Fixture {
    fn new(selected: bool) -> Self {
        Self::with_action(selected, "opened")
    }
    fn with_action(selected: bool, action: &str) -> Self {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("untouched.rs"),
            b"// exact caf\xc3\xa9\r\npub fn untouched() {}\r\n",
        )
        .unwrap();
        let store = Store::open(root.path()).unwrap();
        let host = host(9, "acme/demo");
        let o = worker::Options {
            queue: app::Options {
                allow_writes: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let d = Deadline::new(30_000);
        let raw = body('a', action);
        let admitted = app::ingest(&store, &host, &headers(&raw, 1), &raw, &o.queue, &d).unwrap();
        let job = admitted["jobId"].as_str().unwrap().to_owned();
        if selected {
            app::select(&store, &host, &job, 1, &o.queue, &d).unwrap();
        }
        let time = Arc::new(AtomicU64::new(NOW));
        let clock = worker::Clock::for_fixture(time.clone());
        Self {
            root,
            store,
            host,
            o,
            clock,
            time,
            d,
            job,
            revision: if selected { 2 } else { 1 },
        }
    }
    fn request(&self, n: u32) -> worker::ClaimRequest {
        worker::ClaimRequest::parse(
            &serde_json::to_vec(&json!({"schemaVersion":1,"jobId":self.job,
            "expectedQueueRevision":self.revision,"worker":"owned-worker-A",
            "requestId":format!("89abcdef-0123-4abc-8000-{n:012x}"),"leaseSeconds":60}))
            .unwrap(),
        )
        .unwrap()
    }
    fn claim(&self, r: &worker::ClaimRequest) -> anyhow::Result<worker::Claim> {
        worker::claim(&self.store, &self.host, r, &self.o, &self.clock, &self.d)
    }
    fn rows(&self) -> usize {
        self.store
            .db
            .query_row(
                "SELECT count(*) FROM records WHERE kind=?1",
                [worker::RECORD_KIND],
                |r| r.get(0),
            )
            .unwrap()
    }
    fn status(&self) -> anyhow::Result<Value> {
        worker::status(
            &self.store,
            &self.host,
            &worker::StatusRequest::default(),
            &self.o,
            &self.clock,
            &self.d,
        )
    }
}
#[test]
fn selected_claim_is_durable_fenced_and_preserves_source_bytes() {
    let f = Fixture::new(true);
    let result = f.claim(&f.request(1)).unwrap();
    let v = result.evidence();
    assert_eq!(v["jobId"], f.job);
    assert_eq!(v["fence"], 1);
    assert_eq!(v["worker"], "owned-worker-A");
    assert_eq!(v["claimedAtUnix"], NOW);
    assert_eq!(v["expiresAtUnix"], NOW + 60);
    assert_eq!(v["queueRevision"], 2);
    assert_eq!(v["sourceWritten"], false);
    assert_eq!(v["published"], false);
    assert_eq!(v["complete"], false);
    assert_eq!(v["claimId"].as_str().unwrap().len(), 64);
    assert_eq!(f.rows(), 1);
    let status = f.status().unwrap();
    assert_eq!(status["total"], 1);
    assert_eq!(status["jobs"][0]["status"], "claimed");
    assert_eq!(
        std::fs::read(f.root.path().join("untouched.rs")).unwrap(),
        b"// exact caf\xc3\xa9\r\npub fn untouched() {}\r\n"
    );
    assert!(
        !serde_json::to_string(v)
            .unwrap()
            .contains(std::str::from_utf8(KEY).unwrap())
    );
}
#[test]
fn unselected_suppressed_and_closed_discussion_jobs_cannot_be_claimed_for_review() {
    let f = Fixture::new(false);
    assert!(f.claim(&f.request(1)).is_err());
    assert_eq!(f.rows(), 0);
    let f = Fixture::with_action(true, "closed");
    assert!(f.claim(&f.request(1)).is_err());
    assert_eq!(f.rows(), 0);
    let f = Fixture::new(true);
    let raw = body('c', "synchronize");
    let second = app::ingest(&f.store, &f.host, &headers(&raw, 2), &raw, &f.o.queue, &f.d).unwrap();
    app::select(
        &f.store,
        &f.host,
        second["jobId"].as_str().unwrap(),
        3,
        &f.o.queue,
        &f.d,
    )
    .unwrap();
    let mut r = f.request(1);
    r.expected_queue_revision = 4;
    assert!(f.claim(&r).is_err());
    assert_eq!(f.rows(), 0);
}
#[test]
fn exact_claim_retry_keeps_original_lease_instead_of_extending_it() {
    let f = Fixture::new(true);
    let r = f.request(1);
    let first = f.claim(&r).unwrap();
    f.time.store(NOW + 10, Ordering::SeqCst);
    let repeat = f.claim(&r).unwrap();
    assert_eq!(repeat.evidence()["claimId"], first.evidence()["claimId"]);
    assert_eq!(repeat.evidence()["expiresAtUnix"], NOW + 60);
    assert_eq!(repeat.evidence()["reused"], true);
    assert_eq!(f.rows(), 1);
}
#[test]
fn a_request_uuid_cannot_be_rebound_to_another_worker_or_lease() {
    let f = Fixture::new(true);
    let mut r = f.request(1);
    f.claim(&r).unwrap();
    r.worker = "owned-worker-B".into();
    assert!(f.claim(&r).is_err());
    r.worker = "owned-worker-A".into();
    r.lease_seconds = 61;
    assert!(f.claim(&r).is_err());
    assert_eq!(f.rows(), 1);
}
#[test]
fn competing_claim_refuses_until_expiry_then_takeover_fences_the_old_owner() {
    let f = Fixture::new(true);
    let r = f.request(1);
    let first = f.claim(&r).unwrap();
    assert!(f.claim(&f.request(2)).is_err());
    f.time.store(NOW + 60, Ordering::SeqCst);
    let second = f.claim(&f.request(2)).unwrap();
    assert_eq!(second.evidence()["fence"], 2);
    assert_eq!(second.evidence()["expiresAtUnix"], NOW + 120);
    assert!(
        worker::release(
            &f.store,
            &f.host,
            &first,
            "89abcdef-0123-4abc-8000-000000000003",
            &f.o,
            &f.clock,
            &f.d
        )
        .is_err()
    );
    let historical = f.claim(&r).unwrap();
    assert_eq!(historical.evidence()["fence"], 1);
    assert_eq!(historical.evidence()["expiresAtUnix"], NOW + 60);
    let status = f.status().unwrap();
    assert_eq!(status["jobs"][0]["fence"], 2);
    assert_eq!(f.rows(), 2);
}
#[test]
fn release_is_idempotent_and_cannot_release_a_later_claim() {
    let f = Fixture::new(true);
    let first = f.claim(&f.request(1)).unwrap();
    let id = "89abcdef-0123-4abc-8000-000000000003";
    let released = worker::release(&f.store, &f.host, &first, id, &f.o, &f.clock, &f.d).unwrap();
    assert_eq!(released["status"], "released");
    assert_eq!(f.status().unwrap()["jobs"][0]["status"], "released");
    let second = f.claim(&f.request(2)).unwrap();
    assert_eq!(second.evidence()["fence"], 2);
    let retried = worker::release(&f.store, &f.host, &first, id, &f.o, &f.clock, &f.d).unwrap();
    assert_eq!(retried["reused"], true);
    assert_eq!(f.status().unwrap()["jobs"][0]["fence"], 2);
    assert!(
        worker::release(
            &f.store,
            &f.host,
            &first,
            "89abcdef-0123-4abc-8000-000000000004",
            &f.o,
            &f.clock,
            &f.d
        )
        .is_err()
    );
    assert_eq!(f.rows(), 3);
}
#[test]
fn new_queue_membership_invalidates_an_old_observed_selection_revision() {
    let f = Fixture::new(true);
    let r = f.request(1);
    let raw = body('c', "synchronize");
    app::ingest(&f.store, &f.host, &headers(&raw, 2), &raw, &f.o.queue, &f.d).unwrap();
    assert!(f.claim(&r).is_err());
    assert_eq!(f.rows(), 0);
}
#[test]
fn permission_and_zero_work_limits_precede_any_database_operation() {
    use rusqlite::hooks::{AuthContext, Authorization};
    let f = Fixture::new(true);
    let r = f.request(1);
    f.store
        .db
        .authorizer(Some(|_: AuthContext<'_>| Authorization::Deny));
    let mut o = f.o.clone();
    o.queue.allow_writes = false;
    let error = worker::claim(&f.store, &f.host, &r, &o, &f.clock, &f.d)
        .err()
        .unwrap();
    assert_eq!(error.to_string(), "github_app_permission_required");
    for field in 0..4 {
        let mut o = f.o.clone();
        match field {
            0 => o.max_records = 0,
            1 => o.max_store_bytes = 0,
            2 => o.max_output_bytes = 0,
            _ => o.max_lease_seconds = 0,
        }
        let error = worker::claim(&f.store, &f.host, &r, &o, &f.clock, &f.d)
            .err()
            .unwrap();
        assert_eq!(error.to_string(), "github_app_worker_capacity_exceeded");
    }
    f.store
        .db
        .authorizer(None::<fn(AuthContext<'_>) -> Authorization>);
    assert_eq!(f.rows(), 0);
}
#[test]
fn insufficient_positive_output_or_history_capacity_never_appends_a_claim() {
    let f = Fixture::new(true);
    let r = f.request(1);
    for output in [true, false] {
        let mut o = f.o.clone();
        if output {
            o.max_output_bytes = 1
        } else {
            o.max_store_bytes = 1
        }
        assert!(worker::claim(&f.store, &f.host, &r, &o, &f.clock, &f.d).is_err());
        assert_eq!(f.rows(), 0);
    }
}
#[test]
fn host_repository_grants_are_required_even_with_a_valid_queue_signature() {
    let f = Fixture::new(true);
    let denied = host(10, "acme/other");
    let error = worker::claim(&f.store, &denied, &f.request(1), &f.o, &f.clock, &f.d)
        .err()
        .unwrap();
    assert_eq!(error.to_string(), "github_app_worker_grant_denied");
    assert_eq!(f.rows(), 0);
}
#[test]
fn clock_regression_refuses_a_new_transition_without_extending_existing_lease() {
    let f = Fixture::new(true);
    let claim = f.claim(&f.request(1)).unwrap();
    f.time.store(NOW - 1, Ordering::SeqCst);
    assert!(
        worker::release(
            &f.store,
            &f.host,
            &claim,
            "89abcdef-0123-4abc-8000-000000000002",
            &f.o,
            &f.clock,
            &f.d
        )
        .is_err()
    );
    assert_eq!(f.rows(), 1);
}
#[test]
fn independent_sqlite_writer_contention_is_observed_bounded_and_restores_transaction_state() {
    use rusqlite::hooks::{AuthAction, AuthContext, Authorization, TransactionOperation};
    use std::sync::atomic::AtomicBool;
    let f = Fixture::new(true);
    let blocker = rusqlite::Connection::open(f.store.dir.join("index.sqlite")).unwrap();
    assert!(
        blocker
            .query_row("SELECT count(*) FROM records", [], |r| r.get::<_, u64>(0))
            .is_ok()
    );
    blocker.execute_batch("BEGIN IMMEDIATE").unwrap();
    let began = Arc::new(AtomicBool::new(false));
    let observed = began.clone();
    f.store.db.authorizer(Some(move |context: AuthContext<'_>| {
        if matches!(
            context.action,
            AuthAction::Transaction {
                operation: TransactionOperation::Begin
            }
        ) {
            observed.store(true, Ordering::SeqCst);
        }
        Authorization::Allow
    }));
    let start = Instant::now();
    let result = f.claim(&f.request(1));
    let elapsed = start.elapsed();
    assert!(
        began.load(Ordering::SeqCst),
        "must observe the real transaction while the independent writer is locked"
    );
    assert!(result.is_err());
    assert!(
        elapsed < Duration::from_millis(750),
        "claim waited {elapsed:?}"
    );
    assert!(f.store.db.is_autocommit());
    assert_eq!(f.rows(), 0);
    blocker.execute_batch("ROLLBACK").unwrap();
}
#[test]
fn work_record_tampering_cannot_be_hidden_by_recomputing_its_content_id() {
    let f = Fixture::new(true);
    f.claim(&f.request(1)).unwrap();
    let (id, raw): (String, String) = f
        .store
        .db
        .query_row(
            "SELECT id,value FROM records WHERE kind=?1",
            [worker::RECORD_KIND],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    let mut value: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(yoke::model::identity(&value), id);
    value["event"]["request"]["worker"] = json!("tampered-worker");
    let changed = yoke::model::identity(&value);
    f.store
        .db
        .execute(
            "UPDATE records SET id=?1,value=?2 WHERE kind=?3 AND id=?4",
            rusqlite::params![changed, value.to_string(), worker::RECORD_KIND, id],
        )
        .unwrap();
    assert!(f.status().is_err());
}
#[test]
fn malformed_and_ambiguous_claim_requests_refuse_instead_of_normalizing_authority() {
    let f = Fixture::new(true);
    let good = serde_json::to_value(f.request(1)).unwrap();
    for (key, value) in [
        ("worker", json!("λ")),
        ("worker", json!("bad\u{0000}worker")),
        ("requestId", json!("ABCDEF")),
        ("leaseSeconds", json!(0)),
        ("schemaVersion", json!(2)),
        ("expectedQueueRevision", json!(0)),
        ("extra", json!(true)),
    ] {
        let mut bad = good.clone();
        bad[key] = value;
        assert!(
            worker::ClaimRequest::parse(&serde_json::to_vec(&bad).unwrap()).is_err(),
            "invalid {key}"
        );
    }
    let raw = serde_json::to_string(&good)
        .unwrap()
        .replacen('{', "{\"worker\":\"duplicate\",", 1);
    assert!(worker::ClaimRequest::parse(raw.as_bytes()).is_err());
}
#[test]
fn empty_work_status_is_read_only_and_has_explicit_coverage() {
    let f = Fixture::new(false);
    let status = f.status().unwrap();
    assert_eq!(status["total"], 0);
    assert_eq!(status["jobs"], json!([]));
    assert_eq!(status["hasMore"], false);
    assert_eq!(f.rows(), 0);
    assert_eq!(status["sourceWritten"], false);
    assert_eq!(status["published"], false);
}
#[test]
fn a_reopened_store_reconstructs_the_active_claim_from_durable_history() {
    let f = Fixture::new(true);
    let claim = f.claim(&f.request(1)).unwrap();
    let reopened = Store::open(f.root.path()).unwrap();
    let status = worker::status(
        &reopened,
        &f.host,
        &worker::StatusRequest::default(),
        &f.o,
        &f.clock,
        &f.d,
    )
    .unwrap();
    assert_eq!(status["total"], 1);
    assert_eq!(status["jobs"][0]["claimId"], claim.evidence()["claimId"]);
    assert_eq!(status["jobs"][0]["status"], "claimed");
}
#[test]
fn a_caller_transaction_is_preserved_and_cannot_be_committed_by_claim() {
    let f = Fixture::new(true);
    f.store.db.execute_batch("BEGIN").unwrap();
    assert!(f.claim(&f.request(1)).is_err());
    assert!(!f.store.db.is_autocommit());
    f.store.db.execute_batch("ROLLBACK").unwrap();
    assert_eq!(f.rows(), 0);
}
#[test]
fn rotation_verifies_prior_work_macs_and_new_records_require_the_new_key() {
    let f = Fixture::new(true);
    let first = f.claim(&f.request(1)).unwrap();
    let rotated=app::Host::with_signing_keys(app::HostProfile::parse(&serde_json::to_vec(&json!({
        "schemaVersion":1,"appId":123,"secretCredential":"fixture-new","priorSecretCredentials":["fixture-old"],
        "grants":[{"installationId":42,"repositoryId":9,"repository":"acme/demo"}]})).unwrap()).unwrap(),
        vec![app::SigningKey::from_host_bytes(b"public owned replacement worker key").unwrap(),app::SigningKey::from_host_bytes(KEY).unwrap()]).unwrap();
    worker::status(
        &f.store,
        &rotated,
        &worker::StatusRequest::default(),
        &f.o,
        &f.clock,
        &f.d,
    )
    .unwrap();
    worker::release(
        &f.store,
        &rotated,
        &first,
        "89abcdef-0123-4abc-8000-000000000002",
        &f.o,
        &f.clock,
        &f.d,
    )
    .unwrap();
    assert!(
        f.status().is_err(),
        "old-only host must not authenticate a new-key work record"
    );
    let status = worker::status(
        &f.store,
        &rotated,
        &worker::StatusRequest::default(),
        &f.o,
        &f.clock,
        &f.d,
    )
    .unwrap();
    assert_eq!(status["jobs"][0]["status"], "released");
}
#[test]
fn multiple_apps_share_structural_history_without_sharing_signing_authority() {
    let f = Fixture::new(true);
    let first = f.claim(&f.request(1)).unwrap();
    let key_b = b"public owned separate app worker key";
    let second_host = app::Host::with_signing_key(
        app::HostProfile::parse(
            &serde_json::to_vec(&json!({
        "schemaVersion":1,"appId":124,"secretCredential":"fixture-other-app",
        "grants":[{"installationId":42,"repositoryId":9,"repository":"acme/demo"}]}))
            .unwrap(),
        )
        .unwrap(),
        app::SigningKey::from_host_bytes(key_b).unwrap(),
    )
    .unwrap();
    let raw = body('a', "opened");
    let admitted = app::ingest(
        &f.store,
        &second_host,
        &headers_with_key(&raw, 2, key_b),
        &raw,
        &f.o.queue,
        &f.d,
    )
    .unwrap();
    let job = admitted["jobId"].as_str().unwrap();
    app::select(&f.store, &second_host, job, 1, &f.o.queue, &f.d).unwrap();
    let mut request = f.request(1);
    request.job_id = job.to_owned();
    request.worker = "owned-worker-B".into();
    let second = worker::claim(&f.store, &second_host, &request, &f.o, &f.clock, &f.d).unwrap();
    let status_a = f.status().unwrap();
    let status_b = worker::status(
        &f.store,
        &second_host,
        &worker::StatusRequest::default(),
        &f.o,
        &f.clock,
        &f.d,
    )
    .unwrap();
    assert_eq!(status_a["total"], 1);
    assert_eq!(status_b["total"], 1);
    assert_eq!(status_a["jobs"][0]["claimId"], first.evidence()["claimId"]);
    assert_eq!(status_b["jobs"][0]["claimId"], second.evidence()["claimId"]);
    assert!(f.claim(&request).is_err());
    assert_eq!(f.rows(), 2);
}
#[test]
fn operation_restores_the_prior_store_deadline_and_busy_policy_on_success_and_refusal() {
    use std::sync::atomic::AtomicBool;
    for success in [false, true] {
        let mut f = Fixture::new(true);
        let stop = Arc::new(AtomicBool::new(false));
        let prior =
            Deadline::run_bounded(&f.d, stop.clone(), || Ok(Deadline::new(30_000))).unwrap();
        f.store.set_deadline(&prior).unwrap();
        f.store.db.busy_timeout(Duration::from_millis(125)).unwrap();
        if !success {
            f.o.max_output_bytes = 1;
        }
        let result = f.claim(&f.request(1));
        if success {
            result.unwrap();
        } else {
            assert!(result.is_err());
        }
        let busy: u64 = f
            .store
            .db
            .query_row("PRAGMA busy_timeout", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            busy, 125,
            "the caller's actual busy policy must be restored"
        );
        stop.store(true, Ordering::SeqCst);
        let direct = f.store.db.query_row(
            "WITH RECURSIVE work(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM work WHERE n<100000) SELECT sum(n) FROM work",
            [], |r| r.get::<_,u64>(0),
        );
        assert!(
            matches!(direct, Err(rusqlite::Error::SqliteFailure(ref error, _))
            if error.code == rusqlite::ErrorCode::OperationInterrupted),
            "the prior SQLite progress handler must interrupt direct SQL: {direct:?}"
        );
        let attempted = f.store.record(
            "owned-deadline-fixture",
            &"c".repeat(64),
            &json!({"own":true}),
        );
        assert!(
            attempted.is_err(),
            "the prior Store deadline must still refuse a later write after cancellation"
        );
        f.store.clear_deadline();
    }
}

#[test]
fn observed_expiry_cannot_be_reversed_by_rewinding_the_same_host_clock() {
    let f = Fixture::new(true);
    let claim = f.claim(&f.request(1)).unwrap();
    f.time.store(NOW + 61, Ordering::SeqCst);
    assert_eq!(f.status().unwrap()["jobs"][0]["status"], "expired");
    f.time.store(NOW + 30, Ordering::SeqCst);
    assert!(
        f.status().is_err(),
        "a read must refuse a clock older than its prior observation"
    );
    assert!(
        worker::release(
            &f.store,
            &f.host,
            &claim,
            "89abcdef-0123-4abc-8000-000000000002",
            &f.o,
            &f.clock,
            &f.d
        )
        .is_err()
    );
    assert_eq!(f.rows(), 1);
}

#[test]
fn a_new_claim_for_an_unrelated_pull_request_does_not_stale_the_original_queue_revision() {
    let f = Fixture::new(true);
    f.claim(&f.request(1)).unwrap();
    let mut payload: Value = serde_json::from_slice(&body('c', "opened")).unwrap();
    payload["number"] = json!(2);
    payload["pull_request"]["number"] = json!(2);
    let raw = serde_json::to_vec(&payload).unwrap();
    let other = app::ingest(&f.store, &f.host, &headers(&raw, 2), &raw, &f.o.queue, &f.d).unwrap();
    app::select(
        &f.store,
        &f.host,
        other["jobId"].as_str().unwrap(),
        1,
        &f.o.queue,
        &f.d,
    )
    .unwrap();
    let status = f.status().unwrap();
    assert_eq!(status["jobs"][0]["queueStale"], false);
    assert_eq!(status["jobs"][0]["status"], "claimed");
}

#[test]
fn worker_history_is_recognized_by_retention_without_promoting_arbitrary_text_to_cas_roots() {
    let f = Fixture::new(true);
    f.claim(&f.request(1)).unwrap();
    let report = yoke::retention::plan(&f.store, &Default::default(), &f.d).unwrap();
    assert!(
        !report["unknownDependencyKinds"]
            .as_array()
            .unwrap()
            .contains(&json!(worker::RECORD_KIND)),
        "closed worker history must have a structural retention validator"
    );
}

#[test]
fn malformed_worker_history_blocks_retention_instead_of_becoming_a_generic_record() {
    let f = Fixture::new(true);
    f.claim(&f.request(1)).unwrap();
    let raw = json!({"schemaVersion":1,"workspaceId":f.store.workspace_id,"unknown":"opaque"});
    f.store
        .db
        .execute(
            "UPDATE records SET value=?1 WHERE kind=?2",
            rusqlite::params![raw.to_string(), worker::RECORD_KIND],
        )
        .unwrap();
    assert!(yoke::retention::plan(&f.store, &Default::default(), &f.d).is_err());
}

#[test]
fn historical_claim_remains_readable_after_queue_selection_advances_and_is_marked_stale() {
    let f = Fixture::new(true);
    let original = f.request(1);
    let claim = f.claim(&original).unwrap();
    let raw = body('c', "synchronize");
    let second = app::ingest(&f.store, &f.host, &headers(&raw, 2), &raw, &f.o.queue, &f.d).unwrap();
    app::select(
        &f.store,
        &f.host,
        second["jobId"].as_str().unwrap(),
        3,
        &f.o.queue,
        &f.d,
    )
    .unwrap();
    let status = f.status().unwrap();
    assert_eq!(status["jobs"][0]["claimId"], claim.evidence()["claimId"]);
    assert_eq!(status["jobs"][0]["queueStale"], true);
    let retry = f.claim(&original).unwrap();
    assert_eq!(retry.evidence()["claimId"], claim.evidence()["claimId"]);
    assert_eq!(retry.evidence()["historical"], true);
    assert!(f.claim(&f.request(2)).is_err());
    assert_eq!(f.rows(), 1);
}

#[test]
fn read_only_status_preserves_an_existing_caller_transaction() {
    let f = Fixture::new(true);
    f.claim(&f.request(1)).unwrap();
    f.store.db.execute_batch("BEGIN").unwrap();
    assert_eq!(f.status().unwrap()["total"], 1);
    assert!(!f.store.db.is_autocommit());
    f.store.db.execute_batch("ROLLBACK").unwrap();
}

#[test]
fn independently_opened_store_connections_cannot_both_claim_the_same_job() {
    let f = Fixture::new(true);
    let gate = Arc::new(std::sync::Barrier::new(2));
    let handles: Vec<_> = (1..=2)
        .map(|n| {
            let root = f.root.path().to_owned();
            let gate = gate.clone();
            let request = f.request(n);
            std::thread::spawn(move || {
                let store = Store::open(&root).unwrap();
                let host = host(9, "acme/demo");
                let mut options = worker::Options::default();
                options.queue.allow_writes = true;
                let clock = worker::Clock::for_fixture(Arc::new(AtomicU64::new(NOW)));
                let deadline = Deadline::new(10_000);
                gate.wait();
                worker::claim(&store, &host, &request, &options, &clock, &deadline).is_ok()
            })
        })
        .collect();
    let accepted = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .filter(|accepted| *accepted)
        .count();
    assert_eq!(accepted, 1);
    assert_eq!(f.rows(), 1);
}

#[test]
fn cancellation_triggered_by_an_observed_sql_begin_rolls_back_without_a_work_record() {
    use rusqlite::hooks::{AuthAction, AuthContext, Authorization, TransactionOperation};
    use std::sync::atomic::AtomicBool;
    let f = Fixture::new(true);
    let stop = Arc::new(AtomicBool::new(false));
    let observed = stop.clone();
    f.store.db.authorizer(Some(move |context: AuthContext<'_>| {
        if matches!(
            context.action,
            AuthAction::Transaction {
                operation: TransactionOperation::Begin
            }
        ) {
            observed.store(true, Ordering::SeqCst);
        }
        Authorization::Allow
    }));
    let result = Deadline::run_bounded(&f.d, stop.clone(), || {
        worker::claim(
            &f.store,
            &f.host,
            &f.request(1),
            &f.o,
            &f.clock,
            &Deadline::new(30_000),
        )
    });
    assert!(
        stop.load(Ordering::SeqCst),
        "the actual SQL transaction must be observed"
    );
    assert!(
        result
            .err()
            .unwrap()
            .to_string()
            .to_ascii_lowercase()
            .contains("cancel")
    );
    assert!(f.store.db.is_autocommit());
    assert_eq!(f.rows(), 0);
}

#[test]
fn exact_output_and_history_byte_boundaries_succeed_and_one_byte_less_refuses() {
    let probe = Fixture::new(true);
    let claim = probe.claim(&probe.request(1)).unwrap();
    let output_bytes = serde_json::to_vec(claim.evidence()).unwrap().len() + 1;
    let history_bytes: usize = probe
        .store
        .db
        .query_row(
            "SELECT length(CAST(value AS BLOB)) FROM records WHERE kind=?1",
            [worker::RECORD_KIND],
            |r| r.get(0),
        )
        .unwrap();
    for reduction in [None, Some(true), Some(false)] {
        let mut f = Fixture::new(true);
        f.o.max_output_bytes = output_bytes - usize::from(reduction == Some(true));
        f.o.max_store_bytes = history_bytes - usize::from(reduction == Some(false));
        let result = f.claim(&f.request(1));
        if reduction.is_none() {
            result.unwrap();
            assert_eq!(f.rows(), 1);
        } else {
            assert!(result.is_err());
            assert_eq!(f.rows(), 0);
        }
    }
}

#[test]
fn reopened_store_recovers_the_exact_claim_identity_and_can_release_it() {
    let f = Fixture::new(true);
    let claim = f.claim(&f.request(1)).unwrap();
    let reopened = Store::open(f.root.path()).unwrap();
    let id = claim.evidence()["claimId"].as_str().unwrap();
    let recovered = worker::recover(&reopened, &f.host, id, &f.o, &f.clock, &f.d).unwrap();
    assert_eq!(recovered.evidence()["claimId"], id);
    assert_eq!(recovered.evidence()["historical"], true);
    assert_eq!(recovered.evidence()["executionAuthority"], false);
    assert_eq!(recovered.evidence()["expiresAtUnix"], NOW + 60);
    worker::release(
        &reopened,
        &f.host,
        &recovered,
        "89abcdef-0123-4abc-8000-000000000002",
        &f.o,
        &f.clock,
        &f.d,
    )
    .unwrap();
    assert_eq!(f.rows(), 2);
    assert_eq!(f.status().unwrap()["jobs"][0]["status"], "released");
}

#[test]
fn recovery_refuses_invalid_unknown_nonclaim_and_ungranted_record_ids() {
    let f = Fixture::new(true);
    let claim = f.claim(&f.request(1)).unwrap();
    let id = claim.evidence()["claimId"].as_str().unwrap();
    let denied = host(10, "acme/other");
    let error = worker::recover(&f.store, &denied, id, &f.o, &f.clock, &f.d)
        .err()
        .unwrap();
    assert_eq!(error.to_string(), "github_app_worker_grant_denied");
    for invalid in ["bad-id".to_owned(), "f".repeat(64)] {
        assert!(worker::recover(&f.store, &f.host, &invalid, &f.o, &f.clock, &f.d).is_err());
    }
    let release = worker::release(
        &f.store,
        &f.host,
        &claim,
        "89abcdef-0123-4abc-8000-000000000002",
        &f.o,
        &f.clock,
        &f.d,
    )
    .unwrap();
    assert!(
        worker::recover(
            &f.store,
            &f.host,
            release["releaseId"].as_str().unwrap(),
            &f.o,
            &f.clock,
            &f.d
        )
        .is_err()
    );
    assert_eq!(f.rows(), 2);
}

#[test]
fn recovering_an_expired_superseded_claim_does_not_restore_its_authority() {
    let f = Fixture::new(true);
    let first = f.claim(&f.request(1)).unwrap();
    f.time.store(NOW + 60, Ordering::SeqCst);
    f.claim(&f.request(2)).unwrap();
    let recovered = worker::recover(
        &f.store,
        &f.host,
        first.evidence()["claimId"].as_str().unwrap(),
        &f.o,
        &f.clock,
        &f.d,
    )
    .unwrap();
    assert_eq!(recovered.evidence()["fence"], 1);
    assert_eq!(recovered.evidence()["expiresAtUnix"], NOW + 60);
    assert_eq!(recovered.evidence()["historical"], true);
    assert!(
        worker::release(
            &f.store,
            &f.host,
            &recovered,
            "89abcdef-0123-4abc-8000-000000000003",
            &f.o,
            &f.clock,
            &f.d
        )
        .is_err()
    );
    assert_eq!(f.status().unwrap()["jobs"][0]["fence"], 2);
    assert_eq!(f.rows(), 2);
}

#[test]
fn recovery_is_read_only_and_checks_input_and_zero_capacity_before_sql() {
    use rusqlite::hooks::{AuthContext, Authorization};
    let f = Fixture::new(true);
    let claim = f.claim(&f.request(1)).unwrap();
    let id = claim.evidence()["claimId"].as_str().unwrap();
    let mut o = f.o.clone();
    o.queue.allow_writes = false;
    f.store.db.execute_batch("BEGIN").unwrap();
    worker::recover(&f.store, &f.host, id, &o, &f.clock, &f.d).unwrap();
    assert!(!f.store.db.is_autocommit());
    f.store.db.execute_batch("ROLLBACK").unwrap();
    f.store
        .db
        .authorizer(Some(|_: AuthContext<'_>| Authorization::Deny));
    let error = worker::recover(&f.store, &f.host, "not-an-id", &o, &f.clock, &f.d)
        .err()
        .unwrap();
    assert_eq!(error.to_string(), "github_app_worker_input_invalid");
    o.max_records = 0;
    let error = worker::recover(&f.store, &f.host, id, &o, &f.clock, &f.d)
        .err()
        .unwrap();
    assert_eq!(error.to_string(), "github_app_worker_capacity_exceeded");
    f.store
        .db
        .authorizer(None::<fn(AuthContext<'_>) -> Authorization>);
    assert_eq!(f.rows(), 1);
}
