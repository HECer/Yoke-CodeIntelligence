//! Host-authorized, bounded, read-only GitHub REST observations.
use crate::{Engine, capture::Deadline};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::net::SocketAddr;
use zeroize::Zeroizing;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct HostReadProfile {
    pub allow_network: bool,
    pub allowed_repos: Vec<String>,
    pub default_repo: Option<String>,
    pub credential_ref: Option<String>,
    pub max_pages: u16,
    pub max_items: u16,
    pub max_response_bytes: u32,
    pub aggregate_response_bytes: u32,
    pub max_requests: u16,
    pub stale_after_days: u16,
}
impl Default for HostReadProfile {
    fn default() -> Self {
        Self {
            allow_network: false,
            allowed_repos: vec![],
            default_repo: None,
            credential_ref: None,
            max_pages: 5,
            max_items: 100,
            max_response_bytes: 4 * 1024 * 1024,
            aggregate_response_bytes: 32 * 1024 * 1024,
            max_requests: 40,
            stale_after_days: 30,
        }
    }
}
impl HostReadProfile {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.allowed_repos.len() <= 32,
            "GitHub repository scope capacity exceeded"
        );
        let mut repositories = BTreeSet::new();
        for repo in &self.allowed_repos {
            ensure!(
                repositories.insert(repository(repo)?),
                "Duplicate GitHub repository scope"
            );
        }
        ensure!(
            !self.allow_network || !repositories.is_empty(),
            "GitHub network grant requires explicit repositories"
        );
        if let Some(default) = &self.default_repo {
            ensure!(
                repositories.contains(&repository(default)?),
                "Default GitHub repository is not allowed"
            );
        }
        if let Some(reference) = &self.credential_ref {
            credential_name(reference)?;
        }
        ensure!(
            (1..=20).contains(&self.max_pages)
                && (1..=1000).contains(&self.max_items)
                && (1..=200).contains(&self.max_requests),
            "GitHub request/page/item budget invalid"
        );
        ensure!(
            (256..=8 * 1024 * 1024).contains(&self.max_response_bytes)
                && (self.max_response_bytes..=64 * 1024 * 1024)
                    .contains(&self.aggregate_response_bytes)
                && (1..=365).contains(&self.stale_after_days),
            "GitHub response/freshness budget invalid"
        );
        Ok(())
    }
    pub fn authorized_repo(&self, requested: Option<&str>) -> Result<String> {
        self.validate()?;
        let repo = repository(requested.or(self.default_repo.as_deref()).context(
            "GitHub repository requires an explicit host defaultRepo or repo argument",
        )?)?;
        ensure!(
            self.allowed_repos
                .iter()
                .any(|allowed| allowed.eq_ignore_ascii_case(&repo)),
            "GitHub repository is not host-allowed"
        );
        Ok(repo)
    }
}
fn repository(value: &str) -> Result<String> {
    let (owner, repo) = value
        .split_once('/')
        .context("GitHub repository must be owner/repo")?;
    ensure!(
        !owner.is_empty()
            && owner.len() <= 39
            && !repo.is_empty()
            && repo.len() <= 100
            && owner
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
            && !owner.starts_with('-')
            && !owner.ends_with('-')
            && repo
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
            && !matches!(repo, "." | ".."),
        "Invalid GitHub repository syntax"
    );
    Ok(value.to_ascii_lowercase())
}
fn credential_name(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.len() <= 80
            && value.as_bytes()[0].is_ascii_alphanumeric()
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.')),
        "Invalid GitHub OS credential reference"
    );
    Ok(())
}
/// Explicit native-host test authority; never deserialized from a profile or tool call.
#[derive(Clone, Debug)]
pub struct HostTestProfile {
    endpoint: SocketAddr,
}
impl HostTestProfile {
    pub fn loopback(endpoint: SocketAddr) -> Result<Self> {
        ensure!(
            endpoint.ip().is_loopback() && endpoint.port() != 0,
            "Fixture HTTP requires an explicit numeric loopback endpoint"
        );
        Ok(Self { endpoint })
    }
}
pub struct ReadClient {
    profile: HostReadProfile,
    origin: reqwest::Url,
    client: reqwest::blocking::Client,
    token: Option<Zeroizing<String>>,
    fixture: bool,
}
impl ReadClient {
    fn production(profile: &HostReadProfile, deadline: &Deadline) -> Result<Self> {
        deadline.check()?;
        profile.validate()?;
        ensure!(
            profile.allow_network,
            "Hosted PR read provider unavailable: host network permission required"
        );
        let token = profile
            .credential_ref
            .as_deref()
            .map(crate::host_credentials::read_github_token)
            .transpose()?;
        Self::build(
            profile,
            reqwest::Url::parse("https://api.github.com")?,
            token,
            false,
            deadline,
        )
    }
    fn build(
        profile: &HostReadProfile,
        origin: reqwest::Url,
        token: Option<Zeroizing<String>>,
        fixture: bool,
        deadline: &Deadline,
    ) -> Result<Self> {
        deadline.check()?;
        if rustls::crypto::CryptoProvider::get_default().is_none() {
            // The crate intentionally disables reqwest's AWS-LC default and
            // reuses Yoke's existing native ring implementation.
            let _ = rustls::crypto::ring::default_provider().install_default();
        }
        ensure!(
            rustls::crypto::CryptoProvider::get_default().is_some(),
            "GitHub TLS crypto provider is unavailable"
        );
        let client = reqwest::blocking::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .no_proxy()
            .https_only(!fixture)
            .timeout(deadline.remaining())
            .build()
            .map_err(|_| anyhow::anyhow!("GitHub HTTP client could not be initialized"))?;
        Ok(Self {
            profile: profile.clone(),
            origin,
            client,
            token,
            fixture,
        })
    }
    pub fn for_fixture(
        profile: &HostReadProfile,
        test: &HostTestProfile,
        deadline: &Deadline,
    ) -> Result<Self> {
        deadline.check()?;
        profile.validate()?;
        ensure!(
            profile.credential_ref.is_none(),
            "Fixture HTTP cannot read production credentials"
        );
        Self::build(
            profile,
            reqwest::Url::parse(&format!("http://{}", test.endpoint))?,
            None,
            true,
            deadline,
        )
    }
    pub fn query(
        &mut self,
        engine: &mut Engine,
        operation: &str,
        args: &Value,
        deadline: &Deadline,
    ) -> Result<Value> {
        deadline.check()?;
        validate_arguments(operation, args)?;
        self.profile.validate()?;
        ensure!(
            self.profile.allow_network,
            "Hosted PR read provider unavailable: host network permission required"
        );
        let repo = self.profile.authorized_repo(args["repo"].as_str())?;
        ensure!(
            matches!(operation, "list_prs" | "get_pr_impact" | "triage_prs"),
            "Unknown GitHub read operation"
        );
        let mut request = Operation {
            client: self,
            deadline,
            requests: 0,
            bytes: 0,
            receipts: vec![],
        };
        let metadata = request.get(&format!("/repos/{repo}"), &[])?;
        ensure!(
            metadata.value["full_name"]
                .as_str()
                .is_some_and(|name| name.eq_ignore_ascii_case(&repo)),
            "GitHub repository metadata scope mismatch"
        );
        let repo_id = metadata.value["id"]
            .as_u64()
            .context("GitHub repository ID unavailable")?;
        let base = if let Some(base) = args["base"].as_str() {
            reference(base)?.to_owned()
        } else {
            reference(
                metadata.value["default_branch"]
                    .as_str()
                    .context("GitHub default branch unavailable")?,
            )?
            .to_owned()
        };
        if operation == "get_pr_impact" {
            let number = args["pr_number"]
                .as_u64()
                .filter(|n| *n > 0 && *n <= 1_000_000)
                .context("Invalid GitHub PR number")?;
            let raw = request
                .get(&format!("/repos/{repo}/pulls/{number}"), &[])?
                .value;
            let mut item = pull(&raw, &repo)?;
            ensure!(item["number"] == number, "GitHub PR number mismatch");
            let head = item["headSha"].as_str().unwrap().to_owned();
            let files =
                request.pages(&repo, repo_id, &format!("pulls/{number}/files"), &[], None)?;
            item["ci"] = request.ci(&repo, repo_id, &head)?;
            item["reviews"] = request.reviews(&repo, repo_id, number, &head)?;
            let current = pull(
                &request
                    .get(&format!("/repos/{repo}/pulls/{number}"), &[])?
                    .value,
                &repo,
            )?;
            ensure!(
                item["headSha"] == current["headSha"]
                    && item["baseSha"] == current["baseSha"]
                    && item["baseRef"] == current["baseRef"],
                "GitHub PR changed during source observation; repeat the query"
            );
            let mut impact = source_impact(engine, &files.items, &head, deadline)?;
            impact["remoteFileCoverage"] = files.coverage.clone();
            item["impact"] = impact;
            item["repo"] = json!(repo);
            item["producer"] = json!("yoke-github-read-v1");
            item["coverage"] = json!({"files":files.coverage,"headConsistency":"revalidated_after_files_ci_reviews"});
            item["budget"] = json!({"requests":request.requests,"responseBytes":request.bytes,"maxRequests":request.client.profile.max_requests});
            item["receipts"] = json!(request.receipts);
            item["complete"] = json!(false);
            return Ok(item);
        }
        let pulls = request.pages(
            &repo,
            repo_id,
            "pulls",
            &[("state", "open".into()), ("base", base.clone())],
            None,
        )?;
        let mut items = vec![];
        let mut excluded = vec![];
        let observed_at = time::OffsetDateTime::now_utc();
        for raw in &pulls.items {
            request.deadline.check()?;
            let mut item = pull(raw, &repo)?;
            let number = item["number"].as_u64().unwrap();
            let head = item["headSha"].as_str().unwrap().to_owned();
            let age = utc_timestamp(item["updatedAt"].as_str().unwrap())
                .filter(|updated| *updated <= observed_at)
                .map(|updated| (observed_at - updated).whole_days());
            item["freshness"] = json!({"updatedAgeDays":age,"staleAfterDays":request.client.profile.stale_after_days,"policy":"host_update_age_not_merge_readiness"});
            if operation == "triage_prs" {
                let reason = if item["baseRef"] != base {
                    Some("wrong_base")
                } else if age.is_none() {
                    Some("timestamp_invalid_or_future")
                } else if age
                    .is_some_and(|days| days >= request.client.profile.stale_after_days as i64)
                {
                    Some("stale_by_host_update_age_policy")
                } else {
                    None
                };
                if let Some(reason) = reason {
                    excluded.push(
                        json!({"number":number,"reason":reason,"freshness":item["freshness"]}),
                    );
                    continue;
                }
            }
            item["ci"] = request.ci(&repo, repo_id, &head)?;
            item["reviews"] = request.reviews(&repo, repo_id, number, &head)?;
            let impact = (|| -> Result<Value> {
                let files =
                    request.pages(&repo, repo_id, &format!("pulls/{number}/files"), &[], None)?;
                let current = pull(
                    &request
                        .get(&format!("/repos/{repo}/pulls/{number}"), &[])?
                        .value,
                    &repo,
                )?;
                ensure!(
                    current["headSha"] == item["headSha"]
                        && current["baseSha"] == item["baseSha"]
                        && current["baseRef"] == item["baseRef"],
                    "PR changed during observation"
                );
                let mut impact = source_impact(engine, &files.items, &head, deadline)?;
                impact["remoteFileCoverage"] = files.coverage;
                impact["headConsistency"] = json!("revalidated_after_files_ci_reviews");
                Ok(impact)
            })();
            item["impact"] = match impact {
                Ok(value) => value,
                Err(_) => {
                    deadline.check()?;
                    json!({"state":"unavailable","coverage":{"complete":false},"reason":"files_source_or_head_consistency_unavailable_or_budget_exhausted"})
                }
            };
            items.push(item);
        }
        let mut overlaps = vec![];
        for (index, left) in items.iter().enumerate() {
            let left_paths: BTreeSet<_> = left["impact"]["files"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|row| row["path"].as_str())
                .collect();
            for right in &items[index + 1..] {
                let paths: Vec<_> = right["impact"]["files"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|row| row["path"].as_str())
                    .filter(|path| left_paths.contains(path))
                    .collect();
                if !paths.is_empty() {
                    overlaps.push(json!({"leftPr":left["number"],"rightPr":right["number"],"paths":paths,"certainty":"observed_changed_path_overlap_not_merge_conflict_proof"}));
                }
            }
        }
        Ok(
            json!({"producer":"yoke-github-read-v1","repo":repo,"base":base,"items":items,"excluded":excluded,"overlaps":overlaps,"coverage":{"pulls":pulls.coverage,"impact":"per_item_explicit"},"budget":{"requests":request.requests,"responseBytes":request.bytes,"maxRequests":request.client.profile.max_requests,"aggregateResponseBytes":request.client.profile.aggregate_response_bytes},"receipts":request.receipts,"transport":if request.client.fixture{"explicit_host_loopback_fixture"}else{"host_authorized_github_https"},"complete":false,"limitations":"Remote metadata is an observed provider claim; CI/reviews do not attest merge policy, execution safety or exhaustive local graph impact."}),
        )
    }
}
fn utc_timestamp(value: &str) -> Option<time::OffsetDateTime> {
    let bytes = value.as_bytes();
    if bytes.len() != 20
        || !value.is_ascii()
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes[10] != b'T'
        || bytes[13] != b':'
        || bytes[16] != b':'
        || bytes[19] != b'Z'
    {
        return None;
    }
    let date = time::Date::from_calendar_date(
        value[..4].parse().ok()?,
        time::Month::try_from(value[5..7].parse::<u8>().ok()?).ok()?,
        value[8..10].parse().ok()?,
    )
    .ok()?;
    let clock = time::Time::from_hms(
        value[11..13].parse().ok()?,
        value[14..16].parse().ok()?,
        value[17..19].parse().ok()?,
    )
    .ok()?;
    Some(date.with_time(clock).assume_utc())
}
pub fn query(
    profile: &HostReadProfile,
    engine: &mut Engine,
    operation: &str,
    args: &Value,
    deadline: &Deadline,
) -> Result<Value> {
    deadline.check()?;
    validate_arguments(operation, args)?;
    profile.validate()?;
    ensure!(
        profile.allow_network,
        "Hosted PR read provider unavailable: host network permission required"
    );
    profile.authorized_repo(args["repo"].as_str())?;
    ReadClient::production(profile, deadline)?.query(engine, operation, args, deadline)
}

