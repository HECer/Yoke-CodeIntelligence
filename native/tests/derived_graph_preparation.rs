//! Structural preparation must preserve released bytes without publishing them.
use anyhow::Result;
use serde::Serialize;
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs};
use tempfile::TempDir;
use yoke::{
    Engine,
    analysis::AnalyzerEpoch,
    capture::Deadline,
    manifest_graph::{DerivedGraph, PreparedDerivedGraph},
    model::{Graph, Snapshot, hash, identity},
};

const LIMITATION: &str = "Declarations and name matches are source facts or candidates; they do not authorize execution, network access or establish compiler/runtime behavior.";

#[derive(Clone, Copy)]
enum Kind {
    Manifest,
    Config,
    Markdown,
}

struct Fixture {
    engine: Engine,
    snapshot: Snapshot,
    graph: Graph,
    _dir: TempDir,
}

fn fixture(epoch: AnalyzerEpoch, files: &[(&str, &str)]) -> Fixture {
    let dir = TempDir::new().unwrap();
    for (path, text) in files {
        fs::write(dir.path().join(path), text).unwrap();
    }
    let mut engine = Engine::open(dir.path()).unwrap();
    engine.capture_epoch = epoch;
    let (snapshot, graph, _, _) = engine.index(&Deadline::new(30000)).unwrap();
    Fixture {
        _dir: dir,
        engine,
        snapshot,
        graph,
    }
}

