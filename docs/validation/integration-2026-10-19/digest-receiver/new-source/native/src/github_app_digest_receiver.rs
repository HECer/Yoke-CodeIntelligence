//! Authenticated opaque digest intake. Incoming snapshot/graph IDs carry no
//! local source, retention, model, execution or publication authority.
use super::*;
use crate::federation_http::TlsMaterial;
use std::{
    collections::{BTreeMap, BTreeSet},
    net::{SocketAddr, TcpListener},
    sync::{
        Arc, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};
use subtle::ConstantTimeEq;
#[path = "github_app_digest_receiver_http.rs"]
mod http;
#[path = "github_app_digest_receiver_store.rs"]
mod storage;
pub const RECORD_KIND: &str = "github_app_digest_receiver_v1";
const INVALID: &str = "github_app_digest_receiver_input_invalid";
const BINDING: &str = "github_app_digest_receiver_binding_invalid";
const CAPACITY: &str = "github_app_digest_receiver_capacity_exceeded";

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Access {
    pub id: String,
    /// SHA256 of the decoded 32-byte bearer token, never the bearer itself.
    pub token_hash: String,
    pub app_id: u64,
    pub sender_workspace_id: String,
    pub installation_id: u64,
    pub repository_id: u64,
    pub repository: String,
}
impl Access {
    fn permits(&self, s: &Submission) -> bool {
        self.permits_scope(&s.scope())
    }
    fn permits_scope(&self, s: &Scope) -> bool {
        self.app_id == s.app_id
            && self.sender_workspace_id == s.workspace_id
            && s.installation_id == self.installation_id
            && s.repository_id == self.repository_id
            && s.repository == self.repository
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Profile {
    pub schema_version: u32,
    pub endpoint: String,
    pub grants: Vec<Access>,
    pub max_payload_bytes: usize,
    pub max_records: usize,
    pub max_stored_bytes: usize,
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
        // Reuse the client's exact origin syntax without acquiring a credential.
        let client = super::Profile {
            schema_version: 1,
            endpoint: self.endpoint.clone(),
            credential_name: if self.endpoint.starts_with("https:") {
                Some("validation".into())
            } else {
                None
            },
            grants: vec![Grant {
                installation_id: 1,
                repository_id: 1,
                repository: "validation/repo".into(),
            }],
            max_payload_bytes: 4_000_000,
            max_response_bytes: 64_000,
            max_journal_records: 2,
            max_journal_bytes: 16384,
        };
        client.validate().map_err(|_| anyhow::anyhow!(INVALID))?;
        client.origin().map_err(|_| anyhow::anyhow!(INVALID))
    }
    fn validate(&self) -> Result<()> {
        self.origin()?;
        let mut ids = BTreeSet::new();
        let mut scopes = BTreeSet::new();
        ensure!(
            self.schema_version == 1
                && (1..=32).contains(&self.grants.len())
                && self
                    .grants
                    .iter()
                    .all(|g| crate::github_app::credential_name(&g.id)
                        && ids.insert(&g.id)
                        && valid_id(&g.token_hash)
                        && valid_id(&g.sender_workspace_id)
                        && crate::github_app::numeric(g.app_id)
                        && crate::github_app::numeric(g.installation_id)
                        && crate::github_app::numeric(g.repository_id)
                        && crate::github_app::repository(&g.repository)
                        && scopes.insert((
                            &g.token_hash,
                            g.app_id,
                            &g.sender_workspace_id,
                            g.installation_id,
                            g.repository_id
                        )))
                && (1..=4_000_000).contains(&self.max_payload_bytes)
                && (1..=1000).contains(&self.max_records)
                && (1024..=32_000_000).contains(&self.max_stored_bytes),
            INVALID
        );
        Ok(())
    }
}
pub struct Config {
    pub allow_receiver: bool,
    pub allow_network: bool,
    pub allow_writes: bool,
    pub allow_remote: bool,
    pub allow_fixture: bool,
    pub listen: SocketAddr,
    pub tls: Option<TlsMaterial>,
    pub request_timeout_ms: u64,
    pub lifetime_ms: u64,
    pub max_requests: usize,
    pub max_response_bytes: usize,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            allow_receiver: false,
            allow_network: false,
            allow_writes: false,
            allow_remote: false,
            allow_fixture: false,
            listen: "127.0.0.1:0".parse().expect("literal"),
            tls: None,
            request_timeout_ms: 5000,
            lifetime_ms: 120_000,
            max_requests: 1000,
            max_response_bytes: 4096,
        }
    }
}
impl Config {
    pub fn admit_profile_selection(
        &self,
        profile: &Profile,
        tls_selected: bool,
        d: &Deadline,
    ) -> Result<()> {
        self.admit_selection(tls_selected, d)?;
        profile.validate()?;
        let origin = profile.origin()?;
        ensure!((origin.scheme() == "https") == tls_selected, INVALID);
        ensure!(
            origin.port_or_known_default() == Some(self.listen.port())
                || (self.allow_fixture && self.listen.port() == 0),
            INVALID
        );
        Ok(())
    }
    pub fn admit_flags(&self, d: &Deadline) -> Result<()> {
        self.admit_selection(self.tls.is_some(), d)
    }
    pub fn admit_selection(&self, tls_selected: bool, d: &Deadline) -> Result<()> {
        ensure!(
            self.allow_receiver && self.allow_network && self.allow_writes,
            "github_app_digest_receiver_permission_required"
        );
        d.check()?;
        ensure!(
            (10..=10_000).contains(&self.request_timeout_ms)
                && (10..=86_400_000).contains(&self.lifetime_ms)
                && (1..=10_000).contains(&self.max_requests)
                && (512..=65_536).contains(&self.max_response_bytes),
            CAPACITY
        );
        ensure!(
            self.listen.ip().is_loopback() || self.allow_remote,
            "github_app_digest_receiver_remote_permission_required"
        );
        ensure!(
            tls_selected || (self.listen.ip().is_loopback() && self.allow_fixture),
            "github_app_digest_receiver_tls_required"
        );
        Ok(())
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Submission {
    schema_version: u32,
    kind: String,
    workspace_id: String,
    app_id: u64,
    digest: Value,
}
struct Scope {
    app_id: u64,
    workspace_id: String,
    installation_id: u64,
    repository_id: u64,
    repository: String,
}
fn keys(v: &Value, allowed: &[&str]) -> Result<()> {
    ensure!(
        v.as_object()
            .is_some_and(|m| m.keys().all(|k| allowed.contains(&k.as_str()))),
        INVALID
    );
    Ok(())
}
impl Submission {
    fn scope(&self) -> Scope {
        Scope {
            app_id: self.app_id,
            workspace_id: self.workspace_id.clone(),
            installation_id: self.digest["payload"]["installationId"]
                .as_u64()
                .expect("validated scope"),
            repository_id: self.digest["payload"]["repositoryId"]
                .as_u64()
                .expect("validated scope"),
            repository: self.digest["payload"]["repository"]
                .as_str()
                .expect("validated scope")
                .into(),
        }
    }
    fn parse(raw: &[u8], cap: usize, d: &Deadline) -> Result<Self> {
        d.check()?;
        let s: Self = crate::json::decode(
            crate::json::parse(raw, cap).map_err(|_| anyhow::anyhow!(INVALID))?,
        )
        .map_err(|_| anyhow::anyhow!(INVALID))?;
        ensure!(
            s.schema_version == 1
                && s.kind == "native_digest_submission_v1"
                && valid_id(&s.workspace_id)
                && crate::github_app::numeric(s.app_id),
            INVALID
        );
        let v = &s.digest;
        keys(
            v,
            &[
                "schemaVersion",
                "kind",
                "id",
                "payload",
                "reviewRequired",
                "serviceSend",
                "networkRequests",
                "modelCalls",
                "sourceWritten",
            ],
        )?;
        ensure!(
            v["schemaVersion"] == 1
                && v["kind"] == "native_source_bound_digest_handoff_preview"
                && v["reviewRequired"] == true
                && v["serviceSend"] == "unavailable"
                && v["networkRequests"] == 0
                && v["modelCalls"] == 0
                && v["sourceWritten"] == false,
            INVALID
        );
        let p = &v["payload"];
        keys(
            p,
            &[
                "snapshotId",
                "installationId",
                "repositoryId",
                "repository",
                "sourceCommitId",
                "sourceTreeId",
                "sourceCommitVerified",
                "sourceDigestHash",
                "discussionCaptureId",
                "symbols",
                "threads",
                "symbolCandidates",
                "symbolOmissions",
                "unsupportedFiles",
                "visibilityUnresolved",
                "analyzer",
                "nativeEntityReplayScope",
                "commits",
                "commitMessagesIncluded",
                "discussionCoverageComplete",
                "unacquiredPulls",
                "discussionAcquiredAtUnix",
                "discussionTextIncluded",
                "implementationBodyScope",
                "scope",
                "providerVerified",
                "complete",
            ],
        )?;
        ensure!(
            ["snapshotId", "sourceDigestHash", "discussionCaptureId"]
                .iter()
                .all(|k| p[*k].as_str().is_some_and(valid_id))
                && p["installationId"]
                    .as_u64()
                    .is_some_and(crate::github_app::numeric)
                && p["repositoryId"]
                    .as_u64()
                    .is_some_and(crate::github_app::numeric)
                && p["repository"]
                    .as_str()
                    .is_some_and(crate::github_app::repository)
                && p["commitMessagesIncluded"] == false
                && p["discussionTextIncluded"] == false
                && p["providerVerified"] == false
                && p["complete"] == false
                && p["implementationBodyScope"]
                    == "top_level_function_bodies_and_const_static_initializers_excluded"
                && p["scope"] == "verified_captured_rust_headers_and_imported_discussion_metadata",
            INVALID
        );
        for row in p["symbols"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!(INVALID))?
        {
            d.check()?;
            keys(
                row,
                &[
                    "path",
                    "name",
                    "kind",
                    "signature",
                    "fingerprint",
                    "startByte",
                    "endByte",
                    "producer",
                    "sourceHash",
                ],
            )?;
        }
        for row in p["threads"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!(INVALID))?
        {
            d.check()?;
            keys(
                row,
                &[
                    "number",
                    "providerId",
                    "headSha",
                    "baseSha",
                    "closedAt",
                    "updatedAt",
                    "merged",
                    "coverage",
                    "observations",
                    "sourceAssociation",
                ],
            )?;
            for observation in row["observations"]
                .as_array()
                .ok_or_else(|| anyhow::anyhow!(INVALID))?
            {
                keys(
                    observation,
                    &[
                        "kind",
                        "id",
                        "bodyHash",
                        "createdAt",
                        "updatedAt",
                        "submittedAt",
                        "commitId",
                        "originalCommitId",
                        "inReplyToId",
                        "reviewId",
                        "state",
                    ],
                )?;
            }
        }
        for row in p["commits"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!(INVALID))?
        {
            d.check()?;
            keys(
                row,
                &[
                    "sha",
                    "treeId",
                    "parentIds",
                    "committerTimestamp",
                    "committerUnixSeconds",
                    "filesStatus",
                    "symbolsStatus",
                    "mappedSnapshotId",
                    "parentSnapshotId",
                ],
            )?;
        }
        let encoded = crate::json::encode_bounded(p, 4_000_000, d)?;
        ensure!(
            v["id"]
                .as_str()
                .is_some_and(|id| valid_id(id) && hash(&encoded) == id),
            BINDING
        );
        d.check()?;
        Ok(s)
    }
    fn proposal(&self, endpoint: &str, raw: &[u8]) -> String {
        identity(
            &json!({"schemaVersion":1,"workspaceId":self.workspace_id,"appId":self.app_id,"endpoint":endpoint,"payloadHash":hash(raw)}),
        )
    }
}
struct Shared {
    profile: Profile,
    config: Config,
    tls: Option<Arc<rustls::ServerConfig>>,
    revoked: Arc<RwLock<BTreeSet<String>>>,
    stop: Arc<AtomicBool>,
}
pub struct ServerHandle {
    address: SocketAddr,
    announcement: Value,
    stop: Arc<AtomicBool>,
    revoked: Arc<RwLock<BTreeSet<String>>>,
    worker: Option<JoinHandle<Result<Value>>>,
}
impl ServerHandle {
    pub fn local_addr(&self) -> SocketAddr {
        self.address
    }
    pub fn announcement(&self) -> &Value {
        &self.announcement
    }
    pub fn endpoint(&self) -> &str {
        self.announcement["endpoint"]
            .as_str()
            .expect("owned announcement")
    }
    pub fn revoke(&self, id: &str, d: &Deadline) -> Result<()> {
        ensure!(crate::github_app::credential_name(id), INVALID);
        loop {
            d.check()?;
            match self.revoked.try_write() {
                Ok(mut lock) => {
                    lock.insert(id.to_owned());
                    return Ok(());
                }
                Err(std::sync::TryLockError::WouldBlock) => {
                    thread::sleep(d.remaining().min(Duration::from_millis(2)))
                }
                Err(_) => anyhow::bail!(BINDING),
            }
        }
    }
    pub fn shutdown(mut self) -> Result<Value> {
        self.stop.store(true, Ordering::SeqCst);
        self.finish()
    }
    pub fn wait(mut self) -> Result<Value> {
        self.finish()
    }
    fn finish(&mut self) -> Result<Value> {
        self.worker
            .take()
            .ok_or_else(|| anyhow::anyhow!(INVALID))?
            .join()
            .map_err(|_| anyhow::anyhow!(BINDING))?
    }
}
impl Drop for ServerHandle {
    fn drop(&mut self) {
        if self.worker.is_some() {
            self.stop.store(true, Ordering::SeqCst);
            if self.finish().is_err() {
                eprintln!("github_app_digest_receiver_cleanup_unverified");
            }
        }
    }
}
pub fn start(
    store: Store,
    mut profile: Profile,
    mut config: Config,
    d: &Deadline,
) -> Result<ServerHandle> {
    config.admit_profile_selection(&profile, config.tls.is_some(), d)?;
    ensure!(store.db.is_autocommit(), BINDING);
    storage::validate(&store, &profile, d)?;
    let tls = config
        .tls
        .take()
        .map(crate::github_app::ingress::socket::tls_server)
        .transpose()?;
    let listener = TcpListener::bind(config.listen)
        .map_err(|_| anyhow::anyhow!("github_app_digest_receiver_bind_failed"))?;
    let address = listener.local_addr()?;
    listener.set_nonblocking(true)?;
    if config.allow_fixture && config.listen.port() == 0 {
        let mut origin = profile.origin()?;
        origin
            .set_port(Some(address.port()))
            .map_err(|_| anyhow::anyhow!(INVALID))?;
        profile.endpoint = origin.to_string();
    }
    let stop = Arc::new(AtomicBool::new(false));
    let revoked = Arc::new(RwLock::new(BTreeSet::new()));
    let lifetime = Deadline::run_bounded(d, Arc::clone(&stop), || {
        Ok(Deadline::new(config.lifetime_ms))
    })?;
    let announcement = json!({"schemaVersion":1,"kind":"native_digest_receiver","endpoint":profile.endpoint,"listen":address.to_string(),"tls":tls.is_some(),"sourceAuthority":false,"processingAvailable":false,"modelCalls":0,"maxPayloadBytes":profile.max_payload_bytes});
    let shared = Shared {
        profile,
        config,
        tls,
        revoked: Arc::clone(&revoked),
        stop: Arc::clone(&stop),
    };
    let worker = thread::spawn(move || {
        let mut requests = 0usize;
        while !shared.stop.load(Ordering::SeqCst)
            && lifetime.check().is_ok()
            && requests < shared.config.max_requests
        {
            let stream = match listener.accept() {
                Ok((s, _)) => s,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(lifetime.remaining().min(Duration::from_millis(5)));
                    continue;
                }
                Err(_) => anyhow::bail!(BINDING),
            };
            requests += 1;
            let _ = Deadline::run_bounded(&lifetime, Arc::clone(&shared.stop), || {
                let request = Deadline::new(shared.config.request_timeout_ms);
                http::serve(stream, &store, &shared, &request)
            });
        }
        Ok(
            json!({"requests":requests,"modelCalls":0,"sourceWritten":false,"processingAvailable":false}),
        )
    });
    Ok(ServerHandle {
        address,
        announcement,
        stop,
        revoked,
        worker: Some(worker),
    })
}
pub(crate) fn retained_records(store: &Store, d: &Deadline) -> Result<BTreeSet<String>> {
    storage::retained(store, d)
}
