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
    assert_eq!(
        first["sourceProofs"][0]["evaluated"]["sourceScopes"][0]["kind"],
        "derived"
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
