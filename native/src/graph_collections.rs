//! Host-registered immutable graph collection views.
use crate::{
    capture::Deadline,
    federation_archive::{FederationArchiveAccess, FederationArchiveBinding},
    model::{Graph, Snapshot, hash, identity, valid_id},
    paths::is_link,
    store::Store,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::Path;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    time::Duration,
};
pub struct Registry {
    db: rusqlite::Connection,
    read_only: bool,
    _read_guard: Option<fs::File>,
}
#[cfg(test)]
#[path = "graph_collection_read_tests.rs"]
mod reader_tests;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Registration {
    name: String,
    snapshot: Snapshot,
    graph: Graph,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    archive: Option<crate::graph_view::ArchiveProof>,
    #[serde(
        rename = "captureScope",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    capture_scope: Option<Value>,
    #[serde(
        rename = "sourceAuthority",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    source_authority: Option<SourceAuthority>,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum SourceAuthority {
    FederatedCanonical { binding: FederationArchiveBinding },
    FederatedSources { binding: FederationArchiveBinding },
}
impl SourceAuthority {
    fn binding(&self) -> &FederationArchiveBinding {
        match self {
            Self::FederatedCanonical { binding } | Self::FederatedSources { binding } => binding,
        }
    }
    fn kind(&self) -> &'static str {
        match self {
            Self::FederatedCanonical { .. } => "federatedCanonical",
            Self::FederatedSources { .. } => "federatedSources",
        }
    }
}
pub fn validate_registration_name(name: &str) -> Result<()> {
    ensure!(
        !name.is_empty()
            && name.len() <= 100
            && name
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '-'),
        "Invalid host registration name"
    );
    Ok(())
}
fn replay_provenance(epoch: Option<crate::analysis::AnalyzerEpoch>) -> Value {
    match epoch {
        Some(epoch) => json!({"status":"source_rederived","analyzer":epoch.producer()}),
        None => json!({"status":"historical_host_registered_unreplayed","analyzer":null}),
    }
}
fn name_ok(name: &str) -> Result<()> {
    validate_registration_name(name)
}
impl Registry {
    pub fn register_meaning_archive_cas(
        &self,
        name: &str,
        archive: &crate::graph_view_archive::CheckedMeaningArchive,
        expected: Option<&str>,
        deadline: &Deadline,
    ) -> Result<String> {
        self.writable()?;
        name_ok(name)?;
        if let Some(id) = expected {
            ensure!(valid_id(id), "Invalid expected registration ID");
        }
        self.budget(deadline)?;
        let data = Registration {
            name: name.into(),
            snapshot: archive.snapshot().clone(),
            graph: archive.source_projection().clone(),
            archive: Some(archive.proof().clone()),
            capture_scope: None,
            source_authority: None,
        };
        // The private checked capability already contains every byte. No source
        // lookup, provider or proof replay runs inside this publication transaction.
        let payload = String::from_utf8(crate::json::encode_bounded(&data, 32_000_000, deadline)?)?;
        let id = hash(payload.as_bytes());
        let tx = rusqlite::Transaction::new_unchecked(
            &self.db,
            rusqlite::TransactionBehavior::Immediate,
        )?;
        let count: i64 =
            tx.query_row("SELECT COUNT(*) FROM registrations", [], |row| row.get(0))?;
        ensure!(
            count < 100
                || tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM registrations WHERE name=?1)",
                    [name],
                    |row| row.get::<_, bool>(0)
                )?,
            "Collection registration capacity exceeded"
        );
        let changed = match expected {
            None => tx.execute(
                "INSERT INTO registrations(name,id,payload) VALUES(?1,?2,?3) ON CONFLICT(name) DO NOTHING",
                rusqlite::params![name,id,payload])?,
            Some(prior) => tx.execute(
                "UPDATE registrations SET id=?2,payload=?3 WHERE name=?1 AND id=?4",
                rusqlite::params![name,id,payload,prior])?,
        };
        ensure!(
            changed == 1,
            "Collection registration missing, already present, or stale"
        );
        for file in archive.source_files() {
            deadline.check()?;
            let text = archive
                .source_objects()
                .get(&file.hash)
                .context("Checked typed source object absent")?;
            tx.execute(
                "INSERT INTO source_blobs(hash,bytes) VALUES(?1,?2) ON CONFLICT(hash) DO NOTHING",
                rusqlite::params![file.hash, text.as_bytes()],
            )?;
            ensure!(
                self.source_bytes(file, deadline)?.as_slice() == text.as_bytes(),
                "Collection immutable source differs from explicit typed archive input"
            );
        }
        deadline.check()?;
        tx.commit()?;
        Ok(id)
    }
    pub fn load_meaning_archive(
        &self,
        name: &str,
        limits: crate::graph_view_archive::Limits,
        deadline: &Deadline,
    ) -> Result<crate::graph_view_archive::CheckedMeaningArchive> {
        limits.check(deadline)?;
        name_ok(name)?;
        self.budget(deadline)?;
        let (id, payload) = self.stored_registration(name, limits.max_wire_bytes)?;
        let raw = crate::json::parse(payload.as_bytes(), limits.max_wire_bytes)?;
        let data: Registration = crate::json::decode(raw.clone())?;
        ensure!(
            data.name == name
                && valid_id(&id)
                && identity(&data) == id
                && hash(payload.as_bytes()) == id
                && serde_json::to_value(&data)? == raw,
            "Typed collection registration integrity mismatch"
        );
        self.load_meaning_data(&data, limits, deadline)
    }
    fn load_meaning_data(
        &self,
        data: &Registration,
        limits: crate::graph_view_archive::Limits,
        deadline: &Deadline,
    ) -> Result<crate::graph_view_archive::CheckedMeaningArchive> {
        ensure!(
            data.source_authority.is_none()
                && data.capture_scope.is_none()
                && data.graph.snapshot_id == data.snapshot.id,
            "Foreign authority cannot be combined with a native typed archive"
        );
        let proof = data
            .archive
            .as_ref()
            .context("Typed collection archive proof missing")?;
        ensure!(
            proof.view.meaning.is_some(),
            "Collection has no typed Meaning View"
        );
        let checked = crate::graph_view_archive::reassemble(
            &data.snapshot,
            proof,
            &mut |member, d| self.source_bytes(member.file(), d),
            limits,
            deadline,
        )?;
        ensure!(
            identity(checked.source_projection()) == identity(&data.graph),
            "Typed collection source projection differs from full replay"
        );
        Ok(checked)
    }
    pub fn read_source(
        &self,
        name: &str,
        path: &str,
        range: std::ops::Range<usize>,
        deadline: &Deadline,
    ) -> Result<Value> {
        self.read_source_inner(name, path, range, None, deadline)
    }
    /// Select a registry reader using invocation-only host permission. The
    /// default preserves the Windows file-sharing guard and its zero-sidecar
    /// contract. Coordination permission is independent of registration rights.
    pub fn open_existing_for_host(
        host_directory: &Path,
        allow_registry_coordination: bool,
        deadline: &Deadline,
    ) -> Result<Self> {
        deadline.check()?;
        if allow_registry_coordination {
            Self::open_existing_coordinated(host_directory, deadline)
        } else {
            Self::open_existing(host_directory)
        }
    }
    /// Explicitly permit SQLite WAL/SHM coordination in a trusted local host
    /// directory. No database creation or logical writes are allowed. Unlike
    /// immutable mode, this reads committed WAL data and pins one read
    /// transaction until this Registry is dropped on Unix. Other SQLite writers
    /// may continue; direct filesystem writers are not excluded by this mode.
    /// Windows retains its stronger file-sharing guard even with permission.
    pub fn open_existing_coordinated(host_directory: &Path, deadline: &Deadline) -> Result<Self> {
        deadline.check()?;
        #[cfg(windows)]
        {
            Self::open_existing(host_directory)
        }
        #[cfg(not(windows))]
        {
            Self::open_coordinated_snapshot(host_directory, deadline)
        }
    }
    #[cfg(any(not(windows), test))]
    fn open_coordinated_snapshot(host_directory: &Path, deadline: &Deadline) -> Result<Self> {
        deadline.check()?;
        Self::check_paths(host_directory)?;
        let path = host_directory.join("yoke-collection.sqlite");
        let meta = fs::symlink_metadata(&path)?;
        ensure!(
            meta.is_file() && !is_link(&meta),
            "Registry database link refused"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            ensure!(
                meta.nlink() == 1,
                "Registry database hardlink alias refused"
            );
            for suffix in ["-wal", "-shm", "-journal"] {
                let sidecar = host_directory.join(format!("yoke-collection.sqlite{suffix}"));
                match fs::symlink_metadata(sidecar) {
                    Ok(meta) => {
                        ensure!(meta.nlink() == 1, "Registry sidecar hardlink alias refused")
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                    Err(error) => return Err(error.into()),
                }
            }
        }
        deadline.check()?;
        let db = rusqlite::Connection::open_with_flags(
            &path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
                | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX
                | rusqlite::OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )?;
        // Bound untrusted schema/row materialization before the first schema
        // read. Sources remain lazy and retain their narrower admission limits.
        use rusqlite::limits::Limit;
        for (limit, bound) in [
            (Limit::SQLITE_LIMIT_LENGTH, 64_004_096),
            (Limit::SQLITE_LIMIT_SQL_LENGTH, 65_536),
            (Limit::SQLITE_LIMIT_COLUMN, 64),
            (Limit::SQLITE_LIMIT_EXPR_DEPTH, 64),
        ] {
            db.set_limit(limit, db.limit(limit)?.min(bound))?;
        }
        let registry = Self {
            db,
            read_only: true,
            _read_guard: None,
        };
        registry.budget(deadline)?;
        registry.db.execute_batch(
            "PRAGMA query_only=ON; PRAGMA trusted_schema=OFF; PRAGMA temp_store=MEMORY; BEGIN",
        )?;
        // BEGIN alone is deferred. Reading the schema here pins the snapshot
        // before any subsequent multi-step registration/proof/source reads.
        registry.check_schema()?;
        deadline.check()?;
        Ok(registry)
    }
    fn check_schema(&self) -> Result<()> {
        for table in ["registrations", "source_blobs"] {
            let sql: String = self.db.query_row(
                "SELECT sql FROM sqlite_master WHERE type='table' AND name=?1",
                [table],
                |r| r.get(0),
            )?;
            ensure!(
                sql.starts_with(&format!("CREATE TABLE {table}(")),
                "Unknown registry table schema"
            );
        }
        Ok(())
    }
    #[cfg(not(windows))]
    pub fn open_existing(host_directory: &Path) -> Result<Self> {
        Self::check_paths(host_directory)?;
        let meta = fs::symlink_metadata(host_directory.join("yoke-collection.sqlite"))?;
        ensure!(
            meta.is_file() && !is_link(&meta),
            "Registry database link refused"
        );
        anyhow::bail!("Read-only registry file-sharing guard is currently Windows-only")
    }
    #[cfg(windows)]
    pub fn open_existing(host_directory: &Path) -> Result<Self> {
        Self::check_paths(host_directory)?;
        let path = host_directory.join("yoke-collection.sqlite");
        let meta = fs::symlink_metadata(&path)?;
        ensure!(
            meta.is_file() && !is_link(&meta),
            "Registry database link refused"
        );
        // On Windows a held read-only sharing handle excludes both existing and
        // future database writers. Immutable mode then avoids SQLite creating
        // sidecars; a nonempty journal/WAL must be recovered by a writer first.
        #[cfg(windows)]
        let guard = {
            use std::os::windows::fs::OpenOptionsExt;
            fs::OpenOptions::new()
                .read(true)
                .share_mode(1)
                .open(&path)
                .context("Read-only registry requires no active database writer")?
        };
        for suffix in ["-wal", "-journal"] {
            let file = host_directory.join(format!("yoke-collection.sqlite{suffix}"));
            match fs::metadata(file) {
                Ok(meta) => ensure!(
                    meta.len() == 0,
                    "Read-only registry requires checkpointed journals"
                ),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                Err(e) => return Err(e.into()),
            }
        }
        let text = path
            .to_str()
            .context("Registry path must be Unicode")?
            .replace('\\', "/");
        // canonicalize() uses the Windows verbatim prefix. It is a native path
        // marker, not a URI authority (in particular, do not encode '?' there).
        let text = if let Some(unc) = text.strip_prefix("//?/UNC/") {
            format!("//{unc}")
        } else {
            text.strip_prefix("//?/").unwrap_or(&text).to_owned()
        };
        let mut uri = String::from(if text.starts_with("//") {
            "file://"
        } else {
            "file:"
        });
        for byte in text.bytes() {
            if byte.is_ascii_alphanumeric() || b"/:._-".contains(&byte) {
                uri.push(char::from(byte));
            } else {
                use std::fmt::Write;
                write!(uri, "%{byte:02X}")?;
            }
        }
        uri.push_str("?immutable=1&mode=ro");
        let db = rusqlite::Connection::open_with_flags(
            uri,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
                | rusqlite::OpenFlags::SQLITE_OPEN_URI
                | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        db.execute_batch(
            "PRAGMA query_only=ON; PRAGMA trusted_schema=OFF; PRAGMA temp_store=MEMORY",
        )?;
        let registry = Self {
            db,
            read_only: true,
            _read_guard: Some(guard),
        };
        registry.check_schema()?;
        Ok(registry)
    }
    pub fn register_cas(
        &self,
        name: &str,
        store: &Store,
        snapshot: &Snapshot,
        graph: &Graph,
        expected: Option<&str>,
        deadline: &Deadline,
    ) -> Result<String> {
        self.register_inner(name, store, snapshot, graph, Some(expected), deadline)
    }
    pub fn list(&self, deadline: &Deadline) -> Result<Value> {
        self.list_inner(None, deadline)
    }
    pub fn list_authorized(
        &self,
        access: &FederationArchiveAccess<'_>,
        deadline: &Deadline,
    ) -> Result<Value> {
        self.list_inner(Some(access), deadline)
    }
    fn list_inner(
        &self,
        access: Option<&FederationArchiveAccess<'_>>,
        deadline: &Deadline,
    ) -> Result<Value> {
        self.budget(deadline)?;
        let count: usize = self
            .db
            .query_row("SELECT COUNT(*) FROM registrations", [], |r| r.get(0))?;
        ensure!(count <= 100, "Collection registration capacity exceeded");
        // Bound SQLite cells before selecting names, including corrupt rows.
        let cap = rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH;
        let old = self
            .db
            .set_limit(cap, self.db.limit(cap)?.min(32_004_096))?;
        let names = (|| -> Result<Vec<String>> {
            let mut statement = self.db.prepare("SELECT CASE WHEN typeof(name)='text' AND length(CAST(name AS BLOB)) BETWEEN 1 AND 100 THEN name ELSE NULL END FROM registrations ORDER BY name LIMIT 101")?;
            let rows = statement.query_map([], |r| r.get(0))?;
            Ok(rows.collect::<std::result::Result<Vec<String>, _>>()?)
        })();
        self.db.set_limit(cap, old)?;
        let mut items = Vec::new();
        let mut remaining = 64_000_000usize;
        for name in names? {
            name_ok(&name)?;
            let (id, payload) = self.stored_registration(&name, remaining)?;
            remaining = remaining
                .checked_sub(payload.len())
                .context("Collection metadata input capacity exceeded")?;
            items.push(self.metadata(&name, &id, &payload, access, deadline)?);
        }
        deadline.check()?;
        Ok(json!({"count":items.len(),"items":items,"executionAuthority":"none"}))
    }
    pub fn describe(&self, name: &str, deadline: &Deadline) -> Result<Value> {
        self.describe_inner(name, None, deadline)
    }
    pub fn describe_authorized(
        &self,
        name: &str,
        access: &FederationArchiveAccess<'_>,
        deadline: &Deadline,
    ) -> Result<Value> {
        self.describe_inner(name, Some(access), deadline)
    }
    fn describe_inner(
        &self,
        name: &str,
        access: Option<&FederationArchiveAccess<'_>>,
        deadline: &Deadline,
    ) -> Result<Value> {
        name_ok(name)?;
        self.budget(deadline)?;
        let (id, payload) = self.stored_registration(name, 32_000_000)?;
        self.metadata(name, &id, &payload, access, deadline)
    }
    fn metadata(
        &self,
        name: &str,
        id: &str,
        payload: &str,
        access: Option<&FederationArchiveAccess<'_>>,
        deadline: &Deadline,
    ) -> Result<Value> {
        let raw = crate::json::parse(payload.as_bytes(), 32_000_000)?;
        let data: Registration = crate::json::decode(raw.clone())?;
        ensure!(
            data.name == name
                && valid_id(id)
                && identity(&data) == id
                && data.snapshot.id == data.graph.snapshot_id,
            "Collection registration integrity mismatch"
        );
        if data
            .archive
            .as_ref()
            .is_some_and(|proof| proof.view.meaning.is_some())
        {
            ensure!(
                serde_json::to_value(&data)? == raw && hash(payload.as_bytes()) == id,
                "Lossy typed collection registration schema"
            );
            let checked = self.load_meaning_data(&data, Default::default(), deadline)?;
            let layer = checked
                .view()
                .meaning
                .as_ref()
                .context("Typed archive layer absent")?;
            let concepts: usize = layer
                .packets
                .iter()
                .map(|packet| packet.concepts.len())
                .sum();
            let relations: usize = layer
                .packets
                .iter()
                .map(|packet| packet.relations.len())
                .sum();
            let hyperedges: usize = layer
                .packets
                .iter()
                .map(|packet| packet.hyperedges.len())
                .sum();
            let evidence: usize = layer
                .packets
                .iter()
                .map(|packet| packet.evidence.len())
                .sum();
            let citations: usize = layer
                .packets
                .iter()
                .map(|packet| packet.citation_links.len())
                .sum();
            deadline.check()?;
            return Ok(
                json!({"name":name,"registrationId":id,"kind":"typed_meaning_view",
                "snapshotId":data.snapshot.id,"workspaceId":data.snapshot.workspace_id,
                "graphHash":identity(&data.graph),"fileCount":data.snapshot.files.len(),
                "sourceBytes":checked.source_bytes(),"archivedScopeCount":checked.source_scope_count(),
                "archivedFileEntries":checked.source_file_entries(),
                "nodeCount":data.graph.entities.len(),"observationCount":data.graph.observations.len(),
                "viewId":checked.view().id,"archiveId":checked.archive_id(),
                "selectedOverlayCount":checked.view().overlays.len(),"executionAuthority":"none",
                "canonicalReplay":replay_provenance(Some(checked.analyzer_epoch())),
                "meaning":{"selectionCount":layer.selections.len(),"packetCount":layer.packets.len(),
                    "conceptCount":concepts,"relationCount":relations,"hyperedgeCount":hyperedges,
                    "evidenceCount":evidence,"citationCount":citations,"providerAttestation":"unverified",
                    "unknownCostPacketCount":layer.packets.iter().filter(|packet|packet.provenance.provider_reported_cost_micros.is_none()).count(),
                    "unknownUsagePacketCount":layer.packets.iter().filter(|packet|packet.provenance.provider_reported_usage.is_none()).count()}}),
            );
        }
        let replay = self.verify_archive(&data, access, deadline)?;
        let kind = data
            .source_authority
            .as_ref()
            .map(SourceAuthority::kind)
            .unwrap_or(if data.archive.is_some() {
                "view"
            } else {
                "native"
            });
        let bytes = data.snapshot.files.iter().try_fold(0usize, |total, file| {
            total
                .checked_add(file.size)
                .context("Collection source capacity overflow")
        })?;
        ensure!(
            bytes <= 64_000_000,
            "Collection archive source capacity exceeded"
        );
        // Verify the immutable bytes without publishing excerpts or signatures.
        for file in &data.snapshot.files {
            self.source_bytes(file, deadline)?;
        }
        let mut result = json!({"name":name,"registrationId":id,"kind":kind,"snapshotId":data.snapshot.id,"workspaceId":data.snapshot.workspace_id,"graphHash":identity(&data.graph),"fileCount":data.snapshot.files.len(),"sourceBytes":bytes,"nodeCount":data.graph.entities.len(),"observationCount":data.graph.observations.len(),"executionAuthority":"none"});
        if let Some(proof) = data.archive {
            result["viewId"] = json!(proof.view.id);
            result["selectedOverlayCount"] = json!(proof.view.overlays.len());
        }
        if data.source_authority.is_some() {
            result["authorization"] = json!("current_host_context_rechecked");
        } else {
            result["canonicalReplay"] = replay_provenance(replay);
        }
        deadline.check()?;
        Ok(result)
    }
    fn source_bytes(
        &self,
        file: &crate::model::SourceFile,
        deadline: &Deadline,
    ) -> Result<Vec<u8>> {
        deadline.check()?;
        ensure!(
            valid_id(&file.hash) && file.size <= 4_000_000,
            "Collection source identity/size invalid"
        );
        let limit = rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH;
        let cap = self.db.limit(limit)?.min(4_004_096);
        let prior = self.db.set_limit(limit, cap)?;
        let result = (|| -> Result<Vec<u8>> {
            let (length, kind): (usize, String) = self.db.query_row(
                "SELECT length(bytes),typeof(bytes) FROM source_blobs WHERE hash=?1",
                [&file.hash],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            ensure!(
                length == file.size && kind == "blob",
                "Collection source size or type mismatch"
            );
            let bytes: Vec<u8> = self.db.query_row(
                "SELECT bytes FROM source_blobs WHERE hash=?1",
                [&file.hash],
                |row| row.get(0),
            )?;
            ensure!(
                bytes.len() == file.size && hash(&bytes) == file.hash,
                "Collection source hash mismatch"
            );
            Ok(bytes)
        })();
        self.db.set_limit(limit, prior)?;
        result
    }
    fn verify_archive(
        &self,
        data: &Registration,
        access: Option<&FederationArchiveAccess<'_>>,
        deadline: &Deadline,
    ) -> Result<Option<crate::analysis::AnalyzerEpoch>> {
        if data.archive.is_none() {
            for node in &data.graph.entities {
                if node.kind == "file" {
                    ensure!(
                        node.signature == format!("{} [{}]", node.path, node.language),
                        "Legacy presentation repair required: register a newly checked graph view"
                    );
                }
            }
        }
        if let Some(authority) = &data.source_authority {
            ensure!(
                data.archive.is_none() && data.capture_scope.is_none(),
                "Foreign authority cannot be combined with native archive proof"
            );
            let access =
                access.context("Foreign collection requires current host federation authority")?;
            if matches!(authority, SourceAuthority::FederatedSources { .. }) {
                ensure!(
                    identity(&data.graph)
                        == identity(&Graph {
                            snapshot_id: data.snapshot.id.clone(),
                            ..Default::default()
                        }),
                    "Source-only archive cannot carry graph facts"
                );
                access.authorize_sources(
                    authority.binding(),
                    &data.snapshot,
                    &mut |file, deadline| self.source_bytes(file, deadline),
                    deadline,
                )?;
                return Ok(None);
            }
            access.authorize(
                authority.binding(),
                &data.snapshot,
                &data.graph,
                &mut |file, deadline| self.source_bytes(file, deadline),
                deadline,
            )?;
            return Ok(None);
        }
        ensure!(
            data.archive.is_none() || data.capture_scope.is_none(),
            "Duplicate archived capture scope proofs"
        );
        let mut texts = BTreeMap::new();
        let mut total = 0usize;
        for file in &data.snapshot.files {
            deadline.check()?;
            crate::paths::normalized_syntax(&file.path)?;
            total = total
                .checked_add(file.size)
                .context("Collection archive source byte overflow")?;
            ensure!(
                total <= 64_000_000,
                "Collection archive source capacity exceeded"
            );
            texts.insert(
                file.path.clone(),
                String::from_utf8(self.source_bytes(file, deadline)?)?,
            );
        }
        let Some(proof) = &data.archive else {
            return crate::graph_view::validate_archived_canonical_graph(
                &data.snapshot,
                &data.graph,
                data.capture_scope.as_ref().unwrap_or(&Value::Null),
                &texts,
                deadline,
            );
        };
        let (projected, epoch) =
            crate::graph_view::validate_archive(&data.snapshot, proof, &texts, deadline)?;
        ensure!(
            identity(&projected) == identity(&data.graph),
            "Collection archived projection differs from verified view"
        );
        Ok(epoch)
    }
    pub fn register_view(
        &self,
        name: &str,
        store: &Store,
        view: &crate::graph_view::View,
        deadline: &Deadline,
    ) -> Result<String> {
        self.register_view_inner(name, store, view, None, deadline)
    }
    pub fn register_view_cas(
        &self,
        name: &str,
        store: &Store,
        view: &crate::graph_view::View,
        expected: Option<&str>,
        deadline: &Deadline,
    ) -> Result<String> {
        self.register_view_inner(name, store, view, Some(expected), deadline)
    }
    fn register_view_inner(
        &self,
        name: &str,
        store: &Store,
        view: &crate::graph_view::View,
        expected: Option<Option<&str>>,
        deadline: &Deadline,
    ) -> Result<String> {
        self.writable()?;
        if let Some(Some(id)) = expected {
            ensure!(valid_id(id), "Invalid expected registration ID");
        }
        name_ok(name)?;
        self.budget(deadline)?;
        let (snapshot, graph, archive) = crate::graph_view::archive(store, view, deadline)?;
        self.persist_registration(
            Registration {
                name: name.into(),
                snapshot,
                graph,
                archive: Some(archive),
                capture_scope: None,
                source_authority: None,
            },
            store,
            expected,
            deadline,
        )
    }
    fn stored_registration(&self, name: &str, remaining: usize) -> Result<(String, String)> {
        let limit = remaining.min(32_000_000);
        let sql_limit = rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH;
        let cap = self
            .db
            .limit(sql_limit)?
            .min(i32::try_from(limit.saturating_add(4096))?);
        let prior = self.db.set_limit(sql_limit, cap)?;
        let result = (|| -> Result<(String, String)> {
            let (bytes,id_bytes):(i64,i64)=self.db.query_row("SELECT length(CAST(payload AS BLOB)),length(CAST(id AS BLOB)) FROM registrations WHERE name=?1",[name],|row|Ok((row.get(0)?,row.get(1)?)))?;
            ensure!(
                bytes >= 0 && bytes as u64 <= limit as u64 && id_bytes == 64,
                "Collection registration payload exceeds capacity or has an invalid ID"
            );
            let registration:(String,String)=self.db.query_row("SELECT id,payload FROM registrations WHERE name=?1 AND length(CAST(payload AS BLOB))<=?2 AND length(CAST(id AS BLOB))=64",rusqlite::params![name,limit],|row|Ok((row.get(0)?,row.get(1)?)))?;
            ensure!(
                registration.1.len() <= limit,
                "Collection registration payload exceeds capacity"
            );
            Ok(registration)
        })();
        self.db.set_limit(sql_limit, prior)?;
        result.context("Collection registration payload exceeds capacity or cannot be read")
    }
    /// Routing hint only: never authenticates a registration or grants source
    /// access. The subsequent operation still verifies the complete archive.
    pub(crate) fn needs_federation(&self, names: &[String], deadline: &Deadline) -> Result<bool> {
        ensure!(names.len() <= 100, "Collection selection capacity exceeded");
        self.budget(deadline)?;
        for name in names {
            name_ok(name)?;
            deadline.check()?;
            let foreign: bool = self.db.query_row(
                "SELECT EXISTS(SELECT 1 FROM registrations WHERE name=?1 AND CASE WHEN length(CAST(payload AS BLOB)) <= 32000000 THEN coalesce(json_type(payload,'$.sourceAuthority'),'null') != 'null' ELSE 1 END)",
                [name], |row| row.get(0),
            )?;
            if foreign {
                return Ok(true);
            }
        }
        Ok(false)
    }
    fn read_view(
        &self,
        view: &Value,
        args: &Value,
        access: Option<&FederationArchiveAccess<'_>>,
        deadline: &Deadline,
    ) -> Result<Value> {
        let ids = args["entityIds"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("Collection read entity required"))?;
        ensure!(
            ids.len() == 1 && ids[0].as_str().is_some_and(valid_id),
            "Collection read requires one namespaced entity"
        );
        let node = view["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["id"] == ids[0])
            .ok_or_else(|| anyhow::anyhow!("Unknown collection read entity"))?;
        let name = node["registration"].as_str().unwrap();
        let (id, payload) = self.stored_registration(name, 32_000_000)?;
        let expected = view["lineage"]
            .as_array()
            .unwrap()
            .iter()
            .find(|scope| scope["registration"] == name)
            .unwrap();
        let data: Registration = serde_json::from_str(&payload)?;
        ensure!(
            identity(&data) == id && expected["registrationId"] == id,
            "Collection registration changed during read"
        );
        self.verify_archive(&data, access, deadline)?;
        let file = data
            .snapshot
            .files
            .iter()
            .find(|f| f.path == node["path"])
            .ok_or_else(|| anyhow::anyhow!("Collection source manifest missing"))?;
        let bytes = self.source_bytes(file, deadline)?;
        let text = String::from_utf8(bytes)?;
        let start = args
            .get("startByte")
            .map(|v| {
                v.as_u64()
                    .ok_or_else(|| anyhow::anyhow!("Invalid read start"))
            })
            .transpose()?
            .unwrap_or(node["startByte"].as_u64().unwrap()) as usize;
        let end = args
            .get("endByte")
            .map(|v| {
                v.as_u64()
                    .ok_or_else(|| anyhow::anyhow!("Invalid read end"))
            })
            .transpose()?
            .unwrap_or(node["endByte"].as_u64().unwrap()) as usize;
        crate::paths::utf8_range(&text, start, end)?;
        ensure!(
            end - start <= 256000,
            "Collection read output capacity exceeded"
        );
        deadline.check()?;
        Ok(
            json!({"collectionId":view["collectionId"],"entityId":ids[0],"registration":name,"workspaceId":data.snapshot.workspace_id,"snapshotId":data.snapshot.id,"path":file.path,"contentHash":file.hash,"startByte":start,"endByte":end,"startLine":text[..start].bytes().filter(|b|*b==b'\n').count()+1,"endLine":text[..end].bytes().filter(|b|*b==b'\n').count()+1,"text":text[start..end],"anchor":"exact","source":"collection_immutable_archive"}),
        )
    }
    pub fn query(&self, names: &[String], args: &Value, deadline: &Deadline) -> Result<Value> {
        self.query_inner(names, args, None, deadline)
    }
    pub fn query_authorized(
        &self,
        names: &[String],
        args: &Value,
        access: &FederationArchiveAccess<'_>,
        deadline: &Deadline,
    ) -> Result<Value> {
        self.query_inner(names, args, Some(access), deadline)
    }
    fn query_inner(
        &self,
        names: &[String],
        args: &Value,
        access: Option<&FederationArchiveAccess<'_>>,
        deadline: &Deadline,
    ) -> Result<Value> {
        deadline.check()?;
        let object = args
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("Collection query must be an object"))?;
        ensure!(
            object.keys().all(|key| [
                "operation",
                "query",
                "kind",
                "limit",
                "entityIds",
                "direction",
                "depth",
                "maxVisited",
                "relations",
                "resolutions",
                "startByte",
                "endByte"
            ]
            .contains(&key.as_str())),
            "Unknown collection query option"
        );
        let operation = args["operation"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Collection query operation required"))?;
        let view = self.join_inner(names, access, deadline)?;
        if operation == "read" {
            return self.read_view(&view, args, access, deadline);
        }
        if operation == "find" {
            let query = args["query"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Collection query text required"))?;
            ensure!(
                !query.is_empty() && query.len() <= 1000,
                "Invalid collection query text"
            );
            let limit = args
                .get("limit")
                .map(|v| {
                    v.as_u64()
                        .ok_or_else(|| anyhow::anyhow!("Invalid collection query limit"))
                })
                .transpose()?
                .unwrap_or(30) as usize;
            ensure!(
                (1..=2000).contains(&limit),
                "Collection query limit exceeds capacity"
            );
            let pattern = query.to_lowercase();
            let mut found = Vec::new();
            for node in view["nodes"].as_array().unwrap() {
                deadline.check()?;
                if args.get("kind").is_some_and(|kind| kind != &node["kind"]) {
                    continue;
                }
                if node["qualifiedName"]
                    .as_str()
                    .unwrap_or("")
                    .to_lowercase()
                    .contains(&pattern)
                    || node["signature"]
                        .as_str()
                        .unwrap_or("")
                        .to_lowercase()
                        .contains(&pattern)
                {
                    found.push(node.clone());
                }
            }
            if found
                .iter()
                .any(|node| node["name"].as_str() == Some(query))
            {
                found.retain(|node| node["name"].as_str() == Some(query));
            }
            found.sort_by_key(|n| {
                (
                    n["name"].as_str() != Some(query),
                    n["registration"].as_str().unwrap_or("").to_owned(),
                    n["path"].as_str().unwrap_or("").to_owned(),
                    n["startByte"].as_u64().unwrap_or(0),
                )
            });
            let total = found.len();
            found.truncate(limit);
            return Ok(
                json!({"collectionId":view["collectionId"],"lineage":view["lineage"],"items":found,"itemsOmitted":total.saturating_sub(limit),"complete":false,"retrieval":"qualified_name_and_signature_substring"}),
            );
        }
        ensure!(
            operation == "trace",
            "Unsupported collection query operation"
        );
        let ids = args["entityIds"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("Collection trace entities required"))?;
        ensure!(
            !ids.is_empty()
                && ids.len() <= 100
                && ids.iter().all(|id| id.as_str().is_some_and(valid_id)),
            "Invalid collection trace entities"
        );
        ensure!(
            args.get("direction").is_none_or(|v| v
                .as_str()
                .is_some_and(|s| ["in", "out", "both"].contains(&s))),
            "Invalid trace direction"
        );
        for (key, min, max) in [("depth", 0, 8), ("maxVisited", 1, 20000)] {
            ensure!(
                args.get(key)
                    .is_none_or(|v| v.as_u64().is_some_and(|n| (min..=max).contains(&n))),
                "Invalid collection traversal capacity"
            );
        }
        for key in ["relations", "resolutions"] {
            ensure!(
                args.get(key)
                    .is_none_or(|v| v.as_array().is_some_and(|a| a.len() <= 100
                        && a.iter().all(|v| v.as_str().is_some_and(|s| s.len() <= 100)))),
                "Invalid collection traversal filter"
            );
        }
        let graph = Graph {
            snapshot_id: view["collectionId"].as_str().unwrap().into(),
            entities: serde_json::from_value(view["nodes"].clone())?,
            observations: serde_json::from_value(view["observations"].clone())?,
            evidence: serde_json::from_value(view["evidence"].clone())?,
            ..Default::default()
        };
        let mut traced = crate::query::trace(&graph, args, deadline)?;
        for (result_key, view_key) in [
            ("entities", "nodes"),
            ("observations", "observations"),
            ("evidence", "evidence"),
        ] {
            let selected: BTreeSet<_> = traced[result_key]
                .as_array()
                .unwrap()
                .iter()
                .map(|item| item["id"].as_str().unwrap().to_owned())
                .collect();
            traced[result_key] = json!(
                view[view_key]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|item| selected.contains(item["id"].as_str().unwrap()))
                    .collect::<Vec<_>>()
            );
        }
        traced["lineage"] = view["lineage"].clone();
        traced["collectionId"] = view["collectionId"].clone();
        deadline.check()?;
        Ok(traced)
    }
    pub fn open(host_directory: &Path) -> Result<Self> {
        Self::check_paths(host_directory)?;
        let path = host_directory.join("yoke-collection.sqlite");
        let db = rusqlite::Connection::open(path)?;
        db.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE IF NOT EXISTS registrations(name TEXT PRIMARY KEY,id TEXT NOT NULL,payload TEXT NOT NULL); CREATE TABLE IF NOT EXISTS source_blobs(hash TEXT PRIMARY KEY,bytes BLOB NOT NULL)")?;
        Ok(Self {
            db,
            read_only: false,
            _read_guard: None,
        })
    }
    fn check_paths(host_directory: &Path) -> Result<()> {
        ensure!(
            host_directory.is_absolute(),
            "Registry requires absolute host-selected directory"
        );
        for component in host_directory.components() {
            ensure!(
                !matches!(
                    component,
                    std::path::Component::ParentDir | std::path::Component::CurDir
                ),
                "Registry path must be canonical"
            );
        }
        for ancestor in host_directory.ancestors() {
            let meta = fs::symlink_metadata(ancestor)?;
            ensure!(
                meta.is_dir() && !is_link(&meta),
                "Registry ancestor link refused"
            );
        }
        for suffix in ["", "-wal", "-shm", "-journal"] {
            let file = host_directory.join(format!("yoke-collection.sqlite{suffix}"));
            match fs::symlink_metadata(file) {
                Ok(meta) => ensure!(
                    meta.is_file() && !is_link(&meta),
                    "Registry database or sidecar link refused"
                ),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }
    fn writable(&self) -> Result<()> {
        ensure!(
            !self.read_only,
            "Read-only collection registry cannot mutate bindings or sources"
        );
        Ok(())
    }
    fn budget(&self, deadline: &Deadline) -> Result<()> {
        deadline.check()?;
        self.db
            .busy_timeout(deadline.remaining().min(Duration::from_millis(1000)))?;
        let deadline = deadline.clone();
        self.db
            .progress_handler(1000, Some(move || deadline.check().is_err()));
        Ok(())
    }
    pub fn register(
        &self,
        name: &str,
        store: &Store,
        snapshot: &Snapshot,
        graph: &Graph,
        deadline: &Deadline,
    ) -> Result<String> {
        self.register_inner(name, store, snapshot, graph, None, deadline)
    }
    #[allow(clippy::too_many_arguments)] // Host expectation is independent of source identity.
    fn register_inner(
        &self,
        name: &str,
        store: &Store,
        snapshot: &Snapshot,
        graph: &Graph,
        expected: Option<Option<&str>>,
        deadline: &Deadline,
    ) -> Result<String> {
        self.writable()?;
        if let Some(Some(id)) = expected {
            ensure!(valid_id(id), "Invalid expected registration ID");
        }
        name_ok(name)?;
        self.budget(deadline)?;
        crate::graph_extensions::validate_scope(store, snapshot, graph, deadline)?;
        let data = Registration {
            name: name.into(),
            snapshot: snapshot.clone(),
            graph: graph.clone(),
            archive: None,
            capture_scope: Some(crate::capture::archived_capture_scope(
                store, snapshot, deadline,
            )?),
            source_authority: None,
        };
        self.persist_registration(data, store, expected, deadline)
    }
    pub fn register_federated(
        &self,
        name: &str,
        access: &FederationArchiveAccess<'_>,
        grant_id: &str,
        remote_id: &str,
        deadline: &Deadline,
    ) -> Result<String> {
        self.register_foreign_inner(name, access, grant_id, remote_id, false, None, deadline)
    }
    pub fn register_federated_cas(
        &self,
        name: &str,
        access: &FederationArchiveAccess<'_>,
        grant_id: &str,
        remote_id: &str,
        expected: Option<&str>,
        deadline: &Deadline,
    ) -> Result<String> {
        self.register_foreign_inner(
            name,
            access,
            grant_id,
            remote_id,
            false,
            Some(expected),
            deadline,
        )
    }
    #[allow(clippy::too_many_arguments)] // Kind and CAS are separate host choices.
    fn register_foreign_inner(
        &self,
        name: &str,
        access: &FederationArchiveAccess<'_>,
        grant_id: &str,
        remote_id: &str,
        source_only: bool,
        expected: Option<Option<&str>>,
        deadline: &Deadline,
    ) -> Result<String> {
        self.writable()?;
        if let Some(Some(id)) = expected {
            ensure!(valid_id(id), "Invalid expected registration ID");
        }
        name_ok(name)?;
        self.budget(deadline)?;
        let canonical = if source_only {
            None
        } else {
            Some(access.capture(grant_id, remote_id, deadline)?)
        };
        let only_sources = if source_only {
            Some(access.capture_sources(grant_id, remote_id, deadline)?)
        } else {
            None
        };
        let (snapshot, graph, binding, sources) = if let Some(prepared) = &canonical {
            (
                prepared.snapshot(),
                prepared.graph().clone(),
                prepared.binding(),
                prepared.sources(),
            )
        } else {
            let prepared = only_sources
                .as_ref()
                .context("Source-only preparation absent")?;
            (
                prepared.snapshot(),
                Graph {
                    snapshot_id: prepared.snapshot().id.clone(),
                    ..Default::default()
                },
                prepared.binding(),
                prepared.sources(),
            )
        };
        let data = Registration {
            name: name.into(),
            snapshot: snapshot.clone(),
            graph,
            archive: None,
            capture_scope: None,
            source_authority: Some(if source_only {
                SourceAuthority::FederatedSources {
                    binding: binding.clone(),
                }
            } else {
                SourceAuthority::FederatedCanonical {
                    binding: binding.clone(),
                }
            }),
        };
        self.persist_sources(
            data,
            &mut |file, _| {
                Ok(sources
                    .get(&file.path)
                    .context("Foreign archive source missing")?
                    .clone())
            },
            expected,
            deadline,
        )
    }
    pub fn register_federated_sources(
        &self,
        name: &str,
        access: &FederationArchiveAccess<'_>,
        grant_id: &str,
        remote_id: &str,
        deadline: &Deadline,
    ) -> Result<String> {
        self.register_foreign_inner(name, access, grant_id, remote_id, true, None, deadline)
    }
    pub fn register_federated_sources_cas(
        &self,
        name: &str,
        access: &FederationArchiveAccess<'_>,
        grant_id: &str,
        remote_id: &str,
        expected: Option<&str>,
        deadline: &Deadline,
    ) -> Result<String> {
        self.register_foreign_inner(
            name,
            access,
            grant_id,
            remote_id,
            true,
            Some(expected),
            deadline,
        )
    }
    pub fn read_source_authorized(
        &self,
        name: &str,
        path: &str,
        range: std::ops::Range<usize>,
        access: &FederationArchiveAccess<'_>,
        deadline: &Deadline,
    ) -> Result<Value> {
        self.read_source_inner(name, path, range, Some(access), deadline)
    }
    fn read_source_inner(
        &self,
        name: &str,
        path: &str,
        range: std::ops::Range<usize>,
        access: Option<&FederationArchiveAccess<'_>>,
        deadline: &Deadline,
    ) -> Result<Value> {
        name_ok(name)?;
        crate::paths::normalized_syntax(path)?;
        ensure!(
            range.start <= range.end && range.end - range.start <= 256000,
            "Invalid collection source output range"
        );
        self.budget(deadline)?;
        let (id, payload) = self.stored_registration(name, 32_000_000)?;
        let data: Registration =
            crate::json::decode(crate::json::parse(payload.as_bytes(), 32_000_000)?)?;
        ensure!(
            data.name == name
                && identity(&data) == id
                && data.graph.snapshot_id == data.snapshot.id,
            "Collection registration integrity mismatch"
        );
        self.verify_archive(&data, access, deadline)?;
        let file = data
            .snapshot
            .files
            .iter()
            .find(|file| file.path == path)
            .context("Archived source path absent")?;
        let text = String::from_utf8(self.source_bytes(file, deadline)?)?;
        crate::paths::utf8_range(&text, range.start, range.end)?;
        deadline.check()?;
        Ok(
            json!({"registration":name,"registrationId":id,"snapshotId":data.snapshot.id,"workspaceId":data.snapshot.workspace_id,"path":path,"contentHash":file.hash,"startByte":range.start,"endByte":range.end,"text":text[range],"anchor":"exact","source":"collection_immutable_archive","executionAuthority":"none"}),
        )
    }
    fn persist_registration(
        &self,
        data: Registration,
        store: &Store,
        expected: Option<Option<&str>>,
        deadline: &Deadline,
    ) -> Result<String> {
        let snapshot = data.snapshot.clone();
        self.persist_sources(
            data,
            &mut |file, _| {
                let (text, content_hash) = crate::query::source(store, &snapshot, &file.path)?;
                ensure!(
                    content_hash == file.hash,
                    "Collection source binding mismatch"
                );
                Ok(text.into_bytes())
            },
            expected,
            deadline,
        )
    }
    fn persist_sources(
        &self,
        data: Registration,
        read_source: &mut dyn FnMut(&crate::model::SourceFile, &Deadline) -> Result<Vec<u8>>,
        expected: Option<Option<&str>>,
        deadline: &Deadline,
    ) -> Result<String> {
        self.writable()?;
        let name = &data.name;
        let snapshot = &data.snapshot;
        let payload = serde_json::to_string(&data)?;
        ensure!(
            payload.len() <= 32_000_000,
            "Collection registration byte capacity exceeded"
        );
        let id = identity(&data);
        let tx = rusqlite::Transaction::new_unchecked(
            &self.db,
            rusqlite::TransactionBehavior::Immediate,
        )?;
        let count: i64 = self
            .db
            .query_row("SELECT COUNT(*) FROM registrations", [], |r| r.get(0))?;
        ensure!(
            count < 100
                || self.db.query_row(
                    "SELECT EXISTS(SELECT 1 FROM registrations WHERE name=?1)",
                    [name],
                    |r| r.get::<_, bool>(0)
                )?,
            "Collection registration capacity exceeded"
        );
        let changed=match expected {
            Some(None)=>self.db.execute("INSERT INTO registrations(name,id,payload) VALUES(?1,?2,?3) ON CONFLICT(name) DO NOTHING",rusqlite::params![name,id,payload])?,
            Some(Some(prior))=>self.db.execute("UPDATE registrations SET id=?2,payload=?3 WHERE name=?1 AND id=?4",rusqlite::params![name,id,payload,prior])?,
            None=>self.db.execute("INSERT INTO registrations(name,id,payload) VALUES(?1,?2,?3) ON CONFLICT(name) DO UPDATE SET id=excluded.id,payload=excluded.payload",rusqlite::params![name,id,payload])?,
        };
        ensure!(
            changed == 1,
            "Collection registration missing, already present, or stale"
        );
        let mut total = 0usize;
        for file in &snapshot.files {
            deadline.check()?;
            total = total
                .checked_add(file.size)
                .ok_or_else(|| anyhow::anyhow!("Collection source capacity overflow"))?;
            ensure!(
                total <= 64_000_000,
                "Collection source archive capacity exceeded"
            );
            let source = read_source(file, deadline)?;
            ensure!(
                hash(&source) == file.hash
                    && source.len() == file.size
                    && std::str::from_utf8(&source).is_ok(),
                "Collection source binding mismatch"
            );
            self.db.execute(
                "INSERT INTO source_blobs(hash,bytes) VALUES(?1,?2) ON CONFLICT(hash) DO NOTHING",
                rusqlite::params![file.hash, source],
            )?;
            // Bound the existing SQLite cell before SQLite evaluates length or
            // converts a corrupt TEXT cell; the helper restores the prior cap.
            let bytes = self.source_bytes(file, deadline)?;
            ensure!(
                hash(&bytes) == file.hash,
                "Collection immutable source archive differs"
            );
        }
        deadline.check()?;
        tx.commit()?;
        Ok(id)
    }
    pub fn remove(
        &self,
        name: &str,
        expected_registration: &str,
        deadline: &Deadline,
    ) -> Result<()> {
        self.writable()?;
        name_ok(name)?;
        ensure!(
            valid_id(expected_registration),
            "Invalid expected registration ID"
        );
        self.budget(deadline)?;
        let tx = rusqlite::Transaction::new_unchecked(
            &self.db,
            rusqlite::TransactionBehavior::Immediate,
        )?;
        ensure!(
            self.db.execute(
                "DELETE FROM registrations WHERE name=?1 AND id=?2",
                rusqlite::params![name, expected_registration]
            )? == 1,
            "Collection registration missing or stale"
        );
        deadline.check()?;
        tx.commit()?;
        Ok(())
    }
    pub fn join(&self, names: &[String], deadline: &Deadline) -> Result<Value> {
        self.join_inner(names, None, deadline)
    }
    pub fn join_authorized(
        &self,
        names: &[String],
        access: &FederationArchiveAccess<'_>,
        deadline: &Deadline,
    ) -> Result<Value> {
        self.join_inner(names, Some(access), deadline)
    }
    fn join_inner(
        &self,
        names: &[String],
        access: Option<&FederationArchiveAccess<'_>>,
        deadline: &Deadline,
    ) -> Result<Value> {
        self.budget(deadline)?;
        ensure!(
            !names.is_empty() && names.len() <= 100,
            "Invalid collection scope size"
        );
        let mut seen = BTreeSet::new();
        let mut nodes = vec![];
        let mut observations = vec![];
        let mut evidence = vec![];
        let mut lineage = vec![];
        let mut bytes = 0usize;
        let mut scopes = BTreeSet::new();
        for name in names {
            name_ok(name)?;
            ensure!(
                seen.insert(name),
                "Duplicate collection registration requested"
            );
            deadline.check()?;
            let (id, payload) =
                self.stored_registration(name, 64_000_000usize.saturating_sub(bytes))?;
            bytes = bytes
                .checked_add(payload.len())
                .ok_or_else(|| anyhow::anyhow!("Collection byte capacity overflow"))?;
            ensure!(
                bytes <= 64_000_000,
                "Collection join byte capacity exceeded"
            );
            let data: Registration = serde_json::from_str(&payload)?;
            ensure!(
                data.name == *name
                    && valid_id(&id)
                    && identity(&data) == id
                    && data.graph.snapshot_id == data.snapshot.id,
                "Collection registration integrity mismatch"
            );
            let replay = self.verify_archive(&data, access, deadline)?;
            let scope = if let Some(proof) = &data.archive {
                identity(&(
                    &data.snapshot.workspace_id,
                    &data.snapshot.id,
                    &proof.view.id,
                ))
            } else {
                identity(&(&data.snapshot.workspace_id, &data.snapshot.id))
            };
            ensure!(
                scopes.insert(scope.clone()),
                "Duplicate immutable workspace/snapshot scopes; choose one registration"
            );
            let map: BTreeMap<_, _> = data
                .graph
                .entities
                .iter()
                .map(|n| (n.id.as_str(), identity(&(&scope, &n.id))))
                .collect();
            let anchors: BTreeMap<_, _> = data
                .graph
                .evidence
                .iter()
                .map(|e| (e.id.as_str(), identity(&(&scope, &e.id))))
                .collect();
            let original_nodes: BTreeMap<_, _> = data
                .archive
                .iter()
                .flat_map(|proof| proof.view.nodes.iter())
                .map(|row| (row["id"].as_str().unwrap(), row))
                .collect();
            let original_evidence: BTreeMap<_, _> = data
                .archive
                .iter()
                .flat_map(|proof| proof.view.evidence.iter())
                .map(|row| (row["id"].as_str().unwrap(), row))
                .collect();
            let original_arcs: BTreeMap<_, _> = data
                .archive
                .iter()
                .flat_map(|proof| proof.view.observations.iter())
                .map(|row| (row["id"].as_str().unwrap(), row))
                .collect();
            lineage.push(json!({"registration":name,"registrationId":id,"workspaceId":data.snapshot.workspace_id,"snapshotId":data.snapshot.id,"graphHash":identity(&data.graph),"namespace":scope}));
            if let Some(authority) = &data.source_authority {
                lineage.last_mut().unwrap()["sourceAuthority"] = json!({"kind":authority.kind(),"provenance":authority.binding().provenance(deadline)?,"authorization":"current_host_context_rechecked","executionAuthority":"none"});
            } else {
                lineage.last_mut().unwrap()["canonicalReplay"] = replay_provenance(replay);
            }
            if let Some(proof) = &data.archive {
                let item = lineage.last_mut().unwrap();
                item["viewId"] = json!(proof.view.id);
                item["viewArtifactId"] = json!(proof.view.artifact_id);
                item["baseGraphHash"] = json!(proof.view.base_graph_hash);
                item["selectedOrigins"] = json!(proof.view.overlays);
            }
            for n in &data.graph.entities {
                deadline.check()?;
                let mut v = serde_json::to_value(n)?;
                if let Some(proof) = &data.archive {
                    let raw = original_nodes
                        .get(n.id.as_str())
                        .context("Archived view node absent")?;
                    for (key, value) in raw
                        .as_object()
                        .context("Archived view node is not an object")?
                    {
                        if v.get(key).is_none() {
                            v[key] = value.clone();
                        }
                    }
                    v["viewEntityId"] = json!(n.id);
                    v["viewId"] = json!(proof.view.id);
                    v["viewNamespace"] = raw["namespace"].clone();
                }
                v["id"] = json!(map[n.id.as_str()]);
                v["baseEntityId"] = json!(n.id);
                v["baseEntityKey"] = json!(n.key);
                v["key"] = json!(identity(&(&scope, &n.key)));
                v["parentKey"] = json!(n.parent_key.as_ref().map(|key| identity(&(&scope, key))));
                v["workspaceId"] = json!(data.snapshot.workspace_id);
                v["namespace"] = json!(scope);
                v["registration"] = json!(name);
                nodes.push(v);
                ensure!(nodes.len() <= 20000, "Collection node capacity exceeded");
            }
            for e in &data.graph.evidence {
                let mut v = serde_json::to_value(e)?;
                if let Some(proof) = &data.archive {
                    let raw = original_evidence
                        .get(e.id.as_str())
                        .context("Archived evidence absent")?;
                    for (key, value) in raw
                        .as_object()
                        .context("Archived evidence is not an object")?
                    {
                        if v.get(key).is_none() {
                            v[key] = value.clone();
                        }
                    }
                    v["viewId"] = json!(proof.view.id);
                }
                v["id"] = json!(anchors[e.id.as_str()]);
                v["baseEvidenceId"] = json!(e.id);
                v["workspaceId"] = json!(data.snapshot.workspace_id);
                v["namespace"] = json!(scope);
                evidence.push(v);
                ensure!(
                    evidence.len() <= 100000,
                    "Collection evidence capacity exceeded"
                );
            }
            for o in &data.graph.observations {
                deadline.check()?;
                let mut v = serde_json::to_value(o)?;
                if let Some(proof) = &data.archive {
                    let raw = original_arcs
                        .get(o.id.as_str())
                        .context("Archived observation absent")?;
                    for (key, value) in raw
                        .as_object()
                        .context("Archived observation is not an object")?
                    {
                        if v.get(key).is_none() {
                            v[key] = value.clone();
                        }
                    }
                    v["viewId"] = json!(proof.view.id);
                }
                v["id"] = json!(identity(&(&scope, &o.id)));
                v["baseObservationId"] = json!(o.id);
                v["source"] = json!(map[o.source.as_str()]);
                v["target"] = json!(o.target.as_ref().map(|t| &map[t.as_str()]));
                v["evidenceIds"] = json!(
                    o.evidence_ids
                        .iter()
                        .map(|id| &anchors[id.as_str()])
                        .collect::<Vec<_>>()
                );
                v["namespace"] = json!(scope);
                observations.push(v);
                ensure!(
                    observations.len() <= 100000,
                    "Collection observation capacity exceeded"
                );
            }
        }
        let mut grouped = BTreeMap::<(String, String), Vec<&Value>>::new();
        for n in &nodes {
            if ["class", "struct", "interface", "type", "enum"]
                .contains(&n["kind"].as_str().unwrap_or(""))
            {
                grouped
                    .entry((
                        n["kind"].as_str().unwrap().into(),
                        n["name"].as_str().unwrap().into(),
                    ))
                    .or_default()
                    .push(n);
            }
        }
        let mut candidates = vec![];
        for group in grouped.values() {
            for (i, a) in group.iter().enumerate() {
                for b in group.iter().skip(i + 1) {
                    deadline.check()?;
                    if a["workspaceId"] == b["workspaceId"] {
                        continue;
                    }
                    ensure!(
                        candidates.len() < 10000,
                        "Collection candidate capacity exceeded"
                    );
                    candidates.push(json!({"id":identity(&("shared_type_candidate",&a["id"],&b["id"])),"source":a["id"],"target":b["id"],"relation":"shared_type_candidate","resolution":"heuristic","basis":"same_kind_and_name_across_registered_workspaces"}));
                }
            }
        }
        let mut result = json!({"producer":"yoke-graph-collection-1","lineage":lineage,"nodes":nodes,"observations":observations,"evidence":evidence,"candidates":candidates,"complete":false,"limitations":["Namespaces preserve immutable source scopes; shared-type names are heuristic candidates and do not prove type identity.","Registry registration is host authority; repository files cannot register or launch providers."]});
        let id = identity(&result);
        result["collectionId"] = json!(id);
        deadline.check()?;
        Ok(result)
    }
}
