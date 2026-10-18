//! Explicit host signing authority and bounded immutable GitHub delivery admission.
//! HMAC authenticates body bytes, not delivery headers or current Git head.
use crate::{
    capture::Deadline,
    model::{hash, identity},
    store::Store,
};
use anyhow::{Result, ensure};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::Sha256;
use std::collections::BTreeSet;
use zeroize::Zeroizing;

#[path = "github_app_ingress.rs"]
pub mod ingress;

#[path = "github_app_worker.rs"]
pub mod worker;

#[path = "github_app_publication.rs"]
pub mod publication;

#[path = "github_app_pages.rs"]
pub mod pages;

#[path = "github_app_digest.rs"]
pub mod digest;

#[path = "github_app_store.rs"]
mod state;
pub use state::{ingest, queue, select, validated_inline_records};
pub const RECORD_KIND: &str = "github_app_event_v1";
pub const MAX_PAYLOAD_BYTES: usize = 1_000_000;
pub const MAX_RECORD_BYTES: usize = 2_100_000;
pub const MAX_STORE_BYTES: usize = 64_000_000;
const INVALID: &str = "github_app_input_invalid";
const BINDING: &str = "github_app_binding_invalid";
const CAPACITY: &str = "github_app_capacity_exceeded";

#[derive(Clone)]
pub struct Options {
    pub allow_writes: bool,
    pub max_payload_bytes: usize,
    pub max_deliveries: usize,
    pub max_jobs: usize,
    pub max_transitions: usize,
    pub max_store_bytes: usize,
    /// Includes the CLI's final LF, unlike lower-level JSON encoder limits.
    pub max_output_bytes: usize,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            allow_writes: false,
            max_payload_bytes: 512_000,
            max_deliveries: 1000,
            max_jobs: 1000,
            max_transitions: 1000,
            max_store_bytes: MAX_STORE_BYTES,
            max_output_bytes: 4_000_000,
        }
    }
}
fn options(o: &Options) -> Result<()> {
    ensure!(
        o.max_payload_bytes <= MAX_PAYLOAD_BYTES
            && o.max_deliveries <= 1000
            && o.max_jobs <= 1000
            && o.max_transitions <= 1000
            && o.max_store_bytes <= MAX_STORE_BYTES
            && o.max_output_bytes <= 4_000_000,
        INVALID
    );
    Ok(())
}
fn numeric(id: u64) -> bool {
    (1..=9_007_199_254_740_991).contains(&id)
}
fn credential_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 80
        && name.as_bytes()[0].is_ascii_alphanumeric()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
}
fn repository(name: &str) -> bool {
    if name.len() > 201 || name.to_ascii_lowercase() != name {
        return false;
    }
    let parts: Vec<_> = name.split('/').collect();
    parts.len() == 2
        && parts.iter().enumerate().all(|(index, part)| {
            !part.is_empty()
                && part.len() <= 100
                && !matches!(*part, "." | "..")
                && (index == 1 || part.as_bytes()[0].is_ascii_alphanumeric())
                && part
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
        })
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Grant {
    pub installation_id: u64,
    pub repository_id: u64,
    pub repository: String,
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HostProfile {
    schema_version: u32,
    app_id: u64,
    secret_credential: String,
    #[serde(default)]
    prior_secret_credentials: Vec<String>,
    grants: Vec<Grant>,
}
impl HostProfile {
    pub fn parse(raw: &[u8]) -> Result<Self> {
        let profile: Self = crate::json::decode(crate::json::parse(raw, 32_000)?)?;
        profile.validate()?;
        Ok(profile)
    }
    fn validate(&self) -> Result<()> {
        let mut names = BTreeSet::new();
        let mut grants = BTreeSet::new();
        ensure!(
            self.schema_version == 1
                && numeric(self.app_id)
                && self.prior_secret_credentials.len() <= 3
                && std::iter::once(&self.secret_credential)
                    .chain(&self.prior_secret_credentials)
                    .all(|name| credential_name(name) && names.insert(name))
                && (1..=32).contains(&self.grants.len())
                && self.grants.iter().all(|g| numeric(g.installation_id)
                    && numeric(g.repository_id)
                    && repository(&g.repository)
                    && grants.insert((g.installation_id, g.repository_id))),
            INVALID
        );
        Ok(())
    }
    fn permits(&self, job: &Job) -> bool {
        self.app_id == job.app_id
            && self.grants.iter().any(|grant| {
                grant.installation_id == job.installation_id
                    && grant.repository_id == job.repository_id
                    && grant.repository == job.repository
            })
    }
}
/// Explicit host input only; no Debug, Serialize, Deserialize or secret hash.
pub struct SigningKey(Zeroizing<Vec<u8>>);
impl SigningKey {
    pub fn from_host_bytes(bytes: &[u8]) -> Result<Self> {
        validate_secret(bytes)?;
        Ok(Self(Zeroizing::new(bytes.to_vec())))
    }
    pub fn verify(&self, raw: &[u8], signature: &str) -> Result<()> {
        let supplied = signature_bytes(signature)?;
        let mut mac = Hmac::<Sha256>::new_from_slice(&self.0)
            .map_err(|_| anyhow::anyhow!("github_app_signature_invalid"))?;
        mac.update(raw);
        mac.verify_slice(&supplied)
            .map_err(|_| anyhow::anyhow!("github_app_signature_invalid"))
    }
}
pub(crate) fn validate_secret(bytes: &[u8]) -> Result<()> {
    ensure!(
        !bytes.is_empty()
            && bytes.len() <= 2560
            && std::str::from_utf8(bytes).is_ok()
            && !bytes.iter().any(|b| matches!(b, 0 | b'\r' | b'\n')),
        "github_app_credential_invalid"
    );
    Ok(())
}
/// Keys and grants are chosen by the host, never by incoming or stored JSON.
pub struct Host {
    profile: HostProfile,
    keys: Vec<SigningKey>,
}
impl Host {
    /// Authenticate and validate host bytes before opening the project Store.
    /// Job workspace identity is bound later by ingest to that actual Store.
    pub fn preflight_delivery(
        &self,
        headers: &Headers,
        raw: &[u8],
        o: &Options,
        d: &Deadline,
    ) -> Result<()> {
        options(o)?;
        d.check()?;
        ensure!(raw.len() <= o.max_payload_bytes, CAPACITY);
        headers.validate()?;
        self.verify(raw, &headers.signature256)?;
        d.check()?;
        let checked = checked_payload(raw, self.profile.app_id, "")?;
        ensure!(
            self.profile.permits(&checked.job),
            "github_app_grant_denied"
        );
        d.check()?;
        Ok(())
    }
    pub fn with_signing_key(profile: HostProfile, key: SigningKey) -> Result<Self> {
        Self::with_signing_keys(profile, vec![key])
    }
    /// Explicit host-supplied keys in profile order, including optional rotation keys.
    pub fn with_signing_keys(profile: HostProfile, keys: Vec<SigningKey>) -> Result<Self> {
        profile.validate()?;
        ensure!(
            keys.len() == 1 + profile.prior_secret_credentials.len(),
            INVALID
        );
        Ok(Self { profile, keys })
    }
    pub fn from_os(profile: HostProfile) -> Result<Self> {
        profile.validate()?;
        let mut keys = Vec::new();
        for name in
            std::iter::once(&profile.secret_credential).chain(&profile.prior_secret_credentials)
        {
            let raw = crate::host_credentials::read_github_webhook_secret(name)
                .map_err(|_| anyhow::anyhow!("github_app_credential_unavailable"))?;
            keys.push(SigningKey::from_host_bytes(&raw)?);
        }
        Ok(Self { profile, keys })
    }
    fn verify(&self, raw: &[u8], signature: &str) -> Result<()> {
        let mut verified = false;
        for key in &self.keys {
            verified |= key.verify(raw, signature).is_ok();
        }
        ensure!(verified, "github_app_signature_invalid");
        Ok(())
    }
}
fn signature_bytes(signature: &str) -> Result<[u8; 32]> {
    ensure!(
        signature.len() == 71
            && signature.starts_with("sha256=")
            && signature.as_bytes()[7..]
                .iter()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b)),
        "github_app_signature_invalid"
    );
    let mut out = [0; 32];
    hex::decode_to_slice(&signature[7..], &mut out)
        .map_err(|_| anyhow::anyhow!("github_app_signature_invalid"))?;
    Ok(out)
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Headers {
    pub schema_version: u32,
    pub event: String,
    pub delivery_id: String,
    pub signature256: String,
}
impl Headers {
    pub fn parse(raw: &[u8]) -> Result<Self> {
        let value: Self = crate::json::decode(crate::json::parse(raw, 2000)?)?;
        value.validate()?;
        Ok(value)
    }
    fn validate(&self) -> Result<()> {
        let id = self.delivery_id.as_bytes();
        ensure!(
            self.schema_version == 1
                && self.event == "pull_request"
                && id.len() == 36
                && id
                    .iter()
                    .enumerate()
                    .all(|(i, b)| if [8, 13, 18, 23].contains(&i) {
                        *b == b'-'
                    } else {
                        b.is_ascii_digit() || (b'a'..=b'f').contains(b)
                    }),
            INVALID
        );
        signature_bytes(&self.signature256)?;
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum WorkClass {
    Review,
    ClosedDiscussionCandidate,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Job {
    pub id: String,
    pub workspace_id: String,
    pub app_id: u64,
    pub installation_id: u64,
    pub repository_id: u64,
    pub repository: String,
    pub number: u64,
    pub head_sha: String,
    pub base_sha: String,
    pub work_class: WorkClass,
}
type Group = (u64, u64, u64, u64, WorkClass);
impl Job {
    fn group(&self) -> Group {
        (
            self.app_id,
            self.installation_id,
            self.repository_id,
            self.number,
            self.work_class.clone(),
        )
    }
}
// Unknown GitHub payload properties are authenticated inert data, never host options.
#[derive(Deserialize)]
struct Payload {
    action: String,
    number: u64,
    installation: Installation,
    repository: Repository,
    pull_request: PullRequest,
}
#[derive(Deserialize)]
struct Installation {
    id: u64,
}
#[derive(Deserialize)]
struct Repository {
    id: u64,
    full_name: String,
}
#[derive(Deserialize)]
struct PullRequest {
    number: u64,
    state: String,
    draft: bool,
    title: String,
    head: Head,
    base: Base,
}
#[derive(Deserialize)]
struct Head {
    sha: String,
}
#[derive(Deserialize)]
struct Base {
    sha: String,
    repo: Repository,
}
struct CheckedPayload {
    job: Job,
    title: String,
    draft: bool,
}
fn checked_payload(raw: &[u8], app_id: u64, workspace_id: &str) -> Result<CheckedPayload> {
    let result = (|| -> Result<CheckedPayload> {
        let p: Payload = crate::json::decode(crate::json::parse(raw, MAX_PAYLOAD_BYTES)?)?;
        let repository_name = p.repository.full_name.to_ascii_lowercase();
        ensure!(
            numeric(app_id)
                && numeric(p.number)
                && numeric(p.installation.id)
                && numeric(p.repository.id)
                && repository(&repository_name)
                && p.number == p.pull_request.number
                && p.repository.id == p.pull_request.base.repo.id
                && repository_name == p.pull_request.base.repo.full_name.to_ascii_lowercase(),
            INVALID
        );
        let closed = p.action == "closed";
        ensure!(
            matches!(
                p.action.as_str(),
                "opened"
                    | "reopened"
                    | "synchronize"
                    | "ready_for_review"
                    | "converted_to_draft"
                    | "edited"
                    | "closed"
            ) && p.pull_request.state == if closed { "closed" } else { "open" }
                && (p.action != "ready_for_review" || !p.pull_request.draft)
                && (p.action != "converted_to_draft" || p.pull_request.draft)
                && p.pull_request.title.len() <= 20_000
                && !p
                    .pull_request
                    .title
                    .chars()
                    .any(|c| c.is_control() && !matches!(c, '\r' | '\n' | '\t')),
            INVALID
        );
        for sha in [&p.pull_request.head.sha, &p.pull_request.base.sha] {
            ensure!(
                sha.len() == 40
                    && sha
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
                INVALID
            );
        }
        let mut job = Job {
            id: String::new(),
            workspace_id: workspace_id.into(),
            app_id,
            installation_id: p.installation.id,
            repository_id: p.repository.id,
            repository: repository_name,
            number: p.number,
            head_sha: p.pull_request.head.sha,
            base_sha: p.pull_request.base.sha,
            work_class: if closed {
                WorkClass::ClosedDiscussionCandidate
            } else {
                WorkClass::Review
            },
        };
        job.id = identity(&job);
        Ok(CheckedPayload {
            job,
            title: p.pull_request.title,
            draft: p.pull_request.draft,
        })
    })();
    result.map_err(|_| anyhow::anyhow!(INVALID))
}
fn admit_output(value: &Value, o: &Options, d: &Deadline) -> Result<()> {
    ensure!(o.max_output_bytes > 0, CAPACITY);
    let result = crate::json::encode_bounded(value, o.max_output_bytes - 1, d);
    d.check()?;
    result.map_err(|_| anyhow::anyhow!(CAPACITY))?;
    Ok(())
}
fn boundary() -> Value {
    json!({"complete":false,"currentHeadVerified":false,"jobMetadataAuthority":"first_authenticated_delivery",
    "installationTokenVerified":false,"selectionAuthority":"explicit_host_scheduling_assertion",
    "sourceWritten":false,"modelCalls":0,"networkRequests":0,"published":false})
}
