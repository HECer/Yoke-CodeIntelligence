//! Bounded signed page cache. Reads never refresh, delete or acquire write authority.
use super::*;
use rusqlite::{Transaction, TransactionBehavior, limits::Limit, params};
use std::{collections::BTreeSet, time::Duration};

const BINDING: &str = "github_app_pages_binding_invalid";
const STALE: &str = "github_app_pages_plan_stale";
const MAX_RECORD: usize = 16_065_536;
const MAX_BYTES: usize = 128_000_000;
const DOMAIN: &[u8] = b"yoke-code-intelligence/github-app-page-cache/v1\n";
const FLAGS: &[&str] = &[
    "proposalId",
    "persisted",
    "published",
    "pageWriteAuthority",
    "bearerReadCapability",
    "publicationAuthority",
    "sourceWritten",
    "currentHeadVerified",
    "networkRequests",
    "modelCalls",
];
const DISPLAY: &[&str] = &[
    "schemaVersion",
    "rendererVersion",
    "workspaceId",
    "appId",
    "installationId",
    "repositoryId",
    "repository",
    "number",
    "jobId",
    "completionId",
    "queueRevision",
    "queueCheckpoint",
    "headSha",
    "baseSha",
    "headSnapshotId",
    "baseSnapshotId",
    "reviewHash",
    "coverage",
    "bindings",
    "graph",
    "certainty",
    "complete",
];
const EXTRAS: &[&str] = &[
    "pageRevision",
    "previousPageVersion",
    "cachePolicy",
    "pageId",
    "publicBaseUrl",
    "url",
    "htmlHash",
    "html",
];

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Page {
    schema_version: u32,
    workspace_id: String,
    app_id: u64,
    page_id: String,
    version: u64,
    stored_at_unix: u64,
    expires_at_unix: u64,
    completed_at_unix: u64,
    proposal: Value,
    signature256: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Revision {
    schema_version: u32,
    workspace_id: String,
    app_id: u64,
    revision: u64,
    at_unix: u64,
    signature256: String,
}
struct Cached {
    page: Page,
    bytes: usize,
}
#[derive(Default)]
struct Cache {
    pages: BTreeMap<String, Cached>,
    revisions: BTreeMap<u64, Revision>,
    revision_bytes: BTreeMap<u64, usize>,
}
fn id(value: &Value, key: &str) -> Result<String> {
    value[key]
        .as_str()
        .filter(|v| crate::model::valid_id(v))
        .map(str::to_owned)
        .ok_or_else(|| anyhow::anyhow!(BINDING))
}
fn number(value: &Value, key: &str) -> Result<u64> {
    value[key]
        .as_u64()
        .filter(|v| super::super::numeric(*v))
        .ok_or_else(|| anyhow::anyhow!(BINDING))
}
fn revision_id(workspace: &str, app: u64) -> String {
    crate::model::identity(
        &json!({"domain":"yoke-page-revision-v1","workspaceId":workspace,"appId":app}),
    )
}
fn page_id(store: &Store, p: &Value) -> Result<String> {
    Ok(crate::model::identity(
        &json!({"domain":"yoke-github-page-v1","workspaceId":store.workspace_id,
        "appId":number(p,"appId")?,"repositoryId":number(p,"repositoryId")?,"number":number(p,"number")?}),
    ))
}
fn message(kind: &str, value: &impl Serialize, d: &Deadline) -> Result<Vec<u8>> {
    let mut value = serde_json::to_value(value)?;
    value
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!(BINDING))?
        .remove("signature256");
    let mut message = DOMAIN.to_vec();
    message.extend(bounded(&json!({"kind":kind,"data":value}), MAX_RECORD, d)?);
    Ok(message)
}
fn sign(kind: &str, value: &impl Serialize, host: &Host, d: &Deadline) -> Result<String> {
    let mut mac = Hmac::<sha2::Sha256>::new_from_slice(&host.keys[0].0)
        .map_err(|_| anyhow::anyhow!(BINDING))?;
    for chunk in message(kind, value, d)?.chunks(65536) {
        d.check()?;
        mac.update(chunk);
    }
    Ok(format!(
        "sha256={}",
        hex::encode(mac.finalize().into_bytes())
    ))
}
fn verify(
    kind: &str,
    value: &impl Serialize,
    signature: &str,
    host: &Host,
    d: &Deadline,
) -> Result<()> {
    let message = message(kind, value, d)?;
    let mut verified = false;
    for key in &host.keys {
        d.check()?;
        verified |= key.verify(&message, signature).is_ok();
    }
    ensure!(verified, BINDING);
    Ok(())
}
fn display(p: &Value) -> Result<Value> {
    let object = p.as_object().ok_or_else(|| anyhow::anyhow!(BINDING))?;
    ensure!(
        object.len() == DISPLAY.len() + EXTRAS.len() + FLAGS.len()
            && object.keys().all(|key| DISPLAY.contains(&key.as_str())
                || EXTRAS.contains(&key.as_str())
                || FLAGS.contains(&key.as_str())),
        BINDING
    );
    Ok(Value::Object(
        DISPLAY
            .iter()
            .map(|key| ((*key).into(), p[*key].clone()))
            .collect(),
    ))
}
fn contract_id(p: &Value) -> Result<String> {
    let mut contract = p.clone();
    let object = contract
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!(BINDING))?;
    for key in FLAGS {
        object.remove(*key);
    }
    Ok(crate::model::identity(&contract))
}
fn hard_options(p: &Value, d: &Deadline) -> Result<Options> {
    let policy = &p["cachePolicy"];
    ensure!(policy.as_object().is_some_and(|p| p.len() == 3), BINDING);
    let o = Options {
        ttl_seconds: number(policy, "ttlSeconds")?,
        max_pages: usize::try_from(number(policy, "maxPages")?)?,
        max_store_bytes: usize::try_from(number(policy, "maxStoreBytes")?)?,
        max_html_bytes: 4_000_000,
        max_output_bytes: 16_000_000,
        max_nodes: 10_000,
        max_observations: 40_000,
        ..Default::default()
    };
    o.admit(d)?;
    Ok(o)
}
fn request(p: &Value) -> Result<PreviewRequest> {
    let r = PreviewRequest {
        schema_version: 1,
        completion_id: id(p, "completionId")?,
        expected_queue_revision: usize::try_from(number(p, "queueRevision")?)?,
        public_base_url: p["publicBaseUrl"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!(BINDING))?
            .into(),
    };
    r.validate().map_err(|_| anyhow::anyhow!(BINDING))?;
    Ok(r)
}
fn validate_page(store: &Store, page: &Page, d: &Deadline) -> Result<()> {
    let p = &page.proposal;
    let packet = display(p)?;
    let r = request(p)?;
    let o = hard_options(p, d)?;
    ensure!(
        page.schema_version == 1
            && page.workspace_id == store.workspace_id
            && page.app_id == number(p, "appId")?
            && p["workspaceId"] == store.workspace_id
            && page.page_id == page_id(store, p)?
            && page.page_id == id(p, "pageId")?
            && page.version == number(p, "pageRevision")?
            && page.version < 9_007_199_254_740_991
            && p["previousPageVersion"]
                .as_u64()
                .is_none_or(|v| v > 0 && v < page.version)
            && (p["previousPageVersion"].is_null() || p["previousPageVersion"].as_u64().is_some())
            && (60..=253_402_296_000).contains(&page.completed_at_unix)
            && page.completed_at_unix <= page.stored_at_unix
            && page.stored_at_unix <= 253_402_296_000
            && page.stored_at_unix.checked_add(o.ttl_seconds) == Some(page.expires_at_unix),
        BINDING
    );
    ensure!(
        p["schemaVersion"] == 1
            && p["rendererVersion"] == "yoke-pr-page-v1"
            && p["certainty"]
                == "source_bound_structural_candidates_not_compiler_runtime_or_merge_proof"
            && p["complete"] == false
            && p["networkRequests"] == 0
            && p["modelCalls"] == 0
            && p["bearerReadCapability"]
                == "conditional_on_stored_unexpired_page_current_key_and_repository_grant"
            && [
                "persisted",
                "published",
                "pageWriteAuthority",
                "publicationAuthority",
                "sourceWritten",
                "currentHeadVerified"
            ]
            .iter()
            .all(|key| p[*key] == false),
        BINDING
    );
    for key in [
        "jobId",
        "queueCheckpoint",
        "headSnapshotId",
        "reviewHash",
        "htmlHash",
        "proposalId",
    ] {
        id(p, key)?;
    }
    ensure!(
        p["baseSnapshotId"].is_null()
            || p["baseSnapshotId"]
                .as_str()
                .is_some_and(crate::model::valid_id),
        BINDING
    );
    let html = p["html"].as_str().ok_or_else(|| anyhow::anyhow!(BINDING))?;
    ensure!(
        html.len() <= 4_000_000
            && crate::model::hash(html.as_bytes()) == id(p, "htmlHash")?
            && render::html(&packet, 4_000_000, d)? == html
            && contract_id(p)? == id(p, "proposalId")?,
        BINDING
    );
    let url = p["url"].as_str().ok_or_else(|| anyhow::anyhow!(BINDING))?;
    let prefix = format!("{}/p/{}?t=", r.public_base_url, page.page_id);
    ensure!(
        url.strip_prefix(&prefix)
            .is_some_and(crate::model::valid_id),
        BINDING
    );
    Ok(())
}
fn matches_job(p: &Value, job: &super::super::Job) -> bool {
    p["workspaceId"] == job.workspace_id
        && p["appId"] == job.app_id
        && p["installationId"] == job.installation_id
        && p["repositoryId"] == job.repository_id
        && p["repository"] == job.repository
        && p["number"] == job.number
        && p["jobId"] == job.id
        && p["headSha"] == job.head_sha
        && p["baseSha"] == job.base_sha
        && job.work_class == super::super::WorkClass::Review
}
fn load(store: &Store, host: Option<&Host>, o: &Options, d: &Deadline) -> Result<Cache> {
    let (count,bytes,bad):(usize,usize,usize)=store.db.query_row(
        "SELECT count(*),coalesce(sum(length(CAST(value AS BLOB))),0),coalesce(sum(CASE WHEN typeof(value)!='text' OR length(CAST(value AS BLOB))>?3 OR length(CAST(id AS BLOB))!=64 THEN 1 ELSE 0 END),0) FROM records WHERE kind IN (?1,?2)",
        params![RECORD_KIND,REVISION_KIND,MAX_RECORD],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
    ensure!(count <= 2000 && bytes <= MAX_BYTES && bad == 0, CAPACITY);
    let mut cache = Cache::default();
    let mut proofs = BTreeMap::<String, Vec<(String, usize)>>::new();
    let mut statement = store
        .db
        .prepare("SELECT kind,id,value FROM records WHERE kind IN (?1,?2) ORDER BY kind,id")?;
    for row in statement.query_map(params![RECORD_KIND, REVISION_KIND], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })? {
        d.check()?;
        let (kind, key, raw) = row?;
        ensure!(crate::model::valid_id(&key), BINDING);
        let value =
            crate::json::parse(raw.as_bytes(), MAX_RECORD).map_err(|_| anyhow::anyhow!(BINDING))?;
        if kind == REVISION_KIND {
            ensure!(raw.len() <= 4096, CAPACITY);
            let revision: Revision =
                crate::json::decode(value).map_err(|_| anyhow::anyhow!(BINDING))?;
            ensure!(
                revision.schema_version == 1
                    && revision.workspace_id == store.workspace_id
                    && super::super::numeric(revision.app_id)
                    && (2..=9_007_199_254_740_991).contains(&revision.revision)
                    && (60..=253_402_296_000).contains(&revision.at_unix)
                    && key == revision_id(&store.workspace_id, revision.app_id),
                BINDING
            );
            ensure!(
                super::super::signature_bytes(&revision.signature256).is_ok(),
                BINDING
            );
            if let Some(host) = host.filter(|host| revision.app_id == host.profile.app_id) {
                verify(REVISION_KIND, &revision, &revision.signature256, host, d)?;
            }
            cache.revision_bytes.insert(revision.app_id, raw.len());
            ensure!(
                cache.revisions.insert(revision.app_id, revision).is_none(),
                BINDING
            );
        } else {
            let page: Page = crate::json::decode(value).map_err(|_| anyhow::anyhow!(BINDING))?;
            ensure!(
                super::super::signature_bytes(&page.signature256).is_ok(),
                BINDING
            );
            if let Some(host) = host.filter(|host| page.app_id == host.profile.app_id) {
                verify(RECORD_KIND, &page, &page.signature256, host, d)?;
            }
            validate_page(store, &page, d)?;
            ensure!(key == page.page_id, BINDING);
            let p = &page.proposal;
            proofs
                .entry(id(p, "queueCheckpoint")?)
                .or_default()
                .push((id(p, "jobId")?, request(p)?.expected_queue_revision));
            ensure!(
                cache
                    .pages
                    .insert(
                        key,
                        Cached {
                            page,
                            bytes: raw.len()
                        }
                    )
                    .is_none(),
                BINDING
            );
        }
    }
    ensure!(
        cache.pages.len() <= 1000 && cache.revisions.len() <= 1000,
        CAPACITY
    );
    if !cache.pages.is_empty() {
        let views = super::super::state::worker_views(store, host, &proofs, &o.worker.queue, d)?;
        for entry in cache.pages.values() {
            d.check()?;
            let page = &entry.page;
            let p = &page.proposal;
            let revision = cache
                .revisions
                .get(&page.app_id)
                .ok_or_else(|| anyhow::anyhow!(BINDING))?;
            ensure!(
                page.version < revision.revision && page.stored_at_unix <= revision.at_unix,
                BINDING
            );
            let proof = views
                .proofs
                .get(&(
                    id(p, "queueCheckpoint")?,
                    id(p, "jobId")?,
                    request(p)?.expected_queue_revision,
                ))
                .ok_or_else(|| anyhow::anyhow!(BINDING))?;
            ensure!(matches_job(p, &proof.job), BINDING);
        }
    }
    Ok(cache)
}
fn sql<T>(
    store: &Store,
    host: &Host,
    o: &Options,
    write: bool,
    d: &Deadline,
    run: impl FnOnce(&mut Cache) -> Result<T>,
) -> Result<T> {
    ensure!(
        store.db.is_autocommit(),
        "github_app_pages_caller_transaction_refused"
    );
    if write {
        let synchronous: u32 = store.db.query_row("PRAGMA synchronous", [], |r| r.get(0))?;
        let journal: String = store
            .db
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))?;
        ensure!(
            (2..=3).contains(&synchronous) && journal == "wal",
            "github_app_pages_sql_durability_required"
        );
    }
    worker::publication_policy(store, d, || {
        store
            .db
            .busy_timeout(d.remaining().min(Duration::from_millis(50)))?;
        let prior = store.db.limit(Limit::SQLITE_LIMIT_LENGTH)?;
        store
            .db
            .set_limit(Limit::SQLITE_LIMIT_LENGTH, prior.min(64_065_536))?;
        let tx = Transaction::new_unchecked(
            &store.db,
            if write {
                TransactionBehavior::Immediate
            } else {
                TransactionBehavior::Deferred
            },
        )?;
        let mut cache = load(store, Some(host), o, d)?;
        let result = run(&mut cache)?;
        d.check()?;
        tx.commit()?;
        d.check()?;
        Ok(result)
    })
}
pub(super) fn validated_inline_records(
    store: &Store,
    worker_roots: &worker::RetainedRoots,
    d: &Deadline,
) -> Result<BTreeSet<(String, String)>> {
    let mut o = Options::default();
    o.worker.queue.max_payload_bytes = super::super::MAX_PAYLOAD_BYTES;
    o.admit(d)?;
    worker::publication_policy(store, d, || {
        store
            .db
            .busy_timeout(d.remaining().min(Duration::from_millis(50)))?;
        let prior = store.db.limit(Limit::SQLITE_LIMIT_LENGTH)?;
        store
            .db
            .set_limit(Limit::SQLITE_LIMIT_LENGTH, prior.min(64_065_536))?;
        let tx = if store.db.is_autocommit() {
            Some(Transaction::new_unchecked(
                &store.db,
                TransactionBehavior::Deferred,
            )?)
        } else {
            None
        };
        let cache = load(store, None, &o, d)?;
        let mut ids = BTreeSet::new();
        for (key, entry) in &cache.pages {
            d.check()?;
            let page = &entry.page;
            let p = &page.proposal;
            let binding = worker_roots
                .publications
                .get(&id(p, "completionId")?)
                .ok_or_else(|| anyhow::anyhow!(BINDING))?;
            ensure!(
                matches_job(p, &binding.job)
                    && page.completed_at_unix == binding.completed_at_unix
                    && p["headSnapshotId"] == binding.head_snapshot_id
                    && p["baseSnapshotId"] == json!(binding.base_snapshot_id)
                    && p["reviewHash"] == binding.review_hash
                    && p["coverage"] == binding.facts["coverage"]
                    && p["bindings"] == binding.facts["bindings"]
                    && binding.page_graph_hash.as_ref()
                        == Some(&crate::model::identity(&p["graph"])),
                BINDING
            );
            ids.insert((RECORD_KIND.into(), key.clone()));
        }
        for app in cache.revisions.keys() {
            ids.insert((REVISION_KIND.into(), revision_id(&store.workspace_id, *app)));
        }
        d.check()?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        d.check()?;
        Ok(ids)
    })
}
impl Cache {
    fn revision(&self, app: u64) -> u64 {
        self.revisions.get(&app).map_or(1, |r| r.revision)
    }
    fn clock(&self, c: &Context<'_>) -> Result<u64> {
        let now = c.clock.now()?;
        ensure!(
            self.revisions
                .get(&c.host.profile.app_id)
                .is_none_or(|r| now >= r.at_unix),
            "github_app_pages_clock_regressed"
        );
        Ok(now)
    }
}
pub(super) fn preview(
    store: &Store,
    host: &Host,
    r: &PreviewRequest,
    o: &Options,
    d: &Deadline,
) -> Result<Value> {
    sql(store, host, o, false, d, |cache| {
        let source = worker::publication_source(
            store,
            host,
            &r.completion_id,
            r.expected_queue_revision,
            &o.worker,
            d,
        )?;
        let key = crate::model::identity(
            &json!({"domain":"yoke-github-page-v1","workspaceId":store.workspace_id,
            "appId":source.job().app_id,"repositoryId":source.job().repository_id,"number":source.job().number}),
        );
        proposal(
            store,
            host,
            &source,
            r,
            o,
            PlanVersion {
                revision: cache.revision(host.profile.app_id),
                previous: cache.pages.get(&key).map(|p| p.page.version),
            },
            d,
        )
    })
}
fn grant(c: &Context<'_>, p: &Value) -> Result<u64> {
    ensure!(
        p["appId"] == c.host.profile.app_id,
        "github_app_worker_grant_denied"
    );
    c.host
        .profile
        .grants
        .iter()
        .find(|g| p["repositoryId"] == g.repository_id && p["repository"] == g.repository)
        .map(|g| g.installation_id)
        .ok_or_else(|| anyhow::anyhow!("github_app_worker_grant_denied"))
}
fn checked_source(c: &Context<'_>, page: &Page, d: &Deadline) -> Result<()> {
    let p = &page.proposal;
    let installation = grant(c, p)?;
    let source = worker::recovery_publication_source(
        c.store,
        c.host,
        &id(p, "completionId")?,
        installation,
        &c.options.worker,
        d,
    )?;
    ensure!(
        source.completed_at_unix() == page.completed_at_unix,
        BINDING
    );
    let r = request(p)?;
    let mut o = hard_options(p, d)?;
    o.worker = c.options.worker.clone();
    let mut expected = proposal(
        c.store,
        c.host,
        &source,
        &r,
        &o,
        PlanVersion {
            revision: page.version,
            previous: p["previousPageVersion"].as_u64(),
        },
        d,
    )?;
    // Later selection checkpoints have their own typed queue proof in load().
    expected["queueRevision"] = p["queueRevision"].clone();
    expected["queueCheckpoint"] = p["queueCheckpoint"].clone();
    // Current access verification is separate from the original approved URL.
    expected["url"] = p["url"].clone();
    let html = render::html(&display(&expected)?, 4_000_000, d)?;
    expected["htmlHash"] = json!(crate::model::hash(html.as_bytes()));
    expected["html"] = json!(html);
    expected["proposalId"] = json!(contract_id(&expected)?);
    ensure!(expected == *p, BINDING);
    Ok(())
}
fn result(page: &Page, revision: u64, historical: bool) -> Value {
    let p = &page.proposal;
    json!({"status":"stored","persisted":true,"historical":historical,"pageId":page.page_id,
        "pageVersion":page.version,"pageRevision":revision,"proposalId":p["proposalId"],
        "htmlHash":p["htmlHash"],"url":p["url"],"completionId":p["completionId"],
        "storedAtUnix":page.stored_at_unix,"expiresAtUnix":page.expires_at_unix,
        "published":false,"pageWriteAuthority":false,"publicationAuthority":false,
        "sourceWritten":false,"currentHeadVerified":false,"networkRequests":0,"modelCalls":0,"complete":false})
}
fn output(value: &Value, c: &Context<'_>, d: &Deadline) -> Result<()> {
    bounded(value, c.options.max_output_bytes - 1, d)?;
    Ok(())
}
fn exact(p: &Value, r: &StoreRequest) -> bool {
    p["completionId"] == r.completion_id
        && p["queueRevision"] == r.expected_queue_revision
        && p["publicBaseUrl"] == r.public_base_url
        && p["proposalId"] == r.proposal_id
        && p["htmlHash"] == r.html_hash
}
pub(super) fn store(c: &Context<'_>, r: &StoreRequest, d: &Deadline) -> Result<Value> {
    sql(c.store, c.host, c.options, true, d, |cache| {
        let now = cache.clock(c)?;
        let app = c.host.profile.app_id;
        if let Some(entry) = cache
            .pages
            .values()
            .find(|p| p.page.app_id == app && exact(&p.page.proposal, r))
        {
            checked_source(c, &entry.page, d)?;
            let value = result(&entry.page, cache.revision(app), true);
            output(&value, c, d)?;
            return Ok(value);
        }
        let source = worker::publication_source(
            c.store,
            c.host,
            &r.completion_id,
            r.expected_queue_revision,
            &c.options.worker,
            d,
        )?;
        ensure!(
            now >= source.completed_at_unix(),
            "github_app_pages_clock_regressed"
        );
        let revision = cache.revision(app);
        ensure!(revision < 9_007_199_254_740_991, CAPACITY);
        let key = crate::model::identity(
            &json!({"domain":"yoke-github-page-v1","workspaceId":c.store.workspace_id,
            "appId":app,"repositoryId":source.job().repository_id,"number":source.job().number}),
        );
        let p = proposal(
            c.store,
            c.host,
            &source,
            &r.preview(),
            c.options,
            PlanVersion {
                revision,
                previous: cache.pages.get(&key).map(|p| p.page.version),
            },
            d,
        )?;
        ensure!(exact(&p, r), STALE);
        let mut page = Page {
            schema_version: 1,
            workspace_id: c.store.workspace_id.clone(),
            app_id: app,
            page_id: key.clone(),
            version: revision,
            stored_at_unix: now,
            expires_at_unix: now
                .checked_add(c.options.ttl_seconds)
                .ok_or_else(|| anyhow::anyhow!(CAPACITY))?,
            completed_at_unix: source.completed_at_unix(),
            proposal: p,
            signature256: String::new(),
        };
        page.signature256 = sign(RECORD_KIND, &page, c.host, d)?;
        let raw = bounded(&serde_json::to_value(&page)?, MAX_RECORD, d)?;
        let mut next = Revision {
            schema_version: 1,
            workspace_id: c.store.workspace_id.clone(),
            app_id: app,
            revision: revision + 1,
            at_unix: now,
            signature256: String::new(),
        };
        next.signature256 = sign(REVISION_KIND, &next, c.host, d)?;
        let revision_raw = bounded(&serde_json::to_value(&next)?, 4096, d)?;
        let old_keys: BTreeSet<_> = cache.pages.keys().cloned().collect();
        cache
            .pages
            .retain(|id, e| e.page.app_id != app || id == &key || e.page.expires_at_unix > now);
        cache.pages.insert(
            key.clone(),
            Cached {
                page,
                bytes: raw.len(),
            },
        );
        let revision_count =
            cache.revisions.len() + usize::from(!cache.revisions.contains_key(&app));
        ensure!(revision_count <= 1000, CAPACITY);
        // Unchanged rows retain their original bytes, including admitted JSON
        // whitespace. Only this App's revision is replaced by canonical JSON.
        let revision_bytes = cache.revision_bytes.values().sum::<usize>()
            - cache.revision_bytes.get(&app).copied().unwrap_or(0)
            + revision_raw.len();
        loop {
            let active: Vec<_> = cache
                .pages
                .values()
                .filter(|p| p.page.app_id == app)
                .collect();
            let bytes = cache.pages.values().map(|p| p.bytes).sum::<usize>() + revision_bytes;
            if active.len() <= c.options.max_pages && bytes <= c.options.max_store_bytes {
                break;
            }
            let evict = active
                .iter()
                .filter(|p| p.page.page_id != key)
                .min_by_key(|p| (p.page.stored_at_unix, p.page.version, &p.page.page_id))
                .map(|p| p.page.page_id.clone())
                .ok_or_else(|| anyhow::anyhow!(CAPACITY))?;
            cache.pages.remove(&evict);
        }
        let all_bytes = cache.pages.values().map(|p| p.bytes).sum::<usize>() + revision_bytes;
        ensure!(
            cache.pages.len() <= 1000
                && cache.pages.len() + revision_count <= 2000
                && all_bytes <= MAX_BYTES,
            CAPACITY
        );
        let page = &cache.pages.get(&key).expect("inserted page").page;
        let value = result(page, next.revision, false);
        output(&value, c, d)?;
        for removed in old_keys.difference(&cache.pages.keys().cloned().collect()) {
            d.check()?;
            c.store.db.execute(
                "DELETE FROM records WHERE kind=?1 AND id=?2",
                params![RECORD_KIND, removed],
            )?;
        }
        c.store.db.execute("INSERT INTO records(kind,id,value) VALUES(?1,?2,?3) ON CONFLICT(kind,id) DO UPDATE SET value=excluded.value",params![RECORD_KIND,key,std::str::from_utf8(&raw)?])?;
        c.store.db.execute("INSERT INTO records(kind,id,value) VALUES(?1,?2,?3) ON CONFLICT(kind,id) DO UPDATE SET value=excluded.value",params![REVISION_KIND,revision_id(&c.store.workspace_id,app),std::str::from_utf8(&revision_raw)?])?;
        Ok(value)
    })
}
pub(super) fn approved_link(
    c: &Context<'_>,
    reference: &LinkReference,
    d: &Deadline,
) -> Result<LinkBinding> {
    sql(c.store, c.host, c.options, false, d, |cache| {
        link_from_cache(c, reference, cache, d)
    })
}
pub(super) fn approved_link_in_transaction(
    c: &Context<'_>,
    reference: &LinkReference,
    d: &Deadline,
) -> Result<LinkBinding> {
    let cache = load(c.store, Some(c.host), c.options, d)?;
    link_from_cache(c, reference, &cache, d)
}
fn link_from_cache(
    c: &Context<'_>,
    reference: &LinkReference,
    cache: &Cache,
    d: &Deadline,
) -> Result<LinkBinding> {
    let now = cache.clock(c)?;
    let page = &cache
        .pages
        .get(&reference.page_id)
        .filter(|p| {
            p.page.app_id == c.host.profile.app_id
                && p.page.version == reference.expected_page_version
                && now < p.page.expires_at_unix
        })
        .ok_or_else(|| anyhow::anyhow!("github_app_pages_link_stale"))?
        .page;
    checked_source(c, page, d)?;
    let p = &page.proposal;
    let html = p["html"].as_str().ok_or_else(|| anyhow::anyhow!(BINDING))?;
    ensure!(html.len() <= c.options.max_html_bytes, CAPACITY);
    d.check()?;
    let graph_hash = crate::model::identity(&p["graph"]);
    let mut source = json!({"domain":"yoke-github-page-link-source-v1",
        "graphHash":graph_hash,"completedAtUnix":page.completed_at_unix});
    for key in [
        "workspaceId",
        "appId",
        "repositoryId",
        "repository",
        "number",
        "jobId",
        "completionId",
        "queueRevision",
        "queueCheckpoint",
        "headSha",
        "baseSha",
        "headSnapshotId",
        "baseSnapshotId",
        "reviewHash",
    ] {
        source[key] = p[key].clone();
    }
    let public_base_url = p["publicBaseUrl"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!(BINDING))?
        .to_owned();
    let binding = LinkBinding {
        schema_version: 1,
        page_id: page.page_id.clone(),
        page_version: page.version,
        page_proposal_id: id(p, "proposalId")?,
        html_hash: id(p, "htmlHash")?,
        graph_hash,
        source_binding_hash: crate::model::identity(&source),
        source_queue_revision: number(p, "queueRevision")? as usize,
        source_queue_checkpoint: id(p, "queueCheckpoint")?,
        stored_at_unix: page.stored_at_unix,
        expires_at_unix: page.expires_at_unix,
        url: format!(
            "{public_base_url}/p/{}?t={}",
            page.page_id,
            token(c.host, &c.store.workspace_id, &page.page_id)?
        ),
        public_base_url,
    };
    bounded(
        &serde_json::to_value(&binding)?,
        c.options.max_output_bytes.saturating_sub(1),
        d,
    )?;
    ensure!(
        cache.clock(c)? < binding.expires_at_unix,
        "github_app_pages_link_stale"
    );
    d.check()?;
    Ok(binding)
}
pub(super) fn read(c: &Context<'_>, key: &str, d: &Deadline) -> Result<Option<ReadPage>> {
    sql(c.store, c.host, c.options, false, d, |cache| {
        let now = cache.clock(c)?;
        let Some(entry) = cache
            .pages
            .get(key)
            .filter(|p| p.page.app_id == c.host.profile.app_id)
        else {
            return Ok(None);
        };
        if now >= entry.page.expires_at_unix {
            return Ok(None);
        }
        checked_source(c, &entry.page, d)?;
        let html = entry.page.proposal["html"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!(BINDING))?;
        ensure!(html.len() <= c.options.max_html_bytes, CAPACITY);
        Ok(Some(ReadPage {
            html: html.into(),
            html_hash: id(&entry.page.proposal, "htmlHash")?,
            expires_at_unix: entry.page.expires_at_unix,
        }))
    })
}
pub(super) fn remove(c: &Context<'_>, r: &RemoveRequest, d: &Deadline) -> Result<Value> {
    sql(c.store, c.host, c.options, true, d, |cache| {
        let now = cache.clock(c)?;
        let app = c.host.profile.app_id;
        let revision = cache.revision(app);
        let page = &cache
            .pages
            .get(&r.page_id)
            .ok_or_else(|| anyhow::anyhow!(STALE))?
            .page;
        ensure!(
            page.app_id == app
                && page.version == r.expected_page_version
                && revision == r.expected_revision,
            STALE
        );
        grant(c, &page.proposal)?;
        ensure!(revision < 9_007_199_254_740_991, CAPACITY);
        let mut next = Revision {
            schema_version: 1,
            workspace_id: c.store.workspace_id.clone(),
            app_id: app,
            revision: revision + 1,
            at_unix: now,
            signature256: String::new(),
        };
        next.signature256 = sign(REVISION_KIND, &next, c.host, d)?;
        let raw = bounded(&serde_json::to_value(&next)?, 4096, d)?;
        let value = json!({"status":"removed","pageId":r.page_id,"pageVersion":page.version,"pageRevision":next.revision,
            "removed":true,"persisted":true,"publicationAuthority":false,"sourceWritten":false,"networkRequests":0,"modelCalls":0,"complete":false});
        output(&value, c, d)?;
        c.store.db.execute(
            "DELETE FROM records WHERE kind=?1 AND id=?2",
            params![RECORD_KIND, r.page_id],
        )?;
        c.store.db.execute(
            "UPDATE records SET value=?3 WHERE kind=?1 AND id=?2",
            params![
                REVISION_KIND,
                revision_id(&c.store.workspace_id, app),
                std::str::from_utf8(&raw)?
            ],
        )?;
        Ok(value)
    })
}
