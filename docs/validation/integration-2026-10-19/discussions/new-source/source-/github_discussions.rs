//! Explicit bounded acquisition of closed-PR discussions; no source execution,
//! Store mutation, provider write, model call or implicit service handoff.
use super::{HostReadProfile, Operation, ReadClient};
use crate::{capture::Deadline, model::hash};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

const INVALID: &str = "github_discussions_input_invalid";
const CAPACITY: &str = "github_discussions_capacity_exceeded";
const CHANGED: &str = "github_discussions_pull_changed";
const KIND: &str = "native_github_closed_pr_discussions";

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct Profile {
    pub read: HostReadProfile,
    pub allow_discussion_text: bool,
    pub max_observations: usize,
    pub max_text_bytes: usize,
    pub max_output_bytes: usize,
}
impl Default for Profile {
    fn default() -> Self {
        Self {
            read: HostReadProfile::default(),
            allow_discussion_text: false,
            max_observations: 1000,
            max_text_bytes: 4_000_000,
            max_output_bytes: 8_000_000,
        }
    }
}
impl Profile {
    pub fn parse(raw: &[u8]) -> Result<Self> {
        let value: Self = crate::json::decode(
            crate::json::parse(raw, 32_000).map_err(|_| anyhow::anyhow!(INVALID))?,
        )
        .map_err(|_| anyhow::anyhow!(INVALID))?;
        value.validate()?;
        Ok(value)
    }
    pub fn validate(&self) -> Result<()> {
        self.read.validate()?;
        let hard = Self::default();
        ensure!(
            self.max_observations <= hard.max_observations
                && self.max_text_bytes <= hard.max_text_bytes
                && (1024..=hard.max_output_bytes).contains(&self.max_output_bytes),
            CAPACITY
        );
        Ok(())
    }
    pub fn authorize(&self, request: &Request) -> Result<String> {
        self.validate()?;
        request.validate()?;
        ensure!(
            self.read.allow_network && self.allow_discussion_text,
            "github_discussions_host_permission_required"
        );
        ensure!(
            self.max_observations > 0 && self.max_text_bytes > 0,
            CAPACITY
        );
        ensure!(
            request.pull_requests.is_empty()
                || request.pull_requests.len() * 5 < self.read.max_requests as usize,
            CAPACITY
        );
        self.read.authorized_repo(Some(&request.repository))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub schema_version: u32,
    pub repository: String,
    pub repository_id: u64,
    /// Empty selects the bounded closed-PR listing, never open PRs.
    pub pull_requests: Vec<u64>,
}
impl Request {
    pub fn parse(raw: &[u8]) -> Result<Self> {
        let value: Self = crate::json::decode(
            crate::json::parse(raw, 8192).map_err(|_| anyhow::anyhow!(INVALID))?,
        )
        .map_err(|_| anyhow::anyhow!(INVALID))?;
        value.validate()?;
        Ok(value)
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == 1
                && self.repository_id > 0
                && self.repository_id <= i64::MAX as u64
                && self.pull_requests.len() <= 100,
            INVALID
        );
        ensure!(
            super::repository(&self.repository)? == self.repository,
            INVALID
        );
        let mut seen = BTreeSet::new();
        ensure!(
            self.pull_requests
                .iter()
                .all(|n| (1..=1_000_000).contains(n) && seen.insert(n)),
            INVALID
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Observation {
    pub kind: String,
    pub id: u64,
    pub body: Option<String>,
    pub body_hash: Option<String>,
    pub author: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub submitted_at: Option<String>,
    pub commit_id: Option<String>,
    pub original_commit_id: Option<String>,
    pub path: Option<String>,
    pub diff_hunk: Option<String>,
    pub in_reply_to_id: Option<u64>,
    pub review_id: Option<u64>,
    pub state: Option<String>,
    pub anchor: Option<Anchor>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Anchor {
    pub line: Option<i64>,
    pub original_line: Option<i64>,
    pub start_line: Option<i64>,
    pub original_start_line: Option<i64>,
    pub position: Option<i64>,
    pub original_position: Option<i64>,
    pub side: Option<String>,
    pub start_side: Option<String>,
    pub subject_type: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Coverage {
    pub complete: bool,
    pub reason: String,
    pub pages: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Channels {
    pub issue_comment: Coverage,
    pub review: Coverage,
    pub review_comment: Coverage,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Receipt {
    pub url: String,
    pub response_hash: String,
    pub response_bytes: usize,
    pub api_version: String,
    pub body_retained: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Pull {
    pub provider_id: u64,
    pub number: u64,
    pub title: String,
    pub head_sha: String,
    pub base_sha: String,
    pub closed_at: String,
    pub updated_at: String,
    pub merged: bool,
    pub observations: Vec<Observation>,
    pub coverage: Channels,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Payload {
    pub repository: String,
    pub repository_id: u64,
    pub requested_pulls: Vec<u64>,
    pub unacquired_pulls: Vec<u64>,
    pub pulls: Vec<Pull>,
    pub discovery_coverage: Coverage,
    pub receipts: Vec<Receipt>,
    pub acquired_at_unix: i64,
    pub atomic: bool,
    pub transport: String,
    pub requests: usize,
    pub response_bytes: usize,
    pub text_bytes: usize,
    pub observations: usize,
    pub coverage_complete: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Capture {
    pub schema_version: u32,
    pub kind: String,
    pub id: String,
    pub payload: Payload,
}

fn encoded_id(payload: &Payload, d: &Deadline) -> Result<String> {
    Ok(hash(crate::json::encode_bounded(
        payload,
        Profile::default().max_output_bytes,
        d,
    )?))
}
impl Capture {
    pub fn parse(raw: &[u8], d: &Deadline) -> Result<Self> {
        d.check()?;
        let value: Self = crate::json::decode(
            crate::json::parse(raw, Profile::default().max_output_bytes)
                .map_err(|_| anyhow::anyhow!(INVALID))?,
        )
        .map_err(|_| anyhow::anyhow!(INVALID))?;
        value.validate(d)?;
        Ok(value)
    }
    pub fn validate(&self, d: &Deadline) -> Result<()> {
        d.check()?;
        ensure!(
            self.schema_version == 1
                && self.kind == KIND
                && self.id == encoded_id(&self.payload, d)?,
            INVALID
        );
        let request = Request {
            schema_version: 1,
            repository: self.payload.repository.clone(),
            repository_id: self.payload.repository_id,
            pull_requests: self.payload.requested_pulls.clone(),
        };
        request.validate()?;
        let receipt_counts = validate_receipts(&self.payload)?;
        ensure!(
            matches!(
                self.payload.transport.as_str(),
                "host_authorized_github_https" | "explicit_host_loopback_fixture"
            ) && self.payload.pulls.len() + self.payload.unacquired_pulls.len() <= 100
                && !self.payload.atomic
                && self.payload.acquired_at_unix > 0
                && self.payload.requests <= 200
                && self.payload.response_bytes <= 64 * 1024 * 1024
                && self.payload.receipts.len() == self.payload.requests,
            INVALID
        );
        let mut budget = Budget {
            observations: 0,
            text_bytes: 0,
            profile: Profile::default(),
        };
        let mut pulls = BTreeSet::new();
        let mut observation_ids = BTreeSet::new();
        let mut expected_receipts =
            BTreeMap::from([(format!("/repos/{}", request.repository), 1usize)]);
        coverage(&self.payload.discovery_coverage)?;
        if request.pull_requests.is_empty() {
            expected_receipts.insert(
                format!("/repos/{}/pulls", request.repository),
                self.payload.discovery_coverage.pages,
            );
        } else {
            ensure!(
                self.payload.discovery_coverage.complete
                    && self.payload.discovery_coverage.pages == 0
                    && self.payload.discovery_coverage.reason == "explicit_selected_pull_numbers",
                INVALID
            );
        }
        let mut complete =
            self.payload.discovery_coverage.complete && self.payload.unacquired_pulls.is_empty();
        for pull in &self.payload.pulls {
            d.check()?;
            ensure!(
                pull.provider_id > 0
                    && pull.title.len() <= 4096
                    && (1..=1_000_000).contains(&pull.number)
                    && pulls.insert(pull.number)
                    && (request.pull_requests.is_empty()
                        || request.pull_requests.contains(&pull.number)),
                INVALID
            );
            budget.text(&pull.title)?;
            timestamp(&pull.closed_at)?;
            timestamp(&pull.updated_at)?;
            sha(&pull.head_sha)?;
            sha(&pull.base_sha)?;
            let mut ids = BTreeSet::new();
            let descriptions = pull
                .observations
                .iter()
                .filter(|o| o.kind == "description")
                .collect::<Vec<_>>();
            ensure!(
                descriptions.len() == 1
                    && descriptions[0].id == pull.provider_id
                    && descriptions[0].commit_id.as_deref() == Some(pull.head_sha.as_str()),
                INVALID
            );
            expected_receipts.insert(
                format!("/repos/{}/pulls/{}", request.repository, pull.number),
                2,
            );
            for (endpoint, cov) in [
                (
                    format!("issues/{}/comments", pull.number),
                    &pull.coverage.issue_comment,
                ),
                (
                    format!("pulls/{}/reviews", pull.number),
                    &pull.coverage.review,
                ),
                (
                    format!("pulls/{}/comments", pull.number),
                    &pull.coverage.review_comment,
                ),
            ] {
                coverage(cov)?;
                complete &= cov.complete;
                expected_receipts.insert(
                    format!("/repos/{}/{endpoint}", request.repository),
                    cov.pages,
                );
            }
            for item in &pull.observations {
                ensure!(
                    matches!(
                        item.kind.as_str(),
                        "description" | "issue_comment" | "review" | "review_comment"
                    ) && item.id > 0
                        && ids.insert((item.kind.clone(), item.id)),
                    INVALID
                );
                ensure!(
                    observation_ids.insert((item.kind.clone(), item.id)),
                    INVALID
                );
                ensure!(
                    item.in_reply_to_id.is_none_or(|id| id > 0)
                        && item.review_id.is_none_or(|id| id > 0),
                    INVALID
                );
                if item.kind != "review_comment" {
                    ensure!(
                        item.anchor.is_none()
                            && item.path.is_none()
                            && item.diff_hunk.is_none()
                            && item.in_reply_to_id.is_none()
                            && item.review_id.is_none()
                            && item.original_commit_id.is_none(),
                        INVALID
                    );
                } else {
                    ensure!(item.anchor.is_some(), INVALID);
                }
                budget.observation(item)?;
            }
        }
        for number in &self.payload.unacquired_pulls {
            ensure!(
                (1..=1_000_000).contains(number)
                    && pulls.insert(*number)
                    && (request.pull_requests.is_empty() || request.pull_requests.contains(number)),
                INVALID
            );
        }
        if !request.pull_requests.is_empty() {
            ensure!(
                pulls == request.pull_requests.iter().copied().collect(),
                INVALID
            );
        }
        expected_receipts.retain(|_, count| *count > 0);
        ensure!(
            receipt_counts == expected_receipts && self.payload.coverage_complete == complete,
            INVALID
        );
        ensure!(
            budget.text_bytes == self.payload.text_bytes
                && budget.observations == self.payload.observations,
            INVALID
        );
        // A capture is an immutable host observation, not a signature or proof of
        // atomic provider state. Its id authenticates neither author nor origin.
        Ok(())
    }
}

fn coverage(value: &Coverage) -> Result<()> {
    ensure!(
        value.pages <= 20
            && match value.reason.as_str() {
                "provider_reported_pages_complete" => value.complete && value.pages > 0,
                "explicit_selected_pull_numbers" => value.complete && value.pages == 0,
                "page_budget_exhausted"
                | "item_budget_exhausted"
                | "request_budget_exhausted"
                | "missing_next_page_receipt" => !value.complete,
                _ => false,
            },
        INVALID
    );
    Ok(())
}
fn validate_receipts(payload: &Payload) -> Result<BTreeMap<String, usize>> {
    let mut counts = BTreeMap::new();
    let mut origin = None;
    let mut body_bytes = 0usize;
    let mut pages = BTreeMap::<String, BTreeSet<usize>>::new();
    for receipt in &payload.receipts {
        ensure!(
            crate::model::valid_id(&receipt.response_hash)
                && receipt.api_version == "2026-03-10"
                && !receipt.body_retained
                && receipt.response_bytes <= 8 * 1024 * 1024,
            INVALID
        );
        body_bytes = body_bytes
            .checked_add(receipt.response_bytes)
            .context(CAPACITY)?;
        let url = reqwest::Url::parse(&receipt.url).map_err(|_| anyhow::anyhow!(INVALID))?;
        ensure!(
            url.username().is_empty() && url.password().is_none() && url.fragment().is_none(),
            INVALID
        );
        if payload.transport == "host_authorized_github_https" {
            ensure!(
                url.origin() == reqwest::Url::parse("https://api.github.com")?.origin(),
                INVALID
            );
        } else {
            ensure!(
                url.scheme() == "http"
                    && url
                        .host_str()
                        .and_then(|s| s.trim_matches(['[', ']']).parse::<std::net::IpAddr>().ok())
                        .is_some_and(|ip| ip.is_loopback())
                    && url.port_or_known_default().is_some_and(|port| port > 0),
                INVALID
            );
        }
        if let Some(prior) = &origin {
            ensure!(prior == &url.origin(), INVALID);
        } else {
            origin = Some(url.origin());
        }
        let root = format!("/repos/{}", payload.repository);
        ensure!(
            url.path() == root || url.path().starts_with(&(root.clone() + "/")),
            INVALID
        );
        let tail = url.path().strip_prefix(&(root.clone() + "/")).unwrap_or("");
        let paginated =
            tail == "pulls" || tail.ends_with("/comments") || tail.ends_with("/reviews");
        let mut query = BTreeMap::new();
        for (k, v) in url.query_pairs() {
            ensure!(
                query.insert(k.to_string(), v.to_string()).is_none(),
                INVALID
            );
        }
        if paginated {
            ensure!(query.remove("per_page").as_deref() == Some("100"), INVALID);
            let page = query
                .remove("page")
                .context(INVALID)?
                .parse::<usize>()
                .context(INVALID)?;
            ensure!(
                (1..=20).contains(&page)
                    && pages.entry(url.path().into()).or_default().insert(page),
                INVALID
            );
            if tail == "pulls" {
                ensure!(
                    query.remove("state").as_deref() == Some("closed")
                        && query.remove("sort").as_deref() == Some("updated")
                        && query.remove("direction").as_deref() == Some("desc"),
                    INVALID
                );
            }
        }
        ensure!(query.is_empty(), INVALID);
        *counts.entry(url.path().to_owned()).or_insert(0) += 1;
    }
    for (path, numbers) in pages {
        ensure!(numbers == (1..=counts[&path]).collect(), INVALID);
    }
    ensure!(body_bytes <= payload.response_bytes, INVALID);
    Ok(counts)
}

struct Budget {
    observations: usize,
    text_bytes: usize,
    profile: Profile,
}
impl Budget {
    fn text(&mut self, value: &str) -> Result<()> {
        ensure!(
            !crate::paths::obvious_secret(value),
            "github_discussions_text_refused"
        );
        self.text_bytes = self.text_bytes.checked_add(value.len()).context(CAPACITY)?;
        ensure!(self.text_bytes <= self.profile.max_text_bytes, CAPACITY);
        Ok(())
    }
    fn observation(&mut self, item: &Observation) -> Result<()> {
        self.observations = self.observations.checked_add(1).context(CAPACITY)?;
        ensure!(self.observations <= self.profile.max_observations, CAPACITY);
        for value in [
            &item.body,
            &item.author,
            &item.path,
            &item.diff_hunk,
            &item.state,
        ]
        .into_iter()
        .flatten()
        {
            self.text(value)?;
        }
        ensure!(item.body_hash == item.body.as_ref().map(hash), INVALID);
        ensure!(
            item.author.as_ref().is_none_or(|s| s.len() <= 256)
                && item.path.as_ref().is_none_or(|s| s.len() <= 4096)
                && item.state.as_ref().is_none_or(|s| s.len() <= 64),
            INVALID
        );
        for value in [&item.created_at, &item.updated_at, &item.submitted_at]
            .into_iter()
            .flatten()
        {
            timestamp(value)?;
            self.text(value)?;
        }
        for value in [&item.commit_id, &item.original_commit_id]
            .into_iter()
            .flatten()
        {
            sha(value)?;
        }
        if let Some(anchor) = &item.anchor {
            for value in [&anchor.side, &anchor.start_side, &anchor.subject_type]
                .into_iter()
                .flatten()
            {
                ensure!(value.len() <= 32, INVALID);
                self.text(value)?;
            }
        }
        Ok(())
    }
}
fn sha(value: &str) -> Result<()> {
    ensure!(
        value.len() == 40 && value.bytes().all(|b| b.is_ascii_hexdigit()),
        INVALID
    );
    Ok(())
}
fn timestamp(value: &str) -> Result<()> {
    ensure!(value.len() <= 64, INVALID);
    time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
        .map_err(|_| anyhow::anyhow!(INVALID))?;
    Ok(())
}
fn optional(raw: &Value, field: &str) -> Result<Option<String>> {
    if raw[field].is_null() {
        return Ok(None);
    }
    Ok(Some(raw[field].as_str().context(INVALID)?.to_owned()))
}
fn scope_url(value: &Value, origin: &reqwest::Url, path: &str) -> Result<()> {
    let url = reqwest::Url::parse(value.as_str().context(INVALID)?)?;
    ensure!(
        url.origin() == origin.origin()
            && url.path().eq_ignore_ascii_case(path)
            && url.query().is_none()
            && url.fragment().is_none()
            && url.username().is_empty()
            && url.password().is_none(),
        INVALID
    );
    Ok(())
}
fn project_pull(raw: &Value, repo: &str, repo_id: u64, number: u64) -> Result<Pull> {
    ensure!(
        raw["number"].as_u64() == Some(number)
            && raw["state"] == "closed"
            && raw["base"]["repo"]["id"].as_u64() == Some(repo_id)
            && raw["base"]["repo"]["full_name"]
                .as_str()
                .is_some_and(|s| s.eq_ignore_ascii_case(repo)),
        INVALID
    );
    let head = raw["head"]["sha"].as_str().context(INVALID)?;
    let base = raw["base"]["sha"].as_str().context(INVALID)?;
    sha(head)?;
    sha(base)?;
    let closed = raw["closed_at"].as_str().context(INVALID)?;
    let updated = raw["updated_at"].as_str().context(INVALID)?;
    timestamp(closed)?;
    timestamp(updated)?;
    Ok(Pull {
        provider_id: raw["id"].as_u64().filter(|n| *n > 0).context(INVALID)?,
        number,
        title: raw["title"].as_str().context(INVALID)?.into(),
        head_sha: head.into(),
        base_sha: base.into(),
        closed_at: closed.into(),
        updated_at: updated.into(),
        merged: raw["merged"].as_bool().context(INVALID)?,
        observations: vec![],
        coverage: Channels {
            issue_comment: empty_coverage(),
            review: empty_coverage(),
            review_comment: empty_coverage(),
        },
    })
}
fn empty_coverage() -> Coverage {
    Coverage {
        complete: false,
        reason: "request_budget_exhausted".into(),
        pages: 0,
    }
}
fn observation(kind: &str, raw: &Value) -> Result<Observation> {
    ensure!(raw.get("body").is_some(), INVALID);
    let body = optional(raw, "body")?;
    Ok(Observation {
        kind: kind.into(),
        id: raw["id"].as_u64().filter(|n| *n > 0).context(INVALID)?,
        body_hash: body.as_ref().map(hash),
        body,
        author: optional(&raw["user"], "login")?,
        created_at: optional(raw, "created_at")?,
        updated_at: optional(raw, "updated_at")?,
        submitted_at: optional(raw, "submitted_at")?,
        commit_id: optional(raw, "commit_id")?,
        original_commit_id: optional(raw, "original_commit_id")?,
        path: optional(raw, "path")?,
        diff_hunk: optional(raw, "diff_hunk")?,
        in_reply_to_id: optional_id(raw, "in_reply_to_id")?,
        review_id: optional_id(raw, "pull_request_review_id")?,
        state: optional(raw, "state")?,
        anchor: if kind == "review_comment" {
            Some(Anchor {
                line: integer(raw, "line")?,
                original_line: integer(raw, "original_line")?,
                start_line: integer(raw, "start_line")?,
                original_start_line: integer(raw, "original_start_line")?,
                position: integer(raw, "position")?,
                original_position: integer(raw, "original_position")?,
                side: optional(raw, "side")?,
                start_side: optional(raw, "start_side")?,
                subject_type: optional(raw, "subject_type")?,
            })
        } else {
            None
        },
    })
}
fn optional_id(raw: &Value, field: &str) -> Result<Option<u64>> {
    if raw[field].is_null() {
        Ok(None)
    } else {
        Ok(Some(
            raw[field].as_u64().filter(|n| *n > 0).context(INVALID)?,
        ))
    }
}
fn integer(raw: &Value, field: &str) -> Result<Option<i64>> {
    if raw[field].is_null() {
        Ok(None)
    } else {
        Ok(Some(raw[field].as_i64().context(INVALID)?))
    }
}

pub fn capture_host(profile: &Profile, request: &Request, d: &Deadline) -> Result<Capture> {
    profile.authorize(request)?;
    let client = ReadClient::production(&profile.read, d)?;
    capture(&client, profile, request, d)
}
pub fn capture(
    client: &ReadClient,
    profile: &Profile,
    input: &Request,
    d: &Deadline,
) -> Result<Capture> {
    d.check()?;
    let repo = profile.authorize(input)?;
    // A supplied client cannot widen its own host grant or budget through a
    // second profile. This also keeps fixture credentials/origins host-only.
    ensure!(
        serde_json::to_value(&client.profile)? == serde_json::to_value(&profile.read)?,
        INVALID
    );
    let mut request = Operation {
        client,
        deadline: d,
        requests: 0,
        bytes: 0,
        receipts: vec![],
    };
    let metadata = request.get(&format!("/repos/{repo}"), &[])?.value;
    ensure!(
        metadata["id"].as_u64() == Some(input.repository_id)
            && metadata["full_name"]
                .as_str()
                .is_some_and(|s| s.eq_ignore_ascii_case(&repo)),
        INVALID
    );
    let (numbers, discovery) = if input.pull_requests.is_empty() {
        let pages = request.pages(
            &repo,
            input.repository_id,
            "pulls",
            &[
                ("state", "closed".into()),
                ("sort", "updated".into()),
                ("direction", "desc".into()),
            ],
            None,
        )?;
        let mut numbers = vec![];
        for row in pages.items {
            ensure!(row["state"] == "closed", INVALID);
            let n = row["number"]
                .as_u64()
                .filter(|n| (1..=1_000_000).contains(n))
                .context(INVALID)?;
            ensure!(numbers.len() < 100 && !numbers.contains(&n), CAPACITY);
            numbers.push(n);
        }
        (numbers, pages.coverage)
    } else {
        (
            input.pull_requests.clone(),
            json!({"complete":true,"reason":"explicit_selected_pull_numbers","pages":0}),
        )
    };
    let discovery: Coverage = serde_json::from_value(discovery).context(INVALID)?;
    let mut complete = discovery.complete;
    let mut pulls = vec![];
    let mut unacquired = vec![];
    let mut budget = Budget {
        observations: 0,
        text_bytes: 0,
        profile: profile.clone(),
    };
    let mut numbers = numbers.into_iter();
    while let Some(number) = numbers.next() {
        d.check()?;
        if (client.profile.max_requests as usize).saturating_sub(request.requests) < 5 {
            unacquired.push(number);
            unacquired.extend(numbers);
            complete = false;
            break;
        }
        let path = format!("/repos/{repo}/pulls/{number}");
        let raw = request.get(&path, &[])?.value;
        let mut pull = project_pull(&raw, &repo, input.repository_id, number)?;
        scope_url(&raw["url"], &client.origin, &path)?;
        budget.text(&pull.title)?;
        let mut description = observation("description", &raw)?;
        description.commit_id = Some(pull.head_sha.clone());
        budget.observation(&description)?;
        pull.observations.push(description);
        let mut coverage = serde_json::Map::new();
        for (kind, endpoint, binding) in [
            (
                "issue_comment",
                format!("issues/{number}/comments"),
                format!("/repos/{repo}/issues/{number}"),
            ),
            ("review", format!("pulls/{number}/reviews"), path.clone()),
            (
                "review_comment",
                format!("pulls/{number}/comments"),
                path.clone(),
            ),
        ] {
            let pages =
                request.pages_reserved(&repo, input.repository_id, &endpoint, &[], None, 1)?;
            complete &= pages.coverage["complete"] == true;
            coverage.insert(
                match kind {
                    "issue_comment" => "issueComment",
                    "review_comment" => "reviewComment",
                    _ => "review",
                }
                .into(),
                pages.coverage,
            );
            for row in pages.items {
                scope_url(
                    &row[if kind == "issue_comment" {
                        "issue_url"
                    } else {
                        "pull_request_url"
                    }],
                    &client.origin,
                    &binding,
                )?;
                let item = observation(kind, &row)?;
                budget.observation(&item)?;
                pull.observations.push(item);
            }
        }
        let after_raw = request.get(&path, &[])?.value;
        let after = project_pull(&after_raw, &repo, input.repository_id, number)?;
        scope_url(&after_raw["url"], &client.origin, &path)?;
        ensure!(
            after.provider_id == pull.provider_id
                && after.number == pull.number
                && after.title == pull.title
                && after.head_sha == pull.head_sha
                && after.base_sha == pull.base_sha
                && after.closed_at == pull.closed_at
                && after.updated_at == pull.updated_at
                && after.merged == pull.merged
                && after_raw["body"] == raw["body"],
            CHANGED
        );
        pull.coverage = serde_json::from_value(Value::Object(coverage)).context(INVALID)?;
        pulls.push(pull);
    }
    let payload = Payload {
        repository: repo,
        repository_id: input.repository_id,
        requested_pulls: input.pull_requests.clone(),
        unacquired_pulls: unacquired,
        pulls,
        discovery_coverage: discovery,
        receipts: request
            .receipts
            .into_iter()
            .map(serde_json::from_value)
            .collect::<std::result::Result<Vec<Receipt>, _>>()
            .context(INVALID)?,
        acquired_at_unix: time::OffsetDateTime::now_utc().unix_timestamp(),
        atomic: false,
        transport: if client.fixture {
            "explicit_host_loopback_fixture"
        } else {
            "host_authorized_github_https"
        }
        .into(),
        requests: request.requests,
        response_bytes: request.bytes,
        text_bytes: budget.text_bytes,
        observations: budget.observations,
        coverage_complete: complete,
    };
    let result = Capture {
        schema_version: 1,
        kind: KIND.into(),
        id: encoded_id(&payload, d)?,
        payload,
    };
    result.validate(d)?;
    crate::json::encode_bounded(&result, profile.max_output_bytes, d).context(CAPACITY)?;
    Ok(result)
}
