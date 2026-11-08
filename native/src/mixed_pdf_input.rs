//! Explicit immutable PDF assets share the existing bounded native worker.
use super::*;
use crate::{media, model::hash};
use rusqlite::{OptionalExtension, limits::Limit};
use std::path::{Path, PathBuf};

pub const PDF_EXTRACTION_POLICY: &str =
    "yoke-native-pdf-text-1:lopdf-0.42.0:direct_page_content_streams";
const MAX_CONTAINER: usize = 10_000_000;
const TEXT_RESERVATION: usize = 2_000_000;
const WORKER_RESERVATION: usize = 8_000_000;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PdfPin {
    pub asset_id: String,
    pub container_hash: String,
    pub extraction_policy: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SelectionV2 {
    pub schema_version: u8,
    pub source: SourcePin,
    pub documents: Vec<DocumentPin>,
    pub pdfs: Vec<PdfPin>,
}
impl SelectionV2 {
    pub(super) fn native_selection(&self) -> Selection {
        Selection {
            schema_version: 1,
            source: self.source.clone(),
            documents: self.documents.clone(),
        }
    }
}

/// Host executable authority is never deserialized from a source selection.
/// The caller selects a trusted native helper; binary attestation is not implied.
pub struct NativePdfWorker {
    executable: PathBuf,
    observer: Option<std::sync::Arc<media::PdfSpawnObserver>>,
}
impl NativePdfWorker {
    pub fn new(executable: &Path, d: &Deadline) -> Result<Self> {
        check(d)?;
        input(
            executable.is_absolute()
                && !executable.components().any(|c| {
                    matches!(
                        c,
                        std::path::Component::ParentDir | std::path::Component::CurDir
                    )
                }),
        )?;
        let metadata = std::fs::symlink_metadata(executable)?;
        input(metadata.is_file() && !crate::paths::is_link(&metadata))?;
        check(d)?;
        Ok(Self {
            executable: executable.into(),
            observer: None,
        })
    }
    /// Observe the owned PID after cleanup is installed. A host callback may
    /// deny or cancel; it cannot bypass source, protocol or resource checks.
    pub fn with_spawn_observer<F>(mut self, observer: F) -> Self
    where
        F: Fn(u32, &Deadline) -> Result<()> + Send + Sync + 'static,
    {
        self.observer = Some(std::sync::Arc::new(observer));
        self
    }
}

pub fn validate_pdf_selection(s: &SelectionV2, o: &Options, d: &Deadline) -> Result<()> {
    validate_options(o)?;
    check(d)?;
    input(s.schema_version == 2 && !s.pdfs.is_empty())?;
    capacity(s.documents.len().saturating_add(s.pdfs.len()) <= o.max_documents)?;
    validate_source_pin(&s.source)?;
    validate_document_pins(&s.documents, d)?;
    let mut seen = BTreeSet::new();
    for p in &s.pdfs {
        check(d)?;
        input(
            valid_id(&p.asset_id)
                && valid_id(&p.container_hash)
                && p.extraction_policy == PDF_EXTRACTION_POLICY
                && seen.insert(&p.asset_id),
        )?;
    }
    encode(s, MAX_SELECTION_BYTES, d)?;
    check(d)
}

struct LengthLimit<'a> {
    db: &'a rusqlite::Connection,
    original: i32,
}
impl Drop for LengthLimit<'_> {
    fn drop(&mut self) {
        let _ = self.db.set_limit(Limit::SQLITE_LIMIT_LENGTH, self.original);
    }
}
pub(super) struct PreparedPdfs {
    assets: Vec<media::Asset>,
    pub(super) metadata_bytes: usize,
    pub(super) source_bytes: usize,
}
impl PreparedPdfs {
    fn row(
        &mut self,
        store: &Store,
        kind: &str,
        id: &str,
        remaining: usize,
        d: &Deadline,
    ) -> Result<String> {
        check(d)?;
        let size: Option<usize> = store
            .db
            .query_row(
                "SELECT length(CAST(value AS BLOB)) FROM records WHERE kind=?1 AND id=?2",
                rusqlite::params![kind, id],
                |r| r.get(0),
            )
            .optional()?;
        let size = size.ok_or_else(|| failure("mixed_view_parent_refused"))?;
        capacity(size <= 64000)?;
        self.metadata_bytes = self
            .metadata_bytes
            .checked_add(size)
            .ok_or_else(|| failure("mixed_view_capacity_exceeded"))?;
        capacity(self.metadata_bytes <= remaining)?;
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
            |r| r.get(0),
        )?;
        parent(raw.len() == size)?;
        check(d)?;
        Ok(raw)
    }
    pub(super) fn load(
        self,
        store: &Store,
        worker: &NativePdfWorker,
        d: &Deadline,
    ) -> Result<Vec<CheckedPdf>> {
        let mut result = Vec::new();
        for asset in self.assets {
            check(d)?;
            let bytes = store.read_blob_exact(&asset.blob_id, asset.size, MAX_CONTAINER)?;
            pin(hash(&bytes) == asset.source_hash)?;
            let parsed = media::frozen_pdf_with_worker(
                &bytes,
                &worker.executable,
                worker.observer.as_deref(),
                d,
            )?;
            let mut pages = Vec::new();
            for segment in parsed.segments {
                check(d)?;
                pages.push(PdfPage {
                    number: segment.page,
                    content_hash: hash(segment.text.as_bytes()),
                    text: segment.text,
                });
            }
            result.push(CheckedPdf {
                asset,
                pages,
                metadata: serde_json::to_value(parsed.metadata)?,
                warnings: parsed.warnings,
                resource_limits: serde_json::to_value(parsed.resource_limits)?,
            });
        }
        check(d)?;
        Ok(result)
    }
}

