//! Signed raw bodies, immutable admission, and independently expected queue state.
use hmac::{Hmac, Mac};
use rusqlite::{limits::Limit, params};
use serde_json::{Value, json};
use yoke::{capture::Deadline, github_app as app, model::hash, store::Store};

const KEY: &[u8] = b"public owned deterministic webhook fixture key";
fn host() -> app::Host {
    app::Host::with_signing_key(
        app::HostProfile::parse(
            &serde_json::to_vec(&json!({"schemaVersion":1,"appId":123,
            "secretCredential":"not-read-by-in-memory-host","grants":[{"installationId":42,
            "repositoryId":9,"repository":"acme/demo"}]}))
            .unwrap(),
        )
        .unwrap(),
        app::SigningKey::from_host_bytes(KEY).unwrap(),
    )
    .unwrap()
}
fn body(head: char, base: char, action: &str) -> Vec<u8> {
    let value = json!({"action":action,"number":7,"installation":{"id":42},
        "repository":{"id":9,"full_name":"acme/demo"},"pull_request":{"number":7,
        "state":if action == "closed" {"closed"} else {"open"},"draft":false,
        "title":"Grüße 🌍\r\nline two","head":{"sha":head.to_string().repeat(40)},
        "base":{"sha":base.to_string().repeat(40),"repo":{"id":9,"full_name":"acme/demo"}}}});
    let mut raw = serde_json::to_vec_pretty(&value).unwrap();
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
    _dir: tempfile::TempDir,
    store: Store,
    host: app::Host,
    d: Deadline,
    o: app::Options,
}
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("untouched.rs"),
            b"// source caf\xc3\xa9\r\n",
        )
        .unwrap();
        let store = Store::open(dir.path()).unwrap();
        Self {
            _dir: dir,
            store,
            host: host(),
            d: Deadline::new(60_000),
            o: app::Options {
                allow_writes: true,
                ..Default::default()
            },
        }
    }
    fn ingest(&self, raw: &[u8], n: u32) -> anyhow::Result<Value> {
        app::ingest(
            &self.store,
            &self.host,
            &headers(raw, n),
            raw,
            &self.o,
            &self.d,
        )
    }
    fn rows(&self) -> usize {
        self.store
            .db
            .query_row(
                "SELECT count(*) FROM records WHERE kind=?1",
                [app::RECORD_KIND],
                |r| r.get(0),
            )
            .unwrap()
    }
    fn queue(&self) -> Value {
        app::queue(&self.store, &self.host, 0, 100, &self.o, &self.d).unwrap()
    }
}
#[test]
fn independent_official_hmac_vector_and_exact_body_mutations() {
    let key = app::SigningKey::from_host_bytes(b"It's a Secret to Everybody").unwrap();
    let signature = "sha256=757107ea0eb2509fc211221cce984b8a37570b6d7586c22c46f4379c8b043e17";
    key.verify(b"Hello, World!", signature).unwrap();
    assert!(key.verify(b"Hello, World!\n", signature).is_err());
    assert!(key.verify(b"hello, World!", signature).is_err());
    assert!(
        key.verify(b"Hello, World!", &signature.to_uppercase())
            .is_err()
    );
}
#[test]
fn original_utf8_crlf_body_replay_and_changed_header_id_cannot_multiply_work() {
    let f = Fixture::new();
    let raw = body('a', 'b', "synchronize");
    let first = f.ingest(&raw, 1).unwrap();
    assert_eq!(first["payloadHash"], hash(&raw));
    assert_eq!(first["reused"], false);
    let repeated = f.ingest(&raw, 1).unwrap();
    assert_eq!(repeated["reused"], true);
    let relabeled = f.ingest(&raw, 2).unwrap();
    assert_eq!(relabeled["reused"], true);
    assert_eq!(relabeled["jobId"], first["jobId"]);
    assert_eq!(f.rows(), 1);
    let stored: String = f
        .store
        .db
        .query_row(
            "SELECT value FROM records WHERE kind=?1",
            [app::RECORD_KIND],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&stored).unwrap()["event"]["payload"]
            .as_str()
            .unwrap()
            .as_bytes(),
        raw
    );
    assert!(!stored.contains(std::str::from_utf8(KEY).unwrap()));
    assert_eq!(
        std::fs::read(f._dir.path().join("untouched.rs")).unwrap(),
        b"// source caf\xc3\xa9\r\n"
    );
    assert_eq!(f.queue()["total"], 1);
    assert_eq!(first["sourceWritten"], false);
    assert_eq!(first["modelCalls"], 0);
}
#[test]
fn id_conflict_refuses_and_metadata_does_not_duplicate_a_job() {
    let f = Fixture::new();
    let raw = body('a', 'b', "opened");
    f.ingest(&raw, 1).unwrap();
    let mut changed: Value = serde_json::from_slice(&raw).unwrap();
    changed["pull_request"]["title"] = json!("Different authenticated title");
    let changed = serde_json::to_vec(&changed).unwrap();
    assert!(
        f.ingest(&changed, 1)
            .unwrap_err()
            .to_string()
            .contains("conflict")
    );
    f.ingest(&changed, 2).unwrap();
    assert_eq!(f.rows(), 2);
    assert_eq!(f.queue()["total"], 1);
}
#[test]
fn authenticated_repository_display_case_preserves_raw_bytes_and_canonical_job_identity() {
    let f = Fixture::new();
    let raw = body('a', 'b', "opened");
    let original = f.ingest(&raw, 1).unwrap();
    let mut value: Value = serde_json::from_slice(&raw).unwrap();
    value["repository"]["full_name"] = json!("AcMe/DeMo");
    value["pull_request"]["base"]["repo"]["full_name"] = json!("ACME/demo");
    let displayed = serde_json::to_vec(&value).unwrap();
    let accepted = f.ingest(&displayed, 2).unwrap();
    assert_eq!(accepted["jobId"], original["jobId"]);
    assert_eq!(accepted["payloadHash"], hash(&displayed));
    assert_eq!(f.queue()["total"], 1);
    assert_eq!(f.rows(), 2);
}
#[test]
fn special_github_repository_name_is_a_valid_host_grant_and_authenticated_payload() {
    let f = Fixture::new();
    let profile=app::HostProfile::parse(&serde_json::to_vec(&json!({"schemaVersion":1,"appId":123,
        "secretCredential":"in-memory","grants":[{"installationId":42,"repositoryId":9,"repository":"acme/.github"}]})).unwrap()).unwrap();
    let host = app::Host::with_signing_key(profile, app::SigningKey::from_host_bytes(KEY).unwrap())
        .unwrap();
    let mut payload: Value = serde_json::from_slice(&body('a', 'b', "opened")).unwrap();
    payload["repository"]["full_name"] = json!("AcMe/.github");
    payload["pull_request"]["base"]["repo"]["full_name"] = json!("acme/.github");
    let raw = serde_json::to_vec(&payload).unwrap();
    app::ingest(&f.store, &host, &headers(&raw, 1), &raw, &f.o, &f.d).unwrap();
    assert_eq!(
        app::queue(&f.store, &host, 0, 100, &f.o, &f.d).unwrap()["total"],
        1
    );
}
#[test]
fn base_sha_changes_review_identity_and_closed_work_is_separate() {
    let f = Fixture::new();
    let first = f.ingest(&body('a', 'b', "opened"), 1).unwrap();
    let second = f.ingest(&body('a', 'c', "synchronize"), 2).unwrap();
    let closed = f.ingest(&body('a', 'c', "closed"), 3).unwrap();
    assert_ne!(first["jobId"], second["jobId"]);
    assert_ne!(second["jobId"], closed["jobId"]);
    let q = f.queue();
    assert_eq!(q["total"], 3);
    assert_eq!(q["currentHeadVerified"], false);
    assert_eq!(q["complete"], false);
}
#[test]
fn explicit_selection_uses_group_revision_and_late_arrivals_stay_queued() {
    let f = Fixture::new();
    let a = f.ingest(&body('a', 'b', "opened"), 1).unwrap();
    let c = f.ingest(&body('c', 'b', "synchronize"), 2).unwrap();
    let selected = app::select(
        &f.store,
        &f.host,
        a["jobId"].as_str().unwrap(),
        2,
        &f.o,
        &f.d,
    )
    .unwrap();
    assert_eq!(selected["groupRevision"], 3);
    assert!(
        app::select(
            &f.store,
            &f.host,
            c["jobId"].as_str().unwrap(),
            2,
            &f.o,
            &f.d
        )
        .is_err()
    );
    let late = f.ingest(&body('d', 'b', "synchronize"), 3).unwrap();
    let repeated = app::select(
        &f.store,
        &f.host,
        a["jobId"].as_str().unwrap(),
        2,
        &f.o,
        &f.d,
    )
    .unwrap();
    assert_eq!(repeated, selected);
    let q = f.queue();
    let jobs = q["jobs"].as_array().unwrap();
    for (id, status) in [
        (&a["jobId"], "selected"),
        (&c["jobId"], "suppressed"),
        (&late["jobId"], "queued"),
    ] {
        assert_eq!(
            jobs.iter().find(|j| j["job"]["id"] == *id).unwrap()["status"],
            status
        );
    }
    assert_eq!(
        q["selectionAuthority"],
        "explicit_host_scheduling_assertion"
    );
    assert_eq!(f.rows(), 4);
}
#[test]
fn new_job_between_queue_read_and_selection_invalidates_the_observed_revision() {
    let f = Fixture::new();
    let a = f.ingest(&body('a', 'b', "opened"), 1).unwrap();
    let observed = f.queue()["jobs"][0]["groupRevision"].as_u64().unwrap() as usize;
    assert_eq!(observed, 1);
    f.ingest(&body('c', 'b', "synchronize"), 2).unwrap();
    assert!(
        app::select(
            &f.store,
            &f.host,
            a["jobId"].as_str().unwrap(),
            observed,
            &f.o,
            &f.d
        )
        .is_err()
    );
    assert!(
        f.queue()["jobs"]
            .as_array()
            .unwrap()
            .iter()
            .all(|j| j["status"] == "queued" && j["groupRevision"] == 2)
    );
    assert_eq!(f.rows(), 2);
}
#[test]
fn signatures_and_body_identity_are_admitted_before_json_and_store_changes() {
    let f = Fixture::new();
    let raw = body('a', 'b', "opened");
    let h = headers(&raw, 1);
    assert!(
        app::ingest(&f.store, &f.host, &h, b"not-json", &f.o, &f.d)
            .unwrap_err()
            .to_string()
            .contains("signature")
    );
    assert_eq!(f.rows(), 0);
    let mut tampered = raw.clone();
    tampered.push(b' ');
    assert!(app::ingest(&f.store, &f.host, &h, &tampered, &f.o, &f.d).is_err());
}
#[test]
fn strict_host_headers_and_authenticated_payload_denials_are_static() {
    let f = Fixture::new();
    let raw = body('a', 'b', "opened");
    for pointer in [
        "/installation/id",
        "/repository/id",
        "/pull_request/base/repo/id",
        "/number",
        "/pull_request/number",
    ] {
        let mut v: Value = serde_json::from_slice(&raw).unwrap();
        *v.pointer_mut(pointer).unwrap() = json!(99);
        assert!(
            f.ingest(&serde_json::to_vec(&v).unwrap(), 1).is_err(),
            "{pointer}"
        );
    }
    for (pointer, value) in [
        ("/action", json!("unrecognized-sensitive")),
        ("/pull_request/state", json!("closed")),
        ("/pull_request/head/sha", json!("A".repeat(40))),
        ("/repository/full_name", json!("other/repo")),
    ] {
        let mut v: Value = serde_json::from_slice(&raw).unwrap();
        *v.pointer_mut(pointer).unwrap() = value;
        let error = f
            .ingest(&serde_json::to_vec(&v).unwrap(), 1)
            .unwrap_err()
            .to_string();
        assert!(!error.contains("sensitive"));
    }
    let duplicate = String::from_utf8(raw.clone()).unwrap().replacen(
        "\"action\": \"opened\"",
        "\"action\": \"opened\", \"action\": \"opened\"",
        1,
    );
    assert_ne!(duplicate.as_bytes(), raw.as_slice());
    assert!(f.ingest(duplicate.as_bytes(), 1).is_err());
    let mut unknown = serde_json::to_value(headers(&raw, 1)).unwrap();
    unknown["extra"] = json!(true);
    assert!(app::Headers::parse(&serde_json::to_vec(&unknown).unwrap()).is_err());
    assert_eq!(f.rows(), 0);
}
#[test]
fn exact_payload_delivery_job_and_output_budgets_admit_before_publication() {
    let f = Fixture::new();
    let raw = body('a', 'b', "opened");
    let h = headers(&raw, 1);
    for o in [
        app::Options {
            max_payload_bytes: raw.len() - 1,
            ..f.o.clone()
        },
        app::Options {
            max_deliveries: 0,
            ..f.o.clone()
        },
        app::Options {
            max_jobs: 0,
            ..f.o.clone()
        },
        app::Options {
            max_store_bytes: 0,
            ..f.o.clone()
        },
        app::Options {
            max_output_bytes: 0,
            ..f.o.clone()
        },
    ] {
        assert!(app::ingest(&f.store, &f.host, &h, &raw, &o, &f.d).is_err());
        assert_eq!(f.rows(), 0);
    }
    let exact = app::Options {
        max_payload_bytes: raw.len(),
        max_deliveries: 1,
        max_jobs: 1,
        ..f.o.clone()
    };
    let value = app::ingest(&f.store, &f.host, &h, &raw, &exact, &f.d).unwrap();
    let bytes = serde_json::to_vec(&value).unwrap().len() + 1;
    let repeated = app::ingest(
        &f.store,
        &f.host,
        &h,
        &raw,
        &app::Options {
            max_output_bytes: bytes,
            ..exact.clone()
        },
        &f.d,
    )
    .unwrap();
    let repeated_bytes = serde_json::to_vec(&repeated).unwrap().len() + 1;
    assert_eq!(
        app::ingest(
            &f.store,
            &f.host,
            &h,
            &raw,
            &app::Options {
                max_output_bytes: repeated_bytes,
                ..exact.clone()
            },
            &f.d
        )
        .unwrap(),
        repeated
    );
    assert!(
        app::ingest(
            &f.store,
            &f.host,
            &h,
            &raw,
            &app::Options {
                max_output_bytes: repeated_bytes - 1,
                ..exact
            },
            &f.d
        )
        .is_err()
    );
    assert_eq!(f.rows(), 1);
}
#[test]
fn permission_cancellation_and_caller_sql_state_are_preserved() {
    let f = Fixture::new();
    let raw = body('a', 'b', "opened");
    let h = headers(&raw, 1);
    let no = app::Options {
        allow_writes: false,
        ..f.o.clone()
    };
    assert!(app::ingest(&f.store, &f.host, &h, &raw, &no, &f.d).is_err());
    assert!(app::ingest(&f.store, &f.host, &h, &raw, &f.o, &Deadline::new(0)).is_err());
    let prior = f.store.db.limit(Limit::SQLITE_LIMIT_LENGTH).unwrap();
    f.store.db.execute_batch("BEGIN").unwrap();
    assert!(app::ingest(&f.store, &f.host, &h, &raw, &f.o, &f.d).is_err());
    assert!(!f.store.db.is_autocommit());
    assert_eq!(f.store.db.limit(Limit::SQLITE_LIMIT_LENGTH).unwrap(), prior);
    assert_eq!(f.queue()["total"], 0);
    assert!(!f.store.db.is_autocommit());
    f.store.db.execute_batch("ROLLBACK").unwrap();
    assert_eq!(f.rows(), 0);
}
#[test]
fn stored_unknown_fields_padded_rows_and_changed_inline_body_refuse() {
    for mutation in [0, 1, 2] {
        let f = Fixture::new();
        f.ingest(&body('a', 'b', "opened"), 1).unwrap();
        let (id, raw): (String, String) = f
            .store
            .db
            .query_row(
                "SELECT id,value FROM records WHERE kind=?1",
                [app::RECORD_KIND],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        let changed = match mutation {
            0 => {
                let mut v: Value = serde_json::from_str(&raw).unwrap();
                v["artifactId"] = json!("f".repeat(64));
                v.to_string()
            }
            1 => format!("{}{}", raw, " ".repeat(app::MAX_RECORD_BYTES)),
            _ => {
                let mut v: Value = serde_json::from_str(&raw).unwrap();
                v["event"]["payload"] = json!("{}");
                v.to_string()
            }
        };
        f.store
            .db
            .execute(
                "UPDATE records SET value=?1 WHERE kind=?2 AND id=?3",
                params![changed, app::RECORD_KIND, id],
            )
            .unwrap();
        assert!(app::queue(&f.store, &f.host, 0, 100, &f.o, &f.d).is_err());
        assert!(app::validated_inline_records(&f.store, &f.d).is_err());
    }
}
#[test]
fn retention_recognizes_only_closed_valid_inline_record_history() {
    let f = Fixture::new();
    f.ingest(&body('a', 'b', "opened"), 1).unwrap();
    assert_eq!(
        app::validated_inline_records(&f.store, &f.d).unwrap().len(),
        1
    );
    let plan = yoke::retention::purge_plan(&f.store, 0, &f.d).unwrap();
    assert!(
        !plan["unknownDependencyKinds"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == app::RECORD_KIND)
    );
}
#[test]
fn aggregate_store_exact_boundary_and_transition_zero_refuse_before_insert() {
    let raw = body('a', 'b', "opened");
    let probe = Fixture::new();
    probe.ingest(&raw, 1).unwrap();
    let stored_bytes: usize = probe
        .store
        .db
        .query_row(
            "SELECT sum(length(CAST(value AS BLOB))) FROM records WHERE kind=?1",
            [app::RECORD_KIND],
            |r| r.get(0),
        )
        .unwrap();
    let f = Fixture::new();
    let h = headers(&raw, 1);
    let short = app::Options {
        max_store_bytes: stored_bytes - 1,
        ..f.o.clone()
    };
    assert!(app::ingest(&f.store, &f.host, &h, &raw, &short, &f.d).is_err());
    assert_eq!(f.rows(), 0);
    let exact = app::Options {
        max_store_bytes: stored_bytes,
        ..f.o.clone()
    };
    let job = app::ingest(&f.store, &f.host, &h, &raw, &exact, &f.d).unwrap();
    assert_eq!(f.rows(), 1);
    let revision = f.queue()["jobs"][0]["groupRevision"].as_u64().unwrap() as usize;
    assert!(
        app::select(
            &f.store,
            &f.host,
            job["jobId"].as_str().unwrap(),
            revision,
            &app::Options {
                max_transitions: 0,
                ..f.o.clone()
            },
            &f.d
        )
        .is_err()
    );
    assert_eq!(f.rows(), 1);
    assert_eq!(f.queue()["jobs"][0]["status"], "queued");
}
#[test]
fn lower_caller_sql_limit_and_existing_read_transaction_survive_refusal() {
    let f = Fixture::new();
    f.ingest(&body('a', 'b', "opened"), 1).unwrap();
    let prior = f.store.db.limit(Limit::SQLITE_LIMIT_LENGTH).unwrap();
    f.store
        .db
        .set_limit(Limit::SQLITE_LIMIT_LENGTH, 256)
        .unwrap();
    f.store.db.execute_batch("BEGIN").unwrap();
    assert!(app::queue(&f.store, &f.host, 0, 100, &f.o, &f.d).is_err());
    assert!(!f.store.db.is_autocommit());
    assert_eq!(f.store.db.limit(Limit::SQLITE_LIMIT_LENGTH).unwrap(), 256);
    f.store.db.execute_batch("ROLLBACK").unwrap();
    f.store
        .db
        .set_limit(Limit::SQLITE_LIMIT_LENGTH, prior)
        .unwrap();
    assert_eq!(f.queue()["total"], 1);
}
#[test]
fn inherited_cancellation_refuses_preflight_and_publication_without_new_records() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let f = Fixture::new();
    let raw = body('a', 'b', "opened");
    let h = headers(&raw, 1);
    let cancel = Arc::new(AtomicBool::new(false));
    let result = Deadline::run_bounded(&f.d, cancel.clone(), || {
        let nested = Deadline::new(60_000);
        cancel.store(true, Ordering::SeqCst);
        assert!(
            f.host
                .preflight_delivery(&h, &raw, &f.o, &nested)
                .unwrap_err()
                .to_string()
                .contains("cancelled")
        );
        app::ingest(&f.store, &f.host, &h, &raw, &f.o, &nested)
    });
    assert!(result.unwrap_err().to_string().contains("cancelled"));
    assert_eq!(f.rows(), 0);
}
#[test]
fn keyless_retention_schema_proof_cannot_replace_keyed_authenticity() {
    let f = Fixture::new();
    f.ingest(&body('a', 'b', "opened"), 1).unwrap();
    let (id, raw): (String, String) = f
        .store
        .db
        .query_row(
            "SELECT id,value FROM records WHERE kind=?1",
            [app::RECORD_KIND],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    let mut record: Value = serde_json::from_str(&raw).unwrap();
    let mut payload: Value =
        serde_json::from_str(record["event"]["payload"].as_str().unwrap()).unwrap();
    payload["pull_request"]["title"] = json!("Reidentified but unauthenticated title");
    let changed = serde_json::to_vec(&payload).unwrap();
    record["event"]["payload"] = json!(String::from_utf8(changed.clone()).unwrap());
    record["event"]["payloadHash"] = json!(hash(&changed));
    let original: Value = serde_json::from_str(&raw).unwrap();
    let replaced = raw
        .replace(
            &serde_json::to_string(&original["event"]["payload"]).unwrap(),
            &serde_json::to_string(&record["event"]["payload"]).unwrap(),
        )
        .replace(
            &serde_json::to_string(&original["event"]["payloadHash"]).unwrap(),
            &serde_json::to_string(&record["event"]["payloadHash"]).unwrap(),
        );
    let reidentified = hash(replaced.as_bytes());
    f.store
        .db
        .execute(
            "UPDATE records SET id=?1,value=?2 WHERE kind=?3 AND id=?4",
            params![reidentified, replaced, app::RECORD_KIND, id],
        )
        .unwrap();
    assert!(
        app::queue(&f.store, &f.host, 0, 100, &f.o, &f.d)
            .unwrap_err()
            .to_string()
            .contains("signature")
    );
    assert_eq!(
        app::validated_inline_records(&f.store, &f.d).unwrap().len(),
        1
    );
}
#[test]
fn cancellation_at_actual_sql_publication_boundary_rolls_back_and_restores_limits() {
    use rusqlite::hooks::{AuthAction, AuthContext, Authorization};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let f = Fixture::new();
    let raw = body('a', 'b', "opened");
    let h = headers(&raw, 1);
    let cancel = Arc::new(AtomicBool::new(false));
    let observed = Arc::new(AtomicBool::new(false));
    let signal = cancel.clone();
    let tripwire = observed.clone();
    f.store.db.authorizer(Some(move |context: AuthContext<'_>| {
        if let AuthAction::Insert {
            table_name: "records",
        } = context.action
        {
            tripwire.store(true, Ordering::SeqCst);
            signal.store(true, Ordering::SeqCst);
        }
        Authorization::Allow
    }));
    let prior = f.store.db.limit(Limit::SQLITE_LIMIT_LENGTH).unwrap();
    let result = Deadline::run_bounded(&f.d, cancel, || {
        app::ingest(&f.store, &f.host, &h, &raw, &f.o, &Deadline::new(60_000))
    });
    f.store
        .db
        .authorizer(None::<fn(AuthContext<'_>) -> Authorization>);
    f.store.set_deadline(&f.d).unwrap();
    assert!(observed.load(Ordering::SeqCst));
    assert!(result.unwrap_err().to_string().contains("cancelled"));
    assert!(f.store.db.is_autocommit());
    assert_eq!(f.store.db.limit(Limit::SQLITE_LIMIT_LENGTH).unwrap(), prior);
    assert_eq!(f.rows(), 0);
}
#[test]
fn explicit_rotation_keys_replay_old_receipts_and_admit_new_signatures() {
    let f = Fixture::new();
    let first = body('a', 'b', "opened");
    f.ingest(&first, 1).unwrap();
    let profile=app::HostProfile::parse(&serde_json::to_vec(&json!({"schemaVersion":1,"appId":123,
        "secretCredential":"new","priorSecretCredentials":["previous"],"grants":[{"installationId":42,
        "repositoryId":9,"repository":"acme/demo"}]})).unwrap()).unwrap();
    let rotated = app::Host::with_signing_keys(
        profile,
        vec![
            app::SigningKey::from_host_bytes(b"another public test signing fixture").unwrap(),
            app::SigningKey::from_host_bytes(KEY).unwrap(),
        ],
    )
    .unwrap();
    assert_eq!(
        app::queue(&f.store, &rotated, 0, 100, &f.o, &f.d).unwrap()["total"],
        1
    );
    let next = body('c', 'b', "synchronize");
    app::ingest(
        &f.store,
        &rotated,
        &headers_with_key(&next, 2, b"another public test signing fixture"),
        &next,
        &f.o,
        &f.d,
    )
    .unwrap();
    assert_eq!(
        app::queue(&f.store, &rotated, 0, 100, &f.o, &f.d).unwrap()["total"],
        2
    );
    assert!(app::queue(&f.store, &f.host, 0, 100, &f.o, &f.d).is_err());
}
#[test]
fn app_identity_and_current_explicit_grants_bound_visible_and_selected_jobs() {
    let f = Fixture::new();
    let raw = body('a', 'b', "opened");
    let first = f.ingest(&raw, 1).unwrap();
    let make_host = |app_id: u64, repository_id: u64| {
        let profile=app::HostProfile::parse(&serde_json::to_vec(&json!({"schemaVersion":1,"appId":app_id,
            "secretCredential":"in-memory","grants":[{"installationId":42,"repositoryId":repository_id,
            "repository":"acme/demo"}]})).unwrap()).unwrap();
        app::Host::with_signing_key(profile, app::SigningKey::from_host_bytes(KEY).unwrap())
            .unwrap()
    };
    let other = make_host(124, 9);
    let second = app::ingest(&f.store, &other, &headers(&raw, 1), &raw, &f.o, &f.d).unwrap();
    assert_ne!(first["jobId"], second["jobId"]);
    assert_eq!(f.rows(), 2);
    assert_eq!(f.queue()["total"], 1);
    assert_eq!(
        app::queue(&f.store, &other, 0, 100, &f.o, &f.d).unwrap()["total"],
        1
    );
    let revoked = make_host(123, 10);
    assert_eq!(
        app::queue(&f.store, &revoked, 0, 100, &f.o, &f.d).unwrap()["total"],
        0
    );
    assert!(
        app::select(
            &f.store,
            &revoked,
            first["jobId"].as_str().unwrap(),
            1,
            &f.o,
            &f.d
        )
        .is_err()
    );
    assert!(app::ingest(&f.store, &revoked, &headers(&raw, 2), &raw, &f.o, &f.d).is_err());
    assert_eq!(f.rows(), 2);
}
