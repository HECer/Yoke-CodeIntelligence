//! Host-authorized, pinned native psql transport. Catalog bytes grant no authority.
use crate::{
    capture::Deadline,
    postgres_catalog::{self as catalog, Catalog, Limits},
};
use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    net::IpAddr,
    path::{Component, Path, PathBuf},
    process::Command,
};
use zeroize::Zeroizing;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Spec {
    pub client: PathBuf,
    pub client_sha256: String,
    pub address: IpAddr,
    pub port: u16,
    pub database: String,
    pub user: String,
    pub security: Security,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Security {
    LoopbackTrust,
    TlsScram {
        #[serde(rename = "serverName")]
        server_name: String,
        #[serde(rename = "rootCertificatePem")]
        root_certificate_pem: String,
    },
}
/// Constructed by the host, never deserialized from a provider or artifact.
#[derive(Default)]
pub struct Permissions {
    pub trust_client: bool,
    pub allow_network: bool,
    pub allow_loopback_trust: bool,
}

fn endpoint_text(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 256
        && !s.chars().any(char::is_control)
        && !crate::paths::obvious_secret(s)
}
impl Spec {
    pub fn validate(&self) -> Result<()> {
        catalog::input(
            self.port != 0
                && !self.address.is_unspecified()
                && !self.address.is_multicast()
                && endpoint_text(&self.database)
                && endpoint_text(&self.user)
                && crate::model::valid_id(&self.client_sha256),
        )?;
        host_path(&self.client)?;
        match &self.security {
            Security::LoopbackTrust => catalog::input(self.address.is_loopback())?,
            Security::TlsScram {
                server_name,
                root_certificate_pem,
            } => {
                catalog::input(
                    !server_name.is_empty()
                        && server_name.len() <= 253
                        && server_name
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-'))
                        && !server_name.starts_with('-')
                        && !server_name.ends_with('-'),
                )?;
                catalog::capacity(root_certificate_pem.len() <= 65536)?;
                use rustls::pki_types::pem::PemObject;
                let mut roots = rustls::RootCertStore::empty();
                for cert in rustls::pki_types::CertificateDer::pem_slice_iter(
                    root_certificate_pem.as_bytes(),
                ) {
                    roots
                        .add(cert.map_err(|_| anyhow::anyhow!("postgres_catalog_ca_invalid"))?)
                        .map_err(|_| anyhow::anyhow!("postgres_catalog_ca_invalid"))?;
                    catalog::capacity(roots.len() <= 16)?;
                }
                catalog::input(!roots.is_empty())?;
            }
        }
        Ok(())
    }
    pub fn credential_binding(&self) -> Result<String> {
        self.validate()?;
        let Security::TlsScram {
            server_name,
            root_certificate_pem,
        } = &self.security
        else {
            bail!("postgres_catalog_password_not_applicable")
        };
        Ok(crate::model::identity(&(
            "postgres-catalog-password-v1",
            self.address,
            self.port,
            &self.database,
            &self.user,
            server_name,
            crate::model::hash(root_certificate_pem.as_bytes()),
        )))
    }
}
fn host_path(path: &Path) -> Result<()> {
    let s = path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("postgres_catalog_client_path_invalid"))?;
    catalog::input(path.is_absolute() && s.len() <= 4096 && !s.chars().any(char::is_control))?;
    for part in path.components() {
        match part {
            Component::ParentDir | Component::CurDir => catalog::input(false)?,
            Component::Normal(n) => crate::paths::normalized_syntax(
                n.to_str()
                    .ok_or_else(|| anyhow::anyhow!("postgres_catalog_client_path_invalid"))?,
            )
            .map_err(|_| anyhow::anyhow!("postgres_catalog_client_path_invalid"))?,
            #[cfg(windows)]
            Component::Prefix(p) => catalog::input(matches!(
                p.kind(),
                std::path::Prefix::Disk(_) | std::path::Prefix::VerbatimDisk(_)
            ))?,
            _ => {}
        }
    }
    Ok(())
}
struct ClientGuard {
    _files: Vec<File>,
}
impl ClientGuard {
    fn open(spec: &Spec, d: &Deadline) -> Result<Self> {
        let mut files = Vec::new();
        for path in spec.client.ancestors() {
            catalog::check(d)?;
            let metadata = fs::symlink_metadata(path)
                .map_err(|_| anyhow::anyhow!("postgres_catalog_client_unavailable"))?;
            let leaf = path == spec.client;
            catalog::input(
                !crate::paths::is_link(&metadata)
                    && if leaf {
                        metadata.is_file() && metadata.len() <= 64_000_000
                    } else {
                        metadata.is_dir()
                    },
            )?;
            let mut options = OpenOptions::new();
            options.read(true);
            #[cfg(windows)]
            {
                use std::os::windows::fs::OpenOptionsExt;
                options
                    .share_mode(if leaf { 1 } else { 3 })
                    .custom_flags(0x0020_0000 | if leaf { 0 } else { 0x0200_0000 });
            }
            let mut file = options
                .open(path)
                .map_err(|_| anyhow::anyhow!("postgres_catalog_client_unavailable"))?;
            let opened = file
                .metadata()
                .map_err(|_| anyhow::anyhow!("postgres_catalog_client_unavailable"))?;
            catalog::input(
                !crate::paths::is_link(&opened)
                    && if leaf {
                        opened.is_file() && opened.len() == metadata.len()
                    } else {
                        opened.is_dir()
                    },
            )?;
            if leaf {
                use sha2::{Digest, Sha256};
                let mut h = Sha256::new();
                let mut buffer = [0u8; 65536];
                let mut total = 0usize;
                loop {
                    catalog::check(d)?;
                    let n = file
                        .read(&mut buffer)
                        .map_err(|_| anyhow::anyhow!("postgres_catalog_client_unavailable"))?;
                    if n == 0 {
                        break;
                    }
                    total += n;
                    catalog::capacity(total <= 64_000_000)?;
                    h.update(&buffer[..n]);
                }
                ensure!(
                    format!("{:x}", h.finalize()) == spec.client_sha256,
                    "postgres_catalog_client_hash_mismatch"
                );
            }
            files.push(file);
        }
        Ok(Self { _files: files })
    }
}
fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'"))
}
fn connection(spec: &Spec, scratch: &Path) -> Result<String> {
    let path = |name: &str| -> Result<String> {
        Ok(scratch
            .join(name)
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("postgres_catalog_private_path_invalid"))?
            .to_owned())
    };
    let mut parts=vec![("hostaddr",spec.address.to_string()),("port",spec.port.to_string()),("dbname",spec.database.clone()),("user",spec.user.clone()),
        ("connect_timeout","5".into()),("client_encoding","UTF8".into()),("application_name","yoke_catalog_readonly".into()),("gssencmode","disable".into()),
        ("passfile",path("empty-passfile")?),("sslcertmode","disable".into()),("sslcrldir",path("empty-crls")?),
        ("options","-c default_transaction_read_only=on -c search_path=pg_catalog -c statement_timeout=10000 -c lock_timeout=3000 -c idle_in_transaction_session_timeout=15000".into())];
    match &spec.security {
        Security::LoopbackTrust => parts.extend([
            ("host", spec.address.to_string()),
            ("sslmode", "disable".into()),
            ("require_auth", "none".into()),
        ]),
        Security::TlsScram { server_name, .. } => parts.extend([
            ("host", server_name.clone()),
            ("sslmode", "verify-full".into()),
            ("sslrootcert", path("root.pem")?),
            ("channel_binding", "require".into()),
            ("require_auth", "scram-sha-256".into()),
        ]),
    }
    Ok(parts
        .into_iter()
        .map(|(k, v)| format!("{k}={}", quote(&v)))
        .collect::<Vec<_>>()
        .join(" "))
}
fn command(spec: &Spec, scratch: &Path) -> Command {
    let mut c = Command::new(&spec.client);
    c.current_dir(scratch);
    c.env("HOME", scratch)
        .env("USERPROFILE", scratch)
        .env("APPDATA", scratch)
        .env("PGSERVICEFILE", scratch.join("empty-service"))
        .env("PGSYSCONFDIR", scratch)
        .env("PGPASSFILE", scratch.join("empty-passfile"));
    c
}
fn capture(command: Command, input: &[u8], cap: usize, d: &Deadline) -> Result<Vec<u8>> {
    let out = crate::process::run_capture(
        command,
        input,
        d,
        crate::process::CaptureLimits {
            stdin_bytes: 65536,
            stdout_bytes: cap,
            stderr_bytes: 65536,
        },
    )
    .map_err(|e| {
        if crate::process::cleanup_unproven(&e) {
            e
        } else {
            anyhow::anyhow!("postgres_catalog_client_failed")
        }
    })?;
    catalog::check(d)?;
    ensure!(
        out.exit_code == Some(0)
            && out.stdin_complete
            && out.output_complete
            && !out.timed_out
            && !out.output_exceeded
            && !out.io_error,
        "postgres_catalog_client_failed"
    );
    Ok(out.stdout)
}
pub fn receive(spec: &Spec, p: &Permissions, limits: &Limits, d: &Deadline) -> Result<Catalog> {
    ensure!(
        p.trust_client && p.allow_network,
        "postgres_catalog_host_permission_required"
    );
    if matches!(spec.security, Security::LoopbackTrust) {
        ensure!(
            p.allow_loopback_trust,
            "postgres_catalog_loopback_trust_permission_required"
        );
    }
    limits.validate()?;
    spec.validate()?;
    catalog::check(d)?;
    // This authorizes the selected installation and its runtime libraries. The
    // executable hash alone does not attest its DLLs, server, or internal memory.
    let _client = ClientGuard::open(spec, d)?;
    let scratch = tempfile::tempdir()
        .map_err(|_| anyhow::anyhow!("postgres_catalog_private_directory_unavailable"))?;
    for file in ["empty-passfile", "empty-service"] {
        File::create(scratch.path().join(file))
            .map_err(|_| anyhow::anyhow!("postgres_catalog_private_directory_unavailable"))?;
    }
    fs::create_dir(scratch.path().join("empty-crls"))
        .map_err(|_| anyhow::anyhow!("postgres_catalog_private_directory_unavailable"))?;
    let mut version = command(spec, scratch.path());
    version.arg("--version");
    let output = capture(version, &[], 4096, d)?;
    ensure!(
        output.starts_with(b"psql (PostgreSQL) 18."),
        "postgres_catalog_client_version_unsupported"
    );
    let password: Option<Zeroizing<String>> = match &spec.security {
        Security::LoopbackTrust => None,
        Security::TlsScram {
            root_certificate_pem,
            ..
        } => {
            let mut root = File::create(scratch.path().join("root.pem"))
                .map_err(|_| anyhow::anyhow!("postgres_catalog_private_directory_unavailable"))?;
            root.write_all(root_certificate_pem.as_bytes())
                .map_err(|_| anyhow::anyhow!("postgres_catalog_private_directory_unavailable"))?;
            Some(
                crate::host_credentials::read_postgres_password(&spec.credential_binding()?)
                    .map_err(|_| anyhow::anyhow!("postgres_catalog_credential_unavailable"))?,
            )
        }
    };
    let mut client = command(spec, scratch.path());
    client
        .args([
            "-X",
            "-w",
            "-q",
            "-A",
            "-t",
            "--set=ON_ERROR_STOP=1",
            "--set=FETCH_COUNT=1",
            "--pset=pager=off",
            "--dbname",
        ])
        .arg(connection(spec, scratch.path())?);
    if let Some(password) = &password {
        client.env("PGPASSWORD", password.as_str());
    }
    let response = capture(
        client,
        limits.sql()?.as_bytes(),
        limits.max_response_bytes,
        d,
    )?;
    let mut result = Catalog::from_response_with_limits(&response, limits, d)?;
    let meta = &result.as_value()["meta"];
    ensure!(
        meta["database"] == spec.database
            && meta["user"] == spec.user
            && meta["tls"] == matches!(spec.security, Security::TlsScram { .. }),
        "postgres_catalog_endpoint_report_mismatch"
    );
    result.set_capture_claims(json!({"kind":"unverified_host_capture_claims","transport":"host_trusted_native_psql_18","clientExecutableHash":spec.client_sha256,
        "endpointBinding":crate::model::identity(&(spec.address,spec.port,&spec.database,&spec.user)),"security":if password.is_some(){"tls_verify_full_scram_channel_binding"}else{"explicit_literal_loopback_trust"},
        "queryHash":crate::model::hash(limits.sql()?.as_bytes()),"serverAttestation":"unverified","clientInternalMemoryBounded":false,"serverMemoryBounded":false,"runtimeLibrariesAttested":false}));
    catalog::check(d)?;
    Ok(result)
}
