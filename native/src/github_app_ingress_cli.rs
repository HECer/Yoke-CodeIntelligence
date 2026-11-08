//! Host-only service flags. Incoming JSON cannot select listener or credentials.
use anyhow::{Result, ensure};
use clap::Args;
use std::{io::Write, net::SocketAddr, path::Path};
use yoke::{
    Engine,
    capture::Deadline,
    github_app::{
        Host, HostProfile, Options,
        ingress::{self, Config},
    },
};
#[derive(Args)]
pub struct Arguments {
    #[arg(long, default_value = "127.0.0.1:0")]
    listen: SocketAddr,
    #[arg(long)]
    allow_remote_webhooks: bool,
    #[arg(long)]
    tls_credential: Option<String>,
    #[arg(long = "tls-server-name")]
    server_names: Vec<String>,
    #[arg(long, default_value_t = 5000)]
    request_timeout_ms: u64,
    #[arg(long)]
    lifetime_ms: Option<u64>,
    #[arg(long, default_value_t = 4)]
    max_connections: usize,
    #[arg(long, default_value_t = 1000)]
    max_requests: usize,
    #[arg(long, default_value_t = 4096)]
    max_response_bytes: usize,
}
pub struct Prepared {
    host: Host,
    options: Options,
    config: Config,
}
impl Arguments {
    pub fn prepare(
        &self,
        profile: Option<&Path>,
        allow_webhooks: bool,
        options: Options,
        timeout: u64,
        d: &Deadline,
    ) -> Result<Prepared> {
        let mut config = Config {
            allow_webhooks,
            listen: self.listen,
            allow_remote: self.allow_remote_webhooks,
            tls: None,
            server_names: self.server_names.clone(),
            request_timeout_ms: self.request_timeout_ms,
            lifetime_ms: self.lifetime_ms.unwrap_or(timeout),
            max_connections: self.max_connections,
            max_requests: self.max_requests,
            max_response_bytes: self.max_response_bytes,
        };
        config.admit_limits(&options, d)?;
        ensure!(
            self.listen.ip().is_loopback()
                || (self.allow_remote_webhooks && self.tls_credential.is_some()),
            "github_app_remote_tls_permission_required"
        );
        ensure!(
            self.tls_credential.is_some() != self.server_names.is_empty(),
            "github_app_ingress_input_invalid"
        );
        let path = profile.ok_or_else(|| anyhow::anyhow!("github_app_ingress_input_invalid"))?;
        let raw = crate::feedback_cli::host_bytes(path, 32_000, d)
            .map_err(|_| anyhow::anyhow!("github_app_ingress_input_invalid"))?;
        let profile = HostProfile::parse(&raw)
            .map_err(|_| anyhow::anyhow!("github_app_ingress_input_invalid"))?;
        d.check()?;
        if let Some(name) = &self.tls_credential {
            let identity = yoke::host_credentials::read_tls(name)?;
            config.tls = Some(yoke::federation_http::TlsMaterial {
                certificates: vec![identity.certificate],
                private_key: identity.private_key,
            });
        }
        config.admit(&options, d)?;
        let host = Host::from_os(profile)?;
        d.check()?;
        Ok(Prepared {
            host,
            options,
            config,
        })
    }
}
impl Prepared {
    pub fn run(self, engine: Engine, d: &Deadline) -> Result<bool> {
        let cap = self.options.max_output_bytes;
        let handle = ingress::start(engine, self.host, self.options, self.config, d)?;
        let raw = yoke::json::encode_bounded(handle.announcement(), cap.saturating_sub(1), d)?;
        d.check()?;
        {
            let mut stdout = std::io::stdout().lock();
            stdout.write_all(&raw)?;
            stdout.write_all(b"\n")?;
            stdout.flush()?;
        }
        handle.wait()?;
        Ok(true)
    }
}
