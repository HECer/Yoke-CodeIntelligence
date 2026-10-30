//! An explicitly selected, independently owned digest receiver contract. A
//! proposal is freshly generated from captured source, never imported as authority.
use super::{PreviewRequest, discussions};
use crate::{
    capture::Deadline,
    github_app::{Grant, Host},
    model::{hash, identity, valid_id},
    store::Store,
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{io::Read, net::SocketAddr};
use zeroize::Zeroizing;

#[path = "github_app_digest_receiver.rs"]
pub mod receiver;
#[path = "github_app_digest_service_state.rs"]
mod state;
pub const RECORD_KIND: &str = "github_app_digest_service_v1";
const INVALID: &str = "github_app_digest_service_input_invalid";
const BINDING: &str = "github_app_digest_service_binding_invalid";
const CAPACITY: &str = "github_app_digest_service_capacity_exceeded";
const UNKNOWN: &str = "github_app_digest_service_outcome_unknown";

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Profile {
    pub schema_version: u32,
    /// Exact canonical origin. No path, redirect, ambient proxy or URL credential.
    pub endpoint: String,
    pub credential_name: Option<String>,
    pub grants: Vec<Grant>,
    pub max_payload_bytes: usize,
    pub max_response_bytes: usize,
    pub max_journal_records: usize,
    pub max_journal_bytes: usize,
}
impl Profile {
    pub fn parse(raw: &[u8]) -> Result<Self> {
        let p: Self = crate::json::decode(
            crate::json::parse(raw, 32_000).map_err(|_| anyhow::anyhow!(INVALID))?,
        )
        .map_err(|_| anyhow::anyhow!(INVALID))?;
        p.validate()?;
        Ok(p)
    }
    fn origin(&self) -> Result<reqwest::Url> {
        ensure!(self.endpoint.len() <= 512, INVALID);
        let u = reqwest::Url::parse(&self.endpoint).map_err(|_| anyhow::anyhow!(INVALID))?;
        ensure!(
            u.host_str().is_some()
                && u.username().is_empty()
                && u.password().is_none()
                && u.path() == "/"
                && u.query().is_none()
                && u.fragment().is_none()
                && u.as_str() == self.endpoint,
            INVALID
        );
        Ok(u)
    }
    fn fixture(&self) -> Result<bool> {
        let u = self.origin()?;
        if u.scheme() == "https" {
            return Ok(false);
        }
        ensure!(
            u.scheme() == "http"
                && u.host_str().is_some_and(|h| h
                    .trim_matches(['[', ']'])
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback()))
                && u.port().is_some_and(|p| p != 0),
            INVALID
        );
        Ok(true)
    }
    fn validate(&self) -> Result<()> {
        let fixture = self.fixture()?;
        let mut seen = std::collections::BTreeSet::new();
        ensure!(
            self.schema_version == 1
                && (1..=32).contains(&self.grants.len())
                && self
                    .grants
                    .iter()
                    .all(|g| super::super::numeric(g.installation_id)
                        && super::super::numeric(g.repository_id)
                        && super::super::repository(&g.repository)
                        && seen.insert((g.installation_id, g.repository_id)))
                && (1..=4_000_000).contains(&self.max_payload_bytes)
                && (256..=64_000).contains(&self.max_response_bytes)
                && (2..=1000).contains(&self.max_journal_records)
                && (8192..=8_000_000).contains(&self.max_journal_bytes)
                && if fixture {
                    self.credential_name.is_none()
                } else {
                    self.credential_name
                        .as_deref()
                        .is_some_and(super::super::credential_name)
                },
            INVALID
        );
        Ok(())
    }
    fn permits(&self, request: &PreviewRequest) -> bool {
        self.grants.iter().any(|g| {
            g.installation_id == request.installation_id
                && g.repository_id == request.repository_id
                && g.repository == request.repository
        })
    }
}
/// Host booleans never deserialize from repository data or a proposal.
pub struct Options {
    pub allow_service: bool,
    pub allow_network: bool,
    pub allow_writes: bool,
    pub allow_fixture: bool,
    /// Includes the CLI LF. Admission precedes intent publication and network.
    pub max_output_bytes: usize,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            allow_service: false,
            allow_network: false,
            allow_writes: false,
            allow_fixture: false,
            max_output_bytes: 32_000,
        }
    }
}
pub struct Context<'a> {
    pub store: &'a Store,
    pub host: &'a Host,
    pub profile: &'a Profile,
    pub options: &'a Options,
}
impl Context<'_> {
    fn admit(&self, remote: bool, write: bool, d: &Deadline) -> Result<()> {
        d.check()?;
        ensure!(
            self.options.allow_service,
            "github_app_digest_service_permission_required"
        );
        ensure!(
            !remote || self.options.allow_network,
            "github_app_digest_service_network_permission_required"
        );
        ensure!(
            !write || self.options.allow_writes,
            "github_app_digest_service_write_permission_required"
        );
        self.profile.validate()?;
        ensure!(
            !self.profile.fixture()? || self.options.allow_fixture,
            "github_app_digest_service_fixture_denied"
        );
        ensure!(
            (256..=4_000_000).contains(&self.options.max_output_bytes),
            CAPACITY
        );
        ensure!(
            self.store.db.is_autocommit(),
            "github_app_digest_service_caller_transaction_refused"
        );
        Ok(())
    }
    fn scope(&self, request: &PreviewRequest) -> Result<()> {
        super::authorize_profile(&self.host.profile, request)?;
        ensure!(
            self.profile.permits(request),
            "github_app_digest_service_scope_denied"
        );
        Ok(())
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Approval {
    pub schema_version: u32,
    pub proposal_id: String,
    pub payload_hash: String,
    pub request_id: String,
    pub approval: String,
}
fn request_id(s: &str) -> bool {
    s.len() == 36
        && s.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
            }
        })
}
pub fn admit_status_request(id: &str) -> Result<()> {
    ensure!(request_id(id), INVALID);
    Ok(())
}
impl Approval {
    pub fn parse(raw: &[u8]) -> Result<Self> {
        let a: Self = crate::json::decode(
            crate::json::parse(raw, 4096).map_err(|_| anyhow::anyhow!(INVALID))?,
        )
        .map_err(|_| anyhow::anyhow!(INVALID))?;
        a.validate()?;
        Ok(a)
    }
    fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == 1
                && valid_id(&self.proposal_id)
                && valid_id(&self.payload_hash)
                && request_id(&self.request_id)
                && self.approval == "publish_exact_digest",
            INVALID
        );
        Ok(())
    }
}
/// Cannot be deserialized or caller-constructed. Only current source replay
/// creates submission bytes; printing a proposal grants no sending authority.
pub struct Proposal {
    body: Vec<u8>,
    data: state::Data,
}
impl Proposal {
    pub fn check_approval(&self, approval: &Approval) -> Result<()> {
        approval.validate()?;
        ensure!(
            approval.proposal_id == self.data.proposal_id
                && approval.payload_hash == self.data.payload_hash,
            "github_app_digest_service_stale_approval"
        );
        Ok(())
    }
    pub fn value(&self, d: &Deadline) -> Result<Value> {
        let body =
            crate::json::parse(&self.body, 4_000_000).map_err(|_| anyhow::anyhow!(BINDING))?;
        d.check()?;
        Ok(
            json!({"schemaVersion":1,"kind":"native_digest_service_proposal","proposalId":self.data.proposal_id,
            "payloadHash":self.data.payload_hash,"payloadBytes":self.body.len(),"endpoint":self.data.endpoint,
            "fixture":self.data.fixture,"body":body,"approvalRequired":"publish_exact_digest",
            "networkRequests":0,"sourceWritten":false,"modelCalls":0,"serviceAccepted":false}),
        )
    }
}
pub fn propose(
    source: &discussions::Context<'_>,
    request: &PreviewRequest,
    capture: &crate::github::discussions::Capture,
    profile: &Profile,
    d: &Deadline,
) -> Result<Proposal> {
    profile.validate()?;
    ensure!(
        profile.permits(request),
        "github_app_digest_service_scope_denied"
    );
    let permission = discussions::Permission {
        allow_discussions: source.permission.allow_discussions,
        allow_fixture: source.permission.allow_fixture,
        handoff_preview: true,
    };
    let fresh = discussions::Context {
        store: source.store,
        profile: source.profile,
        options: source.options,
        permission: &permission,
        source: match &source.source {
            discussions::Source::Current => discussions::Source::Current,
            discussions::Source::History {
                git,
                options,
                mapping,
            } => discussions::Source::History {
                git,
                options,
                mapping: *mapping,
            },
        },
    };
    let preview = discussions::preview(&fresh, request, capture, d)?;
    let body = crate::json::encode_bounded(
        &json!({"schemaVersion":1,"kind":"native_digest_submission_v1",
        "workspaceId":source.store.workspace_id,"appId":source.profile.app_id,"digest":preview}),
        profile.max_payload_bytes,
        d,
    )
    .map_err(|_| d.check().err().unwrap_or_else(|| anyhow::anyhow!(CAPACITY)))?;
    let mut snapshot_ids = std::collections::BTreeSet::from([request.snapshot_id.clone()]);
    if let discussions::Source::History {
        mapping: Some(selection),
        ..
    } = &source.source
    {
        snapshot_ids.extend(selection.snapshot_ids.iter().cloned());
    }
    let payload_hash = hash(&body);
    let proposal_id = identity(
        &json!({"schemaVersion":1,"workspaceId":source.store.workspace_id,
        "appId":source.profile.app_id,"endpoint":profile.endpoint,"payloadHash":payload_hash}),
    );
    Ok(Proposal {
        body,
        data: state::Data {
            app_id: source.profile.app_id,
            request: request.clone(),
            snapshot_ids: snapshot_ids.into_iter().collect(),
            endpoint: profile.endpoint.clone(),
            fixture: profile.fixture()?,
            proposal_id,
            payload_hash,
            digest_id: preview["id"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!(BINDING))?
                .into(),
        },
    })
}
pub struct Client {
    endpoint: String,
    credential_name: Option<String>,
    fixture: bool,
    token: Option<Zeroizing<Vec<u8>>>,
    http: reqwest::blocking::Client,
}
impl Client {
    pub fn from_os(c: &Context<'_>, d: &Deadline) -> Result<Self> {
        Self::from_os_root(c, None, d)
    }
    /// Additional CA selected by the host, never by an incoming digest.
    pub fn from_os_with_ca(c: &Context<'_>, ca: &[u8], d: &Deadline) -> Result<Self> {
        Self::from_os_root(c, Some(ca), d)
    }
    fn from_os_root(c: &Context<'_>, ca: Option<&[u8]>, d: &Deadline) -> Result<Self> {
        c.admit(true, false, d)?;
        ensure!(
            !c.profile.fixture()?,
            "github_app_digest_service_fixture_constructor_required"
        );
        let token = crate::host_credentials::read_token(
            c.profile
                .credential_name
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!(INVALID))?,
        )
        .map_err(|_| anyhow::anyhow!("github_app_digest_service_credential_unavailable"))?;
        Self::build(c.profile, Some(token), ca, d)
    }
    /// Numeric loopback only; never reads or forwards a production credential.
    pub fn for_fixture(c: &Context<'_>, endpoint: SocketAddr, d: &Deadline) -> Result<Self> {
        c.admit(true, false, d)?;
        ensure!(
            c.profile.fixture()?
                && endpoint.ip().is_loopback()
                && endpoint.port() != 0
                && c.profile.endpoint == format!("http://{endpoint}/"),
            INVALID
        );
        Self::build(c.profile, None, None, d)
    }
    /// Explicit synthetic token for an owned numeric-loopback receiver fixture.
    /// Never reads a production credential or accepts an HTTP token from JSON.
    pub fn for_fixture_token(
        c: &Context<'_>,
        endpoint: SocketAddr,
        token: [u8; 32],
        d: &Deadline,
    ) -> Result<Self> {
        c.admit(true, false, d)?;
        ensure!(
            c.profile.fixture()?
                && endpoint.ip().is_loopback()
                && endpoint.port() != 0
                && c.profile.endpoint == format!("http://{endpoint}/"),
            INVALID
        );
        Self::build(c.profile, Some(Zeroizing::new(token.to_vec())), None, d)
    }
    fn build(
        profile: &Profile,
        token: Option<Zeroizing<Vec<u8>>>,
        ca: Option<&[u8]>,
        d: &Deadline,
    ) -> Result<Self> {
        ensure!(
            tokio::runtime::Handle::try_current().is_err(),
            "github_app_requires_synchronous_host"
        );
        if rustls::crypto::CryptoProvider::get_default().is_none() {
            let _ = rustls::crypto::ring::default_provider().install_default();
        }
        let mut builder = reqwest::blocking::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .https_only(!profile.fixture()?)
            .timeout(d.remaining());
        if let Some(ca) = ca {
            ensure!(
                !profile.fixture()? && !ca.is_empty() && ca.len() <= 32_000,
                INVALID
            );
            builder = builder.add_root_certificate(
                reqwest::Certificate::from_der(ca).map_err(|_| anyhow::anyhow!(INVALID))?,
            );
        }
        let http = builder.build().map_err(|_| anyhow::anyhow!(INVALID))?;
        Ok(Self {
            endpoint: profile.endpoint.clone(),
            credential_name: profile.credential_name.clone(),
            fixture: profile.fixture()?,
            token,
            http,
        })
    }
    fn matches(&self, c: &Context<'_>) -> Result<()> {
        ensure!(
            self.endpoint == c.profile.endpoint
                && self.fixture == c.profile.fixture()?
                && self.credential_name == c.profile.credential_name,
            BINDING
        );
        Ok(())
    }
    fn call(
        &self,
        c: &Context<'_>,
        data: &state::Data,
        body: Option<&[u8]>,
        d: &Deadline,
    ) -> Result<Option<Receipt>> {
        c.admit(true, true, d)?;
        c.scope(&data.request)?;
        self.matches(c)?;
        ensure!(
            data.endpoint == self.endpoint && data.fixture == self.fixture,
            BINDING
        );
        let url = format!("{}v1/digests/{}", self.endpoint, data.proposal_id);
        let mut call = if let Some(body) = body {
            ensure!(
                hash(body) == data.payload_hash && body.len() <= c.profile.max_payload_bytes,
                BINDING
            );
            self.http
                .put(url)
                .header("Content-Type", "application/json")
                .header("Idempotency-Key", &data.proposal_id)
                .body(body.to_vec())
        } else {
            self.http.get(url)
        };
        call = call
            .header("Accept", "application/json")
            .header("User-Agent", "yoke-code-intelligence/0.5")
            .timeout(d.remaining());
        if let Some(token) = &self.token {
            let secret = Zeroizing::new(hex::encode(token.as_slice()));
            call = call.bearer_auth(secret.as_str());
        }
        d.check()?;
        let mut response = call.send().map_err(|_| anyhow::anyhow!(UNKNOWN))?;
        if body.is_none() && response.status() == reqwest::StatusCode::NOT_FOUND {
            d.check()?;
            return Ok(None);
        }
        ensure!(matches!(response.status().as_u16(), 200 | 201), UNKNOWN);
        ensure!(
            response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|s| s.to_str().ok())
                .is_some_and(|s| s
                    .split(';')
                    .next()
                    .is_some_and(|v| v.trim().eq_ignore_ascii_case("application/json")))
                && response
                    .content_length()
                    .is_none_or(|n| n <= c.profile.max_response_bytes as u64),
            UNKNOWN
        );
        let mut raw = Vec::new();
        let mut block = [0_u8; 4096];
        loop {
            d.check()?;
            let capacity = (c.profile.max_response_bytes - raw.len() + 1).min(block.len());
            let n = response
                .read(&mut block[..capacity])
                .map_err(|_| anyhow::anyhow!(UNKNOWN))?;
            if n == 0 {
                break;
            }
            ensure!(raw.len() + n <= c.profile.max_response_bytes, UNKNOWN);
            raw.extend_from_slice(&block[..n]);
        }
        let receipt: Receipt = crate::json::decode(
            crate::json::parse(&raw, c.profile.max_response_bytes)
                .map_err(|_| anyhow::anyhow!(UNKNOWN))?,
        )
        .map_err(|_| anyhow::anyhow!(UNKNOWN))?;
        receipt.validate(data)?;
        d.check()?;
        Ok(Some(receipt))
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Receipt {
    schema_version: u32,
    proposal_id: String,
    payload_hash: String,
    receipt_id: String,
    accepted: bool,
}
impl Receipt {
    fn validate(&self, data: &state::Data) -> Result<()> {
        ensure!(
            self.schema_version == 1
                && self.accepted
                && self.proposal_id == data.proposal_id
                && self.payload_hash == data.payload_hash
                && valid_id(&self.receipt_id),
            UNKNOWN
        );
        Ok(())
    }
}
pub fn send(
    c: &Context<'_>,
    source: &discussions::Context<'_>,
    request: &PreviewRequest,
    capture: &crate::github::discussions::Capture,
    approval: &Approval,
    client: &Client,
    d: &Deadline,
) -> Result<Value> {
    c.admit(true, true, d)?;
    c.scope(request)?;
    approval.validate()?;
    client.matches(c)?;
    ensure!(
        std::ptr::eq(c.store, source.store) && c.host.profile.app_id == source.profile.app_id,
        BINDING
    );
    let _lock = state::lock(c, d)?;
    let proposal = propose(source, request, capture, c.profile, d)?;
    proposal.check_approval(approval)?;
    if let Some(value) = state::existing(c, approval, d)? {
        return Ok(value);
    }
    state::intend(c, approval, &proposal.data, d)?;
    // A durable intent is deliberately retained for every later failure. It is
    // never considered proof that a send occurred, and never mints a retry.
    let receipt = client
        .call(c, &proposal.data, Some(&proposal.body), d)?
        .ok_or_else(|| anyhow::anyhow!(UNKNOWN))?;
    state::accept(c, &approval.request_id, &receipt, false, d)
}
pub fn status(c: &Context<'_>, request: &str, d: &Deadline) -> Result<Value> {
    c.admit(false, false, d)?;
    ensure!(request_id(request), INVALID);
    state::status(c, request, d)
}
pub fn recover(c: &Context<'_>, request: &str, client: &Client, d: &Deadline) -> Result<Value> {
    c.admit(true, true, d)?;
    ensure!(request_id(request), INVALID);
    client.matches(c)?;
    let _lock = state::lock(c, d)?;
    let (data, terminal) = state::pending(c, request, d)?;
    if let Some(value) = terminal {
        return Ok(value);
    }
    match client.call(c, &data, None, d)? {
        Some(receipt) => state::accept(c, request, &receipt, true, d),
        None => {
            let mut v = state::status(c, request, d)?;
            v["networkRequests"] = json!(1);
            Ok(v)
        }
    }
}
pub(crate) fn retained_roots(
    store: &Store,
    d: &Deadline,
) -> Result<(
    std::collections::BTreeSet<String>,
    std::collections::BTreeSet<String>,
)> {
    state::retained_roots(store, d)
}
