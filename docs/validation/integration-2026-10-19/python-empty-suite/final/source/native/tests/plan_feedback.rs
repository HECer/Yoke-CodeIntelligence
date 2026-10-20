//! Actual saved native plans and immutable Unicode/CRLF sources; no execution.
use serde_json::{Value, json};
use std::{fs, process::Command};
use yoke::{
    Engine,
    capture::Deadline,
    changes, graph_view,
    knowledge::feedback::{
        Options, Outcome, SourceQuote, ViewPin,
        plans::{self, Draft, Evidence, RankInput, Side, Target},
    },
    model::hash,
};

const BEFORE: &str = "pub fn original(){ /* café 🦀 */ }\r\n";
const AFTER: &str = "pub fn reviewed(){ /* café 🦀 */ }\r\n";
fn d() -> Deadline {
    Deadline::new(60_000)
}
fn pin(view: &graph_view::View) -> ViewPin {
    ViewPin {
        workspace_id: view.workspace_id.clone(),
        snapshot_id: view.snapshot_id.clone(),
        view_id: view.id.clone(),
        view_artifact_id: view.artifact_id.clone(),
        base_graph_hash: view.base_graph_hash.clone(),
    }
}
struct Fixture {
    root: tempfile::TempDir,
    engine: Engine,
    plan: changes::Plan,
    target: Target,
}
fn fixture() -> Fixture {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("café.rs"), BEFORE).unwrap();
    fs::write(root.path().join("unchanged.rs"), "pub fn untouched() {}\n").unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let (snapshot, graph, _, _) = engine.index(&d()).unwrap();
    let before =
        graph_view::build(&engine.store, &snapshot, &graph, &Default::default(), &d()).unwrap();
    let plan = changes::prepare(
        &mut engine.store,
        &snapshot,
        &graph,
        &json!({"edits":[{"kind":"replace","path":"café.rs","expectedHash":hash(BEFORE),
            "startByte":0,"endByte":BEFORE.len(),"text":AFTER}]}),
        &d(),
    )
    .unwrap();
    let after = graph_view::build(
        &engine.store,
        &engine.store.snapshot(&plan.result_snapshot_id).unwrap(),
        &engine.store.graph(&plan.result_snapshot_id).unwrap(),
        &Default::default(),
        &d(),
    )
    .unwrap();
    let target = Target {
        plan_id: plan.id.clone(),
        origin: pin(&before),
        evaluated: pin(&after),
    };
    Fixture {
        root,
        engine,
        plan,
        target,
    }
}
fn draft(f: &Fixture, sequence: u32, time: u64, outcome: Outcome) -> Draft {
    Draft {
        schema_version: 1,
        target: f.target.clone(),
        actor: "Ada".into(),
        session: "s1".into(),
        sequence,
        occurred_at_ms: time,
        outcome,
        rationale: "Authored review of planned bytes.".into(),
        source_evidence: vec![Evidence {
            side: Side::Evaluated,
            source: SourceQuote {
                path: "café.rs".into(),
                content_hash: hash(AFTER),
                start_byte: 0,
                end_byte: AFTER.len(),
                quote: AFTER.into(),
            },
        }],
    }
}
fn query(f: &Fixture, now: u64) -> RankInput {
    RankInput {
        schema_version: 1,
        targets: vec![f.target.clone()],
        as_of_ms: now,
        decay_window_days: 1,
        limit: 20,
    }
}
fn count(f: &Fixture) -> usize {
    f.engine
        .store
        .db
        .query_row(
            "SELECT count(*) FROM records WHERE kind=?1",
            [plans::EVENT_KIND],
            |r| r.get(0),
        )
        .unwrap()
}
fn record(f: &Fixture, draft: &Draft) -> anyhow::Result<Value> {
    plans::record(&f.engine.store, draft, &Options::default(), &d())
}
fn rank(f: &Fixture, query: &RankInput) -> anyhow::Result<Value> {
    plans::rank(&f.engine.store, query, &Options::default(), &d())
}
#[test]
fn planned_result_views_replay_exact_unicode_crlf_without_application_or_execution() {
    let f = fixture();
    let draft = draft(&f, 1, 1000, Outcome::Success);
    let first = record(&f, &draft).unwrap();
    assert_eq!(first["inserted"], true);
    assert_eq!(
        first["event"]["draft"]["sourceEvidence"][0]["source"]["quote"],
        AFTER
    );
    assert_eq!(first["plan"]["id"], f.plan.id);
    for field in [
        "executionObserved",
        "workspaceApplicationObserved",
        "independentCorroborationEstablished",
        "sourceAuthority",
        "complete",
    ] {
        assert_eq!(first[field], false, "{field}");
    }
    let scopes = first["sourceProofs"][0]["evaluated"]["sourceScopes"]
        .as_array()
        .unwrap();
    let primary = scopes
        .iter()
        .find(|scope| scope["role"] == "primary_evidence")
        .unwrap();
    assert_eq!(primary["kind"], "derived");
    assert_eq!(primary["snapshotId"], f.target.evaluated.snapshot_id);
    assert!(
        scopes
            .iter()
            .filter(|scope| scope["snapshotId"] != f.target.evaluated.snapshot_id)
            .all(|scope| scope["role"] == "receipt_replay_only")
    );
    assert_eq!(record(&f, &draft).unwrap()["inserted"], false);
    assert_eq!(count(&f), 1);
    assert_eq!(
        rank(&f, &query(&f, 1000)).unwrap()["items"][0]["score"],
        1000
    );
    assert_eq!(
        fs::read(f.root.path().join("café.rs")).unwrap(),
        BEFORE.as_bytes()
    );
}

