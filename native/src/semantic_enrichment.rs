use crate::{
    capture::Deadline,
    model::{Graph, Snapshot, identity, valid_id},
    paths::utf8_range,
    query,
    store::Store,
};
use anyhow::{Context, Result, ensure};
use rusqlite::{OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub const RECORD_KIND: &str = "native_semantic_overlay_v1";
pub const RECORD_KIND_V2: &str = "native_semantic_overlay_v2";
pub const RECORD_KINDS: &[&str] = &[RECORD_KIND, RECORD_KIND_V2];
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Record {
    overlay_id: String,
    artifact_id: String,
    snapshot_id: String,
    base_graph_hash: String,
    source_bindings: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    provider_context_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    server_profile_hash: Option<String>,
}
/// Read only the two exact registered record kinds; callers can preflight the
/// artifact size before loading it. Ambiguous IDs and future fields fail.
pub(crate) fn record_for_overlay(store: &Store, id: &str, deadline: &Deadline) -> Result<Value> {
    store.set_deadline(deadline)?;
    ensure!(valid_id(id), "Invalid semantic overlay ID");
    let prior = store.db.set_limit(
        rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH,
        store
            .db
            .limit(rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH)?
            .min(4_000_000),
    )?;
    let result = (|| -> Result<Vec<(String, String)>> {
        let mut query = store
            .db
            .prepare("SELECT kind,value FROM records WHERE id=?1 AND kind IN(?2,?3) LIMIT 3")?;
        let rows = query.query_map(params![id, RECORD_KIND, RECORD_KIND_V2], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    })();
    store
        .db
        .set_limit(rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH, prior)?;
    let rows = result?;
    ensure!(
        rows.len() == 1,
        "Semantic overlay record absent or ambiguous"
    );
    let (kind, raw) = &rows[0];
    let value = crate::json::parse(raw.as_bytes(), 4_000_000)?;
    let record: Record = crate::json::decode(value.clone())?;
    ensure!(
        serde_json::to_value(&record)? == value
            && record.overlay_id == id
            && [
                &record.artifact_id,
                &record.snapshot_id,
                &record.base_graph_hash
            ]
            .into_iter()
            .all(|id| valid_id(id))
            && record.source_bindings.len() <= 4000,
        "Invalid semantic record fields or identity"
    );
    if kind == RECORD_KIND_V2 {
        ensure!(
            record
                .provider_context_hash
                .as_deref()
                .is_some_and(valid_id)
                && record.server_profile_hash.as_deref().is_some_and(valid_id),
            "V2 semantic record context binding absent"
        );
    } else {
        ensure!(
            record.provider_context_hash.is_none() && record.server_profile_hash.is_none(),
            "Historical semantic record has unknown context fields"
        );
    }
    for (path, hash) in &record.source_bindings {
        deadline.check()?;
        ensure!(
            crate::paths::normalized_syntax(path)? == *path && valid_id(hash),
            "Invalid semantic record source binding"
        );
    }
    deadline.check()?;
    Ok(value)
}
fn validate_provider_context(
    store: &Store,
    snapshot: &Snapshot,
    value: &Value,
    deadline: &Deadline,
) -> Result<()> {
    validate_archived_provider_context(snapshot, value, deadline)?;
    let scope = crate::capture::source_scope_policy(store, snapshot)?;
    for omission in value["omissions"]
        .as_array()
        .context("Provider omissions missing")?
    {
        deadline.check()?;
        let file = scope.file(
            omission["path"]
                .as_str()
                .context("Provider omission path missing")?,
        )?;
        ensure!(
            omission["contentHash"] == file.hash,
            "Provider omission source proof mismatch"
        );
    }
    deadline.check()
}
/// Validate context against an already authenticated archived source manifest.
/// This pure check grants no filesystem or provider execution authority.
pub(crate) fn validate_archived_provider_context(
    snapshot: &Snapshot,
    value: &Value,
    deadline: &Deadline,
) -> Result<()> {
    ensure!(
        value.as_object().is_some_and(|object| object.len() == 3
            && object.keys().all(
                |key| ["sourceExecutionAuthorized", "sourcePolicy", "omissions"]
                    .contains(&key.as_str())
            ))
            && serde_json::to_vec(value)?.len() <= 4_000_000,
        "Unknown or excessive provider context fields"
    );
    let authorized = value["sourceExecutionAuthorized"]
        .as_bool()
        .context("Provider context host permission must be boolean")?;
    let policy = value["sourcePolicy"]
        .as_str()
        .context("Provider source policy missing")?;
    ensure!(
        matches!(
            policy,
            "passive_rust_source_selectors_and_inherited_cargo_config_excluded"
                | "contract_fixture_rust_selector_broker_only"
                | "passive_rust_json_project_selectors_excluded_v2"
                | "contract_fixture_rust_json_project_selectors_excluded_v2"
                | "contract_fixture_protocol_only"
                | "passive_typescript_automatic_typing_acquisition_and_local_plugins_disabled"
                | "passive_jedi_project_import_paths_and_environment_overrides_disabled"
                | "host_selected_provider_unattested_transitive_behavior"
        ),
        "Unknown provider source policy"
    );
    let omissions = value["omissions"]
        .as_array()
        .filter(|items| items.len() <= 1000)
        .context("Invalid provider omission capacity")?;
    let mut files = BTreeMap::new();
    for file in &snapshot.files {
        deadline.check()?;
        ensure!(
            files.insert(file.path.as_str(), file).is_none(),
            "Ambiguous archived source member"
        );
    }
    let mut paths = std::collections::BTreeSet::new();
    let mut prior = "";
    for omission in omissions {
        deadline.check()?;
        ensure!(
            omission.as_object().is_some_and(|object| object.len() == 3
                && object
                    .keys()
                    .all(|key| ["path", "contentHash", "reason"].contains(&key.as_str()))),
            "Unknown provider omission field"
        );
        let path = omission["path"]
            .as_str()
            .context("Provider omission path missing")?;
        ensure!(
            crate::paths::normalized_syntax(path)? == path,
            "Invalid provider omission path"
        );
        let file = files
            .get(path)
            .context("Provider omission is outside archived source manifest")?;
        ensure!(
            path > prior
                && paths.insert(path)
                && omission["contentHash"] == file.hash
                && omission["reason"]
                    == "workspace_execution_selector_omitted_from_private_provider_tree",
            "Provider omission source/order/reason mismatch"
        );
        prior = path;
    }
    if policy != "host_selected_provider_unattested_transitive_behavior" {
        ensure!(
            !authorized,
            "Passive provider context cannot claim execution permission"
        );
    }
    if matches!(
        policy,
        "passive_rust_source_selectors_and_inherited_cargo_config_excluded"
            | "contract_fixture_rust_selector_broker_only"
            | "passive_rust_json_project_selectors_excluded_v2"
            | "contract_fixture_rust_json_project_selectors_excluded_v2"
    ) {
        let selector = if matches!(
            policy,
            "passive_rust_json_project_selectors_excluded_v2"
                | "contract_fixture_rust_json_project_selectors_excluded_v2"
        ) {
            crate::lsp::is_rust_source_selector
        } else {
            crate::lsp::is_legacy_rust_source_selector
        };
        let expected: std::collections::BTreeSet<_> = snapshot
            .files
            .iter()
            .filter(|file| selector(&file.path))
            .map(|file| file.path.as_str())
            .collect();
        ensure!(
            paths == expected,
            "Passive provider omissions differ from exact captured selectors"
        );
    } else {
        ensure!(
            omissions.is_empty(),
            "Unattested provider context cannot invent passive omissions"
        );
    }
    deadline.check()
}
struct Builder<'a> {
    store: &'a Store,
    snapshot: &'a Snapshot,
    graph: &'a Graph,
    producer: &'a str,
    profile_hash: &'a str,
    context_hash: Option<&'a str>,
    deadline: &'a Deadline,
    sources: BTreeMap<String, (String, String)>,
    source_bytes: usize,
    nodes: BTreeMap<String, Value>,
    evidence: Vec<Value>,
    observations: Vec<Value>,
}
fn byte(value: &Value, key: &str) -> Result<usize> {
    usize::try_from(
        value[key]
            .as_u64()
            .context("Semantic byte anchor missing")?,
    )
    .context("Semantic byte anchor overflow")
}
impl Builder<'_> {
    fn bound_identity<T: Serialize>(&self, value: &T) -> String {
        if let Some(context) = self.context_hash {
            identity(&(value, context))
        } else {
            identity(value)
        }
    }
    fn source(&mut self, value: &Value) -> Result<(String, String)> {
        self.deadline.check()?;
        ensure!(
            value["snapshotId"] == self.snapshot.id,
            "Semantic anchor snapshot mismatch"
        );
        let path = value["path"]
            .as_str()
            .context("Semantic source path missing")?;
        let hash = value["contentHash"]
            .as_str()
            .context("Semantic source hash missing")?;
        if !self.sources.contains_key(path) {
            let source = query::source(self.store, self.snapshot, path)?;
            self.source_bytes = self.source_bytes.saturating_add(source.0.len());
            ensure!(
                self.source_bytes <= 32_000_000,
                "Semantic overlay source capacity exceeded"
            );
            self.sources.insert(path.into(), source);
        }
        let (text, stored) = &self.sources[path];
        ensure!(hash == stored, "Semantic anchor content hash mismatch");
        utf8_range(text, byte(value, "startByte")?, byte(value, "endByte")?)?;
        Ok((path.into(), stored.clone()))
    }
    fn node(&mut self, value: &Value) -> Result<String> {
        let (path, hash) = self.source(value)?;
        let start = byte(value, "startByte")?;
        let end = byte(value, "endByte")?;
        let selection_start = byte(value, "selectionStartByte")?;
        let selection_end = byte(value, "selectionEndByte")?;
        ensure!(
            selection_start >= start && selection_end <= end,
            "Semantic symbol selection escaped its range"
        );
        utf8_range(&self.sources[&path].0, selection_start, selection_end)?;
        let name = value["name"]
            .as_str()
            .context("Semantic symbol name missing")?;
        ensure!(
            !name.is_empty() && name.len() <= 1000,
            "Semantic symbol name exceeds budget"
        );
        let kind = value["kind"]
            .as_u64()
            .context("Semantic symbol kind missing")?;
        ensure!((1..=26).contains(&kind), "Invalid semantic symbol kind");
        let id = self.bound_identity(&(
            "semantic-overlay-node-v1",
            &self.snapshot.id,
            self.producer,
            self.profile_hash,
            &path,
            &hash,
            start,
            end,
            selection_start,
            selection_end,
            name,
            kind,
        ));
        let base_ids: Vec<_> = self
            .graph
            .entities
            .iter()
            .filter(|entity| {
                entity.path == path
                    && entity.name_start_byte == Some(selection_start)
                    && entity.name_end_byte == Some(selection_end)
            })
            .map(|entity| entity.id.as_str())
            .collect();
        let node = json!({"id":id,"snapshotId":self.snapshot.id,"path":path,"contentHash":hash,"startByte":start,"endByte":end,"selectionStartByte":selection_start,"selectionEndByte":selection_end,"name":name,"kind":kind,"baseEntityIds":base_ids,"producer":self.producer,"serverProfileHash":self.profile_hash,"origin":"semantic_overlay"});
        self.nodes.insert(id.clone(), node);
        ensure!(
            self.nodes.len() <= 4000,
            "Semantic overlay node capacity exceeded"
        );
        Ok(id)
    }
    fn arc(&mut self, source: &str, target: &str, relation: &str, anchor: &Value) -> Result<()> {
        let (path, hash) = self.source(anchor)?;
        let start = byte(anchor, "startByte")?;
        let end = byte(anchor, "endByte")?;
        let owner = &self.nodes[source];
        if relation == "calls" {
            ensure!(
                owner["path"] == path
                    && start >= byte(owner, "startByte")?
                    && end <= byte(owner, "endByte")?,
                "Call evidence escaped referring symbol"
            );
        }
        let ordinal = self.observations.len();
        ensure!(ordinal < 10000, "Semantic overlay edge capacity exceeded");
        let evidence_id = self.bound_identity(&(
            "semantic-overlay-evidence-v1",
            &self.snapshot.id,
            self.producer,
            self.profile_hash,
            &path,
            &hash,
            start,
            end,
            ordinal,
        ));
        self.evidence.push(json!({"id":evidence_id,"snapshotId":self.snapshot.id,"path":path,"contentHash":hash,"startByte":start,"endByte":end,"anchor":if relation=="calls"{"call_range"}else{"symbol_selection"},"producer":self.producer,"serverProfileHash":self.profile_hash,"trust":"source_bound_provider_claim"}));
        self.observations.push(json!({"id":self.bound_identity(&("semantic-overlay-edge-v1",&self.snapshot.id,self.producer,self.profile_hash,source,target,relation,&evidence_id,ordinal)),"snapshotId":self.snapshot.id,"source":source,"target":target,"relation":relation,"resolution":"exact_binding","evidenceIds":[evidence_id],"producer":self.producer,"serverProfileHash":self.profile_hash,"ordinal":ordinal}));
        Ok(())
    }
}
pub fn persist(
    store: &Store,
    snapshot: &Snapshot,
    graph: &Graph,
    result: &Value,
    deadline: &Deadline,
) -> Result<Value> {
    store.set_deadline(deadline)?;
    deadline.check()?;
    ensure!(
        snapshot.workspace_id == store.workspace_id
            && identity(&store.snapshot(&snapshot.id)?) == identity(snapshot)
            && graph.snapshot_id == snapshot.id
            && identity(&store.graph(&snapshot.id)?) == identity(graph),
        "Semantic overlay base manifest mismatch"
    );
    ensure!(
        result["snapshotId"] == snapshot.id && serde_json::to_vec(result)?.len() <= 4_000_000,
        "Semantic result snapshot or capacity mismatch"
    );
    let operation = result["operation"]
        .as_str()
        .context("Semantic operation missing")?;
    ensure!(
        matches!(
            operation,
            "incomingCalls" | "outgoingCalls" | "supertypes" | "subtypes"
        ),
        "Semantic operation has no graph overlay contract"
    );
    let producer = result["producer"]
        .as_str()
        .context("Semantic producer label missing")?;
    let profile_hash = result["serverProfileHash"]
        .as_str()
        .context("Semantic provider profile identity missing")?;
    ensure!(
        valid_id(profile_hash),
        "Invalid semantic provider profile identity"
    );
    ensure!(
        !producer.is_empty() && producer.len() <= 4096,
        "Semantic producer label exceeds capacity"
    );
    let has_context = [
        "sourceExecutionAuthorized",
        "providerSourcePolicy",
        "providerSourceOmissions",
    ]
    .iter()
    .any(|key| result.get(*key).is_some());
    let provider_context = if has_context {
        let value = json!({"sourceExecutionAuthorized":result["sourceExecutionAuthorized"],"sourcePolicy":result["providerSourcePolicy"],"omissions":result["providerSourceOmissions"]});
        validate_provider_context(store, snapshot, &value, deadline)?;
        Some(value)
    } else {
        None
    };
    let context_hash = provider_context
        .as_ref()
        .map(|context| identity(&(profile_hash, context)));
    let items = result["items"]
        .as_array()
        .context("Semantic items missing")?;
    ensure!(
        items.len() <= 2000,
        "Semantic overlay item capacity exceeded"
    );
    let mut builder = Builder {
        store,
        snapshot,
        graph,
        producer,
        profile_hash,
        context_hash: context_hash.as_deref(),
        deadline,
        sources: BTreeMap::new(),
        source_bytes: 0,
        nodes: BTreeMap::new(),
        evidence: Vec::new(),
        observations: Vec::new(),
    };
    for item in items {
        deadline.check()?;
        let root = builder.node(&item["root"])?;
        match operation {
            "incomingCalls" | "outgoingCalls" => {
                let key = if operation == "incomingCalls" {
                    "from"
                } else {
                    "to"
                };
                let other = builder.node(&item[key])?;
                let (source, target) = if operation == "incomingCalls" {
                    (&other, &root)
                } else {
                    (&root, &other)
                };
                let ranges = item["fromRanges"]
                    .as_array()
                    .context("Call occurrence ranges missing")?;
                ensure!(ranges.len() <= 10000, "Call occurrence capacity exceeded");
                for anchor in ranges {
                    builder.arc(source, target, "calls", anchor)?;
                }
            }
            _ => {
                let other = builder.node(item)?;
                let (source, target) = if operation == "supertypes" {
                    (&root, &other)
                } else {
                    (&other, &root)
                };
                let mut anchor = item.clone();
                anchor["startByte"] = item["selectionStartByte"].clone();
                anchor["endByte"] = item["selectionEndByte"].clone();
                builder.arc(source, target, "inherits", &anchor)?;
            }
        }
    }
    let bindings: BTreeMap<_, _> = builder
        .sources
        .iter()
        .map(|(path, (_, hash))| (path.clone(), hash.clone()))
        .collect();
    let graph_hash = identity(graph);
    let mut overlay = json!({"schemaVersion":1,"snapshotId":snapshot.id,"baseGraphHash":graph_hash,"sourceBindings":bindings,"operation":operation,"producer":producer,"serverProfileHash":result["serverProfileHash"],"nodes":builder.nodes.into_values().collect::<Vec<_>>(),"evidence":builder.evidence,"observations":builder.observations,"complete":false,"providerAttestation":"unverified","limitations":["Exact source bindings do not attest provider correctness or exhaustive semantic/runtime coverage.","This overlay never overwrites the published baseline graph."]});
    let kind = if let Some(context) = provider_context {
        overlay["schemaVersion"] = json!(2);
        overlay["providerContext"] = context;
        overlay["limitations"].as_array_mut().context("Overlay limitations missing")?.push(json!("Host permission and passive materialization context do not attest actual provider/runtime effects."));
        RECORD_KIND_V2
    } else {
        RECORD_KIND
    };
    let id = identity(&overlay);
    overlay["overlayId"] = json!(id);
    let bytes = serde_json::to_vec(&overlay)?;
    ensure!(
        bytes.len() <= 8_000_000,
        "Semantic overlay serialized capacity exceeded"
    );
    deadline.check()?;
    // CAS bytes can remain unregistered after failure. Readers only discover
    // them after both immutable records commit in the same SQLite transaction.
    let artifact_id = store.blob(&bytes)?;
    let record = Record {
        overlay_id: id.clone(),
        artifact_id: artifact_id.clone(),
        snapshot_id: snapshot.id.clone(),
        base_graph_hash: graph_hash,
        source_bindings: bindings,
        provider_context_hash: context_hash,
        server_profile_hash: if has_context {
            Some(profile_hash.into())
        } else {
            None
        },
    };
    let transaction =
        rusqlite::Transaction::new_unchecked(&store.db, TransactionBehavior::Immediate)?;
    for (kind, key, value) in [
        (
            "artifact",
            artifact_id.as_str(),
            json!({"artifactId":artifact_id,"totalBytes":bytes.len()}),
        ),
        (kind, id.as_str(), serde_json::to_value(&record)?),
    ] {
        deadline.check()?;
        let existing: Option<String> = transaction
            .query_row(
                "SELECT value FROM records WHERE kind=?1 AND id=?2",
                params![kind, key],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(existing) = existing {
            ensure!(
                serde_json::from_str::<Value>(&existing)? == value,
                "Immutable semantic publication record collision"
            );
        } else {
            transaction.execute(
                "INSERT INTO records VALUES(?1,?2,?3)",
                params![kind, key, serde_json::to_string(&value)?],
            )?;
        }
    }
    deadline.check()?;
    transaction.commit()?;
    Ok(
        json!({"overlayId":id,"artifactId":artifact_id,"snapshotId":snapshot.id,"nodeCount":overlay["nodes"].as_array().unwrap().len(),"observationCount":overlay["observations"].as_array().unwrap().len(),"complete":false}),
    )
}
pub fn load(store: &Store, id: &str, deadline: &Deadline) -> Result<Value> {
    store.set_deadline(deadline)?;
    deadline.check()?;
    ensure!(valid_id(id), "Invalid semantic overlay ID");
    let record: Record = crate::json::decode(record_for_overlay(store, id, deadline)?)?;
    let version = if record.provider_context_hash.is_some() {
        2
    } else {
        1
    };
    ensure!(
        record.overlay_id == id && valid_id(&record.artifact_id),
        "Semantic overlay record identity mismatch"
    );
    let snapshot = store.snapshot(&record.snapshot_id)?;
    let graph = store.graph(&record.snapshot_id)?;
    ensure!(
        snapshot.workspace_id == store.workspace_id,
        "Semantic overlay workspace mismatch"
    );
    ensure!(
        identity(&graph) == record.base_graph_hash,
        "Semantic overlay graph identity mismatch"
    );
    for (path, hash) in &record.source_bindings {
        ensure!(
            snapshot
                .files
                .iter()
                .any(|file| &file.path == path && &file.hash == hash),
            "Semantic overlay source identity mismatch"
        );
    }
    let registered: Value = store.get_record("artifact", &record.artifact_id)?;
    ensure!(
        registered["artifactId"] == record.artifact_id
            && registered["totalBytes"]
                .as_u64()
                .is_some_and(|bytes| bytes > 0 && bytes <= 8_000_000),
        "Semantic artifact registered capacity invalid"
    );
    let bytes = store.read_artifact(&record.artifact_id)?;
    let mut value = crate::json::parse(&bytes, 8_000_000)?;
    ensure!(
        value
            .as_object()
            .is_some_and(|object| object.len() == if version == 2 { 15 } else { 14 }
                && object.keys().all(|key| [
                    "schemaVersion",
                    "snapshotId",
                    "baseGraphHash",
                    "sourceBindings",
                    "operation",
                    "producer",
                    "serverProfileHash",
                    "nodes",
                    "evidence",
                    "observations",
                    "complete",
                    "providerAttestation",
                    "limitations",
                    "overlayId",
                    "providerContext"
                ]
                .contains(&key.as_str()))),
        "Unknown semantic overlay field"
    );
    ensure!(
        value["schemaVersion"] == version
            && (version == 2 || value.get("providerContext").is_none()),
        "Semantic overlay kind/schema mismatch"
    );
    if version == 2 {
        ensure!(
            record
                .server_profile_hash
                .as_ref()
                .is_some_and(|profile| value["serverProfileHash"] == *profile)
                && record
                    .provider_context_hash
                    .as_ref()
                    .is_some_and(|hash| *hash
                        == identity(&(&value["serverProfileHash"], &value["providerContext"]))),
            "Semantic provider context/profile record binding mismatch"
        );
    }
    ensure!(
        value["overlayId"] == id
            && value["snapshotId"] == snapshot.id
            && value["baseGraphHash"] == record.base_graph_hash
            && value["sourceBindings"] == json!(record.source_bindings),
        "Semantic overlay artifact binding mismatch"
    );
    value
        .as_object_mut()
        .context("Semantic overlay object missing")?
        .remove("overlayId");
    ensure!(
        identity(&value) == id,
        "Semantic overlay content identity mismatch"
    );
    value["overlayId"] = json!(id);
    validate_loaded(store, &snapshot, &graph, &value, deadline)?;
    deadline.check()?;
    Ok(value)
}

fn validate_loaded(
    store: &Store,
    snapshot: &Snapshot,
    graph: &Graph,
    value: &Value,
    deadline: &Deadline,
) -> Result<()> {
    let operation = value["operation"]
        .as_str()
        .context("Semantic overlay operation missing")?;
    ensure!(
        matches!(
            operation,
            "incomingCalls" | "outgoingCalls" | "supertypes" | "subtypes"
        ) && matches!(value["schemaVersion"].as_u64(), Some(1 | 2))
            && value["complete"] == false
            && value["providerAttestation"] == "unverified",
        "Invalid semantic overlay schema"
    );
    let producer = value["producer"]
        .as_str()
        .filter(|producer| !producer.is_empty() && producer.len() <= 4096)
        .context("Invalid semantic overlay producer")?;
    let profile_hash = value["serverProfileHash"]
        .as_str()
        .filter(|profile| valid_id(profile))
        .context("Invalid semantic overlay profile")?;
    let mut limitations = json!([
        "Exact source bindings do not attest provider correctness or exhaustive semantic/runtime coverage.",
        "This overlay never overwrites the published baseline graph."
    ]);
    let context_hash = if value["schemaVersion"] == 2 {
        validate_provider_context(store, snapshot, &value["providerContext"], deadline)?;
        limitations.as_array_mut().context("Limitations missing")?.push(json!("Host permission and passive materialization context do not attest actual provider/runtime effects."));
        Some(identity(&(profile_hash, &value["providerContext"])))
    } else {
        None
    };
    ensure!(
        value["limitations"] == limitations,
        "Unknown semantic overlay limitation contract"
    );
    let nodes = value["nodes"]
        .as_array()
        .context("Semantic overlay nodes missing")?;
    let evidence = value["evidence"]
        .as_array()
        .context("Semantic overlay evidence missing")?;
    let arcs = value["observations"]
        .as_array()
        .context("Semantic overlay observations missing")?;
    ensure!(
        nodes.len() <= 4000 && evidence.len() <= 10000 && arcs.len() <= 10000,
        "Semantic overlay collection capacity exceeded"
    );
    let mut builder = Builder {
        store,
        snapshot,
        graph,
        producer,
        profile_hash,
        context_hash: context_hash.as_deref(),
        deadline,
        sources: BTreeMap::new(),
        source_bytes: 0,
        nodes: BTreeMap::new(),
        evidence: vec![],
        observations: vec![],
    };
    for node in nodes {
        deadline.check()?;
        let id = builder.node(node)?;
        ensure!(
            &builder.nodes[&id] == node,
            "Semantic node identity, source, or canonical bindings differ"
        );
    }
    ensure!(
        builder.nodes.len() == nodes.len(),
        "Duplicate semantic node identity"
    );
    let mut anchors = BTreeMap::new();
    for anchor in evidence {
        deadline.check()?;
        let id = anchor["id"]
            .as_str()
            .context("Semantic evidence identity missing")?;
        ensure!(
            valid_id(id) && anchors.insert(id, anchor).is_none(),
            "Invalid or duplicate semantic evidence identity"
        );
    }
    for arc in arcs {
        deadline.check()?;
        let source = arc["source"]
            .as_str()
            .filter(|id| builder.nodes.contains_key(*id))
            .context("Semantic source node absent")?;
        let target = arc["target"]
            .as_str()
            .filter(|id| builder.nodes.contains_key(*id))
            .context("Semantic target node absent")?;
        let relation = if matches!(operation, "incomingCalls" | "outgoingCalls") {
            "calls"
        } else {
            "inherits"
        };
        ensure!(
            arc["relation"] == relation,
            "Semantic operation relation mismatch"
        );
        let ids = arc["evidenceIds"]
            .as_array()
            .filter(|ids| ids.len() == 1)
            .context("Semantic edge requires one source occurrence")?;
        let anchor = ids[0]
            .as_str()
            .and_then(|id| anchors.get(id))
            .context("Semantic edge evidence absent")?;
        builder.arc(source, target, relation, anchor)?;
    }
    let bindings: BTreeMap<_, _> = builder
        .sources
        .iter()
        .map(|(path, (_, hash))| (path.clone(), hash.clone()))
        .collect();
    ensure!(
        value["nodes"] == json!(builder.nodes.into_values().collect::<Vec<_>>())
            && value["evidence"] == json!(builder.evidence)
            && value["observations"] == json!(builder.observations)
            && value["sourceBindings"] == json!(bindings),
        "Semantic overlay rows differ from validated source claims"
    );
    deadline.check()
}
