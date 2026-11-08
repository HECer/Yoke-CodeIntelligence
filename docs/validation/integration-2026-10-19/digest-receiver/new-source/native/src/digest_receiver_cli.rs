//! Explicit receiving service: no Engine, capture, model or source execution.
use anyhow::{Result, ensure};
use clap::Args;
use std::{
    io::Write,
    net::SocketAddr,
    path::{Path, PathBuf},
};
use yoke::{capture::Deadline, github_app::digest::service::receiver, store::Store};
#[derive(Args)]
pub struct Arguments {
    #[arg(long)]
    profile: PathBuf,
    #[arg(long, default_value = "127.0.0.1:0")]
    listen: SocketAddr,
    #[arg(long)]
    tls_credential: Option<String>,
    #[arg(long)]
    allow_receiver: bool,
    #[arg(long)]
    allow_network: bool,
    #[arg(long)]
    allow_remote: bool,
    #[arg(long)]
    allow_fixture: bool,
    #[arg(long, default_value_t = 5000)]
    request_timeout_ms: u64,
    #[arg(long, default_value_t = 1000)]
    max_requests: usize,
    #[arg(long, default_value_t = 4096)]
    max_response_bytes: usize,
}
impl Arguments {
    pub fn run(&self, root: &Path, writes: bool, lifetime: u64, max_bytes: u64) -> Result<bool> {
        let d = Deadline::new(lifetime);
        let mut config = receiver::Config {
            allow_receiver: self.allow_receiver,
            allow_network: self.allow_network,
            allow_writes: writes,
            allow_remote: self.allow_remote,
            allow_fixture: self.allow_fixture,
            listen: self.listen,
            request_timeout_ms: self.request_timeout_ms,
            lifetime_ms: lifetime,
            max_requests: self.max_requests,
            max_response_bytes: self.max_response_bytes,
            tls: None,
        };
        config.admit_selection(self.tls_credential.is_some(), &d)?;
        ensure!(
            (1024..=65536).contains(&max_bytes) && self.profile.is_absolute(),
            "github_app_digest_receiver_input_invalid"
        );
        let profile = receiver::Profile::parse(
            &crate::feedback_cli::host_bytes(&self.profile, 32_000, &d)
                .map_err(|_| anyhow::anyhow!("github_app_digest_receiver_input_invalid"))?,
        )?;
        config.admit_profile_selection(&profile, self.tls_credential.is_some(), &d)?;
        if let Some(name) = &self.tls_credential {
            let identity = yoke::host_credentials::read_tls(name)
                .map_err(|_| anyhow::anyhow!("github_app_digest_receiver_tls_unavailable"))?;
            config.tls = Some(yoke::federation_http::TlsMaterial {
                certificates: vec![identity.certificate],
                private_key: identity.private_key,
            });
        }
        d.check()?;
        let store = Store::open_with_timeout(root, d.remaining())?;
        let server = receiver::start(store, profile, config, &d)?;
        let mut raw =
            yoke::json::encode_bounded(server.announcement(), max_bytes as usize - 1, &d)?;
        raw.push(b'\n');
        {
            let mut out = std::io::stdout().lock();
            out.write_all(&raw)?;
            out.flush()?;
        }
        server.wait()?;
        Ok(true)
    }
}
