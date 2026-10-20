//! Host-only service credentials. Secret values are never returned as JSON.
#[path = "aws_session_credentials.rs"]
mod aws_session;
#[cfg(windows)]
use anyhow::Context;
use anyhow::{Result, ensure};
pub use aws_session::{import_aws_session, import_aws_session_json, read_aws_session};
use base64::Engine as _;
use serde_json::{Value, json};
use zeroize::Zeroizing;
pub struct TlsIdentity {
    pub certificate: Vec<u8>,
    pub private_key: Zeroizing<Vec<u8>>,
}
fn target(kind: &str, name: &str) -> Result<String> {
    ensure!(
        !name.is_empty()
            && name.len() <= 80
            && name.as_bytes()[0].is_ascii_alphanumeric()
            && name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.')),
        "Invalid Yoke host credential name"
    );
    Ok(format!("yoke-code-intelligence/service/{kind}/{name}"))
}
pub fn generate_tls(name: &str, hostnames: &[String]) -> Result<Value> {
    let target = target("tls", name)?;
    ensure!(
        !hostnames.is_empty()
            && hostnames.len() <= 8
            && hostnames.iter().all(|n| !n.is_empty()
                && n.len() <= 200
                && n.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b':'))),
        "Supply 1-8 explicit DNS names or IP addresses"
    );
    let mut params = rcgen::CertificateParams::new(hostnames.to_vec())?;
    let now = time::OffsetDateTime::now_utc();
    params.not_before = now - time::Duration::minutes(5);
    params.not_after = now + time::Duration::days(365);
    let expires = params.not_after.to_string();
    let key = rcgen::KeyPair::generate()?;
    let certificate = params.self_signed(&key)?;
    let secret = Zeroizing::new(key.serialize_der());
    ensure!(
        certificate.der().len() <= 2000 && secret.len() <= 512,
        "TLS identity exceeds host credential capacity"
    );
    let mut payload = Zeroizing::new(b"YTLS1".to_vec());
    payload.extend_from_slice(&u16::try_from(certificate.der().len())?.to_be_bytes());
    payload.extend_from_slice(certificate.der());
    payload.extend_from_slice(&secret);
    write_new(&target, &payload)?;
    Ok(
        json!({"credentialName":name,"certificatePem":certificate.pem(),"certificateDerBase64":base64::prelude::BASE64_STANDARD.encode(certificate.der()),"certificateHash":digest(certificate.der()),"hostnames":hostnames,"expiresAt":expires,"privateMaterial":"os_credential_store_only"}),
    )
}
pub fn read_tls(name: &str) -> Result<TlsIdentity> {
    let bytes = read(&target("tls", name)?)?;
    ensure!(
        bytes.len() >= 8 && &bytes[..5] == b"YTLS1",
        "Invalid TLS credential encoding"
    );
    let length = u16::from_be_bytes([bytes[5], bytes[6]]) as usize;
    ensure!(
        (100..=2000).contains(&length)
            && bytes.len() > 7 + length
            && bytes.len() <= 7 + length + 512,
        "Invalid TLS identity length"
    );
    Ok(TlsIdentity {
        certificate: bytes[7..7 + length].to_vec(),
        private_key: Zeroizing::new(bytes[7 + length..].to_vec()),
    })
}
pub fn generate_token(name: &str) -> Result<Value> {
    let target = target("token", name)?;
    let mut token = Zeroizing::new(vec![0u8; 32]);
    getrandom::fill(&mut token)
        .map_err(|_| anyhow::anyhow!("Service token randomness unavailable"))?;
    write_new(&target, &token)?;
    Ok(
        json!({"credentialName":name,"tokenHash":digest(&token),"privateMaterial":"os_credential_store_only"}),
    )
}
pub fn read_token(name: &str) -> Result<Zeroizing<Vec<u8>>> {
    let bytes = read(&target("token", name)?)?;
    ensure!(bytes.len() == 32, "Invalid service token format");
    Ok(bytes)
}
fn postgres_password(bytes: &[u8]) -> Result<&str> {
    ensure!(
        (1..=2560).contains(&bytes.len()),
        "Invalid PostgreSQL credential capacity"
    );
    let text = std::str::from_utf8(bytes)
        .map_err(|_| anyhow::anyhow!("Invalid PostgreSQL credential format"))?;
    ensure!(
        !text.chars().any(char::is_control),
        "Invalid PostgreSQL credential format"
    );
    Ok(text)
}
/// Create-only host import, scoped to a validated endpoint/security binding.
pub fn import_postgres_password(binding: &str, bytes: &[u8]) -> Result<Value> {
    ensure!(
        crate::model::valid_id(binding),
        "Invalid PostgreSQL credential binding"
    );
    postgres_password(bytes)?;
    write_new(&target("postgres", binding)?, bytes)?;
    Ok(
        json!({"credentialBinding":binding,"credentialType":"postgres","privateMaterial":"os_credential_store_only"}),
    )
}
pub(crate) fn read_postgres_password(binding: &str) -> Result<Zeroizing<String>> {
    ensure!(
        crate::model::valid_id(binding),
        "Invalid PostgreSQL credential binding"
    );
    let bytes = read(&target("postgres", binding)?)?;
    Ok(Zeroizing::new(postgres_password(&bytes)?.to_owned()))
}
fn neo4j_password(bytes: &[u8]) -> Result<&str> {
    ensure!(
        (1..=2560).contains(&bytes.len()),
        "Invalid Neo4j credential capacity"
    );
    let text = std::str::from_utf8(bytes)
        .map_err(|_| anyhow::anyhow!("Invalid Neo4j credential format"))?;
    ensure!(
        !text.chars().any(char::is_control),
        "Invalid Neo4j credential format"
    );
    Ok(text)
}
/// Create-only host import. The binding includes endpoint, pinned address and TLS policy.
pub fn import_neo4j_password(
    binding: &str,
    bytes: &[u8],
    deadline: &crate::capture::Deadline,
) -> Result<Value> {
    deadline.check()?;
    ensure!(
        cfg!(windows),
        "neo4j_publish_credential_platform_unavailable"
    );
    ensure!(
        crate::model::valid_id(binding),
        "Invalid Neo4j credential binding"
    );
    neo4j_password(bytes)?;
    #[cfg(windows)]
    windows::write_new_bounded(&target("neo4j", binding)?, bytes, deadline)?;
    Ok(
        json!({"credentialBinding":binding,"credentialType":"neo4j","privateMaterial":"os_credential_store_only"}),
    )
}
pub(crate) fn read_neo4j_password(
    binding: &str,
    deadline: &crate::capture::Deadline,
) -> Result<Zeroizing<String>> {
    deadline.check()?;
    ensure!(
        cfg!(windows),
        "neo4j_publish_credential_platform_unavailable"
    );
    ensure!(
        crate::model::valid_id(binding),
        "Invalid Neo4j credential binding"
    );
    let bytes = read(&target("neo4j", binding)?)?;
    deadline.check()?;
    Ok(Zeroizing::new(neo4j_password(&bytes)?.to_owned()))
}
fn falkor_password(bytes: &[u8]) -> Result<&str> {
    ensure!(
        (1..=2560).contains(&bytes.len()),
        "Invalid FalkorDB credential capacity"
    );
    let text = std::str::from_utf8(bytes)
        .map_err(|_| anyhow::anyhow!("Invalid FalkorDB credential format"))?;
    ensure!(
        !text.chars().any(char::is_control),
        "Invalid FalkorDB credential format"
    );
    Ok(text)
}
/// Create-only host import bound to the endpoint, user, pinned address and TLS policy.
pub fn import_falkor_password(
    binding: &str,
    bytes: &[u8],
    deadline: &crate::capture::Deadline,
) -> Result<Value> {
    deadline.check()?;
    ensure!(
        cfg!(windows),
        "falkor_publish_credential_platform_unavailable"
    );
    ensure!(
        crate::model::valid_id(binding),
        "Invalid FalkorDB credential binding"
    );
    falkor_password(bytes)?;
    #[cfg(windows)]
    windows::write_new_bounded(&target("falkor", binding)?, bytes, deadline)?;
    Ok(
        json!({"credentialBinding":binding,"credentialType":"falkor","privateMaterial":"os_credential_store_only"}),
    )
}
pub(crate) fn read_falkor_password(
    binding: &str,
    deadline: &crate::capture::Deadline,
) -> Result<Zeroizing<String>> {
    deadline.check()?;
    ensure!(
        cfg!(windows),
        "falkor_publish_credential_platform_unavailable"
    );
    ensure!(
        crate::model::valid_id(binding),
        "Invalid FalkorDB credential binding"
    );
    let bytes = read(&target("falkor", binding)?)?;
    deadline.check()?;
    Ok(Zeroizing::new(falkor_password(&bytes)?.to_owned()))
}
fn github_token_bytes(bytes: &[u8]) -> Result<&str> {
    ensure!(
        !bytes.is_empty()
            && bytes.len() <= 2560
            && bytes.iter().all(|b| b.is_ascii_alphanumeric()
                || matches!(b, b'_' | b'-' | b'.' | b'+' | b'/' | b'=')),
        "Invalid GitHub credential format or capacity"
    );
    Ok(std::str::from_utf8(bytes)?)
}
/// Host-only import. Secret bytes never enter a serialized result or source file.
pub fn import_github_token(name: &str, bytes: &[u8]) -> Result<Value> {
    let target = target("github", name)?;
    github_token_bytes(bytes)?;
    write_new(&target, bytes)?;
    Ok(
        json!({"credentialName":name,"credentialType":"github","privateMaterial":"os_credential_store_only"}),
    )
}
pub fn read_github_token(name: &str) -> Result<Zeroizing<String>> {
    let bytes = read(&target("github", name)?)?;
    Ok(Zeroizing::new(github_token_bytes(&bytes)?.to_owned()))
}
/// Exact private stdin bytes in a separate namespace; never a bearer credential.
pub fn import_github_webhook_secret(name: &str, bytes: &[u8]) -> Result<Value> {
    let target = target("github-webhook", name)?;
    crate::github_app::validate_secret(bytes)?;
    write_new(&target, bytes)?;
    Ok(
        json!({"credentialName":name,"credentialType":"github-webhook",
        "privateMaterial":"os_credential_store_only"}),
    )
}
pub fn read_github_webhook_secret(name: &str) -> Result<Zeroizing<Vec<u8>>> {
    let bytes = read(&target("github-webhook", name)?)?;
    crate::github_app::validate_secret(&bytes)?;
    Ok(bytes)
}
/// Decode and validate PKCS1/PKCS8 before a create-only, distinct OS-store write.
pub fn import_github_app_key(name: &str, bytes: &[u8]) -> Result<Value> {
    let target = target("github-app-key", name)?;
    let der = crate::github_app_auth::key::decoded_key(bytes)?;
    write_new(&target, &der)?;
    Ok(
        json!({"credentialName":name,"credentialType":"github-app-key",
        "privateMaterial":"os_credential_store_only"}),
    )
}
pub fn read_github_app_key(name: &str) -> Result<crate::github_app_auth::AppKey> {
    let bytes = read(&target("github-app-key", name)?)?;
    crate::github_app_auth::AppKey::from_host_bytes(&bytes)
}
fn model_token_bytes(bytes: &[u8]) -> Result<&str> {
    ensure!(
        !bytes.is_empty() && bytes.len() <= 2560 && bytes.iter().all(u8::is_ascii_graphic),
        "Invalid model credential format or capacity"
    );
    Ok(std::str::from_utf8(bytes)?)
}
/// Import a header-safe opaque key into the explicitly named OS credential slot.
pub fn import_model_token(name: &str, bytes: &[u8]) -> Result<Value> {
    let target = target("model", name)?;
    model_token_bytes(bytes)?;
    write_new(&target, bytes)?;
    Ok(
        json!({"credentialName":name,"credentialType":"model","privateMaterial":"os_credential_store_only"}),
    )
}
pub fn read_model_token(name: &str) -> Result<Zeroizing<String>> {
    let bytes = read(&target("model", name)?)?;
    Ok(Zeroizing::new(model_token_bytes(&bytes)?.to_owned()))
}
fn digest(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(bytes))
}
fn write_new(target: &str, bytes: &[u8]) -> Result<()> {
    #[cfg(windows)]
    {
        windows::write_new(target, bytes)
    }
    #[cfg(not(windows))]
    {
        crate::platform_credentials::write_new(target, bytes)
    }
}
fn read(target: &str) -> Result<Zeroizing<Vec<u8>>> {
    #[cfg(windows)]
    {
        windows::read(target)
    }
    #[cfg(not(windows))]
    {
        crate::platform_credentials::read(target)
    }
}
#[cfg(windows)]
mod windows {
    use super::*;
    use std::{ffi::c_void, ptr};
    use zeroize::Zeroize;
    #[repr(C)]
    struct FileTime {
        low: u32,
        high: u32,
    }
    #[repr(C)]
    struct Credential {
        flags: u32,
        kind: u32,
        target: *mut u16,
        comment: *mut u16,
        last_written: FileTime,
        blob_size: u32,
        blob: *mut u8,
        persist: u32,
        attribute_count: u32,
        attributes: *mut c_void,
        target_alias: *mut u16,
        username: *mut u16,
    }
    #[link(name = "Advapi32")]
    unsafe extern "system" {
        fn CredReadW(target: *const u16, kind: u32, flags: u32, value: *mut *mut Credential)
        -> i32;
        fn CredWriteW(value: *const Credential, flags: u32) -> i32;
        fn CredFree(buffer: *mut c_void);
    }
    #[link(name = "Kernel32")]
    unsafe extern "system" {
        fn CreateMutexW(attributes: *const c_void, owner: i32, name: *const u16) -> *mut c_void;
        fn WaitForSingleObject(handle: *mut c_void, ms: u32) -> u32;
        fn ReleaseMutex(handle: *mut c_void) -> i32;
        fn CloseHandle(handle: *mut c_void) -> i32;
    }
    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }
    struct Buffer(*mut Credential);
    impl Drop for Buffer {
        fn drop(&mut self) {
            unsafe {
                let value = &mut *self.0;
                if !value.blob.is_null() && value.blob_size <= 2560 {
                    std::slice::from_raw_parts_mut(value.blob, value.blob_size as usize).zeroize();
                }
                CredFree(self.0.cast());
            }
        }
    }
    fn lookup(target: &str) -> Result<Option<Buffer>> {
        let target = wide(target);
        let mut value = ptr::null_mut();
        if unsafe { CredReadW(target.as_ptr(), 1, 0, &mut value) } == 0 {
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() == Some(1168) {
                return Ok(None);
            }
            return Err(error).context("Host credential read failed");
        }
        ensure!(!value.is_null(), "Invalid host credential buffer");
        Ok(Some(Buffer(value)))
    }
    struct Lock(*mut c_void);
    impl Drop for Lock {
        fn drop(&mut self) {
            unsafe {
                ReleaseMutex(self.0);
                CloseHandle(self.0);
            }
        }
    }
    fn lock_for(target: &str, milliseconds: u32) -> Result<Lock> {
        let name = wide(&format!(
            "Global\\YokeServiceCreate-{}",
            digest(target.to_ascii_lowercase().as_bytes())
        ));
        let handle = unsafe { CreateMutexW(ptr::null(), 0, name.as_ptr()) };
        ensure!(
            !handle.is_null(),
            "Host credential creation lock unavailable"
        );
        match unsafe { WaitForSingleObject(handle, milliseconds) } {
            0 | 0x80 => Ok(Lock(handle)),
            _ => {
                unsafe {
                    CloseHandle(handle);
                }
                anyhow::bail!("Host credential creation lock unavailable or timed out")
            }
        }
    }
    pub(super) fn write_new(target: &str, bytes: &[u8]) -> Result<()> {
        write_new_bounded(target, bytes, &crate::capture::Deadline::new(5000))
    }
    pub(super) fn exists(target: &str) -> Result<bool> {
        Ok(lookup(target)?.is_some())
    }
    pub(super) fn write_new_bounded(
        target: &str,
        bytes: &[u8],
        deadline: &crate::capture::Deadline,
    ) -> Result<()> {
        deadline.check()?;
        ensure!(
            !bytes.is_empty() && bytes.len() <= 2560,
            "Host credential capacity exceeded"
        );
        let wait = deadline.remaining().as_millis().min(5000) as u32;
        let _lock = lock_for(target, wait)?;
        deadline.check()?;
        ensure!(
            lookup(target)?.is_none(),
            "Host credential already exists; refusing overwrite"
        );
        let mut name = wide(target);
        let mut secret = Zeroizing::new(bytes.to_vec());
        let value = Credential {
            flags: 0,
            kind: 1,
            target: name.as_mut_ptr(),
            comment: ptr::null_mut(),
            last_written: FileTime { low: 0, high: 0 },
            blob_size: secret.len() as u32,
            blob: secret.as_mut_ptr(),
            persist: 2,
            attribute_count: 0,
            attributes: ptr::null_mut(),
            target_alias: ptr::null_mut(),
            username: ptr::null_mut(),
        };
        if unsafe { CredWriteW(&value, 0) } == 0 {
            return Err(std::io::Error::last_os_error()).context("Host credential write failed");
        }
        Ok(())
    }
    pub(super) fn read(target: &str) -> Result<Zeroizing<Vec<u8>>> {
        let buffer = lookup(target)?.context("Host credential not found")?;
        let value = unsafe { &*buffer.0 };
        ensure!(
            value.kind == 1 && !value.blob.is_null() && (1..=2560).contains(&value.blob_size),
            "Invalid host credential type or capacity"
        );
        Ok(Zeroizing::new(
            unsafe { std::slice::from_raw_parts(value.blob, value.blob_size as usize) }.to_vec(),
        ))
    }
}
