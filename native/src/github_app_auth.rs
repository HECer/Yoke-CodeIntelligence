//! Explicit host app-key authority and bounded installation authorization.
//! Protocol fixtures are distinct from real GitHub installation evidence.
use crate::capture::Deadline;
use anyhow::{Result, ensure};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};
use zeroize::{Zeroize, Zeroizing};

#[path = "github_app_key.rs"]
pub(crate) mod key;
#[path = "github_app_auth_read.rs"]
pub(crate) mod read;
#[path = "github_app_auth_http.rs"]
mod transport;
pub use key::AppKey;
const INVALID: &str = "github_app_auth_input_invalid";
const BINDING: &str = "github_app_installation_binding_invalid";
const CAPACITY: &str = "github_app_auth_capacity_exceeded";
const VERSION: &str = "2026-03-10";
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Grant {
    installation_id: u64,
    repository_id: u64,
    repository: String,
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuthProfile {
    schema_version: u32,
    app_id: u64,
    client_id: Option<String>,
    key_credential: String,
    grants: Vec<Grant>,
    permissions: BTreeMap<String, String>,
}
fn numeric(id: u64) -> bool {
    (1..=9_007_199_254_740_991).contains(&id)
}
fn credential(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 80
        && name.as_bytes()[0].is_ascii_alphanumeric()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
}
fn repository(name: &str) -> bool {
    let Some((owner, repo)) = name.split_once('/') else {
        return false;
    };
    !owner.is_empty()
        && owner.len() <= 39
        && !owner.starts_with('-')
        && !owner.ends_with('-')
        && owner
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        && !repo.is_empty()
        && repo.len() <= 100
        && !matches!(repo, "." | "..")
        && repo
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
        && name.to_ascii_lowercase() == name
}
impl AuthProfile {
    pub fn parse(raw: &[u8]) -> Result<Self> {
        let value: Self = crate::json::decode(crate::json::parse(raw, 32_000)?)?;
        value.validate()?;
        Ok(value)
    }
    pub(crate) fn admit_comment_profile(&self, app_id: u64, write: bool) -> Result<()> {
        let permission = self.permissions.get("pull_requests").map(String::as_str);
        ensure!(
            self.app_id == app_id
                && self.permissions.len() == 1
                && if write {
                    permission == Some("write")
                } else {
                    matches!(permission, Some("read" | "write"))
                },
            INVALID
        );
        Ok(())
    }
    fn validate(&self) -> Result<()> {
        let mut ids = BTreeSet::new();
        let mut names = BTreeSet::new();
        ensure!(
            self.schema_version == 1
                && numeric(self.app_id)
                && credential(&self.key_credential)
                && self.client_id.as_ref().is_none_or(|id| !id.is_empty()
                    && id.len() <= 100
                    && id
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.')))
                && (1..=32).contains(&self.grants.len())
                && self.grants.iter().all(|g| numeric(g.installation_id)
                    && numeric(g.repository_id)
                    && repository(&g.repository)
                    && ids.insert((g.installation_id, g.repository_id))
                    && names.insert((g.installation_id, g.repository.as_str())))
                && (1..=4).contains(&self.permissions.len())
                && self.permissions.iter().all(|(k, v)| match k.as_str() {
                    "contents" => v == "read",
                    "pull_requests" | "issues" | "checks" => matches!(v.as_str(), "read" | "write"),
                    _ => false,
                }),
            INVALID
        );
        Ok(())
    }
}
#[derive(Clone)]
pub struct Options {
    pub allow_network: bool,
    pub max_requests: usize,
    pub max_response_bytes: usize,
    pub max_total_bytes: usize,
    /// Includes the CLI's final LF. Does not make a remote token POST atomic.
    pub max_output_bytes: usize,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            allow_network: false,
            max_requests: 4,
            max_response_bytes: 1_000_000,
            max_total_bytes: 4_000_000,
            max_output_bytes: 64_000,
        }
    }
}
impl Options {
    pub fn admit(&self, d: &Deadline) -> Result<()> {
        ensure!(self.allow_network, "github_app_network_permission_required");
        d.check()?;
        ensure!(
            self.max_requests <= 4
                && self.max_response_bytes <= 1_000_000
                && self.max_total_bytes <= 4_000_000
                && self.max_output_bytes <= 4_000_000,
            INVALID
        );
        ensure!(
            self.max_requests == 4
                && self.max_response_bytes > 0
                && self.max_total_bytes > 0
                && self.max_output_bytes > 0,
            CAPACITY
        );
        Ok(())
    }
}
/// An in-process host fixture capability, never selected by JSON or OS keys.
pub struct FixtureProfile {
    endpoint: SocketAddr,
    clock: Arc<AtomicU64>,
}
impl FixtureProfile {
    pub fn loopback(endpoint: SocketAddr, now: u64) -> Result<Self> {
        Self::loopback_with_clock(endpoint, Arc::new(AtomicU64::new(now)))
    }
    /// Trusted scripted fixture time; production always reads the host system clock.
    pub fn loopback_with_clock(endpoint: SocketAddr, clock: Arc<AtomicU64>) -> Result<Self> {
        let now = clock.load(Ordering::SeqCst);
        ensure!(
            endpoint.ip().is_loopback()
                && endpoint.port() != 0
                && (60..=253_402_296_000).contains(&now),
            INVALID
        );
        Ok(Self { endpoint, clock })
    }
}
/// Short-lived token and checked grants. No Debug/Serialize/Deserialize.
pub struct InstallationLease {
    evidence: Value,
    _token: Zeroizing<String>,
    context: LeaseContext,
    observed: AtomicU64,
}
struct LeaseContext {
    client_identity: Arc<()>,
    origin: reqwest::Url,
    app_id: u64,
    grant: Grant,
    permissions: BTreeMap<String, String>,
    checked_at: u64,
    expires_at: u64,
}
impl InstallationLease {
    pub fn evidence(&self) -> &Value {
        &self.evidence
    }
}
pub struct Client {
    profile: AuthProfile,
    key: AppKey,
    options: Options,
    origin: reqwest::Url,
    http: reqwest::Client,
    fixture_clock: Option<Arc<AtomicU64>>,
    runtime: Option<tokio::runtime::Runtime>,
    identity: Arc<()>,
}
impl Drop for Client {
    fn drop(&mut self) {
        if let Some(runtime) = self.runtime.take() {
            // Safe even if an idle synchronous client is dropped by an async host.
            // System DNS tasks are an OS/library boundary, not owned request futures.
            runtime.shutdown_background();
        }
    }
}
impl Client {
    pub fn production(profile: AuthProfile, o: Options, d: &Deadline) -> Result<Self> {
        o.admit(d)?;
        profile.validate()?;
        d.check()?;
        let key = crate::host_credentials::read_github_app_key(&profile.key_credential)?;
        Self::new(profile, key, o, "https://api.github.com", None, d)
    }
    pub fn for_fixture(
        profile: AuthProfile,
        key: AppKey,
        fixture: FixtureProfile,
        o: Options,
        d: &Deadline,
    ) -> Result<Self> {
        o.admit(d)?;
        profile.validate()?;
        Self::new(
            profile,
            key,
            o,
            &format!("http://{}", fixture.endpoint),
            Some(fixture.clock),
            d,
        )
    }
    fn new(
        profile: AuthProfile,
        key: AppKey,
        options: Options,
        origin: &str,
        fixture_clock: Option<Arc<AtomicU64>>,
        d: &Deadline,
    ) -> Result<Self> {
        d.check()?;
        ensure!(
            tokio::runtime::Handle::try_current().is_err(),
            "github_app_requires_synchronous_host"
        );
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| anyhow::anyhow!("github_app_transport_failed"))?;
        if rustls::crypto::CryptoProvider::get_default().is_none() {
            let _ = rustls::crypto::ring::default_provider().install_default();
        }
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .no_proxy()
            .https_only(fixture_clock.is_none())
            .timeout(d.remaining())
            .build()
            .map_err(|_| anyhow::anyhow!("github_app_transport_failed"))?;
        let origin = reqwest::Url::parse(origin).map_err(|_| anyhow::anyhow!(INVALID))?;
        Ok(Self {
            profile,
            key,
            options,
            origin,
            http,
            fixture_clock,
            runtime: Some(runtime),
            identity: Arc::new(()),
        })
    }
    fn now(&self) -> Result<u64> {
        let now = if let Some(clock) = &self.fixture_clock {
            clock.load(Ordering::SeqCst)
        } else {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| anyhow::anyhow!("github_app_clock_invalid"))?
                .as_secs()
        };
        ensure!(
            (60..=253_402_296_000).contains(&now),
            "github_app_clock_invalid"
        );
        Ok(now)
    }
    pub fn authorize(
        &mut self,
        installation_id: u64,
        repository_id: u64,
        d: &Deadline,
    ) -> Result<InstallationLease> {
        self.options.admit(d)?;
        ensure!(
            tokio::runtime::Handle::try_current().is_err(),
            "github_app_requires_synchronous_host"
        );
        let grant = self
            .profile
            .grants
            .iter()
            .find(|g| g.installation_id == installation_id && g.repository_id == repository_id)
            .ok_or_else(|| anyhow::anyhow!("github_app_grant_denied"))?
            .clone();
        let now = self.now()?;
        let issuer = self
            .profile
            .client_id
            .clone()
            .unwrap_or_else(|| self.profile.app_id.to_string());
        let jwt = self.key.jwt(&issuer, now)?;
        let mut run = transport::Run {
            client: self,
            requests: 0,
            bytes: 0,
            deadline: d,
        };
        let app = run.request("/app", &jwt, None, 200)?;
        ensure!(
            app.0["id"].as_u64() == Some(run.client.profile.app_id),
            BINDING
        );
        if let Some(id) = &run.client.profile.client_id {
            ensure!(app.0["client_id"].as_str() == Some(id.as_str()), BINDING);
        }
        drop(app);
        let install = run.request(
            &format!("/app/installations/{installation_id}"),
            &jwt,
            None,
            200,
        )?;
        ensure!(
            install.0["id"].as_u64() == Some(installation_id)
                && install.0["app_id"].as_u64() == Some(run.client.profile.app_id)
                && install.0.get("suspended_at").is_some_and(Value::is_null)
                && install.0.get("suspended_by").is_some_and(Value::is_null),
            BINDING
        );
        permissions(
            &install.0["permissions"],
            &run.client.profile.permissions,
            false,
        )?;
        drop(install);
        let body = serde_json::to_vec(
            &json!({"repository_ids":[repository_id],"permissions":run.client.profile.permissions}),
        )?;
        // GitHub may create a token even if a later response/output check fails.
        let mut minted = run.request(
            &format!("/app/installations/{installation_id}/access_tokens"),
            &jwt,
            Some(body),
            201,
        )?;
        let token = match minted.0["token"].take() {
            Value::String(token) => Zeroizing::new(token),
            _ => return Err(anyhow::anyhow!(BINDING)),
        };
        ensure!(
            !token.is_empty()
                && token.len() <= 16_384
                && token.bytes().all(|b| b.is_ascii_alphanumeric()
                    || matches!(b, b'_' | b'-' | b'.' | b'+' | b'/' | b'=')),
            BINDING
        );
        permissions(
            &minted.0["permissions"],
            &run.client.profile.permissions,
            true,
        )?;
        ensure!(minted.0["repository_selection"] == "selected", BINDING);
        repositories(&minted.0["repositories"], &grant)?;
        let expires_text = minted.0["expires_at"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!(BINDING))?;
        ensure!(expires_text.len() <= 40, BINDING);
        let expires = time::OffsetDateTime::parse(
            expires_text,
            &time::format_description::well_known::Rfc3339,
        )
        .map_err(|_| anyhow::anyhow!(BINDING))?
        .unix_timestamp();
        let expires = u64::try_from(expires).map_err(|_| anyhow::anyhow!(BINDING))?;
        ensure!(expires >= now + 30 && expires <= now + 3660, BINDING);
        drop(minted);
        let repos = run.request(
            "/installation/repositories?per_page=100&page=1",
            &token,
            None,
            200,
        )?;
        ensure!(repos.0["total_count"].as_u64() == Some(1), BINDING);
        repositories(&repos.0["repositories"], &grant)?;
        drop(repos);
        d.check()?;
        let checked_at = run.client.now()?;
        ensure!(checked_at >= now && expires >= checked_at + 30, BINDING);
        let evidence = json!({"schemaVersion":1,"appId":run.client.profile.app_id,
            "installationId":installation_id,"repositoryId":repository_id,"repository":grant.repository,
            "permissions":run.client.profile.permissions,"expiresAtUnix":expires,"checkedAtUnix":now,
            "apiVersion":VERSION,"requests":run.requests,"responseBytes":run.bytes,
            "fixture":run.client.fixture_clock.is_some(),"installationTokenVerified":true,
            "clockAuthority":"explicit_fixture_or_host_system_clock","tokenMaterial":"opaque_in_memory_only",
            "sourceWritten":false,"published":false,"currentHeadVerified":false,"modelCalls":0,"complete":false});
        crate::json::encode_bounded(
            &evidence,
            run.client.options.max_output_bytes.saturating_sub(1),
            d,
        )?;
        d.check()?;
        Ok(InstallationLease {
            evidence,
            _token: token,
            context: LeaseContext {
                client_identity: run.client.identity.clone(),
                origin: run.client.origin.clone(),
                app_id: run.client.profile.app_id,
                grant,
                permissions: run.client.profile.permissions.clone(),
                checked_at: now,
                expires_at: expires,
            },
            observed: AtomicU64::new(checked_at),
        })
    }
}
fn permissions(value: &Value, wanted: &BTreeMap<String, String>, exact: bool) -> Result<()> {
    let map = value.as_object().ok_or_else(|| anyhow::anyhow!(BINDING))?;
    ensure!(
        map.len() <= 100
            && wanted.iter().all(|(name, level)| map
                .get(name)
                .and_then(Value::as_str)
                .is_some_and(
                    |actual| actual == level || (!exact && level == "read" && actual == "write")
                )),
        BINDING
    );
    if exact {
        ensure!(
            map.iter().all(|(name, level)| wanted
                .get(name)
                .is_some_and(|want| level.as_str() == Some(want.as_str()))
                || (name == "metadata" && level == "read")),
            BINDING
        );
    }
    Ok(())
}
fn repositories(value: &Value, grant: &Grant) -> Result<()> {
    let rows = value.as_array().ok_or_else(|| anyhow::anyhow!(BINDING))?;
    ensure!(
        rows.len() == 1
            && rows[0]["id"].as_u64() == Some(grant.repository_id)
            && rows[0]["full_name"]
                .as_str()
                .is_some_and(|name| name.to_ascii_lowercase() == grant.repository),
        BINDING
    );
    Ok(())
}
/// Scrub decoded remote strings on all exits, including invalid token responses.
struct PrivateJson(Value);
impl Drop for PrivateJson {
    fn drop(&mut self) {
        fn scrub(value: &mut Value) {
            match value {
                Value::String(s) => s.zeroize(),
                Value::Array(a) => a.iter_mut().for_each(scrub),
                Value::Object(o) => o.values_mut().for_each(scrub),
                _ => {}
            }
        }
        scrub(&mut self.0);
    }
}
