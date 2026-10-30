//! Candidate reservations precede native replay and original-container reads.
use super::*;
use crate::mixed_view::{DocumentPin, Options, capacity, check, failure, parent, pin};
use rusqlite::{OptionalExtension, limits::Limit};

struct LengthLimit<'a> {
    db: &'a rusqlite::Connection,
    original: i32,
}
impl Drop for LengthLimit<'_> {
    fn drop(&mut self) {
        let _ = self.db.set_limit(Limit::SQLITE_LIMIT_LENGTH, self.original);
    }
}
struct Budget {
    metadata_bytes: usize,
    metadata_limit: usize,
    source_bytes: usize,
    source_limit: usize,
    rows: BTreeMap<(String, String), String>,
    metadata: BTreeMap<String, usize>,
    sources: BTreeMap<String, usize>,
}
impl Budget {
    fn charge_metadata(&mut self, n: usize) -> Result<()> {
        self.metadata_bytes = self
            .metadata_bytes
            .checked_add(n)
            .ok_or_else(|| failure("mixed_view_capacity_exceeded"))?;
        capacity(self.metadata_bytes <= self.metadata_limit)
    }
    fn row(&mut self, store: &Store, kind: &str, id: &str, d: &Deadline) -> Result<String> {
        check(d)?;
        let key = (kind.into(), id.into());
        if let Some(raw) = self.rows.get(&key) {
            return Ok(raw.clone());
        }
        let size: Option<usize> = store
            .db
            .query_row(
                "SELECT length(CAST(value AS BLOB)) FROM records WHERE kind=?1 AND id=?2",
                rusqlite::params![kind, id],
                |row| row.get(0),
            )
            .optional()?;
        let size = size.ok_or_else(|| failure("mixed_view_parent_refused"))?;
        capacity(size <= 64000)?;
        self.charge_metadata(size)?;
        let original = store.db.set_limit(
            Limit::SQLITE_LIMIT_LENGTH,
            store.db.limit(Limit::SQLITE_LIMIT_LENGTH)?.min(65536),
        )?;
        let _limit = LengthLimit {
            db: &store.db,
            original,
        };
        let raw: String = store.db.query_row(
            "SELECT value FROM records WHERE kind=?1 AND id=?2 AND length(CAST(value AS BLOB))=?3",
            rusqlite::params![kind, id, size],
            |row| row.get(0),
        )?;
        parent(raw.len() == size)?;
        check(d)?;
        self.rows.insert(key, raw.clone());
        Ok(raw)
    }
    fn registered_size(&mut self, store: &Store, id: &str, d: &Deadline) -> Result<usize> {
        parent(valid_id(id))?;
        let raw = self.row(store, "artifact", id, d)?;
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Registration {
            artifact_id: String,
            total_bytes: usize,
        }
        let record: Registration = crate::json::decode(crate::json::parse(raw.as_bytes(), 64000)?)?;
        parent(record.artifact_id == id)?;
        capacity(record.total_bytes <= 512_000_000)?;
        check(d)?;
        Ok(record.total_bytes)
    }
    fn metadata_object(
        &mut self,
        store: &Store,
        id: &str,
        maximum: usize,
        d: &Deadline,
    ) -> Result<usize> {
        check(d)?;
        parent(valid_id(id))?;
        let size = self.registered_size(store, id, d)?;
        capacity(size <= maximum)?;
        if let Some(old) = self.metadata.get(id) {
            parent(*old == size)?;
        } else {
            self.charge_metadata(size)?;
            self.metadata.insert(id.into(), size);
        }
        Ok(size)
    }
    fn source_object(&mut self, store: &Store, id: &str, size: usize, d: &Deadline) -> Result<()> {
        check(d)?;
        parent(valid_id(id) && self.registered_size(store, id, d)? == size)?;
        if let Some(old) = self.sources.get(id) {
            parent(*old == size)?;
        } else {
            self.source_bytes = self
                .source_bytes
                .checked_add(size)
                .ok_or_else(|| failure("mixed_view_capacity_exceeded"))?;
            capacity(self.source_bytes <= self.source_limit)?;
            self.sources.insert(id.into(), size);
        }
        Ok(())
    }
}
pub(crate) struct PreparedDocuments {
    pins: Vec<DocumentPin>,
    metadata_bytes: usize,
    source_bytes: usize,
}
impl PreparedDocuments {
    pub(crate) fn metadata_bytes(&self) -> usize {
        self.metadata_bytes
    }
    pub(crate) fn source_bytes(&self) -> usize {
        self.source_bytes
    }
    pub(crate) fn load(self, store: &Store, d: &Deadline) -> Result<Vec<CheckedDocumentGraph>> {
        let mut graphs = Vec::new();
        for pin in self.pins {
            check(d)?;
            let graph = super::load(store, &pin.document_graph_id, d)?;
            parent(
                graph.raw.id == pin.document_graph_id
                    && graph.raw.asset.source_hash == pin.container_hash
                    && graph.raw.decoded_text_hash == pin.decoded_text_hash,
            )?;
            graphs.push(graph);
        }
        check(d)?;
        Ok(graphs)
    }
}
pub(crate) fn prepare(
    store: &Store,
    pins: &[DocumentPin],
    options: &Options,
    d: &Deadline,
) -> Result<PreparedDocuments> {
    let mut b = Budget {
        metadata_bytes: 0,
        metadata_limit: options.max_metadata_bytes,
        source_bytes: 0,
        source_limit: options.max_source_bytes,
        rows: BTreeMap::new(),
        metadata: BTreeMap::new(),
        sources: BTreeMap::new(),
    };
    for p in pins {
        check(d)?;
        let raw = b.row(store, RECORD_KIND, &p.document_graph_id, d)?;
        let value = crate::json::parse(raw.as_bytes(), 64000)?;
        let r: Record = crate::json::decode(value.clone())?;
        parent(
            serde_json::to_value(&r)? == value
                && r.schema_version == 1
                && r.document_graph_id == p.document_graph_id
                && r.workspace_id == store.workspace_id
                && valid_id(&r.asset_id)
                && valid_id(&r.inspection_artifact_id)
                && valid_id(&r.container_hash),
        )?;
        pin(r.artifact_id == p.artifact_id && r.container_hash == p.container_hash)?;
        let raw = b.row(store, "native_media_asset_v1", &r.asset_id, d)?;
        let value = crate::json::parse(raw.as_bytes(), 64000)?;
        let asset: Asset = crate::json::decode(value.clone())?;
        parent(serde_json::to_value(&asset)? == value)?;
        crate::media::validate_asset(&asset)?;
        parent(
            asset.id == r.asset_id
                && asset.workspace_id == store.workspace_id
                && asset.source_hash == r.container_hash,
        )?;
        b.metadata_object(store, &r.inspection_artifact_id, 8_000_000, d)?;
        let graph_size = b.metadata_object(store, &r.artifact_id, MAX_GRAPH, d)?;
        let graph_raw = store.read_artifact_limited(&r.artifact_id, graph_size)?;
        parent(graph_raw.len() == graph_size)?;
        let value = crate::json::parse(&graph_raw, graph_size)?;
        let graph: Raw = crate::json::decode(value.clone())?;
        parent(
            serde_json::to_value(&graph)? == value
                && graph.schema_version == 1
                && graph.id == p.document_graph_id
                && graph.asset == asset,
        )?;
        pin(graph.decoded_text_hash == p.decoded_text_hash)?;
        capacity(
            graph.parts.len() <= 4096
                && graph.segments.len() <= 20_000
                && graph.nodes.len() <= 25_000
                && graph.observations.len() <= 100_000
                && graph.decoded_text_bytes <= 2_020_000,
        )?;
        parent(matches!(graph.format.as_str(), "docx" | "xlsx"))?;
        b.source_object(store, &asset.source_hash, asset.size, d)?;
        let mut names = BTreeSet::new();
        for part in &graph.parts {
            check(d)?;
            parent(names.insert(&part.name))?;
            capacity(part.size <= 16_000_000)?;
            b.source_object(store, &part.hash, part.size, d)?;
        }
        b.source_object(store, &graph.decoded_text_hash, graph.decoded_text_bytes, d)?;
    }
    check(d)?;
    Ok(PreparedDocuments {
        pins: pins.to_vec(),
        metadata_bytes: b.metadata_bytes,
        source_bytes: b.source_bytes,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Header<'a> {
    pub document_graph_id: &'a str,
    pub asset_id: &'a str,
    pub workspace_id: &'a str,
    pub path: &'a str,
    pub container_hash: &'a str,
    pub decoded_text_hash: &'a str,
    pub format: &'a str,
}
pub(crate) fn header(g: &CheckedDocumentGraph) -> Header<'_> {
    Header {
        document_graph_id: &g.raw.id,
        asset_id: &g.raw.asset.id,
        workspace_id: &g.raw.asset.workspace_id,
        path: &g.raw.asset.path,
        container_hash: &g.raw.asset.source_hash,
        decoded_text_hash: &g.raw.decoded_text_hash,
        format: &g.raw.format,
    }
}
pub(crate) struct DecodedSegment<'a> {
    pub id: &'a str,
    pub start: usize,
    pub text: &'a str,
    pub xml_origins: &'a [XmlOrigin],
}
pub(crate) fn segments(g: &CheckedDocumentGraph) -> impl Iterator<Item = DecodedSegment<'_>> {
    g.raw.segments.iter().map(|s| DecodedSegment {
        id: &s.id,
        start: s.start_byte,
        text: &g.text[s.start_byte..s.end_byte],
        xml_origins: &s.xml_origins,
    })
}
pub(crate) fn read_decoded<'a>(
    g: &'a CheckedDocumentGraph,
    id: &str,
    content_hash: &str,
    start: usize,
    end: usize,
) -> Result<(&'a str, &'a [XmlOrigin])> {
    pin(content_hash == g.raw.decoded_text_hash)?;
    let segment = g
        .raw
        .segments
        .iter()
        .find(|s| s.id == id)
        .ok_or_else(|| failure("mixed_view_input_invalid"))?;
    crate::mixed_view::input(
        start >= segment.start_byte
            && end <= segment.end_byte
            && start <= end
            && end - start <= 256000
            && g.text.is_char_boundary(start)
            && g.text.is_char_boundary(end),
    )?;
    Ok((&g.text[start..end], &segment.xml_origins))
}
pub(crate) fn read_xml<'a>(
    g: &'a CheckedDocumentGraph,
    part: &str,
    content_hash: &str,
    start: usize,
    end: usize,
) -> Result<&'a str> {
    let bytes = g
        .parts
        .get(part)
        .ok_or_else(|| failure("mixed_view_input_invalid"))?;
    pin(hash(bytes) == content_hash)?;
    let text = std::str::from_utf8(bytes)?;
    crate::mixed_view::input(
        start <= end
            && end <= text.len()
            && end - start <= 256000
            && text.is_char_boundary(start)
            && text.is_char_boundary(end),
    )?;
    Ok(&text[start..end])
}
