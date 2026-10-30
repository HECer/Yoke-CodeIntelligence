//! Explicit native Drive export. Imported bytes never supply credentials or grants.
use super::{ExporterClaim, Limits, ReceivedExport, Shortcut, acquisition};
use crate::capture::Deadline;
use anyhow::{Result, ensure};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    fs::File,
    future::Future,
    net::SocketAddr,
    path::{Path, PathBuf},
    task::Poll,
    time::Duration,
};
use zeroize::Zeroizing;

const HOST: &str = "www.googleapis.com";
const VERSION: &str = "native-drive-http-v1";
const MAX_TOKEN: usize = 2560;
const MAX_NATIVE_IMAGE: usize = 512_000_000;

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HttpSpec {
    pub shortcut_path: String,
    pub expected_shortcut_sha256: String,
    pub expected_file_id: String,
    pub address: SocketAddr,
    pub expected_exporter_sha256: String,
}
/// Only the host can construct these grants; they are never deserialized.
#[derive(Default)]
pub struct HttpPermissions {
    pub allow_network: bool,
    pub allow_private_credential: bool,
    pub trust_native_exporter: bool,
}
/// Private, non-serializable and non-Debug. No environment or credential discovery.
pub struct HostAccessToken(Zeroizing<String>);
impl HostAccessToken {
    pub fn from_host_input(bytes: &[u8]) -> Result<Self> {
        let bytes = if let Some(line) = bytes.strip_suffix(b"\n") {
            line.strip_suffix(b"\r").unwrap_or(line)
        } else {
            bytes
        };
        ensure!(
            (1..=MAX_TOKEN).contains(&bytes.len())
                && bytes
                    .iter()
                    .all(|b| b.is_ascii_alphanumeric() || b"-._~+/=".contains(b)),
            "google_workspace_private_token_invalid"
        );
        Ok(Self(Zeroizing::new(
            std::str::from_utf8(bytes)
                .map_err(|_| anyhow::anyhow!("google_workspace_private_token_invalid"))?
                .to_owned(),
        )))
    }
}

