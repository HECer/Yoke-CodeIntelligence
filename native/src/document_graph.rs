//! Explicit immutable Office graphs; ZIP bytes never become a SourceSnapshot.
use crate::{
    Engine,
    capture::Deadline,
    media::Asset,
    model::{hash, identity, valid_id},
    office::XmlOrigin,
    store::Store,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs,
    io::{Read, Write},
    path::Path,
};
pub const RECORD_KIND: &str = "native_document_graph_v1";
#[path = "document_mixed_input.rs"]
pub(crate) mod mixed_input;
const PRODUCER: &str = "yoke-document-graph-1";
const MAX_GRAPH: usize = 32_000_000;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Part {
    name: String,
    hash: String,
    size: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Segment {
    id: String,
    kind: String,
    part: String,
    start_byte: usize,
    end_byte: usize,
    text_hash: String,
    xml_origins: Vec<XmlOrigin>,
    sheet: Option<String>,
    cell: Option<String>,
    cell_type: Option<String>,
    formula: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Node {
    id: String,
    kind: String,
    name: String,
    part: Option<String>,
    segment_id: Option<String>,
    parent_id: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Arc {
    id: String,
    relation: String,
    source: String,
    target: String,
    xml_origins: Vec<XmlOrigin>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Raw {
    schema_version: u32,
    id: String,
    producer: String,
    asset: Asset,
    format: String,
    parts: Vec<Part>,
    decoded_text_hash: String,
    decoded_text_bytes: usize,
    separator_policy: String,
    segments: Vec<Segment>,
    nodes: Vec<Node>,
    observations: Vec<Arc>,
    limitations: Vec<String>,
}
#[derive(Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Record {
    schema_version: u32,
    document_graph_id: String,
    workspace_id: String,
    asset_id: String,
    artifact_id: String,
    container_hash: String,
    inspection_artifact_id: String,
}
/// Owns immutable reconstructed bytes; caller JSON cannot construct this type.
pub struct CheckedDocumentGraph {
    raw: Raw,
    container: Vec<u8>,
    parts: BTreeMap<String, Vec<u8>>,
    text: String,
}
fn metadata_identity(raw: &Raw, deadline: &Deadline) -> Result<String> {
    use sha2::{Digest, Sha256};
    struct IdentityWriter<'a> {
        bytes: usize,
        digest: Sha256,
        deadline: &'a Deadline,
    }
    impl Write for IdentityWriter<'_> {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.deadline.check().map_err(std::io::Error::other)?;
            // Filling the initially empty ID adds exactly 64 JSON ASCII bytes.
            if bytes.len() > (MAX_GRAPH - 64).saturating_sub(self.bytes) {
                return Err(std::io::Error::other(
                    "Document graph metadata capacity exceeded",
                ));
            }
            self.bytes += bytes.len();
            self.digest.update(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut writer = IdentityWriter {
        bytes: 0,
        digest: Sha256::new(),
        deadline,
    };
    serde_json::to_writer(&mut writer, raw)?;
    Ok(hex::encode(writer.digest.finalize()))
}
fn derive(asset: Asset, container: Vec<u8>, deadline: &Deadline) -> Result<CheckedDocumentGraph> {
    deadline.check()?;
    crate::media::validate_asset(&asset)?;
    ensure!(
        container.len() == asset.size && hash(&container) == asset.source_hash,
        "Document container binding mismatch"
    );
    let format = Path::new(&asset.path)
        .extension()
        .and_then(|v| v.to_str())
        .context("Document extension absent")?
        .to_ascii_lowercase();
    ensure!(
        matches!(format.as_str(), "docx" | "xlsx"),
        "Document graph supports DOCX and XLSX only"
    );
    let extracted = crate::office::extract_document(&container, &format, deadline)?;
    let mut raw=Raw{schema_version:1,id:String::new(),producer:PRODUCER.into(),asset:asset.clone(),format,parts:vec![],decoded_text_hash:String::new(),decoded_text_bytes:0,separator_policy:"single_lf_between_segments".into(),segments:vec![],nodes:vec![],observations:vec![],limitations:vec!["XML origins are exact enclosing elements, not a character-by-character map into the container.".into(),"Decoded text and cached cell values are not rendered layout, evaluated formulas, OCR or execution evidence.".into(),"Document graph identity is separate from canonical SourceSnapshot and GraphView identities.".into()]};
    let asset_node = identity(&(PRODUCER, &asset.id, "asset"));
    raw.nodes.push(Node {
        id: asset_node.clone(),
        kind: "documentAsset".into(),
        name: asset.path.clone(),
        part: None,
        segment_id: None,
        parent_id: None,
    });
    let mut part_ids = BTreeMap::new();
    for (name, bytes) in &extracted.parts {
        deadline.check()?;
        let part_hash = hash(bytes);
        let id = identity(&(PRODUCER, &asset.id, "part", name, &part_hash));
        raw.parts.push(Part {
            name: name.clone(),
            hash: part_hash,
            size: bytes.len(),
        });
        raw.nodes.push(Node {
            id: id.clone(),
            kind: "documentPart".into(),
            name: name.clone(),
            part: Some(name.clone()),
            segment_id: None,
            parent_id: Some(asset_node.clone()),
        });
        part_ids.insert(name.clone(), id.clone());
        raw.observations.push(Arc {
            id: identity(&(PRODUCER, &asset.id, "containsPart", &id)),
            relation: "containsPart".into(),
            source: asset_node.clone(),
            target: id,
            xml_origins: vec![],
        });
    }
    let mut text = String::new();
    let mut previous = BTreeMap::<String, String>::new();
    for (ordinal, input) in extracted.segments.into_iter().enumerate() {
        deadline.check()?;
        if ordinal > 0 {
            text.push('\n');
        }
        let start = text.len();
        text.push_str(&input.text);
        let end = text.len();
        ensure!(
            text.len() <= 2_020_000,
            "Document decoded text capacity exceeded"
        );
        let id = identity(&(
            PRODUCER,
            &asset.id,
            "segment",
            ordinal,
            &input.origins,
            hash(&input.text),
            &input.sheet,
            &input.cell,
            &input.formula,
        ));
        let kind = if input.cell.is_some() {
            "cell"
        } else {
            "paragraph"
        };
        let parent = part_ids
            .get(&input.part)
            .context("Document primary part absent")?;
        let label = if let Some(cell) = &input.cell {
            format!("{}!{cell}", input.sheet.as_deref().unwrap_or("worksheet"))
        } else {
            input.text.chars().take(160).collect()
        };
        raw.nodes.push(Node {
            id: id.clone(),
            kind: kind.into(),
            name: label,
            part: Some(input.part.clone()),
            segment_id: Some(id.clone()),
            parent_id: Some(parent.clone()),
        });
        raw.observations.push(Arc {
            id: identity(&(PRODUCER, &asset.id, "containsSegment", &id)),
            relation: "containsSegment".into(),
            source: parent.clone(),
            target: id.clone(),
            xml_origins: input.origins.clone(),
        });
        if let Some(before) = previous.insert(input.part.clone(), id.clone()) {
            raw.observations.push(Arc {
                id: identity(&(PRODUCER, &asset.id, "next", &before, &id)),
                relation: "next".into(),
                source: before,
                target: id.clone(),
                xml_origins: input.origins.clone(),
            });
        }
        for origin in input
            .origins
            .iter()
            .filter(|origin| origin.role == "sharedStringValue")
        {
            raw.observations.push(Arc {
                id: identity(&(PRODUCER, &asset.id, "valueFrom", &id, &origin.part)),
                relation: "valueFrom".into(),
                source: id.clone(),
                target: part_ids[&origin.part].clone(),
                xml_origins: vec![origin.clone()],
            });
        }
        raw.segments.push(Segment {
            id,
            kind: kind.into(),
            part: input.part,
            start_byte: start,
            end_byte: end,
            text_hash: hash(&input.text),
            xml_origins: input.origins,
            sheet: input.sheet,
            cell: input.cell,
            cell_type: input.cell_type,
            formula: input.formula,
        });
    }
    ensure!(
        raw.nodes.len() <= 25000 && raw.observations.len() <= 100000,
        "Document graph capacity exceeded"
    );
    raw.decoded_text_hash = hash(&text);
    raw.decoded_text_bytes = text.len();
    raw.id = metadata_identity(&raw, deadline)?;
    Ok(CheckedDocumentGraph {
        raw,
        container,
        parts: extracted.parts,
        text,
    })
}
pub fn describe(graph: &CheckedDocumentGraph) -> Value {
    json!({"documentGraphId":graph.raw.id,"assetId":graph.raw.asset.id,"workspaceId":graph.raw.asset.workspace_id,"path":graph.raw.asset.path,"sourceHash":graph.raw.asset.source_hash,"format":graph.raw.format,"nodeCount":graph.raw.nodes.len(),"segmentCount":graph.raw.segments.len(),"decodedTextHash":graph.raw.decoded_text_hash,"decodedTextBytes":graph.raw.decoded_text_bytes,"producer":PRODUCER,"complete":false,"sourceWritten":false})
}
pub fn build(engine: &Engine, path: &str, deadline: &Deadline) -> Result<Value> {
    ensure!(
        Path::new(path)
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("docx")
                || extension.eq_ignore_ascii_case("xlsx")),
        "Document graph supports DOCX and XLSX only"
    );
    let inspected = crate::media::inspect(engine, path, deadline)?;
    let (asset, container) = crate::media::checked_asset(
        &engine.store,
        inspected["assetId"]
            .as_str()
            .context("Captured asset ID absent")?,
        deadline,
    )?;
    let graph = derive(asset, container, deadline)?;
    let store = &engine.store;
    store.set_deadline(deadline)?;
    let tx =
        rusqlite::Transaction::new_unchecked(&store.db, rusqlite::TransactionBehavior::Immediate)?;
    for bytes in graph.parts.values() {
        deadline.check()?;
        store.artifact(bytes)?;
    }
    store.artifact(graph.text.as_bytes())?;
    let artifact = store.artifact(&serde_json::to_vec(&graph.raw)?)?;
    let registered = Record {
        schema_version: 1,
        document_graph_id: graph.raw.id.clone(),
        workspace_id: graph.raw.asset.workspace_id.clone(),
        asset_id: graph.raw.asset.id.clone(),
        artifact_id: artifact.clone(),
        container_hash: graph.raw.asset.source_hash.clone(),
        inspection_artifact_id: inspected["artifactId"]
            .as_str()
            .context("Office inspection artifact missing")?
            .into(),
    };
    use rusqlite::OptionalExtension;
    let sql_limit = rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH;
    let prior = store
        .db
        .set_limit(sql_limit, store.db.limit(sql_limit)?.min(64000))?;
    let existing: rusqlite::Result<Option<String>> = store
        .db
        .query_row(
            "SELECT value FROM records WHERE kind=?1 AND id=?2",
            rusqlite::params![RECORD_KIND, graph.raw.id],
            |row| row.get(0),
        )
        .optional();
    store.db.set_limit(sql_limit, prior)?;
    let existing = existing?;
    let raw = serde_json::to_string(&registered)?;
    if let Some(existing) = existing {
        ensure!(
            existing == raw,
            "Existing document graph registration differs"
        );
    } else {
        store.record(RECORD_KIND, &graph.raw.id, &registered)?;
    }
    deadline.check()?;
    tx.commit()?;
    let mut result = describe(&graph);
    result["artifactId"] = json!(artifact);
    Ok(result)
}
fn record(store: &Store, id: &str, deadline: &Deadline) -> Result<Record> {
    ensure!(valid_id(id), "Invalid document graph ID");
    store.set_deadline(deadline)?;
    let limit = rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH;
    let prior = store
        .db
        .set_limit(limit, store.db.limit(limit)?.min(64000))?;
    let raw: rusqlite::Result<String> = store.db.query_row(
        "SELECT value FROM records WHERE kind=?1 AND id=?2",
        rusqlite::params![RECORD_KIND, id],
        |row| row.get(0),
    );
    store.db.set_limit(limit, prior)?;
    let raw = raw?;
    let result: Record = crate::json::decode(crate::json::parse(raw.as_bytes(), 64000)?)?;
    ensure!(
        result.schema_version == 1
            && result.document_graph_id == id
            && result.workspace_id == store.workspace_id
            && valid_id(&result.artifact_id),
        "Document graph registration identity mismatch"
    );
    Ok(result)
}
pub fn load(store: &Store, id: &str, deadline: &Deadline) -> Result<CheckedDocumentGraph> {
    let registered = record(store, id, deadline)?;
    let (asset, container) = crate::media::checked_asset(store, &registered.asset_id, deadline)?;
    ensure!(
        asset.source_hash == registered.container_hash,
        "Document source registration differs"
    );
    let graph = derive(asset, container, deadline)?;
    let inspection = serde_json::to_vec(&crate::media::office_inspection_data(
        &graph.raw.asset,
        &graph.container,
        deadline,
    )?)?;
    ensure!(
        hash(&inspection) == registered.inspection_artifact_id
            && store.read_artifact_limited(&registered.inspection_artifact_id, 8_000_000)?
                == inspection,
        "Document inspection artifact differs from exact container"
    );
    let expected = serde_json::to_vec(&graph.raw)?;
    ensure!(
        graph.raw.id == id && hash(&expected) == registered.artifact_id,
        "Document graph claims differ from original container reconstruction"
    );
    ensure!(
        store.read_artifact_limited(&registered.artifact_id, MAX_GRAPH)? == expected,
        "Document graph artifact differs"
    );
    for part in &graph.raw.parts {
        deadline.check()?;
        ensure!(
            store.read_artifact_limited(&part.hash, 16_000_000)? == graph.parts[&part.name],
            "Document XML part artifact differs"
        );
    }
    ensure!(
        store.read_artifact_limited(&graph.raw.decoded_text_hash, 2_020_000)?
            == graph.text.as_bytes(),
        "Document decoded text artifact differs"
    );
    Ok(graph)
}
fn fields(args: &Value, allowed: &[&str]) -> Result<()> {
    ensure!(
        args.as_object()
            .is_some_and(|object| object.keys().all(|key| allowed.contains(&key.as_str()))),
        "Unknown document query field"
    );
    Ok(())
}
fn maximum(args: &Value, key: &str, default: usize, max: usize) -> Result<usize> {
    let value = match args.get(key) {
        Some(value) => usize::try_from(value.as_u64().context("Invalid document range/capacity")?)?,
        None => default,
    };
    ensure!(value <= max, "Document query capacity exceeded");
    Ok(value)
}
fn bounded(value: Value) -> Result<Value> {
    struct Capacity(usize);
    impl Write for Capacity {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > 4_000_000usize.saturating_sub(self.0) {
                return Err(std::io::Error::other(
                    "Document result exceeds output capacity; refine the query",
                ));
            }
            self.0 += bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    // Count exact JSON escaping without allocating a serialized result first.
    serde_json::to_writer(&mut Capacity(0), &value)?;
    Ok(value)
}
pub fn find(graph: &CheckedDocumentGraph, args: &Value, deadline: &Deadline) -> Result<Value> {
    fields(args, &["query", "limit", "offset", "ignoreCase"])?;
    let query = args["query"]
        .as_str()
        .filter(|text| !text.is_empty() && text.len() <= 4096)
        .context("Document search query required")?;
    ensure!(
        args.get("ignoreCase").is_none_or(Value::is_boolean),
        "Invalid document case option"
    );
    let expression = regex::RegexBuilder::new(&regex::escape(query))
        .case_insensitive(args["ignoreCase"] != false)
        .size_limit(1_000_000)
        .build()?;
    let limit = maximum(args, "limit", 30, 2000)?;
    ensure!(limit > 0, "Document search limit must be positive");
    let offset = maximum(args, "offset", 0, 100000)?;
    let mut items = vec![];
    let mut total = 0usize;
    for segment in &graph.raw.segments {
        deadline.check()?;
        for found in expression.find_iter(&graph.text[segment.start_byte..segment.end_byte]) {
            deadline.check()?;
            if total >= offset && items.len() < limit {
                items.push(json!({"segmentId":segment.id,"sourceSpace":"decodedText","path":graph.raw.asset.path,"assetId":graph.raw.asset.id,"text":found.as_str(),"startByte":segment.start_byte+found.start(),"endByte":segment.start_byte+found.end(),"contentHash":graph.raw.decoded_text_hash,"xmlOrigins":segment.xml_origins}));
            }
            total += 1;
            ensure!(total <= 100000, "Document match capacity exceeded");
        }
    }
    bounded(
        json!({"documentGraphId":graph.raw.id,"items":items,"total":total,"truncated":total>offset+limit,"sourceSpace":"decodedText","complete":false}),
    )
}
pub fn read(graph: &CheckedDocumentGraph, args: &Value, deadline: &Deadline) -> Result<Value> {
    fields(
        args,
        &["sourceSpace", "segmentId", "part", "startByte", "endByte"],
    )?;
    deadline.check()?;
    ensure!(
        args.get("sourceSpace").is_none_or(Value::is_string),
        "Document sourceSpace must be a string"
    );
    let space = args["sourceSpace"].as_str().unwrap_or("decodedText");
    if space == "partXml" {
        ensure!(
            args.get("segmentId").is_none(),
            "XML reads cannot select a decoded segment"
        );
        let name = args["part"]
            .as_str()
            .context("Document XML part required")?;
        let source = graph.parts.get(name).context("Document XML part absent")?;
        let text = std::str::from_utf8(source)?;
        let start = maximum(args, "startByte", 0, text.len())?;
        let end = maximum(args, "endByte", text.len(), text.len())?;
        crate::paths::utf8_range(text, start, end)?;
        ensure!(
            end - start <= 256000,
            "Document read range exceeds capacity"
        );
        return bounded(
            json!({"documentGraphId":graph.raw.id,"assetId":graph.raw.asset.id,"sourceSpace":"partXml","part":name,"contentHash":hash(source),"containerHash":graph.raw.asset.source_hash,"startByte":start,"endByte":end,"text":text[start..end]}),
        );
    }
    ensure!(
        space == "decodedText",
        "Unsupported document coordinate space"
    );
    ensure!(
        args.get("part").is_none(),
        "Decoded reads cannot select an XML part"
    );
    let segment = graph
        .raw
        .segments
        .iter()
        .find(|segment| Some(segment.id.as_str()) == args["segmentId"].as_str())
        .context("Document segment absent")?;
    let start = maximum(args, "startByte", segment.start_byte, segment.end_byte)?;
    let end = maximum(args, "endByte", segment.end_byte, segment.end_byte)?;
    ensure!(
        start >= segment.start_byte && end >= start && end - start <= 256000,
        "Document read range outside segment or capacity"
    );
    crate::paths::utf8_range(&graph.text, start, end)?;
    bounded(
        json!({"documentGraphId":graph.raw.id,"assetId":graph.raw.asset.id,"segmentId":segment.id,"sourceSpace":"decodedText","contentHash":graph.raw.decoded_text_hash,"containerHash":graph.raw.asset.source_hash,"startByte":start,"endByte":end,"text":graph.text[start..end],"xmlOrigins":segment.xml_origins,"precision":"exact_decoded_text_with_enclosing_xml_origins"}),
    )
}
pub fn trace(graph: &CheckedDocumentGraph, args: &Value, deadline: &Deadline) -> Result<Value> {
    fields(args, &["nodeIds", "direction", "depth", "maxVisited"])?;
    let ids = args["nodeIds"]
        .as_array()
        .filter(|ids| !ids.is_empty() && ids.len() <= 100)
        .context("Document traversal seeds required")?;
    ensure!(
        args.get("direction").is_none_or(Value::is_string),
        "Document direction must be a string"
    );
    let direction = args["direction"].as_str().unwrap_or("both");
    ensure!(
        ["in", "out", "both"].contains(&direction),
        "Invalid document traversal direction"
    );
    let depth = maximum(args, "depth", 2, 8)?;
    let max = maximum(args, "maxVisited", 2000, 20000)?;
    ensure!(max > 0, "Document traversal capacity must be positive");
    let mut seen = BTreeSet::new();
    let mut queue = VecDeque::new();
    for id in ids {
        let id = id.as_str().context("Invalid document node ID")?;
        ensure!(
            graph.raw.nodes.iter().any(|node| node.id == id),
            "Document seed absent"
        );
        if seen.insert(id.to_owned()) {
            queue.push_back((id.to_owned(), 0));
        }
    }
    ensure!(
        seen.len() <= max,
        "Document seeds exceed traversal capacity"
    );
    let mut neighbors = BTreeMap::<&str, Vec<&str>>::new();
    for edge in &graph.raw.observations {
        deadline.check()?;
        if direction != "in" {
            neighbors
                .entry(&edge.source)
                .or_default()
                .push(&edge.target);
        }
        if direction != "out" {
            neighbors
                .entry(&edge.target)
                .or_default()
                .push(&edge.source);
        }
    }
    let mut truncated = false;
    while let Some((id, level)) = queue.pop_front() {
        deadline.check()?;
        if level == depth {
            continue;
        }
        for next in neighbors.get(id.as_str()).into_iter().flatten() {
            deadline.check()?;
            if !seen.contains(*next) {
                if seen.len() == max {
                    truncated = true;
                } else {
                    seen.insert((*next).to_owned());
                    queue.push_back(((*next).to_owned(), level + 1));
                }
            }
        }
    }
    bounded(
        json!({"documentGraphId":graph.raw.id,"nodes":graph.raw.nodes.iter().filter(|node|seen.contains(&node.id)).collect::<Vec<_>>(),"observations":graph.raw.observations.iter().filter(|edge|seen.contains(&edge.source)&&seen.contains(&edge.target)).collect::<Vec<_>>(),"truncated":truncated,"complete":false}),
    )
}
pub fn explorer(graph: &CheckedDocumentGraph, deadline: &Deadline) -> Result<Vec<u8>> {
    crate::export::serialize_document_explorer(graph, deadline)
}
pub fn explorer_payload(graph: &CheckedDocumentGraph, deadline: &Deadline) -> Result<Value> {
    deadline.check()?;
    Ok(
        json!({"documentGraph":graph.raw,"decodedText":graph.text,"sourceSpaces":["opaqueContainer","partXml","decodedText"]}),
    )
}
fn safe_directory(directory: &Path) -> Result<()> {
    ensure!(
        directory.is_absolute(),
        "Document archive requires an absolute host directory"
    );
    for ancestor in directory.ancestors() {
        let metadata = fs::symlink_metadata(ancestor)?;
        ensure!(
            metadata.is_dir() && !crate::paths::is_link(&metadata),
            "Document archive directory link refused"
        );
    }
    Ok(())
}
fn create_only(path: &Path, bytes: &[u8], deadline: &Deadline) -> Result<()> {
    deadline.check()?;
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(mut file) => {
            for chunk in bytes.chunks(65536) {
                deadline.check()?;
                file.write_all(chunk)?;
            }
            file.sync_all()?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            ensure!(
                read_file(path, bytes.len(), deadline)? == bytes,
                "Archive destination contains different bytes"
            );
        }
        Err(error) => return Err(error.into()),
    }
    Ok(())
}
fn read_file(path: &Path, max: usize, deadline: &Deadline) -> Result<Vec<u8>> {
    deadline.check()?;
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file() && !crate::paths::is_link(&metadata) && metadata.len() <= max as u64,
        "Invalid document archive file size/type"
    );
    let mut file = fs::File::open(path)?;
    let mut bytes = vec![];
    let mut chunk = [0; 65536];
    loop {
        deadline.check()?;
        let count = file.read(&mut chunk)?;
        if count == 0 {
            break;
        }
        ensure!(
            bytes.len() + count <= max,
            "Document archive file grew beyond capacity"
        );
        bytes.extend_from_slice(&chunk[..count]);
    }
    ensure!(
        bytes.len() as u64 == metadata.len(),
        "Document archive file changed"
    );
    Ok(bytes)
}
pub fn write_archive(
    graph: &CheckedDocumentGraph,
    directory: &Path,
    deadline: &Deadline,
) -> Result<Value> {
    safe_directory(directory)?;
    let objects = directory.join("objects");
    if !objects.exists() {
        fs::create_dir(&objects)?;
    }
    safe_directory(&objects)?;
    create_only(
        &objects.join(&graph.raw.asset.source_hash),
        &graph.container,
        deadline,
    )?;
    for bytes in graph.parts.values() {
        deadline.check()?;
        create_only(&objects.join(hash(bytes)), bytes, deadline)?;
    }
    create_only(
        &objects.join(&graph.raw.decoded_text_hash),
        graph.text.as_bytes(),
        deadline,
    )?;
    create_only(
        &directory.join("document-graph.json"),
        &serde_json::to_vec(&graph.raw)?,
        deadline,
    )?;
    Ok(
        json!({"documentGraphId":graph.raw.id,"directory":directory,"format":"yoke-document-archive-1","sourceWritten":false}),
    )
}
pub fn load_archive(directory: &Path, deadline: &Deadline) -> Result<CheckedDocumentGraph> {
    safe_directory(directory)?;
    let bytes = read_file(&directory.join("document-graph.json"), MAX_GRAPH, deadline)?;
    let raw: Raw = crate::json::decode(crate::json::parse(&bytes, MAX_GRAPH)?)?;
    crate::media::validate_asset(&raw.asset)?;
    let objects = directory.join("objects");
    safe_directory(&objects)?;
    let container = read_file(
        &objects.join(&raw.asset.source_hash),
        raw.asset.size,
        deadline,
    )?;
    let graph = derive(raw.asset.clone(), container, deadline)?;
    ensure!(
        raw == graph.raw,
        "Document archive claims differ from exact container reconstruction"
    );
    for part in &raw.parts {
        ensure!(valid_id(&part.hash), "Invalid archived XML hash");
        ensure!(
            read_file(&objects.join(&part.hash), part.size, deadline)? == graph.parts[&part.name],
            "Archived XML part differs"
        );
    }
    ensure!(
        read_file(
            &objects.join(&raw.decoded_text_hash),
            raw.decoded_text_bytes,
            deadline
        )? == graph.text.as_bytes(),
        "Archived decoded text differs"
    );
    Ok(graph)
}
pub(crate) fn validate_retention_record(
    store: &Store,
    kind: &str,
    id: &str,
    value: &Value,
    deadline: &Deadline,
) -> Result<Option<Vec<String>>> {
    if kind != RECORD_KIND {
        return Ok(None);
    }
    let registered = record(store, id, deadline)?;
    ensure!(
        serde_json::to_value(&registered)? == *value,
        "Document retention registration differs"
    );
    let graph = load(store, id, deadline)?;
    let mut artifacts = vec![
        registered.inspection_artifact_id,
        registered.artifact_id,
        graph.raw.asset.source_hash.clone(),
        graph.raw.decoded_text_hash.clone(),
    ];
    artifacts.extend(graph.raw.parts.iter().map(|part| part.hash.clone()));
    Ok(Some(artifacts))
}
