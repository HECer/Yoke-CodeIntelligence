use super::*;
use crate::{document_graph::mixed_input as documents, office::XmlOrigin};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum SourceRef {
    SourceUtf8 {
        view_id: String,
        snapshot_id: String,
        path: String,
        content_hash: String,
        start_byte: usize,
        end_byte: usize,
    },
    DocumentDecodedText {
        document_graph_id: String,
        segment_id: String,
        content_hash: String,
        start_byte: usize,
        end_byte: usize,
    },
    DocumentPartXml {
        document_graph_id: String,
        part: String,
        content_hash: String,
        start_byte: usize,
        end_byte: usize,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(
    tag = "operation",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Query {
    Describe {},
    Find {
        query: String,
        #[serde(default)]
        ignore_case: bool,
        #[serde(default)]
        offset: usize,
        #[serde(default = "default_limit")]
        limit: usize,
    },
    Read {
        source: SourceRef,
    },
}
fn default_limit() -> usize {
    30
}
pub fn validate_query(q: &Query, s: &Selection, d: &Deadline) -> Result<()> {
    check(d)?;
    match q {
        Query::Describe {} => (),
        Query::Find {
            query,
            offset,
            limit,
            ..
        } => {
            input(query.len() <= 4096)?;
            input(
                !query.trim().is_empty()
                    && !crate::paths::obvious_secret(query)
                    && !query
                        .chars()
                        .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
                    && *offset <= 100000
                    && (1..=2000).contains(limit),
            )?;
        }
        Query::Read { source } => {
            let (start, end) = match source {
                SourceRef::SourceUtf8 {
                    view_id,
                    snapshot_id,
                    path,
                    content_hash,
                    start_byte,
                    end_byte,
                } => {
                    input(
                        view_id == &s.source.view_id
                            && snapshot_id == &s.source.snapshot_id
                            && valid_id(content_hash)
                            && path.len() <= 4096
                            && crate::paths::normalized_syntax(path).is_ok_and(|p| p == *path),
                    )?;
                    (*start_byte, *end_byte)
                }
                SourceRef::DocumentDecodedText {
                    document_graph_id,
                    segment_id,
                    content_hash,
                    start_byte,
                    end_byte,
                } => {
                    input(
                        s.documents.iter().any(|p| {
                            p.document_graph_id == *document_graph_id
                                && p.decoded_text_hash == *content_hash
                        }) && valid_id(segment_id)
                            && valid_id(content_hash),
                    )?;
                    (*start_byte, *end_byte)
                }
                SourceRef::DocumentPartXml {
                    document_graph_id,
                    part,
                    content_hash,
                    start_byte,
                    end_byte,
                } => {
                    input(
                        s.documents
                            .iter()
                            .any(|p| p.document_graph_id == *document_graph_id)
                            && valid_id(content_hash)
                            && part.len() <= 1024
                            && crate::paths::normalized_syntax(part).is_ok_and(|p| p == *part),
                    )?;
                    (*start_byte, *end_byte)
                }
            };
            input(start <= end && end - start <= 256000)?;
        }
    }
    check(d)
}
#[derive(Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum Reference<'a> {
    SourceUtf8 {
        view_id: &'a str,
        snapshot_id: &'a str,
        path: &'a str,
        content_hash: &'a str,
        start_byte: usize,
        end_byte: usize,
    },
    DocumentDecodedText {
        document_graph_id: &'a str,
        segment_id: &'a str,
        content_hash: &'a str,
        start_byte: usize,
        end_byte: usize,
    },
    DocumentPartXml {
        document_graph_id: &'a str,
        part: &'a str,
        content_hash: &'a str,
        start_byte: usize,
        end_byte: usize,
    },
}
impl<'a> From<&'a SourceRef> for Reference<'a> {
    fn from(r: &'a SourceRef) -> Self {
        match r {
            SourceRef::SourceUtf8 {
                view_id,
                snapshot_id,
                path,
                content_hash,
                start_byte,
                end_byte,
            } => Self::SourceUtf8 {
                view_id,
                snapshot_id,
                path,
                content_hash,
                start_byte: *start_byte,
                end_byte: *end_byte,
            },
            SourceRef::DocumentDecodedText {
                document_graph_id,
                segment_id,
                content_hash,
                start_byte,
                end_byte,
            } => Self::DocumentDecodedText {
                document_graph_id,
                segment_id,
                content_hash,
                start_byte: *start_byte,
                end_byte: *end_byte,
            },
            SourceRef::DocumentPartXml {
                document_graph_id,
                part,
                content_hash,
                start_byte,
                end_byte,
            } => Self::DocumentPartXml {
                document_graph_id,
                part,
                content_hash,
                start_byte: *start_byte,
                end_byte: *end_byte,
            },
        }
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Item<'a> {
    source: Reference<'a>,
    text: &'a str,
    xml_origins: &'a [XmlOrigin],
    #[serde(skip_serializing_if = "Option::is_none")]
    container_hash: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    asset_id: Option<&'a str>,
    precision: &'static str,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PacketProof<'a> {
    selection_index: usize,
    index: usize,
    input_hash: &'a str,
    unit_set_hash: &'a str,
    contract_hash: &'a str,
    source_bindings: &'a [crate::meaning_graph::Binding],
    provenance: &'a crate::meaning_view::ReceiptProvenance,
    coverage: &'a [crate::meaning_graph::UnitCoverage],
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ScopeProof<'a> {
    workspace_id: &'a str,
    snapshot_id: &'a str,
    kind: &'a str,
    capture_consistency: &'a str,
    file_count: usize,
    source_bytes: usize,
    role: &'static str,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SourceProvenance<'a> {
    view_complete: bool,
    view_limitations: &'a [String],
    overlays: &'a [Value],
    meaning_scope: Option<&'a crate::meaning_view::MeaningScope>,
    meaning_selections: &'a [crate::meaning_view::MeaningSelectionRef],
    meaning_packets: Vec<PacketProof<'a>>,
    meaning_provider_attestation: &'static str,
    source_scopes: Vec<ScopeProof<'a>>,
}
fn source_provenance<'a>(view: &'a CheckedMixedView, d: &Deadline) -> Result<SourceProvenance<'a>> {
    let side = view.source.before();
    let original = side.view();
    let meaning = original.meaning.as_ref();
    let mut meaning_packets = Vec::new();
    let mut source_scopes = Vec::new();
    if let Some(meaning) = meaning {
        for packet in &meaning.packets {
            check(d)?;
            meaning_packets.push(PacketProof {
                selection_index: packet.selection_index,
                index: packet.index,
                input_hash: &packet.input_hash,
                unit_set_hash: &packet.unit_set_hash,
                contract_hash: &packet.contract_hash,
                source_bindings: &packet.source_bindings,
                provenance: &packet.provenance,
                coverage: &packet.coverage,
            });
        }
    }
    for scope in side.source_scopes() {
        check(d)?;
        let mut source_bytes = 0usize;
        for file in &scope.files {
            check(d)?;
            source_bytes = source_bytes
                .checked_add(file.size)
                .ok_or_else(|| failure("mixed_view_capacity_exceeded"))?;
        }
        source_scopes.push(ScopeProof {
            workspace_id: &scope.workspace_id,
            snapshot_id: &scope.id,
            kind: &scope.kind,
            capture_consistency: &scope.capture_consistency,
            file_count: scope.files.len(),
            source_bytes,
            role: if scope.id == side.primary_snapshot().id {
                "primary_search"
            } else {
                "receipt_replay_only"
            },
        });
    }
    check(d)?;
    Ok(SourceProvenance {
        view_complete: original.complete,
        view_limitations: &original.limitations,
        overlays: &original.overlays,
        meaning_scope: meaning.map(|m| &m.scope),
        meaning_selections: meaning.map_or(&[][..], |m| m.selections.as_slice()),
        meaning_packets,
        meaning_provider_attestation: "unverified",
        source_scopes,
    })
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Metadata<'a> {
    kind: &'static str,
    selection: &'a Selection,
    documents: Vec<documents::Header<'a>>,
    source_provenance: SourceProvenance<'a>,
    admitted_metadata_bytes: usize,
    admitted_source_bytes: usize,
    complete: bool,
    model_calls: u8,
    source_written: bool,
    limitations: [&'static str; 3],
}
fn metadata<'a>(view: &'a CheckedMixedView, d: &Deadline) -> Result<Metadata<'a>> {
    Ok(Metadata {
        kind: "mixed_view_query_v1",
        selection: &view.selection,
        documents: view.documents.iter().map(documents::header).collect(),
        source_provenance: source_provenance(view, d)?,
        admitted_metadata_bytes: view.metadata_bytes,
        admitted_source_bytes: view.source_bytes,
        complete: false,
        model_calls: 0,
        source_written: false,
        limitations: [
            "Native UTF-8, decoded document text and original XML coordinates remain separate; container bytes never become UTF-8 source.",
            "XML origins enclose decoded segments, not individual entity-decoded characters. Cached spreadsheet values may be stale.",
            "This explicit same-workspace query selection does not establish mixed semantic identity, processed PDF coverage or cross-source graph links.",
        ],
    })
}
#[derive(Serialize)]
struct Description<'a> {
    #[serde(flatten)]
    metadata: Metadata<'a>,
    operation: &'static str,
}
#[derive(Serialize)]
struct Read<'a> {
    #[serde(flatten)]
    metadata: Metadata<'a>,
    operation: &'static str,
    item: Item<'a>,
}
#[derive(Serialize)]
struct Found<'a> {
    #[serde(flatten)]
    metadata: Metadata<'a>,
    operation: &'static str,
    items: Vec<Item<'a>>,
    total: usize,
    offset: usize,
    limit: usize,
    truncated: bool,
}
fn collect<'a>(
    items: &mut Vec<Item<'a>>,
    total: &mut usize,
    item: Item<'a>,
    offset: usize,
    limit: usize,
    max: usize,
) -> Result<()> {
    *total = total
        .checked_add(1)
        .ok_or_else(|| failure("mixed_view_capacity_exceeded"))?;
    capacity(*total <= max)?;
    if *total > offset && items.len() < limit {
        items.push(item);
    }
    Ok(())
}
pub(super) fn run(
    view: &CheckedMixedView,
    q: &Query,
    max_output: usize,
    d: &Deadline,
) -> Result<Value> {
    validate_query(q, &view.selection, d)?;
    input(max_output <= MAX_OUTPUT_BYTES)?;
    let max_output = max_output.min(view.options.max_output_bytes);
    capacity(max_output > 0)?;
    let side = view.source.before();
    let snapshot = side.primary_snapshot();
    let scope = side.bind_scope(snapshot, d)?;
    match q {
        Query::Describe {} => output(
            &Description {
                metadata: metadata(view, d)?,
                operation: "describe",
            },
            max_output,
            d,
        ),
        Query::Read { source } => {
            let item = match source {
                SourceRef::SourceUtf8 {
                    path,
                    content_hash,
                    start_byte,
                    end_byte,
                    ..
                } => {
                    let file = snapshot
                        .files
                        .iter()
                        .find(|f| f.path == *path)
                        .ok_or_else(|| failure("mixed_view_input_invalid"))?;
                    pin(file.hash == *content_hash)?;
                    let source_text = scope.source(file, d)?.text();
                    input(
                        *end_byte <= source_text.len()
                            && source_text.is_char_boundary(*start_byte)
                            && source_text.is_char_boundary(*end_byte),
                    )?;
                    Item {
                        source: source.into(),
                        text: &source_text[*start_byte..*end_byte],
                        xml_origins: &[],
                        container_hash: None,
                        asset_id: None,
                        precision: "exact_native_utf8",
                    }
                }
                SourceRef::DocumentDecodedText {
                    document_graph_id,
                    segment_id,
                    content_hash,
                    start_byte,
                    end_byte,
                } => {
                    let graph = view
                        .documents
                        .iter()
                        .find(|g| documents::header(g).document_graph_id == document_graph_id)
                        .ok_or_else(|| failure("mixed_view_input_invalid"))?;
                    let header = documents::header(graph);
                    let (text, origins) = documents::read_decoded(
                        graph,
                        segment_id,
                        content_hash,
                        *start_byte,
                        *end_byte,
                    )?;
                    Item {
                        source: source.into(),
                        text,
                        xml_origins: origins,
                        container_hash: Some(header.container_hash),
                        asset_id: Some(header.asset_id),
                        precision: "exact_decoded_text_with_enclosing_xml_origins",
                    }
                }
                SourceRef::DocumentPartXml {
                    document_graph_id,
                    part,
                    content_hash,
                    start_byte,
                    end_byte,
                } => {
                    let graph = view
                        .documents
                        .iter()
                        .find(|g| documents::header(g).document_graph_id == document_graph_id)
                        .ok_or_else(|| failure("mixed_view_input_invalid"))?;
                    let header = documents::header(graph);
                    let text =
                        documents::read_xml(graph, part, content_hash, *start_byte, *end_byte)?;
                    Item {
                        source: source.into(),
                        text,
                        xml_origins: &[],
                        container_hash: Some(header.container_hash),
                        asset_id: Some(header.asset_id),
                        precision: "exact_part_xml_utf8",
                    }
                }
            };
            check(d)?;
            output(
                &Read {
                    metadata: metadata(view, d)?,
                    operation: "read",
                    item,
                },
                max_output,
                d,
            )
        }
        Query::Find {
            query,
            ignore_case,
            offset,
            limit,
        } => {
            let pattern = regex::RegexBuilder::new(&regex::escape(query))
                .case_insensitive(*ignore_case)
                .size_limit(1_000_000)
                .build()?;
            check(d)?;
            let mut items = Vec::new();
            let mut total = 0usize;
            let mut files: Vec<_> = snapshot.files.iter().collect();
            files.sort_by(|a, b| a.path.cmp(&b.path));
            for file in files {
                check(d)?;
                let text = scope.source(file, d)?.text();
                for found in pattern.find_iter(text) {
                    check(d)?;
                    collect(
                        &mut items,
                        &mut total,
                        Item {
                            source: Reference::SourceUtf8 {
                                view_id: &view.selection.source.view_id,
                                snapshot_id: &snapshot.id,
                                path: &file.path,
                                content_hash: &file.hash,
                                start_byte: found.start(),
                                end_byte: found.end(),
                            },
                            text: found.as_str(),
                            xml_origins: &[],
                            container_hash: None,
                            asset_id: None,
                            precision: "exact_native_utf8",
                        },
                        *offset,
                        *limit,
                        view.options.max_matches,
                    )?;
                }
            }
            for graph in &view.documents {
                check(d)?;
                let header = documents::header(graph);
                for segment in documents::segments(graph) {
                    check(d)?;
                    for found in pattern.find_iter(segment.text) {
                        check(d)?;
                        collect(
                            &mut items,
                            &mut total,
                            Item {
                                source: Reference::DocumentDecodedText {
                                    document_graph_id: header.document_graph_id,
                                    segment_id: segment.id,
                                    content_hash: header.decoded_text_hash,
                                    start_byte: segment.start + found.start(),
                                    end_byte: segment.start + found.end(),
                                },
                                text: found.as_str(),
                                xml_origins: segment.xml_origins,
                                container_hash: Some(header.container_hash),
                                asset_id: Some(header.asset_id),
                                precision: "exact_decoded_text_with_enclosing_xml_origins",
                            },
                            *offset,
                            *limit,
                            view.options.max_matches,
                        )?;
                    }
                }
            }
            check(d)?;
            output(
                &Found {
                    metadata: metadata(view, d)?,
                    operation: "find",
                    items,
                    total,
                    offset: *offset,
                    limit: *limit,
                    truncated: total > offset.saturating_add(*limit),
                },
                max_output,
                d,
            )
        }
    }
}