struct Prepared {
    shortcut: Shortcut,
    program: PathBuf,
    _program_guard: File,
    claim: ExporterClaim,
    url: url::Url,
}
fn prepare(
    source_root: &Path,
    bytes: &[u8],
    spec: &HttpSpec,
    limits: &Limits,
    deadline: &Deadline,
) -> Result<Prepared> {
    deadline.check()?;
    limits.validate()?;
    ensure!(
        ["SSL_CERT_FILE", "SSL_CERT_DIR", "SSLKEYLOGFILE"]
            .iter()
            .all(|name| std::env::var_os(name).is_none()),
        "google_workspace_http_ambient_tls_settings_refused"
    );
    let _root = acquisition::directory(source_root)?;
    let shortcut = Shortcut::parse(&spec.shortcut_path, bytes, deadline)?;
    shortcut
        .metadata()
        .check_selection(&spec.expected_shortcut_sha256, &spec.expected_file_id)?;
    ensure!(
        spec.address.port() == 443
            && !spec.address.ip().is_unspecified()
            && !spec.address.ip().is_multicast(),
        "google_workspace_http_address_invalid"
    );
    super::shortcut::digest(&spec.expected_exporter_sha256)?;
    let program = std::env::current_exe()
        .map_err(|_| anyhow::anyhow!("google_workspace_native_exporter_unavailable"))?
        .canonicalize()?;
    let (guard, hash) = acquisition::hash_program_bounded(&program, MAX_NATIVE_IMAGE, deadline)?;
    ensure!(
        hash == spec.expected_exporter_sha256,
        "google_workspace_native_exporter_changed"
    );
    let claim = ExporterClaim {
        executable_sha256: hash,
        version: VERSION.into(),
    };
    acquisition::archive_admission(&shortcut, &claim, limits, deadline)?;
    let mut url =
        url::Url::parse("https://www.googleapis.com/drive/v3/files/").expect("Owned endpoint");
    url.path_segments_mut()
        .expect("Owned base path")
        .pop_if_empty()
        .push(&shortcut.metadata().file_id)
        .push("export");
    url.query_pairs_mut()
        .append_pair("mimeType", shortcut.metadata().kind.export_mime());
    deadline.check()?;
    Ok(Prepared {
        shortcut,
        program,
        _program_guard: guard,
        claim,
        url,
    })
}
/// Pure local admission/preview: no token input, socket, Store, process or output file.
pub fn plan_http(
    source_root: &Path,
    bytes: &[u8],
    spec: &HttpSpec,
    limits: &Limits,
    deadline: &Deadline,
) -> Result<Value> {
    let p = prepare(source_root, bytes, spec, limits, deadline)?;
    Ok(
        json!({"schema":"native_google_workspace_http_plan_v1","metadata":p.shortcut.metadata(),"requestedMimeType":p.shortcut.metadata().kind.export_mime(),"requestUrl":p.url.as_str(),"address":spec.address,"method":"GET","requestBodyBytes":0,"resourceKeyPresent":p.shortcut.has_resource_key(),"resourceKeyTransport":"X-Goog-Drive-Resource-Keys","authorizationRequired":true,"credentialInput":"explicit_private_host_input","exporterClaim":p.claim,"requests":0,"credentialsRead":false,"outputCreated":false,"sourceExecuted":false,"modelCalls":0,"captureClaimsAreAuthority":false,"complete":false}),
    )
}
async fn checked<T>(
    future: impl Future<Output = reqwest::Result<T>>,
    deadline: &Deadline,
) -> Result<T> {
    let mut future = std::pin::pin!(future);
    let mut ticks = tokio::time::interval(Duration::from_millis(10));
    std::future::poll_fn(|cx| {
        if let Err(e) = deadline.check() {
            return Poll::Ready(Err(e));
        }
        if let Poll::Ready(result) = future.as_mut().poll(cx) {
            return Poll::Ready(
                result
                    .map_err(|_| anyhow::anyhow!("google_workspace_http_transport_failed"))
                    .and_then(|value| {
                        deadline.check()?;
                        Ok(value)
                    }),
            );
        }
        while ticks.poll_tick(cx).is_ready() {
            if let Err(e) = deadline.check() {
                return Poll::Ready(Err(e));
            }
        }
        Poll::Pending
    })
    .await
}
// Linear-time secret reflection check; neither positions nor private bytes escape.
fn reflected(body: &[u8], secret: &[u8], deadline: &Deadline) -> Result<bool> {
    if secret.is_empty() {
        return Ok(false);
    }
    let mut prefix = Zeroizing::new(vec![0usize; secret.len()]);
    let mut matched = 0;
    for i in 1..secret.len() {
        while matched > 0 && secret[i] != secret[matched] {
            matched = prefix[matched - 1];
        }
        if secret[i] == secret[matched] {
            matched += 1;
        }
        prefix[i] = matched;
    }
    matched = 0;
    for (i, b) in body.iter().enumerate() {
        if i % 4096 == 0 {
            deadline.check()?;
        }
        while matched > 0 && *b != secret[matched] {
            matched = prefix[matched - 1];
        }
        if *b == secret[matched] {
            matched += 1;
        }
        if matched == secret.len() {
            return Ok(true);
        }
    }
    deadline.check()?;
    Ok(false)
}
fn sensitive(value: &[u8]) -> Result<reqwest::header::HeaderValue> {
    let mut value = reqwest::header::HeaderValue::from_bytes(value)
        .map_err(|_| anyhow::anyhow!("google_workspace_private_header_invalid"))?;
    value.set_sensitive(true);
    Ok(value)
}
fn roots(deadline: &Deadline) -> Result<Vec<reqwest::Certificate>> {
    let loaded = rustls_native_certs::load_native_certs();
    ensure!(
        loaded.errors.is_empty() && !loaded.certs.is_empty() && loaded.certs.len() <= 1024,
        "google_workspace_http_tls_roots_unavailable"
    );
    let mut roots = Vec::new();
    for cert in loaded.certs {
        deadline.check()?;
        ensure!(
            cert.len() <= 16384,
            "google_workspace_http_tls_roots_invalid"
        );
        roots.push(
            reqwest::Certificate::from_der(&cert)
                .map_err(|_| anyhow::anyhow!("google_workspace_http_tls_roots_invalid"))?,
        );
    }
    Ok(roots)
}
fn receive(
    p: &Prepared,
    address: SocketAddr,
    token: &HostAccessToken,
    limits: &Limits,
    deadline: &Deadline,
    url: &url::Url,
) -> Result<ReceivedExport> {
    deadline.check()?;
    ensure!(
        tokio::runtime::Handle::try_current().is_err(),
        "google_workspace_http_synchronous_host_required"
    );
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
        .timeout(deadline.remaining())
        .connect_timeout(deadline.remaining())
        .resolve_to_addrs(url.host_str().expect("Owned host"), &[address]);
    if url.scheme() == "https" {
        builder = builder.tls_certs_only(roots(deadline)?);
    }
    let client = builder
        .build()
        .map_err(|_| anyhow::anyhow!("google_workspace_http_transport_failed"))?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| anyhow::anyhow!("google_workspace_http_transport_failed"))?;
    let result = runtime.block_on(async {
        let auth = Zeroizing::new(format!("Bearer {}", token.0.as_str()));
        let mut request = client
            .get(url.clone())
            .header(reqwest::header::AUTHORIZATION, sensitive(auth.as_bytes())?)
            .header("Accept-Encoding", "identity")
            .header("Accept", p.shortcut.metadata().kind.export_mime())
            .header(
                "User-Agent",
                concat!(
                    "Yoke-CodeIntelligence/",
                    env!("CARGO_PKG_VERSION"),
                    " native-google-export"
                ),
            );
        if let Some(key) = p.shortcut.resource_key() {
            let pair = Zeroizing::new(format!("{}/{key}", p.shortcut.metadata().file_id));
            request = request.header("X-Goog-Drive-Resource-Keys", sensitive(pair.as_bytes())?);
        }
        let mut response = checked(request.send(), deadline).await?;
        ensure!(
            response.url() == url && response.status() == 200,
            "google_workspace_http_status_refused"
        );
        let mut header_bytes = 0usize;
        for (k, v) in response.headers() {
            deadline.check()?;
            header_bytes = header_bytes
                .checked_add(k.as_str().len() + v.as_bytes().len())
                .ok_or_else(|| anyhow::anyhow!("google_workspace_http_header_capacity_exceeded"))?;
            ensure!(
                header_bytes <= 16384,
                "google_workspace_http_header_capacity_exceeded"
            );
        }
        let single = |name: reqwest::header::HeaderName| -> Result<Option<&str>> {
            let mut values = response.headers().get_all(name).iter();
            let first = values.next();
            ensure!(
                values.next().is_none(),
                "google_workspace_http_duplicate_header"
            );
            first
                .map(|v| {
                    v.to_str()
                        .map_err(|_| anyhow::anyhow!("google_workspace_http_header_invalid"))
                })
                .transpose()
        };
        ensure!(
            single(reqwest::header::CONTENT_ENCODING)?
                .is_none_or(|v| v.eq_ignore_ascii_case("identity")),
            "google_workspace_http_content_encoding_unsupported"
        );
        let content_type = single(reqwest::header::CONTENT_TYPE)?
            .ok_or_else(|| anyhow::anyhow!("google_workspace_http_content_type_missing"))?;
        let mut parts = content_type.split(';');
        let base = parts.next().unwrap_or("").trim();
        ensure!(
            base.eq_ignore_ascii_case(p.shortcut.metadata().kind.export_mime())
                || base.eq_ignore_ascii_case("application/octet-stream"),
            "google_workspace_http_content_type_mismatch"
        );
        for part in parts {
            let (name, value) = part
                .trim()
                .split_once('=')
                .ok_or_else(|| anyhow::anyhow!("google_workspace_http_content_type_invalid"))?;
            ensure!(
                name.trim().eq_ignore_ascii_case("charset")
                    && matches!(
                        value.trim().trim_matches('"').to_ascii_lowercase().as_str(),
                        "utf-8" | "utf8"
                    ),
                "google_workspace_http_content_type_unsupported"
            );
        }
        ensure!(
            response
                .content_length()
                .is_none_or(|n| n <= limits.max_body_bytes as u64),
            "google_workspace_body_capacity_exceeded"
        );
        let mut body = Zeroizing::new(Vec::new());
        while let Some(chunk) = checked(response.chunk(), deadline).await? {
            ensure!(
                body.len()
                    .checked_add(chunk.len())
                    .is_some_and(|n| n <= limits.max_body_bytes),
                "google_workspace_body_capacity_exceeded"
            );
            body.extend_from_slice(&chunk);
        }
        ensure!(
            !reflected(&body, token.0.as_bytes(), deadline)?
                && !p
                    .shortcut
                    .resource_key()
                    .map(|key| reflected(&body, key.as_bytes(), deadline))
                    .transpose()?
                    .unwrap_or(false),
            "google_workspace_private_response_refused"
        );
        let (_after, hash) =
            acquisition::hash_program_bounded(&p.program, MAX_NATIVE_IMAGE, deadline)?;
        ensure!(
            hash == p.claim.executable_sha256,
            "google_workspace_native_exporter_changed"
        );
        ReceivedExport::from_received_data(
            p.shortcut.metadata().clone(),
            p.claim.clone(),
            &body,
            limits,
            deadline,
        )
    });
    drop(client);
    runtime.shutdown_timeout(Duration::from_millis(100));
    result
}
/// One GET to the fixed Drive endpoint. No ambient credentials or refresh behavior.
pub fn capture_http(
    source_root: &Path,
    bytes: &[u8],
    spec: &HttpSpec,
    permissions: &HttpPermissions,
    token: &HostAccessToken,
    limits: &Limits,
    deadline: &Deadline,
) -> Result<ReceivedExport> {
    deadline.check()?;
    ensure!(
        permissions.allow_network
            && permissions.allow_private_credential
            && permissions.trust_native_exporter,
        "google_workspace_http_host_permission_required"
    );
    let p = prepare(source_root, bytes, spec, limits, deadline)?;
    ensure!(
        p.url.scheme() == "https"
            && p.url.host_str() == Some(HOST)
            && p.url.port_or_known_default() == Some(443),
        "google_workspace_http_endpoint_invalid"
    );
    receive(&p, spec.address, token, limits, deadline, &p.url)
}

#[path = "google_workspace_http_stdin.rs"]
mod private_stdin;
pub use private_stdin::read_private_token_stdin;
#[cfg(test)]
#[path = "google_workspace_http_tests.rs"]
mod tests;
