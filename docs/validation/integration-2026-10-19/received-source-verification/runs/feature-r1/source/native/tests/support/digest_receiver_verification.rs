//! Receiver intake plus independent captured-source replay; no model/credentials.
use super::*;
use receiver::verification as verify;
struct Received {
    sender: Source,
    root: tempfile::TempDir,
    profile: receiver::Profile,
    proposal: String,
    receipt: String,
}
impl Received {
    fn new(change: impl FnOnce(&mut Value)) -> Self {
        let sender = Source::new();
        let (root, h) = query_start(&sender, true, true);
        let mut profile = sender.receiver_profile();
        profile.endpoint = h.endpoint().into();
        profile.grants[0].allow_query = true;
        let mut wire = sender.proposal(&sender.client_profile(h.endpoint()))["body"].clone();
        change(&mut wire["digest"]["payload"]);
        wire["digest"]["id"] = json!(hash(
            yoke::json::encode_bounded(
                &wire["digest"]["payload"],
                4_000_000,
                &Deadline::new(20000)
            )
            .unwrap()
        ));
        let raw = serde_json::to_vec(&wire).unwrap();
        let proposal = identity(
            &json!({"schemaVersion":1,"workspaceId":sender.engine.store.workspace_id,"appId":123,"endpoint":h.endpoint(),"payloadHash":hash(&raw)}),
        );
        let (status, receipt) = request(&h, "PUT", &proposal, &TOKEN, &raw);
        assert_eq!(status, 201);
        h.shutdown().unwrap();
        Self {
            sender,
            root,
            profile,
            proposal,
            receipt: receipt["receiptId"].as_str().unwrap().into(),
        }
    }
    fn request(&self, snapshot: &str) -> verify::Request {
        verify::Request {
            schema_version: 1,
            app_id: 123,
            sender_workspace_id: self.sender.engine.store.workspace_id.clone(),
            installation_id: 42,
            repository_id: 9,
            repository: "org/repo".into(),
            proposal_id: self.proposal.clone(),
            receipt_id: self.receipt.clone(),
            local_snapshot_id: snapshot.into(),
        }
    }
    fn local(&self) -> (tempfile::TempDir, Engine, String) {
        let root = tempfile::tempdir().unwrap();
        std::fs::copy(
            self.sender.root.path().join("main.rs"),
            root.path().join("main.rs"),
        )
        .unwrap();
        let mut engine = Engine::open(root.path()).unwrap();
        engine.capture_epoch = self.sender.engine.capture_epoch;
        let snapshot = engine.index(&Deadline::new(20000)).unwrap().0.id;
        (root, engine, snapshot)
    }
}
fn options() -> verify::Options {
    verify::Options {
        allow_query: true,
        allow_verification: true,
        ..Default::default()
    }
}
fn run(
    f: &Received,
    source: &Store,
    snapshot: &str,
    o: &verify::Options,
) -> anyhow::Result<verify::CheckedDigest> {
    let d = Deadline::new(20000);
    let receiver = Store::open_existing_read_only(f.root.path(), &d).unwrap();
    verify::verify(
        &verify::Context {
            receiver_store: &receiver,
            receiver_profile: &f.profile,
            principal: "sender",
            source_store: source,
            source_profile: &f.sender.app,
            source_options: &f.sender.options,
            options: o,
        },
        &f.request(snapshot),
        &d,
    )
}
#[test]
fn separate_workspace_source_replay_qualifies_each_ordinal_without_promoting_packet_claims() {
    let f = Received::new(|_| {});
    let (_root, engine, id) = f.local();
    let before = count(f.root.path());
    assert_ne!(
        engine.store.workspace_id,
        f.sender.engine.store.workspace_id
    );
    assert_ne!(id, f.sender.request.snapshot_id);
    let checked = run(&f, &engine.store, &id, &options()).unwrap();
    let v = checked.export(4_000_000, &Deadline::new(20000)).unwrap();
    assert_eq!(v["verifiedSymbols"], 1);
    assert_eq!(v["advertisedRustProjectionChecked"], true);
    assert_eq!(v["rows"][0]["localSource"]["snapshotId"], id);
    assert_eq!(v["rows"][0]["receivedMetadata"]["name"], "café");
    assert_eq!(
        v["rows"][0]["localSource"]["fingerprint"],
        v["rows"][0]["receivedMetadata"]["fingerprint"]
    );
    assert_eq!(
        v["rows"][0]["localSource"]["contentHash"],
        hash(std::fs::read(f.sender.root.path().join("main.rs")).unwrap())
    );
    for key in [
        "sourceAuthority",
        "complete",
        "senderSourceDigestHashVerified",
        "senderSnapshotIdentityEstablished",
        "remoteRepositoryAttested",
        "discussionsVerified",
        "historicalCommitsVerified",
    ] {
        assert_eq!(v[key], false, "{key}");
    }
    assert_eq!(v["networkRequests"], 0);
    assert_eq!(v["modelCalls"], 0);
    assert_eq!(count(f.root.path()), before);
    let q = query_request(&f.sender, vec![f.proposal.clone()]);
    let d = Deadline::new(20000);
    let receiver = Store::open_existing_read_only(f.root.path(), &d).unwrap();
    let unchanged = query::local(
        &receiver,
        &f.profile,
        "sender",
        &q,
        &query::Options {
            allow_query: true,
            ..Default::default()
        },
        &d,
    )
    .unwrap();
    assert_eq!(unchanged["sourceAuthority"], false);
    assert_eq!(unchanged["rows"][0]["sourceAuthority"], false);
}
#[test]
fn frozen_literal_queries_survive_store_loss_and_remain_bounded() {
    let f = Received::new(|_| {});
    let (root, engine, id) = f.local();
    let checked = run(&f, &engine.store, &id, &options()).unwrap();
    drop(engine);
    std::fs::rename(
        root.path().join(".yoke"),
        root.path().join("offline-source"),
    )
    .unwrap();
    let d = Deadline::new(20000);
    let found = checked.find("café", 4_000_000, &d).unwrap();
    assert_eq!(found["rows"].as_array().unwrap().len(), 1);
    assert_eq!(found["rows"][0]["ordinal"], 0);
    assert_eq!(found["rows"][0]["pointer"], "/digest/payload/symbols/0");
    let absent = checked.find("CAFÉ", 4_000_000, &d).unwrap();
    assert!(absent["rows"].as_array().unwrap().is_empty());
    assert_eq!(absent["query"]["completeForLiteralSelection"], true);
    assert_eq!(absent["complete"], false);
    assert!(checked.find("", 4096, &d).is_err());
    assert!(checked.find("x\n", 4096, &d).is_err());
    assert!(checked.export(1024, &d).is_err());
    assert!(checked.export(4_000_000, &Deadline::new(0)).is_err());
}
#[test]
fn rehashed_sender_declarations_counters_and_duplicate_rows_cannot_pass_source_verification() {
    for mutation in 0..7 {
        let f = Received::new(|p| match mutation {
            0 => p["symbols"][0]["path"] = json!("../../private.rs"),
            1 => p["symbols"][0]["signature"] = json!("pub fn forged()"),
            2 => p["symbols"][0]["fingerprint"] = json!("f".repeat(64)),
            3 => p["symbols"][0]["startByte"] = json!(1),
            4 => p["symbolCandidates"] = json!(2),
            5 => p["symbols"] = json!([p["symbols"][0].clone(), p["symbols"][0].clone()]),
            _ => p["symbols"][0]["sourceHash"] = json!("e".repeat(64)),
        });
        let (_root, engine, id) = f.local();
        let before = count(f.root.path());
        let error = run(&f, &engine.store, &id, &options())
            .err()
            .expect("forged projection accepted");
        assert_eq!(
            error.to_string(),
            "github_app_digest_verification_mismatch",
            "{mutation}"
        );
        assert_eq!(count(f.root.path()), before);
    }
}
#[test]
fn real_source_change_or_graph_tamper_invalidates_the_comparison() {
    let f = Received::new(|_| {});
    let (root, mut engine, id) = f.local();
    let original = std::fs::read_to_string(root.path().join("main.rs")).unwrap();
    std::fs::write(
        root.path().join("main.rs"),
        original.replace("123456789", "987654321"),
    )
    .unwrap();
    let changed = engine.index(&Deadline::new(20000)).unwrap().0.id;
    assert_eq!(
        run(&f, &engine.store, &changed, &options())
            .err()
            .unwrap()
            .to_string(),
        "github_app_digest_verification_mismatch"
    );
    let mut graph: Value = engine
        .store
        .db
        .query_row("SELECT value FROM graphs WHERE id=?1", [&id], |r| {
            let raw: String = r.get(0)?;
            Ok(serde_json::from_str(&raw).unwrap())
        })
        .unwrap();
    let declaration = graph["entities"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entity| entity["kind"] == "function")
        .unwrap();
    declaration["name"] = json!("tampered");
    engine
        .store
        .db
        .execute(
            "UPDATE graphs SET value=?1 WHERE id=?2",
            rusqlite::params![graph.to_string(), id],
        )
        .unwrap();
    assert!(run(&f, &engine.store, &id, &options()).is_err());
}
#[test]
fn grants_permissions_receipt_pin_and_output_budget_refuse_without_mutating_intake() {
    let mut f = Received::new(|_| {});
    let (_root, engine, id) = f.local();
    let before = count(f.root.path());
    assert!(run(&f, &engine.store, &id, &verify::Options::default()).is_err());
    let mut o = options();
    o.allow_query = false;
    assert!(run(&f, &engine.store, &id, &o).is_err());
    o = options();
    o.max_output_bytes = 1024;
    assert!(run(&f, &engine.store, &id, &o).is_err());
    o = options();
    o.require_git_identity = true;
    assert_eq!(
        run(&f, &engine.store, &id, &o).err().unwrap().to_string(),
        "github_app_digest_verification_git_identity_required"
    );
    f.receipt = "f".repeat(64);
    assert_eq!(
        run(&f, &engine.store, &id, &options())
            .err()
            .unwrap()
            .to_string(),
        "github_app_digest_verification_mismatch"
    );
    f.profile.grants[0].allow_query = false;
    assert_eq!(
        run(&f, &engine.store, &id, &options())
            .err()
            .unwrap()
            .to_string(),
        "github_app_digest_verification_permission_required"
    );
    assert_eq!(count(f.root.path()), before);
}
#[test]
fn unknown_request_fields_duplicate_keys_and_foreign_ids_never_grant_source_access() {
    let f = Received::new(|_| {});
    let (_root, engine, id) = f.local();
    let mut r = f.request(&id);
    let good = serde_json::to_string(&r).unwrap();
    assert!(verify::Request::parse(good.as_bytes()).is_ok());
    let mut unknown = serde_json::to_value(&r).unwrap();
    unknown["allowVerification"] = json!(true);
    let mut oversized = serde_json::to_value(&r).unwrap();
    oversized["localSnapshotId"] = json!("a".repeat(8192));
    for raw in [
        unknown.to_string(),
        oversized.to_string(),
        format!("{{\"schemaVersion\":1,{}", good.strip_prefix('{').unwrap()),
    ] {
        assert!(verify::Request::parse(raw.as_bytes()).is_err());
    }
    r.sender_workspace_id = engine.store.workspace_id.clone();
    let d = Deadline::new(20000);
    assert_eq!(
        verify::admit(&f.profile, "sender", &f.sender.app, &r, &options(), &d)
            .unwrap_err()
            .to_string(),
        "github_app_digest_verification_permission_required"
    );
}
