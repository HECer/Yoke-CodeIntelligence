//! Real source/Meaning/receipt replay and immutable authored outcomes; no model
//! executes during feedback reads or writes. The owned fixture builds explicitly.
#[path = "support/meaning_feedback_cases.rs"]
mod cases;
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, process::Command};
use yoke::{
    analysis::AnalyzerEpoch,
    graph_view,
    knowledge::feedback::{
        Options, Outcome, SourceQuote, ViewPin,
        meaning::{self, Draft, Evidence, Kind, RankInput, Reference, Side, Target},
    },
    model::{hash, identity},
};
#[path = "support/meaning_comparison_fixture.rs"]
mod source;
use source::{Fixture, d};
fn fixture() -> Fixture {
    source::fixture(AnalyzerEpoch::V9)
}
fn pin(v: &graph_view::View) -> ViewPin {
    ViewPin {
        workspace_id: v.workspace_id.clone(),
        snapshot_id: v.snapshot_id.clone(),
        view_id: v.id.clone(),
        view_artifact_id: v.artifact_id.clone(),
        base_graph_hash: v.base_graph_hash.clone(),
    }
}
fn target(f: &Fixture, kind: Kind) -> Target {
    let layer = f.view.meaning.as_ref().unwrap();
    let packet = layer
        .packets
        .iter()
        .find(|p| !p.relations.is_empty())
        .unwrap();
    let id = match kind {
        Kind::Concept => packet
            .concepts
            .iter()
            .find(|c| c.label == "Storage policy")
            .unwrap()
            .claim_id
            .clone(),
        Kind::Relation => packet
            .relations
            .iter()
            .find(|c| c.description == "Forward")
            .unwrap()
            .claim_id
            .clone(),
        Kind::Hyperedge => packet.hyperedges[0].claim_id.clone(),
    };
    Target {
        origin: pin(&f.view),
        evaluated: pin(&f.view),
        reference: Reference {
            kind,
            graph_id: layer.selections[packet.selection_index].graph_id.clone(),
            packet_index: packet.index,
            claim_id: id,
        },
    }
}
fn draft(f: &Fixture, kind: Kind, sequence: u32, time: u64, outcome: Outcome) -> Draft {
    Draft {
        schema_version: 1,
        target: target(f, kind),
        actor: "Ada".into(),
        session: "review-one".into(),
        sequence,
        occurred_at_ms: time,
        outcome,
        rationale: "Explicit authored assessment of an existing inference.".into(),
        source_evidence: vec![Evidence {
            side: Side::Origin,
            source: SourceQuote {
                path: "a.rs".into(),
                content_hash: hash(source::A),
                start_byte: 0,
                end_byte: source::A.len(),
                quote: source::A.into(),
            },
        }],
    }
}
fn query(f: &Fixture, now: u64) -> RankInput {
    RankInput {
        schema_version: 1,
        targets: vec![target(f, Kind::Concept)],
        as_of_ms: now,
        decay_window_days: 1,
        limit: 20,
    }
}
fn record(f: &Fixture, draft: &Draft) -> anyhow::Result<Value> {
    meaning::record(&f.engine.store, draft, &Options::default(), &d())
}
fn rank(f: &Fixture, q: &RankInput) -> anyhow::Result<Value> {
    meaning::rank(&f.engine.store, q, &Options::default(), &d())
}
fn calls(f: &Fixture) -> usize {
    fs::read_to_string(&f.marker).unwrap().lines().count()
}
fn count(f: &Fixture) -> usize {
    f.engine
        .store
        .db
        .query_row(
            "SELECT count(*) FROM records WHERE kind=?1",
            [meaning::EVENT_KIND],
            |r| r.get(0),
        )
        .unwrap()
}
type State = (Vec<(String, String, String)>, BTreeMap<String, String>);
fn state(f: &Fixture) -> State {
    let mut stmt = f
        .engine
        .store
        .db
        .prepare("SELECT kind,id,value FROM records ORDER BY kind,id")
        .unwrap();
    let records = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    let blobs = fs::read_dir(f.engine.store.dir.join("blobs"))
        .unwrap()
        .map(|e| {
            let e = e.unwrap();
            (
                e.file_name().to_str().unwrap().to_owned(),
                hash(fs::read(e.path()).unwrap()),
            )
        })
        .collect();
    (records, blobs)
}
fn false_authorities(v: &Value) {
    for key in [
        "executionObserved",
        "workspaceApplicationObserved",
        "currentApplicabilityEstablished",
        "independentCorroborationEstablished",
        "interpretationTruthEstablished",
        "semanticIdentityEstablished",
        "sourceAuthority",
        "sourceWritten",
        "complete",
    ] {
        assert_eq!(v[key], false, "{key}");
    }
    assert_eq!(v["modelCalls"], 0);
}

