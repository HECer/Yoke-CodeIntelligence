//! Native non-Windows credential storage. Never accepts source-selected names.
use anyhow::{Result, ensure};
use zeroize::Zeroizing;

const SERVICE: &str = "yoke-code-intelligence";
const CAPACITY: usize = 2560;

fn validate_target(target: &str) -> Result<()> {
    let name = target
        .strip_prefix("yoke-code-intelligence/federation/")
        .or_else(|| target.strip_prefix("yoke-code-intelligence/service/tls/"))
        .or_else(|| target.strip_prefix("yoke-code-intelligence/service/token/"))
        .or_else(|| target.strip_prefix("yoke-code-intelligence/service/postgres/"));
    ensure!(
        name.is_some_and(|name| !name.is_empty()
            && name.len() <= 80
            && name.as_bytes()[0].is_ascii_alphanumeric()
            && name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))),
        "Invalid Yoke credential target"
    );
    Ok(())
}

pub fn write_new(target: &str, bytes: &[u8]) -> Result<()> {
    validate_target(target)?;
    ensure!(
        (1..=CAPACITY).contains(&bytes.len()),
        "Yoke credential secret must contain 1-2560 bytes"
    );
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        let root = creation_lock::native_root()?;
        let _lock = creation_lock::CreationLock::acquire_at(
            &root,
            target,
            std::time::Duration::from_secs(5),
        )?;
        let secret = Zeroizing::new(bytes.to_vec());
        backend::write_new(target, &secret)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = SERVICE;
        anyhow::bail!("Native platform credential storage is unavailable on this platform")
    }
}

pub fn read(target: &str) -> Result<Zeroizing<Vec<u8>>> {
    validate_target(target)?;
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        let root = creation_lock::native_root()?;
        let _lock = creation_lock::CreationLock::acquire_at(
            &root,
            target,
            std::time::Duration::from_secs(5),
        )?;
        let secret = backend::read(target)?;
        ensure!(
            (1..=CAPACITY).contains(&secret.len()),
            "Native Yoke credential has an invalid secret size"
        );
        Ok(secret)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    anyhow::bail!("Native platform credential storage is unavailable on this platform")
}

#[cfg(target_os = "macos")]
mod backend {
    use super::*;
    use keyring::credential::CredentialApi;
    use security_framework::os::macos::keychain::{SecKeychain, SecPreferencesDomain};
    use std::{
        sync::{Mutex, MutexGuard, TryLockError},
        time::{Duration, Instant},
    };
    static INTERACTION_LOCK: Mutex<()> = Mutex::new(());
    #[link(name = "Security", kind = "framework")]
    unsafe extern "C" {
        fn SecKeychainGetUserInteractionAllowed(state: *mut u8) -> i32;
        fn SecKeychainSetUserInteractionAllowed(state: u8) -> i32;
    }
    struct NoInteraction {
        previous: u8,
        _lock: MutexGuard<'static, ()>,
    }
    impl NoInteraction {
        fn acquire() -> Result<Self> {
            let started = Instant::now();
            let lock = loop {
                match INTERACTION_LOCK.try_lock() {
                    Ok(lock) => break lock,
                    Err(TryLockError::WouldBlock) => {
                        ensure!(
                            started.elapsed() < Duration::from_secs(5),
                            "Native Keychain interaction guard timed out"
                        );
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => anyhow::bail!("Native Keychain interaction guard is unavailable"),
                }
            };
            let mut previous = 0u8;
            ensure!(
                unsafe { SecKeychainGetUserInteractionAllowed(&mut previous) } == 0
                    && unsafe { SecKeychainSetUserInteractionAllowed(0) } == 0,
                "Native Keychain noninteractive access is unavailable"
            );
            Ok(Self {
                previous,
                _lock: lock,
            })
        }
    }
    impl Drop for NoInteraction {
        fn drop(&mut self) {
            unsafe {
                SecKeychainSetUserInteractionAllowed(self.previous);
            }
        }
    }
    fn native(target: &str) -> Result<keyring::macos::MacCredential> {
        // Explicit concrete builder: a host library's global mock cannot replace this store.
        keyring::macos::MacCredential::new_with_target(Some("User"), SERVICE, target)
            .map_err(|_| anyhow::anyhow!("Native Keychain credential construction failed"))
    }
    pub(super) fn write_new(target: &str, bytes: &[u8]) -> Result<()> {
        let _interaction = NoInteraction::acquire()?;
        match native(target)?.get_secret() {
            Ok(secret) => {
                let _wipe = Zeroizing::new(secret);
                anyhow::bail!("Native Yoke credential already exists; refusing overwrite");
            }
            Err(keyring::Error::NoEntry) => {}
            Err(_) => anyhow::bail!("Native Keychain is unavailable, locked or access was refused"),
        }
        let keychain = SecKeychain::default_for_domain(SecPreferencesDomain::User)
            .map_err(|_| anyhow::anyhow!("Native user Keychain is unavailable"))?;
        // Native Add is atomic/create-only, whereas keyring set_secret updates existing entries.
        keychain
            .add_generic_password(SERVICE, target, bytes)
            .map_err(|error| {
                if error.code() == -25299 {
                    anyhow::anyhow!("Native Yoke credential already exists; refusing overwrite")
                } else {
                    anyhow::anyhow!("Native Keychain creation failed or access was refused")
                }
            })
    }
    pub(super) fn read(target: &str) -> Result<Zeroizing<Vec<u8>>> {
        let _interaction = NoInteraction::acquire()?;
        native(target)?
            .get_secret()
            .map(Zeroizing::new)
            .map_err(|error| {
                if matches!(error, keyring::Error::NoEntry) {
                    anyhow::anyhow!("Native Yoke credential was not found")
                } else {
                    anyhow::anyhow!("Native Keychain is unavailable, locked or access was refused")
                }
            })
    }
}

#[cfg(target_os = "linux")]
mod backend {
    use super::*;
    use secret_service::{EncryptionType, blocking::SecretService};
    use std::collections::HashMap;