fn validate_arguments(operation: &str, args: &Value) -> Result<()> {
    ensure!(
        matches!(operation, "list_prs" | "get_pr_impact" | "triage_prs"),
        "Unknown GitHub read operation"
    );
    let object = args
        .as_object()
        .context("GitHub arguments must be an object")?;
    for (key, value) in object {
        match key.as_str() {
            "repo" => {
                repository(value.as_str().context("GitHub repo must be a string")?)?;
            }
            "project_path" => {
                ensure!(
                    value
                        .as_str()
                        .is_some_and(|s| !s.is_empty() && s.len() <= 4096),
                    "Invalid registered project selector"
                );
            }
            "base" if operation != "get_pr_impact" => {
                reference(value.as_str().context("GitHub base must be a string")?)?;
            }
            "pr_number" if operation == "get_pr_impact" => {
                ensure!(
                    value.as_u64().is_some_and(|n| n > 0 && n <= 1_000_000),
                    "Invalid GitHub PR number"
                );
            }
            _ => bail!("Unsupported GitHub operation argument"),
        }
    }
    ensure!(
        operation != "get_pr_impact" || object.contains_key("pr_number"),
        "GitHub PR number required"
    );
    Ok(())
}

fn reference(value: &str) -> Result<&str> {
    ensure!(
        !value.is_empty()
            && value.len() <= 1024
            && !value.starts_with(['-', '/'])
            && !value.ends_with('/')
            && !value.contains(['\\', ':', '~', '^', '?', '*', '['])
            && !value.contains("..")
            && !value.contains("@{")
            && !value.chars().any(|c| c.is_control() || c.is_whitespace())
            && value
                .split('/')
                .all(|part| !part.is_empty() && part != "." && !part.ends_with(".lock")),
        "Invalid GitHub branch reference"
    );
    Ok(value)
}
fn oid(value: &Value) -> Result<&str> {
    let value = value.as_str().context("GitHub object ID missing")?;
    ensure!(
        value.len() == 40 && value.bytes().all(|b| b.is_ascii_hexdigit()),
        "Invalid GitHub object ID"
    );
    Ok(value)
}
fn safe_text(value: &Value, max: usize) -> Result<String> {
    let value = value.as_str().context("GitHub metadata text missing")?;
    ensure!(
        value.len() <= max
            && !crate::paths::obvious_secret(value)
            && !value
                .chars()
                .any(|c| c.is_control() && c != '\n' && c != '\t'),
        "GitHub metadata omitted: unsafe text or capacity"
    );
    Ok(value.into())
}
fn pull(raw: &Value, repo: &str) -> Result<Value> {
    let number = raw["number"]
        .as_u64()
        .filter(|n| *n > 0 && *n <= 1_000_000)
        .context("Invalid GitHub PR number")?;
    ensure!(
        raw["base"]["repo"]["full_name"]
            .as_str()
            .is_some_and(|value| value.eq_ignore_ascii_case(repo)),
        "GitHub PR base repository scope mismatch"
    );
    Ok(
        json!({"number":number,"title":safe_text(&raw["title"],4096)?,"state":safe_text(&raw["state"],32)?,"draft":raw["draft"].as_bool(),"updatedAt":safe_text(&raw["updated_at"],64)?,"baseRef":reference(raw["base"]["ref"].as_str().context("PR base ref missing")?)?,"baseSha":oid(&raw["base"]["sha"] )?,"headRef":reference(raw["head"]["ref"].as_str().context("PR head ref missing")?)?,"headSha":oid(&raw["head"]["sha"] )?,"author":safe_text(&raw["user"]["login"],256)?}),
    )
}
struct Response {
    value: Value,
    link: Option<String>,
}
struct Pages {
    items: Vec<Value>,
    coverage: Value,
}
struct Operation<'a> {
    client: &'a ReadClient,
    deadline: &'a Deadline,
    requests: usize,
    bytes: usize,
    receipts: Vec<Value>,
}
impl Operation<'_> {
    fn get(&mut self, path: &str, params: &[(&str, String)]) -> Result<Response> {
        self.deadline.check()?;
        ensure!(
            self.requests < self.client.profile.max_requests as usize,
            "GitHub operation request budget exhausted"
        );
        ensure!(
            self.bytes < self.client.profile.aggregate_response_bytes as usize,
            "GitHub operation aggregate response budget exhausted"
        );
        let mut url = self.client.origin.clone();
        url.set_path(path);
        url.query_pairs_mut()
            .extend_pairs(params.iter().map(|(k, v)| (*k, v.as_str())));
        let mut builder = self
            .client
            .client
            .get(url.clone())
            .timeout(self.deadline.remaining())
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2026-03-10")
            .header("User-Agent", "Yoke-CodeIntelligence/0.5 readonly");
        if let Some(token) = &self.client.token {
            let mut header =
                reqwest::header::HeaderValue::from_str(&format!("Bearer {}", token.as_str()))
                    .map_err(|_| {
                        anyhow::anyhow!("GitHub credential cannot form an authorization header")
                    })?;
            header.set_sensitive(true);
            builder = builder.header(reqwest::header::AUTHORIZATION, header);
        }
        self.requests += 1;
        let mut response = builder.send().map_err(|error| {
            if self.client.fixture {
                anyhow::anyhow!("Fixture GitHub request failed: {:?}", error.without_url())
            } else {
                anyhow::anyhow!("GitHub read request failed or exceeded its deadline")
            }
        })?;
        ensure!(
            response.url().as_str() == url.as_str(),
            "GitHub response URL changed"
        );
        ensure!(
            response.status().is_success(),
            "GitHub read returned HTTP {}; metadata coverage is unavailable",
            response.status().as_u16()
        );
        let mut header_bytes = 0usize;
        for (key, value) in response.headers() {
            header_bytes = header_bytes
                .checked_add(key.as_str().len() + value.as_bytes().len())
                .context("GitHub header size overflow")?;
        }
        ensure!(
            header_bytes <= 16384,
            "GitHub response headers exceed capacity"
        );
        self.bytes = self
            .bytes
            .checked_add(header_bytes)
            .context("GitHub response size overflow")?;
        let cap = (self.client.profile.max_response_bytes as usize).min(
            (self.client.profile.aggregate_response_bytes as usize).saturating_sub(self.bytes),
        );
        if let Some(length) = response.content_length() {
            ensure!(
                length <= cap as u64,
                "GitHub response content length exceeds operation budget"
            );
        }
        let link = response
            .headers()
            .get(reqwest::header::LINK)
            .map(|value| value.to_str().map(str::to_owned))
            .transpose()
            .map_err(|_| anyhow::anyhow!("Invalid GitHub pagination header"))?;
        let mut bytes = Vec::new();
        let mut chunk = [0u8; 16384];
        loop {
            self.deadline.check()?;
            let count = response.read(&mut chunk).map_err(|_| {
                anyhow::anyhow!("GitHub response read failed or exceeded its deadline")
            })?;
            if count == 0 {
                break;
            }
            ensure!(
                bytes.len().saturating_add(count) <= cap,
                "GitHub response bytes exceed operation budget"
            );
            bytes.extend_from_slice(&chunk[..count]);
        }
        self.bytes += bytes.len();
        self.deadline.check()?;
        if let Some(token) = &self.client.token {
            ensure!(
                !bytes
                    .windows(token.len())
                    .any(|window| window == token.as_bytes()),
                "GitHub response rejected: credential material in remote data"
            );
        }
        json_admission(&bytes, self.deadline)?;
        let value = crate::json::parse(&bytes, cap)?;
        self.receipts.push(json!({"url":url.as_str(),"responseHash":crate::model::hash(&bytes),"responseBytes":bytes.len(),"apiVersion":"2026-03-10","bodyRetained":false}));
        Ok(Response { value, link })
    }
    fn pages(
        &mut self,
        repo: &str,
        repo_id: u64,
        endpoint: &str,
        params: &[(&str, String)],
        field: Option<&str>,
    ) -> Result<Pages> {
        let path = format!("/repos/{repo}/{endpoint}");
        let mut items = vec![];
        let mut seen = BTreeMap::new();
        let mut pages = 0usize;
        let mut complete = false;
        let mut reason = "page_budget_exhausted";
        for page in 1..=self.client.profile.max_pages {
            self.deadline.check()?;
            if self.requests >= self.client.profile.max_requests as usize {
                reason = "request_budget_exhausted";
                break;
            }
            let mut query = params.to_vec();
            query.extend([("per_page", 100.to_string()), ("page", page.to_string())]);
            let response = self.get(&path, &query)?;
            pages += 1;
            let rows = if let Some(field) = field {
                response.value[field].as_array()
            } else {
                response.value.as_array()
            }
            .context("GitHub paginated response shape invalid")?;
            ensure!(
                rows.len() <= 100,
                "GitHub page item count exceeds requested per_page"
            );
            for row in rows {
                let key = if endpoint.ends_with("/files") {
                    format!("file:{}", safe_text(&row["filename"], 4096)?)
                } else {
                    format!(
                        "id:{}",
                        row["id"]
                            .as_u64()
                            .or_else(|| row["number"].as_u64())
                            .context("GitHub page item ID missing")?
                    )
                };
                if let Some(previous) = seen.get(&key) {
                    ensure!(
                        previous == row,
                        "GitHub repeated item changed during pagination"
                    );
                    continue;
                }
                if items.len() == self.client.profile.max_items as usize {
                    reason = "item_budget_exhausted";
                    return Ok(Pages {
                        items,
                        coverage: json!({"complete":false,"reason":reason,"pages":pages}),
                    });
                }
                seen.insert(key, row.clone());
                items.push(row.clone());
            }
            if let Some(link) = &response.link
                && next_page(
                    link,
                    &self.client.origin,
                    repo,
                    repo_id,
                    endpoint,
                    &query,
                    page as usize,
                )?
                .is_some()
            {
                continue;
            }
            if let Some(total) = response.value["total_count"].as_u64()
                && total > items.len() as u64
            {
                reason = "missing_next_page_receipt";
                break;
            }
            complete = true;
            reason = "provider_reported_pages_complete";
            break;
        }
        Ok(Pages {
            items,
            coverage: json!({"complete":complete,"reason":reason,"pages":pages}),
        })
    }
    fn ci(&mut self, repo: &str, repo_id: u64, head: &str) -> Result<Value> {
        let status = self.get(&format!("/repos/{repo}/commits/{head}/status"), &[]);
        let checks = self.pages(
            repo,
            repo_id,
            &format!("commits/{head}/check-runs"),
            &[("filter", "latest".into())],
            Some("check_runs"),
        );
        let (status, checks) = match (status, checks) {
            (Ok(status), Ok(checks)) => (status, checks),
            _ => {
                return Ok(
                    json!({"state":"unknown","coverage":{"complete":false,"reason":"status_or_checks_unavailable"},"requiredChecksPolicy":"unknown"}),
                );
            }
        };
        ensure!(status.value["sha"] == head, "GitHub status commit mismatch");
        let statuses = status.value["statuses"]
            .as_array()
            .context("GitHub statuses missing")?;
        let count = statuses.len() + checks.items.len();
        let all = statuses.iter().all(|s| s["state"] == "success")
            && checks
                .items
                .iter()
                .all(|c| c["status"] == "completed" && c["conclusion"] == "success");
        for check in &checks.items {
            ensure!(check["head_sha"] == head, "GitHub check commit mismatch");
        }
        let complete = checks.coverage["complete"] == true
            && status.value["total_count"].as_u64() == Some(statuses.len() as u64);
        Ok(
            json!({"state":if !complete||count==0{"unknown"}else if all{"all_observed_success"}else{"observed_non_success"},"headSha":head,"observedStatuses":statuses.len(),"observedChecks":checks.items.len(),"coverage":{"complete":complete,"checks":checks.coverage},"requiredChecksPolicy":"unknown"}),
        )
    }
    fn reviews(&mut self, repo: &str, repo_id: u64, number: u64, head: &str) -> Result<Value> {
        let pages = match self.pages(repo, repo_id, &format!("pulls/{number}/reviews"), &[], None) {
            Ok(pages) => pages,
            Err(_) => {
                return Ok(
                    json!({"state":"unknown","coverage":{"complete":false,"reason":"reviews_unavailable"},"requiredReviewPolicy":"unknown"}),
                );
            }
        };
        let mut latest = BTreeMap::new();
        let mut stale = 0usize;
        for row in &pages.items {
            self.deadline.check()?;
            if row["commit_id"] != head {
                stale += 1;
                continue;
            }
            let login = safe_text(&row["user"]["login"], 256)?;
            let state = safe_text(&row["state"], 64)?;
            if state != "COMMENTED" && state != "PENDING" {
                latest.insert(login, state);
            }
        }
        let state = if pages.coverage["complete"] != true {
            "unknown"
        } else if latest.values().any(|s| s == "CHANGES_REQUESTED") {
            "observed_changes_requested"
        } else if latest.values().any(|s| s == "APPROVED") {
            "observed_approvals"
        } else {
            "none_observed"
        };
        Ok(
            json!({"state":state,"headSha":head,"staleReviews":stale,"coverage":pages.coverage,"requiredReviewPolicy":"unknown"}),
        )
    }
}
fn next_page(
    link: &str,
    origin: &reqwest::Url,
    repo: &str,
    repo_id: u64,
    endpoint: &str,
    params: &[(&str, String)],
    current: usize,
) -> Result<Option<usize>> {
    ensure!(
        link.len() <= 8192,
        "GitHub pagination receipt exceeds capacity"
    );
    let mut next = None;
    for relation in link.split(',') {
        let Some((url, attributes)) = relation.trim().split_once('>') else {
            bail!("Invalid GitHub pagination receipt")
        };
        if !attributes.contains("rel=\"next\"") {
            continue;
        }
        ensure!(next.is_none(), "Duplicate GitHub next-page receipts");
        let parsed = reqwest::Url::parse(
            url.strip_prefix('<')
                .context("Invalid GitHub pagination URL")?,
        )?;
        ensure!(
            parsed.scheme() == origin.scheme()
                && parsed.host_str() == origin.host_str()
                && parsed.port_or_known_default() == origin.port_or_known_default()
                && parsed.username().is_empty()
                && parsed.password().is_none()
                && parsed.fragment().is_none(),
            "GitHub pagination origin refused"
        );
        ensure!(
            parsed.path() == format!("/repos/{repo}/{endpoint}")
                || parsed.path() == format!("/repositories/{repo_id}/{endpoint}"),
            "GitHub pagination repository/endpoint refused"
        );
        let expected: BTreeMap<_, _> = params.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let mut keys = BTreeSet::new();
        let mut page = None;
        for (key, value) in parsed.query_pairs() {
            ensure!(
                keys.insert(key.to_string()),
                "Duplicate GitHub pagination query"
            );
            if key == "page" {
                let n = value
                    .parse::<usize>()
                    .context("Invalid GitHub page number")?;
                ensure!(n == current + 1, "GitHub pagination cycle or skipped page");
                page = Some(n);
            } else {
                ensure!(
                    expected
                        .get(key.as_ref())
                        .is_some_and(|expected| *expected == value.as_ref()),
                    "GitHub pagination query changed"
                );
            }
        }
        next = Some(page.context("GitHub next page number missing")?);
    }
    Ok(next)
}
fn json_admission(bytes: &[u8], deadline: &Deadline) -> Result<()> {
    let (mut quoted, mut escaped, mut depth, mut nodes) = (false, false, 0usize, 0usize);
    for (i, b) in bytes.iter().enumerate() {
        if i.is_multiple_of(4096) {
            deadline.check()?;
        }
        if quoted {
            if escaped {
                escaped = false;
            } else if *b == b'\\' {
                escaped = true;
            } else if *b == b'"' {
                quoted = false;
            }
        } else {
            match b {
                b'"' => quoted = true,
                b'{' | b'[' => {
                    depth += 1;
                    nodes += 1;
                }
                b'}' | b']' => {
                    depth = depth.saturating_sub(1);
                }
                b',' | b':' => nodes += 1,
                _ => {}
            }
            ensure!(
                depth <= 64 && nodes <= 200_000,
                "GitHub JSON structural budget exceeded"
            );
        }
    }
    deadline.check()
}

