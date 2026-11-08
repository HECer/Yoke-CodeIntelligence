//! Separately versioned strict-schema drafts; no predecessor files are edited.
//! Fixture Values are small actual graphs, never production64MB decode trees.

use super::*;
use crate::{
    Engine,
    analysis::AnalyzerEpoch,
    model::{Graph, Snapshot, identity},
};
use rusqlite::limits::Limit;
use serde_json::{Value, json};
use std::fs;
use tempfile::TempDir;

struct Fixture {
    engine: Engine,
    snapshot: Snapshot,
    graph: Graph,
    _root: TempDir,
}
impl Fixture {
    fn new(epoch: AnalyzerEpoch) -> Self {
        let root = TempDir::new().unwrap();
        fs::write(root.path().join("lib.rs"), "fn f(){} fn c(){f();}\n").unwrap();
        let mut engine = Engine::open(root.path()).unwrap();
        engine.capture_epoch = epoch;
        let (snapshot, graph, _, _) = engine.index(&Deadline::new(30_000)).unwrap();
        assert!(!graph.observations.is_empty() && !graph.evidence.is_empty());
        assert!(serde_json::to_vec(&graph).unwrap().len() < 16_384);
        Self {
            engine,
            snapshot,
            graph,
            _root: root,
        }
    }
    fn check(&self) -> Result<(), RetrievalHardError> {
        let d = Deadline::new(10_000);
        let tx = self.engine.store.db.unchecked_transaction().unwrap();
        let result = verify_saved_graph_in(
            &self.engine.store,
            &tx,
            &self.snapshot.id,
            &identity(&self.graph),
            &d,
        );
        assert!(
            !self.engine.store.db.is_autocommit(),
            "verifier must not end caller-owned transaction"
        );
        drop(tx);
        result
    }
    fn replace(&self, raw: &str) {
        self.engine
            .store
            .db
            .execute(
                "UPDATE graphs SET value=?1 WHERE id=?2",
                rusqlite::params![raw, self.snapshot.id],
            )
            .unwrap();
    }
}
fn schema_error(result: Result<(), RetrievalHardError>) {
    let error = result.unwrap_err();
    assert!(
        matches!(error, RetrievalHardError::SourceIntegrity(_)),
        "{error}"
    );
    assert!(!error.to_string().contains("SCHEMA_SECRET_FIELD"));
    assert!(!error.to_string().contains("SCHEMA_SECRET_VALUE"));
}

#[test]
fn current_native_epochs8_and9_keep_genuine_saved_graph_controls() {
    for epoch in [AnalyzerEpoch::V8, AnalyzerEpoch::V9] {
        let f = Fixture::new(epoch);
        f.check().unwrap();
        assert_eq!(
            identity(&f.engine.store.graph(&f.snapshot.id).unwrap()),
            identity(&f.graph)
        );
    }
}

#[test]
fn unknown_graph_entity_observation_evidence_and_coverage_fields_are_hard() {
    for target in ["graph", "entity", "observation", "evidence", "coverage"] {
        let f = Fixture::new(AnalyzerEpoch::V9);
        f.check().unwrap(); // Actual valid authority before foreign schema.
        let mut raw = serde_json::to_value(&f.graph).unwrap();
        let object = match target {
            "graph" => &mut raw,
            "entity" => &mut raw["entities"][0],
            "observation" => &mut raw["observations"][0],
            "evidence" => &mut raw["evidence"][0],
            "coverage" => &mut raw["coverage"],
            _ => unreachable!(),
        };
        object["SCHEMA_SECRET_FIELD_DO_NOT_ECHO"] =
            json!({"value":"SCHEMA_SECRET_VALUE_DO_NOT_ECHO"});
        let text = serde_json::to_string(&raw).unwrap();
        // Explicit negative control: legacy typed data ignores these extras and
        // retains the expected digest. That typed value is NOT source authority
        // and is never published or passed as a checked proof.
        let ignored: Graph = serde_json::from_str(&text).unwrap();
        assert_eq!(
            identity(&ignored),
            identity(&f.graph),
            "{target} must isolate ignored-field admission"
        );
        f.replace(&text);
        schema_error(f.check());
    }
}

