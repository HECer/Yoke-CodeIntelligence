//! Fixed-origin, no-redirect/no-proxy/no-retry authorization transport.
use super::*;
use std::{future::Future, task::Poll, time::Duration};

/// Poll the owned request/body future together with the inherited host deadline.
/// Dropping it on cancellation stops this request without a detached worker/retry.
async fn checked<T>(future: impl Future<Output = reqwest::Result<T>>, d: &Deadline) -> Result<T> {
    let mut future = std::pin::pin!(future);
    let mut ticks = tokio::time::interval(Duration::from_millis(10));
    std::future::poll_fn(|cx| {
        if let Err(error) = d.check() {
            return Poll::Ready(Err(error));
        }
        if let Poll::Ready(result) = future.as_mut().poll(cx) {
            if let Err(error) = d.check() {
                return Poll::Ready(Err(error));
            }
            return Poll::Ready(result.map_err(|_| anyhow::anyhow!("github_app_transport_failed")));
        }
        while ticks.poll_tick(cx).is_ready() {
            if let Err(error) = d.check() {
                return Poll::Ready(Err(error));
            }
        }
        Poll::Pending
    })
    .await
}
pub(super) struct Run<'a> {
    pub client: &'a Client,
    pub requests: usize,
    pub bytes: usize,
    pub deadline: &'a Deadline,
}
#[derive(Clone, Copy)]
pub(super) struct Limits {
    pub max_requests: usize,
    pub max_response_bytes: usize,
    pub max_total_bytes: usize,
}
pub(super) struct Response {
    pub value: PrivateJson,
    pub links: Vec<String>,
}
struct Request<'a> {
    method: reqwest::Method,
    path: &'a str,
    token: &'a str,
    body: Option<Vec<u8>>,
    status: u16,
    allow_links: bool,
}
impl Run<'_> {
    pub fn request(
        &mut self,
        path: &str,
        token: &str,
        body: Option<Vec<u8>>,
        status: u16,
    ) -> Result<PrivateJson> {
        let limits = Limits {
            max_requests: self.client.options.max_requests,
            max_response_bytes: self.client.options.max_response_bytes,
            max_total_bytes: self.client.options.max_total_bytes,
        };
        Ok(self
            .request_with(
                Request {
                    method: if body.is_some() {
                        reqwest::Method::POST
                    } else {
                        reqwest::Method::GET
                    },
                    path,
                    token,
                    body,
                    status,
                    allow_links: false,
                },
                limits,
            )?
            .value)
    }
    pub fn scoped_get(
        &mut self,
        path: &str,
        token: &str,
        limits: Limits,
        allow_links: bool,
    ) -> Result<Response> {
        self.request_with(
            Request {
                method: reqwest::Method::GET,
                path,
                token,
                body: None,
                status: 200,
                allow_links,
            },
            limits,
        )
    }
    pub fn scoped_write(
        &mut self,
        path: &str,
        token: &str,
        body: Vec<u8>,
        status: u16,
        update: bool,
        limits: Limits,
    ) -> Result<Response> {
        self.request_with(
            Request {
                method: if update {
                    reqwest::Method::PATCH
                } else {
                    reqwest::Method::POST
                },
                path,
                token,
                body: Some(body),
                status,
                allow_links: false,
            },
            limits,
        )
    }
    fn request_with(&mut self, request: Request<'_>, limits: Limits) -> Result<Response> {
        self.deadline.check()?;
        ensure!(
            tokio::runtime::Handle::try_current().is_err(),
            "github_app_requires_synchronous_host"
        );
        let client = self.client;
        client
            .runtime
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("github_app_transport_failed"))?
            .block_on(self.request_async(request, limits))
    }
    async fn request_async(&mut self, request: Request<'_>, limits: Limits) -> Result<Response> {
        let Request {
            method,
            path,
            token,
            body,
            status,
            allow_links,
        } = request;
        self.deadline.check()?;
        ensure!(self.requests < limits.max_requests, CAPACITY);
        let url = self
            .client
            .origin
            .join(path)
            .map_err(|_| anyhow::anyhow!(INVALID))?;
        ensure!(url.origin() == self.client.origin.origin(), INVALID);
        let mut header =
            reqwest::header::HeaderValue::from_str(&Zeroizing::new(format!("Bearer {token}")))
                .map_err(|_| anyhow::anyhow!(BINDING))?;
        header.set_sensitive(true);
        let mut request = self
            .client
            .http
            .request(method, url.clone())
            .timeout(self.deadline.remaining())
            .header(reqwest::header::AUTHORIZATION, header)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", VERSION)
            .header("User-Agent", "Yoke-CodeIntelligence/0.5 app-auth");
        if let Some(body) = body {
            request = request
                .header("Content-Type", "application/json")
                .body(body);
        }
        self.requests += 1;
        let mut response = checked(request.send(), self.deadline).await?;
        self.deadline.check()?;
        ensure!(
            response.url() == &url && response.status().as_u16() == status,
            "github_app_http_refused"
        );
        ensure!(
            (allow_links || !response.headers().contains_key(reqwest::header::LINK))
                && !response
                    .headers()
                    .contains_key(reqwest::header::CONTENT_ENCODING),
            "github_app_http_refused"
        );
        let header_bytes = response.headers().iter().try_fold(0usize, |n, (k, v)| {
            n.checked_add(k.as_str().len() + v.as_bytes().len())
                .ok_or_else(|| anyhow::anyhow!(CAPACITY))
        })?;
        ensure!(header_bytes <= 16_384, CAPACITY);
        self.bytes = self
            .bytes
            .checked_add(header_bytes)
            .ok_or_else(|| anyhow::anyhow!(CAPACITY))?;
        ensure!(self.bytes <= limits.max_total_bytes, CAPACITY);
        let mut links = Vec::new();
        for header in response.headers().get_all(reqwest::header::LINK) {
            ensure!(allow_links && links.is_empty(), "github_app_http_refused");
            links.push(
                header
                    .to_str()
                    .map_err(|_| anyhow::anyhow!("github_app_http_refused"))?
                    .to_owned(),
            );
        }
        let cap = limits
            .max_response_bytes
            .min(limits.max_total_bytes - self.bytes);
        ensure!(
            response.content_length().is_none_or(|n| n <= cap as u64),
            CAPACITY
        );
        let mut raw = Zeroizing::new(Vec::new());
        loop {
            self.deadline.check()?;
            let Some(chunk) = checked(response.chunk(), self.deadline).await? else {
                break;
            };
            ensure!(raw.len().saturating_add(chunk.len()) <= cap, CAPACITY);
            raw.extend_from_slice(&chunk);
        }
        self.bytes += raw.len();
        self.deadline.check()?;
        let value = PrivateJson(
            crate::json::parse(&raw, cap)
                .map_err(|_| anyhow::anyhow!("github_app_response_invalid"))?,
        );
        self.deadline.check()?;
        Ok(Response { value, links })
    }
}