type StoreState = (Vec<(String, String, String)>, Vec<(String, String)>);
fn state(f: &Fixture) -> StoreState {
    let rows = f
        .engine
        .store
        .db
        .prepare("SELECT kind,id,value FROM records ORDER BY kind,id")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    let mut blobs: Vec<_> = fs::read_dir(f.engine.store.dir.join("blobs"))
        .unwrap()
        .map(|e| {
            let p = e.unwrap().path();
            (
                p.file_name().unwrap().to_string_lossy().into_owned(),
                hash(fs::read(p).unwrap()),
            )
        })
        .collect();
    blobs.sort();
    (rows, blobs)
}
#[test]
fn ordered_session_history_is_idempotent_and_cannot_rewrite_or_backdate_a_slot() {
    let f = fixture();
    let first = draft(&f, 1, 1000, Outcome::Success);
    record(&f, &first).unwrap();
    let before = state(&f);
    let mut conflict = first.clone();
    conflict.outcome = Outcome::Failure;
    assert_eq!(
        record(&f, &conflict).unwrap_err().to_string(),
        "feedback_sequence_conflict"
    );
    let earlier = draft(&f, 2, 999, Outcome::Failure);
    assert_eq!(
        record(&f, &earlier).unwrap_err().to_string(),
        "feedback_sequence_conflict"
    );
    assert_eq!(state(&f), before);
    record(&f, &draft(&f, 3, 3000, Outcome::Failure)).unwrap();
    record(&f, &draft(&f, 2, 2000, Outcome::Inconclusive)).unwrap();
    let mut invalid = draft(&f, 4, 4000, Outcome::Success);
    invalid.sequence = 0;
    let before = state(&f);
    assert!(record(&f, &invalid).is_err());
    assert_eq!(state(&f), before);
    assert_eq!(count(&f), 3);
}
#[test]
fn ranking_uses_latest_eligible_session_and_independently_expected_integer_decay() {
    let f = fixture();
    record(&f, &draft(&f, 1, 0, Outcome::Success)).unwrap();
    record(&f, &draft(&f, 2, 86_400_000, Outcome::Failure)).unwrap();
    let mut other = draft(&f, 1, 86_400_000, Outcome::Corroboration);
    other.actor = "Bob".into();
    record(&f, &other).unwrap();
    let old = rank(&f, &query(&f, 43_200_000)).unwrap();
    assert_eq!(old["items"][0]["score"], 666);
    assert_eq!(old["items"][0]["votingSessions"], 1);
    assert_eq!(
        old["items"][0]["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|v| v["disposition"] == "future")
            .count(),
        2
    );
    let current = rank(&f, &query(&f, 172_800_000)).unwrap();
    assert_eq!(current["items"][0]["score"], 0);
    assert_eq!(current["items"][0]["votingSessions"], 2);
    let votes = current["items"][0]["events"].as_array().unwrap();
    assert_eq!(
        votes
            .iter()
            .filter(|v| v["disposition"] == "counted")
            .map(|v| v["decayedWeight"].as_i64().unwrap())
            .collect::<Vec<_>>(),
        vec![500, 500]
    );
    assert_eq!(
        votes
            .iter()
            .filter(|v| v["disposition"] == "superseded_in_session")
            .count(),
        1
    );
    assert_eq!(
        rank(&f, &query(&f, u64::MAX)).unwrap()["items"][0]["score"],
        0
    );
    assert_eq!(current["independentCorroborationEstablished"], false);
}
#[test]
fn wrong_plan_views_paths_hashes_utf8_boundaries_and_unchanged_quotes_refuse_without_writes() {
    let f = fixture();
    let good = draft(&f, 1, 1000, Outcome::Success);
    let before = state(&f);
    for case in 0..11 {
        let mut bad = good.clone();
        let q = &mut bad.source_evidence[0].source;
        match case {
            0 => bad.target.plan_id = "7".repeat(64),
            1 => bad.target.evaluated = bad.target.origin.clone(),
            2 => bad.target.origin.workspace_id = "7".repeat(64),
            3 => q.path = "../café.rs".into(),
            4 => q.path = "C:/outside.rs".into(),
            5 => q.content_hash = hash(BEFORE),
            6 => {
                q.quote = BEFORE.into();
                q.end_byte = BEFORE.len();
            }
            7 => {
                q.start_byte = AFTER.find('é').unwrap() + 1;
                q.end_byte = q.start_byte + 1;
                q.quote = "x".into();
            }
            8 => {
                q.path = "unchanged.rs".into();
                q.content_hash = hash("pub fn untouched() {}\n");
                q.start_byte = 0;
                q.quote = "pub".into();
                q.end_byte = 3;
            }
            9 => bad.source_evidence[0].side = Side::Origin,
            10 => bad.source_evidence.push(bad.source_evidence[0].clone()),
            _ => unreachable!(),
        }
        assert!(record(&f, &bad).is_err(), "case {case}");
        assert_eq!(state(&f), before, "case {case}");
    }
}
#[test]
fn every_budget_refuses_the_whole_publication_and_rank_without_mutation() {
    let f = fixture();
    let draft = draft(&f, 1, 1000, Outcome::Success);
    let before = state(&f);
    for case in 0..5 {
        let mut options = Options::default();
        match case {
            0 => options.max_events = 0,
            1 => options.max_input_bytes = 0,
            2 => options.max_source_bytes = 0,
            3 => options.max_pairs = 0,
            4 => options.max_output_bytes = 0,
            _ => unreachable!(),
        }
        assert_eq!(
            plans::record(&f.engine.store, &draft, &options, &d())
                .unwrap_err()
                .to_string(),
            "feedback_capacity_exceeded",
            "case {case}"
        );
        assert_eq!(state(&f), before);
    }
    record(&f, &draft).unwrap();
    let before = state(&f);
    for case in 0..6 {
        let mut options = Options::default();
        match case {
            0 => options.max_events = 0,
            1 => options.max_input_bytes = 0,
            2 => options.max_source_bytes = 0,
            3 => options.max_pairs = 0,
            4 => options.max_output_bytes = 0,
            5 => options.max_candidates = 0,
            _ => unreachable!(),
        }
        assert_eq!(
            plans::rank(&f.engine.store, &query(&f, 1000), &options, &d())
                .unwrap_err()
                .to_string(),
            "feedback_capacity_exceeded",
            "case {case}"
        );
        assert_eq!(state(&f), before);
    }
}
#[test]
fn altered_saved_plan_and_missing_source_cannot_reuse_an_event() {
    let f = fixture();
    record(&f, &draft(&f, 1, 1000, Outcome::Success)).unwrap();
    for case in 0..4 {
        let mut changed = f.plan.clone();
        match case {
            0 => changed.diff = "forged review".into(),
            1 => changed.expires_at += 1,
            2 => changed.files[0].after_hash = Some(hash(BEFORE)),
            3 => changed.workspace_id = "7".repeat(64),
            _ => unreachable!(),
        }
        f.engine.store.record("plan", &f.plan.id, &changed).unwrap();
        let before = state(&f);
        assert!(rank(&f, &query(&f, 1000)).is_err());
        assert!(record(&f, &draft(&f, 2, 2000, Outcome::Success)).is_err());
        assert_eq!(state(&f), before);
    }
    f.engine.store.record("plan", &f.plan.id, &f.plan).unwrap();
    let source = f.engine.store.dir.join("blobs").join(hash(AFTER));
    let bytes = fs::read(&source).unwrap();
    fs::remove_file(&source).unwrap();
    let before = state(&f);
    assert!(rank(&f, &query(&f, 1000)).is_err());
    assert_eq!(state(&f), before);
    fs::write(source, bytes).unwrap();
    assert_eq!(
        rank(&f, &query(&f, 1000)).unwrap()["items"][0]["score"],
        1000
    );
}
#[test]
fn forged_verification_and_approval_rows_do_not_become_outcome_authority() {
    let f = fixture();
    f.engine
        .store
        .record(
            "verification",
            &f.plan.id,
            &json!({"status":"passed","level":"executed",
        "planId":f.plan.id,"snapshotId":f.plan.result_snapshot_id}),
        )
        .unwrap();
    f.engine
        .store
        .record(
            "approval",
            &f.plan.id,
            &json!({"plan_id":f.plan.id,"approved_at":1,"allow_test_changes":true}),
        )
        .unwrap();
    record(&f, &draft(&f, 1, 1000, Outcome::Success)).unwrap();
    let before = state(&f);
    let out = rank(&f, &query(&f, 1000)).unwrap();
    assert_eq!(out["executionObserved"], false);
    assert_eq!(out["workspaceApplicationObserved"], false);
    assert_eq!(out["currentApplicabilityEstablished"], false);
    assert_eq!(state(&f), before);
    assert_eq!(
        fs::read(f.root.path().join("café.rs")).unwrap(),
        BEFORE.as_bytes()
    );
}
#[test]
fn existing_read_transaction_is_preserved_and_cannot_receive_a_feedback_write() {
    let f = fixture();
    record(&f, &draft(&f, 1, 1000, Outcome::Success)).unwrap();
    f.engine.store.db.execute_batch("BEGIN DEFERRED").unwrap();
    let before = state(&f);
    assert_eq!(
        record(&f, &draft(&f, 2, 2000, Outcome::Failure))
            .unwrap_err()
            .to_string(),
        "feedback_transaction_unavailable"
    );
    assert_eq!(
        rank(&f, &query(&f, 1000)).unwrap()["items"][0]["score"],
        1000
    );
    assert!(!f.engine.store.db.is_autocommit());
    assert_eq!(state(&f), before);
    f.engine.store.db.execute_batch("ROLLBACK").unwrap();
}
#[test]
fn create_delete_plan_quotes_keep_the_original_side_and_byte_identity() {
    let mut f = fixture();
    let base = f.engine.store.snapshot(&f.plan.base_snapshot_id).unwrap();
    let graph = f.engine.store.graph(&base.id).unwrap();
    let plan = changes::prepare(
        &mut f.engine.store,
        &base,
        &graph,
        &json!({"edits":[
        {"kind":"delete","path":"café.rs","expectedHash":hash(BEFORE)},
        {"kind":"create","path":"new.rs","text":AFTER}]}),
        &d(),
    )
    .unwrap();
    let after = graph_view::build(
        &f.engine.store,
        &f.engine.store.snapshot(&plan.result_snapshot_id).unwrap(),
        &f.engine.store.graph(&plan.result_snapshot_id).unwrap(),
        &Default::default(),
        &d(),
    )
    .unwrap();
    f.target.plan_id = plan.id.clone();
    f.target.evaluated = pin(&after);
    f.plan = plan;
    let mut event = draft(&f, 1, 1000, Outcome::Success);
    event.source_evidence[0].source.path = "new.rs".into();
    event.source_evidence.push(Evidence {
        side: Side::Origin,
        source: SourceQuote {
            path: "café.rs".into(),
            content_hash: hash(BEFORE),
            start_byte: 0,
            end_byte: BEFORE.len(),
            quote: BEFORE.into(),
        },
    });
    let out = record(&f, &event).unwrap();
    assert_eq!(
        out["event"]["draft"]["sourceEvidence"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    event.sequence = 2;
    event.source_evidence[0].side = Side::Origin;
    let before = state(&f);
    assert!(record(&f, &event).is_err());
    assert_eq!(state(&f), before);
}
fn command(root: &std::path::Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_yoke"));
    command
        .arg("--root")
        .arg(root)
        .args(["--timeout", "60000", "--max-bytes", "4000000"]);
    command
}
fn host(f: &Fixture, name: &str, value: &impl serde::Serialize) -> std::path::PathBuf {
    // Host input is outside the analyzed root so fresh source never participates.
    let path = f.root.path().parent().unwrap().join(format!(
        "{}-{name}",
        f.root.path().file_name().unwrap().to_str().unwrap()
    ));
    fs::write(&path, serde_json::to_vec(value).unwrap()).unwrap();
    path
}
#[test]
fn cli_record_rank_and_closed_stdout_keep_one_immutable_recoverable_event() {
    let f = fixture();
    let input = host(&f, "event.json", &draft(&f, 1, 1000, Outcome::Success));
    let denied = command(f.root.path())
        .args(["feedback", "plan-record", "--input"])
        .arg(&input)
        .output()
        .unwrap();
    assert!(!denied.status.success());
    assert!(denied.stdout.is_empty());
    assert_eq!(count(&f), 0);
    let mut child = command(f.root.path())
        .args(["--allow-writes", "feedback", "plan-record", "--input"])
        .arg(&input)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    assert!(!child.wait().unwrap().success());
    assert_eq!(count(&f), 1);
    let out = command(f.root.path())
        .args(["--allow-writes", "feedback", "plan-record", "--input"])
        .arg(&input)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.stdout.ends_with(b"\n"));
    let saved: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(saved["inserted"], false);
    assert_eq!(saved["id"], saved["event"]["id"]);
    let query = host(&f, "query.json", &query(&f, 1000));
    let before = state(&f);
    let out = command(f.root.path())
        .args(["feedback", "plan-rank", "--input"])
        .arg(&query)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&out.stdout).unwrap()["items"][0]["score"],
        1000
    );
    assert_eq!(state(&f), before);
    fs::remove_file(input).unwrap();
    fs::remove_file(query).unwrap();
}
#[test]
fn host_json_protocol_refusals_precede_store_creation() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("absent");
    let f = fixture();
    let mut value = serde_json::to_value(draft(&f, 1, 1000, Outcome::Success)).unwrap();
    let valid = serde_json::to_string(&value).unwrap();
    value["allowExecution"] = json!(true);
    let invalid = [
        serde_json::to_string(&value).unwrap(),
        format!("{{\"schemaVersion\":1,{}", &valid[1..]),
        "{}".into(),
    ];
    for (i, body) in invalid.iter().enumerate() {
        let input = temp.path().join(format!("invalid-{i}.json"));
        fs::write(&input, body).unwrap();
        let out = command(&root)
            .args(["--allow-writes", "feedback", "plan-record", "--input"])
            .arg(input)
            .output()
            .unwrap();
        assert!(!out.status.success());
        assert!(out.stdout.is_empty());
        assert!(!root.exists());
    }
    let input = temp.path().join("empty.json");
    fs::write(&input, b"{}").unwrap();
    let out = command(&root)
        .args(["--allow-writes", "feedback", "plan-rank", "--input"])
        .arg(&input)
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    assert!(!root.exists());
}
#[test]
fn retention_protects_original_plan_sources_and_refuses_a_forged_event() {
    let f = fixture();
    let out = record(&f, &draft(&f, 1, 1000, Outcome::Success)).unwrap();
    let plan = yoke::retention::plan(&f.engine.store, &Default::default(), &d()).unwrap();
    assert_eq!(plan["unknownDependencyKinds"], json!([]));
    let orphan = f.engine.store.blob(b"orphan plan feedback bytes").unwrap();
    yoke::maintenance::collect(
        &f.engine.store,
        &yoke::maintenance::Policy {
            apply: true,
            minimum_age_ms: 0,
            purge_quarantine: false,
        },
        &d(),
    )
    .unwrap();
    assert!(
        f.engine
            .store
            .dir
            .join("blobs")
            .join(hash(BEFORE))
            .is_file()
    );
    assert!(f.engine.store.dir.join("blobs").join(hash(AFTER)).is_file());
    assert!(!f.engine.store.dir.join("blobs").join(orphan).exists());
    assert_eq!(
        rank(&f, &query(&f, 1000)).unwrap()["items"][0]["score"],
        1000
    );
    let id = out["id"].as_str().unwrap();
    let mut forged = out["event"].clone();
    forged["draft"]["outcome"] = json!("failure");
    f.engine
        .store
        .record(plans::EVENT_KIND, id, &forged)
        .unwrap();
    let before = state(&f);
    assert!(yoke::retention::plan(&f.engine.store, &Default::default(), &d()).is_err());
    assert!(rank(&f, &query(&f, 1000)).is_err());
    assert_eq!(state(&f), before);
}

