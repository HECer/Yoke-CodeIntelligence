//! Explicit host-only history retirement and irreversible committed-CAS purge.
use crate::{
    capture::Deadline,
    model::{Extraction, Graph, Snapshot, hash, identity, valid_id},
    paths::is_link,
    store::Store,
};
use anyhow::{Context, Result, ensure};
use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_ROWS: usize = 200_000;
const MAX_MANIFEST: u64 = 128_000_000;
#[derive(Clone, Debug)]
pub struct Policy {
    pub minimum_age_ms: u64,
    pub expires_after_ms: u64,
}
impl Default for Policy {
    fn default() -> Self {
        Self {
            minimum_age_ms: 0,
            expires_after_ms: 600_000,
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HistoryPlan {
    schema_version: u32,
    workspace_id: String,
    created_ms: u64,
    expires_ms: u64,
    minimum_age_ms: u64,
    inventory_hash: String,
    rows_hash: String,
    snapshot_ids: Vec<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Row {
    id: String,
    value: String,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HistoryManifest {
    schema_version: u32,
    plan: HistoryPlan,
    snapshots: Vec<Row>,
    graphs: Vec<Row>,
    symbols: Vec<Vec<String>>,
    documents: Vec<Vec<String>>,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HistoryReceipt {
    schema_version: u32,
    workspace_id: String,
    manifest_hash: String,
    manifest_text: String,
    result: Value,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PurgeItem {
    run_id: String,
    blob_id: String,
    bytes: u64,
    created_ms: u64,
    manifest_hash: String,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PurgePlan {
    schema_version: u32,
    workspace_id: String,
    created_ms: u64,
    expires_ms: u64,
    minimum_age_ms: u64,
    inventory_hash: String,
    items: Vec<PurgeItem>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Progress {
    schema_version: u32,
    workspace_id: String,
    plan_id: String,
    blob_ids: Vec<String>,
    removed: Vec<String>,
    deleting: Option<String>,
    reclaimed_bytes: u64,
    unverified_removal_bytes: u64,
    complete: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CasManifest {
    schema_version: u32,
    id: String,
    workspace_id: String,
    created_ms: u64,
    blob_ids: Vec<String>,
}
#[derive(Default)]
struct Scan {
    snapshots: BTreeMap<String, String>,
    graphs: BTreeMap<String, String>,
    references: BTreeSet<String>,
    all_references: BTreeSet<String>,
    unknown: BTreeSet<String>,
    inventory: Vec<(String, String, String)>,
    published: BTreeMap<String, u64>,
}

fn now_ms() -> Result<u64> {
    Ok(u64::try_from(
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis(),
    )?)
}
fn directory(path: &Path, create: bool) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(meta) => {
            ensure!(
                meta.is_dir() && !is_link(&meta),
                "Unsafe retention directory"
            );
            Ok(true)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if create {
                fs::create_dir(path)?;
                directory(path, false)?;
            }
            Ok(create)
        }
        Err(e) => Err(e.into()),
    }
}
fn leaf(path: &Path) -> Result<fs::Metadata> {
    let meta = fs::symlink_metadata(path)?;
    ensure!(
        meta.is_file() && !is_link(&meta),
        "Retention file/link/reparse entry refused"
    );
    Ok(meta)
}
fn checked_hash(path: &Path, expected: &str, deadline: &Deadline) -> Result<u64> {
    deadline.check()?;
    let size = leaf(path)?.len();
    let mut file = fs::File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        deadline.check()?;
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    ensure!(
        hex::encode(digest.finalize()) == expected,
        "Retention content hash mismatch"
    );
    Ok(size)
}
fn single_link(path: &Path) -> Result<()> {
    leaf(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        ensure!(
            fs::metadata(path)?.nlink() == 1,
            "Multiply-linked quarantine content is not eligible for physical reclamation"
        );
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        #[repr(C)]
        struct FileInformation {
            attributes: u32,
            created: [u32; 2],
            accessed: [u32; 2],
            written: [u32; 2],
            volume: u32,
            size_high: u32,
            size_low: u32,
            links: u32,
            index_high: u32,
            index_low: u32,
        }
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetFileInformationByHandle(
                handle: *mut std::ffi::c_void,
                information: *mut FileInformation,
            ) -> i32;
        }
        let file = fs::File::open(path)?;
        let mut information = FileInformation {
            attributes: 0,
            created: [0; 2],
            accessed: [0; 2],
            written: [0; 2],
            volume: 0,
            size_high: 0,
            size_low: 0,
            links: 0,
            index_high: 0,
            index_low: 0,
        };
        // The handle remains live and the correctly sized C layout is writable
        // for this synchronous documented Win32 information call.
        ensure!(
            unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut information) } != 0,
            "Unable to verify quarantine hard-link count: {}",
            std::io::Error::last_os_error()
        );
        ensure!(
            information.links == 1,
            "Multiply-linked quarantine content is not eligible for physical reclamation"
        );
    }
    #[cfg(not(any(windows, unix)))]
    anyhow::bail!("Physical quarantine reclamation is unavailable on this platform");
    Ok(())
}
fn references(value: &Value, out: &mut BTreeSet<String>, deadline: &Deadline) -> Result<()> {
    let mut queue = vec![value];
    let mut count = 0;
    while let Some(value) = queue.pop() {
        count += 1;
        ensure!(count <= 2_000_000, "Retention dependency capacity exceeded");
        if count % 512 == 0 {
            deadline.check()?;
        }
        match value {
            Value::String(s) if valid_id(s) => {
                out.insert(s.clone());
            }
            Value::Array(a) => queue.extend(a),
            Value::Object(o) => {
                for (k, v) in o {
                    if valid_id(k) {
                        out.insert(k.clone());
                    }
                    queue.push(v);
                }
            }
            _ => {}
        }
    }
    Ok(())
}
fn json_value(raw: &str) -> Result<Value> {
    ensure!(
        raw.len() <= MAX_MANIFEST as usize,
        "Retention metadata capacity exceeded"
    );
    crate::json::parse(raw.as_bytes(), MAX_MANIFEST as usize)
        .context("Corrupt persisted metadata; retention refused")
}
fn known_kind(kind: &str) -> bool {
    [
        "plan",
        "approval",
        "journal",
        "receipt",
        "verification",
        "syntax-verification",
        "snapshot-pin",
        "editor-session",
        "overlay-snapshot",
        "artifact",
        "native_knowledge_v1",
        "native_vectors_v1",
        "native_local_tfidf_v1",
        "native_coverage_v1",
        "native_runtime_v1",
        "native_sarif_v1",
        "native_media_asset_v1",
        "native_test_map_v1",
        "native_model_attempt_v1",
        "native_model_task_v1",
        "native_compiler_report_v1",
        "native_federation_receipt_v1",
        "native_federation_graph_v1",
        "native_federation_replay_v1",
        "maintenance-committed",
        "maintenance-restored",
        "retention-plan",
        "retention-committed",
        "retention-restored",
        "purge-plan",
        "purge-intent",
        "purge-completed",
        "snapshot-published",
    ]
    .contains(&kind)
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReportRecord {
    snapshot_id: String,
    workspace_id: String,
    graph_hash: String,
    previous_snapshot_id: Option<String>,
    artifact_id: String,
    markdown_artifact_id: String,
    options: crate::graph_extensions::AnalysisOptions,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ExportRecord {
    snapshot_id: String,
    workspace_id: String,
    graph_hash: String,
    artifact_id: String,
    format: crate::graph_exports::ExportFormat,
    options: crate::graph_exports::ExportOptions,
}
fn fields_only(value: &Value, allowed: &[&str]) -> Result<()> {
    ensure!(
        value
            .as_object()
            .is_some_and(|object| object.keys().all(|key| allowed.contains(&key.as_str()))),
        "Unknown retained artifact field"
    );
    Ok(())
}
fn report_anchor(snapshot: &Snapshot, graph: &Graph, value: &Value) -> Result<()> {
    let entity = value["entityId"]
        .as_str()
        .and_then(|id| graph.entities.iter().find(|entity| entity.id == id))
        .context("Retained report anchor entity absent")?;
    let hash = snapshot
        .files
        .iter()
        .find(|file| file.path == entity.path)
        .map(|file| file.hash.as_str());
    let source = json!({"path":entity.path,"contentHash":hash,"startByte":entity.start_byte,"endByte":entity.end_byte,"startLine":entity.start_line,"endLine":entity.end_line,"entityId":entity.id,"anchor":"exact"});
    let community = json!({"entityId":entity.id,"path":entity.path,"startByte":entity.start_byte,"endByte":entity.end_byte,"startLine":entity.start_line,"endLine":entity.end_line,"snapshotId":graph.snapshot_id,"anchor":"exact"});
    ensure!(
        value == &source || value == &community,
        "Retained report anchor mismatch"
    );
    Ok(())
}
fn report_shape(
    store: &Store,
    snapshot: &Snapshot,
    graph: &Graph,
    report: &Value,
    record: &ReportRecord,
    deadline: &Deadline,
) -> Result<()> {
    fields_only(
        report,
        &[
            "producer",
            "workspaceId",
            "snapshotId",
            "graphHash",
            "previousSnapshotId",
            "previousGraphHash",
            "options",
            "sourceBindings",
            "evidenceAnchors",
            "audit",
            "godNodes",
            "communities",
            "clustering",
            "surprises",
            "questions",
            "diff",
            "modelCalls",
            "complete",
            "limitations",
        ],
    )?;
    ensure!(
        report["producer"] == "yoke-graph-report-v1"
            && report["workspaceId"] == snapshot.workspace_id
            && report["snapshotId"] == snapshot.id
            && report["graphHash"] == identity(graph)
            && report["options"] == serde_json::to_value(&record.options)?
            && report["previousSnapshotId"] == json!(record.previous_snapshot_id)
            && report["modelCalls"] == 0
            && report["complete"] == false
            && report["limitations"].is_string(),
        "Retained report scope mismatch"
    );
    let bindings = report["sourceBindings"]
        .as_array()
        .context("Report source bindings absent")?;
    ensure!(
        bindings.len() == graph.entities.len(),
        "Report source binding count mismatch"
    );
    for (entity, binding) in graph.entities.iter().zip(bindings) {
        deadline.check()?;
        ensure!(
            binding["entityId"] == entity.id,
            "Report binding order mismatch"
        );
        report_anchor(snapshot, graph, binding)?;
    }
    ensure!(
        report["evidenceAnchors"] == serde_json::to_value(&graph.evidence)?,
        "Report evidence differs from source graph"
    );
    let mut resolutions = BTreeMap::<&str, usize>::new();
    let mut relations = BTreeMap::<&str, usize>::new();
    for observation in &graph.observations {
        deadline.check()?;
        *resolutions.entry(&observation.resolution).or_default() += 1;
        *relations.entry(&observation.relation).or_default() += 1;
    }
    ensure!(
        report["audit"]
            == json!({"totalEntities":graph.entities.len(),"totalObservations":graph.observations.len(),"byResolution":resolutions,"byRelation":relations,"unresolvedObservations":graph.observations.iter().filter(|item|item.target.is_none()).count(),"omissions":snapshot.omissions,"coverage":graph.coverage,"confidenceMeaning":"source_and_analysis_categories_not_probabilities"}),
        "Report audit source mismatch"
    );
    fields_only(
        &report["clustering"],
        &[
            "algorithm",
            "producer",
            "resolution",
            "seed",
            "maxIterations",
            "maxCommunitySize",
            "uncertainWeight",
            "excludeHubsPercentile",
            "excludedHubs",
            "quality",
            "projection",
            "cancellation",
            "hubReattachment",
        ],
    )?;
    let cluster = &report["clustering"];
    ensure!(
        cluster["quality"].is_null() || cluster["quality"].as_f64().is_some_and(f64::is_finite),
        "Invalid report quality type"
    );
    ensure!(
        cluster["projection"]
            == "undirected_weighted_resolved_observations_parallel_weight_retained"
            && cluster["cancellation"] == "before_and_after_bounded_library_call_not_inside_leiden"
            && cluster["hubReattachment"]
                == "weighted_majority_then_standalone_may_exceed_max_community_size",
        "Unknown report clustering contract"
    );
    ensure!(
        cluster["algorithm"] == "leiden"
            && cluster["producer"] == "leiden-rs-0.8.1"
            && cluster["resolution"] == json!(record.options.resolution)
            && cluster["seed"] == record.options.seed
            && cluster["maxIterations"] == record.options.max_iterations
            && cluster["maxCommunitySize"] == record.options.max_community_size
            && cluster["uncertainWeight"] == json!(record.options.uncertain_weight)
            && cluster["excludeHubsPercentile"] == json!(record.options.exclude_hubs_percentile),
        "Report clustering options mismatch"
    );
    ensure!(
        cluster["excludedHubs"]
            .as_array()
            .is_some_and(|items| items.iter().all(|id| id
                .as_str()
                .is_some_and(|id| graph.entities.iter().any(|entity| entity.id == id)))),
        "Invalid retained excluded hubs"
    );
    for (section, allowed, maximum) in [
        (
            "godNodes",
            vec!["entityId", "label", "degree", "degreeDefinition", "anchor"],
            record.options.hub_limit,
        ),
        (
            "surprises",
            vec![
                "observationId",
                "source",
                "target",
                "relation",
                "resolution",
                "reason",
                "rankingScore",
                "evidenceIds",
                "anchor",
            ],
            record.options.surprise_limit,
        ),
        (
            "questions",
            vec!["kind", "question", "observationId", "anchor"],
            record.options.question_limit,
        ),
        (
            "communities",
            vec![
                "id",
                "members",
                "stableMemberKeys",
                "identityMethod",
                "label",
                "labelProducer",
                "cohesion",
                "cohesionProjection",
                "anchor",
                "previousCommunityId",
                "previousMemberOverlap",
                "previousMatchAmbiguous",
            ],
            graph.entities.len(),
        ),
    ] {
        let items = report[section]
            .as_array()
            .context("Retained report result section absent")?;
        ensure!(items.len() <= maximum, "Report result capacity exceeded");
        for item in items {
            deadline.check()?;
            fields_only(item, &allowed)?;
            report_anchor(snapshot, graph, &item["anchor"])?;
            if section == "godNodes" {
                ensure!(
                    item["label"].is_string()
                        && item["degree"].is_u64()
                        && item["degreeDefinition"]
                            == "resolved_parallel_in_plus_out_self_loop_twice"
                        && item["entityId"] == item["anchor"]["entityId"],
                    "Invalid report hub schema"
                );
            } else if section == "questions" {
                ensure!(
                    item["question"].is_string()
                        && matches!(
                            item["kind"].as_str(),
                            Some("candidate" | "unresolved" | "bridge" | "isolation")
                        ),
                    "Invalid report question schema"
                );
            } else if section == "surprises" {
                ensure!(
                    item["rankingScore"]
                        .as_u64()
                        .is_some_and(|score| (1..=3).contains(&score))
                        && matches!(
                            item["reason"].as_str(),
                            Some("cross_file_candidate" | "cross_community_candidate")
                        ),
                    "Invalid report surprise schema"
                );
            }
            ensure!(
                item.get("observationId")
                    .is_none_or(|id| id.as_str().is_some_and(valid_id)),
                "Invalid retained report observation ID"
            );
            if let Some(id) = item["observationId"].as_str() {
                let observation = graph
                    .observations
                    .iter()
                    .find(|value| value.id == id)
                    .context("Report observation absent")?;
                if section == "surprises" {
                    ensure!(
                        item["source"] == observation.source
                            && item["target"] == json!(observation.target)
                            && item["relation"] == observation.relation
                            && item["resolution"] == observation.resolution
                            && item["evidenceIds"] == json!(observation.evidence_ids),
                        "Report observation source mismatch"
                    );
                }
            }
            if section == "communities" {
                ensure!(
                    item["label"].is_string()
                        && item["labelProducer"] == "deterministic_source_hubs"
                        && matches!(
                            item["identityMethod"].as_str(),
                            Some("ambiguous_members_snapshot_scoped" | "unique_structural_members")
                        )
                        && item["cohesion"]
                            .as_f64()
                            .is_some_and(|value| value.is_finite() && (0.0..=1.0).contains(&value))
                        && item["cohesionProjection"] == "unique_undirected_positive_weight_pairs"
                        && item
                            .get("previousCommunityId")
                            .is_none_or(|id| id.as_str().is_some_and(valid_id))
                        && item.get("previousMemberOverlap").is_none_or(|value| value
                            .as_f64()
                            .is_some_and(|value| (0.0..=1.0).contains(&value)))
                        && item
                            .get("previousMatchAmbiguous")
                            .is_none_or(Value::is_boolean),
                    "Invalid retained community schema"
                );
                ensure!(
                    item["id"].as_str().is_some_and(valid_id)
                        && item["members"]
                            .as_array()
                            .is_some_and(|members| members.iter().all(|id| id
                                .as_str()
                                .is_some_and(|id| graph
                                    .entities
                                    .iter()
                                    .any(|entity| entity.id == id))))
                        && item["stableMemberKeys"]
                            .as_array()
                            .is_some_and(|keys| keys.iter().all(Value::is_string)),
                    "Invalid retained community members"
                );
            }
        }
    }
    if let Some(previous) = &record.previous_snapshot_id {
        let old = store.snapshot(previous)?;
        let old_graph = store.graph(previous)?;
        ensure!(
            report["previousGraphHash"] == identity(&old_graph)
                && report["diff"]
                    == crate::graph_extensions::compare(
                        store, &old, &old_graph, snapshot, graph, deadline
                    )?,
            "Report previous source graph mismatch"
        );
    } else {
        ensure!(
            report["diff"].is_null() && report["previousGraphHash"].is_null(),
            "Unexpected report previous graph"
        );
    }
    Ok(())
}
fn parity_artifact(store: &Store, id: &str, deadline: &Deadline) -> Result<Vec<u8>> {
    deadline.check()?;
    ensure!(valid_id(id), "Invalid parity artifact ID");
    let path = store.dir.join("blobs").join(id);
    ensure!(
        leaf(&path)?.len() <= 64_000_000,
        "Parity artifact capacity exceeded"
    );
    checked_hash(&path, id, deadline)?;
    store.read_artifact(id)
}
fn validate_report_export(
    store: &Store,
    kind: &str,
    id: &str,
    value: &Value,
    deadline: &Deadline,
) -> Result<Vec<String>> {
    if kind == crate::graph_extensions::REPORT_KIND {
        let record: ReportRecord = serde_json::from_value(value.clone())?;
        ensure!(
            serde_json::to_value(&record)? == *value
                && record.artifact_id == id
                && record.workspace_id == store.workspace_id
                && record.options.previous_snapshot_id == record.previous_snapshot_id,
            "Invalid report retention record"
        );
        crate::graph_extensions::validate_options(&record.options)?;
        let snapshot = store.snapshot(&record.snapshot_id)?;
        let graph = store.graph(&record.snapshot_id)?;
        crate::graph_extensions::validate_scope(store, &snapshot, &graph, deadline)?;
        ensure!(
            record.graph_hash == identity(&graph),
            "Report base graph mismatch"
        );
        let bytes = parity_artifact(store, id, deadline)?;
        let report = crate::json::parse(&bytes, 64_000_000)?;
        ensure!(
            serde_json::to_vec(&report)? == bytes,
            "Noncanonical retained report"
        );
        report_shape(store, &snapshot, &graph, &report, &record, deadline)?;
        ensure!(
            parity_artifact(store, &record.markdown_artifact_id, deadline)?
                == crate::graph_extensions::markdown(&report).as_bytes(),
            "Report Markdown content mismatch"
        );
        Ok(vec![id.into(), record.markdown_artifact_id])
    } else {
        let record: ExportRecord = serde_json::from_value(value.clone())?;
        ensure!(
            serde_json::to_value(&record)? == *value
                && record.artifact_id == id
                && record.workspace_id == store.workspace_id
                && record.options.format == record.format
                && (1..=5000).contains(&record.options.max_nodes)
                && (1..=50000).contains(&record.options.max_edges),
            "Invalid export retention record"
        );
        let snapshot = store.snapshot(&record.snapshot_id)?;
        let graph = store.graph(&record.snapshot_id)?;
        crate::graph_extensions::validate_scope(store, &snapshot, &graph, deadline)?;
        ensure!(
            record.graph_hash == identity(&graph),
            "Export base graph mismatch"
        );
        let bytes = parity_artifact(store, id, deadline)?;
        let value = crate::json::parse(&bytes, 64_000_000)?;
        let mut bundle: crate::graph_exports::ExportBundle = serde_json::from_value(value.clone())?;
        ensure!(
            serde_json::to_value(&bundle)? == value
                && serde_json::to_vec(&bundle)? == bytes
                && bundle.artifact_id.is_empty()
                && bundle.snapshot_id == record.snapshot_id
                && bundle.workspace_id == record.workspace_id
                && bundle.graph_hash == record.graph_hash
                && bundle.format == record.format,
            "Export artifact scope mismatch"
        );
        bundle.artifact_id = id.into();
        crate::graph_exports::validate_bundle(&bundle, deadline)?;
        Ok(vec![id.into()])
    }
}
type ParityDependencies = (
    BTreeSet<(String, String)>,
    BTreeSet<String>,
    BTreeSet<String>,
    BTreeSet<String>,
);
fn add_meaning_dependencies(
    store: &Store,
    roots: crate::meaning_build::MeaningDependencies,
    artifacts: &mut BTreeSet<String>,
    typed_references: &mut BTreeSet<String>,
    deadline: &Deadline,
) -> Result<()> {
    artifacts.extend(roots.artifacts);
    typed_references.extend(roots.snapshots);
    for (record_kind, record_id) in roots.records {
        deadline.check()?;
        ensure!(
            store.db.query_row(
                "SELECT EXISTS(SELECT 1 FROM records WHERE kind=?1 AND id=?2)",
                params![record_kind, record_id],
                |row| row.get::<_, bool>(0),
            )?,
            "Retained meaning dependency record is absent"
        );
        typed_references.insert(record_id);
    }
    Ok(())
}
fn validate_parity_records(store: &Store, deadline: &Deadline) -> Result<ParityDependencies> {
    let mut recognized = BTreeSet::new();
    let mut artifacts = BTreeSet::new();
    let mut federation_namespaces = BTreeSet::new();
    let mut typed_references = BTreeSet::new();
    let mut statement = store
        .db
        .prepare("SELECT kind,id,value FROM records ORDER BY kind,id")?;
    for (count, row) in statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .enumerate()
    {
        deadline.check()?;
        ensure!(count < MAX_ROWS, "Retention row capacity exceeded");
        let (kind, id, raw) = row?;
        match kind.as_str() {
            crate::github_app::RECORD_KIND
            | crate::github_app::worker::RECORD_KIND
            | crate::github_app::worker::history::RECORD_KIND
            | crate::github_app::publication::RECORD_KIND
            | crate::github_app::digest::service::RECORD_KIND
            | crate::github_app::digest::service::receiver::RECORD_KIND
            | crate::github_app::digest::history_jobs::RECORD_KIND
            | crate::github_app::pages::RECORD_KIND
            | crate::github_app::pages::REVISION_KIND => {
                // scan validates the complete closed inline history before this pass.
            }
            crate::knowledge::feedback::EVENT_KIND
            | crate::knowledge::feedback::TRANSCRIPT_KIND
            | crate::knowledge::feedback::plans::EVENT_KIND
            | crate::knowledge::feedback::meaning::EVENT_KIND
            | crate::knowledge::feedback::semantic_transcript::JOB_KIND => {
                // scan validates the complete feedback set once before this pass.
            }
            crate::decision_records::REVISION_KIND
            | crate::decision_records::HEAD_KIND
            | crate::decision_records::HOST_KIND
            | crate::decision_records::PIN_KIND => {
                // The complete authored revision/head graph is validated once
                // by scan before any of these rows can be recognized.
            }
            "native_model_batch_v1" => {
                artifacts.extend(crate::provider_batches::validate_retained_batch(
                    store,
                    &id,
                    &json_value(&raw)?,
                    deadline,
                )?)
            }
            "native_federation_receipt_v1" | crate::federation::RECEIPT_V2 => {
                artifacts.extend(crate::federation::validate_retention_receipt(
                    store,
                    &kind,
                    &id,
                    &json_value(&raw)?,
                    deadline,
                )?);
                federation_namespaces.insert(id.clone());
            }
            crate::skill_takeaways::REVISION_KIND
            | crate::skill_takeaways::HEAD_KIND
            | crate::skill_takeaways::OPERATION_KIND
            | crate::skill_takeaways::RECEIPT_KIND => artifacts.extend(
                crate::skill_takeaways::validate_retention_record(
                    store,
                    &kind,
                    &id,
                    &json_value(&raw)?,
                    deadline,
                )?
                .context("Skill retention record was not validated")?,
            ),
            crate::document_graph::RECORD_KIND => artifacts.extend(
                crate::document_graph::validate_retention_record(
                    store,
                    &kind,
                    &id,
                    &json_value(&raw)?,
                    deadline,
                )?
                .context("Document retention record was not validated")?,
            ),
            crate::capture::OPTIONS_RECORD_KIND
            | crate::capture::DERIVED_OPTIONS_RECORD_KIND
            | crate::capture::IGNORED_RECORD_KIND => {
                crate::capture::validate_retention_record(store, &kind, &id, &json_value(&raw)?)?
            }
            crate::notes::HEAD_KIND | crate::notes::REVISION_KIND => {
                crate::notes::validate_retention_record(store, &kind, &id, &json_value(&raw)?)?
            }
            crate::project_session::RECORD_KIND => {
                crate::project_session::validate_retention_record(store, &id, &json_value(&raw)?)?
            }
            crate::graph_extensions::REPORT_KIND | crate::graph_exports::EXPORT_KIND => artifacts
                .extend(validate_report_export(
                    store,
                    &kind,
                    &id,
                    &json_value(&raw)?,
                    deadline,
                )?),
            crate::graph_extensions::VIEW_REPORT_KIND => {
                let roots = crate::graph_extensions::validate_view_report_record(
                    store,
                    &id,
                    &json_value(&raw)?,
                    deadline,
                )?;
                // Snapshot and View IDs remain typed record references below;
                // only CAS objects belong in the artifact roots set.
                artifacts.extend(roots.artifact_ids);
            }
            crate::mixed_view::provider::JOB_KIND => {
                let roots = crate::mixed_view::provider::validate_retained_record(
                    store,
                    &kind,
                    &id,
                    &json_value(&raw)?,
                    deadline,
                )?;
                add_meaning_dependencies(
                    store,
                    roots,
                    &mut artifacts,
                    &mut typed_references,
                    deadline,
                )?;
            }
            crate::meaning_build::JOB_KIND
            | crate::meaning_build::CACHE_KIND
            | crate::meaning_build::GRAPH_KIND => {
                let roots = crate::meaning_build::validate_retained_record(
                    store,
                    &kind,
                    &id,
                    &json_value(&raw)?,
                    deadline,
                )?;
                add_meaning_dependencies(
                    store,
                    roots,
                    &mut artifacts,
                    &mut typed_references,
                    deadline,
                )?;
            }
            _ => {
                let value = json_value(&raw)?;
                match crate::graph_view::validate_retention_record(
                    store, &kind, &id, &value, deadline,
                )? {
                    Some(ids) => {
                        artifacts.extend(ids);
                        if kind == crate::graph_view::RECORD_KIND
                            && value["meaningGraphIds"]
                                .as_array()
                                .is_some_and(|ids| !ids.is_empty())
                        {
                            let view = crate::graph_view::load(store, &id, deadline)?;
                            if let Some(roots) =
                                crate::graph_view::meaning_dependencies(store, &view, deadline)?
                            {
                                add_meaning_dependencies(
                                    store,
                                    roots,
                                    &mut artifacts,
                                    &mut typed_references,
                                    deadline,
                                )?;
                            }
                        }
                    }
                    None => match crate::git_workflows::validate_retention_record(
                        store, &kind, &id, &value, deadline,
                    )? {
                        Some(ids) => artifacts.extend(ids),
                        None => continue,
                    },
                }
            }
        }
        recognized.insert((kind, id));
    }
    Ok((
        recognized,
        artifacts,
        federation_namespaces,
        typed_references,
    ))
}
fn known_artifact(value: &Value) -> Result<bool> {
    if let Ok(graph) = serde_json::from_value::<Graph>(value.clone())
        && serde_json::to_value(graph)? == *value
    {
        return Ok(true);
    }
    let Some(object) = value.as_object() else {
        return Ok(false);
    };
    let kind = value["kind"].as_str();
    let allowed = [
        "kind",
        "snapshotId",
        "assetId",
        "path",
        "contentHash",
        "model",
        "sourceBindings",
        "sourceOmissions",
        "statements",
        "resolution",
        "truth",
        "dimension",
        "count",
        "normalized",
        "trust",
        "omittedEntities",
        "truncated",
        "description",
        "attemptId",
        "accounting",
        "modelCalls",
        "localOnly",
        "networkIsolation",
        "sourceWritten",
        "currency",
        "providerReportedCostMicros",
        "providerReportedUsage",
    ];
    if kind.is_some()
        && !matches!(
            kind,
            Some("meaning_layer" | "model_embedding_generation" | "model_media_description")
        )
    {
        return Ok(false);
    }
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Ok(false);
    }
    if kind.is_none()
        && object
            .keys()
            .any(|key| !["snapshotId", "sourceBindings"].contains(&key.as_str()))
    {
        return Ok(false);
    }
    let source_owner = value["snapshotId"].as_str().is_some_and(valid_id)
        || (kind == Some("model_media_description")
            && value["assetId"].as_str().is_some_and(valid_id)
            && value["contentHash"].as_str().is_some_and(valid_id));
    if !source_owner {
        return Ok(false);
    }
    let Some(bindings) = value["sourceBindings"].as_array() else {
        return Ok(false);
    };
    Ok(bindings.iter().all(|binding| {
        binding.as_object().is_some_and(|fields| {
            fields.keys().all(|key| {
                [
                    "snapshotId",
                    "sourceId",
                    "path",
                    "contentHash",
                    "startByte",
                    "endByte",
                ]
                .contains(&key.as_str())
            })
        })
    }))
}
fn scan(store: &Store, deadline: &Deadline, exclude_intent: Option<&str>) -> Result<Scan> {
    deadline.check()?;
    directory(&store.dir, false)?;
    let mut schema=store.db.prepare("SELECT type,name FROM sqlite_master WHERE type IN ('table','view','trigger') ORDER BY name")?;
    for row in schema.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
        let (kind, name) = row?;
        let known = [
            "meta",
            "snapshots",
            "graphs",
            "cache",
            "records",
            "symbols",
            "documents",
        ]
        .contains(&name.as_str())
            || name.starts_with("sqlite_")
            || ["symbols", "documents"].iter().any(|p| {
                ["data", "idx", "content", "docsize", "config"]
                    .iter()
                    .any(|s| name == format!("{p}_{s}"))
            });
        ensure!(
            kind == "table" && known,
            "Unknown database schema; retention refused"
        );
    }
    let mut out = Scan::default();
    let github_inline_ids = crate::github_app::validated_inline_records(store, deadline)?;
    let github_worker_roots = crate::github_app::worker::retained_roots(store, deadline)?;
    let github_publication_ids = crate::github_app::publication::validated_inline_records(
        store,
        &github_worker_roots,
        deadline,
    )?;
    let github_page_ids =
        crate::github_app::pages::validated_inline_records(store, &github_worker_roots, deadline)?;
    let (digest_service_ids, digest_service_roots) =
        crate::github_app::digest::service::retained_roots(store, deadline)?;
    out.references.extend(digest_service_roots.iter().cloned());
    out.all_references.extend(digest_service_roots);
    let digest_receiver_ids =
        crate::github_app::digest::service::receiver::retained_records(store, deadline)?;
    let (digest_history_ids, digest_history_roots) =
        crate::github_app::digest::history_jobs::retained_roots(store, deadline)?;
    let app_history_ids = crate::github_app::worker::history::retained_roots(store, deadline)?;
    out.references.extend(digest_history_roots.iter().cloned());
    out.all_references.extend(digest_history_roots);
    out.references
        .extend(github_worker_roots.snapshot_ids.iter().cloned());
    out.all_references.extend(github_worker_roots.snapshot_ids);
    let decision_roots = crate::decision_records::validated_roots(store, deadline)?;
    let decision_ids: BTreeSet<_> = decision_roots
        .snapshot_ids
        .into_iter()
        .chain(decision_roots.blob_ids)
        .collect();
    out.references.extend(decision_ids.iter().cloned());
    out.all_references.extend(decision_ids);
    let feedback_roots = crate::knowledge::feedback::retained_roots(store, deadline)?;
    let feedback_ids: BTreeSet<_> = feedback_roots
        .snapshot_ids
        .into_iter()
        .chain(feedback_roots.blob_ids)
        .collect();
    out.references.extend(feedback_ids.iter().cloned());
    out.all_references.extend(feedback_ids);
    let (parity_records, parity_artifacts, federation_namespaces, typed_references) =
        validate_parity_records(store, deadline)?;
    out.references.extend(typed_references.iter().cloned());
    out.all_references.extend(typed_references);
    out.references.extend(parity_artifacts.iter().cloned());
    out.all_references.extend(parity_artifacts.iter().cloned());
    let mut owners: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut dependencies: BTreeMap<String, Value> = BTreeMap::new();
    for table in ["snapshots", "graphs", "cache", "meta"] {
        let key = if ["cache", "meta"].contains(&table) {
            "key"
        } else {
            "id"
        };
        let mut statement = store
            .db
            .prepare(&format!("SELECT {key},value FROM {table} ORDER BY {key}"))?;
        for row in
            statement.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
        {
            deadline.check()?;
            ensure!(
                out.inventory.len() < MAX_ROWS,
                "Retention row capacity exceeded"
            );
            let (id, raw) = row?;
            out.inventory.push((table.into(), id.clone(), raw.clone()));
            if table == "meta" {
                if valid_id(&raw) {
                    out.references.insert(raw.clone());
                    out.all_references.insert(raw);
                }
                continue;
            }
            let value = json_value(&raw)?;
            references(&value, &mut out.all_references, deadline)?;
            match table {
                "snapshots" => {
                    let s: Snapshot = serde_json::from_str(&raw)
                        .context("Corrupt snapshot; retention refused")?;
                    ensure!(
                        s.id == id
                            && valid_id(&id)
                            && s.workspace_id == store.workspace_id
                            && s.files.iter().all(|f| valid_id(&f.hash)),
                        "Snapshot identity invalid; retention refused"
                    );
                    if !["disk", "derived"].contains(&s.kind.as_str())
                        || value != serde_json::to_value(&s)?
                    {
                        out.references.insert(id.clone());
                        if value != serde_json::to_value(&s)? {
                            out.unknown.insert("snapshots:unknown-shape".into());
                        }
                    }
                    dependencies.insert(id.clone(), value);
                    out.snapshots.insert(id, raw);
                }
                "graphs" => {
                    let g: Graph =
                        serde_json::from_str(&raw).context("Corrupt graph; retention refused")?;
                    ensure!(
                        g.snapshot_id == id
                            && valid_id(&id)
                            && g.entities.iter().all(|e| e.snapshot_id == id)
                            && g.evidence.iter().all(|e| e.snapshot_id == id)
                            && g.observations.iter().all(|e| e.snapshot_id == id),
                        "Graph identity invalid; retention refused"
                    );
                    for member in g
                        .entities
                        .iter()
                        .map(|e| &e.id)
                        .chain(g.evidence.iter().map(|e| &e.id))
                        .chain(g.observations.iter().map(|e| &e.id))
                    {
                        owners.entry(member.clone()).or_default().insert(id.clone());
                    }
                    if value != serde_json::to_value(&g)? {
                        out.references.insert(id.clone());
                        out.unknown.insert("graphs:unknown-shape".into());
                    }
                    dependencies.insert(format!("graph:{id}"), value);
                    out.graphs.insert(id, raw);
                }
                _ => {
                    let parsed = serde_json::from_str::<Extraction>(&raw)
                        .context("Corrupt cache; retention refused")?;
                    let old_epoch = parsed.entities.iter().all(|entity| {
                        matches!(
                            entity.producer.as_str(),
                            "yoke-tree-sitter-4" | "yoke-tree-sitter-6"
                        )
                    });
                    let old_six = parsed
                        .entities
                        .iter()
                        .all(|entity| entity.producer == "yoke-tree-sitter-6");
                    let mut schema = serde_json::to_value(&parsed)?;
                    if value.get("resolution").is_none() && old_epoch {
                        // Released 0.5 Extraction had exactly these same nested
                        // schemas and omitted the later resolution enrichment.
                        schema
                            .as_object_mut()
                            .context("Cache schema invalid")?
                            .remove("resolution");
                    } else if old_six
                        && parsed.resolution.language.is_empty()
                        && value["resolution"]
                            .as_object()
                            .is_some_and(|fields| !fields.contains_key("language"))
                    {
                        // Early development-six caches predate composite
                        // language enrichment; all other fields remain exact.
                        schema["resolution"]
                            .as_object_mut()
                            .context("Resolution cache schema invalid")?
                            .remove("language");
                    }
                    if value != schema {
                        out.unknown.insert("cache:unknown-shape".into());
                    }
                    references(&value, &mut out.references, deadline)?;
                }
            }
        }
    }
    let mut statement = store
        .db
        .prepare("SELECT kind,id,value FROM records ORDER BY kind,id")?;
    for row in statement.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })? {
        deadline.check()?;
        let (kind, id, raw) = row?;
        let value = json_value(&raw)?;
        ensure!(
            out.inventory.len() < MAX_ROWS,
            "Retention row capacity exceeded"
        );
        if kind == "retention-plan" {
            let p: HistoryPlan = serde_json::from_str(&raw)?;
            validate_history_plan(store, &id, &p)?;
            continue;
        }
        if kind == "purge-plan" {
            let p: PurgePlan = serde_json::from_str(&raw)?;
            validate_purge_plan(store, &id, &p)?;
            continue;
        }
        if kind == "purge-intent" && exclude_intent == Some(id.as_str()) {
            let p: Progress = serde_json::from_str(&raw)?;
            ensure!(
                p.workspace_id == store.workspace_id && p.plan_id == id,
                "Invalid purge journal"
            );
            continue;
        }
        out.inventory
            .push((format!("records:{kind}"), id.clone(), raw.clone()));
        if !known_kind(&kind) && !parity_records.contains(&(kind.clone(), id.clone())) {
            out.unknown.insert(kind.clone());
        }
        if kind == crate::github_app::RECORD_KIND {
            ensure!(
                github_inline_ids.contains(&id),
                "Invalid GitHub inline history"
            );
            // Strict typed proof above establishes no CAS authority. Payload text
            // and Git IDs must never become source-retention references.
            continue;
        }
        if kind == crate::github_app::worker::RECORD_KIND {
            ensure!(
                github_worker_roots.record_ids.contains(&id),
                "Invalid GitHub worker history"
            );
            // Only validated typed head/base roots above carry source authority.
            // Inert report JSON, Git IDs, MACs and request text are never walked.
            continue;
        }
        if kind == crate::github_app::worker::history::RECORD_KIND {
            ensure!(
                app_history_ids.contains(&(kind.clone(), id.clone())),
                "Invalid App history binding"
            );
            // Binding foreign keys are checked against real worker/history rows.
            // Existing typed worker/job roots alone retain source and CAS state.
            continue;
        }
        if kind == crate::github_app::publication::RECORD_KIND {
            ensure!(
                github_publication_ids.contains(&id),
                "Invalid GitHub publication history"
            );
            // Only the validated completed worker roots carry CAS authority. Body,
            // request, URL and other inline strings are never generic root inputs.
            continue;
        }
        if kind == crate::github_app::digest::service::RECORD_KIND {
            ensure!(
                digest_service_ids.contains(&id),
                "Invalid digest service history"
            );
            // Only closed typed source selectors above retain captured state.
            // Payload hashes, endpoint strings and receiver IDs carry no CAS authority.
            continue;
        }
        if kind == crate::github_app::digest::service::receiver::RECORD_KIND {
            ensure!(
                digest_receiver_ids.contains(&id),
                "Invalid digest receiver history"
            );
            // An authenticated remote sender supplies inert observations only;
            // no imported snapshot/hash/body field can retain local CAS state.
            continue;
        }
        if kind == crate::github_app::digest::history_jobs::RECORD_KIND {
            ensure!(
                digest_history_ids.contains(&id),
                "Invalid digest history job"
            );
            // Only source selectors validated above retain state; commit text,
            // imported Git labels and counters never grant retention authority.
            continue;
        }
        if kind == crate::github_app::pages::RECORD_KIND
            || kind == crate::github_app::pages::REVISION_KIND
        {
            ensure!(
                github_page_ids.contains(&(kind.clone(), id.clone())),
                "Invalid GitHub page history"
            );
            // Typed completed worker roots alone retain sources. Never walk HTML,
            // labels, capability URLs or caller-provided identifiers as CAS roots.
            continue;
        }
        if kind == "retention-committed" {
            let r: HistoryReceipt = serde_json::from_str(&raw)?;
            ensure!(
                r.schema_version == 1
                    && r.workspace_id == store.workspace_id
                    && hash(&r.manifest_text) == r.manifest_hash,
                "Corrupt retirement receipt"
            );
            let manifest: HistoryManifest = serde_json::from_str(&r.manifest_text)?;
            validate_manifest(store, &id, &manifest)?;
            continue;
        }
        if kind == "purge-completed" {
            let p: Progress = serde_json::from_str(&raw)?;
            ensure!(
                p.schema_version == 1
                    && p.workspace_id == store.workspace_id
                    && p.plan_id == id
                    && p.complete,
                "Invalid completed purge receipt"
            );
            continue;
        }
        if kind == "snapshot-published" {
            let sid = value["snapshotId"]
                .as_str()
                .context("Invalid snapshot age receipt")?;
            let time = value["publishedAtMs"]
                .as_u64()
                .context("Invalid snapshot age receipt")?;
            ensure!(
                sid == id && valid_id(sid) && value["workspaceId"] == store.workspace_id,
                "Invalid snapshot age receipt"
            );
            out.published.insert(id, time);
            continue;
        }
        if valid_id(&id) {
            out.references.insert(id.clone());
            out.all_references.insert(id.clone());
        }
        if kind == "artifact" {
            if let (Some(artifact_id), Some(size)) =
                (value["artifactId"].as_str(), value["totalBytes"].as_u64())
            {
                ensure!(
                    valid_id(artifact_id) && artifact_id == id,
                    "Artifact registration identity invalid"
                );
                let path = store.dir.join("blobs").join(artifact_id);
                ensure!(
                    leaf(&path)?.len() == size,
                    "Artifact registration size invalid"
                );
                if size > MAX_MANIFEST {
                    out.unknown.insert("artifact:capacity".into());
                } else {
                    checked_hash(&path, artifact_id, deadline)?;
                    match serde_json::from_slice::<Value>(&fs::read(path)?) {
                        Ok(content) => {
                            if !parity_artifacts.contains(artifact_id) && !known_artifact(&content)?
                            {
                                out.unknown.insert("artifact:unknown-shape".into());
                            }
                            references(&content, &mut out.references, deadline)?;
                            references(&content, &mut out.all_references, deadline)?;
                        }
                        Err(_) => {
                            if !parity_artifacts.contains(artifact_id) {
                                out.unknown.insert("artifact:opaque".into());
                            }
                        }
                    }
                }
            } else {
                out.unknown.insert("artifact:unknown-shape".into());
            }
        }
        references(&value, &mut out.references, deadline)?;
        references(&value, &mut out.all_references, deadline)?;
    }
    loop {
        deadline.check()?;
        let before = out.references.len();
        let marked: Vec<_> = out.references.iter().cloned().collect();
        for id in marked {
            if let Some(sids) = owners.get(&id) {
                out.references.extend(sids.iter().cloned());
            }
            if let Some(v) = dependencies.get(&id) {
                references(v, &mut out.references, deadline)?;
            }
            if let Some(v) = dependencies.get(&format!("graph:{id}")) {
                references(v, &mut out.references, deadline)?;
            }
        }
        if before == out.references.len() {
            break;
        }
    }
    for id in out.graphs.keys() {
        ensure!(
            out.snapshots.contains_key(id),
            "Unbound graph; retention refused"
        );
    }
    for (table, columns) in [
        ("symbols", "snapshot,id,name,path,signature"),
        ("documents", "snapshot,path,text"),
    ] {
        let mut statement = store
            .db
            .prepare(&format!("SELECT {columns} FROM {table} ORDER BY rowid"))?;
        let count = if table == "symbols" { 5 } else { 3 };
        let rows = statement.query_map([], |r| {
            (0..count)
                .map(|n| r.get::<_, String>(n))
                .collect::<rusqlite::Result<Vec<_>>>()
        })?;
        for row in rows {
            deadline.check()?;
            ensure!(
                out.inventory.len() < MAX_ROWS,
                "Retention row capacity exceeded"
            );
            let row = row?;
            ensure!(
                out.snapshots.contains_key(&row[0]) || federation_namespaces.contains(&row[0]),
                "Unbound search row; retention refused"
            );
            out.inventory
                .push((table.into(), row[0].clone(), serde_json::to_string(&row)?));
        }
    }
    Ok(out)
}
fn fingerprint(scan: &Scan) -> String {
    identity(&scan.inventory)
}
fn candidate_ids(scan: &Scan, policy: &Policy, clock: u64) -> (Vec<String>, usize) {
    let mut ids = Vec::new();
    let mut unknown_age = 0;
    if !scan.unknown.is_empty() {
        return (ids, 0);
    }
    for id in scan.snapshots.keys() {
        if scan.references.contains(id) {
            continue;
        }
        if policy.minimum_age_ms > 0 {
            match scan.published.get(id) {
                Some(time)
                    if clock
                        .checked_sub(*time)
                        .is_some_and(|age| age >= policy.minimum_age_ms) => {}
                Some(_) => continue,
                None => {
                    unknown_age += 1;
                    continue;
                }
            }
        }
        ids.push(id.clone());
    }
    (ids, unknown_age)
}
fn validate_history_plan(store: &Store, id: &str, p: &HistoryPlan) -> Result<()> {
    ensure!(
        valid_id(id)
            && identity(p) == id
            && p.schema_version == 1
            && p.workspace_id == store.workspace_id
            && valid_id(&p.inventory_hash)
            && valid_id(&p.rows_hash)
            && p.snapshot_ids.len() <= MAX_ROWS
            && p.snapshot_ids.iter().all(|s| valid_id(s))
            && p.snapshot_ids.windows(2).all(|a| a[0] < a[1]),
        "Invalid or tampered retirement plan"
    );
    Ok(())
}
fn validate_purge_plan(store: &Store, id: &str, p: &PurgePlan) -> Result<()> {
    ensure!(
        valid_id(id)
            && identity(p) == id
            && p.schema_version == 1
            && p.workspace_id == store.workspace_id
            && valid_id(&p.inventory_hash)
            && p.items.len() <= MAX_ROWS
            && p.items
                .iter()
                .all(|i| valid_id(&i.run_id) && valid_id(&i.blob_id) && valid_id(&i.manifest_hash))
            && p.items
                .windows(2)
                .all(|a| (&a[0].run_id, &a[0].blob_id) < (&a[1].run_id, &a[1].blob_id)),
        "Invalid or tampered purge plan"
    );
    Ok(())
}
fn validate_manifest(store: &Store, id: &str, m: &HistoryManifest) -> Result<()> {
    validate_history_plan(store, id, &m.plan)?;
    ensure!(
        m.schema_version == 1 && m.snapshots.len() == m.plan.snapshot_ids.len(),
        "Invalid history manifest"
    );
    ensure!(
        identity(&(&m.snapshots, &m.graphs, &m.symbols, &m.documents)) == m.plan.rows_hash,
        "History manifest rows do not match the reviewed hash"
    );
    for (row, sid) in m.snapshots.iter().zip(&m.plan.snapshot_ids) {
        let s: Snapshot = serde_json::from_str(&row.value)?;
        ensure!(
            &row.id == sid
                && s.id == row.id
                && s.workspace_id == store.workspace_id
                && s.files.iter().all(|f| valid_id(&f.hash)),
            "Invalid history snapshot manifest"
        );
    }
    let mut graphs = BTreeSet::new();
    for row in &m.graphs {
        let g: Graph = serde_json::from_str(&row.value)?;
        ensure!(
            m.plan.snapshot_ids.contains(&row.id)
                && graphs.insert(&row.id)
                && g.snapshot_id == row.id
                && g.entities.iter().all(|e| e.snapshot_id == row.id)
                && g.observations.iter().all(|e| e.snapshot_id == row.id)
                && g.evidence.iter().all(|e| e.snapshot_id == row.id),
            "Invalid history graph manifest"
        );
    }
    ensure!(
        m.symbols.len() + m.documents.len() <= MAX_ROWS,
        "History search capacity exceeded"
    );
    for (row, len) in m
        .symbols
        .iter()
        .map(|r| (r, 5))
        .chain(m.documents.iter().map(|r| (r, 3)))
    {
        ensure!(
            row.len() == len && m.plan.snapshot_ids.contains(&row[0]),
            "Invalid history search manifest"
        );
    }
    Ok(())
}
fn transaction<'a>(store: &'a Store, deadline: &Deadline) -> Result<Transaction<'a>> {
    store.set_deadline(deadline)?;
    Ok(Transaction::new_unchecked(
        &store.db,
        TransactionBehavior::Immediate,
    )?)
}
fn pending_history(
    store: &Store,
    deadline: &Deadline,
) -> Result<Vec<(String, PathBuf, String, HistoryManifest)>> {
    let root = store.dir.join("history-quarantine");
    if !directory(&root, false)? {
        return Ok(Vec::new());
    }
    let mut pending = Vec::new();
    for entry in fs::read_dir(&root)? {
        deadline.check()?;
        ensure!(
            pending.len() < 10_000,
            "History quarantine capacity exceeded"
        );
        let entry = entry?;
        let id = entry
            .file_name()
            .to_str()
            .context("Invalid history run name")?
            .to_owned();
        ensure!(valid_id(&id), "Unknown history quarantine entry");
        directory(&entry.path(), false)?;
        let path = entry.path().join("manifest.json");
        ensure!(
            leaf(&path)?.len() <= MAX_MANIFEST,
            "History manifest exceeds capacity"
        );
        for file in fs::read_dir(entry.path())? {
            let file = file?;
            leaf(&file.path())?;
            ensure!(
                file.file_name() == "manifest.json",
                "Unknown history quarantine file"
            );
        }
        let raw = fs::read_to_string(&path)?;
        let m: HistoryManifest = serde_json::from_str(&raw)?;
        validate_manifest(store, &id, &m)?;
        if let Some(receipt) = store
            .db
            .query_row(
                "SELECT value FROM records WHERE kind='retention-committed' AND id=?1",
                [&id],
                |r| r.get::<_, String>(0),
            )
            .optional()?
        {
            let receipt: HistoryReceipt = serde_json::from_str(&receipt)?;
            ensure!(
                receipt.workspace_id == store.workspace_id
                    && receipt.manifest_hash == hash(&raw)
                    && receipt.manifest_text == raw,
                "History manifest/receipt hash mismatch"
            );
            continue;
        }
        let restored: bool = store.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM records WHERE kind='retention-restored' AND id=?1)",
            [&id],
            |r| r.get(0),
        )?;
        if !restored {
            pending.push((id, path, raw, m));
        }
    }
    pending.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(pending)
}
pub fn plan(store: &Store, policy: &Policy, deadline: &Deadline) -> Result<Value> {
    ensure!(
        policy.expires_after_ms > 0 && policy.expires_after_ms <= 86_400_000,
        "Invalid retirement plan lifetime"
    );
    store.with_exclusive_client(deadline,||{let tx=transaction(store,deadline)?;let pending=pending_history(store,deadline)?;ensure!(pending.is_empty(),"Uncommitted history requires recovery before retirement");let scan=scan(store,deadline,None)?;let clock=now_ms()?;let(ids,deferred)=candidate_ids(&scan,policy,clock);
        let snapshots:Vec<_>=ids.iter().map(|id|Row{id:id.clone(),value:scan.snapshots[id].clone()}).collect();
        let graphs:Vec<_>=ids.iter().filter_map(|id|scan.graphs.get(id).map(|value|Row{id:id.clone(),value:value.clone()})).collect();
        let symbols=selected_search(store,"symbols","snapshot,id,name,path,signature",&ids,deadline)?;
        let documents=selected_search(store,"documents","snapshot,path,text",&ids,deadline)?;
        let rows_hash=identity(&(&snapshots,&graphs,&symbols,&documents));
        let p=HistoryPlan{schema_version:1,workspace_id:store.workspace_id.clone(),created_ms:clock,expires_ms:clock.checked_add(policy.expires_after_ms).context("Plan lifetime overflow")?,minimum_age_ms:policy.minimum_age_ms,inventory_hash:fingerprint(&scan),rows_hash,snapshot_ids:ids.clone()};let id=identity(&p);
        tx.execute("INSERT OR IGNORE INTO records VALUES('retention-plan',?1,?2)",params![id,serde_json::to_string(&p)?])?;deadline.check()?;tx.commit()?;
        Ok(json!({"planId":id,"confirmationHash":id,"expiresMs":p.expires_ms,"snapshotIds":ids,"eligibleSnapshots":ids.len(),"retainedSnapshots":scan.snapshots.len()-ids.len(),"deferredUnknownAgeSnapshots":deferred,"unknownDependencyKinds":scan.unknown,"reclaimedBytes":0,"scope":"host_explicit_history_retirement"}))
    })
}
fn selected_search(
    store: &Store,
    table: &str,
    columns: &str,
    ids: &[String],
    deadline: &Deadline,
) -> Result<Vec<Vec<String>>> {
    let mut out = Vec::new();
    let len = if table == "symbols" { 5 } else { 3 };
    let mut statement = store.db.prepare(&format!(
        "SELECT {columns} FROM {table} WHERE snapshot=?1 ORDER BY rowid"
    ))?;
    for id in ids {
        for row in statement.query_map([id], |r| {
            (0..len)
                .map(|i| r.get::<_, String>(i))
                .collect::<rusqlite::Result<Vec<_>>>()
        })? {
            deadline.check()?;
            ensure!(out.len() < MAX_ROWS, "History search capacity exceeded");
            out.push(row?);
        }
    }
    Ok(out)
}
pub fn apply(
    store: &Store,
    plan_id: &str,
    confirmed_hash: &str,
    allow_state_deletion: bool,
    deadline: &Deadline,
) -> Result<Value> {
    ensure!(
        allow_state_deletion,
        "History retirement requires explicit host state deletion permission"
    );
    ensure!(
        valid_id(plan_id) && plan_id == confirmed_hash,
        "Exact retirement plan confirmation required"
    );
    store.with_exclusive_client(deadline,||{
        let p:HistoryPlan=store.get_record("retention-plan",plan_id)?;validate_history_plan(store,plan_id,&p)?;
        if let Some(raw)=store.db.query_row("SELECT value FROM records WHERE kind='retention-committed' AND id=?1",[plan_id],|r|r.get::<_,String>(0)).optional()?{let r:HistoryReceipt=serde_json::from_str(&raw)?;ensure!(r.workspace_id==store.workspace_id&&hash(&r.manifest_text)==r.manifest_hash,"Invalid retirement receipt");let manifest:HistoryManifest=serde_json::from_str(&r.manifest_text)?;validate_manifest(store,plan_id,&manifest)?;return Ok(r.result);}
        ensure!(p.expires_ms>now_ms()?,"Retirement plan expired");let tx=transaction(store,deadline)?;ensure!(pending_history(store,deadline)?.is_empty(),"Uncommitted history requires recovery");let scan=scan(store,deadline,None)?;
        ensure!(fingerprint(&scan)==p.inventory_hash,"Retirement plan is stale");let policy=Policy{minimum_age_ms:p.minimum_age_ms,expires_after_ms:1};ensure!(candidate_ids(&scan,&policy,now_ms()?).0==p.snapshot_ids,"Retirement roots or age changed");
        let m=HistoryManifest{schema_version:1,plan:p.clone(),snapshots:p.snapshot_ids.iter().map(|id|Row{id:id.clone(),value:scan.snapshots[id].clone()}).collect(),graphs:p.snapshot_ids.iter().filter_map(|id|scan.graphs.get(id).map(|value|Row{id:id.clone(),value:value.clone()})).collect(),symbols:selected_search(store,"symbols","snapshot,id,name,path,signature",&p.snapshot_ids,deadline)?,documents:selected_search(store,"documents","snapshot,path,text",&p.snapshot_ids,deadline)?};
        validate_manifest(store,plan_id,&m)?;for row in &m.snapshots{let s:Snapshot=serde_json::from_str(&row.value)?;for file in s.files{deadline.check()?;checked_hash(&store.dir.join("blobs").join(&file.hash),&file.hash,deadline)?;}}
        let root=store.dir.join("history-quarantine");directory(&root,true)?;let run=root.join(plan_id);ensure!(!run.try_exists()?,"History quarantine already exists; recover first");fs::create_dir(&run)?;
        let text=serde_json::to_string(&m)?;ensure!(text.len()<=MAX_MANIFEST as usize,"History manifest exceeds capacity");let mut file=fs::OpenOptions::new().write(true).create_new(true).open(run.join("manifest.json"))?;file.write_all(text.as_bytes())?;file.sync_all()?;drop(file);
        let result=json!({"planId":plan_id,"retiredSnapshots":m.snapshots.len(),"retiredGraphs":m.graphs.len(),"retiredSearchRows":m.symbols.len()+m.documents.len(),"quarantinePath":run,"reclaimedBytes":0,"complete":true,"physicalSourceBytesRemoved":0});
        for row in &m.snapshots{deadline.check()?;ensure!(tx.execute("DELETE FROM snapshots WHERE id=?1 AND value=?2",params![row.id,row.value])?==1,"Snapshot changed during retirement");tx.execute("DELETE FROM symbols WHERE snapshot=?1",[&row.id])?;tx.execute("DELETE FROM documents WHERE snapshot=?1",[&row.id])?;}
        for row in &m.graphs{deadline.check()?;ensure!(tx.execute("DELETE FROM graphs WHERE id=?1 AND value=?2",params![row.id,row.value])?==1,"Graph changed during retirement");}
        let receipt=HistoryReceipt{schema_version:1,workspace_id:store.workspace_id.clone(),manifest_hash:hash(&text),manifest_text:text,result:result.clone()};tx.execute("INSERT INTO records VALUES('retention-committed',?1,?2)",params![plan_id,serde_json::to_string(&receipt)?])?;deadline.check()?;tx.commit()?;Ok(result)
    })
}
pub fn recover(store: &Store, deadline: &Deadline) -> Result<Value> {
    store.with_exclusive_client(deadline,||{let tx=transaction(store,deadline)?;let pending=pending_history(store,deadline)?;let mut count=0;
    for(id,_,raw,m)in pending{deadline.check()?;
        for row in &m.snapshots{let s:Snapshot=serde_json::from_str(&row.value)?;for file in s.files{checked_hash(&store.dir.join("blobs").join(&file.hash),&file.hash,deadline)?;}}
        for(table,rows)in[("snapshots",&m.snapshots),("graphs",&m.graphs)]{for row in rows{deadline.check()?;let existing:Option<String>=tx.query_row(&format!("SELECT value FROM {table} WHERE id=?1"),[&row.id],|r|r.get(0)).optional()?;if let Some(existing)=existing{ensure!(existing==row.value,"Foreign history row conflicts with recovery");}else{tx.execute(&format!("INSERT INTO {table} VALUES(?1,?2)"),params![row.id,row.value])?;count+=1;}}}
        for(table,columns,rows)in[("symbols","snapshot,id,name,path,signature",&m.symbols),("documents","snapshot,path,text",&m.documents)]{let mut existing=selected_search(store,table,columns,&m.plan.snapshot_ids,deadline)?;existing.sort();let mut expected=rows.clone();expected.sort();if existing.is_empty(){for row in rows{deadline.check()?;if table=="symbols"{tx.execute("INSERT INTO symbols VALUES(?1,?2,?3,?4,?5)",params![row[0],row[1],row[2],row[3],row[4]])?;}else{tx.execute("INSERT INTO documents VALUES(?1,?2,?3)",params![row[0],row[1],row[2]])?;}}}else{ensure!(existing==expected,"Foreign search rows conflict with recovery");}}
        tx.execute("INSERT INTO records VALUES('retention-restored',?1,?2)",params![id,serde_json::to_string(&json!({"schemaVersion":1,"workspaceId":store.workspace_id,"manifestHash":hash(raw),"snapshotIds":m.plan.snapshot_ids}))?])?;
    }deadline.check()?;tx.commit()?;Ok(json!({"restoredRows":count,"reclaimedBytes":0,"complete":true}))
})
}

