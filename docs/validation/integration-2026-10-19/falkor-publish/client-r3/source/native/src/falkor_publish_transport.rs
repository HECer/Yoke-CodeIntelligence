//! Owned pinned TCP/TLS connection; no DNS, proxy, redirects or reconnects.
use super::Target;
use crate::{
    capture::Deadline,
    falkor_wire::{self, Value},
};
use anyhow::{Result, ensure};
use base64::Engine as _;
use serde_json::{Value as Json, json};
use std::{
    io::{Read, Write},
    net::TcpStream,
    sync::Arc,
    time::Duration,
};
#[cfg(test)]
#[path = "falkor_publish_transport_tests.rs"]
mod tests;
#[derive(Default)]
pub struct Permissions {
    pub allow_network: bool,
    pub allow_credential_lookup: bool,
    pub allow_loopback_plaintext: bool,
    pub allow_write: bool,
    pub allow_schema: bool,
    pub allow_approval: bool,
    pub allow_journal: bool,
    pub allow_recovery: bool,
}
struct Socket {
    stream: TcpStream,
    deadline: Deadline,
}
impl Socket {
    fn check(&self) -> std::io::Result<()> {
        self.deadline.check().map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "falkor_transport_deadline_or_cancel",
            )
        })
    }
}
impl Read for Socket {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        loop {
            self.check()?;
            self.stream.set_read_timeout(Some(
                self.deadline.remaining().min(Duration::from_millis(10)),
            ))?;
            match self.stream.read(out) {
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::TimedOut
                            | std::io::ErrorKind::WouldBlock
                            | std::io::ErrorKind::Interrupted
                    ) =>
                {
                    continue;
                }
                result => {
                    self.check()?;
                    return result;
                }
            }
        }
    }
}
impl Write for Socket {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        loop {
            self.check()?;
            self.stream.set_write_timeout(Some(
                self.deadline.remaining().min(Duration::from_millis(10)),
            ))?;
            match self.stream.write(bytes) {
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::TimedOut
                            | std::io::ErrorKind::WouldBlock
                            | std::io::ErrorKind::Interrupted
                    ) =>
                {
                    continue;
                }
                result => {
                    self.check()?;
                    return result;
                }
            }
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.check()?;
        self.stream.flush()?;
        self.check()
    }
}
enum Stream {
    Plain(Socket),
    Tls(Box<rustls::StreamOwned<rustls::ClientConnection, Socket>>),
}
impl Read for Stream {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::Plain(s) => s.read(out),
            Self::Tls(s) => s.read(out),
        }
    }
}
impl Write for Stream {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        match self {
            Self::Plain(s) => s.write(bytes),
            Self::Tls(s) => s.write(bytes),
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Self::Plain(s) => s.flush(),
            Self::Tls(s) => s.flush(),
        }
    }
}
pub(super) struct Session {
    stream: Stream,
}
fn no_error(value: &Value) -> bool {
    match value {
        Value::Error(_) => false,
        Value::Array(Some(values)) => values.iter().all(no_error),
        _ => true,
    }
}
impl Session {
    pub(super) fn exists(&mut self, key: &str, d: &Deadline) -> Result<bool> {
        match self.send(&[b"EXISTS".to_vec(), key.as_bytes().to_vec()], d)? {
            Value::Integer(0) => Ok(false),
            Value::Integer(1) => Ok(true),
            _ => anyhow::bail!("falkor_publish_response_invalid"),
        }
    }
    pub(super) fn queue_initialize(
        &mut self,
        key: &str,
        command: &[Vec<u8>],
        d: &Deadline,
    ) -> Result<()> {
        ensure!(
            self.send(&[b"WATCH".to_vec(), key.as_bytes().to_vec()], d)?
                == Value::Simple(b"OK".to_vec()),
            "falkor_publish_response_invalid"
        );
        ensure!(
            !self.exists(key, d)?,
            "falkor_publish_namespace_already_exists"
        );
        ensure!(
            self.send(&[b"MULTI".to_vec()], d)? == Value::Simple(b"OK".to_vec()),
            "falkor_publish_response_invalid"
        );
        ensure!(
            self.send(command, d)? == Value::Simple(b"QUEUED".to_vec()),
            "falkor_publish_response_invalid"
        );
        Ok(())
    }
    pub(super) fn execute_initialize(&mut self, d: &Deadline) -> Result<Value> {
        let value = self.send(&[b"EXEC".to_vec()], d)?;
        let Value::Array(Some(mut values)) = value else {
            anyhow::bail!("falkor_publish_atomic_initialization_unconfirmed")
        };
        ensure!(values.len() == 1, "falkor_publish_response_invalid");
        Ok(values.remove(0))
    }
    pub(super) fn open(target: &Target, permissions: &Permissions, d: &Deadline) -> Result<Self> {
        ensure!(
            permissions.allow_network,
            "falkor_publish_network_permission_required"
        );
        ensure!(
            permissions.allow_credential_lookup,
            "falkor_publish_credential_permission_required"
        );
        target.validate()?;
        d.check()?;
        ensure!(
            target.url()?.scheme() != "redis" || permissions.allow_loopback_plaintext,
            "falkor_publish_loopback_plaintext_permission_required"
        );
        let secret =
            crate::host_credentials::read_falkor_password(&target.credential_binding()?, d)
                .map_err(|_| anyhow::anyhow!("falkor_publish_credential_unavailable"))?;
        let mut session = Self::with_secret(target, &secret, d)?;
        let modules = session.send(&[b"MODULE".to_vec(), b"LIST".to_vec()], d)?;
        let Value::Array(Some(modules)) = modules else {
            anyhow::bail!("falkor_publish_server_profile_unsupported")
        };
        let mut graph_modules = 0;
        for module in modules {
            let Value::Array(Some(fields)) = module else {
                anyhow::bail!("falkor_publish_server_profile_unsupported")
            };
            ensure!(
                fields.len() % 2 == 0 && fields.len() <= 64,
                "falkor_publish_server_profile_unsupported"
            );
            let mut name = None;
            let mut version = None;
            for field in fields.chunks_exact(2) {
                match &field[0] {
                    Value::Bulk(Some(key)) if key == b"name" => {
                        ensure!(name.is_none(), "falkor_publish_server_profile_unsupported");
                        name = Some(&field[1]);
                    }
                    Value::Bulk(Some(key)) if key == b"ver" => {
                        ensure!(
                            version.is_none(),
                            "falkor_publish_server_profile_unsupported"
                        );
                        version = Some(&field[1]);
                    }
                    _ => {}
                }
            }
            if name == Some(&Value::Bulk(Some(b"graph".to_vec()))) {
                ensure!(
                    version == Some(&Value::Integer(60002)),
                    "falkor_publish_server_profile_unsupported"
                );
                graph_modules += 1;
            }
        }
        ensure!(
            graph_modules == 1,
            "falkor_publish_server_profile_unsupported"
        );
        d.check()?;
        Ok(session)
    }
    fn with_secret(target: &Target, secret: &str, d: &Deadline) -> Result<Self> {
        target.validate()?;
        d.check()?;
        let url = target.url()?;
        let tls = if url.scheme() == "rediss" {
            let mut roots = rustls::RootCertStore::empty();
            if target.use_system_roots {
                let loaded = rustls_native_certs::load_native_certs();
                ensure!(
                    loaded.errors.is_empty() && loaded.certs.len() <= 1024,
                    "falkor_publish_tls_roots_unavailable"
                );
                for cert in loaded.certs {
                    d.check()?;
                    ensure!(cert.len() <= 16384, "falkor_publish_tls_roots_invalid");
                    roots
                        .add(cert)
                        .map_err(|_| anyhow::anyhow!("falkor_publish_tls_roots_invalid"))?;
                }
            }
            for encoded in &target.root_certificates_der_base64 {
                d.check()?;
                let cert = base64::prelude::BASE64_STANDARD
                    .decode(encoded)
                    .map_err(|_| anyhow::anyhow!("falkor_publish_tls_roots_invalid"))?;
                roots
                    .add(rustls::pki_types::CertificateDer::from(cert))
                    .map_err(|_| anyhow::anyhow!("falkor_publish_tls_roots_invalid"))?;
            }
            ensure!(!roots.is_empty(), "falkor_publish_tls_roots_unavailable");
            let config = rustls::ClientConfig::builder_with_provider(Arc::new(
                rustls::crypto::ring::default_provider(),
            ))
            .with_safe_default_protocol_versions()
            .map_err(|_| anyhow::anyhow!("falkor_publish_tls_unavailable"))?
            .with_root_certificates(roots)
            .with_no_client_auth();
            let host = url.host_str().unwrap();
            let server_name = match url.host() {
                Some(url::Host::Ipv6(ip)) => {
                    rustls::pki_types::ServerName::IpAddress(std::net::IpAddr::V6(ip).into())
                }
                _ => rustls::pki_types::ServerName::try_from(host.to_owned())
                    .map_err(|_| anyhow::anyhow!("falkor_publish_target_invalid"))?,
            };
            Some(
                rustls::ClientConnection::new(Arc::new(config), server_name)
                    .map_err(|_| anyhow::anyhow!("falkor_publish_tls_unavailable"))?,
            )
        } else {
            None
        };
        d.check()?;
        let stream = TcpStream::connect_timeout(&target.address, d.remaining())
            .map_err(|_| anyhow::anyhow!("falkor_publish_transport_failed"))?;
        stream
            .set_nodelay(true)
            .map_err(|_| anyhow::anyhow!("falkor_publish_transport_failed"))?;
        let socket = Socket {
            stream,
            deadline: d.clone(),
        };
        let stream = if let Some(connection) = tls {
            Stream::Tls(Box::new(rustls::StreamOwned::new(connection, socket)))
        } else {
            Stream::Plain(socket)
        };
        let mut session = Self { stream };
        let args = [
            b"AUTH".as_slice(),
            target.user.as_bytes(),
            secret.as_bytes(),
        ];
        let wire = falkor_wire::encode(&args, &Default::default(), d)?;
        session
            .stream
            .write_all(&wire)
            .and_then(|_| session.stream.flush())
            .map_err(|_| anyhow::anyhow!("falkor_publish_transport_failed"))?;
        let response = falkor_wire::read(&mut session.stream, &Default::default(), d)?;
        ensure!(
            response == Value::Simple(b"OK".to_vec()),
            "falkor_publish_authentication_refused"
        );
        d.check()?;
        Ok(session)
    }
    pub(super) fn send(&mut self, args: &[Vec<u8>], d: &Deadline) -> Result<Value> {
        let wire = falkor_wire::encode(
            &args.iter().map(Vec::as_slice).collect::<Vec<_>>(),
            &Default::default(),
            d,
        )?;
        d.check()?;
        self.stream
            .write_all(&wire)
            .and_then(|_| self.stream.flush())
            .map_err(|_| anyhow::anyhow!("falkor_publish_transport_failed"))?;
        let response = falkor_wire::read(&mut self.stream, &Default::default(), d)?;
        ensure!(no_error(&response), "falkor_publish_query_refused");
        d.check()?;
        Ok(response)
    }
}
pub(super) fn data(value: &Value) -> Result<(&[Value], &[Value])> {
    let Value::Array(Some(parts)) = value else {
        anyhow::bail!("falkor_publish_response_invalid")
    };
    ensure!(parts.len() == 3, "falkor_publish_response_invalid");
    let (Value::Array(Some(fields)), Value::Array(Some(rows)), Value::Array(Some(stats))) =
        (&parts[0], &parts[1], &parts[2])
    else {
        anyhow::bail!("falkor_publish_response_invalid")
    };
    ensure!(
        fields.len() <= 64 && rows.len() <= 50000 && stats.len() <= 64,
        "falkor_publish_response_invalid"
    );
    for field in fields.iter().chain(stats) {
        text(field)?;
    }
    for row in rows {
        let Value::Array(Some(cells)) = row else {
            anyhow::bail!("falkor_publish_response_invalid")
        };
        ensure!(
            cells.len() == fields.len(),
            "falkor_publish_response_invalid"
        );
    }
    Ok((fields, rows))
}
pub(super) fn text(value: &Value) -> Result<&str> {
    let Value::Bulk(Some(bytes)) = value else {
        anyhow::bail!("falkor_publish_response_invalid")
    };
    std::str::from_utf8(bytes).map_err(|_| anyhow::anyhow!("falkor_publish_response_invalid"))
}
pub(super) fn json_value(value: &Value) -> Result<Json> {
    Ok(match value {
        Value::Simple(bytes) | Value::Bulk(Some(bytes)) => json!(
            std::str::from_utf8(bytes)
                .map_err(|_| anyhow::anyhow!("falkor_publish_response_invalid"))?
        ),
        Value::Integer(n) => json!(n),
        Value::Bulk(None) | Value::Array(None) => Json::Null,
        Value::Array(Some(values)) => {
            Json::Array(values.iter().map(json_value).collect::<Result<_>>()?)
        }
        Value::Error(_) => anyhow::bail!("falkor_publish_query_refused"),
    })
}
