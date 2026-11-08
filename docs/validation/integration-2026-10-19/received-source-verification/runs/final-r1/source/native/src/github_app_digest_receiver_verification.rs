//! Explicit host-local correspondence checks. Sender JSON never selects a source
//! Store or grants authority; received packet/projection identities remain distinct.
use super::*;
use crate::github_app::{HostProfile, digest as native};
use anyhow::Context as _;

const INVALID: &str = "github_app_digest_verification_input_invalid";
const UNAVAILABLE: &str = "github_app_digest_verification_unavailable";
const MISMATCH: &str = "github_app_digest_verification_mismatch";
const CAPACITY: &str = "github_app_digest_verification_capacity_exceeded";
const PERMISSION: &str = "github_app_digest_verification_permission_required";

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub schema_version: u8,
    pub app_id: u64,
    pub sender_workspace_id: String,
    pub installation_id: u64,
    pub repository_id: u64,
    pub repository: String,
    pub proposal_id: String,
    pub receipt_id: String,
    pub local_snapshot_id: String,
}
impl Request {
    pub fn parse(raw: &[u8]) -> Result<Self> {
        let r: Self = crate::json::parse(raw, 8192)
            .and_then(crate::json::decode)
            .map_err(|_| anyhow::anyhow!(INVALID))?;
        r.validate()?;
        Ok(r)
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == 1
                && crate::github_app::numeric(self.app_id)
                && crate::github_app::numeric(self.installation_id)
                && crate::github_app::numeric(self.repository_id)
                && crate::github_app::repository(&self.repository)
                && [
                    &self.sender_workspace_id,
                    &self.proposal_id,
                    &self.receipt_id,
                    &self.local_snapshot_id
                ]
                .iter()
                .all(|id| valid_id(id)),
            INVALID
        );
        Ok(())
    }
    fn permits(&self, s: &Scope) -> bool {
        self.app_id == s.app_id
            && self.sender_workspace_id == s.workspace_id
            && self.installation_id == s.installation_id
            && self.repository_id == s.repository_id
            && self.repository == s.repository
    }
    fn local(&self) -> native::PreviewRequest {
        native::PreviewRequest {
            schema_version: 1,
            snapshot_id: self.local_snapshot_id.clone(),
            installation_id: self.installation_id,
            repository_id: self.repository_id,
            repository: self.repository.clone(),
        }
    }
}
/// Constructed by the host only; never deserialized from either packet or request.
pub struct Options {
    pub allow_query: bool,
    pub allow_verification: bool,
    pub require_git_identity: bool,
    pub max_packet_bytes: usize,
    pub max_symbols: usize,
    pub max_output_bytes: usize,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            allow_query: false,
            allow_verification: false,
            require_git_identity: false,
            max_packet_bytes: 4_000_000,
            max_symbols: 4000,
            max_output_bytes: 4_000_000,
        }
    }
}
pub struct Context<'a> {
    pub receiver_store: &'a Store,
    pub receiver_profile: &'a Profile,
    pub principal: &'a str,
    pub source_store: &'a Store,
    pub source_profile: &'a HostProfile,
    pub source_options: &'a native::Options,
    pub options: &'a Options,
}
/// Frozen checked projection, with no JSON constructor or source-write capability.
pub struct CheckedDigest {
    value: Value,
}
fn admit_options(o: &Options, d: &Deadline) -> Result<()> {
    ensure!(o.allow_query && o.allow_verification, PERMISSION);
    d.check()?;
    ensure!(
        (1..=4_000_000).contains(&o.max_packet_bytes)
            && (1..=4000).contains(&o.max_symbols)
            && (1024..=16_000_000).contains(&o.max_output_bytes),
        CAPACITY
    );
    Ok(())
}
pub fn admit(
    profile: &Profile,
    principal: &str,
    source: &HostProfile,
    r: &Request,
    o: &Options,
    d: &Deadline,
) -> Result<()> {
    admit_options(o, d)?;
    r.validate()?;
    ensure!(crate::github_app::credential_name(principal), INVALID);
    profile.validate()?;
    ensure!(
        profile.grants.iter().any(|a| a.id == principal
            && a.allow_query
            && a.app_id == r.app_id
            && a.sender_workspace_id == r.sender_workspace_id
            && a.installation_id == r.installation_id
            && a.repository_id == r.repository_id
            && a.repository == r.repository),
        PERMISSION
    );
    ensure!(source.app_id == r.app_id, PERMISSION);
    native::authorize_profile(source, &r.local()).map_err(|_| anyhow::anyhow!(PERMISSION))?;
    Ok(())
}
pub fn verify(c: &Context<'_>, r: &Request, d: &Deadline) -> Result<CheckedDigest> {
    admit(
        c.receiver_profile,
        c.principal,
        c.source_profile,
        r,
        c.options,
        d,
    )?;
    c.source_options.validate()?;
    // End the receipt read before opening the independent source read. Both are
    // immutable selections; this also permits two handles to the same Store.
    let (raw, receipt, received_at) =
        storage::sql(c.receiver_store, c.receiver_profile, false, d, |h| {
            let (record, scope) = h
                .records
                .get(&r.proposal_id)
                .filter(|(record, scope)| {
                    record.endpoint == c.receiver_profile.endpoint
                        && record.principal_id == c.principal
                        && r.permits(scope)
                })
                .ok_or_else(|| anyhow::anyhow!(UNAVAILABLE))?;
            let receipt = record.receipt();
            ensure!(
                receipt["receiptId"].as_str() == Some(r.receipt_id.as_str()),
                MISMATCH
            );
            ensure!(record.body.len() <= c.options.max_packet_bytes, CAPACITY);
            let _ = scope;
            Ok((record.body.clone(), receipt, record.received_at_unix))
        })?;
    let packet = Submission::parse(raw.as_bytes(), c.options.max_packet_bytes, d)
        .map_err(|_| anyhow::anyhow!(UNAVAILABLE))?;
    let payload = &packet.digest["payload"];
    let claims = payload["symbols"].as_array().context(UNAVAILABLE)?;
    ensure!(
        !claims.is_empty(),
        "github_app_digest_verification_no_supported_declarations"
    );
    ensure!(claims.len() <= c.options.max_symbols, CAPACITY);
    let mut limits = c.source_options.clone();
    limits.max_symbols = limits.max_symbols.min(c.options.max_symbols);
    let local =
        native::preview_with_profile(c.source_store, c.source_profile, &limits, &r.local(), d)
            .map_err(|error| {
                d.check().err().unwrap_or_else(|| {
                    if matches!(
                        error.downcast_ref::<native::Failure>(),
                        Some(native::Failure::Capacity)
                    ) {
                        anyhow::anyhow!(CAPACITY)
                    } else {
                        anyhow::anyhow!(UNAVAILABLE)
                    }
                })
            })?;
    ensure!(local["symbolOmissions"] == 0, CAPACITY);
    let symbols = local["symbols"].as_array().context(UNAVAILABLE)?;
    ensure!(claims.len() == symbols.len(), MISMATCH);
    for key in [
        "symbolCandidates",
        "symbolOmissions",
        "unsupportedFiles",
        "visibilityUnresolved",
        "analyzer",
        "nativeEntityReplayScope",
    ] {
        d.check()?;
        ensure!(payload[key] == local[key], MISMATCH);
    }
    let claimed_git = payload["sourceCommitVerified"]
        .as_bool()
        .context(MISMATCH)?;
    if claimed_git {
        ensure!(
            local["sourceCommitVerified"] == true
                && payload["sourceCommitId"] == local["sourceCommitId"]
                && payload["sourceTreeId"] == local["sourceTreeId"],
            MISMATCH
        );
    } else {
        ensure!(
            payload["sourceCommitId"].is_null() && payload["sourceTreeId"].is_null(),
            MISMATCH
        );
    }
    ensure!(
        !c.options.require_git_identity || claimed_git,
        "github_app_digest_verification_git_identity_required"
    );
    let mut rows = Vec::new();
    for (i, (claim, source)) in claims.iter().zip(symbols).enumerate() {
        d.check()?;
        for key in [
            "path",
            "name",
            "kind",
            "signature",
            "fingerprint",
            "startByte",
            "endByte",
            "producer",
        ] {
            ensure!(claim[key] == source[key], MISMATCH);
        }
        if let Some(file_hash) = claim.get("sourceHash") {
            ensure!(*file_hash == source["contentHash"], MISMATCH);
        }
        let pointer = format!("/digest/payload/symbols/{i}");
        rows.push(json!({"id":identity(&("received_rust_source_match_v1",&r.receipt_id,&c.source_store.workspace_id,&r.local_snapshot_id,i)),
            "ordinal":i,"pointer":pointer,"receivedMetadata":claim,
            "localSource":{"workspaceId":c.source_store.workspace_id,"snapshotId":r.local_snapshot_id,
                "entityId":source["id"],"graphEntityId":source["graphEntityId"],"path":source["path"],
                "contentHash":source["contentHash"],"startByte":source["startByte"],"endByte":source["endByte"],"fingerprint":source["fingerprint"],
                "anchorGranularity":"full_declaration_utf8_byte_range","signatureGranularity":"trimmed_comment_stripped_header_projection"},
            "declarationBytesMatched":true,"headerProjectionMatched":true,"reportedWholeFileHashMatched":claim.get("sourceHash").is_some(),
            "compilerVerified":false,"semanticIdentityEstablished":false,"sourceAuthority":"host_selected_local_capture_only"}));
    }
    let value = json!({"schemaVersion":1,"kind":"native_received_source_verification_v1",
        "verificationId":identity(&("received_rust_source_verification_v1",&r.receipt_id,&c.source_store.workspace_id,&r.local_snapshot_id,&rows)),
        "receipt":receipt,"receiverWorkspaceId":c.receiver_store.workspace_id,"principalId":c.principal,"receivedAtUnix":received_at,
        "reportedScope":{"appId":r.app_id,"senderWorkspaceId":r.sender_workspace_id,"snapshotId":payload["snapshotId"],"installationId":r.installation_id,"repositoryId":r.repository_id,"repository":r.repository},
        "localScope":{"workspaceId":c.source_store.workspace_id,"snapshotId":r.local_snapshot_id,"sourceCommitId":local["sourceCommitId"],"sourceTreeId":local["sourceTreeId"],"analyzer":local["analyzer"]},
        "rows":rows,"verifiedSymbols":symbols.len(),"projectionCountersMatched":true,"advertisedRustProjectionChecked":true,
        "reportedGitIdentityMatched":claimed_git,"remoteRepositoryAttested":false,"senderSnapshotIdentityEstablished":false,
        "reportedSourceDigestHash":payload["sourceDigestHash"],"senderSourceDigestHashVerified":false,
        "unsupportedFiles":local["unsupportedFiles"],"visibilityUnresolved":local["visibilityUnresolved"],
        "discussionCaptureId":payload["discussionCaptureId"],"discussionsVerified":false,"historicalCommitsVerified":false,
        "sourceAuthority":false,"complete":false,"networkRequests":0,"modelCalls":0,"sourceWritten":false,"recordsWritten":false,
        "limitations":["Every advertised Rust declaration is compared in original order with a separately replayed local projection, including full declaration fingerprint and optional supplied whole-file hash. Matching headers do not establish semantic identity or compiler accessibility.",
        "Sender snapshot/workspace IDs, source-digest hash, discussion claims and historical commit arrays remain received metadata. A matching local Git identity does not attest remote ownership or a fresh remote head. No whole-source equivalence or semantic reasoning is claimed."]});
    admit_output(&value, c.options.max_output_bytes, d)?;
    Ok(CheckedDigest { value })
}
fn admit_output(value: &Value, max_bytes: usize, d: &Deadline) -> Result<()> {
    crate::json::encode_bounded(value, max_bytes - 1, d)
        .map_err(|_| d.check().err().unwrap_or_else(|| anyhow::anyhow!(CAPACITY)))?;
    d.check()
}
impl CheckedDigest {
    pub fn export(&self, max_bytes: usize, d: &Deadline) -> Result<Value> {
        ensure!((1024..=16_000_000).contains(&max_bytes), CAPACITY);
        d.check()?;
        let value = self.value.clone();
        admit_output(&value, max_bytes, d)?;
        Ok(value)
    }
    pub fn find(&self, text: &str, max_bytes: usize, d: &Deadline) -> Result<Value> {
        ensure!((1024..=16_000_000).contains(&max_bytes), CAPACITY);
        ensure!(
            !text.is_empty()
                && text.len() <= 1024
                && text.chars().count() <= 256
                && !text.chars().any(char::is_control),
            INVALID
        );
        let mut rows = Vec::new();
        for row in self.value["rows"].as_array().context(UNAVAILABLE)? {
            d.check()?;
            if ["path", "name", "kind", "signature"].iter().any(|key| {
                row["receivedMetadata"][*key]
                    .as_str()
                    .is_some_and(|s| s.contains(text))
            }) {
                rows.push(row.clone());
            }
        }
        let matched = rows.len();
        let mut result = self.value.clone();
        result["rows"] = json!(rows);
        result["query"] = json!({"kind":"literal_verified_header_match","text":text,"matchedRows":matched,"completeForLiteralSelection":true});
        admit_output(&result, max_bytes, d)?;
        Ok(result)
    }
}
