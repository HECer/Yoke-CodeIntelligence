//! Fresh page authority is separate from immutable publication approval facts.
use super::super::pages::{self, LinkBinding, LinkReference};
use super::*;
const BINDING: &str = "github_app_publication_binding_invalid";
const MISMATCH: &str = "github_app_page_link_source_mismatch";

struct Source<'a> {
    job: &'a super::super::Job,
    completion_id: &'a str,
    queue_revision: usize,
    queue_checkpoint: &'a str,
    completed_at_unix: u64,
    head_snapshot_id: &'a str,
    base_snapshot_id: Option<&'a str>,
    review_hash: &'a str,
    graph_hash: &'a str,
}
impl Source<'_> {
    fn hash(&self) -> String {
        let j = self.job;
        crate::model::identity(&json!({"domain":"yoke-github-page-link-source-v1",
            "workspaceId":j.workspace_id,"appId":j.app_id,"repositoryId":j.repository_id,"repository":j.repository,"number":j.number,
            "jobId":j.id,"completionId":self.completion_id,"queueRevision":self.queue_revision,"queueCheckpoint":self.queue_checkpoint,
            "headSha":j.head_sha,"baseSha":j.base_sha,"headSnapshotId":self.head_snapshot_id,"baseSnapshotId":self.base_snapshot_id,
            "reviewHash":self.review_hash,"graphHash":self.graph_hash,"completedAtUnix":self.completed_at_unix}))
    }
}
fn validate(link: &LinkBinding, source: &Source<'_>) -> Result<()> {
    let j = source.job;
    let page_id = crate::model::identity(
        &json!({"domain":"yoke-github-page-v1","workspaceId":j.workspace_id,
        "appId":j.app_id,"repositoryId":j.repository_id,"number":j.number}),
    );
    ensure!(
        link.schema_version == 1
            && link.page_id == page_id
            && (1..=9_007_199_254_740_991).contains(&link.page_version)
            && [
                &link.page_id,
                &link.page_proposal_id,
                &link.html_hash,
                &link.graph_hash,
                &link.source_binding_hash,
                &link.source_queue_checkpoint
            ]
            .iter()
            .all(|v| crate::model::valid_id(v))
            && (1..=2000).contains(&link.source_queue_revision)
            && link.source_queue_revision == source.queue_revision
            && link.source_queue_checkpoint == source.queue_checkpoint
            && link.graph_hash == source.graph_hash
            && link.source_binding_hash == source.hash()
            && source.completed_at_unix <= link.stored_at_unix
            && link.expires_at_unix <= 253_402_296_000
            && link
                .expires_at_unix
                .checked_sub(link.stored_at_unix)
                .is_some_and(|ttl| (1..=90 * 24 * 60 * 60).contains(&ttl))
            && link.public_base_url.len() <= 512,
        BINDING
    );
    // Reuse the closed canonical-origin admission without selecting files or SQL.
    pages::PreviewRequest::parse(&serde_json::to_vec(
        &json!({"schemaVersion":1,"completionId":source.completion_id,
        "expectedQueueRevision":source.queue_revision,"publicBaseUrl":link.public_base_url}),
    )?)
    .map_err(|_| anyhow::anyhow!(BINDING))?;
    let prefix = format!("{}/p/{}?t=", link.public_base_url, link.page_id);
    ensure!(
        link.url
            .strip_prefix(&prefix)
            .is_some_and(crate::model::valid_id),
        BINDING
    );
    Ok(())
}
fn validate_native(
    link: &LinkBinding,
    source: &worker::PublicationSource,
    d: &Deadline,
) -> Result<()> {
    let (head, base) = source.page_impacts();
    let graph_hash =
        pages::retained_graph_hash(head, base, d)?.ok_or_else(|| anyhow::anyhow!(MISMATCH))?;
    let facts = source.facts();
    let review_hash = source.review_hash()?;
    validate(
        link,
        &Source {
            job: source.job(),
            completion_id: source.completion_id(),
            queue_revision: source.queue_revision(),
            queue_checkpoint: &link.source_queue_checkpoint,
            completed_at_unix: source.completed_at_unix(),
            head_snapshot_id: facts["headSnapshotId"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!(MISMATCH))?,
            base_snapshot_id: facts["baseSnapshotId"].as_str(),
            review_hash: &review_hash,
            graph_hash: &graph_hash,
        },
    )
    .map_err(|_| anyhow::anyhow!(MISMATCH))
}
pub(super) fn fresh(
    c: &Context<'_>,
    reference: Option<&LinkReference>,
    source: &worker::PublicationSource,
    d: &Deadline,
) -> Result<Option<LinkBinding>> {
    let Some(reference) = reference else {
        return Ok(None);
    };
    ensure!(
        c.options.allow_page_links,
        "github_app_page_links_permission_required"
    );
    let options = pages::Options {
        allow_pages: true,
        worker: c.options.worker.clone(),
        max_html_bytes: 4_000_000,
        max_output_bytes: c.options.max_output_bytes,
        max_nodes: 10_000,
        max_observations: 40_000,
        max_pages: 1000,
        max_store_bytes: 128_000_000,
        ..Default::default()
    };
    let context = pages::Context {
        store: c.store,
        host: c.host,
        options: &options,
        clock: c.clock,
    };
    let link = (if c.store.db.is_autocommit() {
        pages::approved_link(&context, reference, d)?
    } else {
        pages::approved_link_in_publication_transaction(&context, reference, d)?
    })
    .into_binding();
    validate_native(&link, source, d)?;
    Ok(Some(link))
}
pub(super) fn captured(
    reference: Option<&LinkReference>,
    data: &state::Data,
    source: &worker::PublicationSource,
    d: &Deadline,
) -> Result<Option<LinkBinding>> {
    let link = data.page_link.as_ref();
    ensure!(
        reference.map(|r| (&r.page_id, r.expected_page_version))
            == link.map(|l| (&l.page_id, l.page_version)),
        BINDING
    );
    if let Some(link) = link {
        // The journal replay proves the captured checkpoint; the source factory
        // separately checks that this job is still selected at its exact revision.
        validate_native(link, source, d)?;
    }
    Ok(link.cloned())
}
pub(super) fn validate_intent_time(link: Option<&LinkBinding>, at: u64) -> Result<()> {
    ensure!(
        link.is_none_or(|l| l.stored_at_unix <= at && at < l.expires_at_unix),
        BINDING
    );
    Ok(())
}
pub(super) fn validate_data(data: &state::Data) -> Result<()> {
    if let Some(link) = &data.page_link {
        validate(
            link,
            &Source {
                job: &data.job,
                completion_id: &data.completion_id,
                queue_revision: data.queue_revision,
                queue_checkpoint: &link.source_queue_checkpoint,
                completed_at_unix: data.completed_at_unix,
                head_snapshot_id: &data.head_snapshot_id,
                base_snapshot_id: data.base_snapshot_id.as_deref(),
                review_hash: &data.review_hash,
                graph_hash: &link.graph_hash,
            },
        )?;
    }
    Ok(())
}
pub(super) fn validate_worker(
    data: &state::Data,
    binding: &worker::PublicationBinding,
) -> Result<()> {
    if let Some(link) = &data.page_link {
        ensure!(
            binding.page_graph_hash.as_ref() == Some(&link.graph_hash),
            BINDING
        );
    }
    Ok(())
}
pub(super) fn attach(contract: &mut Value, link: Option<&LinkBinding>) {
    if let Some(link) = link {
        contract["rendererVersion"] = json!("yoke-pr-comment-v2");
        contract["pageLink"] = json!(link);
    }
}
pub(super) fn append(body: &mut Body<'_>, link: Option<&LinkBinding>) -> Result<()> {
    if let Some(link) = link {
        let mut url = String::new();
        for ch in link.url.chars() {
            body.d.check()?;
            match ch {
                '&' => url.push_str("&amp;"),
                '"' => url.push_str("&quot;"),
                '\'' => url.push_str("&#39;"),
                '<' => url.push_str("&lt;"),
                '>' => url.push_str("&gt;"),
                _ => url.push(ch),
            }
        }
        body.append(&format!(
            "\n<a href=\"{url}\">View captured structural graph</a>\n"
        ))?;
    }
    Ok(())
}
pub(super) fn live(link: Option<&LinkBinding>, clock: &worker::Clock, d: &Deadline) -> Result<()> {
    d.check()?;
    if let Some(link) = link {
        ensure!(
            clock.now()? < link.expires_at_unix,
            "github_app_pages_link_stale"
        );
    }
    d.check()
}
