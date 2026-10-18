//! Owned TCP/TLS channels with absolute, inherited read/write deadlines.
use super::*;
use std::{
    io::{self, Read, Write},
    net::{Shutdown, TcpStream},
};
use zeroize::Zeroizing;
pub(super) struct Physical<'a> {
    socket: TcpStream,
    deadline: &'a Deadline,
}
impl Physical<'_> {
    fn budget(&self) -> io::Result<Duration> {
        self.deadline
            .check()
            .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "github_app_io_deadline"))?;
        Ok(self.deadline.remaining().min(Duration::from_millis(50)))
    }
}
impl Drop for Physical<'_> {
    fn drop(&mut self) {
        let _ = self.socket.shutdown(Shutdown::Both);
    }
}
impl Read for Physical<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.socket.set_read_timeout(Some(self.budget()?))?;
        let result = self.socket.read(bytes);
        self.budget()?;
        result
    }
}
impl Write for Physical<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.socket.set_write_timeout(Some(self.budget()?))?;
        let result = self.socket.write(bytes);
        self.budget()?;
        result
    }
    fn flush(&mut self) -> io::Result<()> {
        self.budget()?;
        self.socket.flush()
    }
}
pub(super) enum Channel<'a> {
    Plain(Physical<'a>),
    Tls(Box<rustls::StreamOwned<rustls::ServerConnection, Physical<'a>>>),
}
impl<'a> Channel<'a> {
    pub fn new(socket: TcpStream, shared: &Shared, deadline: &'a Deadline) -> Result<Self> {
        socket.set_nonblocking(false)?;
        let io = Physical { socket, deadline };
        Ok(if let Some(tls) = &shared.tls {
            Self::Tls(Box::new(rustls::StreamOwned::new(
                rustls::ServerConnection::new(tls.clone())
                    .map_err(|_| anyhow::anyhow!("github_app_tls_invalid"))?,
                io,
            )))
        } else {
            Self::Plain(io)
        })
    }
    pub fn server_name(&self) -> Option<&str> {
        match self {
            Self::Plain(_) => None,
            Self::Tls(stream) => stream.conn.server_name(),
        }
    }
    pub fn discard_pending(&mut self, d: &Deadline) {
        if let Self::Plain(io) = self {
            if io.socket.set_nonblocking(true).is_err() {
                return;
            }
            let mut bytes = Zeroizing::new([0; 4096]);
            let mut read = 0;
            while read < 65_536 && d.check().is_ok() {
                match io.socket.read(&mut *bytes) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => read += n,
                }
            }
        }
    }
}
impl Read for Channel<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Plain(io) => io.read(bytes),
            Self::Tls(io) => io.read(bytes),
        }
    }
}
impl Write for Channel<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        match self {
            Self::Plain(io) => io.write(bytes),
            Self::Tls(io) => io.write(bytes),
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Plain(io) => io.flush(),
            Self::Tls(io) => io.flush(),
        }
    }
}
pub(super) fn retry(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
    )
}
pub(super) fn tls_server(material: TlsMaterial) -> Result<Arc<rustls::ServerConfig>> {
    ensure!(
        !material.certificates.is_empty()
            && material.certificates.len() <= 8
            && material
                .certificates
                .iter()
                .all(|c| !c.is_empty() && c.len() <= 32_000)
            && material.certificates.iter().map(Vec::len).sum::<usize>() <= 128_000
            && !material.private_key.is_empty()
            && material.private_key.len() <= 64_000,
        "github_app_tls_invalid"
    );
    let certificates = material
        .certificates
        .into_iter()
        .map(rustls::pki_types::CertificateDer::from)
        .collect();
    let key = rustls::pki_types::PrivateKeyDer::try_from(material.private_key.to_vec())
        .map_err(|_| anyhow::anyhow!("github_app_tls_invalid"))?;
    let mut config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_protocol_versions(&[&rustls::version::TLS13])
    .map_err(|_| anyhow::anyhow!("github_app_tls_invalid"))?
    .with_no_client_auth()
    .with_single_cert(certificates, key)
    .map_err(|_| anyhow::anyhow!("github_app_tls_invalid"))?;
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(Arc::new(config))
}
