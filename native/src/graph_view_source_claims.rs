//! Host-selected declarations bind original observations; attributes grant no authority.
use super::{View, canonical_view, sources};
use crate::{
    capture::Deadline,
    model::{Graph, Snapshot, hash, identity, valid_id},
    store::Store,
};
use anyhow::{Context, Result, ensure};
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

pub const RECORD_KIND: &str = "native_declared_source_claims_v1";
const PRODUCER: &str = "yoke-declared-source-claims-1";
const MAX_INPUT: usize = 1_000_000;
const MAX_ARTIFACT: usize = 2_100_000;
pub(crate) const ORIGIN: &str = "declared_source_claims";

#[path = "graph_view_claim_file.rs"]
mod host_file;
#[path = "graph_view_claim_json.rs"]
mod raw_json;
pub(crate) use raw_json::{Object, RawValue};
fn schema<T: serde::de::DeserializeOwned>(raw: &str) -> Result<T> {
    serde_json::from_str(raw).map_err(|_| anyhow::anyhow!("Invalid declared-source input schema"))
}
pub(crate) fn object(raw: &str, deadline: &Deadline) -> Result<Object> {
    raw_json::object(raw, deadline)
}
pub(crate) fn array(raw: &str, deadline: &Deadline) -> Result<Vec<RawValue>> {
    raw_json::array(raw, deadline)
}
pub(crate) struct Claim {
    pub observation_id: String,
    source: String,
    target: Value,
    evidence_ids: Vec<String>,
    pub attributes: RawValue,
}
pub(crate) struct Document {
    schema_version: u32,
    snapshot_id: String,
    workspace_id: String,
    base_graph_hash: String,
    pub claims: Vec<Claim>,
}
fn exact_fields(object: &Object, fields: &[&str]) -> Result<()> {
    ensure!(
        object.0.len() == fields.len() && fields.iter().all(|field| object.0.contains_key(*field)),
        "Missing or unknown declared source field"
    );
    Ok(())
}
fn parse_document(text: &str, deadline: &Deadline) -> Result<Document> {
    let root = object(text, deadline)?;
    exact_fields(
        &root,
        &[
            "schemaVersion",
            "snapshotId",
            "workspaceId",
            "baseGraphHash",
            "claims",
        ],
    )?;
    let mut claims = Vec::new();
    for raw in array(root.0["claims"].get(), deadline)? {
        deadline.check()?;
        let mut entry = object(raw.get(), deadline)?;
        exact_fields(
            &entry,
            &[
                "observationId",
                "source",
                "target",
                "evidenceIds",
                "attributes",
            ],
        )?;
        let target = if entry.0["target"].get() == "null" {
            Value::Null
        } else {
            json!(schema::<String>(entry.0["target"].get())?)
        };
        let mut evidence_ids = Vec::new();
        for id in array(entry.0["evidenceIds"].get(), deadline)? {
            evidence_ids.push(schema::<String>(id.get())?);
        }
        let observation_id = schema(entry.0["observationId"].get())?;
        let source = schema(entry.0["source"].get())?;
        let attributes = entry
            .0
            .remove("attributes")
            .context("Declared attributes absent")?;
        object(attributes.get(), deadline)?;
        claims.push(Claim {
            observation_id,
            source,
            target,
            evidence_ids,
            attributes,
        });
        ensure!(
            claims.len() <= 100_000,
            "Declared source claim capacity exceeded"
        );
    }
    Ok(Document {
        schema_version: schema(root.0["schemaVersion"].get())?,
        snapshot_id: schema(root.0["snapshotId"].get())?,
        workspace_id: schema(root.0["workspaceId"].get())?,
        base_graph_hash: schema(root.0["baseGraphHash"].get())?,
        claims,
    })
}
fn document(text: &str, view: &View, deadline: &Deadline) -> Result<Document> {
    ensure!(
        text.len() <= MAX_INPUT,
        "Declared source document exceeds byte capacity"
    );
    let input = parse_document(text, deadline)?;
    ensure!(
        input.schema_version == 1
            && input.snapshot_id == view.snapshot_id
            && input.workspace_id == view.workspace_id
            && input.base_graph_hash == view.base_graph_hash,
        "Declared source document scope mismatch"
    );
    ensure!(
        input.claims.len() <= 100_000,
        "Declared source claim capacity exceeded"
    );
    let mut originals = BTreeMap::new();
    for edge in &view.observations {
        deadline.check()?;
        if edge["origin"] == "canonical" {
            originals.insert(
                edge["id"]
                    .as_str()
                    .context("Original observation identity absent")?,
                edge,
            );
        }
    }
    for claim in &input.claims {
        deadline.check()?;
        let original = originals
            .get(claim.observation_id.as_str())
            .context("Declared original observation absent")?;
        ensure!(
            original["source"] == claim.source
                && original["target"] == claim.target
                && original["evidenceIds"] == json!(claim.evidence_ids),
            "Declared original endpoints or complete evidence list differ"
        );
        object(claim.attributes.get(), deadline)?;
    }
    Ok(input)
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Artifact {
    schema_version: u32,
    producer: String,
    snapshot_id: String,
    workspace_id: String,
    base_graph_hash: String,
    source_document_hash: String,
    source_document: String,
}
impl Artifact {
    fn receipt(&self, id: &str) -> Value {
        json!({"artifactId":id,"snapshotId":self.snapshot_id,"workspaceId":self.workspace_id,"baseGraphHash":self.base_graph_hash,"sourceDocumentHash":self.source_document_hash,"producer":self.producer})
    }
    fn selection(&self, id: &str) -> Value {
        let mut out = self.receipt(id);
        out["origin"] = json!(ORIGIN);
        out
    }
}
fn validate_artifact(
    raw: &[u8],
    id: &str,
    view: &View,
    deadline: &Deadline,
) -> Result<(Artifact, Document)> {
    deadline.check()?;
    ensure!(
        raw.len() <= MAX_ARTIFACT && valid_id(id) && hash(raw) == id,
        "Declared artifact size/identity mismatch"
    );
    let value = crate::json::parse(raw, MAX_ARTIFACT)?;
    let artifact: Artifact = crate::json::decode(value)?;
    ensure!(
        artifact.schema_version == 1
            && artifact.producer == PRODUCER
            && artifact.source_document_hash == hash(artifact.source_document.as_bytes())
            && artifact.snapshot_id == view.snapshot_id
            && artifact.workspace_id == view.workspace_id
            && artifact.base_graph_hash == view.base_graph_hash,
        "Declared artifact source scope mismatch"
    );
    ensure!(
        crate::json::encode_bounded(&artifact, MAX_ARTIFACT, deadline)? == raw,
        "Declared artifact differs from canonical retained bytes"
    );
    let input = document(&artifact.source_document, view, deadline)?;
    Ok((artifact, input))
}
fn preflight(
    store: &Store,
    kind: &str,
    id: &str,
    expected: &Value,
    deadline: &Deadline,
) -> Result<()> {
    deadline.check()?;
    let stored: Option<Option<String>> = store.db.query_row("SELECT CASE WHEN length(CAST(value AS BLOB)) <= 64000 THEN value END FROM records WHERE kind=?1 AND id=?2",rusqlite::params![kind,id],|row|row.get(0)).optional()?;
    if let Some(raw) = stored {
        let raw = raw.context("Existing declared-source registration exceeds capacity")?;
        ensure!(
            crate::json::parse(raw.as_bytes(), 64000)? == *expected,
            "Existing declared-source registration differs"
        );
    }
    Ok(())
}
/// Only the explicit host CLI calls this; no analyzed path or MCP tool invokes it.
pub fn import_file(
    store: &Store,
    snapshot: &Snapshot,
    graph: &Graph,
    path: &Path,
    deadline: &Deadline,
) -> Result<Value> {
    import_with_reader(store, snapshot, graph, deadline, &mut || {
        host_file::read(path, MAX_INPUT, deadline)
    })
}
fn import_with_reader(
    store: &Store,
    snapshot: &Snapshot,
    graph: &Graph,
    deadline: &Deadline,
    read_input: &mut dyn FnMut() -> Result<Vec<u8>>,
) -> Result<Value> {
    deadline.check()?;
    store.set_deadline(deadline)?;
    crate::graph_extensions::validate_scope(store, snapshot, graph, deadline)?;
    let bytes = read_input()?;
    ensure!(
        bytes.len() <= MAX_INPUT,
        "Declared host input exceeds byte capacity"
    );
    let text = String::from_utf8(bytes).context("Declared source document must be UTF-8")?;
    let original = canonical_view(
        snapshot,
        graph,
        &sources(store, snapshot, deadline)?,
        deadline,
    )?;
    document(&text, &original, deadline)?;
    let artifact = Artifact {
        schema_version: 1,
        producer: PRODUCER.into(),
        snapshot_id: snapshot.id.clone(),
        workspace_id: snapshot.workspace_id.clone(),
        base_graph_hash: identity(graph),
        source_document_hash: hash(text.as_bytes()),
        source_document: text,
    };
    let encoded = crate::json::encode_bounded(&artifact, MAX_ARTIFACT, deadline)?;
    let id = hash(&encoded);
    let receipt = artifact.receipt(&id);
    let artifact_record = json!({"artifactId":id,"totalBytes":encoded.len()});
    let tx =
        rusqlite::Transaction::new_unchecked(&store.db, rusqlite::TransactionBehavior::Immediate)?;
    // Predictable scope/registration failures all precede physical CAS publication.
    preflight(store, RECORD_KIND, &id, &receipt, deadline)?;
    preflight(store, "artifact", &id, &artifact_record, deadline)?;
    ensure!(
        store.artifact(&encoded)? == id,
        "Declared artifact publication identity mismatch"
    );
    tx.execute(
        "INSERT INTO records(kind,id,value) VALUES(?1,?2,?3) ON CONFLICT(kind,id) DO NOTHING",
        rusqlite::params![RECORD_KIND, id, serde_json::to_string(&receipt)?],
    )?;
    ensure!(
        super::record_value(store, RECORD_KIND, &id, deadline)? == receipt,
        "Declared artifact registration changed"
    );
    deadline.check()?;
    tx.commit()?;
    Ok(receipt)
}
pub(crate) fn load(
    store: &Store,
    id: &str,
    view: &View,
    deadline: &Deadline,
) -> Result<(Value, Document)> {
    let record = super::record_value(store, RECORD_KIND, id, deadline)?;
    let raw = super::artifact_limit(store, id, MAX_ARTIFACT, deadline)?;
    let (artifact, input) = validate_artifact(&raw, id, view, deadline)?;
    ensure!(
        record == artifact.receipt(id),
        "Declared selected registration differs from retained artifact"
    );
    Ok((artifact.selection(id), input))
}
pub(crate) fn replay(raw: &[u8], id: &str, expected: &mut View, deadline: &Deadline) -> Result<()> {
    let (artifact, _) = validate_artifact(raw, id, expected, deadline)?;
    expected.overlays.push(artifact.selection(id));
    super::limits(expected)
}
pub(crate) fn selected(
    store: &Store,
    view: &View,
    deadline: &Deadline,
) -> Result<BTreeMap<String, Vec<RawValue>>> {
    let mut out = BTreeMap::<String, Vec<RawValue>>::new();
    let mut count = 0usize;
    let mut bytes = 0usize;
    let mut ids = BTreeSet::new();
    for selection in &view.overlays {
        if selection["origin"] != ORIGIN {
            continue;
        }
        deadline.check()?;
        let id = selection["artifactId"]
            .as_str()
            .context("Declared selection identity absent")?;
        ensure!(ids.insert(id), "Duplicate declared selection");
        super::charge_input(store, id, &mut bytes, deadline)?;
        let (checked, input) = load(store, id, view, deadline)?;
        ensure!(
            checked == *selection,
            "Declared selected descriptor changed"
        );
        for claim in input.claims {
            deadline.check()?;
            count += 1;
            ensure!(
                count <= 100_000,
                "Selected declared claim capacity exceeded"
            );
            out.entry(claim.observation_id)
                .or_default()
                .push(claim.attributes);
        }
    }
    ensure!(
        !ids.is_empty(),
        "Declared source policy requires an explicitly selected admitted document"
    );
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    #[cfg(unix)]
    fn state(store: &Store) -> (Vec<(String, String, String)>, BTreeMap<String, String>) {
        let mut statement=store.db.prepare("SELECT 'snapshot',id,value FROM snapshots UNION ALL SELECT 'graph',id,value FROM graphs UNION ALL SELECT 'record:'||kind,id,value FROM records ORDER BY 1,2,3").unwrap();
        let rows = statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .unwrap()
            .collect::<rusqlite::Result<Vec<(String, String, String)>>>()
            .unwrap();
        let blobs = fs::read_dir(store.dir.join("blobs"))
            .unwrap()
            .map(|entry| {
                let entry = entry.unwrap();
                let name = entry.file_name().into_string().unwrap();
                let raw = fs::read(entry.path()).unwrap();
                assert!(raw.len() < 2_100_000);
                assert_eq!(hash(&raw), name);
                (name, hash(&raw))
            })
            .collect();
        (rows, blobs)
    }
    #[test]
    #[cfg(unix)]
    fn same_inode_multichunk_mutation_refuses_before_any_publication() {
        let root = tempfile::TempDir::new().unwrap();
        let host = tempfile::TempDir::new().unwrap();
        let source = b"fn root(){ leaf(); }\nfn leaf(){}\n";
        fs::write(root.path().join("main.rs"), source).unwrap();
        let mut engine = crate::Engine::open(root.path()).unwrap();
        let d = Deadline::new(30000);
        let (snapshot, graph, _, _) = engine.index(&d).unwrap();
        let claims:Vec<_>=graph.observations.iter().map(|edge|json!({"observationId":edge.id,"source":edge.source,"target":edge.target,"evidenceIds":edge.evidence_ids,"attributes":{"confidence":"INFERRED","confidence_score":0.1,"padding":"x".repeat(100_000)}})).collect();
        let raw=serde_json::to_vec(&json!({"schemaVersion":1,"snapshotId":snapshot.id,"workspaceId":snapshot.workspace_id,"baseGraphHash":identity(&graph),"claims":claims})).unwrap();
        assert!(raw.len() > 128_000 && raw.len() < MAX_INPUT);
        let changed = String::from_utf8(raw.clone())
            .unwrap()
            .replace("0.1", "0.9")
            .into_bytes();
        assert_eq!(raw.len(), changed.len());
        assert_ne!(raw, changed);
        let path = host.path().join("selected.json");
        fs::write(&path, &raw).unwrap();
        import_file(&engine.store, &snapshot, &graph, &path, &d).unwrap();
        let before = state(&engine.store);
        let latest = engine.store.latest().unwrap();
        let mut changed_once = false;
        let result = import_with_reader(&engine.store, &snapshot, &graph, &d, &mut || {
            host_file::read_with_hook(&path, MAX_INPUT, &d, &mut || {
                if !changed_once {
                    fs::write(&path, &changed)?;
                    fs::File::open(&path)?
                        .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1))?;
                    changed_once = true;
                }
                Ok(())
            })
        });
        assert!(changed_once && result.is_err());
        assert_eq!(state(&engine.store), before);
        assert_eq!(engine.store.latest().unwrap(), latest);
        assert_eq!(fs::read(root.path().join("main.rs")).unwrap(), source);
    }
    #[test]
    fn declared_selection_replays_storeless_original_bytes_and_refuses_corrupt_input() {
        let root = tempfile::TempDir::new().unwrap();
        let host = tempfile::TempDir::new().unwrap();
        let source = "// π\r\nfn root(){ leaf(); }\r\nfn leaf(){}\r\n";
        fs::write(root.path().join("main.rs"), source).unwrap();
        let mut engine = crate::Engine::open(root.path()).unwrap();
        let d = Deadline::new(30000);
        let (snapshot, graph, _, _) = engine.index(&d).unwrap();
        let claims:Vec<_>=graph.observations.iter().map(|edge|json!({"observationId":edge.id,"source":edge.source,"target":edge.target,"evidenceIds":edge.evidence_ids,"attributes":{"confidence":"INFERRED","confidence_score":0.1}})).collect();
        let raw=serde_json::to_vec(&json!({"schemaVersion":1,"snapshotId":snapshot.id,"workspaceId":snapshot.workspace_id,"baseGraphHash":identity(&graph),"claims":claims})).unwrap();
        let input = host.path().join("claims.json");
        fs::write(&input, &raw).unwrap();
        let receipt = import_file(&engine.store, &snapshot, &graph, &input, &d).unwrap();
        let view = super::super::build(
            &engine.store,
            &snapshot,
            &graph,
            &super::super::BuildOptions {
                structural_kinds: vec![],
                source_claim_overlay_ids: vec![receipt["artifactId"].as_str().unwrap().to_owned()],
                ..Default::default()
            },
            &d,
        )
        .unwrap();
        let (original, projection, proof) =
            super::super::archive(&engine.store, &view, &d).unwrap();
        fs::remove_file(&input).unwrap();
        fs::write(root.path().join("main.rs"), "fn newer(){}\n").unwrap();
        let (newer, _, _, _) = engine.index(&d).unwrap();
        assert_ne!(newer.id, original.id);
        let texts = BTreeMap::from([("main.rs".to_owned(), source.to_owned())]);
        let (replayed, epoch) =
            super::super::validate_archive(&original, &proof, &texts, &d).unwrap();
        assert!(epoch.is_some());
        assert_eq!(identity(&replayed), identity(&projection));
        let mut corrupt = proof.clone();
        corrupt.inputs[0].payload.push(' ');
        assert!(super::super::validate_archive(&original, &corrupt, &texts, &d).is_err());
        assert!(!input.exists());
    }
}
