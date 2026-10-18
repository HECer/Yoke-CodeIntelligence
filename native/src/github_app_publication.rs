//! Explicit source-bound comment previews; host approval never comes from repository data.
use super::{Host, worker};
use crate::{capture::Deadline, store::Store};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::fmt::Write;
#[path = "github_app_publication_pages.rs"]
mod linked_pages;
#[path = "github_app_publication_state.rs"]
mod state;

const INVALID: &str = "github_app_publication_input_invalid";
const CAPACITY: &str = "github_app_publication_capacity_exceeded";
pub const RECORD_KIND: &str = "github_app_publication_v1";

/// Checks explicit profile documents before credential lookup. This grants no
/// installation, repository, source, publication or network authority.
pub fn admit_profiles(
    host: &super::HostProfile,
    auth: &crate::github_app_auth::AuthProfile,
    require_write: bool,
    d: &Deadline,
) -> Result<()> {
    d.check()?;
    auth.admit_comment_profile(host.app_id, require_write)
        .map_err(|_| anyhow::anyhow!(INVALID))
}

#[derive(Clone)]
pub struct Options {
    pub allow_publication: bool,
    pub allow_page_links: bool,
    pub worker: worker::Options,
    pub max_body_bytes: usize,
    pub max_output_bytes: usize,
    pub max_journal_records: usize,
    pub max_journal_bytes: usize,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            allow_publication: false,
            allow_page_links: false,
            worker: worker::Options::default(),
            max_body_bytes: 48_000,
            max_output_bytes: 128_000,
            max_journal_records: 1000,
            max_journal_bytes: 64_000_000,
        }
    }
}