#[test]
fn target_selection_reports_outside_events_and_does_not_duplicate_a_plan_vote() {
    let mut f = fixture();
    let original = f.target.clone();
    record(&f, &draft(&f, 1, 1000, Outcome::Success)).unwrap();
    let base = f.engine.store.snapshot(&f.plan.base_snapshot_id).unwrap();
    let graph = f.engine.store.graph(&base.id).unwrap();
    let other = changes::prepare(
        &mut f.engine.store,
        &base,
        &graph,
        &json!({"edits":[{"kind":"create","path":"other.rs","text":"pub fn second() {}\n"}]}),
        &d(),
    )
    .unwrap();
    let view = graph_view::build(
        &f.engine.store,
        &f.engine.store.snapshot(&other.result_snapshot_id).unwrap(),
        &f.engine.store.graph(&other.result_snapshot_id).unwrap(),
        &Default::default(),
        &d(),
    )
    .unwrap();
    let other_target = Target {
        plan_id: other.id,
        origin: original.origin.clone(),
        evaluated: pin(&view),
    };
    let mut query = query(&f, 1000);
    query.targets = vec![other_target.clone()];
    let before = state(&f);
    let out = rank(&f, &query).unwrap();
    assert_eq!(out["eventsOutsideSelectedTargets"], 1);
    assert_eq!(out["items"][0]["score"], 0);
    assert_eq!(out["items"][0]["votingSessions"], 0);
    query.targets = vec![other_target, original.clone()];
    query.limit = 1;
    let out = rank(&f, &query).unwrap();
    assert_eq!(out["items"][0]["target"]["planId"], original.plan_id);
    assert_eq!(out["selectedTargets"], 2);
    assert_eq!(out["notReturnedByLimit"], 1);
    query.targets = vec![original.clone(), original];
    assert!(rank(&f, &query).is_err());
    assert_eq!(state(&f), before);
}

#[test]
fn oversized_plan_registration_and_expired_deadline_publish_nothing_and_restore_sql_limits() {
    use rusqlite::limits::Limit;
    let f = fixture();
    let draft = draft(&f, 1, 1000, Outcome::Success);
    let prior = f.engine.store.db.limit(Limit::SQLITE_LIMIT_LENGTH).unwrap();
    let before = state(&f);
    assert!(
        plans::record(
            &f.engine.store,
            &draft,
            &Options::default(),
            &Deadline::new(0)
        )
        .is_err()
    );
    assert_eq!(state(&f), before);
    let oversized = "x".repeat(128_001);
    f.engine
        .store
        .db
        .execute(
            "UPDATE records SET value=?1 WHERE kind='plan' AND id=?2",
            rusqlite::params![oversized, f.plan.id],
        )
        .unwrap();
    let before = state(&f);
    assert_eq!(
        record(&f, &draft).unwrap_err().to_string(),
        "feedback_capacity_exceeded"
    );
    assert_eq!(state(&f), before);
    assert_eq!(
        f.engine.store.db.limit(Limit::SQLITE_LIMIT_LENGTH).unwrap(),
        prior
    );
}
