//! Explicit host-only text disclosure to a token-count endpoint, without generation.
use crate::{capture::Deadline, model};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use serde_json::json;
use std::{
    io::Read,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};
use zeroize::Zeroizing;

const PATH: &str = "/v1/messages/count_tokens";
const MODEL: &str = "claude-sonnet-4-6";
const MAX_RESPONSE: usize = 16_384;
type Credentials = dyn Fn(&str) -> Result<Zeroizing<String>> + Send + Sync;

/// Typed trusted-host configuration. It cannot be deserialized from source or requests.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Spec {
    pub endpoint: String,
    pub credential_name: String,
    pub allow_loopback_http: bool,
    pub additional_root_certificates: Vec<Vec<u8>>,
}
#[cfg(test)]
#[path = "http_tests.rs"]
mod tests;
impl Spec {
    fn url(&self) -> Result<url::Url> {
        ensure!(
            !self.credential_name.is_empty()
                && self.credential_name.len() <= 80
                && self.credential_name.as_bytes()[0].is_ascii_alphanumeric()
                && self
                    .credential_name
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"_.-".contains(&c)),
            "tool_usage_credential_reference_invalid"
        );
        ensure!(
            self.endpoint.len() <= 2048 && self.additional_root_certificates.len() <= 8,
            "tool_usage_host_capacity"
        );
        for root in &self.additional_root_certificates {
            ensure!(
                !root.is_empty() && root.len() <= 128_000,
                "tool_usage_certificate_capacity"
            );
            reqwest::Certificate::from_der(root)
                .map_err(|_| anyhow::anyhow!("tool_usage_certificate_invalid"))?;
        }
        let url = url::Url::parse(&self.endpoint)
            .map_err(|_| anyhow::anyhow!("tool_usage_endpoint_invalid"))?;
        ensure!(
            url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none()
                && url.path() == PATH
                && url.host().is_some()
                && url.as_str() == self.endpoint,
            "tool_usage_endpoint_invalid"
        );
        let loopback = match url.host() {
            Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
            Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
            _ => false,
        };
        ensure!(
            url.scheme() == "https"
                || (url.scheme() == "http" && self.allow_loopback_http && loopback),
            "tool_usage_tls_required"
        );
        Ok(url)
    }
}
struct Inner {
    spec: Spec,
    fingerprint: String,
    credentials: Arc<Credentials>,
    allowed: AtomicBool,
    attempts: AtomicU64,
}
/// Nonserializable, revocable network AND tool-text-disclosure grant.
#[derive(Clone)]
pub struct Host(Arc<Inner>);
impl Host {
    pub fn new(
        spec: Spec,
        allow_tool_text_disclosure: bool,
        credentials: Arc<Credentials>,
    ) -> Result<Self> {
        ensure!(
            allow_tool_text_disclosure,
            "tool_usage_independent_text_disclosure_grant_required"
        );
        spec.url()?;
        let fingerprint =
            model::identity(&(super::VERSION, "host_token_count_http_v1", MODEL, &spec));
        Ok(Self(Arc::new(Inner {
            spec,
            fingerprint,
            credentials,
            allowed: AtomicBool::new(true),
            attempts: AtomicU64::new(0),
        })))
    }
    pub fn anthropic(credential_name: String, allow_tool_text_disclosure: bool) -> Result<Self> {
        Self::new(
            Spec {
                endpoint: format!("https://api.anthropic.com{PATH}"),
                credential_name,
                allow_loopback_http: false,
                additional_root_certificates: vec![],
            },
            allow_tool_text_disclosure,
            Arc::new(crate::host_credentials::read_model_token),
        )
    }
    pub fn revoke(&self) {
        self.0.allowed.store(false, Ordering::Release);
    }
    pub fn fingerprint(&self) -> &str {
        &self.0.fingerprint
    }
    pub fn attempts(&self) -> u64 {
        self.0.attempts.load(Ordering::Acquire)
    }
    pub(crate) fn check(&self, deadline: &Deadline) -> Result<()> {
        deadline.check()?;
        ensure!(
            self.0.allowed.load(Ordering::Acquire),
            "tool_usage_text_disclosure_grant_revoked"
        );
        Ok(())
    }
    #[cfg(test)]
    pub(crate) fn count(&self, text: &str, deadline: &Deadline) -> Result<u64> {
        self.count_checked(text, deadline, &|| Ok(()))
    }
    pub(crate) fn count_checked(
        &self,
        text: &str,
        deadline: &Deadline,
        current: &dyn Fn() -> Result<()>,
    ) -> Result<u64> {
        self.check(deadline)?;
        current()?;
        ensure!(
            text.len() <= super::MAX_TEXT_BYTES,
            "tool_usage_text_capacity"
        );
        let url = self.0.spec.url()?;
        let body = Zeroizing::new(crate::json::encode_bounded(
            &json!({"model":MODEL,"messages":[{"role":"user","content":text}]}),
            super::MAX_TEXT_BYTES * 2 + 1024,
            deadline,
        )?);
        if rustls::crypto::CryptoProvider::get_default().is_none() {
            let _ = rustls::crypto::ring::default_provider().install_default();
        }
        let mut client = reqwest::blocking::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .no_proxy()
            .https_only(url.scheme() == "https")
            .http1_only()
            .http1_max_headers(32)
            .timeout(deadline.remaining())
            .connect_timeout(deadline.remaining());
        for certificate in &self.0.spec.additional_root_certificates {
            client = client.add_root_certificate(
                reqwest::Certificate::from_der(certificate)
                    .map_err(|_| anyhow::anyhow!("tool_usage_certificate_invalid"))?,
            );
        }
        let client = client
            .build()
            .map_err(|_| anyhow::anyhow!("tool_usage_http_client_unavailable"))?;
        self.check(deadline)?;
        current()?;
        let secret = (self.0.credentials)(&self.0.spec.credential_name)
            .map_err(|_| anyhow::anyhow!("tool_usage_host_credential_unavailable"))?;
        ensure!(
            !secret.is_empty()
                && secret.len() <= 2560
                && secret.bytes().all(|c| (33..=126).contains(&c)),
            "tool_usage_host_credential_invalid"
        );
        let mut header = reqwest::header::HeaderValue::from_str(&secret)
            .map_err(|_| anyhow::anyhow!("tool_usage_host_credential_invalid"))?;
        header.set_sensitive(true);
        self.check(deadline)?;
        current()?;
        self.0
            .attempts
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| n.checked_add(1))
            .map_err(|_| anyhow::anyhow!("tool_usage_counter_overflow"))?;
        let mut response = client
            .post(url)
            .timeout(deadline.remaining())
            .header("x-api-key", header)
            .header("anthropic-version", "2023-06-01")
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .header(reqwest::header::ACCEPT, "application/json")
            .header(reqwest::header::ACCEPT_ENCODING, "identity")
            .body(body.to_vec())
            .send()
            .map_err(|_| {
                anyhow::anyhow!("tool_usage_http_attempt_failed_no_retry_remote_outcome_unknown")
            })?;
        self.check(deadline)?;
        current()?;
        let header_bytes = response
            .headers()
            .iter()
            .try_fold(0usize, |n, (key, value)| {
                n.checked_add(key.as_str().len() + value.len() + 4)
                    .context("tool_usage_http_header_capacity")
            })?;
        ensure!(
            header_bytes <= 8192 && response.status().as_u16() == 200,
            "tool_usage_http_response_refused_no_retry"
        );
        let media = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        ensure!(
            media.split(';').next().unwrap_or("").trim() == "application/json",
            "tool_usage_http_media_invalid"
        );
        ensure!(
            response
                .headers()
                .get(reqwest::header::CONTENT_ENCODING)
                .is_none_or(|v| v == "identity"),
            "tool_usage_http_encoding_invalid"
        );
        ensure!(
            response
                .content_length()
                .is_none_or(|n| n <= MAX_RESPONSE as u64),
            "tool_usage_http_response_capacity"
        );
        let mut bytes = Zeroizing::new(Vec::new());
        loop {
            self.check(deadline)?;
            let mut block = [0u8; 2048];
            let count = response
                .read(&mut block)
                .map_err(|_| anyhow::anyhow!("tool_usage_http_read_failed"))?;
            if count == 0 {
                break;
            }
            ensure!(
                bytes
                    .len()
                    .checked_add(count)
                    .is_some_and(|n| n <= MAX_RESPONSE),
                "tool_usage_http_response_capacity"
            );
            bytes.extend_from_slice(&block[..count]);
        }
        self.check(deadline)?;
        let value = crate::json::parse(&bytes, MAX_RESPONSE)
            .map_err(|_| anyhow::anyhow!("tool_usage_http_json_invalid"))?;
        let fields = value.as_object().context("tool_usage_http_json_invalid")?;
        ensure!(
            fields.len() == 1 && fields.contains_key("input_tokens"),
            "tool_usage_http_count_schema_invalid"
        );
        let count = value["input_tokens"]
            .as_u64()
            .filter(|n| *n <= 100_000_000)
            .context("tool_usage_http_count_invalid")?;
        self.check(deadline)?;
        current()?;
        Ok(count)
    }
}