#[derive(Default)]
struct PatchHunk {
    old_start: usize,
    old_count: usize,
    new_start: usize,
    new_count: usize,
    old_lines: Vec<String>,
    new_lines: Vec<String>,
    old_changed: Vec<usize>,
    new_changed: Vec<usize>,
    new_deletion_points: Vec<usize>,
    additions: usize,
    deletions: usize,
}
fn patch_range(value: &str, prefix: char) -> Result<(usize, usize)> {
    let raw = value
        .strip_prefix(prefix)
        .context("Malformed GitHub patch range")?;
    let (start, count) = raw.split_once(',').unwrap_or((raw, "1"));
    let start = start.parse::<usize>().context("Invalid patch start")?;
    let count = count.parse::<usize>().context("Invalid patch count")?;
    ensure!(
        start <= 10_000_000 && count <= 100_000 && (count == 0 || start > 0),
        "Patch line range exceeds capacity"
    );
    Ok((start, count))
}
fn patch_hunks(patch: &str, deadline: &Deadline) -> Result<Vec<PatchHunk>> {
    ensure!(patch.len() <= 4_000_000, "GitHub patch capacity exceeded");
    let mut hunks: Vec<PatchHunk> = vec![];
    for (line, row) in patch.split_terminator('\n').enumerate() {
        if line.is_multiple_of(64) {
            deadline.check()?;
        }
        if row.starts_with("@@ ") {
            let fields: Vec<_> = row.split_whitespace().take(4).collect();
            ensure!(
                fields.len() == 4 && fields[0] == "@@" && fields[3] == "@@",
                "Malformed GitHub hunk header"
            );
            let (old_start, old_count) = patch_range(fields[1], '-')?;
            let (new_start, new_count) = patch_range(fields[2], '+')?;
            ensure!(hunks.len() < 1000, "Patch hunk count exceeded");
            hunks.push(PatchHunk {
                old_start,
                old_count,
                new_start,
                new_count,
                ..Default::default()
            });
            continue;
        }
        if row == "\\ No newline at end of file" {
            continue;
        }
        let hunk = hunks
            .last_mut()
            .context("Patch data precedes a hunk header")?;
        match row.as_bytes().first() {
            Some(b' ') => {
                hunk.old_lines.push(row[1..].into());
                hunk.new_lines.push(row[1..].into());
            }
            Some(b'-') => {
                hunk.old_changed.push(hunk.old_lines.len());
                hunk.new_deletion_points.push(hunk.new_lines.len());
                hunk.old_lines.push(row[1..].into());
                hunk.deletions += 1;
            }
            Some(b'+') => {
                hunk.new_changed.push(hunk.new_lines.len());
                hunk.new_lines.push(row[1..].into());
                hunk.additions += 1;
            }
            _ => bail!("Malformed GitHub patch line"),
        }
        ensure!(
            hunk.old_lines.len() <= hunk.old_count && hunk.new_lines.len() <= hunk.new_count,
            "Patch lines exceed declared hunk ranges"
        );
    }
    ensure!(
        !hunks.is_empty()
            && hunks
                .iter()
                .all(|h| h.old_lines.len() == h.old_count && h.new_lines.len() == h.new_count),
        "GitHub patch is incomplete"
    );
    for adjacent in hunks.windows(2) {
        ensure!(
            adjacent[0].old_start.saturating_add(adjacent[0].old_count) <= adjacent[1].old_start
                && adjacent[0].new_start.saturating_add(adjacent[0].new_count)
                    <= adjacent[1].new_start,
            "GitHub patch hunks overlap or are unsorted"
        );
    }
    Ok(hunks)
}
fn source_impact(
    engine: &mut Engine,
    files: &[Value],
    head: &str,
    deadline: &Deadline,
) -> Result<Value> {
    let (snapshot, graph) = if let Some(id) = engine.store.latest()? {
        (engine.store.snapshot(&id)?, engine.store.graph(&id)?)
    } else {
        let (s, g, _, _) = engine.index(deadline)?;
        (s, g)
    };
    Ok(source_impact_for(
        &engine.store,
        &snapshot,
        &graph,
        files,
        head,
        deadline,
        true,
    )?
    .value)
}

