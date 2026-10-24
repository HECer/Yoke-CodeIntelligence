//! Archived foreign source remains subject to the receiving host's live grants.
use crate::{
    Engine,
    capture::Deadline,
    federation::{self, ImportGrant},
    model::{Graph, Snapshot, SourceFile},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// Persisted provenance, deliberately containing no grant or receiving policy.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FederationArchiveBinding {
    schema_version: u32,
    grant_id: String,
    receipt: Value,
}

impl FederationArchiveBinding {
    pub fn grant_id(&self) -> &str {
        &self.grant_id
    }

    /// Provenance metadata alone does not validate a grant or authorize bytes.
    pub fn provenance(&self, deadline: &Deadline) -> Result<Value> {
        ensure!(
            self.schema_version == 1,
            "Unknown federation archive binding version"
        );
        federation::archive_receipt_provenance(&self.receipt, deadline)
    }
}

/// Created only by a trusted Rust host. Neither JSON nor a signed receipt can
/// construct the receiving engine or the host's currently authorized grant map.
pub struct FederationArchiveAccess<'a> {
    receiver: &'a Engine,
    grants: &'a BTreeMap<String, ImportGrant>,
}

pub struct PreparedFederationArchive {
    pub(crate) binding: FederationArchiveBinding,
    pub(crate) snapshot: Snapshot,
    pub(crate) graph: Graph,
    pub(crate) sources: BTreeMap<String, Vec<u8>>,
}

/// Checked signed source bytes, without authority over an original local graph.
pub struct PreparedFederationSources {
    binding: FederationArchiveBinding,
    snapshot: Snapshot,
    sources: BTreeMap<String, Vec<u8>>,
}

impl<'a> FederationArchiveAccess<'a> {
    /// HTTP hosts must supply only grants currently allowed to that principal.
    /// Keep their policy read lock through the whole archive operation.
    pub fn new(receiver: &'a Engine, grants: &'a BTreeMap<String, ImportGrant>) -> Self {
        Self { receiver, grants }
    }

    pub fn capture(
        &self,
        grant_id: &str,
        remote_id: &str,
        deadline: &Deadline,
    ) -> Result<PreparedFederationArchive> {
        let prepared = self.capture_sources(grant_id, remote_id, deadline)?;
        let grant = self
            .grants
            .get(grant_id)
            .context("Current archive import grant is absent")?;
        let graph =
            federation::graph_for_archive_validation(self.receiver, remote_id, grant, deadline)?;
        self.authorize(
            &prepared.binding,
            &prepared.snapshot,
            &graph,
            &mut |file, _| {
                Ok(prepared
                    .sources
                    .get(&file.path)
                    .context("Captured archive source missing")?
                    .clone())
            },
            deadline,
        )?;
        Ok(PreparedFederationArchive {
            binding: prepared.binding,
            snapshot: prepared.snapshot,
            graph,
            sources: prepared.sources,
        })
    }

    pub fn capture_sources(
        &self,
        grant_id: &str,
        remote_id: &str,
        deadline: &Deadline,
    ) -> Result<PreparedFederationSources> {
        let grant = self
            .grants
            .get(grant_id)
            .context("Current archive import grant is absent")?;
        ensure!(
            !grant_id.is_empty()
                && grant_id.len() <= 200
                && !grant_id.chars().any(char::is_control),
            "Invalid archive grant ID"
        );
        let receipt = federation::archive_receipt(self.receiver, remote_id, grant, deadline)?;
        let snapshot = federation::archive_receipt_snapshot(&receipt, deadline)?;
        let binding = FederationArchiveBinding {
            schema_version: 1,
            grant_id: grant_id.into(),
            receipt,
        };
        let mut sources = BTreeMap::new();
        let mut total = 0usize;
        self.authorize_sources(
            &binding,
            &snapshot,
            &mut |file, _| {
                ensure!(
                    file.size <= 16_000_000usize.saturating_sub(total),
                    "Federated archive source capacity exceeded"
                );
                let bytes = self
                    .receiver
                    .store
                    .read_blob_exact(&file.hash, file.size, 4_000_000)?;
                total += bytes.len();
                sources.insert(file.path.clone(), bytes.clone());
                Ok(bytes)
            },
            deadline,
        )?;
        Ok(PreparedFederationSources {
            binding,
            snapshot,
            sources,
        })
    }

    /// Re-resolves the current grant and current local receiver policy before
    /// reading any archived bytes. A successful earlier call grants no lease.
    pub fn authorize(
        &self,
        binding: &FederationArchiveBinding,
        snapshot: &Snapshot,
        graph: &Graph,
        read_source: &mut dyn FnMut(&SourceFile, &Deadline) -> Result<Vec<u8>>,
        deadline: &Deadline,
    ) -> Result<()> {
        ensure!(
            binding.schema_version == 1,
            "Unknown federation archive binding version"
        );
        ensure!(
            !binding.grant_id.is_empty()
                && binding.grant_id.len() <= 200
                && !binding.grant_id.chars().any(char::is_control),
            "Invalid archive grant ID"
        );
        let grant = self
            .grants
            .get(&binding.grant_id)
            .context("Current archive import grant is absent or revoked")?;
        federation::validate_archive_receipt(
            self.receiver,
            &binding.receipt,
            snapshot,
            graph,
            grant,
            read_source,
            deadline,
        )
    }

    /// Verifies signed immutable bytes and current policy only. Original local
    /// graph hashes in the receipt carry no canonical graph authority.
    pub fn authorize_sources(
        &self,
        binding: &FederationArchiveBinding,
        snapshot: &Snapshot,
        read_source: &mut dyn FnMut(&SourceFile, &Deadline) -> Result<Vec<u8>>,
        deadline: &Deadline,
    ) -> Result<()> {
        ensure!(
            binding.schema_version == 1,
            "Unknown federation archive binding version"
        );
        ensure!(
            !binding.grant_id.is_empty()
                && binding.grant_id.len() <= 200
                && !binding.grant_id.chars().any(char::is_control),
            "Invalid archive grant ID"
        );
        let grant = self
            .grants
            .get(&binding.grant_id)
            .context("Current archive import grant is absent or revoked")?;
        federation::validate_archive_sources(
            self.receiver,
            &binding.receipt,
            snapshot,
            grant,
            read_source,
            deadline,
        )
    }
}

impl PreparedFederationSources {
    pub fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }
    pub fn binding(&self) -> &FederationArchiveBinding {
        &self.binding
    }
    pub fn sources(&self) -> &BTreeMap<String, Vec<u8>> {
        &self.sources
    }
}

impl PreparedFederationArchive {
    pub fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }
    pub fn graph(&self) -> &Graph {
        &self.graph
    }
    pub fn binding(&self) -> &FederationArchiveBinding {
        &self.binding
    }
    pub fn sources(&self) -> &BTreeMap<String, Vec<u8>> {
        &self.sources
    }
}