    fn session_address() -> Result<()> {
        let address = std::env::var("DBUS_SESSION_BUS_ADDRESS").map_err(|_| {
            anyhow::anyhow!(
                "Existing native session bus is unavailable (headless credentials are unsupported)"
            )
        })?;
        ensure!(
            !address.is_empty()
                && address.len() <= 1024
                && address.is_ascii()
                && !address.bytes().any(|b| b.is_ascii_control())
                && !address.contains(';')
                && (address.starts_with("unix:path=/") || address.starts_with("unix:abstract=")),
            "Only one existing native Unix session bus is supported"
        );
        for field in address.strip_prefix("unix:").unwrap().split(',') {
            let (key, value) = field
                .split_once('=')
                .ok_or_else(|| anyhow::anyhow!("Invalid native session bus address"))?;
            ensure!(
                !value.is_empty() && matches!(key, "path" | "abstract" | "guid"),
                "Invalid native session bus address"
            );
        }
        Ok(())
    }
    fn connect() -> Result<SecretService<'static>> {
        session_address()?;
        let conn = zbus::blocking::Connection::session()
            .map_err(|_| anyhow::anyhow!("Existing native session bus is unavailable"))?;
        let bus = zbus::blocking::fdo::DBusProxy::new(&conn)
            .map_err(|_| anyhow::anyhow!("Existing native session bus is unavailable"))?;
        let name = zbus::names::BusName::try_from("org.freedesktop.secrets")
            .expect("fixed SecretService bus name");
        ensure!(
            bus.name_has_owner(name).unwrap_or(false),
            "Native SecretService is unavailable; Yoke does not start or provision it"
        );
        let service = SecretService::connect(EncryptionType::Dh).map_err(|_| {
            anyhow::anyhow!("Native encrypted SecretService session is unavailable")
        })?;
        let collection = service.get_default_collection().map_err(|_| {
            anyhow::anyhow!("Existing default SecretService collection is unavailable")
        })?;
        ensure!(
            !collection.is_locked().map_err(|_| anyhow::anyhow!(
                "Native SecretService collection inspection failed"
            ))?,
            "Native SecretService collection is locked; interactive unlocking is unsupported"
        );
        drop(collection);
        Ok(service)
    }
    fn credential(target: &str) -> Result<keyring::secret_service::SsCredential> {
        keyring::secret_service::SsCredential::new_with_target(None, SERVICE, target)
            .map_err(|_| anyhow::anyhow!("Native SecretService credential construction failed"))
    }
    fn attributes(credential: &keyring::secret_service::SsCredential) -> HashMap<&str, &str> {
        credential
            .attributes
            .iter()
            .filter(|(key, _)| matches!(key.as_str(), "service" | "username" | "target"))
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect()
    }
    pub(super) fn write_new(target: &str, bytes: &[u8]) -> Result<()> {
        let service = connect()?;
        let credential = credential(target)?;
        // An existing item is refused even when locked or its blob is invalid/empty.
        let found = service
            .search_items(attributes(&credential))
            .map_err(|_| anyhow::anyhow!("Native SecretService credential lookup failed"))?;
        ensure!(
            found.unlocked.is_empty() && found.locked.is_empty(),
            "Native Yoke credential already exists; refusing overwrite"
        );
        let collection = service.get_default_collection().map_err(|_| {
            anyhow::anyhow!("Existing default SecretService collection is unavailable")
        })?;
        ensure!(
            !collection.is_locked().map_err(|_| anyhow::anyhow!(
                "Native SecretService collection inspection failed"
            ))?,
            "Native SecretService collection is locked"
        );
        // Only create an item in the existing collection; never create or unlock a store.
        collection
            .create_item(
                "Yoke native host credential",
                attributes(&credential),
                bytes,
                false,
                "application/octet-stream",
            )
            .map_err(|_| anyhow::anyhow!("Native SecretService credential creation failed"))?;
        Ok(())
    }
    pub(super) fn read(target: &str) -> Result<Zeroizing<Vec<u8>>> {
        let service = connect()?;
        let credential = credential(target)?;
        let found = service
            .search_items(attributes(&credential))
            .map_err(|_| anyhow::anyhow!("Native SecretService credential lookup failed"))?;
        ensure!(
            found.locked.is_empty(),
            "Native Yoke credential is locked; interactive unlocking is unsupported"
        );
        ensure!(
            found.unlocked.len() == 1,
            "Native Yoke credential is missing or ambiguous"
        );
        found.unlocked[0]
            .get_secret()
            .map(Zeroizing::new)
            .map_err(|_| anyhow::anyhow!("Native SecretService credential read failed"))
    }
}