impl Kind {
    fn prepare<'a>(self, f: &'a Fixture, d: &'a Deadline) -> Result<PreparedDerivedGraph<'a>> {
        let s = &f.engine.store;
        match self {
            Self::Manifest => yoke::manifest_graph::prepare(s, &f.snapshot, &f.graph, d),
            Self::Config => yoke::config_graph::prepare(s, &f.snapshot, &f.graph, d),
            Self::Markdown => yoke::markdown_graph::prepare(s, &f.snapshot, &f.graph, d),
        }
    }
    fn extract(self, f: &Fixture, d: &Deadline) -> Result<DerivedGraph> {
        let s = &f.engine.store;
        match self {
            Self::Manifest => yoke::manifest_graph::extract(s, &f.snapshot, &f.graph, d),
            Self::Config => yoke::config_graph::extract(s, &f.snapshot, &f.graph, d),
            Self::Markdown => yoke::markdown_graph::extract(s, &f.snapshot, &f.graph, d),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct State {
    tables: BTreeMap<String, i64>,
    records: Vec<(String, String, String)>,
    blobs: BTreeMap<String, u64>,
}

fn state(f: &Fixture) -> State {
    let db = &f.engine.store.db;
    let names: Vec<String> = db.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").unwrap()
        .query_map([], |r| r.get(0)).unwrap().collect::<rusqlite::Result<_>>().unwrap();
    let tables = names
        .into_iter()
        .map(|name| {
            let n = db
                .query_row(&format!("SELECT count(*) FROM {name}"), [], |r| r.get(0))
                .unwrap();
            (name, n)
        })
        .collect();
    let records = db
        .prepare("SELECT kind,id,value FROM records ORDER BY kind,id")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    let blobs = fs::read_dir(f.engine.store.dir.join("blobs"))
        .unwrap()
        .map(|e| {
            let e = e.unwrap();
            (
                e.file_name().into_string().unwrap(),
                e.metadata().unwrap().len(),
            )
        })
        .collect();
    State {
        tables,
        records,
        blobs,
    }
}

// Independent released serialization oracle: fixture ranges and graph facts are literal.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Released<'a> {
    snapshot_id: &'a str,
    workspace_id: &'a str,
    graph_hash: String,
    producer: &'a str,
    nodes: Vec<Value>,
    observations: Vec<Value>,
    evidence: Vec<Value>,
    limitations: Vec<&'a str>,
    artifact_id: &'a str,
}

fn expected(f: &Fixture, kind: Kind, path: &str, text: &str) -> Vec<u8> {
    let (producer, node_kind, name, ecosystem, start, end, granularity) = match kind {
        Kind::Manifest => (
            "yoke-manifest-graph-1",
            "package",
            "app",
            "npm",
            9,
            12,
            "unique_literal_occurrence",
        ),
        Kind::Config => (
            "yoke-config-graph-1",
            "configuration_key",
            "mode",
            "configuration",
            2,
            6,
            "unique_literal_occurrence",
        ),
        Kind::Markdown => (
            "yoke-markdown-graph-2",
            "document",
            "guide.md",
            "markdown",
            0,
            8,
            "file",
        ),
    };
    let content_hash = hash(text.as_bytes());
    let anchor = identity(&(&f.snapshot.id, path, &content_hash, start, end, granularity));
    let mut node = json!({"id":identity(&(&f.snapshot.id,path,node_kind,name,ecosystem)),"snapshotId":f.snapshot.id,"path":path,"kind":node_kind,"name":name,"ecosystem":ecosystem,"evidenceIds":[anchor]});
    if matches!(kind, Kind::Config) {
        node["jsonPointer"] = json!("/mode");
    }
    serde_json::to_vec(&Released {
        snapshot_id: &f.snapshot.id, workspace_id: &f.snapshot.workspace_id,
        graph_hash: identity(&f.graph), producer, nodes: vec![node], observations: vec![],
        evidence: vec![json!({"id":anchor,"snapshotId":f.snapshot.id,"path":path,"contentHash":content_hash,"startByte":start,"endByte":end,"startLine":1,"endLine":if matches!(kind,Kind::Markdown){2}else{1},"anchor":granularity})],
        limitations: vec![LIMITATION], artifact_id: "",
    }).unwrap()
}

fn exact_released_bytes(kind: Kind, path: &str, text: &str) {
    for epoch in [AnalyzerEpoch::V8, AnalyzerEpoch::V9] {
        let f = fixture(epoch, &[(path, text)]);
        let d = Deadline::new(30000);
        let before = state(&f);
        let wanted = expected(&f, kind, path, text);
        let prepared = kind
            .prepare(&f, &d)
            .expect("Pure structural preparation is required");
        assert_eq!(state(&f), before, "Preparation published structural state");
        assert_eq!(prepared.bytes(), wanted);
        assert_eq!(prepared.byte_len(), wanted.len());
        assert_eq!(prepared.artifact_id(), hash(&wanted));
        let published = kind.extract(&f, &d).unwrap();
        assert_eq!(published.artifact_id, hash(&wanted));
        assert_eq!(
            f.engine
                .store
                .read_artifact(&published.artifact_id)
                .unwrap(),
            wanted
        );
    }
}

#[test]
fn manifest_preparation_keeps_authentic_v8_v9_released_bytes_without_publication() {
    exact_released_bytes(Kind::Manifest, "package.json", r#"{"name":"app"}"#);
}

#[test]
fn config_preparation_keeps_authentic_v8_v9_released_bytes_without_publication() {
    exact_released_bytes(Kind::Config, "settings.json", r#"{"mode":"local"}"#);
}

#[test]
fn markdown_preparation_keeps_authentic_v8_v9_released_bytes_without_publication() {
    exact_released_bytes(Kind::Markdown, "guide.md", "# Guide\n");
}

fn all_fixture() -> Fixture {
    fixture(
        AnalyzerEpoch::V9,
        &[
            ("package.json", r#"{"name":"app"}"#),
            ("settings.json", r#"{"mode":"local"}"#),
            ("guide.md", "# Guide\n"),
        ],
    )
}

#[test]
fn three_prepared_graphs_publish_in_one_caller_transaction_without_nested_transactions() {
    let f = all_fixture();
    let d = Deadline::new(30000);
    let prepared = [Kind::Manifest, Kind::Config, Kind::Markdown]
        .into_iter()
        .map(|k| k.prepare(&f, &d).unwrap())
        .collect::<Vec<_>>();
    let ids = prepared
        .iter()
        .map(|p| p.artifact_id().to_owned())
        .collect::<Vec<_>>();
    let tx = rusqlite::Transaction::new_unchecked(
        &f.engine.store.db,
        rusqlite::TransactionBehavior::Immediate,
    )
    .unwrap();
    for p in prepared {
        p.publish_into(&tx, &d)
            .expect("Caller transaction must publish without nesting");
    }
    tx.commit().unwrap();
    for id in ids {
        assert!(f.engine.store.read_artifact(&id).is_ok());
        let record: Value = f
            .engine
            .store
            .get_record(yoke::manifest_graph::RECORD_KIND, &id)
            .unwrap();
        assert_eq!(record["artifactId"], id);
    }
}

#[test]
fn rolling_back_caller_transaction_removes_all_three_graph_registrations() {
    let f = all_fixture();
    let d = Deadline::new(30000);
    let before = state(&f);
    let prepared = [Kind::Manifest, Kind::Config, Kind::Markdown]
        .into_iter()
        .map(|k| k.prepare(&f, &d).unwrap())
        .collect::<Vec<_>>();
    let tx = rusqlite::Transaction::new_unchecked(
        &f.engine.store.db,
        rusqlite::TransactionBehavior::Immediate,
    )
    .unwrap();
    for p in prepared {
        p.publish_into(&tx, &d).unwrap();
    }
    tx.rollback().unwrap();
    let after = state(&f);
    assert_eq!(after.tables, before.tables);
    assert_eq!(after.records, before.records);
    // CAS files are immutable physical writes and are outside SQLite rollback.
}

#[test]
fn foreign_connection_is_refused_before_any_cas_or_record_write() {
    let source = all_fixture();
    let foreign = all_fixture();
    let d = Deadline::new(30000);
    let before_source = state(&source);
    let before_foreign = state(&foreign);
    let p = Kind::Manifest.prepare(&source, &d).unwrap();
    let tx = rusqlite::Transaction::new_unchecked(
        &foreign.engine.store.db,
        rusqlite::TransactionBehavior::Immediate,
    )
    .unwrap();
    assert!(p.publish_into(&tx, &d).is_err());
    tx.rollback().unwrap();
    assert_eq!(state(&source), before_source);
    assert_eq!(state(&foreign), before_foreign);
}

#[test]
fn conflicting_existing_derived_rows_refuse_before_cas_in_caller_and_standalone_publication() {
    let mut changed_namespaces = Vec::new();
    for caller_transaction in [true, false] {
        let f = all_fixture();
        let d = Deadline::new(30000);
        let prepared = Kind::Manifest.prepare(&f, &d).unwrap();
        let id = prepared.artifact_id().to_owned();
        assert!(!f.engine.store.dir.join("blobs").join(&id).exists());
        // The occupied key is real, but its claimed scope is inconsistent. Its
        // absence from CAS must survive this predictable admission refusal.
        f.engine.store.record(yoke::manifest_graph::RECORD_KIND, &id, &json!({
            "snapshotId":f.snapshot.id,"workspaceId":"0".repeat(64),
            "graphHash":identity(&f.graph),"artifactId":id,"producer":"yoke-manifest-graph-1"
        })).unwrap();
        let before = state(&f);
        let error = if caller_transaction {
            let tx = rusqlite::Transaction::new_unchecked(
                &f.engine.store.db,
                rusqlite::TransactionBehavior::Immediate,
            )
            .unwrap();
            let result = prepared.publish_into(&tx, &d);
            tx.rollback().unwrap();
            result.expect_err("An occupied inconsistent derived row must be refused")
        } else {
            drop(prepared);
            Kind::Manifest
                .extract(&f, &d)
                .expect_err("Standalone publication must also admit its existing scope first")
        };
        assert!(
            format!("{error:#}").contains("Stored derived graph scope differs"),
            "The fixture must reach the occupied derived scope: {error:#}"
        );
        if state(&f) != before {
            changed_namespaces.push(if caller_transaction {
                "caller transaction"
            } else {
                "standalone extract"
            });
        }
    }
    assert!(
        changed_namespaces.is_empty(),
        "Predictable occupied-row refusals changed DB/CAS namespaces: {changed_namespaces:?}"
    );
}
