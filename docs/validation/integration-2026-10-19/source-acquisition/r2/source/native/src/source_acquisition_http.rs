//! One host-authorized public GET to one literal address. No implicit DNS,
//! credentials, proxy, redirect, retry, page resources or project execution.
use crate::{
    capture::Deadline,
    source_acquisition::{self as source, Format, Limits, ReceivedSource},
};
use anyhow::{Result, ensure};
use base64::Engine as _;
use serde::Deserialize;
use serde_json::json;
use std::{future::Future, net::SocketAddr, task::Poll, time::Duration};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Spec {
    pub url: String,
    pub address: SocketAddr,
    pub format: Format,
    #[serde(default)]
    pub root_certificates_der_base64: Vec<String>,
}
/// Deliberately not deserializable from an imported source or request.
#[derive(Default)]
pub struct Permissions {
    pub allow_network: bool,
    pub allow_loopback_http: bool,
}
impl Spec {
    pub fn validate(&self) -> Result<url::Url> {
        let url = source::url(&self.url)?;
        source::input(
            self.address.port() > 0
                && Some(self.address.port()) == url.port_or_known_default()
                && !self.address.ip().is_unspecified()
                && !self.address.ip().is_multicast(),
        )?;
        match url
            .host()
            .ok_or_else(|| anyhow::anyhow!("source_acquisition_input_invalid"))?
        {
            url::Host::Ipv4(ip) => source::input(self.address.ip() == ip)?,
            url::Host::Ipv6(ip) => source::input(self.address.ip() == ip)?,
            url::Host::Domain(host) => {
                if host == "arxiv.org" || host.ends_with(".arxiv.org") {
                    anyhow::bail!("source_acquisition_arxiv_connector_required");
                }
            }
        }
        if url.scheme() == "http" {
            source::input(
                self.address.ip().is_loopback()
                    && !matches!(url.host(), Some(url::Host::Domain(_))),
            )?;
        }
        source::capacity(self.root_certificates_der_base64.len() <= 8)?;
        for certificate in &self.root_certificates_der_base64 {
            decode_certificate(certificate)?;
        }
        Ok(url)
    }
}
fn decode_certificate(encoded: &str) -> Result<reqwest::Certificate> {
    source::capacity(encoded.len() <= 16_384usize.div_ceil(3) * 4)?;
    let bytes = base64::prelude::BASE64_STANDARD
        .decode(encoded)
        .map_err(|_| anyhow::anyhow!("source_acquisition_tls_roots_invalid"))?;
    source::capacity(!bytes.is_empty() && bytes.len() <= 16384)?;
    let mut roots = rustls::RootCertStore::empty();
    roots
        .add(rustls::pki_types::CertificateDer::from(bytes.clone()))
        .map_err(|_| anyhow::anyhow!("source_acquisition_tls_roots_invalid"))?;
    reqwest::Certificate::from_der(&bytes)
        .map_err(|_| anyhow::anyhow!("source_acquisition_tls_roots_invalid"))
}
async fn checked<T>(
    future: impl Future<Output = reqwest::Result<T>>,
    deadline: &Deadline,
) -> Result<T> {
    let mut future = std::pin::pin!(future);
    let mut ticks = tokio::time::interval(Duration::from_millis(10));
    std::future::poll_fn(|cx| {
        if let Err(error) = source::check(deadline) {
            return Poll::Ready(Err(error));
        }
        if let Poll::Ready(value) = future.as_mut().poll(cx) {
            if let Err(error) = source::check(deadline) {
                return Poll::Ready(Err(error));
            }
            return Poll::Ready(
                value.map_err(|_| anyhow::anyhow!("source_acquisition_transport_failed")),
            );
        }
        while ticks.poll_tick(cx).is_ready() {
            if let Err(error) = source::check(deadline) {
                return Poll::Ready(Err(error));
            }
        }
        Poll::Pending
    })
    .await
}
pub fn capture(
    spec: &Spec,
    permissions: &Permissions,
    limits: &Limits,
    deadline: &Deadline,
) -> Result<ReceivedSource> {
    ensure!(
        permissions.allow_network,
        "source_acquisition_network_permission_required"
    );
    limits.validate()?;
    let url = spec.validate()?;
    if url.scheme() == "http" {
        ensure!(
            permissions.allow_loopback_http,
            "source_acquisition_loopback_http_permission_required"
        );
    }
    source::check(deadline)?;
    ensure!(
        tokio::runtime::Handle::try_current().is_err(),
        "source_acquisition_synchronous_host_required"
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
        .resolve_to_addrs(url.host_str().unwrap(), &[spec.address]);
    for root in &spec.root_certificates_der_base64 {
        builder = builder.add_root_certificate(decode_certificate(root)?);
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| anyhow::anyhow!("source_acquisition_transport_failed"))?;
    let client = builder
        .build()
        .map_err(|_| anyhow::anyhow!("source_acquisition_transport_failed"))?;
    let result=runtime.block_on(async{
        let mut response=checked(client.get(url.clone()).header("User-Agent","Yoke-CodeIntelligence/0.5 source-acquisition").header("Accept-Encoding","identity").header("Accept",match spec.format{Format::Text=>"text/plain",Format::Html=>"text/html",Format::Pdf=>"application/pdf"}).send(),deadline).await?;
        ensure!(response.url()==&url && response.status()==200,"source_acquisition_http_refused");
        let mut header_bytes=0usize;
        for (key,value) in response.headers(){source::check(deadline)?;header_bytes=header_bytes.checked_add(key.as_str().len()+value.as_bytes().len()).ok_or_else(||anyhow::anyhow!("source_acquisition_capacity_exceeded"))?;source::capacity(header_bytes<=16384)?;}
        ensure!(!response.headers().contains_key(reqwest::header::CONTENT_ENCODING),"source_acquisition_content_encoding_unsupported");
        let field=|key:reqwest::header::HeaderName|->Result<Option<String>>{
            let mut values=response.headers().get_all(key).iter();let first=values.next();source::input(values.next().is_none())?;
            first.map(|v|v.to_str().map(str::to_owned).map_err(|_|anyhow::anyhow!("source_acquisition_header_invalid"))).transpose()
        };
        let content_type=field(reqwest::header::CONTENT_TYPE)?.ok_or_else(||anyhow::anyhow!("source_acquisition_content_type_missing"))?;
        let metadata=json!({"schemaVersion":1,"format":spec.format,"requestedUrl":url.as_str(),"httpStatus":200,"contentType":content_type,"etag":field(reqwest::header::ETAG)?,"lastModified":field(reqwest::header::LAST_MODIFIED)?});
        source::capacity(response.content_length().is_none_or(|n|n<=limits.max_body_bytes as u64))?;
        let mut bytes=Vec::new();
        loop {let Some(chunk)=checked(response.chunk(),deadline).await? else{break};source::capacity(bytes.len().checked_add(chunk.len()).is_some_and(|n|n<=limits.max_body_bytes))?;bytes.extend_from_slice(&chunk);}
        let mut source=ReceivedSource::from_response_with_limits(&metadata,&bytes,limits,deadline)?;
        source.set_claims(json!({"kind":"unverified_http_capture_claims","transport":"native_reqwest_http1","address":spec.address.to_string(),"tls":url.scheme()=="https","trustRoots":"platform_roots_plus_host_additions","requests":1,"redirectsFollowed":0,"credentialsForwarded":false,"proxyUsed":false,"libraryInternalAllocationsBounded":false,"originalWireHeadersPreserved":false}));
        source::check(deadline)?;Ok(source)
    });
    drop(client);
    runtime.shutdown_timeout(Duration::from_millis(100));
    result
}
