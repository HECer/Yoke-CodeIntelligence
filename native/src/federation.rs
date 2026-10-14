//! Host-authorized offline immutable source federation.
use crate::{
    Engine, analysis,
    capture::{Deadline, language},
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
const REMOTE_GRAPH: &str = "native_federation_graph_v1";
const REPLAY: &str = "native_federation_replay_v1";
const MAX_FILES: usize = 2_000;
const MAX_FILE: usize = 4_000_000;
const MAX_SOURCE: usize = 16_000_000;
const MAX_ENVELOPE: usize = 32_000_000;
const MAX_STATE: usize = 64_000_000;
const MAX_TTL: u64 = 7 * 24 * 60 * 60 * 1000;
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
    let public: [u8; 32] = hex::decode(&grant.public_key)?
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
pub fn import_snapshot(
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
    let remote_id = remote_id(&engine.store.workspace_id, &envelope.manifest, &manifest_id);
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
            prior.manifest_id == manifest_id && prior.remote_id == remote_id,
            "Federation nonce was reused for a different signed manifest"
        );
        let _ = snapshot(engine, &remote_id, grant, deadline)?;
        return Ok(
            json!({"remoteId":remote_id,"manifestId":manifest_id,"idempotent":true,"sourceWritten":false,"scope":grant.scope,"namespace":"federated"}),
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
                language: language(&file.path).into(),
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
        extractions.push(analysis::extract(
            file,
            std::str::from_utf8(bytes)?,
            deadline,
        )?);
    }
    let mut graph = analysis::assemble(&snapshot, extractions, deadline)?;
    graph.coverage.exhaustive = false;
    graph.coverage.limitations.push("A valid sender signature authenticates supplied bytes and scope, not completeness, safety or current source freshness.".into());
    ensure!(
        graph.entities.len() <= 50_000
            && graph.observations.len() <= 100_000
            && graph.evidence.len() <= 200_000,
        "Federated graph capacity exceeded"
    );
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
            replay.manifest_id == manifest_id && replay.remote_id == remote_id,
            "Concurrent federation nonce conflict"
        );
        return Ok(
            json!({"remoteId":remote_id,"manifestId":manifest_id,"idempotent":true,"sourceWritten":false,"scope":grant.scope,"namespace":"federated"}),
        );
    }
    let count: usize = tx.query_row(
        "SELECT count(*) FROM records WHERE kind=?1",
        [RECEIPT],
        |row| row.get(0),
    )?;
    let source_bytes:usize=tx.query_row("SELECT coalesce(sum(json_extract(file.value,'$.size')),0) FROM records AS record JOIN json_each(record.value,'$.snapshot.files') AS file WHERE record.kind=?1",[RECEIPT],|row|row.get(0))?;
    let stored_bytes: usize = tx.query_row(
        "SELECT coalesce(sum(length(CAST(value AS BLOB))),0) FROM records WHERE kind IN (?1,?2)",
        params![RECEIPT, REMOTE_GRAPH],
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
        params![RECEIPT, remote_id, receipt_raw],
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
fn receipt(engine: &Engine, id: &str, grant: &ImportGrant, deadline: &Deadline) -> Result<Receipt> {
    engine.store.set_deadline(deadline)?;
    ensure!(valid_id(id), "Invalid federated source ID");
    let raw: String = engine.store.db.query_row(
        "SELECT value FROM records WHERE kind=?1 AND id=?2",
        params![RECEIPT, id],
        |row| row.get(0),
    )?;
    ensure!(raw.len() <= MAX_STATE, "Federated receipt exceeds capacity");
    let receipt: Receipt = serde_json::from_str(&raw)?;
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
            && remote_id(
                &engine.store.workspace_id,
                &receipt.manifest,
                &receipt.manifest_id
            ) == id,
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
                language: language(&file.path).into(),
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
    let mut policy = ReceiverPolicy::new(engine, grant, deadline)?;
    for file in &receipt.snapshot.files {
        deadline.check()?;
        policy.check(&file.path, deadline)?;
        let bytes = engine.store.read_blob(&file.hash)?;
        ensure!(
            bytes.len() == file.size,
            "Stored federated source size mismatch"
        );
        let text = std::str::from_utf8(&bytes)?;
        ensure!(
            !text.contains('\0') && !obvious_secret(text),
            "Stored federated source violates receiving policy"
        );
    }
    Ok(receipt)
}
pub fn snapshot(
    engine: &Engine,
    id: &str,
    grant: &ImportGrant,
    deadline: &Deadline,
) -> Result<Snapshot> {
    Ok(receipt(engine, id, grant, deadline)?.snapshot)
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
    let text = String::from_utf8(engine.store.read_blob(&file.hash)?)?;
    Ok(
        json!({"remoteId":id,"namespace":"federated","scope":receipt.manifest.scope,"sourceSnapshotId":receipt.manifest.source_snapshot_id,"senderWorkspaceId":receipt.manifest.sender_workspace_id,"path":path,"contentHash":file.hash,"text":text,"mode":file.mode,"trust":"signed_source_not_execution_authority"}),
    )
}
pub fn graph(engine: &Engine, id: &str, grant: &ImportGrant, deadline: &Deadline) -> Result<Graph> {
    let receipt = receipt(engine, id, grant, deadline)?;
    let raw: String = engine.store.db.query_row(
        "SELECT value FROM records WHERE kind=?1 AND id=?2",
        params![REMOTE_GRAPH, id],
        |row| row.get(0),
    )?;
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
    Ok(graph)
}
pub fn list(store: &Store, deadline: &Deadline) -> Result<Value> {
    store.set_deadline(deadline)?;
    let mut statement = store
        .db
        .prepare("SELECT id,value FROM records WHERE kind=?1 ORDER BY id LIMIT 257")?;
    let rows = statement.query_map([RECEIPT], |row| {
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
        let receipt: Receipt = serde_json::from_str(&raw)?;
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
