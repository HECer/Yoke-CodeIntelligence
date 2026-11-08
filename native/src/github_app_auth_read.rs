//! Original-client installation-token reads; no route from remote JSON is followed.
use super::*;
use crate::github_app::Job;
use serde::{Deserialize, Serialize};

const STALE: &str = "github_app_worker_pull_request_stale";
const INVALID_RESPONSE: &str = "github_app_worker_provider_invalid";
#[derive(Clone, Copy)]
pub(crate) struct Limits {
    pub max_requests: usize,
    pub max_response_bytes: usize,
    pub max_total_bytes: usize,
    pub max_files: usize,
}
impl Limits {
    pub(crate) fn admit(&self) -> Result<()> {
        ensure!(
            (3..=33).contains(&self.max_requests)
                && (1..=2_000_000).contains(&self.max_response_bytes)
                && (1..=64_000_000).contains(&self.max_total_bytes)
                && (1..=3000).contains(&self.max_files),
            CAPACITY
        );
        Ok(())
    }
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PullRequest {
    pub number: u64,
    pub head_sha: String,
    pub base_sha: String,
    pub changed_files: usize,
    pub head_repository_id: Option<u64>,
    pub head_repository: Option<String>,
}
pub(crate) fn admit_job(client: &Client, job: &Job, d: &Deadline) -> Result<()> {
    d.check()?;
    client.profile.validate()?;
    ensure!(
        job.app_id == client.profile.app_id
            && numeric(job.number)
            && client
                .profile
                .grants
                .iter()
                .any(|g| g.installation_id == job.installation_id
                    && g.repository_id == job.repository_id
                    && g.repository == job.repository),
        BINDING
    );
    ensure!(
        client
            .profile
            .permissions
            .get("pull_requests")
            .is_some_and(|level| matches!(level.as_str(), "read" | "write")),
        BINDING
    );
    Ok(())
}
pub(crate) fn admit_publication(client: &Client, job: &Job, d: &Deadline) -> Result<()> {
    admit_job(client, job, d)?;
    ensure!(
        client.profile.permissions.len() == 1
            && client
                .profile
                .permissions
                .get("pull_requests")
                .is_some_and(|v| v == "write"),
        BINDING
    );
    Ok(())
}
pub(crate) fn admit_recovery(
    client: &Client,
    job: &Job,
    installation: u64,
    d: &Deadline,
) -> Result<()> {
    d.check()?;
    client.profile.validate()?;
    ensure!(
        job.app_id == client.profile.app_id
            && numeric(job.number)
            && numeric(installation)
            && client
                .profile
                .grants
                .iter()
                .any(|g| g.installation_id == installation
                    && g.repository_id == job.repository_id
                    && g.repository == job.repository)
            && client.profile.permissions.len() == 1
            && client
                .profile
                .permissions
                .get("pull_requests")
                .is_some_and(|v| matches!(v.as_str(), "read" | "write")),
        BINDING
    );
    Ok(())
}
pub(crate) struct CheckedComment {
    pub id: u64,
    pub html_url: String,
    pub fixture: bool,
}
pub(crate) struct Reader<'a> {
    run: transport::Run<'a>,
    lease: &'a InstallationLease,
    job: &'a Job,
    limits: Limits,
    phase: u8,
    initial: Option<PullRequest>,
    initial_observed_at: Option<u64>,
    final_observed_at: Option<u64>,
    recovery_installation: Option<u64>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Receipt {
    pub requests: usize,
    pub response_bytes: usize,
    pub api_version: String,
    pub fixture: bool,
    pub app_id: u64,
    pub installation_id: u64,
    pub repository_id: u64,
    pub repository: String,
    pub number: u64,
    pub head_sha: String,
    pub base_sha: String,
    /// Host token-clock checks after each complete metadata response, not server timestamps.
    pub initial_observed_at_unix: u64,
    pub final_observed_at_unix: u64,
}
impl<'a> Reader<'a> {
    pub(crate) fn request_count(&self) -> usize {
        self.run.requests
    }
    fn checked_comment(
        &self,
        value: &Value,
        expected_id: Option<u64>,
        body_hash: &str,
    ) -> Result<CheckedComment> {
        let id = value["id"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!(INVALID_RESPONSE))?;
        ensure!(
            numeric(id)
                && expected_id.is_none_or(|n| n == id)
                && value["user"]["type"] == "Bot"
                && value["user"]["id"].as_u64().is_some_and(numeric)
                && value["performed_via_github_app"]["id"].as_u64() == Some(self.job.app_id)
                && value["body"].as_str().is_some_and(
                    |s| s.len() <= 60_000 && crate::model::hash(s.as_bytes()) == body_hash
                ),
            INVALID_RESPONSE
        );
        let issue = format!(
            "https://api.github.com/repos/{}/issues/{}",
            self.job.repository, self.job.number
        );
        let api = format!(
            "https://api.github.com/repos/{}/issues/comments/{id}",
            self.job.repository
        );
        let numeric_api = format!(
            "https://api.github.com/repositories/{}/issues/comments/{id}",
            self.job.repository_id
        );
        let html = format!(
            "https://github.com/{}/pull/{}#issuecomment-{id}",
            self.job.repository, self.job.number
        );
        let html_issue = format!(
            "https://github.com/{}/issues/{}#issuecomment-{id}",
            self.job.repository, self.job.number
        );
        ensure!(
            info_url(value["issue_url"].as_str(), &issue)
                && (info_url(value["url"].as_str(), &api)
                    || info_url(value["url"].as_str(), &numeric_api))
                && (info_url(value["html_url"].as_str(), &html)
                    || info_url(value["html_url"].as_str(), &html_issue)),
            INVALID_RESPONSE
        );
        Ok(CheckedComment {
            id,
            html_url: html,
            fixture: self.run.client.fixture_clock.is_some(),
        })
    }
    pub(crate) fn owned_comment(
        &mut self,
        id: u64,
        body_hash: &str,
        max_requests: usize,
    ) -> Result<CheckedComment> {
        ensure!(
            self.phase == 4 && self.recovery_installation.is_none(),
            BINDING
        );
        self.read_comment(id, body_hash, max_requests.saturating_sub(1), 6)
    }
    pub(crate) fn unchanged_comment(
        &mut self,
        id: u64,
        body_hash: &str,
        max_requests: usize,
    ) -> Result<CheckedComment> {
        ensure!(
            self.phase == 4 && self.recovery_installation.is_none(),
            BINDING
        );
        self.read_comment(id, body_hash, max_requests, 8)
    }
    pub(crate) fn recovery_comment(
        &mut self,
        id: u64,
        body_hash: &str,
        max_requests: usize,
    ) -> Result<CheckedComment> {
        ensure!(
            self.phase == 1 && self.recovery_installation.is_some(),
            BINDING
        );
        self.read_comment(id, body_hash, max_requests, 8)
    }
    fn read_comment(
        &mut self,
        id: u64,
        body_hash: &str,
        max_requests: usize,
        next_phase: u8,
    ) -> Result<CheckedComment> {
        ensure!(numeric(id), BINDING);
        self.check_lease()?;
        self.phase = 9;
        let limits = transport::Limits {
            max_requests,
            max_response_bytes: self.limits.max_response_bytes,
            max_total_bytes: self.limits.max_total_bytes,
        };
        let response = self.run.scoped_get(
            &format!("/repos/{}/issues/comments/{id}", self.job.repository),
            &self.lease._token,
            limits,
            false,
        )?;
        self.check_lease()?;
        reject_token_reflection(&response.value.0, &self.lease._token, self.run.deadline)?;
        let checked = self.checked_comment(&response.value.0, Some(id), body_hash)?;
        self.phase = next_phase;
        Ok(checked)
    }
    pub(crate) fn write_comment(
        &mut self,
        body: &str,
        id: Option<u64>,
        max_requests: usize,
    ) -> Result<CheckedComment> {
        ensure!(
            self.recovery_installation.is_none()
                && ((id.is_none() && self.phase == 4)
                    || (id.is_some_and(numeric) && self.phase == 6)),
            BINDING
        );
        admit_publication(self.run.client, self.job, self.run.deadline)?;
        self.check_lease()?;
        ensure!(
            body.len() <= 60_000 && !body.contains(self.lease._token.as_str()),
            BINDING
        );
        let (path, status) = if let Some(id) = id {
            (
                format!("/repos/{}/issues/comments/{id}", self.job.repository),
                200,
            )
        } else {
            (
                format!(
                    "/repos/{}/issues/{}/comments",
                    self.job.repository, self.job.number
                ),
                201,
            )
        };
        let raw = crate::json::encode_bounded(&json!({"body":body}), 128_000, self.run.deadline)?;
        let limits = transport::Limits {
            max_requests,
            max_response_bytes: self.limits.max_response_bytes,
            max_total_bytes: self.limits.max_total_bytes,
        };
        self.phase = 5;
        let response =
            self.run
                .scoped_write(&path, &self.lease._token, raw, status, id.is_some(), limits)?;
        self.check_lease()?;
        reject_token_reflection(&response.value.0, &self.lease._token, self.run.deadline)?;
        self.checked_comment(&response.value.0, id, &crate::model::hash(body.as_bytes()))
    }
    pub(crate) fn new(
        client: &'a Client,
        lease: &'a InstallationLease,
        job: &'a Job,
        limits: Limits,
        d: &'a Deadline,
    ) -> Result<Self> {
        Self::scoped(client, lease, job, limits, None, d)
    }
    pub(crate) fn for_recovery(
        client: &'a Client,
        lease: &'a InstallationLease,
        job: &'a Job,
        installation: u64,
        limits: Limits,
        d: &'a Deadline,
    ) -> Result<Self> {
        Self::scoped(client, lease, job, limits, Some(installation), d)
    }
    fn scoped(
        client: &'a Client,
        lease: &'a InstallationLease,
        job: &'a Job,
        limits: Limits,
        recovery_installation: Option<u64>,
        d: &'a Deadline,
    ) -> Result<Self> {
        d.check()?;
        ensure!(
            tokio::runtime::Handle::try_current().is_err(),
            "github_app_requires_synchronous_host"
        );
        limits.admit()?;
        if let Some(installation) = recovery_installation {
            admit_recovery(client, job, installation, d)?;
        } else {
            admit_job(client, job, d)?;
        }
        let reader = Self {
            run: transport::Run {
                client,
                requests: 0,
                bytes: 0,
                deadline: d,
            },
            lease,
            job,
            limits,
            phase: 0,
            initial: None,
            initial_observed_at: None,
            final_observed_at: None,
            recovery_installation,
        };
        reader.check_lease()?;
        Ok(reader)
    }
    pub(crate) fn check_lease(&self) -> Result<()> {
        self.run.deadline.check()?;
        let client = self.run.client;
        let context = &self.lease.context;
        ensure!(
            Arc::ptr_eq(&context.client_identity, &client.identity)
                && context.origin == client.origin
                && context.app_id == self.job.app_id
                && context.grant.installation_id
                    == self
                        .recovery_installation
                        .unwrap_or(self.job.installation_id)
                && context.grant.repository_id == self.job.repository_id
                && context.grant.repository == self.job.repository
                && context.permissions == client.profile.permissions,
            BINDING
        );
        if let Some(installation) = self.recovery_installation {
            admit_recovery(client, self.job, installation, self.run.deadline)?;
        } else {
            admit_job(client, self.job, self.run.deadline)?;
        }
        let now = client.now()?;
        self.lease
            .observed
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |prior| {
                (now >= prior).then_some(now)
            })
            .map_err(|_| anyhow::anyhow!(BINDING))?;
        ensure!(
            now >= context.checked_at
                && context.expires_at
                    >= now
                        .checked_add(30)
                        .ok_or_else(|| anyhow::anyhow!(BINDING))?,
            BINDING
        );
        Ok(())
    }
    fn get(&mut self, path: &str, allow_links: bool) -> Result<transport::Response> {
        self.check_lease()?;
        let limits = transport::Limits {
            max_requests: self.limits.max_requests,
            max_response_bytes: self.limits.max_response_bytes,
            max_total_bytes: self.limits.max_total_bytes,
        };
        let response = self
            .run
            .scoped_get(path, &self.lease._token, limits, allow_links)?;
        self.check_lease()?;
        reject_token_reflection(&response.value.0, &self.lease._token, self.run.deadline)?;
        Ok(response)
    }
    pub(crate) fn repository(&mut self) -> Result<()> {
        ensure!(self.phase == 0, BINDING);
        let path = format!("/repos/{}", self.job.repository);
        let response = self.get(&path, false)?;
        ensure!(
            response.value.0["id"].as_u64() == Some(self.job.repository_id)
                && response.value.0["full_name"]
                    .as_str()
                    .is_some_and(|name| name.to_ascii_lowercase() == self.job.repository),
            BINDING
        );
        self.phase = 1;
        Ok(())
    }
    pub(crate) fn pull_request(&mut self) -> Result<PullRequest> {
        ensure!(
            self.recovery_installation.is_none() && matches!(self.phase, 1 | 3),
            BINDING
        );
        let path = format!("/repos/{}/pulls/{}", self.job.repository, self.job.number);
        let response = self.get(&path, false)?;
        let value = &response.value.0;
        ensure!(
            value["number"].as_u64() == Some(self.job.number)
                && value["base"]["repo"]["id"].as_u64() == Some(self.job.repository_id)
                && value["base"]["repo"]["full_name"]
                    .as_str()
                    .is_some_and(|name| name.to_ascii_lowercase() == self.job.repository),
            BINDING
        );
        let head = git_id(&value["head"]["sha"])?;
        let base = git_id(&value["base"]["sha"])?;
        ensure!(
            value["state"] == "open"
                && value["draft"].as_bool() == Some(false)
                && head == self.job.head_sha
                && base == self.job.base_sha,
            STALE
        );
        let changed_files = usize::try_from(
            value["changed_files"]
                .as_u64()
                .ok_or_else(|| anyhow::anyhow!(INVALID_RESPONSE))?,
        )?;
        ensure!(changed_files <= self.limits.max_files, CAPACITY);
        let (head_repository_id, head_repository) =
            if value["head"].get("repo").is_some_and(Value::is_null) {
                (None, None)
            } else {
                let id = value["head"]["repo"]["id"]
                    .as_u64()
                    .ok_or_else(|| anyhow::anyhow!(INVALID_RESPONSE))?;
                let name = value["head"]["repo"]["full_name"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!(INVALID_RESPONSE))?
                    .to_ascii_lowercase();
                ensure!(numeric(id) && repository(&name), INVALID_RESPONSE);
                (Some(id), Some(name))
            };
        let pr = PullRequest {
            number: self.job.number,
            head_sha: head,
            base_sha: base,
            changed_files,
            head_repository_id,
            head_repository,
        };
        if self.phase == 1 {
            self.initial = Some(pr.clone());
            self.initial_observed_at = Some(self.lease.observed.load(Ordering::SeqCst));
            self.phase = 2;
        } else {
            ensure!(self.initial.as_ref() == Some(&pr), STALE);
            self.final_observed_at = Some(self.lease.observed.load(Ordering::SeqCst));
            self.phase = 4;
        }
        Ok(pr)
    }
    pub(crate) fn files(&mut self, pr: &PullRequest) -> Result<Vec<Value>> {
        ensure!(
            self.phase == 2 && self.initial.as_ref() == Some(pr),
            BINDING
        );
        let pages = pr.changed_files.div_ceil(100);
        // Reserve the final metadata observation before fetching any file page.
        ensure!(
            self.run.requests.saturating_add(pages).saturating_add(1) <= self.limits.max_requests,
            CAPACITY
        );
        let mut files = Vec::new();
        let mut names = BTreeSet::new();
        let path = format!(
            "/repos/{}/pulls/{}/files",
            self.job.repository, self.job.number
        );
        for page in 1..=pages {
            self.run.deadline.check()?;
            let mut response = self.get(&format!("{path}?per_page=100&page={page}"), true)?;
            validate_links(&response.links, &self.run.client.origin, &path, page, pages)?;
            let Value::Array(rows) = std::mem::take(&mut response.value.0) else {
                anyhow::bail!(INVALID_RESPONSE)
            };
            let expected = (pr.changed_files - files.len()).min(100);
            ensure!(rows.len() == expected, INVALID_RESPONSE);
            for row in rows {
                self.run.deadline.check()?;
                let name = file_path(&row["filename"])?;
                ensure!(names.insert(name), INVALID_RESPONSE);
                git_id(&row["sha"])?;
                ensure!(
                    row["status"].as_str().is_some_and(|s| matches!(
                        s,
                        "added"
                            | "removed"
                            | "modified"
                            | "renamed"
                            | "copied"
                            | "changed"
                            | "unchanged"
                    )),
                    INVALID_RESPONSE
                );
                if let Some(previous) = row.get("previous_filename") {
                    file_path(previous)?;
                }
                let additions = row["additions"]
                    .as_u64()
                    .ok_or_else(|| anyhow::anyhow!(INVALID_RESPONSE))?;
                let deletions = row["deletions"]
                    .as_u64()
                    .ok_or_else(|| anyhow::anyhow!(INVALID_RESPONSE))?;
                ensure!(
                    additions <= 1_000_000
                        && deletions <= 1_000_000
                        && row["changes"].as_u64() == additions.checked_add(deletions),
                    INVALID_RESPONSE
                );
                if let Some(patch) = row.get("patch") {
                    ensure!(
                        patch.is_null() || patch.as_str().is_some_and(|text| text.len() <= 256_000),
                        CAPACITY
                    );
                }
                files.push(row);
            }
        }
        ensure!(files.len() == pr.changed_files, INVALID_RESPONSE);
        self.phase = 3;
        Ok(files)
    }
    pub(crate) fn evidence(&self) -> Result<Receipt> {
        ensure!(
            self.phase == 4 && self.recovery_installation.is_none(),
            BINDING
        );
        self.check_lease()?;
        Ok(Receipt {
            requests: self.run.requests,
            response_bytes: self.run.bytes,
            api_version: VERSION.into(),
            fixture: self.run.client.fixture_clock.is_some(),
            app_id: self.job.app_id,
            installation_id: self.job.installation_id,
            repository_id: self.job.repository_id,
            repository: self.job.repository.clone(),
            number: self.job.number,
            head_sha: self.job.head_sha.clone(),
            base_sha: self.job.base_sha.clone(),
            initial_observed_at_unix: self
                .initial_observed_at
                .ok_or_else(|| anyhow::anyhow!(BINDING))?,
            final_observed_at_unix: self
                .final_observed_at
                .ok_or_else(|| anyhow::anyhow!(BINDING))?,
        })
    }
}
fn info_url(raw: Option<&str>, expected: &str) -> bool {
    let Some(raw) = raw else { return false };
    let Ok(url) = reqwest::Url::parse(raw) else {
        return false;
    };
    let Ok(expected) = reqwest::Url::parse(expected) else {
        return false;
    };
    url.scheme() == "https"
        && url.host_str() == expected.host_str()
        && url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment() == expected.fragment()
        && url.path().eq_ignore_ascii_case(expected.path())
}
fn git_id(value: &Value) -> Result<String> {
    let id = value
        .as_str()
        .ok_or_else(|| anyhow::anyhow!(INVALID_RESPONSE))?;
    ensure!(
        id.len() == 40 && id.bytes().all(|b| b.is_ascii_hexdigit()),
        INVALID_RESPONSE
    );
    Ok(id.to_ascii_lowercase())
}
fn file_path(value: &Value) -> Result<String> {
    let path = value
        .as_str()
        .ok_or_else(|| anyhow::anyhow!(INVALID_RESPONSE))?;
    ensure!(
        path.len() <= 4096 && crate::paths::normalized_syntax(path)? == path,
        INVALID_RESPONSE
    );
    Ok(path.into())
}
fn reject_token_reflection(value: &Value, token: &str, d: &Deadline) -> Result<()> {
    let mut stack = vec![value];
    while let Some(value) = stack.pop() {
        d.check()?;
        match value {
            Value::String(text) => ensure!(!text.contains(token), BINDING),
            Value::Array(values) => stack.extend(values),
            Value::Object(values) => {
                ensure!(values.keys().all(|key| !key.contains(token)), BINDING);
                stack.extend(values.values());
            }
            _ => {}
        }
    }
    Ok(())
}
fn validate_links(
    headers: &[String],
    origin: &reqwest::Url,
    path: &str,
    page: usize,
    pages: usize,
) -> Result<()> {
    ensure!(headers.len() <= 1, INVALID_RESPONSE);
    let Some(header) = headers.first() else {
        return Ok(());
    };
    ensure!(pages > 1 && header.len() <= 16_384, INVALID_RESPONSE);
    let mut relations = BTreeSet::new();
    for entry in header.split(',') {
        let parts: Vec<_> = entry.trim().split(';').collect();
        ensure!(parts.len() == 2, INVALID_RESPONSE);
        let raw = parts[0]
            .trim()
            .strip_prefix('<')
            .and_then(|s| s.strip_suffix('>'))
            .ok_or_else(|| anyhow::anyhow!(INVALID_RESPONSE))?;
        let relation = parts[1]
            .trim()
            .strip_prefix("rel=\"")
            .and_then(|s| s.strip_suffix('"'))
            .ok_or_else(|| anyhow::anyhow!(INVALID_RESPONSE))?;
        ensure!(relations.insert(relation), INVALID_RESPONSE);
        let target = match relation {
            "first" => 1,
            "prev" if page > 1 => page - 1,
            "next" if page < pages => page + 1,
            "last" => pages,
            _ => anyhow::bail!(INVALID_RESPONSE),
        };
        let url = reqwest::Url::parse(raw).map_err(|_| anyhow::anyhow!(INVALID_RESPONSE))?;
        ensure!(
            url.as_str() == raw
                && url.origin() == origin.origin()
                && url.path() == path
                && url.username().is_empty()
                && url.password().is_none()
                && url.fragment().is_none()
                && url.query().is_some_and(|query| !query.contains('%')),
            INVALID_RESPONSE
        );
        let mut query = BTreeMap::new();
        for (key, value) in url.query_pairs() {
            ensure!(
                query.insert(key.into_owned(), value.into_owned()).is_none(),
                INVALID_RESPONSE
            );
        }
        ensure!(
            query.len() == 2
                && query.get("per_page").is_some_and(|v| v == "100")
                && query.get("page").is_some_and(|v| *v == target.to_string()),
            INVALID_RESPONSE
        );
    }
    ensure!(
        page == pages || relations.contains("next"),
        INVALID_RESPONSE
    );
    Ok(())
}
