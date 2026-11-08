//! Deterministic received-metadata processing. No local source authority, model,
//! semantic identity, compiler binding or execution follows from a sender claim.
use super::*;
use serde::{Deserialize, Serialize};
#[path = "github_app_digest_query_client.rs"]
pub mod client;
const INVALID: &str = "github_app_digest_query_input_invalid";
const CAPACITY: &str = "github_app_digest_query_capacity_exceeded";
const UNAVAILABLE: &str = "github_app_digest_query_selection_unavailable";
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Symbol,
    Commit,
    Pull,
    Observation,
    Relation,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub schema_version: u32,
    pub app_id: u64,
    pub sender_workspace_id: String,
    pub installation_id: u64,
    pub repository_id: u64,
    pub repository: String,
    pub proposal_ids: Vec<String>,
    pub kinds: Vec<Kind>,
    pub text: String,
    pub offset: usize,
    pub limit: usize,
}
impl Request {
    pub fn parse(raw: &[u8]) -> Result<Self> {
        let r: Self = crate::json::decode(
            crate::json::parse(raw, 8192).map_err(|_| anyhow::anyhow!(INVALID))?,
        )
        .map_err(|_| anyhow::anyhow!(INVALID))?;
        r.validate()?;
        Ok(r)
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == 1
                && crate::github_app::numeric(self.app_id)
                && valid_id(&self.sender_workspace_id)
                && crate::github_app::numeric(self.installation_id)
                && crate::github_app::numeric(self.repository_id)
                && crate::github_app::repository(&self.repository)
                && (1..=32).contains(&self.proposal_ids.len())
                && self.proposal_ids.iter().all(|id| valid_id(id))
                && self.proposal_ids.iter().collect::<BTreeSet<_>>().len()
                    == self.proposal_ids.len()
                && (1..=5).contains(&self.kinds.len())
                && self.kinds.iter().collect::<BTreeSet<_>>().len() == self.kinds.len()
                && self.text.len() <= 1024
                && self.text.chars().count() <= 256
                && !self.text.chars().any(char::is_control)
                && self.offset <= 32_000
                && (1..=1000).contains(&self.limit),
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
    fn permits_access(&self, a: &Access) -> bool {
        a.allow_query
            && self.app_id == a.app_id
            && self.sender_workspace_id == a.sender_workspace_id
            && self.installation_id == a.installation_id
            && self.repository_id == a.repository_id
            && self.repository == a.repository
    }
}
/// Host-only work/output admission; never imported from a remote query body.
pub struct Options {
    pub allow_query: bool,
    pub max_visited: usize,
    pub max_search_bytes: usize,
    pub max_output_bytes: usize,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            allow_query: false,
            max_visited: 32_000,
            max_search_bytes: 32_000_000,
            max_output_bytes: 32_000,
        }
    }
}
impl Options {
    fn admit(&self, d: &Deadline) -> Result<()> {
        ensure!(
            self.allow_query,
            "github_app_digest_query_permission_required"
        );
        d.check()?;
        ensure!(
            (1..=32_000).contains(&self.max_visited)
                && self.max_search_bytes <= 32_000_000
                && (1024..=65_536).contains(&self.max_output_bytes),
            CAPACITY
        );
        Ok(())
    }
}
struct Page<'a> {
    request: &'a Request,
    options: &'a Options,
    deadline: &'a Deadline,
    rows: Vec<Value>,
    visited: usize,
    matches: usize,
    search_bytes: usize,
    receipts: BTreeMap<String, String>,
}
impl Page<'_> {
    fn row(
        &mut self,
        record: &storage::Record,
        kind: Kind,
        pointer: &str,
        context: Value,
        data: Value,
    ) -> Result<()> {
        self.deadline.check()?;
        if !self.request.kinds.contains(&kind) {
            return Ok(());
        }
        ensure!(self.visited < self.options.max_visited, CAPACITY);
        self.visited += 1;
        let metadata = json!({"context":context,"data":data});
        if !self.request.text.is_empty() {
            let cap = self
                .options
                .max_search_bytes
                .saturating_sub(self.search_bytes);
            let raw = crate::json::encode_bounded(&metadata, cap, self.deadline)
                .map_err(|_| anyhow::anyhow!(CAPACITY))?;
            self.search_bytes += raw.len();
            if !std::str::from_utf8(&raw)
                .map_err(|_| anyhow::anyhow!(INVALID))?
                .contains(&self.request.text)
            {
                return Ok(());
            }
        }
        let ordinal = self.matches;
        self.matches += 1;
        if ordinal < self.request.offset || self.rows.len() >= self.request.limit {
            return Ok(());
        }
        let receipt = self
            .receipts
            .entry(record.proposal_id.clone())
            .or_insert_with(|| identity(record))
            .clone();
        let id = row_id(&receipt, kind, pointer);
        self.rows.push(json!({"id":id,"kind":kind,"proposalId":record.proposal_id,"receiptId":receipt,"payloadHash":record.payload_hash,
            "pointer":pointer,"anchorGranularity":"json_pointer_into_received_digest","metadata":metadata,
            "provenance":"authenticated_sender_claim_unverified_source","sourceAuthority":false,"semanticIdentityEstablished":false}));
        Ok(())
    }
}
fn row_id(receipt: &str, kind: Kind, pointer: &str) -> String {
    identity(
        &json!({"domain":"yoke-received-digest-row/v1","receiptId":receipt,"kind":kind,"pointer":pointer}),
    )
}
fn fields(v: &Value, keys: &[&str]) -> Value {
    Value::Object(
        keys.iter()
            .filter_map(|k| v.get(*k).map(|v| ((*k).into(), v.clone())))
            .collect(),
    )
}
fn array<'a>(v: &'a Value, name: &str) -> Result<&'a [Value]> {
    v[name]
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| anyhow::anyhow!("github_app_digest_query_metadata_unavailable"))
}
fn project(record: &storage::Record, page: &mut Page<'_>) -> Result<Value> {
    let s = Submission::parse(record.body.as_bytes(), 4_000_000, page.deadline)?;
    let p = &s.digest["payload"];
    let receipt = record.receipt();
    let scope = json!({"appId":s.app_id,"senderWorkspaceId":s.workspace_id,"installationId":p["installationId"],"repositoryId":p["repositoryId"],"repository":p["repository"],"reportedSnapshotId":p["snapshotId"]});
    for (i, row) in array(p, "symbols")?.iter().enumerate() {
        page.row(
            record,
            Kind::Symbol,
            &format!("/digest/payload/symbols/{i}"),
            scope.clone(),
            row.clone(),
        )?;
    }
    let commits = array(p, "commits")?;
    let shas = commits
        .iter()
        .filter_map(|c| c["sha"].as_str())
        .collect::<BTreeSet<_>>();
    for (i, row) in commits.iter().enumerate() {
        page.row(
            record,
            Kind::Commit,
            &format!("/digest/payload/commits/{i}"),
            scope.clone(),
            row.clone(),
        )?;
        ensure!(
            row["sha"].as_str().is_some(),
            "github_app_digest_query_metadata_unavailable"
        );
        {
            for (j, parent) in array(row, "parentIds")?.iter().enumerate() {
                ensure!(
                    parent.as_str().is_some(),
                    "github_app_digest_query_metadata_unavailable"
                );
                page.row(record,Kind::Relation,&format!("/digest/payload/commits/{i}/parentIds/{j}"),scope.clone(),
                json!({"relation":"reported_commit_parent","from":{"commitSha":row["sha"]},"to":{"commitSha":parent,"presentInSelectedMetadata":parent.as_str().is_some_and(|s|shas.contains(s))},"resolution":"sender_claim_not_verified_git_objects"}))?;
            }
        }
    }
    for (i, pull) in array(p, "threads")?.iter().enumerate() {
        page.deadline.check()?;
        let mut context = scope.clone();
        context["pullNumber"] = pull["number"].clone();
        page.row(
            record,
            Kind::Pull,
            &format!("/digest/payload/threads/{i}"),
            context.clone(),
            fields(
                pull,
                &[
                    "number",
                    "providerId",
                    "headSha",
                    "baseSha",
                    "closedAt",
                    "updatedAt",
                    "merged",
                    "coverage",
                ],
            ),
        )?;
        page.row(record,Kind::Relation,&format!("/digest/payload/threads/{i}/sourceAssociation"),context.clone(),json!({"relation":"reported_source_association","from":{"pullNumber":pull["number"],"headSha":pull["headSha"]},"claim":pull["sourceAssociation"],"resolution":"sender_claim_no_receiver_source_binding"}))?;
        let observations = array(pull, "observations")?;
        ensure!(
            pull["number"].as_u64().is_some(),
            "github_app_digest_query_metadata_unavailable"
        );
        let ids = observations
            .iter()
            .filter_map(|o| Some((o["kind"].as_str()?, o["id"].as_u64()?)))
            .collect::<BTreeSet<_>>();
        for (j, o) in observations.iter().enumerate() {
            ensure!(
                o["kind"].as_str().is_some() && o["id"].as_u64().is_some(),
                "github_app_digest_query_metadata_unavailable"
            );
            page.row(
                record,
                Kind::Observation,
                &format!("/digest/payload/threads/{i}/observations/{j}"),
                context.clone(),
                o.clone(),
            )?;
            for (field, relation, target_kind) in [
                ("inReplyToId", "reported_in_reply_to", "review_comment"),
                ("reviewId", "reported_review_membership", "review"),
            ] {
                ensure!(
                    o[field].is_null() || o[field].as_u64().is_some(),
                    "github_app_digest_query_metadata_unavailable"
                );
                if let Some(target) = o[field].as_u64() {
                    page.row(record,Kind::Relation,&format!("/digest/payload/threads/{i}/observations/{j}/{field}"),context.clone(),
                        json!({"relation":relation,"from":{"kind":o["kind"],"providerId":o["id"]},"to":{"kind":target_kind,"providerId":target,"presentInCapturedThread":ids.contains(&(target_kind,target))},"resolution":"sender_reported_relation_not_source_call_or_execution"}))?;
                }
            }
        }
    }
    Ok(
        json!({"proposalId":record.proposal_id,"receiptId":receipt["receiptId"],"payloadHash":record.payload_hash,"digestId":s.digest["id"],"principalId":record.principal_id,"receivedAtUnix":record.received_at_unix,"scope":scope,
        "sourceDigestHash":p["sourceDigestHash"],"discussionCaptureId":p["discussionCaptureId"],"discussionCoverageComplete":p["discussionCoverageComplete"],"unacquiredPulls":p["unacquiredPulls"],
        "symbolCandidates":p["symbolCandidates"],"symbolOmissions":p["symbolOmissions"],"unsupportedFiles":p["unsupportedFiles"],"visibilityUnresolved":p["visibilityUnresolved"],"receiverSourceVerification":"unavailable_no_sender_cas"}),
    )
}
pub(super) fn run(
    store: &Store,
    profile: &Profile,
    accesses: &[&Access],
    request: &Request,
    options: &Options,
    d: &Deadline,
) -> Result<Value> {
    options.admit(d)?;
    request.validate()?;
    profile.validate()?;
    ensure!(
        accesses.iter().any(|a| request.permits_access(a)),
        "github_app_digest_query_permission_required"
    );
    storage::sql(store, profile, false, d, |h| {
        // All selections are scoped before any metadata row can be released.
        let mut selected = vec![];
        for id in &request.proposal_ids {
            d.check()?;
            let (record, scope) = h
                .records
                .get(id)
                .filter(|(r, s)| {
                    r.endpoint == profile.endpoint
                        && request.permits(s)
                        && accesses.iter().any(|a| {
                            request.permits_access(a)
                                && a.id == r.principal_id
                                && a.permits_scope(s)
                        })
                })
                .ok_or_else(|| anyhow::anyhow!(UNAVAILABLE))?;
            let _ = scope;
            selected.push(record);
        }
        let mut page = Page {
            request,
            options,
            deadline: d,
            rows: vec![],
            visited: 0,
            matches: 0,
            search_bytes: 0,
            receipts: BTreeMap::new(),
        };
        let mut manifests = vec![];
        for record in selected {
            manifests.push(project(record, &mut page)?);
        }
        let has_more = request.offset.saturating_add(page.rows.len()) < page.matches;
        let value = json!({"schemaVersion":1,"kind":"native_received_digest_query","processor":"literal_metadata_projection_v1","requestHash":identity(request),"receiverWorkspaceId":store.workspace_id,
            "manifests":manifests,"rows":page.rows,"visitedRows":page.visited,"matchingRows":page.matches,"searchBytes":page.search_bytes,"offset":request.offset,"limit":request.limit,"hasMore":has_more,
            "completeForSelectedMetadata":!has_more,"complete":false,"sourceAuthority":false,"modelCalls":0,"networkRequests":0,"sourceWritten":false,"recordsWritten":false,
            "limitations":["Sender source, compiler binding, semantic identity and execution are unverified. Byte/line coordinates are reported claims, not receiver-local source anchors.","Literal matching inspects received metadata JSON and context. Discussion bodies and commit messages were excluded by the handoff; semantic reasoning and unsupported source content are unavailable."]});
        crate::json::encode_bounded(&value, options.max_output_bytes - 1, d)
            .map_err(|_| anyhow::anyhow!(CAPACITY))?;
        d.check()?;
        Ok(value)
    })
}
/// Explicit host-local read; no network, credential lookup, source capture or write.
pub fn admit_local(
    profile: &Profile,
    principal: &str,
    request: &Request,
    options: &Options,
    d: &Deadline,
) -> Result<()> {
    options.admit(d)?;
    request.validate()?;
    ensure!(crate::github_app::credential_name(principal), INVALID);
    profile.validate()?;
    ensure!(
        profile
            .grants
            .iter()
            .any(|g| g.id == principal && request.permits_access(g)),
        "github_app_digest_query_permission_required"
    );
    Ok(())
}
pub fn local(
    store: &Store,
    profile: &Profile,
    principal: &str,
    request: &Request,
    options: &Options,
    d: &Deadline,
) -> Result<Value> {
    admit_local(profile, principal, request, options, d)?;
    let a = profile
        .grants
        .iter()
        .find(|g| g.id == principal && request.permits_access(g))
        .ok_or_else(|| anyhow::anyhow!("github_app_digest_query_permission_required"))?;
    run(store, profile, &[a], request, options, d)
}