#[derive(Clone)]
pub struct PublishOptions {
    pub allow_network: bool,
    pub max_requests: usize,
    pub max_response_bytes: usize,
    pub max_total_bytes: usize,
}
impl Default for PublishOptions {
    fn default() -> Self {
        Self {
            allow_network: false,
            max_requests: 35,
            max_response_bytes: 1_000_000,
            max_total_bytes: 8_000_000,
        }
    }
}
impl PublishOptions {
    pub fn admit(&self, d: &Deadline) -> Result<()> {
        ensure!(self.allow_network, "github_app_network_permission_required");
        d.check()?;
        ensure!(
            self.max_requests > 0 && self.max_response_bytes > 0 && self.max_total_bytes > 0,
            CAPACITY
        );
        ensure!(
            (4..=35).contains(&self.max_requests)
                && self.max_response_bytes <= 2_000_000
                && self.max_total_bytes <= 64_000_000,
            INVALID
        );
        Ok(())
    }
}
pub struct Context<'a> {
    pub store: &'a Store,
    pub host: &'a Host,
    pub options: &'a Options,
    pub clock: &'a worker::Clock,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PublishRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<super::pages::LinkReference>,
    pub schema_version: u32,
    pub completion_id: String,
    pub expected_queue_revision: usize,
    pub proposal_id: String,
    pub body_hash: String,
    pub request_id: String,
    pub approval: String,
}
impl PublishRequest {
    pub fn parse(raw: &[u8]) -> Result<Self> {
        let request: Self = crate::json::decode(crate::json::parse(raw, 4096)?)?;
        request.validate()?;
        Ok(request)
    }
    fn validate(&self) -> Result<()> {
        if let Some(page) = &self.page {
            page.validate().map_err(|_| anyhow::anyhow!(INVALID))?;
        }
        let uuid = self.request_id.len() == 36
            && self.request_id.bytes().enumerate().all(|(i, b)| {
                if [8, 13, 18, 23].contains(&i) {
                    b == b'-'
                } else {
                    b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
                }
            });
        ensure!(
            self.schema_version == 1
                && crate::model::valid_id(&self.completion_id)
                && (1..=2000).contains(&self.expected_queue_revision)
                && crate::model::valid_id(&self.proposal_id)
                && crate::model::valid_id(&self.body_hash)
                && uuid
                && self.approval == "publish_exact_comment",
            INVALID
        );
        Ok(())
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StatusRequest {
    pub schema_version: u32,
    pub request_id: String,
}
impl StatusRequest {
    pub fn parse(raw: &[u8]) -> Result<Self> {
        let request: Self = crate::json::decode(crate::json::parse(raw, 4096)?)?;
        request.validate()?;
        Ok(request)
    }
    fn validate(&self) -> Result<()> {
        ensure!(self.schema_version == 1 && uuid(&self.request_id), INVALID);
        Ok(())
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecoverRequest {
    pub schema_version: u32,
    pub intent_id: String,
    pub comment_id: u64,
    pub installation_id: u64,
}
impl RecoverRequest {
    pub fn parse(raw: &[u8]) -> Result<Self> {
        let request: Self = crate::json::decode(crate::json::parse(raw, 4096)?)?;
        request.validate()?;
        Ok(request)
    }
    fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == 1
                && crate::model::valid_id(&self.intent_id)
                && super::numeric(self.comment_id)
                && super::numeric(self.installation_id),
            INVALID
        );
        Ok(())
    }
}
fn uuid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
            }
        })
}
pub fn status(context: &Context<'_>, request: &StatusRequest, d: &Deadline) -> Result<Value> {
    context.options.admit(d)?;
    request.validate()?;
    state::status(context, request, d)
}
pub fn recover(
    context: &Context<'_>,
    client: &mut crate::github_app_auth::Client,
    request: &RecoverRequest,
    remote: &PublishOptions,
    d: &Deadline,
) -> Result<Value> {
    context.options.worker.admit(true, d)?;
    context.options.admit(d)?;
    remote.admit(d)?;
    request.validate()?;
    ensure!(
        tokio::runtime::Handle::try_current().is_err(),
        "github_app_requires_synchronous_host"
    );
    if let state::Recovery::Historical(value) = state::recovery(context, request, d)? {
        return Ok(value);
    }
    let _owner = state::lock(context, d)?;
    let data = match state::recovery(context, request, d)? {
        state::Recovery::Historical(value) => return Ok(value),
        state::Recovery::Pending(data) => data,
    };
    state::admit_durability(context, d)?;
    crate::github_app_auth::read::admit_recovery(client, &data.job, request.installation_id, d)?;
    let lease = client.authorize(request.installation_id, data.job.repository_id, d)?;
    let mut reader = crate::github_app_auth::read::Reader::for_recovery(
        client,
        &lease,
        &data.job,
        request.installation_id,
        crate::github_app_auth::read::Limits {
            max_requests: remote.max_requests.min(33),
            max_response_bytes: remote.max_response_bytes,
            max_total_bytes: remote.max_total_bytes,
            max_files: 1,
        },
        d,
    )?;
    reader.repository()?;
    let checked =
        reader.recovery_comment(request.comment_id, &data.body_hash, remote.max_requests)?;
    state::reconcile(
        context,
        request,
        state::Comment {
            id: checked.id,
            body_hash: data.body_hash,
            html_url: checked.html_url,
            fixture: checked.fixture,
        },
        4 + reader.request_count(),
        || reader.check_lease(),
        d,
    )
}
pub fn publish(
    context: &Context<'_>,
    client: &mut crate::github_app_auth::Client,
    request: &PublishRequest,
    remote: &PublishOptions,
    d: &Deadline,
) -> Result<Value> {
    ensure!(
        context.options.allow_publication,
        "github_app_publication_permission_required"
    );
    context.options.worker.admit(true, d)?;
    context.options.admit(d)?;
    remote.admit(d)?;
    request.validate()?;
    ensure!(
        tokio::runtime::Handle::try_current().is_err(),
        "github_app_requires_synchronous_host"
    );
    ensure!(
        context.store.db.is_autocommit(),
        "github_app_publication_caller_transaction_refused"
    );
    if let Some(value) = state::lookup(context, request, d)? {
        return Ok(value);
    }
    prepared_data(context, request, d)?;
    let _owner = state::lock(context, d)?;
    if let Some(value) = state::lookup(context, request, d)? {
        return Ok(value);
    }
    let (source, data) = prepared(context, request, d)?;
    let target = state::plan(context, &data, d)?;
    state::admit_durability(context, d)?;
    let reserved = if matches!(target, state::Target::Update { .. }) {
        2
    } else {
        1
    };
    let listed = source.facts()["coverage"]["listedFiles"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!(INVALID))? as usize;
    let reads = 3 + listed.div_ceil(100);
    ensure!(reads + reserved <= remote.max_requests, CAPACITY);
    crate::github_app_auth::read::admit_publication(client, source.job(), d)?;
    let lease = client.authorize(source.job().installation_id, source.job().repository_id, d)?;
    let mut reader = crate::github_app_auth::read::Reader::new(
        client,
        &lease,
        source.job(),
        crate::github_app_auth::read::Limits {
            max_requests: (remote.max_requests - reserved).min(33),
            max_response_bytes: remote.max_response_bytes,
            max_total_bytes: remote.max_total_bytes,
            max_files: 3000,
        },
        d,
    )?;
    reader.repository()?;
    let initial = reader.pull_request()?;
    ensure!(
        source.matches_provider(&initial),
        "github_app_publication_provider_stale"
    );
    reader.files(&initial)?;
    let final_pr = reader.pull_request()?;
    ensure!(
        source.matches_provider(&final_pr),
        "github_app_publication_provider_stale"
    );
    if let state::Target::Observe {
        comment_id,
        body_hash,
    } = &target
    {
        let checked = reader.unchanged_comment(*comment_id, body_hash, remote.max_requests)?;
        return state::observe(
            context,
            request,
            state::Comment {
                id: checked.id,
                body_hash: data.body_hash,
                html_url: checked.html_url,
                fixture: checked.fixture,
            },
            4 + reader.request_count(),
            || reader.check_lease(),
            d,
        );
    }
    if let state::Target::Update {
        comment_id,
        previous_body_hash,
    } = &target
    {
        reader.owned_comment(*comment_id, previous_body_hash, remote.max_requests)?;
    }
    let permit = state::intend(context, request, || reader.check_lease(), d)?;
    ensure!(
        permit.target == target,
        "github_app_publication_binding_invalid"
    );
    ensure!(
        context.store.db.is_autocommit(),
        "github_app_publication_caller_transaction_refused"
    );
    let id = match target {
        state::Target::Create {} => None,
        state::Target::Update { comment_id, .. } => Some(comment_id),
        state::Target::Observe { .. } => anyhow::bail!("github_app_publication_binding_invalid"),
    };
    linked_pages::live(permit.data.page_link.as_ref(), context.clock, d)?;
    let checked = reader.write_comment(&permit.data.body, id, remote.max_requests)?;
    state::finish(
        context,
        &permit,
        state::Comment {
            id: checked.id,
            body_hash: permit.data.body_hash.clone(),
            html_url: checked.html_url,
            fixture: checked.fixture,
        },
        4 + reader.request_count(),
        || reader.check_lease(),
        d,
    )
}
fn prepared_data(
    context: &Context<'_>,
    request: &PublishRequest,
    d: &Deadline,
) -> Result<state::Data> {
    Ok(prepared(context, request, d)?.1)
}
fn prepared(
    context: &Context<'_>,
    request: &PublishRequest,
    d: &Deadline,
) -> Result<(worker::PublicationSource, state::Data)> {
    prepared_mode(context, request, PageAdmission::Fresh, d)
}
enum PageAdmission<'a> {
    Fresh,
    Captured(&'a state::Data),
}
fn prepared_captured_data(
    c: &Context<'_>,
    request: &PublishRequest,
    captured: &state::Data,
    d: &Deadline,
) -> Result<state::Data> {
    Ok(prepared_mode(c, request, PageAdmission::Captured(captured), d)?.1)
}
fn prepared_mode(
    context: &Context<'_>,
    request: &PublishRequest,
    admission: PageAdmission<'_>,
    d: &Deadline,
) -> Result<(worker::PublicationSource, state::Data)> {
    let source = worker::publication_source(
        context.store,
        context.host,
        &request.completion_id,
        request.expected_queue_revision,
        &context.options.worker,
        d,
    )?;
    let (page_link, queue_checkpoint) = match admission {
        PageAdmission::Fresh => (
            linked_pages::fresh(context, request.page.as_ref(), &source, d)?,
            source.queue_checkpoint().to_owned(),
        ),
        PageAdmission::Captured(data) => (
            linked_pages::captured(request.page.as_ref(), data, &source, d)?,
            data.queue_checkpoint.clone(),
        ),
    };
    let value = proposal(
        context.store,
        &source,
        page_link.as_ref(),
        context.options,
        d,
    )?;
    ensure!(
        value["proposalId"] == request.proposal_id && value["bodyHash"] == request.body_hash,
        "github_app_publication_approval_mismatch"
    );
    let facts = source.facts();
    let data = state::Data {
        page_link,
        job: source.job().clone(),
        completion_id: request.completion_id.clone(),
        completed_at_unix: source.completed_at_unix(),
        queue_revision: request.expected_queue_revision,
        queue_checkpoint,
        head_snapshot_id: facts["headSnapshotId"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!(INVALID))?
            .into(),
        base_snapshot_id: facts["baseSnapshotId"].as_str().map(str::to_owned),
        review_hash: source.review_hash()?,
        body: value["body"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!(INVALID))?
            .into(),
        body_hash: request.body_hash.clone(),
        proposal_id: request.proposal_id.clone(),
    };
    Ok((source, data))
}
impl Options {
    pub fn admit(&self, d: &Deadline) -> Result<()> {
        self.worker.admit(false, d)?;
        ensure!(
            self.max_body_bytes > 0
                && self.max_output_bytes > 0
                && self.max_journal_records > 0
                && self.max_journal_bytes > 0,
            CAPACITY
        );
        ensure!(
            self.max_body_bytes <= 60_000
                && self.max_output_bytes <= 4_000_000
                && self.max_journal_records <= 1000
                && self.max_journal_bytes <= 64_000_000,
            INVALID
        );
        Ok(())
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviewRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<super::pages::LinkReference>,
    pub schema_version: u32,
    pub completion_id: String,
    pub expected_queue_revision: usize,
}
impl PreviewRequest {
    pub fn parse(raw: &[u8]) -> Result<Self> {
        let request: Self = crate::json::decode(crate::json::parse(raw, 4096)?)?;
        request.validate()?;
        Ok(request)
    }
    fn validate(&self) -> Result<()> {
        if let Some(page) = &self.page {
            page.validate().map_err(|_| anyhow::anyhow!(INVALID))?;
        }
        ensure!(
            self.schema_version == 1
                && crate::model::valid_id(&self.completion_id)
                && (1..=2000).contains(&self.expected_queue_revision),
            INVALID
        );
        Ok(())
    }
}
pub fn preview_with_context(
    c: &Context<'_>,
    request: &PreviewRequest,
    d: &Deadline,
) -> Result<Value> {
    c.options.admit(d)?;
    request.validate()?;
    ensure!(
        request.page.is_none() || c.options.allow_page_links,
        "github_app_page_links_permission_required"
    );
    ensure!(
        c.store.db.is_autocommit(),
        "github_app_publication_caller_transaction_refused"
    );
    let source = worker::publication_source(
        c.store,
        c.host,
        &request.completion_id,
        request.expected_queue_revision,
        &c.options.worker,
        d,
    )?;
    let link = linked_pages::fresh(c, request.page.as_ref(), &source, d)?;
    proposal(c.store, &source, link.as_ref(), c.options, d)
}
pub fn preview(
    store: &Store,
    host: &Host,
    request: &PreviewRequest,
    o: &Options,
    d: &Deadline,
) -> Result<Value> {
    let clock = worker::Clock::system();
    preview_with_context(
        &Context {
            store,
            host,
            options: o,
            clock: &clock,
        },
        request,
        d,
    )
}
fn code(value: &str, d: &Deadline) -> Result<String> {
    let mut out = String::from("<code>");
    for (index, ch) in value.chars().enumerate() {
        if index % 1024 == 0 {
            d.check()?;
        }
        if ch.is_ascii_alphanumeric() || matches!(ch, ' ' | '.' | '_' | '/' | '-') {
            out.push(ch);
        } else {
            write!(out, "&#x{:X};", ch as u32)?;
        }
    }
    out.push_str("</code>");
    Ok(out)
}
struct Body<'a> {
    text: String,
    limit: usize,
    d: &'a Deadline,
}
impl Body<'_> {
    fn append(&mut self, value: &str) -> Result<()> {
        self.d.check()?;
        ensure!(
            self.text
                .len()
                .checked_add(value.len())
                .is_some_and(|n| n <= self.limit),
            CAPACITY
        );
        self.text.push_str(value);
        Ok(())
    }
    fn section(&mut self, label: &str, facts: &Value) -> Result<()> {
        self.append(&format!("\n### {label}\n\nChanged symbols: {}\n\nIncoming candidates: {}\n\nDirected observations: {}\n",
            facts["changedSymbols"], facts["incomingCandidates"], facts["observations"]))?;
        for row in facts["files"].as_array().into_iter().flatten() {
            self.append(&format!(
                "\n- {}: {}; patch hunks verified: {}{}\n",
                code(row["path"].as_str().unwrap_or_default(), self.d)?,
                code(row["sourceMatch"].as_str().unwrap_or_default(), self.d)?,
                row["hunksVerified"] == true,
                row["reason"]
                    .as_str()
                    .map(|r| code(r, self.d))
                    .transpose()?
                    .map(|r| format!("; omission: {r}"))
                    .unwrap_or_default()
            ))?;
        }
        for row in facts["omissions"].as_array().into_iter().flatten() {
            self.append(&format!(
                "\n- Omitted {}: {}\n",
                code(row["path"].as_str().unwrap_or_default(), self.d)?,
                code(row["reason"].as_str().unwrap_or_default(), self.d)?
            ))?;
        }
        Ok(())
    }
}
fn render_body(
    job: &super::Job,
    completion_id: &str,
    review_hash: &str,
    facts: &Value,
    page_link: Option<&super::pages::LinkBinding>,
    limit: usize,
    d: &Deadline,
) -> Result<String> {
    let mut body = Body {
        text: String::new(),
        limit,
        d,
    };
    body.append("<!-- yoke-code-intelligence/pr-comment/v1 -->\n## Yoke structural impact\n\nStructural candidates; no compiler, runtime, merge or complete coverage proof.\n")?;
    body.append(&format!("\nRepository: {} #{}\n\nHead: {}\n\nBase: {}\n\nCompletion: {}\n\nReport: {}\n\nHead snapshot: {}\n\nBase snapshot: {}\n",
        code(&job.repository,d)?,job.number,code(&job.head_sha,d)?,code(&job.base_sha,d)?,
        code(completion_id,d)?,code(review_hash,d)?,
        code(facts["headSnapshotId"].as_str().unwrap_or_default(),d)?,
        code(facts["baseSnapshotId"].as_str().unwrap_or("not supplied"),d)?))?;
    body.append(&format!(
        "\nListed files: {}\n\nRemoved files: {}\n\nRemoved-source coverage: {}\n",
        facts["coverage"]["listedFiles"],
        facts["coverage"]["removedFiles"],
        code(
            facts["coverage"]["removedSource"]
                .as_str()
                .unwrap_or_default(),
            d
        )?
    ))?;
    body.section("Head", &facts["head"])?;
    if !facts["base"].is_null() {
        body.section("Removed-source base", &facts["base"])?;
    }
    body.append("\n### Exact captured source bindings\n")?;
    for binding in facts["bindings"].as_array().into_iter().flatten() {
        body.append(&format!(
            "\n- {} bytes [{}, {}); snapshot {}; content {}; Git blob {}\n",
            code(binding["path"].as_str().unwrap_or_default(), d)?,
            binding["startByte"],
            binding["endByte"],
            code(binding["snapshotId"].as_str().unwrap_or_default(), d)?,
            code(binding["contentHash"].as_str().unwrap_or_default(), d)?,
            code(binding["gitBlobSha"].as_str().unwrap_or_default(), d)?
        ))?;
    }
    linked_pages::append(&mut body, page_link)?;
    body.append("\nWhole-tree commit equality: not attested. Provider metadata was observed twice during analysis; it is historical and does not authorize publication. No source code or model-generated text is included.\n")?;
    Ok(body.text)
}
fn proposal(
    store: &Store,
    source: &worker::PublicationSource,
    page_link: Option<&super::pages::LinkBinding>,
    o: &Options,
    d: &Deadline,
) -> Result<Value> {
    let job = source.job();
    let facts = source.facts();
    let review_hash = source.review_hash()?;
    let body = render_body(
        job,
        source.completion_id(),
        &review_hash,
        &facts,
        page_link,
        o.max_body_bytes,
        d,
    )?;
    let mut contract = json!({"schemaVersion":1,"rendererVersion":"yoke-pr-comment-v1",
        "workspaceId":store.workspace_id,"appId":job.app_id,"installationId":job.installation_id,
        "repositoryId":job.repository_id,"repository":job.repository,"number":job.number,
        "jobId":job.id,"completionId":source.completion_id(),"queueRevision":source.queue_revision(),
        "headSha":job.head_sha,"baseSha":job.base_sha,
        "headSnapshotId":facts["headSnapshotId"],"baseSnapshotId":facts["baseSnapshotId"],
        "reviewHash":review_hash,"bodyHash":crate::model::hash(body.as_bytes()),"body":body});
    linked_pages::attach(&mut contract, page_link);
    let proposal_id = crate::model::identity(&contract);
    let object = contract.as_object_mut().expect("proposal object");
    object.extend([
        ("proposalId".into(), json!(proposal_id)),
        ("published".into(), json!(false)),
        ("publicationAuthority".into(), json!(false)),
        ("currentHeadVerified".into(), json!(false)),
        ("installationTokenVerified".into(), json!(false)),
        ("sourceWritten".into(), json!(false)),
        ("complete".into(), json!(false)),
        ("networkRequests".into(), json!(0)),
        ("modelCalls".into(), json!(0)),
    ]);
    crate::json::encode_bounded(&contract, o.max_output_bytes - 1, d)
        .map_err(|_| d.check().err().unwrap_or_else(|| anyhow::anyhow!(CAPACITY)))?;
    Ok(contract)
}
/// Closed typed inline roots only. No host-key lookup or execution/publication authority.
pub(crate) fn validated_inline_records(
    store: &Store,
    worker_roots: &worker::RetainedRoots,
    d: &Deadline,
) -> Result<std::collections::BTreeSet<String>> {
    state::validated_inline_records(store, worker_roots, d)
}