#[test]
fn three_typed_targets_preserve_original_packets_parallel_directions_roles_and_receipt_unknowns() {
    let f = fixture();
    let mut ids = std::collections::BTreeSet::new();
    for kind in [Kind::Concept, Kind::Relation, Kind::Hyperedge] {
        let draft = draft(&f, kind, 1, 1000, Outcome::Success);
        let v = record(&f, &draft).unwrap();
        ids.insert(v["subjectId"].as_str().unwrap().to_owned());
        assert_eq!(v["inserted"], true);
        false_authorities(&v);
        let layer = f.view.meaning.as_ref().unwrap();
        let packet = layer
            .packets
            .iter()
            .find(|p| p.index == draft.target.reference.packet_index)
            .unwrap();
        assert_eq!(
            v["subject"]["packet"],
            serde_json::to_value(packet).unwrap()
        );
        assert_eq!(
            v["subject"]["packet"]["relations"]
                .as_array()
                .unwrap()
                .len(),
            3
        );
        assert_eq!(
            v["subject"]["packet"]["hyperedges"][0]["participants"][1]["role"],
            "input"
        );
        assert!(v["subject"]["packet"]["provenance"]["providerReportedCostMicros"].is_null());
        assert_eq!(v["subject"]["packet"]["provenance"]["rebound"], true);
        let before = state(&f);
        let retry = record(&f, &draft).unwrap();
        assert_eq!(retry["inserted"], false);
        assert_eq!(retry["id"], v["id"]);
        assert_eq!(state(&f), before);
    }
    assert_eq!(ids.len(), 3);
    assert_eq!(count(&f), 3);
    assert_eq!(calls(&f), 3);
    let legacy:usize=f.engine.store.db.query_row("SELECT count(*) FROM records WHERE kind IN ('native_feedback_event_v1','native_plan_feedback_event_v1')",[],|r|r.get(0)).unwrap();
    assert_eq!(legacy, 0);
}

#[test]
fn origin_support_membership_refuses_other_claim_paths_foreign_kinds_and_unselected_graphs() {
    let f = fixture();
    let before = state(&f);
    let packet = f
        .view
        .meaning
        .as_ref()
        .unwrap()
        .packets
        .iter()
        .find(|p| !p.relations.is_empty())
        .unwrap();
    for mode in 0..6 {
        let mut e = draft(&f, Kind::Concept, 1, 1000, Outcome::Success);
        match mode {
            0 => {
                e.target.reference.claim_id = packet
                    .concepts
                    .iter()
                    .find(|c| c.label == "Read path")
                    .unwrap()
                    .claim_id
                    .clone()
            }
            1 => e.target.reference.kind = Kind::Relation,
            2 => e.target.reference.packet_index = 0,
            3 => e.target.reference.graph_id = "a".repeat(64),
            4 => e.target.reference.claim_id = "b".repeat(64),
            _ => e.target.origin.base_graph_hash = "c".repeat(64),
        }
        assert!(record(&f, &e).is_err(), "case {mode}");
        assert_eq!(state(&f), before);
    }
}

#[test]
fn exact_unicode_ranges_and_path_admission_cannot_be_forged_by_rehashing_events() {
    let f = fixture();
    let before = state(&f);
    for mode in 0..5 {
        let mut e = draft(&f, Kind::Concept, 1, 1000, Outcome::Success);
        let q = &mut e.source_evidence[0].source;
        match mode {
            0 => q.path = "../a.rs".into(),
            1 => q.content_hash = "f".repeat(64),
            2 => {
                q.start_byte = source::A.find('é').unwrap() + 1;
                q.end_byte = q.start_byte + 2;
                q.quote = "é".into();
            }
            3 => q.quote = q.quote.replace("café", "cafe"),
            _ => {
                q.path = "history.rs".into();
                q.content_hash = hash(source::HISTORY_OLD);
                q.start_byte = 0;
                q.end_byte = source::HISTORY_OLD.len();
                q.quote = source::HISTORY_OLD.into();
            }
        }
        assert!(record(&f, &e).is_err(), "case {mode}");
        assert_eq!(state(&f), before);
    }
    let original = record(&f, &draft(&f, Kind::Concept, 1, 1000, Outcome::Success)).unwrap();
    let mut forged = original["event"].clone();
    forged["draft"]["sourceEvidence"][0]["source"]["contentHash"] = json!("f".repeat(64));
    let new_id = identity(&(
        meaning::EVENT_KIND,
        &f.engine.store.workspace_id,
        &forged["draft"],
    ));
    forged["id"] = json!(new_id);
    f.engine
        .store
        .db
        .execute(
            "DELETE FROM records WHERE kind=?1 AND id=?2",
            rusqlite::params![meaning::EVENT_KIND, original["id"].as_str().unwrap()],
        )
        .unwrap();
    f.engine
        .store
        .record(meaning::EVENT_KIND, &new_id, &forged)
        .unwrap();
    let before = state(&f);
    assert!(rank(&f, &query(&f, 1000)).is_err());
    assert_eq!(state(&f), before);
}

