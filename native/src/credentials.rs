//! Explicit host-only federation key storage; never a source or MCP capability.
use anyhow::{Result, ensure};
use serde_json::Value;

pub(crate) fn target_name(name: &str) -> Result<String> {
    ensure!(
        !name.is_empty()
            && name.len() <= 80
            && name.as_bytes()[0].is_ascii_alphanumeric()
            && name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.')),
        "Invalid federation key name: use 1-80 ASCII letters, digits, '_', '-' or '.', starting with a letter or digit"
    );
    Ok(format!("yoke-code-intelligence/federation/{name}"))
}

/// Create a new per-user OS credential, returning only public key metadata.
/// An existing entry is an error, including an entry with an invalid blob.
pub fn generate(name: &str) -> Result<Value> {
    let target = target_name(name)?;
    #[cfg(windows)]
    {
        windows::generate(name, &target)
    }
    #[cfg(not(windows))]
    {
        let _ = target;
        anyhow::bail!("Federation OS credential storage is unavailable on this platform")
    }
}

/// Read a private seed into memory for an explicit host signing operation.
/// The caller must zeroize the returned array after use and must never log it.
pub fn read(name: &str) -> Result<[u8; 32]> {
    let target = target_name(name)?;
    #[cfg(windows)]
    {
        windows::read(&target)
    }
    #[cfg(not(windows))]
    {
        let _ = target;
        anyhow::bail!("Federation OS credential storage is unavailable on this platform")
    }
}

#[cfg(windows)]
mod windows {
    use anyhow::{Result, anyhow, bail, ensure};
    use ed25519_dalek::SigningKey;
    use serde_json::{Value, json};
    use sha2::{Digest, Sha256};
    use std::{ffi::c_void, ptr};
    use zeroize::{Zeroize, Zeroizing};

    const GENERIC: u32 = 1;
    const PERSIST_LOCAL_MACHINE: u32 = 2;
    const NOT_FOUND: i32 = 1168;
    const MAX_CREDENTIAL_BLOB: usize = 5 * 512;

    // Layout and signatures follow the installed Windows SDK wincred.h.
    #[repr(C)]
    struct FileTime {
        low: u32,
        high: u32,
    }
    #[repr(C)]
    struct CredentialW {
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
        fn CredWriteW(credential: *const CredentialW, flags: u32) -> i32;
        fn CredReadW(
            target: *const u16,
            kind: u32,
            flags: u32,
            credential: *mut *mut CredentialW,
        ) -> i32;
        fn CredFree(buffer: *mut c_void);
    }
    #[link(name = "Kernel32")]
    unsafe extern "system" {
        fn CreateMutexW(attributes: *const c_void, owner: i32, name: *const u16) -> *mut c_void;
        fn WaitForSingleObject(handle: *mut c_void, milliseconds: u32) -> u32;
        fn ReleaseMutex(handle: *mut c_void) -> i32;
        fn CloseHandle(handle: *mut c_void) -> i32;
    }

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(Some(0)).collect()
    }
    fn os_error(operation: &str) -> anyhow::Error {
        // Numeric OS errors only: never format an input or a credential buffer.
        let code = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
        anyhow!("{operation} failed (Windows error {code})")
    }

    struct CredentialBuffer(*mut CredentialW);
    impl Drop for CredentialBuffer {
        fn drop(&mut self) {
            // CredReadW owns one OS allocation. Its blob is writable until CredFree.
            unsafe {
                let credential = &mut *self.0;
                let length = credential.blob_size as usize;
                if !credential.blob.is_null() && length <= MAX_CREDENTIAL_BLOB {
                    std::slice::from_raw_parts_mut(credential.blob, length).zeroize();
                }
                CredFree(self.0.cast());
            }
        }
    }
    fn credential(target: &[u16]) -> Result<Option<CredentialBuffer>> {
        let mut value = ptr::null_mut();
        // The validated, terminated target always belongs to our fixed namespace.
        if unsafe { CredReadW(target.as_ptr(), GENERIC, 0, &mut value) } == 0 {
            let code = std::io::Error::last_os_error().raw_os_error();
            if code == Some(NOT_FOUND) {
                return Ok(None);
            }
            return Err(os_error("Federation credential read"));
        }
        ensure!(
            !value.is_null(),
            "Federation credential read returned an invalid buffer"
        );
        Ok(Some(CredentialBuffer(value)))
    }

    struct CreationLock(*mut c_void);
    impl CreationLock {
        fn acquire(target: &str) -> Result<Self> {
            // Generic target matching is case-insensitive. A machine-wide mutex also
            // serializes this helper across logon sessions; it contains no key material.
            let digest = hex::encode(Sha256::digest(target.to_ascii_lowercase().as_bytes()));
            let name = wide(&format!("Global\\YokeFederationCreate-{digest}"));
            let handle = unsafe { CreateMutexW(ptr::null(), 0, name.as_ptr()) };
            ensure!(
                !handle.is_null(),
                "{}",
                os_error("Federation credential creation lock")
            );
            match unsafe { WaitForSingleObject(handle, 5_000) } {
                0 | 0x80 => Ok(Self(handle)), // acquired or abandoned: always recheck the entry
                0x102 => {
                    unsafe {
                        CloseHandle(handle);
                    }
                    bail!("Federation credential creation lock timed out")
                }
                _ => {
                    let error = os_error("Federation credential creation lock wait");
                    unsafe {
                        CloseHandle(handle);
                    }
                    Err(error)
                }
            }
        }
    }
    impl Drop for CreationLock {
        fn drop(&mut self) {
            unsafe {
                ReleaseMutex(self.0);
                CloseHandle(self.0);
            }
        }
    }

    pub(super) fn generate(name: &str, target: &str) -> Result<Value> {
        let _lock = CreationLock::acquire(target)?;
        let mut target = wide(target);
        ensure!(
            credential(&target)?.is_none(),
            "Federation key already exists; refusing overwrite"
        );
        let mut seed = Zeroizing::new([0_u8; 32]);
        getrandom::fill(&mut *seed)
            .map_err(|_| anyhow!("Federation key randomness is unavailable"))?;
        let public_key = hex::encode(SigningKey::from_bytes(&seed).verifying_key().to_bytes());
        let value = CredentialW {
            flags: 0,
            kind: GENERIC,
            target: target.as_mut_ptr(),
            comment: ptr::null_mut(),
            last_written: FileTime { low: 0, high: 0 },
            blob_size: 32,
            blob: seed.as_mut_ptr(),
            persist: PERSIST_LOCAL_MACHINE,
            attribute_count: 0,
            attributes: ptr::null_mut(),
            target_alias: ptr::null_mut(),
            username: ptr::null_mut(),
        };
        ensure!(
            unsafe { CredWriteW(&value, 0) } != 0,
            "{}",
            os_error("Federation credential write")
        );
        Ok(json!({"keyName":name,"publicKey":public_key}))
    }

    pub(super) fn read(target: &str) -> Result<[u8; 32]> {
        let target = wide(target);
        let buffer = credential(&target)?.ok_or_else(|| anyhow!("Federation key was not found"))?;
        let value = unsafe { &*buffer.0 };
        ensure!(
            value.kind == GENERIC && value.blob_size == 32 && !value.blob.is_null(),
            "Federation credential has an invalid seed format"
        );
        let mut seed = Zeroizing::new([0_u8; 32]);
        // Validated exact-length OS blob; both this temporary and the OS copy are wiped.
        seed.copy_from_slice(unsafe { std::slice::from_raw_parts(value.blob, 32) });
        Ok(*seed)
    }
}