/// Minted only after captured bytes and the reported remote Git blob actually match.
/// Stored/remote JSON cannot deserialize this private preparation capability.
pub(crate) struct CheckedFileBlob {
    snapshot_id: String,
    path: String,
    content_hash: String,
    git_blob_sha: String,
    start_byte: usize,
    end_byte: usize,
}
impl CheckedFileBlob {
    pub(crate) fn evidence(&self) -> Value {
        json!({"snapshotId":self.snapshot_id,"path":self.path,"contentHash":self.content_hash,
            "gitBlobSha":self.git_blob_sha,"startByte":self.start_byte,"endByte":self.end_byte})
    }
}
pub(crate) struct WorkerImpact {
    value: Value,
    matched: Vec<CheckedFileBlob>,
}
impl WorkerImpact {
    pub(crate) fn into_parts(self) -> (Value, Vec<CheckedFileBlob>) {
        (self.value, self.matched)
    }
}
pub(crate) fn worker_source_impact(
    store: &crate::store::Store,
    snapshot: &crate::model::Snapshot,
    graph: &crate::model::Graph,
    files: &[Value],
    commit: &str,
    deadline: &Deadline,
) -> Result<WorkerImpact> {
    source_impact_for(store, snapshot, graph, files, commit, deadline, false)
}
fn source_impact_for(
    store: &crate::store::Store,
    snapshot: &crate::model::Snapshot,
    graph: &crate::model::Graph,
    files: &[Value],
    head: &str,
    deadline: &Deadline,
    publish_communities: bool,
) -> Result<WorkerImpact> {
    let mut matched = Vec::new();
    crate::graph_extensions::validate_scope(store, snapshot, graph, deadline)?;
    let scope = crate::capture::source_scope_policy(store, snapshot)?;
    let mut selected = BTreeSet::new();
    let mut path_ids = BTreeSet::new();
    let mut rows = vec![];
    let mut omissions = vec![];
    let mut source_bytes = 0usize;
    for remote in files {
        deadline.check()?;
        let path = safe_text(&remote["filename"], 4096)?;
        if crate::paths::normalized_syntax(&path).is_err()
            || scope.prospective_path_policy(&path).is_err()
        {
            omissions.push(json!({"path":path,"reason":"outside_host_source_scope_or_protected"}));
            continue;
        }
        let status = safe_text(&remote["status"], 32)?;
        let sha = oid(&remote["sha"])?;
        let local = snapshot.files.iter().find(|file| file.path == path);
        let mut row = json!({"path":path,"status":status,"reportedBlobSha":sha,"reportedHeadSha":head,"sourceMatch":"outside_local_snapshot","hunksVerified":false});
        for entity in graph
            .entities
            .iter()
            .filter(|entity| entity.kind != "file" && entity.path == path)
        {
            path_ids.insert(entity.id.clone());
        }
        if let Some(file) = local {
            source_bytes = source_bytes
                .checked_add(file.size)
                .context("PR local source byte overflow")?;
            ensure!(
                source_bytes <= 64_000_000,
                "PR selected source capacity exceeded"
            );
            let text = String::from_utf8(scope.read_source(&path)?)?;
            use sha1::{Digest, Sha1};
            let mut digest = Sha1::new();
            digest.update(format!("blob {}\0", text.len()));
            for chunk in text.as_bytes().chunks(65536) {
                deadline.check()?;
                digest.update(chunk);
            }
            let local_sha = hex::encode(digest.finalize());
            let binding = json!({"snapshotId":snapshot.id,"path":path,"contentHash":file.hash,"startByte":0,"endByte":file.size,"anchor":"exact_captured_source"});
            row["sourceBinding"] = binding;
            if !local_sha.eq_ignore_ascii_case(sha) {
                row["sourceMatch"] = json!("path_only_candidate");
                row["reason"] = json!("local_blob_differs_from_reported_PR_blob");
                rows.push(row);
                continue;
            }
            matched.push(CheckedFileBlob {
                snapshot_id: snapshot.id.clone(),
                path: path.clone(),
                content_hash: file.hash.clone(),
                git_blob_sha: local_sha,
                start_byte: 0,
                end_byte: file.size,
            });
            let removed = status == "removed";
            row["sourceMatch"] = json!(if removed {
                "local_blob_matches_reported_removed_file"
            } else {
                "local_blob_matches_reported_head"
            });
            let Some(patch) = remote["patch"].as_str() else {
                row["reason"] = json!("patch_unavailable_or_binary");
                rows.push(row);
                continue;
            };
            let hunks = match patch_hunks(patch, deadline) {
                Ok(hunks) => hunks,
                Err(_) => {
                    deadline.check()?;
                    row["reason"] = json!("patch_incomplete_or_malformed");
                    rows.push(row);
                    continue;
                }
            };
            let additions: usize = hunks.iter().map(|h| h.additions).sum();
            let deletions: usize = hunks.iter().map(|h| h.deletions).sum();
            if remote["additions"].as_u64() != Some(additions as u64)
                || remote["deletions"].as_u64() != Some(deletions as u64)
            {
                row["reason"] = json!("patch_totals_do_not_cover_reported_file_changes");
                rows.push(row);
                continue;
            }
            let lines: Vec<_> = text.split_terminator('\n').collect();
            let starts = crate::paths::line_starts(&text);
            let mut anchors = vec![];
            let mut valid = true;
            for hunk in hunks {
                deadline.check()?;
                let (start, count, expected) = if removed {
                    (hunk.old_start, hunk.old_count, &hunk.old_lines)
                } else {
                    (hunk.new_start, hunk.new_count, &hunk.new_lines)
                };
                let index = if count == 0 {
                    start
                } else {
                    start.saturating_sub(1)
                };
                if index > lines.len()
                    || index.saturating_add(count) > lines.len()
                    || lines[index..index + count]
                        .iter()
                        .zip(expected)
                        .any(|(actual, expected)| *actual != expected)
                {
                    valid = false;
                    break;
                }
                let begin = starts.get(index).copied().unwrap_or(text.len());
                let end = starts.get(index + count).copied().unwrap_or(text.len());
                crate::paths::utf8_range(&text, begin, end)?;
                anchors.push(json!({"oldStartLine":hunk.old_start,"oldLineCount":hunk.old_count,"newStartLine":hunk.new_start,"newLineCount":hunk.new_count,"anchor":{"snapshotId":snapshot.id,"path":path,"contentHash":file.hash,"startByte":begin,"endByte":end,"startLine":if count==0{index+1}else{start},"endLine":if count==0{index+1}else{index+count},"precision":"exact_local_lines_matching_reported_blob_and_patch"}}));
                let mut changed_ranges = Vec::new();
                let changed = if removed {
                    &hunk.old_changed
                } else {
                    &hunk.new_changed
                };
                for offset in changed {
                    let line = index + offset;
                    changed_ranges.push(json!({"startByte":starts.get(line).copied().unwrap_or(text.len()),"endByte":starts.get(line+1).copied().unwrap_or(text.len()),"kind":"changed_line"}));
                }
                if !removed && hunk.new_changed.is_empty() {
                    for offset in &hunk.new_deletion_points {
                        let point = starts.get(index + offset).copied().unwrap_or(text.len());
                        changed_ranges.push(json!({"startByte":point,"endByte":point,"kind":"deleted_line_boundary_candidate"}));
                    }
                }
                anchors.last_mut().unwrap()["changedRanges"] = json!(changed_ranges);
            }
            if valid {
                for range in anchors
                    .iter()
                    .flat_map(|anchor| anchor["changedRanges"].as_array().into_iter().flatten())
                {
                    let begin = range["startByte"].as_u64().unwrap() as usize;
                    let end = range["endByte"].as_u64().unwrap() as usize;
                    for entity in graph
                        .entities
                        .iter()
                        .filter(|e| e.kind != "file" && e.path == path)
                    {
                        if entity.start_byte < end && begin < entity.end_byte
                            || begin == end && entity.start_byte <= begin && begin < entity.end_byte
                        {
                            selected.insert(entity.id.clone());
                        }
                    }
                }
                row["hunksVerified"] = json!(true);
                row["hunks"] = json!(anchors);
            } else {
                row["reason"] = json!("patch_text_does_not_match_local_blob_lines");
            }
        }
        rows.push(row);
    }
    let mut incoming = BTreeSet::new();
    let mut observations = vec![];
    for edge in &graph.observations {
        deadline.check()?;
        if edge
            .target
            .as_ref()
            .is_some_and(|target| selected.contains(target))
            && !selected.contains(&edge.source)
            && matches!(
                edge.relation.as_str(),
                "calls" | "references" | "imports" | "implements" | "extends" | "inherits"
            )
        {
            incoming.insert(edge.source.clone());
            observations.push(serde_json::to_value(edge)?);
        }
    }
    let entities = |ids: &BTreeSet<String>| -> Vec<Value> {
        graph
            .entities
            .iter()
            .filter(|e| ids.contains(&e.id))
            .map(|e| {
                let mut value = serde_json::to_value(e).expect("Serializable entity");
                value["contentHash"] = json!(
                    snapshot
                        .files
                        .iter()
                        .find(|f| f.path == e.path)
                        .map(|f| &f.hash)
                );
                value
            })
            .collect()
    };
    let evidence_ids: BTreeSet<_> = observations
        .iter()
        .flat_map(|e| e["evidenceIds"].as_array().into_iter().flatten())
        .filter_map(Value::as_str)
        .collect();
    let evidence: Vec<_> = graph
        .evidence
        .iter()
        .filter(|e| evidence_ids.contains(e.id.as_str()))
        .collect();
    let mut value = json!({"snapshotId":snapshot.id,"graphHash":crate::model::identity(graph),
        "reportedHeadSha":head,"sourceSelection":if publish_communities {"existing_immutable_index_or_initial_capture"}
            else {"host_selected_existing_immutable_snapshot"},
        "files":rows,"omissions":omissions,"changedSymbols":entities(&selected),"pathCandidates":entities(&path_ids),
        "incomingCandidates":entities(&incoming),"observations":observations,"evidence":evidence,
        "certainty":"source_bound_structural_candidate_not_compiler_or_merge_proof",
        "localTreeCommitEquality":"not_attested_file_blob_comparisons_only","complete":false});
    if publish_communities {
        let report = crate::graph_extensions::analyze(
            store,
            snapshot,
            graph,
            &Default::default(),
            deadline,
        )?;
        let communities:Vec<_>=report["communities"].as_array().context("PR impact communities missing")?.iter()
            .filter(|community|community["members"].as_array().into_iter().flatten().filter_map(Value::as_str)
                .any(|id|selected.contains(id)||path_ids.contains(id)))
            .map(|c|json!({"id":c["id"],"label":c["label"],"memberCount":c["members"].as_array().map_or(0,Vec::len)})).collect();
        value["communities"] = json!(communities);
        value["communityReportArtifactId"] = report["artifactId"].clone();
    } else {
        value["communityAnalysis"] = json!("not_run_by_pure_worker");
    }
    Ok(WorkerImpact { value, matched })
}
