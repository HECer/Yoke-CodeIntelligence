//! Host-authorized offline immutable source federation.
use crate::{
    Engine, analysis,
    capture::Deadline,
    model::{ANALYZER, Graph, Omission, Snapshot, SourceFile, hash, identity, valid_id},
    paths::{is_link, normalized, obvious_secret, scoped},
    store::Store,
};
use anyhow::{Context, Result, ensure};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use rusqlite::{OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use unicode_normalization::UnicodeNormalization;

const DOMAIN: &[u8] = b"YOKE-SOURCE-FEDERATION-V1\0";
const RECEIPT: &str = "native_federation_receipt_v1";
pub const RECEIPT_V2: &str = "native_federation_receipt_v2";
const REMOTE_GRAPH: &str = "native_federation_graph_v1";
const REPLAY: &str = "native_federation_replay_v1";
const MAX_FILES: usize = 2_000;
const MAX_FILE: usize = 4_000_000;
const MAX_SOURCE: usize = 16_000_000;
const MAX_ENVELOPE: usize = 32_000_000;
const MAX_STATE: usize = 64_000_000;
const MAX_TTL: u64 = 7 * 24 * 60 * 60 * 1000;
const SIGNED_SOURCE_LIMITATION: &str = "A valid sender signature authenticates supplied bytes and scope, not completeness, safety or current source freshness.";
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Scope {
    pub tenant_id: String,
    pub project_id: String,
    pub sender_id: String,
    pub key_id: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExportScope {
    pub scope: Scope,
    pub ttl_ms: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImportGrant {
    pub scope: Scope,
    pub public_key: String,
    pub sender_workspace_id: String,
    #[serde(default)]
    pub exclude: Vec<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManifestFile {
    path: String,
    content_hash: String,
    size: usize,
    mode: u32,
    language: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SignedOmission {
    path: String,
    reason: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Manifest {
    schema: u8,
    scope: Scope,
    sender_workspace_id: String,
    source_snapshot_id: String,
    source_kind: String,
    analyzer: String,
    nonce: String,
    issued_at_ms: u64,
    expires_at_ms: u64,
    files: Vec<ManifestFile>,
    omissions: Vec<SignedOmission>,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Content {
    path: String,
    base64: String,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Envelope {
    manifest: Manifest,
    signature: String,
    contents: Vec<Content>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Receipt {
    remote_id: String,
    receiver_workspace_id: String,
    manifest_id: String,
    public_key: String,
    manifest: Manifest,
    signature: String,
    graph_hash: String,
    snapshot: Snapshot,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReceiptV2 {
    schema_version: u8,
    receiver_analyzer: String,
    receipt: Receipt,
}
#[derive(Clone, Serialize)]
#[serde(untagged)]
enum SelectedReceipt {
    V1(Receipt),
    V2(ReceiptV2),
}
impl std::ops::Deref for SelectedReceipt {
    type Target = Receipt;
    fn deref(&self) -> &Self::Target {
        match self {
            Self::V1(receipt) => receipt,
            Self::V2(receipt) => &receipt.receipt,
        }
    }
}
impl SelectedReceipt {
    fn into_snapshot(self) -> Snapshot {
        match self {
            Self::V1(receipt) => receipt.snapshot,
            Self::V2(receipt) => receipt.receipt.snapshot,
        }
    }
    fn epoch(&self) -> Result<analysis::AnalyzerEpoch> {
        match self {
            Self::V1(_) => Ok(analysis::AnalyzerEpoch::V8),
            Self::V2(receipt) => analysis::AnalyzerEpoch::parse(&receipt.receiver_analyzer),
        }
    }
    fn kind(&self) -> &'static str {
        match self {
            Self::V1(_) => RECEIPT,
            Self::V2(_) => RECEIPT_V2,
        }
    }
    fn classifier(&self) -> Result<&'static str> {
        let result = receiver_classifier(&self.snapshot, self.epoch()?);
        ensure!(
            !matches!(self, Self::V2(_)) || result == "compatible_current",
            "V2 snapshot classification does not match the declared receiver epoch"
        );
        Ok(result)
    }
    fn expected_remote_id(&self, workspace: &str) -> Result<String> {
        Ok(match self {
            Self::V1(_) => remote_id(workspace, &self.manifest, &self.manifest_id),
            Self::V2(_) => {
                remote_id_v2(workspace, &self.manifest, &self.manifest_id, self.epoch()?)
            }
        })
    }
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Replay {
    manifest_id: String,
    remote_id: String,
}

fn now() -> Result<u64> {
    Ok(u64::try_from(
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis(),
    )?)
}
fn scope_valid(scope: &Scope) -> Result<()> {
    for label in [
        &scope.tenant_id,
        &scope.project_id,
        &scope.sender_id,
        &scope.key_id,
    ] {
        ensure!(
            !label.is_empty()
                && label.len() <= 128
                && label
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.'))
                && !obvious_secret(label),
            "Invalid federation scope label"
        );
    }
    Ok(())
}
fn data_label(value: &str, limit: usize) -> Result<()> {
    ensure!(
        value.len() <= limit && !value.chars().any(char::is_control) && !obvious_secret(value),
        "Invalid federation data label"
    );
    Ok(())
}
fn canonical(manifest: &Manifest) -> Result<Vec<u8>> {
    Ok(serde_json::to_vec(&serde_json::to_value(manifest)?)?)
}
fn signing_message(manifest: &Manifest) -> Result<Vec<u8>> {
    let mut bytes = DOMAIN.to_vec();
    bytes.extend(canonical(manifest)?);
    Ok(bytes)
}
fn remote_id(workspace: &str, manifest: &Manifest, manifest_id: &str) -> String {
    identity(&(
        workspace,
        "federated-source-v1",
        &manifest.scope,
        &manifest.sender_workspace_id,
        &manifest.source_snapshot_id,
        manifest_id,
    ))
}
fn remote_id_v2(
    workspace: &str,
    manifest: &Manifest,
    manifest_id: &str,
    epoch: analysis::AnalyzerEpoch,
) -> String {
    identity(&(
        workspace,
        "federated-source-v2",
        &manifest.scope,
        &manifest.sender_workspace_id,
        &manifest.source_snapshot_id,
        manifest_id,
        epoch.producer(),
    ))
}
fn replay_id(manifest: &Manifest) -> String {
    identity(&(
        &manifest.scope,
        &manifest.sender_workspace_id,
        &manifest.nonce,
    ))
}
fn verify(
    manifest: &Manifest,
    signature: &str,
    grant: &ImportGrant,
    check_time: bool,
) -> Result<()> {
    scope_valid(&manifest.scope)?;
    scope_valid(&grant.scope)?;
    ensure!(
        manifest.schema == 1
            && manifest.scope == grant.scope
            && manifest.sender_workspace_id == grant.sender_workspace_id,
        "Signed source does not match the host grant"
    );
    ensure!(
        valid_id(&manifest.sender_workspace_id)
            && valid_id(&manifest.source_snapshot_id)
            && valid_id(&manifest.nonce)
            && valid_id(&grant.public_key),
        "Invalid federation source or key identity"
    );
    ensure!(
        ["disk", "derived", "editor", "overlay"].contains(&manifest.source_kind.as_str()),
        "Unsupported sender snapshot kind"
    );
    data_label(&manifest.analyzer, 200)?;
    ensure!(
        manifest.expires_at_ms > manifest.issued_at_ms
            && manifest.expires_at_ms - manifest.issued_at_ms <= MAX_TTL,
        "Invalid federation validity interval"
    );
    if check_time {
        let clock = now()?;
        ensure!(
            manifest.issued_at_ms <= clock.saturating_add(300_000)
                && manifest.expires_at_ms > clock,
            "Federation signature is expired or issued in the future"
        );
    }
    verify_signature(manifest, signature, &grant.public_key)
}
fn verify_signature(manifest: &Manifest, signature: &str, public_key: &str) -> Result<()> {
    ensure!(
        valid_id(public_key),
        "Invalid federation public key identity"
    );
    let public: [u8; 32] = hex::decode(public_key)?
        .try_into()
        .map_err(|_| anyhow::anyhow!("Invalid Ed25519 public key size"))?;
    let key = VerifyingKey::from_bytes(&public)?;
    ensure!(!key.is_weak(), "Weak federation public key refused");
    ensure!(
        signature.len() == 128
            && signature
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "Invalid federation signature encoding"
    );
    let signature: [u8; 64] = hex::decode(signature)?
        .try_into()
        .map_err(|_| anyhow::anyhow!("Invalid Ed25519 signature size"))?;
    key.verify_strict(
        &signing_message(manifest)?,
        &Signature::from_bytes(&signature),
    )?;
    Ok(())
}
fn manifest_valid(manifest: &Manifest) -> Result<()> {
    ensure!(
        manifest.files.len() <= MAX_FILES && manifest.omissions.len() <= MAX_FILES,
        "Federation file or omission capacity exceeded"
    );
    let mut previous = None;
    let mut aliases = BTreeSet::new();
    let mut upper_aliases = BTreeSet::new();
    let mut total = 0usize;
    for file in &manifest.files {
        normalized(&file.path)?;
        if let Some(previous) = previous {
            ensure!(
                previous < file.path.as_str(),
                "Federation file paths must be unique and sorted"
            );
        }
        previous = Some(file.path.as_str());
        let canonical = file.path.nfc().collect::<String>();
        ensure!(
            aliases.insert(canonical.to_lowercase())
                && upper_aliases.insert(canonical.to_uppercase()),
            "Duplicate or case-colliding federation path"
        );
        ensure!(
            valid_id(&file.content_hash) && file.size <= MAX_FILE && file.mode <= 0o777,
            "Invalid federation file hash, size or mode"
        );
        data_label(&file.language, 100)?;
        total = total
            .checked_add(file.size)
            .context("Federation source size overflow")?;
        ensure!(
            total <= MAX_SOURCE,
            "Federation decoded source capacity exceeded"
        );
    }
    for omission in &manifest.omissions {
        data_label(&omission.path, 4096)?;
        data_label(&omission.reason, 512)?;
    }
    Ok(())
}
pub fn export_snapshot(
    store: &Store,
    snapshot: &Snapshot,
    configuration: &ExportScope,
    private_key: &[u8; 32],
    deadline: &Deadline,
) -> Result<Value> {
    store.set_deadline(deadline)?;
    scope_valid(&configuration.scope)?;
    ensure!(
        configuration.ttl_ms > 0 && configuration.ttl_ms <= MAX_TTL,
        "Federation expiry must be within seven days"
    );
    ensure!(
        snapshot.workspace_id == store.workspace_id
            && identity(&store.snapshot(&snapshot.id)?) == identity(snapshot),
        "Only canonical snapshots from this local workspace can be exported"
    );
    ensure!(
        ["disk", "derived", "editor", "overlay"].contains(&snapshot.kind.as_str()),
        "Re-export of foreign source is not implicitly authorized"
    );
    let mut files: Vec<_> = snapshot
        .files
        .iter()
        .map(|file| ManifestFile {
            path: file.path.clone(),
            content_hash: file.hash.clone(),
            size: file.size,
            mode: file.mode,
            language: file.language.clone(),
        })
        .collect();
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let issued_at_ms = now()?;
    let mut entropy = [0u8; 32];
    getrandom::fill(&mut entropy)
        .map_err(|_| anyhow::anyhow!("Federation nonce randomness unavailable"))?;
    let manifest = Manifest {
        schema: 1,
        scope: configuration.scope.clone(),
        sender_workspace_id: store.workspace_id.clone(),
        source_snapshot_id: snapshot.id.clone(),
        source_kind: snapshot.kind.clone(),
        analyzer: ANALYZER.into(),
        nonce: hex::encode(entropy),
        issued_at_ms,
        expires_at_ms: issued_at_ms
            .checked_add(configuration.ttl_ms)
            .context("Federation expiry overflow")?,
        files,
        omissions: snapshot
            .omissions
            .iter()
            .map(|o| SignedOmission {
                path: o.path.clone(),
                reason: o.reason.clone(),
            })
            .collect(),
    };
    manifest_valid(&manifest)?;
    let mut contents = Vec::with_capacity(manifest.files.len());
    for file in &manifest.files {
        deadline.check()?;
        let bytes = store.read_blob(&file.content_hash)?;
        ensure!(
            bytes.len() == file.size,
            "Export source size differs from manifest"
        );
        let text = std::str::from_utf8(&bytes)?;
        ensure!(
            !text.contains('\0') && !obvious_secret(text),
            "Binary or obvious secret source cannot be exported"
        );
        contents.push(Content {
            path: file.path.clone(),
            base64: STANDARD.encode(bytes),
        });
    }
    // The caller owns the seed buffer; only the dalek SigningKey holds a local
    // copy, zeroized on drop by its default feature. No key is serialized.
    let key = SigningKey::from_bytes(private_key);
    let signature = hex::encode(key.sign(&signing_message(&manifest)?).to_bytes());
    let result = serde_json::to_value(Envelope {
        manifest,
        signature,
        contents,
    })?;
    ensure!(
        serde_json::to_vec(&result)?.len() <= MAX_ENVELOPE,
        "Federation envelope exceeds capacity"
    );
    deadline.check()?;
    Ok(result)
}

struct ReceiverPolicy {
    root: PathBuf,
    checked: BTreeSet<PathBuf>,
    matchers: Vec<ignore::IncrementalIgnore>,
    policy_bytes: usize,
    host: Gitignore,
}
impl ReceiverPolicy {
    fn new(engine: &Engine, grant: &ImportGrant, deadline: &Deadline) -> Result<Self> {
        ensure!(
            grant.exclude.len() <= 1000,
            "Receiver exclude capacity exceeded"
        );
        let mut host = GitignoreBuilder::new(&engine.root);
        host.case_insensitive(true)?;
        for pattern in &grant.exclude {
            data_label(pattern, 1024)?;
            ensure!(
                !pattern.is_empty() && !pattern.starts_with('!'),
                "Receiver exclude patterns cannot re-enable denied paths"
            );
            host.add_line(None, pattern)?;
        }
        let mut value = Self {
            root: engine.root.clone(),
            checked: BTreeSet::new(),
            matchers: Vec::new(),
            policy_bytes: 0,
            host: host.build()?,
        };
        let mut ancestors: Vec<_> = engine.root.ancestors().collect();
        ancestors.reverse();
        for directory in ancestors {
            value.load(directory, deadline)?;
        }
        value.matchers = ignore::WalkBuilder::new(&engine.root)
            .hidden(false)
            .follow_links(false)
            .git_ignore(true)
            .git_global(false)
            .git_exclude(true)
            .require_git(false)
            .build_matchers();
        ensure!(
            value.matchers.len() == 1,
            "Receiver ignore matcher is unavailable"
        );
        Ok(value)
    }
    fn load(&mut self, directory: &Path, deadline: &Deadline) -> Result<()> {
        if self.checked.contains(directory) {
            return Ok(());
        }
        ensure!(
            self.checked.len() < 1000,
            "Receiver ignore-policy capacity exceeded"
        );
        let mut files = Vec::new();
        let git = directory.join(".git");
        if let Ok(meta) = fs::symlink_metadata(&git) {
            ensure!(
                meta.is_dir() && !is_link(&meta),
                "External or linked Git policies are unsupported for federation"
            );
            let info = git.join("info");
            if let Ok(meta) = fs::symlink_metadata(&info) {
                ensure!(
                    meta.is_dir() && !is_link(&meta),
                    "Linked Git exclude policy directory refused"
                );
            }
            files.push(info.join("exclude"));
        }
        files.extend([directory.join(".gitignore"), directory.join(".ignore")]);
        let mut count = 0usize;
        for policy in files {
            deadline.check()?;
            match fs::symlink_metadata(&policy) {
                Ok(meta) => {
                    ensure!(
                        meta.is_file() && !is_link(&meta) && meta.len() <= 1_000_000,
                        "Unsupported or linked receiver ignore policy"
                    );
                    let text = fs::read_to_string(&policy)?;
                    ensure!(
                        text.len() <= 1_000_000,
                        "Receiver ignore policy grew past capacity"
                    );
                    self.policy_bytes = self
                        .policy_bytes
                        .checked_add(text.len())
                        .context("Receiver policy size overflow")?;
                    ensure!(
                        self.policy_bytes <= 8_000_000,
                        "Receiver policy byte capacity exceeded"
                    );
                    for line in text.lines() {
                        deadline.check()?;
                        count += 1;
                        ensure!(
                            count <= 10_000 && line.len() <= 4096,
                            "Receiver ignore rule capacity exceeded"
                        );
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        self.checked.insert(directory.to_path_buf());
        Ok(())
    }
    fn check(&mut self, path: &str, deadline: &Deadline) -> Result<()> {
        let absolute = scoped(&self.root, path)?;
        ensure!(
            !self
                .host
                .matched_path_or_any_parents(&absolute, false)
                .is_ignore(),
            "Federated source is excluded by host grant"
        );
        let relative = Path::new(path);
        let mut current = self.root.clone();
        let parents: Vec<_> = relative.components().collect();
        for part in parents.iter().take(parents.len().saturating_sub(1)) {
            deadline.check()?;
            current.push(part.as_os_str());
            self.load(&current, deadline)?;
        }
        // The incrementally loaded walker matcher composes ancestor/root/nested
        // rule priorities for virtual paths. Remote .gitignore text never enters it.
        let (matched, error) = self.matchers[0].matched_with_errors(path, false);
        if let Some(error) = error {
            return Err(error.into());
        }
        ensure!(
            !matched.is_ignore(),
            "Federated source is ignored by receiver policy"
        );
        Ok(())
    }
}

fn decoded(
    envelope: &Envelope,
    policy: &mut ReceiverPolicy,
    deadline: &Deadline,
) -> Result<Vec<Vec<u8>>> {
    manifest_valid(&envelope.manifest)?;
    ensure!(
        envelope.contents.len() == envelope.manifest.files.len(),
        "Federation contents must cover every manifest file exactly once"
    );
    let mut output = Vec::with_capacity(envelope.contents.len());
    let mut total = 0usize;
    for (file, content) in envelope.manifest.files.iter().zip(&envelope.contents) {
        deadline.check()?;
        ensure!(
            content.path == file.path && content.base64.len() <= MAX_FILE.div_ceil(3) * 4,
            "Federation content path/order/size differs"
        );
        policy.check(&file.path, deadline)?;
        let bytes = STANDARD.decode(&content.base64)?;
        ensure!(
            STANDARD.encode(&bytes) == content.base64
                && bytes.len() == file.size
                && hash(&bytes) == file.content_hash,
            "Federation source bytes do not match signed hash or size"
        );
        let text = std::str::from_utf8(&bytes)?;
        ensure!(
            !text.contains('\0') && !obvious_secret(text),
            "Binary or obvious secret source refused by receiving policy"
        );
        total = total
            .checked_add(bytes.len())
            .context("Federation content size overflow")?;
        ensure!(
            total <= MAX_SOURCE,
            "Federation decoded source capacity exceeded"
        );
        output.push(bytes);
    }
    Ok(output)
}
fn assemble_federated_at(
    epoch: analysis::AnalyzerEpoch,
    snapshot: &Snapshot,
    extractions: Vec<crate::model::Extraction>,
    deadline: &Deadline,
) -> Result<Graph> {
    let mut graph = analysis::assemble_at(epoch, snapshot, extractions, deadline)?;
    graph.coverage.exhaustive = false;
    graph
        .coverage
        .limitations
        .push(SIGNED_SOURCE_LIMITATION.into());
    ensure!(
        graph.entities.len() <= 50_000
            && graph.observations.len() <= 100_000
            && graph.evidence.len() <= 200_000
            && graph.diagnostics.len() <= 50_000,
        "Federated graph capacity exceeded"
    );
    deadline.check()?;
    Ok(graph)
}
pub fn import_snapshot_at(
    epoch: analysis::AnalyzerEpoch,
    engine: &Engine,
    input: Value,
    grant: &ImportGrant,
    deadline: &Deadline,
) -> Result<Value> {
    engine.store.set_deadline(deadline)?;
    ensure!(
        serde_json::to_vec(&input)?.len() <= MAX_ENVELOPE,
        "Federation envelope exceeds capacity"
    );
    let envelope: Envelope = serde_json::from_value(input)?;
    verify(&envelope.manifest, &envelope.signature, grant, true)?;
    let mut policy = ReceiverPolicy::new(engine, grant, deadline)?;
    let contents = decoded(&envelope, &mut policy, deadline)?;
    let manifest_id = hash(canonical(&envelope.manifest)?);
    let remote_id = match epoch {
        analysis::AnalyzerEpoch::V8 => {
            remote_id(&engine.store.workspace_id, &envelope.manifest, &manifest_id)
        }
        analysis::AnalyzerEpoch::V9 => remote_id_v2(
            &engine.store.workspace_id,
            &envelope.manifest,
            &manifest_id,
            epoch,
        ),
    };
    let replay_id = replay_id(&envelope.manifest);
    let prior: Option<String> = engine
        .store
        .db
        .query_row(
            "SELECT value FROM records WHERE kind=?1 AND id=?2",
            params![REPLAY, replay_id],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(prior) = prior {
        let prior: Replay = serde_json::from_str(&prior)?;
        ensure!(
            prior.manifest_id == manifest_id && valid_id(&prior.remote_id),
            "Federation nonce was reused for a different signed manifest"
        );
        let retained = receipt(engine, &prior.remote_id, grant, deadline)?;
        ensure!(
            retained.manifest_id == manifest_id,
            "Replay receipt manifest mismatch"
        );
        return Ok(
            json!({"remoteId":prior.remote_id,"manifestId":manifest_id,"idempotent":true,"sourceWritten":false,"scope":grant.scope,"namespace":"federated"}),
        );
    }
    let snapshot = Snapshot {
        id: remote_id.clone(),
        workspace_id: engine.store.workspace_id.clone(),
        kind: "federated".into(),
        files: envelope
            .manifest
            .files
            .iter()
            .map(|file| SourceFile {
                path: file.path.clone(),
                hash: file.content_hash.clone(),
                size: file.size,
                mode: file.mode,
                language: crate::capture::language_at(epoch, &file.path).into(),
            })
            .collect(),
        omissions: envelope
            .manifest
            .omissions
            .iter()
            .map(|o| Omission {
                path: o.path.clone(),
                reason: o.reason.clone(),
            })
            .collect(),
        capture_consistency: "verified-signed-source-envelope".into(),
    };
    let mut extractions = Vec::with_capacity(contents.len());
    for (file, bytes) in snapshot.files.iter().zip(&contents) {
        deadline.check()?;
        extractions.push(analysis::extract_at(
            epoch,
            file,
            std::str::from_utf8(bytes)?,
            deadline,
        )?);
    }
    let graph = assemble_federated_at(epoch, &snapshot, extractions, deadline)?;
    let graph_raw = serde_json::to_string(&graph)?;
    let receipt = Receipt {
        remote_id: remote_id.clone(),
        receiver_workspace_id: engine.store.workspace_id.clone(),
        manifest_id: manifest_id.clone(),
        public_key: grant.public_key.clone(),
        manifest: envelope.manifest,
        signature: envelope.signature,
        graph_hash: hash(graph_raw.as_bytes()),
        snapshot,
    };
    let receipt = match epoch {
        analysis::AnalyzerEpoch::V8 => SelectedReceipt::V1(receipt),
        analysis::AnalyzerEpoch::V9 => SelectedReceipt::V2(ReceiptV2 {
            schema_version: 2,
            receiver_analyzer: epoch.producer().into(),
            receipt,
        }),
    };
    let receipt_raw = serde_json::to_string(&receipt)?;
    ensure!(
        receipt_raw.len() <= MAX_STATE && graph_raw.len() <= MAX_STATE,
        "Federation persisted state capacity exceeded"
    );
    // Parsing and serialization consumed part of the absolute request budget.
    // Refresh SQLite's lock wait before admitting the atomic import transaction.
    engine.store.set_deadline(deadline)?;
    let tx =
        rusqlite::Transaction::new_unchecked(&engine.store.db, TransactionBehavior::Immediate)?;
    let replay: Option<String> = tx
        .query_row(
            "SELECT value FROM records WHERE kind=?1 AND id=?2",
            params![REPLAY, replay_id],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(replay) = replay {
        let replay: Replay = serde_json::from_str(&replay)?;
        ensure!(
            replay.manifest_id == manifest_id && valid_id(&replay.remote_id),
            "Concurrent federation nonce conflict"
        );
        let retained = self::receipt(engine, &replay.remote_id, grant, deadline)?;
        ensure!(
            retained.manifest_id == manifest_id,
            "Concurrent replay receipt manifest mismatch"
        );
        return Ok(
            json!({"remoteId":replay.remote_id,"manifestId":manifest_id,"idempotent":true,"sourceWritten":false,"scope":grant.scope,"namespace":"federated"}),
        );
    }
    let count: usize = tx.query_row(
        "SELECT count(*) FROM records WHERE kind IN (?1,?2)",
        params![RECEIPT, RECEIPT_V2],
        |row| row.get(0),
    )?;
    let source_bytes:usize=tx.query_row("SELECT coalesce(sum(json_extract(file.value,'$.size')),0) FROM records AS record JOIN json_each(CASE WHEN record.kind=?2 THEN json_extract(record.value,'$.receipt.snapshot.files') ELSE json_extract(record.value,'$.snapshot.files') END) AS file WHERE record.kind IN (?1,?2)",params![RECEIPT,RECEIPT_V2],|row|row.get(0))?;
    let stored_bytes: usize = tx.query_row(
        "SELECT coalesce(sum(length(CAST(value AS BLOB))),0) FROM records WHERE kind IN (?1,?2,?3)",
        params![RECEIPT, REMOTE_GRAPH, RECEIPT_V2],
        |row| row.get(0),
    )?;
    let decoded_size: usize = contents.iter().map(Vec::len).sum();
    ensure!(
        count < 256
            && source_bytes.saturating_add(decoded_size) <= 512_000_000
            && stored_bytes
                .saturating_add(receipt_raw.len())
                .saturating_add(graph_raw.len())
                <= 512_000_000,
        "Federation namespace capacity reached"
    );
    for bytes in &contents {
        deadline.check()?;
        engine.store.blob(bytes)?;
    }
    tx.execute(
        "INSERT INTO records VALUES(?1,?2,?3)",
        params![receipt.kind(), remote_id, receipt_raw],
    )?;
    tx.execute(
        "INSERT INTO records VALUES(?1,?2,?3)",
        params![REMOTE_GRAPH, remote_id, graph_raw],
    )?;
    tx.execute(
        "INSERT INTO records VALUES(?1,?2,?3)",
        params![
            REPLAY,
            replay_id,
            serde_json::to_string(&Replay {
                manifest_id: manifest_id.clone(),
                remote_id: remote_id.clone()
            })?
        ],
    )?;
    for entity in &graph.entities {
        deadline.check()?;
        tx.execute(
            "INSERT INTO symbols VALUES(?1,?2,?3,?4,?5)",
            params![
                remote_id,
                entity.id,
                crate::query::identifier_terms(&format!(
                    "{} {}",
                    entity.name, entity.qualified_name
                )),
                entity.path,
                entity.signature
            ],
        )?;
    }
    for (file, bytes) in receipt.snapshot.files.iter().zip(&contents) {
        deadline.check()?;
        tx.execute(
            "INSERT INTO documents VALUES(?1,?2,?3)",
            params![remote_id, file.path, std::str::from_utf8(bytes)?],
        )?;
    }
    engine.store.set_deadline(deadline)?;
    tx.commit()?;
    Ok(
        json!({"remoteId":remote_id,"manifestId":manifest_id,"idempotent":false,"sourceWritten":false,"scope":grant.scope,"namespace":"federated","files":contents.len(),"trust":"signed_source_not_verified_complete_or_safe"}),
    )
}
pub fn import_snapshot(
    engine: &Engine,
    input: Value,
    grant: &ImportGrant,
    deadline: &Deadline,
) -> Result<Value> {
    import_snapshot_at(
        analysis::AnalyzerEpoch::active()?,
        engine,
        input,
        grant,
        deadline,
    )
}
fn receipt(
    engine: &Engine,
    id: &str,
    grant: &ImportGrant,
    deadline: &Deadline,
) -> Result<SelectedReceipt> {
    engine.store.set_deadline(deadline)?;
    ensure!(valid_id(id), "Invalid federated source ID");
    let (v1,v2): (bool,bool) = engine.store.db.query_row(
        "SELECT EXISTS(SELECT 1 FROM records WHERE kind=?1 AND id=?3),EXISTS(SELECT 1 FROM records WHERE kind=?2 AND id=?3)",
        params![RECEIPT,RECEIPT_V2,id],|row|Ok((row.get(0)?,row.get(1)?)))?;
    ensure!(v1 ^ v2, "Missing or conflicting federated receipt versions");
    let kind = if v1 { RECEIPT } else { RECEIPT_V2 };
    let value = archive_record(&engine.store, kind, id, deadline)?;
    let receipt = bounded_receipt(&value, deadline)?;
    ensure!(
        receipt.kind() == kind,
        "Federated receipt schema/kind mismatch"
    );
    validate_receipt(
        engine,
        id,
        &receipt,
        grant,
        false,
        &mut |file, _| {
            engine
                .store
                .read_blob_exact(&file.hash, file.size, MAX_FILE)
        },
        deadline,
    )?;
    Ok(receipt)
}

fn archive_record(store: &Store, kind: &str, id: &str, deadline: &Deadline) -> Result<Value> {
    crate::json::parse(
        archive_record_raw(store, kind, id, deadline)?.as_bytes(),
        MAX_STATE,
    )
}

fn archive_record_raw(store: &Store, kind: &str, id: &str, deadline: &Deadline) -> Result<String> {
    store.set_deadline(deadline)?;
    let limit = rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH;
    let prior = store
        .db
        .set_limit(limit, store.db.limit(limit)?.min((MAX_STATE + 4096) as i32))?;
    let result =
        (|| -> Result<String> {
            let (size, value_type): (usize, String) = store.db.query_row(
            "SELECT length(CAST(value AS BLOB)),typeof(value) FROM records WHERE kind=?1 AND id=?2",
            params![kind,id], |row| Ok((row.get(0)?, row.get(1)?)))?;
            ensure!(
                size <= MAX_STATE && value_type == "text",
                "Federated record exceeds capacity or has invalid storage type"
            );
            let raw: String = store.db.query_row(
                "SELECT value FROM records WHERE kind=?1 AND id=?2",
                params![kind, id],
                |row| row.get(0),
            )?;
            ensure!(raw.len() == size, "Federated record changed during read");
            deadline.check()?;
            Ok(raw)
        })();
    store.db.set_limit(limit, prior)?;
    result
}

fn released4_language(path: &str) -> &'static str {
    match path
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "rs" => "rust",
        "ts" | "mts" | "cts" => "typescript",
        "tsx" => "tsx",
        "js" | "mjs" | "cjs" => "javascript",
        "jsx" => "jsx",
        "py" | "pyi" => "python",
        "go" => "go",
        "c" | "h" => "c",
        "cpp" | "cc" | "cxx" | "hpp" => "cpp",
        "java" => "java",
        "cs" => "csharp",
        "rb" => "ruby",
        "php" => "php",
        "swift" => "swift",
        "scala" | "sc" => "scala",
        "lua" => "lua",
        "sh" | "bash" => "bash",
        "ex" | "exs" => "elixir",
        "sol" => "solidity",
        "ml" => "ocaml",
        "mli" => "ocaml-interface",
        "zig" => "zig",
        "nix" => "nix",
        "r" => "r",
        "kt" | "kts" => "kotlin",
        "dart" => "dart",
        "clj" | "cljs" | "cljc" => "clojure",
        "md" | "mdx" | "rst" => "markdown",
        "sql" => "sql",
        "json" | "yaml" | "yml" | "toml" | "xml" => "config",
        _ => "text",
    }
}

fn receiver_classifier(snapshot: &Snapshot, epoch: analysis::AnalyzerEpoch) -> &'static str {
    if snapshot
        .files
        .iter()
        .all(|file| file.language == crate::capture::language_at(epoch, &file.path))
    {
        "compatible_current"
    } else if snapshot
        .files
        .iter()
        .all(|file| file.language == released4_language(&file.path))
    {
        "compatible_released4"
    } else {
        "unknown_preserved"
    }
}

fn bounded_serialized_hash(value: &impl Serialize, deadline: &Deadline) -> Result<String> {
    use sha2::Digest;
    struct Counter<'a> {
        size: usize,
        deadline: &'a Deadline,
        hasher: sha2::Sha256,
    }
    impl std::io::Write for Counter<'_> {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.deadline.check().map_err(std::io::Error::other)?;
            if bytes.len() > MAX_STATE.saturating_sub(self.size) {
                return Err(std::io::Error::other("Federated receipt exceeds capacity"));
            }
            self.size += bytes.len();
            self.hasher.update(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut counter = Counter {
        size: 0,
        deadline,
        hasher: sha2::Sha256::new(),
    };
    serde_json::to_writer(&mut counter, value)?;
    Ok(hex::encode(counter.hasher.finalize()))
}

fn bounded_receipt(value: &Value, deadline: &Deadline) -> Result<SelectedReceipt> {
    let core = if value.get("schemaVersion").is_some() {
        ensure!(
            value["schemaVersion"] == 2,
            "Unknown federation receipt schema"
        );
        ensure!(
            value["receiverAnalyzer"] == "yoke-tree-sitter-9",
            "Unsupported V2 receiver analyzer"
        );
        value.get("receipt").context("V2 receipt core missing")?
    } else {
        value
    };
    for owner in ["manifest", "snapshot"] {
        let files = core[owner]["files"]
            .as_array()
            .context("Invalid federated receipt files")?;
        let omissions = core[owner]["omissions"]
            .as_array()
            .context("Invalid federated receipt omissions")?;
        ensure!(
            files.len() <= MAX_FILES && omissions.len() <= MAX_FILES,
            "Federated receipt count exceeds capacity"
        );
        for file in files {
            deadline.check()?;
            ensure!(
                file["language"]
                    .as_str()
                    .is_some_and(|value| value.len() <= 100),
                "Invalid federated receipt language label"
            );
        }
    }
    bounded_serialized_hash(value, deadline)?;
    let receipt = if value.get("schemaVersion").is_some() {
        SelectedReceipt::V2(serde_json::from_value(value.clone())?)
    } else {
        SelectedReceipt::V1(serde_json::from_value(value.clone())?)
    };
    receipt.epoch()?;
    ensure!(
        serde_json::to_value(&receipt)? == *value,
        "Unknown federated receipt fields"
    );
    Ok(receipt)
}

pub(crate) fn archive_receipt_provenance(value: &Value, deadline: &Deadline) -> Result<Value> {
    let receipt = bounded_receipt(value, deadline)?;
    Ok(
        json!({"remoteId":receipt.remote_id,"receiverWorkspaceId":receipt.receiver_workspace_id,
        "scope":receipt.manifest.scope,"senderWorkspaceId":receipt.manifest.sender_workspace_id,
        "sourceSnapshotId":receipt.manifest.source_snapshot_id,"manifestId":receipt.manifest_id,
        "receiverEpoch":match &receipt { SelectedReceipt::V1(_) => "not_recorded", SelectedReceipt::V2(_) =>receipt.epoch()?.producer()},
        "receiverClassifier":match &receipt { SelectedReceipt::V1(_) =>receipt.classifier()?,SelectedReceipt::V2(_) => {receipt.classifier()?;"compatible_declared_epoch"}},
        "trust":"signed_source_not_execution_authority"}),
    )
}
pub(crate) fn archive_receipt_snapshot(value: &Value, deadline: &Deadline) -> Result<Snapshot> {
    Ok(bounded_receipt(value, deadline)?.into_snapshot())
}

/// Preservation proof only. The receipt's public signing key is verified as
/// data; this function constructs no host grant and authorizes no source query.
pub(crate) fn validate_retention_receipt(
    store: &Store,
    kind: &str,
    id: &str,
    value: &Value,
    deadline: &Deadline,
) -> Result<Vec<String>> {
    store.set_deadline(deadline)?;
    ensure!(
        [RECEIPT, RECEIPT_V2].contains(&kind) && valid_id(id),
        "Unknown retained federation receipt kind/ID"
    );
    let selected = bounded_receipt(value, deadline)?;
    ensure!(
        selected.kind() == kind
            && selected.receiver_workspace_id == store.workspace_id
            && selected.remote_id == id
            && selected.expected_remote_id(&store.workspace_id)? == id
            && selected.manifest_id == hash(canonical(&selected.manifest)?)
            && valid_id(&selected.graph_hash),
        "Retained V2 receipt identity mismatch"
    );
    let conflict: bool = store.db.query_row(
        "SELECT EXISTS(SELECT 1 FROM records WHERE kind=?1 AND id=?2)",
        params![
            if kind == RECEIPT_V2 {
                RECEIPT
            } else {
                RECEIPT_V2
            },
            id
        ],
        |row| row.get(0),
    )?;
    ensure!(
        !conflict,
        "Conflicting retained federation receipt versions"
    );
    let manifest = &selected.manifest;
    scope_valid(&manifest.scope)?;
    ensure!(
        manifest.schema == 1
            && valid_id(&manifest.sender_workspace_id)
            && valid_id(&manifest.source_snapshot_id)
            && valid_id(&manifest.nonce)
            && ["disk", "derived", "editor", "overlay"].contains(&manifest.source_kind.as_str())
            && manifest.expires_at_ms > manifest.issued_at_ms
            && manifest.expires_at_ms - manifest.issued_at_ms <= MAX_TTL,
        "Invalid retained signed-source manifest header"
    );
    data_label(&manifest.analyzer, 200)?;
    manifest_valid(manifest)?;
    verify_signature(manifest, &selected.signature, &selected.public_key)?;
    let epoch = selected.epoch()?;
    let languages: std::collections::BTreeMap<_, _> = selected
        .snapshot
        .files
        .iter()
        .map(|file| (file.path.as_str(), file.language.as_str()))
        .collect();
    ensure!(
        languages.len() == selected.snapshot.files.len(),
        "Duplicate retained source paths"
    );
    let expected = Snapshot {
        id: id.into(),
        workspace_id: store.workspace_id.clone(),
        kind: "federated".into(),
        files: manifest
            .files
            .iter()
            .map(|file| SourceFile {
                path: file.path.clone(),
                hash: file.content_hash.clone(),
                size: file.size,
                mode: file.mode,
                language: if kind == RECEIPT_V2 {
                    crate::capture::language_at(epoch, &file.path)
                } else {
                    languages.get(file.path.as_str()).copied().unwrap_or("")
                }
                .into(),
            })
            .collect(),
        omissions: manifest
            .omissions
            .iter()
            .map(|row| Omission {
                path: row.path.clone(),
                reason: row.reason.clone(),
            })
            .collect(),
        capture_consistency: "verified-signed-source-envelope".into(),
    };
    ensure!(
        identity(&expected) == identity(&selected.snapshot)
            && selected.classifier()? != "unknown_preserved",
        "Retained V2 signed-source membership/classification mismatch"
    );
    {
        let raw = archive_record_raw(store, REMOTE_GRAPH, id, deadline)?;
        ensure!(
            hash(raw.as_bytes()) == selected.graph_hash,
            "Retained V2 inline graph differs from its receipt"
        );
    }
    let mut extractions = Vec::with_capacity(expected.files.len());
    let mut references = BTreeSet::new();
    let mut entities = 0usize;
    let mut edges = 0usize;
    let mut diagnostics = 0usize;
    for file in &expected.files {
        deadline.check()?;
        let bytes = store.read_blob_exact(&file.hash, file.size, MAX_FILE)?;
        let text = std::str::from_utf8(&bytes)?;
        ensure!(
            !text.contains('\0') && !obvious_secret(text),
            "Retained V2 source violates source-data policy"
        );
        let extraction = analysis::extract_at(epoch, file, text, deadline)?;
        entities = entities
            .checked_add(extraction.entities.len())
            .context("Retained V2 entity count overflow")?;
        edges = edges
            .checked_add(extraction.edges.len())
            .context("Retained V2 edge count overflow")?;
        diagnostics = diagnostics
            .checked_add(extraction.diagnostics.len())
            .context("Retained V2 diagnostic count overflow")?;
        ensure!(
            entities <= 50_000 && edges <= 100_000 && diagnostics <= 50_000,
            "Retained V2 extraction capacity exceeded"
        );
        extractions.push(extraction);
        references.insert(file.hash.clone());
    }
    let graph = assemble_federated_at(epoch, &expected, extractions, deadline)?;
    ensure!(
        bounded_serialized_hash(&graph, deadline)? == selected.graph_hash,
        "Retained V2 graph differs from exact declared-epoch source extraction"
    );
    deadline.check()?;
    Ok(references.into_iter().collect())
}

/// Captures a signed receipt only after the current receiving host grant and
/// policy have been checked. The returned data does not carry an authorization.
pub(crate) fn archive_receipt(
    engine: &Engine,
    id: &str,
    grant: &ImportGrant,
    deadline: &Deadline,
) -> Result<Value> {
    let receipt = receipt(engine, id, grant, deadline)?;
    ensure!(
        receipt.classifier()? != "unknown_preserved",
        "Federated archive receiver classifier is unknown"
    );
    Ok(serde_json::to_value(receipt)?)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn validate_archive_receipt(
    receiver: &Engine,
    value: &Value,
    snapshot: &Snapshot,
    graph: &Graph,
    grant: &ImportGrant,
    read_source: &mut dyn FnMut(&SourceFile, &Deadline) -> Result<Vec<u8>>,
    deadline: &Deadline,
) -> Result<()> {
    let receipt = archive_snapshot(value, snapshot, deadline)?;
    validate_selected_archive_receipt(
        receiver,
        &receipt,
        snapshot,
        graph,
        grant,
        read_source,
        deadline,
    )
}
fn validate_selected_archive_receipt(
    receiver: &Engine,
    receipt: &SelectedReceipt,
    snapshot: &Snapshot,
    graph: &Graph,
    grant: &ImportGrant,
    read_source: &mut dyn FnMut(&SourceFile, &Deadline) -> Result<Vec<u8>>,
    deadline: &Deadline,
) -> Result<()> {
    ensure!(
        receipt.classifier()? == "compatible_current",
        "Historical receiver graph extraction contract is unavailable; use signed source-only archive access"
    );
    ensure!(
        graph.snapshot_id == snapshot.id
            && graph.entities.len() <= 50_000
            && graph.observations.len() <= 100_000
            && graph.evidence.len() <= 200_000
            && graph.diagnostics.len() <= 50_000,
        "Invalid archived federated graph capacity or scope"
    );
    ensure!(
        valid_id(&receipt.graph_hash)
            && bounded_serialized_hash(graph, deadline)? == receipt.graph_hash,
        "Archived federated graph differs from original receipt"
    );
    let entities: std::collections::BTreeMap<_, _> = graph
        .entities
        .iter()
        .map(|entity| (&entity.id, entity))
        .collect();
    let evidence: std::collections::BTreeMap<_, _> =
        graph.evidence.iter().map(|item| (&item.id, item)).collect();
    let files: std::collections::BTreeMap<_, _> = snapshot
        .files
        .iter()
        .map(|file| (&file.path, file))
        .collect();
    let mut entity_paths = std::collections::BTreeMap::<_, Vec<_>>::new();
    let mut evidence_paths = std::collections::BTreeMap::<_, Vec<_>>::new();
    ensure!(
        entities.len() == graph.entities.len() && evidence.len() == graph.evidence.len(),
        "Duplicate federated graph IDs"
    );
    for entity in &graph.entities {
        deadline.check()?;
        ensure!(
            valid_id(&entity.id)
                && entity.snapshot_id == snapshot.id
                && files
                    .get(&entity.path)
                    .is_some_and(|file| file.language == entity.language),
            "Foreign federated graph entity"
        );
        entity_paths.entry(&entity.path).or_default().push(entity);
    }
    for item in &graph.evidence {
        deadline.check()?;
        ensure!(
            valid_id(&item.id)
                && item.snapshot_id == snapshot.id
                && files
                    .get(&item.path)
                    .is_some_and(|file| file.hash == item.content_hash),
            "Foreign federated graph evidence"
        );
        evidence_paths.entry(&item.path).or_default().push(item);
    }
    let mut observation_ids = BTreeSet::new();
    for observation in &graph.observations {
        deadline.check()?;
        ensure!(
            valid_id(&observation.id)
                && observation_ids.insert(&observation.id)
                && observation.snapshot_id == snapshot.id
                && entities.contains_key(&observation.source)
                && observation
                    .target
                    .as_ref()
                    .is_none_or(|target| entities.contains_key(target))
                && observation
                    .evidence_ids
                    .iter()
                    .all(|id| evidence.contains_key(id)),
            "Foreign federated graph observation"
        );
    }
    for diagnostic in &graph.diagnostics {
        deadline.check()?;
        ensure!(
            files.contains_key(&diagnostic.path),
            "Foreign federated graph diagnostic"
        );
    }
    let mut extractions = Vec::with_capacity(snapshot.files.len());
    let mut checked_read = |file: &SourceFile, deadline: &Deadline| -> Result<Vec<u8>> {
        let bytes = read_source(file, deadline)?;
        ensure!(
            bytes.len() == file.size && hash(&bytes) == file.hash,
            "Archived source content mismatch"
        );
        let text = std::str::from_utf8(&bytes)?;
        ensure!(
            !text.contains('\0') && !obvious_secret(text),
            "Archived source violates receiving policy"
        );
        let mut lines = vec![0usize];
        for (index, byte) in bytes.iter().enumerate() {
            if index % 65_536 == 0 {
                deadline.check()?;
            }
            if *byte == b'\n' {
                lines.push(index + 1);
            }
        }
        let range = |start: usize, end: usize| {
            start <= end
                && end <= text.len()
                && text.is_char_boundary(start)
                && text.is_char_boundary(end)
        };
        let line_range = |start: usize, end: usize, start_line: usize, end_line: usize| {
            start_line == lines.partition_point(|offset| *offset <= start)
                && (end_line == lines.partition_point(|offset| *offset <= end.saturating_sub(1))
                    || end_line == lines.partition_point(|offset| *offset <= end))
        };
        for entity in entity_paths.get(&file.path).into_iter().flatten() {
            deadline.check()?;
            ensure!(
                range(entity.start_byte, entity.end_byte)
                    && entity.name_start_byte.zip(entity.name_end_byte).is_none_or(
                        |(start, end)| range(start, end)
                            && start >= entity.start_byte
                            && end <= entity.end_byte
                    )
                    && entity.name_start_byte.is_some() == entity.name_end_byte.is_some()
                    && line_range(
                        entity.start_byte,
                        entity.end_byte,
                        entity.start_line,
                        entity.end_line
                    ),
                "Invalid archived entity source range"
            );
        }
        for item in evidence_paths.get(&file.path).into_iter().flatten() {
            deadline.check()?;
            ensure!(
                range(item.start_byte, item.end_byte)
                    && line_range(
                        item.start_byte,
                        item.end_byte,
                        item.start_line,
                        item.end_line
                    ),
                "Invalid archived evidence source range"
            );
        }
        for diagnostic in graph
            .diagnostics
            .iter()
            .filter(|item| item.path == file.path)
        {
            deadline.check()?;
            ensure!(
                range(diagnostic.start_byte, diagnostic.start_byte),
                "Invalid archived diagnostic source range"
            );
        }
        extractions.push(analysis::extract_at(
            receipt.epoch()?,
            file,
            text,
            deadline,
        )?);
        Ok(bytes)
    };
    validate_receipt(
        receiver,
        &snapshot.id,
        receipt,
        grant,
        true,
        &mut checked_read,
        deadline,
    )?;
    let reconstructed = assemble_federated_at(receipt.epoch()?, snapshot, extractions, deadline)?;
    ensure!(
        bounded_serialized_hash(&reconstructed, deadline)? == receipt.graph_hash,
        "Archived graph facts differ from exact current source extraction; historical original is archival data only"
    );
    Ok(())
}

fn archive_snapshot(
    value: &Value,
    snapshot: &Snapshot,
    deadline: &Deadline,
) -> Result<SelectedReceipt> {
    ensure!(
        snapshot.files.len() <= MAX_FILES && snapshot.omissions.len() <= MAX_FILES,
        "Archived federated snapshot count exceeds capacity"
    );
    let snapshot_hash = bounded_serialized_hash(snapshot, deadline)?;
    let receipt = bounded_receipt(value, deadline)?;
    ensure!(
        snapshot_hash == bounded_serialized_hash(&receipt.snapshot, deadline)?,
        "Archived federated snapshot differs from receipt"
    );
    Ok(receipt)
}

pub(crate) fn validate_archive_sources(
    receiver: &Engine,
    value: &Value,
    snapshot: &Snapshot,
    grant: &ImportGrant,
    read_source: &mut dyn FnMut(&SourceFile, &Deadline) -> Result<Vec<u8>>,
    deadline: &Deadline,
) -> Result<()> {
    let receipt = archive_snapshot(value, snapshot, deadline)?;
    validate_receipt(
        receiver,
        &snapshot.id,
        &receipt,
        grant,
        true,
        read_source,
        deadline,
    )
}

#[allow(clippy::too_many_arguments)]
fn validate_receipt(
    engine: &Engine,
    id: &str,
    receipt: &SelectedReceipt,
    grant: &ImportGrant,
    archive: bool,
    read_source: &mut dyn FnMut(&SourceFile, &Deadline) -> Result<Vec<u8>>,
    deadline: &Deadline,
) -> Result<()> {
    deadline.check()?;
    ensure!(
        receipt.receiver_workspace_id == engine.store.workspace_id
            && receipt.remote_id == id
            && receipt.public_key == grant.public_key,
        "Federation receipt workspace or current trusted key mismatch"
    );
    verify(&receipt.manifest, &receipt.signature, grant, false)?;
    manifest_valid(&receipt.manifest)?;
    ensure!(
        hash(canonical(&receipt.manifest)?) == receipt.manifest_id
            && receipt.expected_remote_id(&engine.store.workspace_id)? == id,
        "Corrupt federated receipt identity"
    );
    let expected = Snapshot {
        id: id.into(),
        workspace_id: engine.store.workspace_id.clone(),
        kind: "federated".into(),
        files: receipt
            .manifest
            .files
            .iter()
            .map(|file| SourceFile {
                path: file.path.clone(),
                hash: file.content_hash.clone(),
                size: file.size,
                mode: file.mode,
                // V1 never recorded the receiving analyzer. Sender analyzer is
                // signed sender metadata, not proof of receiver classification.
                language: receipt
                    .snapshot
                    .files
                    .iter()
                    .find(|stored| stored.path == file.path)
                    .map(|stored| stored.language.clone())
                    .unwrap_or_default(),
            })
            .collect(),
        omissions: receipt
            .manifest
            .omissions
            .iter()
            .map(|o| Omission {
                path: o.path.clone(),
                reason: o.reason.clone(),
            })
            .collect(),
        capture_consistency: "verified-signed-source-envelope".into(),
    };
    ensure!(
        identity(&expected) == identity(&receipt.snapshot),
        "Corrupt federated snapshot differs from its signed source"
    );
    ensure!(
        receipt.classifier()? != "unknown_preserved",
        "Federated{} receiver classifier is unknown; source authorization unavailable",
        if archive { " archive" } else { "" }
    );
    let mut policy = ReceiverPolicy::new(engine, grant, deadline)?;
    for file in &receipt.snapshot.files {
        deadline.check()?;
        policy.check(&file.path, deadline)?;
    }
    for file in &receipt.snapshot.files {
        deadline.check()?;
        ensure!(
            file.size <= MAX_FILE,
            "Stored federated source exceeds capacity"
        );
        let bytes = read_source(file, deadline)?;
        ensure!(
            bytes.len() == file.size && hash(&bytes) == file.hash,
            "Stored federated source size mismatch"
        );
        let text = std::str::from_utf8(&bytes)?;
        ensure!(
            !text.contains('\0') && !obvious_secret(text),
            "Stored federated source violates receiving policy"
        );
    }
    Ok(())
}
pub fn snapshot(
    engine: &Engine,
    id: &str,
    grant: &ImportGrant,
    deadline: &Deadline,
) -> Result<Snapshot> {
    Ok(receipt(engine, id, grant, deadline)?.into_snapshot())
}
pub fn read(
    engine: &Engine,
    id: &str,
    path: &str,
    grant: &ImportGrant,
    deadline: &Deadline,
) -> Result<Value> {
    let receipt = receipt(engine, id, grant, deadline)?;
    let path = normalized(path)?;
    let file = receipt
        .snapshot
        .files
        .iter()
        .find(|file| file.path == path)
        .context("Source path absent from federated snapshot")?;
    let text = String::from_utf8(
        engine
            .store
            .read_blob_exact(&file.hash, file.size, MAX_FILE)?,
    )?;
    Ok(
        json!({"remoteId":id,"namespace":"federated","scope":receipt.manifest.scope,"sourceSnapshotId":receipt.manifest.source_snapshot_id,"senderWorkspaceId":receipt.manifest.sender_workspace_id,"path":path,"contentHash":file.hash,"text":text,"mode":file.mode,"trust":"signed_source_not_execution_authority"}),
    )
}
fn graph_record(
    engine: &Engine,
    id: &str,
    grant: &ImportGrant,
    deadline: &Deadline,
) -> Result<(SelectedReceipt, Graph)> {
    let receipt = receipt(engine, id, grant, deadline)?;
    let raw = archive_record_raw(&engine.store, REMOTE_GRAPH, id, deadline)?;
    ensure!(
        raw.len() <= MAX_STATE
            && valid_id(&receipt.graph_hash)
            && hash(raw.as_bytes()) == receipt.graph_hash,
        "Stored federated graph is corrupt"
    );
    let graph: Graph = serde_json::from_str(&raw)?;
    ensure!(
        graph.snapshot_id == id
            && graph.entities.iter().all(|entity| entity.snapshot_id == id
                && receipt
                    .snapshot
                    .files
                    .iter()
                    .any(|file| file.path == entity.path))
            && graph.evidence.iter().all(|e| e.snapshot_id == id
                && receipt
                    .snapshot
                    .files
                    .iter()
                    .any(|f| f.path == e.path && f.hash == e.content_hash)),
        "Corrupt federated graph scope"
    );
    Ok((receipt, graph))
}
/// Storage data for the archive bridge, which must perform canonical source
/// validation before returning or persisting graph facts.
pub(crate) fn graph_for_archive_validation(
    engine: &Engine,
    id: &str,
    grant: &ImportGrant,
    deadline: &Deadline,
) -> Result<Graph> {
    Ok(graph_record(engine, id, grant, deadline)?.1)
}
pub fn graph(engine: &Engine, id: &str, grant: &ImportGrant, deadline: &Deadline) -> Result<Graph> {
    let (receipt, graph) = graph_record(engine, id, grant, deadline)?;
    validate_selected_archive_receipt(
        engine,
        &receipt,
        &receipt.snapshot,
        &graph,
        grant,
        &mut |file, _| {
            engine
                .store
                .read_blob_exact(&file.hash, file.size, MAX_FILE)
        },
        deadline,
    )?;
    Ok(graph)
}
pub fn list(store: &Store, deadline: &Deadline) -> Result<Value> {
    store.set_deadline(deadline)?;
    let mut statement = store
        .db
        .prepare("SELECT id,value FROM records WHERE kind IN (?1,?2) ORDER BY id LIMIT 257")?;
    let rows = statement.query_map(params![RECEIPT, RECEIPT_V2], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut items = Vec::new();
    let clock = now()?;
    for row in rows {
        deadline.check()?;
        let (id, raw) = row?;
        ensure!(
            items.len() < 256 && raw.len() <= MAX_STATE,
            "Federation namespace capacity exceeded"
        );
        let receipt = bounded_receipt(&crate::json::parse(raw.as_bytes(), MAX_STATE)?, deadline)?;
        ensure!(
            receipt.receiver_workspace_id == store.workspace_id && receipt.remote_id == id,
            "Corrupt federation namespace identity"
        );
        items.push(json!({"remoteId":id,"scope":receipt.manifest.scope,"senderWorkspaceId":receipt.manifest.sender_workspace_id,"sourceSnapshotId":receipt.manifest.source_snapshot_id,"files":receipt.snapshot.files.len(),"issuedAtMs":receipt.manifest.issued_at_ms,"expiresAtMs":receipt.manifest.expires_at_ms,"importValidityExpired":receipt.manifest.expires_at_ms<=clock,"trust":"previously_accepted_signed_source_receipt"}));
    }
    Ok(
        json!({"namespace":"federated","items":items,"sourceWritten":false,"limitations":["Listing historical receipts does not establish current grants, source completeness or latest remote state.","Source reads require the host's current scope/key grant and receiving policy."]}),
    )
}

/// Query the separate remote namespace only after checking the current host grant.
/// This capability has no public constructor and cannot be deserialized. It is
/// valid only for this synchronous query after the receipt and receiving policy
/// have been checked; it is never a native capture or a prospective path grant.
pub(crate) struct VerifiedQuerySource<'a> {
    store: &'a Store,
    snapshot: Snapshot,
}
impl VerifiedQuerySource<'_> {
    pub(crate) fn source_parts(&self) -> (&Store, &Snapshot) {
        (self.store, &self.snapshot)
    }
}

pub fn find(
    engine: &Engine,
    id: &str,
    grant: &ImportGrant,
    args: &Value,
    deadline: &Deadline,
) -> Result<Value> {
    let limit = args["limit"].as_u64().unwrap_or(30);
    ensure!(
        (1..=1000).contains(&limit),
        "Federation search limit must be 1..1000"
    );
    ensure!(
        args.get("languageServer").is_none() && args.get("snapshotId").is_none(),
        "Federation queries cannot select native snapshots or execute providers"
    );
    let mut validated = args.clone();
    validated["limit"] = json!(limit.min(200));
    crate::validate("ci_find", &validated)?;
    let kind = args["kind"].as_str().unwrap_or("symbol");
    crate::query::require_kind(kind)?;
    let verified = VerifiedQuerySource {
        store: &engine.store,
        snapshot: receipt(engine, id, grant, deadline)?.into_snapshot(),
    };
    let snapshot = &verified.snapshot;
    let graph = graph(engine, id, grant, deadline)?;
    let scope = crate::capture::federated_query_scope(&verified)?;
    match kind {
        "text" => crate::query_features::text_find_in_scope(
            &engine.store,
            snapshot,
            &graph,
            args,
            &scope,
            deadline,
        ),
        "symbol" | "file" | "all" => crate::query_features::symbols_find_in_scope(
            &engine.store,
            snapshot,
            &graph,
            args,
            &scope,
            deadline,
        ),
        _ => crate::query::find_base(&engine.store, snapshot, &graph, args, deadline),
    }
}