#[test]
fn same_value_duplicate_known_fields_at_graph_member_and_coverage_are_hard() {
    for target in ["graph", "entity", "coverage"] {
        let f = Fixture::new(AnalyzerEpoch::V9);
        f.check().unwrap();
        let mut text = serde_json::to_string(&f.graph).unwrap();
        match target {
            "graph" => text.insert_str(1, &format!("\"snapshotId\":{},", json!(f.snapshot.id))),
            "entity" => {
                let needle = "\"entities\":[{";
                let at = text.find(needle).unwrap() + needle.len();
                text.insert_str(
                    at,
                    &format!("\"name\":{},", json!(f.graph.entities[0].name)),
                );
            }
            "coverage" => {
                let needle = "\"coverage\":{";
                let at = text.find(needle).unwrap() + needle.len();
                text.insert_str(
                    at,
                    &format!("\"unresolved\":{},", f.graph.coverage.unresolved),
                );
            }
            _ => unreachable!(),
        }
        f.replace(&text);
        schema_error(f.check());
    }
}

#[test]
fn decoded_escaped_duplicate_language_map_key_is_hard_even_when_values_are_equal() {
    let f = Fixture::new(AnalyzerEpoch::V9);
    f.check().unwrap();
    assert_eq!(f.graph.coverage.languages.len(), 1);
    assert_eq!(f.graph.coverage.languages.get("rust"), Some(&1));
    let text = serde_json::to_string(&f.graph).unwrap();
    let needle = r#""languages":{"rust":1}"#;
    assert_eq!(text.matches(needle).count(), 1);
    let duplicate = text.replacen(needle, r#""languages":{"rust":1,"\u0072ust":1}"#, 1);
    let ignored: Graph = serde_json::from_str(&duplicate).unwrap();
    assert_eq!(
        identity(&ignored),
        identity(&f.graph),
        "ordinary map decode collapses the duplicate"
    );
    f.replace(&duplicate);
    schema_error(f.check());
}

#[test]
fn strict_schema_corruption_reaches_real_auto_entry_as_hard_without_fallback() {
    let f = Fixture::new(AnalyzerEpoch::V9);
    f.check().unwrap();
    let mut raw: Value = serde_json::to_value(&f.graph).unwrap();
    raw["SCHEMA_SECRET_FIELD_DO_NOT_ECHO"] = json!("SCHEMA_SECRET_VALUE_DO_NOT_ECHO");
    f.replace(&serde_json::to_string(&raw).unwrap());
    let before = f.engine.store.db.limit(Limit::SQLITE_LIMIT_LENGTH).unwrap();
    let error = crate::retrieval::find_compatibility_auto(
        &f.engine.store,
        &f.snapshot,
        &f.graph,
        &json!({"query":"f"}),
        &Deadline::new(10_000),
        4_000_000,
    )
    .unwrap_err();
    assert!(
        matches!(
            error.downcast_ref::<RetrievalHardError>(),
            Some(RetrievalHardError::SourceIntegrity(_))
        ),
        "{error:#}"
    );
    assert!(!format!("{error:#}").contains("SCHEMA_SECRET_FIELD"));
    assert!(!format!("{error:#}").contains("SCHEMA_SECRET_VALUE"));
    assert_eq!(
        f.engine.store.db.limit(Limit::SQLITE_LIMIT_LENGTH).unwrap(),
        before
    );
}

#[test]
fn foreign_transaction_refusal_preserves_both_distinct_lower_host_limits() {
    let f = Fixture::new(AnalyzerEpoch::V9);
    f.check().unwrap();
    let other = Store::open(&f.engine.root).unwrap();
    assert_eq!(other.workspace_id, f.engine.store.workspace_id);
    let kind = Limit::SQLITE_LIMIT_LENGTH;
    let own_original = f.engine.store.db.set_limit(kind, 24_576).unwrap();
    let other_original = other.db.set_limit(kind, 32_768).unwrap();
    assert_eq!(f.engine.store.db.limit(kind).unwrap(), 24_576);
    assert_eq!(other.db.limit(kind).unwrap(), 32_768);
    let tx = other.db.unchecked_transaction().unwrap();
    let result = verify_saved_graph_in(
        &f.engine.store,
        &tx,
        &f.snapshot.id,
        &identity(&f.graph),
        &Deadline::new(10_000),
    );
    assert!(
        matches!(result, Err(RetrievalHardError::Authority(_))),
        "{result:?}"
    );
    assert_eq!(f.engine.store.db.limit(kind).unwrap(), 24_576);
    assert_eq!(
        other.db.limit(kind).unwrap(),
        32_768,
        "foreign connection policy must not be changed before Authority refusal"
    );
    assert!(
        !other.db.is_autocommit(),
        "foreign caller transaction must remain open"
    );
    drop(tx);
    f.engine.store.db.set_limit(kind, own_original).unwrap();
    other.db.set_limit(kind, other_original).unwrap();
}
