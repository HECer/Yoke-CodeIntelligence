//! Explicit source-bound page proposals. Display data never grants send authority.
use super::{Host, worker};
use crate::{capture::Deadline, store::Store};
use anyhow::{Result, ensure};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[path = "github_app_pages_render.rs"]
mod render;
#[path = "github_app_pages_state.rs"]
mod state;
const INVALID: &str = "github_app_pages_input_invalid";
const CAPACITY: &str = "github_app_pages_capacity_exceeded";

#[derive(Clone)]
pub struct Options {
    pub allow_pages: bool,
    pub worker: worker::Options,
    pub ttl_seconds: u64,
    /// Per App; the persistent cache also has fixed workspace-wide hard bounds.
    pub max_pages: usize,
    /// All page and revision bytes in this workspace, including other Apps.
    pub max_store_bytes: usize,
    pub max_html_bytes: usize,
    pub max_output_bytes: usize,
    /// Per captured side; complete HTML and stdout byte limits remain global.
    pub max_nodes: usize,
    /// Per captured side. No observation is silently truncated to fit this cap.
    pub max_observations: usize,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            allow_pages: false,
            worker: worker::Options::default(),
            ttl_seconds: 14 * 24 * 60 * 60,
            max_pages: 500,
            max_store_bytes: 64_000_000,
            max_html_bytes: 2_000_000,
            max_output_bytes: 4_000_000,
            max_nodes: 5000,
            max_observations: 20_000,
        }
    }
}
pub const RECORD_KIND: &str = "github_app_page_v1";
pub(crate) const REVISION_KIND: &str = "github_app_page_revision_v1";
pub struct Context<'a> {
    pub store: &'a Store,
    pub host: &'a Host,
    pub options: &'a Options,
    pub clock: &'a worker::Clock,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LinkReference {
    pub page_id: String,
    pub expected_page_version: u64,
}
impl LinkReference {
    pub(crate) fn validate(&self) -> Result<()> {
        ensure!(
            crate::model::valid_id(&self.page_id)
                && (1..=9_007_199_254_740_991).contains(&self.expected_page_version),
            INVALID
        );
        Ok(())
    }
}
/// Captured host approval facts. This packet does not attest public reachability.
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct LinkBinding {
    pub schema_version: u32,
    pub page_id: String,
    pub page_version: u64,
    pub page_proposal_id: String,
    pub html_hash: String,
    pub graph_hash: String,
    pub source_binding_hash: String,
    pub source_queue_revision: usize,
    pub source_queue_checkpoint: String,
    pub stored_at_unix: u64,
    pub expires_at_unix: u64,
    pub public_base_url: String,
    pub url: String,
}
/// Opaque native approval fact; never deserialized from caller input.
#[derive(Serialize)]
#[serde(transparent)]
pub struct ApprovedLink(LinkBinding);
impl ApprovedLink {
    pub(crate) fn into_binding(self) -> LinkBinding {
        self.0
    }
}
fn checked_link(c: &Context<'_>, binding: LinkBinding, d: &Deadline) -> Result<ApprovedLink> {
    d.check()?;
    ensure!(
        c.clock.now()? < binding.expires_at_unix,
        "github_app_pages_link_stale"
    );
    d.check()?;
    Ok(ApprovedLink(binding))
}
pub fn approved_link(
    c: &Context<'_>,
    reference: &LinkReference,
    d: &Deadline,
) -> Result<ApprovedLink> {
    c.options.operation(false, d)?;
    reference.validate()?;
    let binding = state::approved_link(c, reference, d)?;
    checked_link(c, binding, d)
}
/// Only the publication core calls this inside its already-owned transaction.
/// Public page operations still refuse caller-owned SQL transactions.
pub(crate) fn approved_link_in_publication_transaction(
    c: &Context<'_>,
    reference: &LinkReference,
    d: &Deadline,
) -> Result<ApprovedLink> {
    ensure!(
        !c.store.db.is_autocommit(),
        "github_app_pages_owned_transaction_required"
    );
    c.options.operation(false, d)?;
    reference.validate()?;
    checked_link(c, state::approved_link_in_transaction(c, reference, d)?, d)
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StoreRequest {
    pub schema_version: u32,
    pub completion_id: String,
    pub expected_queue_revision: usize,
    pub public_base_url: String,
    pub proposal_id: String,
    pub html_hash: String,
    pub approval: String,
}
impl StoreRequest {
    pub fn parse(raw: &[u8]) -> Result<Self> {
        let request: Self = crate::json::parse(raw, 4096)
            .and_then(crate::json::decode)
            .map_err(|_| anyhow::anyhow!(INVALID))?;
        request.validate()?;
        Ok(request)
    }
    fn preview(&self) -> PreviewRequest {
        PreviewRequest {
            schema_version: self.schema_version,
            completion_id: self.completion_id.clone(),
            expected_queue_revision: self.expected_queue_revision,
            public_base_url: self.public_base_url.clone(),
        }
    }
    fn validate(&self) -> Result<()> {
        ensure!(
            crate::model::valid_id(&self.completion_id) && self.public_base_url.len() <= 512,
            INVALID
        );
        self.preview().validate()?;
        ensure!(
            crate::model::valid_id(&self.proposal_id) && crate::model::valid_id(&self.html_hash),
            INVALID
        );
        ensure!(
            self.approval == "store_exact_page",
            "github_app_pages_approval_required"
        );
        Ok(())
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RemoveRequest {
    pub schema_version: u32,
    pub page_id: String,
    pub expected_page_version: u64,
    pub expected_revision: u64,
    pub approval: String,
}
impl RemoveRequest {
    pub fn parse(raw: &[u8]) -> Result<Self> {
        let request: Self = crate::json::parse(raw, 4096)
            .and_then(crate::json::decode)
            .map_err(|_| anyhow::anyhow!(INVALID))?;
        request.validate()?;
        Ok(request)
    }
    fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == 1
                && crate::model::valid_id(&self.page_id)
                && (1..=9_007_199_254_740_991).contains(&self.expected_page_version)
                && (1..=9_007_199_254_740_991).contains(&self.expected_revision),
            INVALID
        );
        ensure!(
            self.approval == "remove_exact_page",
            "github_app_pages_approval_required"
        );
        Ok(())
    }
}
pub struct ReadPage {
    html: String,
    html_hash: String,
    expires_at_unix: u64,
}
impl ReadPage {
    pub fn html(&self) -> &str {
        &self.html
    }
    pub fn html_hash(&self) -> &str {
        &self.html_hash
    }
    pub(crate) fn expires_at_unix(&self) -> u64 {
        self.expires_at_unix
    }
    pub(crate) fn content_security_policy(&self) -> Result<&str> {
        self.html
            .split_once("http-equiv=\"Content-Security-Policy\" content=\"")
            .and_then(|(_, tail)| tail.split_once('"').map(|(policy, _)| policy))
            .filter(|policy| {
                policy.len() <= 4096
                    && policy.is_ascii()
                    && !policy.bytes().any(|b| b < 32 || b == 127)
            })
            .ok_or_else(|| anyhow::anyhow!(INVALID))
    }
}
pub fn store_page(c: &Context<'_>, r: &StoreRequest, d: &Deadline) -> Result<Value> {
    c.options.operation(true, d)?;
    r.validate()?;
    state::store(c, r, d)
}
pub fn read_page(
    c: &Context<'_>,
    id: &str,
    supplied_token: &str,
    d: &Deadline,
) -> Result<Option<ReadPage>> {
    c.options.operation(false, d)?;
    if !valid_read_token(c.host, &c.store.workspace_id, id, supplied_token)? {
        return Ok(None);
    }
    let page = state::read(c, id, d)?;
    d.check()?;
    let now = c.clock.now()?;
    d.check()?;
    Ok(page.filter(|page| now < page.expires_at_unix))
}
pub fn remove_page(c: &Context<'_>, r: &RemoveRequest, d: &Deadline) -> Result<Value> {
    c.options.operation(true, d)?;
    r.validate()?;
    state::remove(c, r, d)
}
impl Options {
    pub(crate) fn admit_read(&self, d: &Deadline) -> Result<()> {
        self.operation(false, d)
    }
    fn operation(&self, write: bool, d: &Deadline) -> Result<()> {
        ensure!(self.allow_pages, "github_app_pages_permission_required");
        self.worker.admit(write, d)?;
        self.admit(d)
    }
    /// Admit projection and cache limits before selecting host files or opening a Store.
    pub fn admit(&self, d: &Deadline) -> Result<()> {
        d.check()?;
        self.worker.admit(false, d)?;
        ensure!(
            self.max_html_bytes > 0
                && self.max_output_bytes > 0
                && self.max_nodes > 0
                && self.max_observations > 0
                && self.ttl_seconds > 0
                && self.max_pages > 0
                && self.max_store_bytes > 0,
            CAPACITY
        );
        ensure!(
            self.max_html_bytes <= 4_000_000
                && self.max_output_bytes <= 16_000_000
                && self.max_nodes <= 10_000
                && self.max_observations <= 40_000
                && self.ttl_seconds <= 90 * 24 * 60 * 60
                && self.max_pages <= 1000
                && self.max_store_bytes <= 128_000_000,
            INVALID
        );
        Ok(())
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviewRequest {
    pub schema_version: u32,
    pub completion_id: String,
    pub expected_queue_revision: usize,
    pub public_base_url: String,
}
impl PreviewRequest {
    pub fn parse(raw: &[u8]) -> Result<Self> {
        let request: Self = crate::json::parse(raw, 4096)
            .and_then(crate::json::decode)
            .map_err(|_| anyhow::anyhow!(INVALID))?;
        request.validate()?;
        Ok(request)
    }
    fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == 1
                && crate::model::valid_id(&self.completion_id)
                && (1..=2000).contains(&self.expected_queue_revision)
                && self.public_base_url.len() <= 512
                && self.public_base_url.is_ascii(),
            INVALID
        );
        let origin =
            url::Url::parse(&self.public_base_url).map_err(|_| anyhow::anyhow!(INVALID))?;
        ensure!(
            origin.scheme() == "https"
                && origin.host_str().is_some()
                && origin.username().is_empty()
                && origin.password().is_none()
                && origin.query().is_none()
                && origin.fragment().is_none()
                && origin.path() == "/"
                && origin.origin().ascii_serialization() == self.public_base_url,
            INVALID
        );
        Ok(())
    }
}
fn bounded(value: &Value, limit: usize, d: &Deadline) -> Result<Vec<u8>> {
    crate::json::encode_bounded(value, limit, d)
        .map_err(|_| d.check().err().unwrap_or_else(|| anyhow::anyhow!(CAPACITY)))
}
/// The source factory has already replayed native analysis against exact bytes.
/// Only this closed display projection crosses into HTML; signatures and source
/// excerpts are deliberately absent. Parallel directed observations are retained.
fn section(impact: &Value, o: &Options, d: &Deadline) -> Result<Value> {
    let mut nodes = BTreeMap::<String, Value>::new();
    let mut bytes = 0usize;
    for (key, role) in [
        ("changedSymbols", "changed"),
        ("incomingCandidates", "incoming"),
        ("pathCandidates", "path"),
    ] {
        let values = impact[key]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!(INVALID))?;
        ensure!(values.len() <= o.max_nodes, CAPACITY);
        for value in values {
            d.check()?;
            let id = value["id"]
                .as_str()
                .filter(|id| crate::model::valid_id(id))
                .ok_or_else(|| anyhow::anyhow!(INVALID))?;
            if let Some(existing) = nodes.get_mut(id) {
                existing["roles"]
                    .as_array_mut()
                    .expect("closed roles")
                    .push(json!(role));
                continue;
            }
            ensure!(nodes.len() < o.max_nodes, CAPACITY);
            let mut node = json!({"roles":[role]});
            for field in [
                "id",
                "snapshotId",
                "path",
                "name",
                "qualifiedName",
                "kind",
                "language",
                "startByte",
                "endByte",
                "startLine",
                "endLine",
                "contentHash",
                "producer",
            ] {
                let field_value = value.get(field).ok_or_else(|| anyhow::anyhow!(INVALID))?;
                node[field] = field_value.clone();
            }
            bytes = bytes
                .checked_add(bounded(&node, o.max_html_bytes, d)?.len())
                .ok_or_else(|| anyhow::anyhow!(CAPACITY))?;
            ensure!(bytes <= o.max_html_bytes, CAPACITY);
            nodes.insert(id.into(), node);
        }
    }
    let observations = impact["observations"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!(INVALID))?;
    let evidence = impact["evidence"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!(INVALID))?;
    ensure!(
        observations.len() <= o.max_observations && evidence.len() <= 40_000,
        CAPACITY
    );
    // Validate the retained endpoints and count before duplicating the arrays.
    for edge in observations {
        d.check()?;
        ensure!(
            edge["source"]
                .as_str()
                .is_some_and(|id| nodes.contains_key(id))
                && edge["target"]
                    .as_str()
                    .is_some_and(|id| nodes.contains_key(id)),
            INVALID
        );
    }
    let value = json!({"snapshotId":impact["snapshotId"],"graphHash":impact["graphHash"],
        "nodes":nodes.into_values().collect::<Vec<_>>(),"observations":observations,"evidence":evidence});
    bounded(&value, o.max_html_bytes, d)?;
    Ok(value)
}
fn projected_graph(head: &Value, base: Option<&Value>, o: &Options, d: &Deadline) -> Result<Value> {
    Ok(json!({"head":section(head,o,d)?,"base":base.map(|impact|section(impact,o,d)).transpose()?}))
}
/// Fixed-bound structural identity, never page, worker or publication authority.
/// Valid non-pageable completions remain valid history; only their pages refuse.
pub(crate) fn retained_graph_hash(
    head: &Value,
    base: Option<&Value>,
    d: &Deadline,
) -> Result<Option<String>> {
    let o = Options {
        max_html_bytes: 4_000_000,
        max_output_bytes: 16_000_000,
        max_nodes: 10_000,
        max_observations: 40_000,
        ..Default::default()
    };
    let projected =
        projected_graph(head, base, &o, d).and_then(|graph| bounded(&graph, 4_000_000, d));
    match projected {
        Ok(bytes) => {
            d.check()?;
            Ok(Some(crate::model::hash(&bytes)))
        }
        Err(_) => {
            d.check()?;
            Ok(None)
        }
    }
}
pub(crate) fn validated_inline_records(
    store: &Store,
    worker_roots: &worker::RetainedRoots,
    d: &Deadline,
) -> Result<std::collections::BTreeSet<(String, String)>> {
    state::validated_inline_records(store, worker_roots, d)
}
fn access_mac(host: &Host, workspace: &str, page_id: &str) -> Result<Hmac<sha2::Sha256>> {
    let mut mac = Hmac::<sha2::Sha256>::new_from_slice(&host.keys[0].0)
        .map_err(|_| anyhow::anyhow!(INVALID))?;
    mac.update(b"yoke-code-intelligence/github-app-page-access/v1\n");
    mac.update(&serde_json::to_vec(
        &json!({"workspaceId":workspace,"appId":host.profile.app_id,
        "pageId":page_id}),
    )?);
    Ok(mac)
}
pub(crate) fn valid_read_token(
    host: &Host,
    workspace: &str,
    id: &str,
    supplied: &str,
) -> Result<bool> {
    if !crate::model::valid_id(id) || !crate::model::valid_id(supplied) {
        return Ok(false);
    }
    let supplied = hex::decode(supplied).map_err(|_| anyhow::anyhow!(INVALID))?;
    Ok(access_mac(host, workspace, id)?
        .verify_slice(&supplied)
        .is_ok())
}
fn token(host: &Host, workspace: &str, page_id: &str) -> Result<String> {
    Ok(hex::encode(
        access_mac(host, workspace, page_id)?
            .finalize()
            .into_bytes(),
    ))
}
pub fn preview(
    store: &Store,
    host: &Host,
    request: &PreviewRequest,
    o: &Options,
    d: &Deadline,
) -> Result<Value> {
    o.admit(d)?;
    request.validate()?;
    state::preview(store, host, request, o, d)
}
struct PlanVersion {
    revision: u64,
    previous: Option<u64>,
}
fn proposal(
    store: &Store,
    host: &Host,
    source: &worker::PublicationSource,
    request: &PreviewRequest,
    o: &Options,
    version: PlanVersion,
    d: &Deadline,
) -> Result<Value> {
    let job = source.job();
    let facts = source.facts();
    let (head, base) = source.page_impacts();
    let graph = projected_graph(head, base, o, d)?;
    let page_id = crate::model::identity(&json!({"domain":"yoke-github-page-v1",
        "workspaceId":store.workspace_id,"appId":job.app_id,"repositoryId":job.repository_id,"number":job.number}));
    let review_hash = source.review_hash()?;
    let display = json!({"schemaVersion":1,"rendererVersion":"yoke-pr-page-v1",
        "workspaceId":store.workspace_id,"appId":job.app_id,"installationId":job.installation_id,
        "repositoryId":job.repository_id,"repository":job.repository,"number":job.number,
        "jobId":job.id,"completionId":source.completion_id(),"queueRevision":source.queue_revision(),
        "queueCheckpoint":source.queue_checkpoint(),"headSha":job.head_sha,"baseSha":job.base_sha,
        "headSnapshotId":facts["headSnapshotId"],"baseSnapshotId":facts["baseSnapshotId"],
        "reviewHash":review_hash,"coverage":facts["coverage"],"bindings":facts["bindings"],
        "graph":graph,"certainty":"source_bound_structural_candidates_not_compiler_runtime_or_merge_proof",
        "complete":false});
    let html = render::html(&display, o.max_html_bytes, d)?;
    let mut proposal = display;
    let object = proposal.as_object_mut().expect("closed page display");
    object.extend([
        ("pageRevision".into(),json!(version.revision)),
        ("previousPageVersion".into(),json!(version.previous)),
        ("cachePolicy".into(),json!({"ttlSeconds":o.ttl_seconds,"maxPages":o.max_pages,"maxStoreBytes":o.max_store_bytes})),
        ("pageId".into(), json!(page_id)),
        ("publicBaseUrl".into(), json!(request.public_base_url)),
        (
            "url".into(),
            json!(format!(
                "{}/p/{}?t={}",
                request.public_base_url,
                page_id,
                token(host, &store.workspace_id, &page_id)?
            )),
        ),
        (
            "htmlHash".into(),
            json!(crate::model::hash(html.as_bytes())),
        ),
        ("html".into(), json!(html)),
    ]);
    let proposal_id = crate::model::identity(&proposal);
    proposal
        .as_object_mut()
        .expect("closed page proposal")
        .extend([
            ("proposalId".into(), json!(proposal_id)),
            ("persisted".into(), json!(false)),
            ("published".into(), json!(false)),
            ("pageWriteAuthority".into(), json!(false)),
            (
                "bearerReadCapability".into(),
                json!("conditional_on_stored_unexpired_page_current_key_and_repository_grant"),
            ),
            ("publicationAuthority".into(), json!(false)),
            ("sourceWritten".into(), json!(false)),
            ("currentHeadVerified".into(), json!(false)),
            ("networkRequests".into(), json!(0)),
            ("modelCalls".into(), json!(0)),
        ]);
    bounded(&proposal, o.max_output_bytes - 1, d)?;
    Ok(proposal)
}