pub(super) fn prepare(
    store: &Store,
    pins: &[PdfPin],
    remaining_metadata: usize,
    remaining_source: usize,
    d: &Deadline,
) -> Result<PreparedPdfs> {
    let mut prepared = PreparedPdfs {
        assets: Vec::new(),
        metadata_bytes: 0,
        source_bytes: 0,
    };
    for p in pins {
        let raw = prepared.row(
            store,
            "native_media_asset_v1",
            &p.asset_id,
            remaining_metadata,
            d,
        )?;
        let asset: media::Asset = crate::json::decode(crate::json::parse(raw.as_bytes(), 64000)?)?;
        media::validate_asset(&asset)?;
        pin(asset.id == p.asset_id
            && asset.workspace_id == store.workspace_id
            && asset.source_hash == p.container_hash)?;
        input(
            Path::new(&asset.path)
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("pdf")),
        )?;
        capacity(asset.size <= MAX_CONTAINER)?;
        let raw = prepared.row(store, "artifact", &asset.blob_id, remaining_metadata, d)?;
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Registration {
            artifact_id: String,
            total_bytes: usize,
        }
        let registration: Registration =
            crate::json::decode(crate::json::parse(raw.as_bytes(), 64000)?)?;
        parent(
            registration.artifact_id == asset.blob_id && registration.total_bytes == asset.size,
        )?;
        prepared.metadata_bytes = prepared
            .metadata_bytes
            .checked_add(WORKER_RESERVATION)
            .ok_or_else(|| failure("mixed_view_capacity_exceeded"))?;
        prepared.source_bytes = prepared
            .source_bytes
            .checked_add(asset.size)
            .and_then(|n| n.checked_add(TEXT_RESERVATION))
            .ok_or_else(|| failure("mixed_view_capacity_exceeded"))?;
        capacity(
            prepared.metadata_bytes <= remaining_metadata
                && prepared.source_bytes <= remaining_source,
        )?;
        prepared.assets.push(asset);
    }
    check(d)?;
    Ok(prepared)
}
pub(super) struct PdfPage {
    pub(super) number: usize,
    pub(super) text: String,
    pub(super) content_hash: String,
}
pub(super) struct CheckedPdf {
    pub(super) asset: media::Asset,
    pub(super) pages: Vec<PdfPage>,
    pub(super) metadata: Value,
    pub(super) warnings: Vec<String>,
    pub(super) resource_limits: Value,
}