#[test]
fn independent_label_sessions_latest_sequences_future_events_and_integer_decay_are_visible() {
    let f = fixture();
    let day = 86_400_000u64;
    record(&f, &draft(&f, Kind::Concept, 1, 0, Outcome::Success)).unwrap();
    record(&f, &draft(&f, Kind::Concept, 2, day, Outcome::Failure)).unwrap();
    let mut other = draft(&f, Kind::Concept, 1, 0, Outcome::Corroboration);
    other.actor = "Bo".into();
    record(&f, &other).unwrap();
    let before = state(&f);
    let v = rank(&f, &query(&f, day)).unwrap();
    assert_eq!(v["items"][0]["score"], -500);
    assert_eq!(v["items"][0]["votingSessions"], 2);
    let votes = v["items"][0]["events"].as_array().unwrap();
    assert_eq!(
        votes
            .iter()
            .filter(|e| e["disposition"] == "superseded_in_session")
            .count(),
        1
    );
    assert!(votes.iter().any(|e| e["contribution"] == -1000));
    assert!(votes.iter().any(|e| e["contribution"] == 500));
    false_authorities(&v);
    assert_eq!(state(&f), before);
    let early = rank(&f, &query(&f, 0)).unwrap();
    assert_eq!(early["items"][0]["score"], 2000);
    assert!(
        early["items"][0]["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["disposition"] == "future")
    );
    assert_eq!(calls(&f), 3);
}

#[test]
fn changed_evaluated_crlf_source_retains_historical_events_without_claiming_semantic_identity() {
    let mut f = fixture();
    let first = draft(&f, Kind::Concept, 1, 1000, Outcome::Success);
    record(&f, &first).unwrap();
    let changed = source::A.replace('\n', "\r\n").replace("über", "anders");
    fs::write(f.engine.root.join("a.rs"), &changed).unwrap();
    let (snapshot, graph, _, _) = f.engine.index(&d()).unwrap();
    let view = graph_view::build(
        &f.engine.store,
        &snapshot,
        &graph,
        &Default::default(),
        &d(),
    )
    .unwrap();
    let mut next = first.clone();
    next.target.evaluated = pin(&view);
    next.sequence = 2;
    next.occurred_at_ms = 2000;
    next.outcome = Outcome::Contradiction;
    next.source_evidence = vec![Evidence {
        side: Side::Evaluated,
        source: SourceQuote {
            path: "a.rs".into(),
            content_hash: hash(&changed),
            start_byte: 0,
            end_byte: changed.len(),
            quote: changed,
        },
    }];
    let v = record(&f, &next).unwrap();
    assert_eq!(
        v["evaluatedQuoteCorrespondence"],
        "same_claim_support_path_only"
    );
    false_authorities(&v);
    let mut q = query(&f, 2000);
    q.targets = vec![next.target.clone()];
    let ranked = rank(&f, &q).unwrap();
    assert_eq!(ranked["items"][0]["score"], -1000);
    assert_eq!(ranked["items"][0]["events"].as_array().unwrap().len(), 2);
    assert_eq!(ranked["sourceProofs"].as_array().unwrap().len(), 2);
    let mut occupied = next;
    occupied.sequence = 1;
    occupied.occurred_at_ms = 1000;
    let before = state(&f);
    assert_eq!(
        record(&f, &occupied).unwrap_err().to_string(),
        "feedback_sequence_conflict"
    );
    assert_eq!(state(&f), before);
    assert_eq!(calls(&f), 3);
}

#[test]
fn out_of_order_timestamp_conflicts_and_other_subjects_never_rewrite_original_outcomes() {
    let f = fixture();
    record(&f, &draft(&f, Kind::Concept, 3, 3000, Outcome::Success)).unwrap();
    record(&f, &draft(&f, Kind::Concept, 1, 1000, Outcome::Failure)).unwrap();
    let before = state(&f);
    assert_eq!(
        record(&f, &draft(&f, Kind::Concept, 2, 4000, Outcome::Success))
            .unwrap_err()
            .to_string(),
        "feedback_sequence_conflict"
    );
    assert_eq!(state(&f), before);
    record(&f, &draft(&f, Kind::Relation, 1, 1000, Outcome::Success)).unwrap();
    let v = rank(&f, &query(&f, 3000)).unwrap();
    assert_eq!(v["eventsOutsideSelectedSubjects"], 1);
    assert_eq!(v["items"][0]["score"], 1000);
}
