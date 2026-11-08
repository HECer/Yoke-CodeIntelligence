//! Owned, bounded Query API v2 transport. No redirect, retry, proxy or DNS fallback.
use super::Target;
use crate::capture::Deadline;
use anyhow::{Result, ensure};
use base64::Engine as _;
use serde_json::Value;
use std::{future::Future, task::Poll, time::Duration};
use zeroize::Zeroizing;
#[cfg(test)]
#[path = "neo4j_publish_transport_tests.rs"]
mod tests;

/// Host-only capabilities; intentionally not deserializable from a plan/config.
#[derive(Default)]
pub struct Permissions {
    pub allow_network: bool,
    pub allow_credential_lookup: bool,
    pub allow_loopback_http: bool,
    pub allow_write: bool,
    pub allow_schema: bool,
    pub allow_journal: bool,
    pub allow_approval: bool,
    pub allow_recovery: bool,
}
pub(super) struct Session {
    client: reqwest::Client,
    runtime: tokio::runtime::Runtime,
    url: url::Url,
    authorization: reqwest::header::HeaderValue,
}
impl Session {
    pub(super) fn open(target: &Target, permissions: &Permissions, d: &Deadline) -> Result<Self> {
        ensure!(
            permissions.allow_network,
            "neo4j_publish_network_permission_required"
        );
        ensure!(
            permissions.allow_credential_lookup,
            "neo4j_publish_credential_permission_required"
        );
        target.validate()?;
        ensure!(
            target.url()?.scheme() != "http" || permissions.allow_loopback_http,
            "neo4j_publish_loopback_http_permission_required"
        );
        d.check()?;
        ensure!(
            tokio::runtime::Handle::try_current().is_err(),
            "neo4j_publish_synchronous_host_required"
        );
        let secret = crate::host_credentials::read_neo4j_password(&target.credential_binding()?, d)
            .map_err(|_| anyhow::anyhow!("neo4j_publish_credential_unavailable"))?;
        Self::with_secret(target, &secret, d)
    }
    fn with_secret(target: &Target, password: &str, d: &Deadline) -> Result<Self> {
        d.check()?;
        let url = target.query_url()?;
        let plain = Zeroizing::new(format!("{}:{password}", target.user));
        let encoded = Zeroizing::new(base64::prelude::BASE64_STANDARD.encode(plain.as_bytes()));
        let header = Zeroizing::new(format!("Basic {}", encoded.as_str()));
        let mut authorization = reqwest::header::HeaderValue::from_str(&header)
            .map_err(|_| anyhow::anyhow!("neo4j_publish_credential_invalid"))?;
        authorization.set_sensitive(true);
        if rustls::crypto::CryptoProvider::get_default().is_none() {
            let _ = rustls::crypto::ring::default_provider().install_default();
        }
        let mut builder = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .no_proxy()
            .https_only(url.scheme() == "https")
            .http1_only()
            .http1_max_headers(64)
            .pool_max_idle_per_host(0)
            .timeout(d.remaining())
            .connect_timeout(d.remaining())
            .resolve_to_addrs(url.host_str().unwrap(), &[target.address]);
        if url.scheme() == "https" {
            let mut roots = Vec::new();
            if target.use_system_roots {
                let loaded = rustls_native_certs::load_native_certs();
                ensure!(
                    loaded.errors.is_empty() && loaded.certs.len() <= 1024,
                    "neo4j_publish_tls_roots_unavailable"
                );
                for der in loaded.certs {
                    d.check()?;
                    ensure!(der.len() <= 16384, "neo4j_publish_tls_roots_invalid");
                    roots.push(
                        reqwest::Certificate::from_der(&der)
                            .map_err(|_| anyhow::anyhow!("neo4j_publish_tls_roots_invalid"))?,
                    );
                }
            }
            for encoded in &target.root_certificates_der_base64 {
                d.check()?;
                let der = base64::prelude::BASE64_STANDARD
                    .decode(encoded)
                    .map_err(|_| anyhow::anyhow!("neo4j_publish_tls_roots_invalid"))?;
                roots.push(
                    reqwest::Certificate::from_der(&der)
                        .map_err(|_| anyhow::anyhow!("neo4j_publish_tls_roots_invalid"))?,
                );
            }
            ensure!(!roots.is_empty(), "neo4j_publish_tls_roots_unavailable");
            builder = builder.tls_certs_only(roots);
        }
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| anyhow::anyhow!("neo4j_publish_transport_unavailable"))?;
        let client = builder
            .build()
            .map_err(|_| anyhow::anyhow!("neo4j_publish_transport_unavailable"))?;
        d.check()?;
        Ok(Self {
            client,
            runtime,
            url,
            authorization,
        })
    }
    pub(super) fn query(&self, request: &Value, d: &Deadline) -> Result<Value> {
        let bytes = crate::json::encode_bounded(request, 32_000_000, d)?;
        self.runtime.block_on(async {
            let mut response = checked(
                self.client
                    .post(self.url.clone())
                    .header(reqwest::header::AUTHORIZATION, self.authorization.clone())
                    .header(reqwest::header::CONTENT_TYPE, "application/json")
                    .header(reqwest::header::ACCEPT, "application/json")
                    .header(reqwest::header::ACCEPT_ENCODING, "identity")
                    .header(
                        reqwest::header::USER_AGENT,
                        "Yoke-CodeIntelligence/0.5 neo4j-publication",
                    )
                    .body(bytes)
                    .send(),
                d,
            )
            .await?;
            ensure!(
                response.url() == &self.url && response.status() == 202,
                "neo4j_publish_http_refused"
            );
            let headers = response.headers();
            ensure!(
                !headers.contains_key(reqwest::header::CONTENT_ENCODING)
                    && headers
                        .iter()
                        .map(|(k, v)| k.as_str().len() + v.as_bytes().len())
                        .sum::<usize>()
                        <= 16384,
                "neo4j_publish_response_invalid"
            );
            let mut types = headers.get_all(reqwest::header::CONTENT_TYPE).iter();
            let content_type = types.next().and_then(|v| v.to_str().ok()).unwrap_or("");
            ensure!(
                types.next().is_none()
                    && content_type
                        .split(';')
                        .next()
                        .is_some_and(|v| v.trim().eq_ignore_ascii_case("application/json")),
                "neo4j_publish_response_invalid"
            );
            ensure!(
                response.content_length().is_none_or(|n| n <= 32_000_000),
                "neo4j_publish_response_capacity_exceeded"
            );
            let mut body = Vec::new();
            while let Some(chunk) = checked(response.chunk(), d).await? {
                ensure!(
                    body.len()
                        .checked_add(chunk.len())
                        .is_some_and(|n| n <= 32_000_000),
                    "neo4j_publish_response_capacity_exceeded"
                );
                body.extend_from_slice(&chunk);
            }
            let value = crate::json::parse(&body, 32_000_000)
                .map_err(|_| anyhow::anyhow!("neo4j_publish_response_invalid"))?;
            response_data(&value)?;
            d.check()?;
            Ok(value)
        })
    }
}
pub(super) fn response_data(value: &Value) -> Result<(&[Value], &[Value])> {
    ensure!(value.is_object(), "neo4j_publish_response_invalid");
    if let Some(errors) = value.get("errors") {
        ensure!(
            errors.as_array().is_some_and(Vec::is_empty),
            "neo4j_publish_query_refused"
        );
    }
    let data = value
        .get("data")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow::anyhow!("neo4j_publish_response_invalid"))?;
    ensure!(data.len() == 2, "neo4j_publish_response_invalid");
    let fields = data
        .get("fields")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("neo4j_publish_response_invalid"))?;
    let rows = data
        .get("values")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("neo4j_publish_response_invalid"))?;
    let mut names = std::collections::BTreeSet::new();
    ensure!(
        fields.len() <= 32
            && fields.iter().all(|f| f
                .as_str()
                .is_some_and(|s| !s.is_empty() && s.len() <= 256 && names.insert(s)))
            && rows.len() <= 50000
            && rows
                .iter()
                .all(|r| r.as_array().is_some_and(|r| r.len() == fields.len())),
        "neo4j_publish_response_invalid"
    );
    Ok((fields, rows))
}
async fn checked<T>(future: impl Future<Output = reqwest::Result<T>>, d: &Deadline) -> Result<T> {
    let mut future = std::pin::pin!(future);
    let mut ticks = tokio::time::interval(Duration::from_millis(10));
    std::future::poll_fn(|cx| {
        if let Err(e) = d.check() {
            return Poll::Ready(Err(e));
        }
        if let Poll::Ready(result) = future.as_mut().poll(cx) {
            if let Err(e) = d.check() {
                return Poll::Ready(Err(e));
            }
            return Poll::Ready(
                result.map_err(|_| anyhow::anyhow!("neo4j_publish_transport_failed")),
            );
        }
        while ticks.poll_tick(cx).is_ready() {
            if let Err(e) = d.check() {
                return Poll::Ready(Err(e));
            }
        }
        Poll::Pending
    })
    .await
}