#[cfg(any(target_os = "macos", target_os = "linux", test))]
mod creation_lock {
    use super::*;
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{Duration, Instant},
    };

    pub(super) struct CreationLock(fs::File);
    impl Drop for CreationLock {
        fn drop(&mut self) {
            let _ = fs2::FileExt::unlock(&self.0);
        }
    }

    fn no_link(meta: &fs::Metadata) -> bool {
        if meta.file_type().is_symlink() {
            return false;
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if meta.file_attributes() & 0x400 != 0 {
                return false;
            }
        }
        true
    }

    fn verify_directory(path: &Path, private: bool) -> Result<()> {
        ensure!(
            path.is_absolute(),
            "Credential lock directory must be absolute"
        );
        let mut part = PathBuf::new();
        for component in path.components() {
            ensure!(
                !matches!(
                    component,
                    std::path::Component::ParentDir | std::path::Component::CurDir
                ),
                "Noncanonical credential lock directory"
            );
            part.push(component.as_os_str());
            // A Windows drive prefix alone is not a directory yet.
            if matches!(component, std::path::Component::Prefix(_)) {
                continue;
            }
            let meta = fs::symlink_metadata(&part)
                .map_err(|_| anyhow::anyhow!("Credential lock directory is unavailable"))?;
            ensure!(
                no_link(&meta) && meta.is_dir(),
                "Linked or non-directory credential lock path refused"
            );
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let meta = fs::symlink_metadata(path)
                .map_err(|_| anyhow::anyhow!("Credential lock directory is unavailable"))?;
            ensure!(
                meta.uid() == unsafe { libc::geteuid() }
                    && meta.mode() & if private { 0o077 } else { 0o022 } == 0,
                "Credential lock directory must be owned by the current user and private"
            );
        }
        #[cfg(not(unix))]
        let _ = private;
        Ok(())
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    pub(super) fn native_root() -> Result<PathBuf> {
        use std::{ffi::CStr, os::unix::ffi::OsStrExt, os::unix::fs::DirBuilderExt};
        // OS account database, never source-selected HOME/XDG variables.
        let uid = unsafe { libc::getuid() };
        ensure!(
            uid == unsafe { libc::geteuid() },
            "Privileged credential execution is refused"
        );
        let mut storage = vec![0u8; 16_384];
        let mut record: libc::passwd = unsafe { std::mem::zeroed() };
        let mut found = std::ptr::null_mut();
        let status = unsafe {
            libc::getpwuid_r(
                uid,
                &mut record,
                storage.as_mut_ptr().cast(),
                storage.len(),
                &mut found,
            )
        };
        ensure!(
            status == 0 && !found.is_null() && !record.pw_dir.is_null(),
            "Current OS user home is unavailable"
        );
        let home = PathBuf::from(std::ffi::OsStr::from_bytes(
            unsafe { CStr::from_ptr(record.pw_dir) }.to_bytes(),
        ));
        verify_directory(&home, false)?;
        let root = home.join(".yoke-credential-locks");
        match fs::DirBuilder::new().mode(0o700).create(&root) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(_) => anyhow::bail!("Credential lock directory creation failed"),
        }
        verify_directory(&root, true)?;
        Ok(root)
    }

    fn check_file(meta: &fs::Metadata) -> Result<()> {
        ensure!(
            no_link(meta) && meta.is_file() && meta.len() == 0,
            "Credential lock must be an empty regular file"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            ensure!(
                meta.uid() == unsafe { libc::geteuid() }
                    && meta.mode() & 0o077 == 0
                    && meta.nlink() == 1,
                "Public, foreign or multiply-linked credential lock refused"
            );
        }
        Ok(())
    }

    impl CreationLock {
        pub(super) fn acquire_at(root: &Path, target: &str, timeout: Duration) -> Result<Self> {
            use sha2::{Digest, Sha256};
            validate_target(target)?;
            ensure!(
                !timeout.is_zero() && timeout <= Duration::from_secs(5),
                "Invalid credential creation lock timeout"
            );
            verify_directory(root, true)?;
            let name = format!(
                "{}.lock",
                hex::encode(Sha256::digest(target.to_ascii_lowercase().as_bytes()))
            );
            let path = root.join(&name);
            if let Ok(meta) = fs::symlink_metadata(&path) {
                check_file(&meta)?;
            }
            #[cfg(unix)]
            let file = {
                use std::{
                    ffi::CString,
                    os::fd::{AsRawFd, FromRawFd},
                    os::unix::fs::{MetadataExt, OpenOptionsExt},
                };
                let directory = fs::OpenOptions::new()
                    .read(true)
                    .custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY | libc::O_CLOEXEC)
                    .open(root)
                    .map_err(|_| anyhow::anyhow!("Credential lock directory open failed"))?;
                let path_meta = fs::symlink_metadata(root)
                    .map_err(|_| anyhow::anyhow!("Credential lock directory changed"))?;
                let handle_meta = directory
                    .metadata()
                    .map_err(|_| anyhow::anyhow!("Credential lock directory inspection failed"))?;
                ensure!(
                    path_meta.dev() == handle_meta.dev() && path_meta.ino() == handle_meta.ino(),
                    "Credential lock directory changed"
                );
                let name = CString::new(name).expect("hex lock name");
                let descriptor = unsafe {
                    libc::openat(
                        directory.as_raw_fd(),
                        name.as_ptr(),
                        libc::O_CREAT | libc::O_RDWR | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                        0o600,
                    )
                };
                ensure!(descriptor >= 0, "Credential lock file open failed");
                unsafe { fs::File::from_raw_fd(descriptor) }
            };
            #[cfg(not(unix))]
            let file = fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(&path)
                .map_err(|_| anyhow::anyhow!("Credential lock file open failed"))?;
            let started = Instant::now();
            loop {
                match fs2::FileExt::try_lock_exclusive(&file) {
                    Ok(()) => break,
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            || error.raw_os_error()
                                == fs2::lock_contended_error().raw_os_error() =>
                    {
                        ensure!(
                            started.elapsed() < timeout,
                            "Credential creation lock timed out"
                        );
                        std::thread::sleep(
                            timeout
                                .saturating_sub(started.elapsed())
                                .min(Duration::from_millis(5)),
                        );
                    }
                    Err(_) => anyhow::bail!("Credential creation lock failed"),
                }
            }
            let held = Self(file);
            verify_directory(root, true)?;
            let meta = held
                .0
                .metadata()
                .map_err(|_| anyhow::anyhow!("Credential lock inspection failed"))?;
            check_file(&meta)?;
            let path_meta = fs::symlink_metadata(&path)
                .map_err(|_| anyhow::anyhow!("Credential lock path changed"))?;
            check_file(&path_meta)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                ensure!(
                    meta.ino() == path_meta.ino() && meta.dev() == path_meta.dev(),
                    "Credential lock path changed"
                );
            }
            // Never delete lock files: unlinking a held inode breaks cross-process exclusion.
            Ok(held)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::{fs, time::Instant};
        const TARGET: &str = "yoke-code-intelligence/service/token/lock-test";

        fn private_temp() -> tempfile::TempDir {
            let root = tempfile::tempdir().unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
            }
            root
        }

        #[test]
        fn creation_locks_serialize_and_time_out_without_secret_contents() {
            let root = private_temp();
            let root_path = fs::canonicalize(root.path()).unwrap();
            let held =
                CreationLock::acquire_at(&root_path, TARGET, Duration::from_secs(1)).unwrap();
            let started = Instant::now();
            assert!(
                CreationLock::acquire_at(&root_path, TARGET, Duration::from_millis(40)).is_err()
            );
            assert!(started.elapsed() < Duration::from_secs(1));
            let files = fs::read_dir(root.path())
                .unwrap()
                .collect::<std::io::Result<Vec<_>>>()
                .unwrap();
            assert_eq!(files.len(), 1);
            assert_eq!(fs::metadata(files[0].path()).unwrap().len(), 0);
            drop(held);
            assert!(fs::read(files[0].path()).unwrap().is_empty());
            assert!(CreationLock::acquire_at(&root_path, TARGET, Duration::from_secs(1)).is_ok());
        }

        #[test]
        fn contaminated_and_nonfile_locks_are_refused_without_repair() {
            let root = private_temp();
            let root_path = fs::canonicalize(root.path()).unwrap();
            drop(CreationLock::acquire_at(&root_path, TARGET, Duration::from_secs(1)).unwrap());
            let path = fs::read_dir(root.path())
                .unwrap()
                .next()
                .unwrap()
                .unwrap()
                .path();
            fs::write(&path, b"synthetic-not-a-real-secret").unwrap();
            assert!(CreationLock::acquire_at(&root_path, TARGET, Duration::from_secs(1)).is_err());
            assert_eq!(fs::read(&path).unwrap(), b"synthetic-not-a-real-secret");
            fs::remove_file(&path).unwrap();
            fs::create_dir(&path).unwrap();
            assert!(CreationLock::acquire_at(&root_path, TARGET, Duration::from_secs(1)).is_err());
            assert!(path.is_dir());
        }

        #[cfg(unix)]
        #[test]
        fn linked_or_public_creation_paths_are_refused() {
            use std::os::unix::fs::{PermissionsExt, symlink};
            let parent = tempfile::tempdir().unwrap();
            let real = fs::canonicalize(parent.path()).unwrap().join("real");
            fs::create_dir(&real).unwrap();
            fs::set_permissions(&real, fs::Permissions::from_mode(0o700)).unwrap();
            let link = parent.path().join("link");
            symlink(&real, &link).unwrap();
            assert!(CreationLock::acquire_at(&link, TARGET, Duration::from_secs(1)).is_err());
            drop(CreationLock::acquire_at(&real, TARGET, Duration::from_secs(1)).unwrap());
            let path = fs::read_dir(&real).unwrap().next().unwrap().unwrap().path();
            let copy = parent.path().join("hardlink");
            fs::hard_link(&path, &copy).unwrap();
            assert!(CreationLock::acquire_at(&real, TARGET, Duration::from_secs(1)).is_err());
            fs::remove_file(&copy).unwrap();
            fs::set_permissions(&real, fs::Permissions::from_mode(0o777)).unwrap();
            assert!(CreationLock::acquire_at(&real, TARGET, Duration::from_secs(1)).is_err());
        }
    }
}