fn cas_items(
    store: &Store,
    marked: &BTreeSet<String>,
    minimum_age_ms: u64,
    deadline: &Deadline,
) -> Result<Vec<PurgeItem>> {
    let root = store.dir.join("quarantine");
    if !directory(&root, false)? {
        return Ok(Vec::new());
    }
    let clock = now_ms()?;
    let mut out = Vec::new();
    let mut runs = 0;
    for entry in fs::read_dir(&root)? {
        deadline.check()?;
        runs += 1;
        ensure!(runs <= 10_000, "CAS quarantine capacity exceeded");
        let entry = entry?;
        let id = entry
            .file_name()
            .to_str()
            .context("Invalid quarantine ID")?
            .to_owned();
        ensure!(valid_id(&id), "Unknown quarantine run refused");
        directory(&entry.path(), false)?;
        let path = entry.path().join("manifest.json");
        ensure!(
            leaf(&path)?.len() <= 16_000_000,
            "CAS manifest capacity exceeded"
        );
        let raw = fs::read(&path)?;
        let manifest: CasManifest = serde_json::from_slice(&raw)?;
        ensure!(
            manifest.schema_version == 1
                && manifest.id == id
                && manifest.workspace_id == store.workspace_id
                && manifest.blob_ids.len() <= MAX_ROWS
                && manifest.blob_ids.iter().all(|id| valid_id(id))
                && manifest.blob_ids.iter().collect::<BTreeSet<_>>().len()
                    == manifest.blob_ids.len(),
            "Invalid CAS quarantine manifest"
        );
        let committed: bool = store.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM records WHERE kind='maintenance-committed' AND id=?1)",
            [&id],
            |r| r.get(0),
        )?;
        for file in fs::read_dir(entry.path())? {
            deadline.check()?;
            let file = file?;
            let name = file
                .file_name()
                .to_str()
                .context("Invalid CAS quarantine filename")?
                .to_owned();
            let meta = leaf(&file.path())?;
            ensure!(
                name == "manifest.json" || manifest.blob_ids.contains(&name),
                "Undeclared CAS quarantine file"
            );
            if name == "manifest.json"
                || !committed
                || marked.contains(&name)
                || clock
                    .checked_sub(manifest.created_ms)
                    .is_none_or(|age| age < minimum_age_ms)
            {
                continue;
            }
            ensure!(out.len() < MAX_ROWS, "Purge item capacity exceeded");
            single_link(&file.path())?;
            checked_hash(&file.path(), &name, deadline)?;
            out.push(PurgeItem {
                run_id: id.clone(),
                blob_id: name,
                bytes: meta.len(),
                created_ms: manifest.created_ms,
                manifest_hash: hash(&raw),
            });
        }
    }
    out.sort_by(|a, b| (&a.run_id, &a.blob_id).cmp(&(&b.run_id, &b.blob_id)));
    Ok(out)
}
pub fn purge_plan(store: &Store, minimum_age_ms: u64, deadline: &Deadline) -> Result<Value> {
    store.with_exclusive_client(deadline,||{let tx=transaction(store,deadline)?;ensure!(pending_history(store,deadline)?.is_empty(),"Pending history recovery blocks purge");let scan=scan(store,deadline,None)?;let items=if scan.unknown.is_empty(){cas_items(store,&scan.all_references,minimum_age_ms,deadline)?}else{Vec::new()};let eligible_bytes=items.iter().try_fold(0u64,|sum,i|sum.checked_add(i.bytes)).context("Purge byte count overflow")?;let clock=now_ms()?;
    let p=PurgePlan{schema_version:1,workspace_id:store.workspace_id.clone(),created_ms:clock,expires_ms:clock.checked_add(600_000).context("Purge lifetime overflow")?,minimum_age_ms,inventory_hash:fingerprint(&scan),items};let id=identity(&p);tx.execute("INSERT OR IGNORE INTO records VALUES('purge-plan',?1,?2)",params![id,serde_json::to_string(&p)?])?;deadline.check()?;tx.commit()?;Ok(json!({"planId":id,"confirmationHash":id,"expiresMs":p.expires_ms,"items":p.items,"eligibleBytes":eligible_bytes,"unknownDependencyKinds":scan.unknown,"reclaimedBytes":0}))
})
}
fn item_key(item: &PurgeItem) -> String {
    format!("{}:{}", item.run_id, item.blob_id)
}
fn progress_value(p: &Progress, error: Option<String>) -> Value {
    json!({"planId":p.plan_id,"removedBlobs":p.removed.len(),"reclaimedBytes":p.reclaimed_bytes,"unverifiedRemovalBytes":p.unverified_removal_bytes,"complete":p.complete,"error":error,"scope":"committed_quarantine_only"})
}
pub fn purge(
    store: &Store,
    plan_id: &str,
    confirmed_hash: &str,
    allow_state_deletion: bool,
    deadline: &Deadline,
) -> Result<Value> {
    ensure!(
        allow_state_deletion,
        "Irreversible purge requires explicit host state deletion permission"
    );
    ensure!(
        valid_id(plan_id) && confirmed_hash == plan_id,
        "Exact purge plan confirmation required"
    );
    store.with_exclusive_client(deadline,||{let p:PurgePlan=store.get_record("purge-plan",plan_id)?;validate_purge_plan(store,plan_id,&p)?;
        if let Some(raw)=store.db.query_row("SELECT value FROM records WHERE kind='purge-completed' AND id=?1",[plan_id],|r|r.get::<_,String>(0)).optional()?{let progress:Progress=serde_json::from_str(&raw)?;ensure!(progress.workspace_id==store.workspace_id&&progress.plan_id==plan_id&&progress.complete,"Invalid purge receipt");return Ok(progress_value(&progress,None));}
        ensure!(p.expires_ms>now_ms()?,"Purge plan expired");let tx=transaction(store,deadline)?;ensure!(pending_history(store,deadline)?.is_empty(),"Pending history recovery blocks purge");let scan_result=scan(store,deadline,Some(plan_id))?;ensure!(scan_result.unknown.is_empty()&&fingerprint(&scan_result)==p.inventory_hash,"Purge plan stale or dependencies unknown");
        let blob_ids:Vec<_>=p.items.iter().map(|i|i.blob_id.clone()).collect::<BTreeSet<_>>().into_iter().collect();
        let mut progress:Progress=store.db.query_row("SELECT value FROM records WHERE kind='purge-intent' AND id=?1",[plan_id],|r|r.get::<_,String>(0)).optional()?.map(|raw|serde_json::from_str(&raw)).transpose()?.unwrap_or(Progress{schema_version:1,workspace_id:store.workspace_id.clone(),plan_id:plan_id.into(),blob_ids:blob_ids.clone(),removed:Vec::new(),deleting:None,reclaimed_bytes:0,unverified_removal_bytes:0,complete:false});
        ensure!(progress.schema_version==1&&progress.workspace_id==store.workspace_id&&progress.plan_id==plan_id&&progress.blob_ids==blob_ids&&!progress.complete&&progress.removed.iter().all(|key|p.items.iter().any(|i|item_key(i)==*key))&&progress.removed.iter().collect::<BTreeSet<_>>().len()==progress.removed.len()&&progress.deleting.as_ref().is_none_or(|key|p.items.iter().any(|i|item_key(i)==*key)),"Invalid purge recovery progress");
        let eligible=cas_items(store,&scan_result.all_references,p.minimum_age_ms,deadline)?;
        for item in &p.items{let key=item_key(item);if progress.removed.contains(&key){ensure!(!store.dir.join("quarantine").join(&item.run_id).join(&item.blob_id).try_exists()?,"Previously purged path has foreign bytes");continue;}
            if eligible.contains(item){continue;}
            let path=store.dir.join("quarantine").join(&item.run_id).join(&item.blob_id);
            ensure!(progress.deleting.as_ref()==Some(&key)&&!path.try_exists()?,"Purge reachability, age, bytes or manifest changed");progress.removed.push(key);progress.deleting=None;progress.unverified_removal_bytes=progress.unverified_removal_bytes.checked_add(item.bytes).context("Purge byte count overflow")?;
        }
        tx.execute("INSERT INTO records VALUES('purge-intent',?1,?2) ON CONFLICT(kind,id) DO UPDATE SET value=excluded.value",params![plan_id,serde_json::to_string(&progress)?])?;deadline.check()?;tx.commit()?;
        // Durable intent precedes removal. Immediate transactions keep raw SQLite
        // writers from changing dependency roots during each destructive step.
        let deletion=(||->Result<()>{for item in &p.items{let key=item_key(item);if progress.removed.contains(&key){continue;}deadline.check()?;let tx=transaction(store,deadline)?;let current=scan(store,deadline,Some(plan_id))?;ensure!(current.unknown.is_empty()&&fingerprint(&current)==p.inventory_hash&&!current.all_references.contains(&item.blob_id),"Purge state changed during destruction");
            let root=store.dir.join("quarantine");directory(&root,false)?;let run=root.join(&item.run_id);directory(&run,false)?;let path=run.join(&item.blob_id);
            ensure!(run.canonicalize()?.parent()==Some(root.canonicalize()?.as_path())&&path.canonicalize()?.parent()==Some(run.canonicalize()?.as_path()),"Purge path containment failed");
            let manifest_path=run.join("manifest.json");leaf(&manifest_path)?;let raw=fs::read(manifest_path)?;ensure!(hash(&raw)==item.manifest_hash&&now_ms()?.checked_sub(item.created_ms).is_some_and(|age|age>=p.minimum_age_ms),"Purge manifest or age changed");ensure!(checked_hash(&path,&item.blob_id,deadline)?==item.bytes,"Purge size changed");
            progress.deleting=Some(key.clone());tx.execute("UPDATE records SET value=?1 WHERE kind='purge-intent' AND id=?2",params![serde_json::to_string(&progress)?,plan_id])?;deadline.check()?;tx.commit()?;
            let tx=transaction(store,deadline)?;let checked=scan(store,deadline,Some(plan_id))?;ensure!(fingerprint(&checked)==p.inventory_hash&&checked.unknown.is_empty()&&!checked.all_references.contains(&item.blob_id),"Purge roots changed after intent");directory(&root,false)?;directory(&run,false)?;ensure!(checked_hash(&path,&item.blob_id,deadline)?==item.bytes,"Purge content changed after intent");deadline.check()?;
            single_link(&path)?;fs::remove_file(&path)?;progress.removed.push(key);progress.deleting=None;progress.reclaimed_bytes=progress.reclaimed_bytes.checked_add(item.bytes).context("Purge byte count overflow")?;
            tx.execute("UPDATE records SET value=?1 WHERE kind='purge-intent' AND id=?2",params![serde_json::to_string(&progress)?,plan_id])?;tx.commit()?;
        }Ok(())})();
        if let Err(error)=deletion{return Ok(progress_value(&progress,Some(format!("Purge incomplete: {error:#}"))));}
        progress.complete=true;
        let completion=(||->Result<()>{let tx=transaction(store,deadline)?;tx.execute("INSERT INTO records VALUES('purge-completed',?1,?2)",params![plan_id,serde_json::to_string(&progress)?])?;tx.execute("DELETE FROM records WHERE kind='purge-intent' AND id=?1",[plan_id])?;deadline.check()?;tx.commit()?;Ok(())})();
        if let Err(error)=completion{progress.complete=false;return Ok(progress_value(&progress,Some(format!("Destruction finished; durable completion needs recovery: {error:#}"))));}
        Ok(progress_value(&progress,None))
    })
}
