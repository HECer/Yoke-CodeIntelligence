//! Windows handle guards and create-only namespace operations for host installation.
use crate::capture::Deadline;
use anyhow::Result;
use std::{
    fs::File,
    path::{Path, PathBuf},
};

#[cfg(windows)]
#[path = "host_install_namespace.rs"]
mod namespace;

pub(super) struct GuardedDirectory {
    pub(super) path: PathBuf,
    pub(super) identity: String,
    _handles: Vec<File>,
}

pub(super) struct HeldFile {
    pub(super) path: PathBuf,
    pub(super) identity: String,
    pub(super) hash: String,
    pub(super) bytes: Vec<u8>,
    pub(super) byte_length: usize,
    _handle: File,
    _parent: GuardedDirectory,
    _delete_access: bool,
    _writable: bool,
    _executable_binding: bool,
}

pub(super) struct OutputBinding {
    #[cfg(all(test, windows))]
    pub(super) path: PathBuf,
    pub(super) identity: String,
    #[cfg(all(test, windows))]
    pub(super) byte_length: usize,
    pub(super) security_hash: String,
}

/// An empty output whose creation handle still owns deletion on close.
pub(super) struct ArmedOutput {
    file: HeldFile,
    security_hash: String,
}

/// A durably registered output; bytes are admitted only through canonical appends.
pub(super) struct RegisteredOutput {
    #[cfg(windows)]
    file: HeldFile,
    #[cfg(windows)]
    after_hash: String,
    #[cfg(windows)]
    security_hash: String,
}

pub(super) fn allocate_output(
    path: &Path,
    original: Option<&HeldFile>,
    d: &Deadline,
) -> Result<ArmedOutput> {
    #[cfg(windows)]
    return windows::allocate_output(path, original, d);
    #[cfg(not(windows))]
    {
        let _ = (path, original);
        d.check()?;
        anyhow::bail!("Staged host-output allocation is Windows-only")
    }
}

pub(super) fn open_registered_output(
    path: &Path,
    exact_id: &str,
    original: Option<&HeldFile>,
    canonical: &[u8],
    require_empty: bool,
    registration: &super::DurableOutputRegistration,
    d: &Deadline,
) -> Result<Option<RegisteredOutput>> {
    #[cfg(windows)]
    return windows::open_registered_output(
        path,
        exact_id,
        original,
        canonical,
        require_empty,
        registration,
        d,
    );
    #[cfg(not(windows))]
    {
        let _ = (
            path,
            exact_id,
            original,
            canonical,
            require_empty,
            registration,
        );
        d.check()?;
        anyhow::bail!("Registered host-output reopening is Windows-only")
    }
}

impl ArmedOutput {
    pub(super) fn binding(&self) -> OutputBinding {
        OutputBinding {
            #[cfg(all(test, windows))]
            path: self.file.path.clone(),
            identity: self.file.identity.clone(),
            #[cfg(all(test, windows))]
            byte_length: self.file.byte_length,
            security_hash: self.security_hash.clone(),
        }
    }

    #[cfg(windows)]
    pub(super) fn identity(&self) -> &str {
        &self.file.identity
    }
    #[cfg(windows)]
    pub(super) fn len(&self) -> usize {
        self.file.byte_length
    }
    #[cfg(windows)]
    pub(super) fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub(super) fn retain(
        self,
        registration: &super::DurableOutputRegistration,
        d: &Deadline,
    ) -> Result<RegisteredOutput> {
        #[cfg(windows)]
        return windows::retain_output(self, registration, d);
        #[cfg(not(windows))]
        {
            let _ = registration;
            d.check()?;
            anyhow::bail!("Durable host-output retention is Windows-only")
        }
    }
}

impl RegisteredOutput {
    #[cfg(all(test, windows))]
    pub(super) fn binding(&self) -> OutputBinding {
        OutputBinding {
            path: self.file.path.clone(),
            identity: self.file.identity.clone(),
            byte_length: self.file.byte_length,
            security_hash: self.security_hash.clone(),
        }
    }

    #[cfg(all(test, windows))]
    pub(super) fn identity(&self) -> &str {
        &self.file.identity
    }
    #[cfg(windows)]
    pub(super) fn len(&self) -> usize {
        self.file.byte_length
    }
    #[cfg(windows)]
    pub(super) fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub(super) fn append_chunk(&mut self, canonical: &[u8], d: &Deadline) -> Result<usize> {
        #[cfg(windows)]
        return windows::append_registered(self, canonical, d);
        #[cfg(not(windows))]
        {
            let _ = canonical;
            d.check()?;
            anyhow::bail!("Registered host-output canonical append is Windows-only")
        }
    }

    pub(super) fn finalize(self, canonical: &[u8], d: &Deadline) -> Result<HeldFile> {
        #[cfg(windows)]
        return windows::finalize_registered(self, canonical, d);
        #[cfg(not(windows))]
        {
            let _ = canonical;
            d.check()?;
            anyhow::bail!("Registered host-output finalization is Windows-only")
        }
    }

    pub(super) fn discard(self, canonical: &[u8], d: &Deadline) -> Result<()> {
        #[cfg(windows)]
        return windows::discard_registered(self, canonical, d);
        #[cfg(not(windows))]
        {
            let _ = canonical;
            d.check()?;
            anyhow::bail!("Registered host-output discard is Windows-only")
        }
    }
}

pub(super) fn security_fingerprint(file: &HeldFile, d: &Deadline) -> Result<String> {
    #[cfg(windows)]
    return windows::security_fingerprint(file, d);
    #[cfg(not(windows))]
    {
        let _ = file;
        d.check()?;
        anyhow::bail!("Host-install guarded security inspection is Windows-only")
    }
}

pub(super) struct EntryInfo {
    pub(super) directory: bool,
    pub(super) length: u64,
}

/// One physically held metadata directory for a single read/scan operation.
pub(super) struct MetadataDirectory {
    _directory: GuardedDirectory,
}

pub(super) fn open_metadata_directory(path: &Path, d: &Deadline) -> Result<MetadataDirectory> {
    #[cfg(windows)]
    return Ok(MetadataDirectory {
        _directory: windows::guarded_directory(path, d)?,
    });
    #[cfg(not(windows))]
    {
        let _ = path;
        d.check()?;
        anyhow::bail!("Host metadata-directory reading is Windows-only")
    }
}

impl MetadataDirectory {
    pub(super) fn names(&self, max_entries: usize, d: &Deadline) -> Result<Vec<String>> {
        #[cfg(windows)]
        return windows::metadata_names(&self._directory, max_entries, d);
        #[cfg(not(windows))]
        {
            let _ = max_entries;
            d.check()?;
            anyhow::bail!("Host metadata-directory reading is Windows-only")
        }
    }

    pub(super) fn entry_info(&self, leaf: &str, d: &Deadline) -> Result<Option<EntryInfo>> {
        #[cfg(windows)]
        return windows::metadata_entry_info(&self._directory, leaf, d);
        #[cfg(not(windows))]
        {
            let _ = leaf;
            d.check()?;
            anyhow::bail!("Host metadata-directory reading is Windows-only")
        }
    }

    pub(super) fn read_bytes(
        &self,
        leaf: &str,
        max_bytes: usize,
        d: &Deadline,
    ) -> Result<Option<Vec<u8>>> {
        #[cfg(windows)]
        return windows::metadata_read_bytes(&self._directory, leaf, max_bytes, d);
        #[cfg(not(windows))]
        {
            let _ = (leaf, max_bytes);
            d.check()?;
            anyhow::bail!("Host metadata-directory reading is Windows-only")
        }
    }
}

pub(super) fn entry_info(path: &Path, d: &Deadline) -> Result<Option<EntryInfo>> {
    #[cfg(windows)]
    return windows::entry_info(path, d);
    #[cfg(not(windows))]
    {
        let _ = (path, d);
        anyhow::bail!("Host-install guarded filesystem backend is Windows-only")
    }
}

pub(super) fn create_directory(path: &Path, d: &Deadline) -> Result<GuardedDirectory> {
    #[cfg(windows)]
    return windows::create_directory(path, d);
    #[cfg(not(windows))]
    {
        let _ = (path, d);
        anyhow::bail!("Host-install guarded filesystem backend is Windows-only")
    }
}

#[cfg(all(test, windows))]
pub(super) fn directory_entries(
    path: &Path,
    max_entries: usize,
    d: &Deadline,
) -> Result<Vec<PathBuf>> {
    #[cfg(windows)]
    return windows::directory_entries(path, max_entries, d);
    #[cfg(not(windows))]
    {
        let _ = (path, max_entries, d);
        anyhow::bail!("Host-install guarded filesystem backend is Windows-only")
    }
}

pub(super) fn open_state_lock(path: &Path, d: &Deadline) -> Result<File> {
    #[cfg(windows)]
    return windows::open_state_lock(path, d);
    #[cfg(not(windows))]
    {
        let _ = (path, d);
        anyhow::bail!("Host-install guarded filesystem backend is Windows-only")
    }
}

pub(super) fn guarded_directory(path: &Path, d: &Deadline) -> Result<GuardedDirectory> {
    #[cfg(windows)]
    return windows::guarded_directory(path, d);
    #[cfg(not(windows))]
    {
        let _ = (path, d);
        anyhow::bail!("Host-install guarded filesystem backend is Windows-only")
    }
}

pub(super) fn read_file(
    path: &Path,
    max_bytes: usize,
    delete_access: bool,
    d: &Deadline,
) -> Result<Option<HeldFile>> {
    #[cfg(windows)]
    return windows::read_file(path, max_bytes, delete_access, d);
    #[cfg(not(windows))]
    {
        let _ = (path, max_bytes, delete_access, d);
        anyhow::bail!("Host-install guarded filesystem backend is Windows-only")
    }
}

pub(super) fn prepare_file(
    path: &Path,
    original: Option<&HeldFile>,
    bytes: &[u8],
    d: &Deadline,
) -> Result<HeldFile> {
    #[cfg(windows)]
    return windows::prepare_file(path, original, bytes, d);
    #[cfg(not(windows))]
    {
        let _ = (path, original, bytes, d);
        anyhow::bail!("Host-install guarded filesystem backend is Windows-only")
    }
}

pub(super) fn read_executable_binding(
    path: &Path,
    max_bytes: usize,
    d: &Deadline,
) -> Result<Option<HeldFile>> {
    #[cfg(windows)]
    return windows::read_executable_binding(path, max_bytes, d);
    #[cfg(not(windows))]
    {
        let _ = (path, max_bytes, d);
        anyhow::bail!("Host-install guarded filesystem backend is Windows-only")
    }
}

pub(super) fn verify_state_lock_handle(handle: &File, d: &Deadline) -> Result<()> {
    #[cfg(windows)]
    return windows::verify_state_lock_handle(handle, d);
    #[cfg(not(windows))]
    {
        let _ = (handle, d);
        anyhow::bail!("Host-install guarded filesystem backend is Windows-only")
    }
}

impl HeldFile {
    pub(super) fn rename_no_replace(&mut self, destination: &Path, d: &Deadline) -> Result<()> {
        #[cfg(windows)]
        return windows::rename_no_replace(self, destination, d);
        #[cfg(not(windows))]
        {
            let _ = (destination, d);
            anyhow::bail!("Host-install guarded filesystem backend is Windows-only")
        }
    }

    pub(super) fn delete_owned(self, d: &Deadline) -> Result<()> {
        #[cfg(windows)]
        return windows::delete_owned(self, d);
        #[cfg(not(windows))]
        {
            let _ = d;
            anyhow::bail!("Host-install guarded filesystem backend is Windows-only")
        }
    }

    pub(super) fn sync_all(&self, d: &Deadline) -> Result<()> {
        d.check()?;
        anyhow::ensure!(self._writable, "Cannot flush a read-only host guard");
        self._handle.sync_all()?;
        d.check()
    }
}

#[cfg(windows)]
mod windows {
    use super::namespace::{Disposition, EntryKind, OpenRequest};
    use super::*;
    use anyhow::{Context, ensure};
    use sha2::{Digest, Sha256};
    use std::{
        ffi::{OsStr, c_void},
        fs::OpenOptions,
        io::{Read, Seek, SeekFrom, Write},
        mem::{offset_of, size_of},
        os::windows::{ffi::OsStrExt, fs::OpenOptionsExt, io::AsRawHandle},
        path::{Component, Prefix},
        ptr,
    };

    const GENERIC_READ: u32 = 0x8000_0000;
    const GENERIC_WRITE: u32 = 0x4000_0000;
    const DELETE: u32 = 0x0001_0000;
    const WRITE_DAC: u32 = 0x0004_0000;
    const SHARE_READ: u32 = 1;
    const FILE_TRAVERSE: u32 = 0x20;
    const FILE_READ_ATTRIBUTES: u32 = 0x80;
    const OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const ATTRIBUTE_DIRECTORY: u32 = 0x10;
    const ATTRIBUTE_READONLY: u32 = 1;
    const ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    const DACL_INFORMATION: u32 = 4;
    const DACL_CONTROL: u16 = 0x150c;

    #[cfg(test)]
    type NamespaceHook = Box<dyn FnOnce() -> Result<()>>;

    #[cfg(test)]
    std::thread_local! {
        static BEFORE_NAMESPACE_OPEN: std::cell::RefCell<Option<NamespaceHook>> =
            std::cell::RefCell::new(None);
        static AFTER_OUTPUT_RETAIN: std::cell::RefCell<Option<Box<dyn FnOnce()>>> =
            std::cell::RefCell::new(None);
    }

    #[cfg(test)]
    pub(super) fn set_namespace_hook(hook: Option<NamespaceHook>) {
        BEFORE_NAMESPACE_OPEN.with(|slot| *slot.borrow_mut() = hook);
    }

    #[cfg(test)]
    pub(super) fn set_output_retain_hook(hook: Option<Box<dyn FnOnce()>>) {
        AFTER_OUTPUT_RETAIN.with(|slot| *slot.borrow_mut() = hook);
    }

    #[cfg(test)]
    fn run_output_retain_hook() {
        if let Some(hook) = AFTER_OUTPUT_RETAIN.with(|slot| slot.borrow_mut().take()) {
            hook();
        }
    }

    #[cfg(test)]
    fn run_namespace_hook() -> Result<()> {
        let hook = BEFORE_NAMESPACE_OPEN.with(|slot| slot.borrow_mut().take());
        match hook {
            Some(hook) => hook(),
            None => Ok(()),
        }
    }

    #[repr(C)]
    #[derive(Default)]
    struct FileInformation {
        attributes: u32,
        created: [u32; 2],
        accessed: [u32; 2],
        written: [u32; 2],
        volume: u32,
        size_high: u32,
        size_low: u32,
        links: u32,
        index_high: u32,
        index_low: u32,
    }
    #[repr(C)]
    #[derive(Default)]
    struct FileIdInformation {
        volume: u64,
        id: [u8; 16],
    }
    #[repr(C)]
    struct RenameInformation {
        flags: u32,
        root_directory: *mut c_void,
        name_length: u32,
        name: [u16; 1],
    }
    #[repr(C)]
    struct IoStatusBlock {
        status_or_pointer: usize,
        information: usize,
    }
    #[link(name = "Kernel32")]
    unsafe extern "system" {
        fn GetFileInformationByHandle(
            handle: *mut c_void,
            information: *mut FileInformation,
        ) -> i32;
        fn GetFileInformationByHandleEx(
            handle: *mut c_void,
            class: i32,
            information: *mut c_void,
            size: u32,
        ) -> i32;
        fn SetFileInformationByHandle(
            handle: *mut c_void,
            class: i32,
            information: *const c_void,
            size: u32,
        ) -> i32;
        fn LocalAlloc(flags: u32, bytes: usize) -> *mut c_void;
        fn LocalFree(memory: *mut c_void) -> *mut c_void;
        fn WaitForSingleObject(handle: *mut c_void, milliseconds: u32) -> u32;
    }
    #[link(name = "ntdll")]
    unsafe extern "system" {
        fn NtSetInformationFile(
            handle: *mut c_void,
            status: *mut IoStatusBlock,
            information: *const c_void,
            length: u32,
            class: i32,
        ) -> i32;
        fn RtlNtStatusToDosError(status: i32) -> u32;
    }
    #[link(name = "Advapi32")]
    unsafe extern "system" {
        fn GetKernelObjectSecurity(
            handle: *mut c_void,
            information: u32,
            descriptor: *mut c_void,
            length: u32,
            needed: *mut u32,
        ) -> i32;
        fn GetSecurityInfo(
            handle: *mut c_void,
            object_type: i32,
            information: u32,
            owner: *mut *mut c_void,
            group: *mut *mut c_void,
            dacl: *mut *mut c_void,
            sacl: *mut *mut c_void,
            descriptor: *mut *mut c_void,
        ) -> u32;
        fn GetSecurityDescriptorControl(
            descriptor: *mut c_void,
            control: *mut u16,
            revision: *mut u32,
        ) -> i32;
        fn GetSecurityDescriptorDacl(
            descriptor: *mut c_void,
            present: *mut i32,
            dacl: *mut *mut c_void,
            defaulted: *mut i32,
        ) -> i32;
        fn GetSecurityDescriptorSacl(
            descriptor: *mut c_void,
            present: *mut i32,
            sacl: *mut *mut c_void,
            defaulted: *mut i32,
        ) -> i32;
        fn GetSecurityDescriptorLength(descriptor: *mut c_void) -> u32;
        fn GetSecurityDescriptorOwner(
            descriptor: *mut c_void,
            owner: *mut *mut c_void,
            defaulted: *mut i32,
        ) -> i32;
        fn GetSecurityDescriptorGroup(
            descriptor: *mut c_void,
            group: *mut *mut c_void,
            defaulted: *mut i32,
        ) -> i32;
        fn GetLengthSid(sid: *mut c_void) -> u32;
        fn SetSecurityInfo(
            handle: *mut c_void,
            object_type: i32,
            information: u32,
            owner: *mut c_void,
            group: *mut c_void,
            dacl: *mut c_void,
            sacl: *mut c_void,
        ) -> u32;
    }

    fn volume_prefix(name: &std::ffi::OsStr) -> bool {
        let Some(text) = name.to_str() else {
            return false;
        };
        let Some(guid) = text
            .strip_prefix("Volume{")
            .and_then(|s| s.strip_suffix('}'))
        else {
            return false;
        };
        guid.len() == 36
            && guid.bytes().enumerate().all(|(index, byte)| {
                if matches!(index, 8 | 13 | 18 | 23) {
                    byte == b'-'
                } else {
                    byte.is_ascii_hexdigit()
                }
            })
    }

    fn checked_path(path: &Path) -> Result<()> {
        ensure!(path.is_absolute(), "Host-install path must be absolute");
        let text = path.to_str().context("Host-install path must be UTF-8")?;
        ensure!(!text.contains('\0'), "Host-install path contains NUL");
        let mut components = path.components();
        ensure!(
            matches!(components.next(), Some(Component::Prefix(p)) if match p.kind() {
                Prefix::Disk(_) | Prefix::VerbatimDisk(_) => true,
                Prefix::Verbatim(name) => volume_prefix(name),
                _ => false,
            }),
            "Host-install UNC/device path refused"
        );
        ensure!(
            matches!(components.next(), Some(Component::RootDir)),
            "Host-install path root refused"
        );
        for component in components {
            let Component::Normal(name) = component else {
                anyhow::bail!("Host-install path traversal refused");
            };
            let name = name.to_str().context("Host-install path must be UTF-8")?;
            crate::paths::normalized_syntax(name)?;
        }
        Ok(())
    }

    fn information(handle: &File, directory: bool) -> Result<(String, FileInformation)> {
        information_with_links(handle, directory, false)
    }

    fn information_with_links(
        handle: &File,
        directory: bool,
        executable: bool,
    ) -> Result<(String, FileInformation)> {
        let mut information = FileInformation::default();
        // Both output layouts match the installed Windows SDK structures.
        ensure!(
            unsafe { GetFileInformationByHandle(handle.as_raw_handle(), &mut information) } != 0,
            "Host file information unavailable: {}",
            std::io::Error::last_os_error()
        );
        ensure!(
            information.attributes & ATTRIBUTE_REPARSE_POINT == 0,
            "Host-install reparse point refused"
        );
        ensure!(
            (information.attributes & ATTRIBUTE_DIRECTORY != 0) == directory,
            "Host-install file type changed"
        );
        if !directory && !executable {
            ensure!(
                information.links == 1,
                "Host-install multiply-linked file refused"
            );
        }
        let mut id = FileIdInformation::default();
        ensure!(
            unsafe {
                GetFileInformationByHandleEx(
                    handle.as_raw_handle(),
                    18,
                    ptr::from_mut(&mut id).cast(),
                    size_of::<FileIdInformation>() as u32,
                )
            } != 0,
            "Host file identity unavailable: {}",
            std::io::Error::last_os_error()
        );
        Ok((
            format!("{:016x}:{}", id.volume, hex::encode(id.id)),
            information,
        ))
    }

    pub(super) fn verify_state_lock_handle(handle: &File, d: &Deadline) -> Result<()> {
        d.check()?;
        let (_, info) = information(handle, false)?;
        ensure!(
            info.size_high == 0 && info.size_low == 0,
            "Host writer lock must be empty"
        );
        d.check()
    }

    pub(super) fn guarded_directory(path: &Path, d: &Deadline) -> Result<GuardedDirectory> {
        d.check()?;
        checked_path(path)?;
        let mut components = path.components();
        let mut anchor = PathBuf::new();
        anchor.push(
            components
                .next()
                .context("Host volume prefix missing")?
                .as_os_str(),
        );
        anchor.push(
            components
                .next()
                .context("Host volume root missing")?
                .as_os_str(),
        );
        // Initial ordinary drive input is bound once; subsequent internal paths
        // use the handle-derived Volume{GUID} anchor rather than a drive alias.
        let root = OpenOptions::new()
            .read(true)
            .access_mode(GENERIC_READ | FILE_TRAVERSE)
            .share_mode(SHARE_READ)
            .custom_flags(OPEN_REPARSE_POINT | BACKUP_SEMANTICS)
            .open(&anchor)
            .context("Cannot guard host volume anchor")?;
        let mut identity = information(&root, true)?.0;
        let mut handles = vec![root];
        for component in components {
            let Component::Normal(leaf) = component else {
                anyhow::bail!("Host directory traversal refused");
            };
            d.check()?;
            let opened = namespace::open_at(
                handles.last().context("Host ancestor handle missing")?,
                leaf,
                OpenRequest {
                    kind: EntryKind::Directory,
                    disposition: Disposition::Open,
                    access: GENERIC_READ | FILE_TRAVERSE,
                    share: SHARE_READ,
                },
                d,
            )
            .context("Cannot guard host directory ancestor")?;
            identity = information(&opened.file, true)?.0;
            handles.push(opened.file);
        }
        // This queries the held object; it never traverses the pathname again.
        let canonical =
            namespace::final_path(handles.last().context("Host directory handle missing")?, d)?;
        checked_path(&canonical)?;
        d.check()?;
        Ok(GuardedDirectory {
            path: canonical,
            identity,
            _handles: handles,
        })
    }

    fn parent_handle(directory: &GuardedDirectory) -> Result<&File> {
        directory
            ._handles
            .last()
            .context("Host directory handle missing")
    }

    fn missing(error: &anyhow::Error) -> bool {
        error
            .downcast_ref::<std::io::Error>()
            .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound)
    }

    fn metadata_leaf(leaf: &str) -> Result<&OsStr> {
        ensure!(
            !leaf.contains(['/', '\\']),
            "Host metadata operation requires one leaf"
        );
        crate::paths::normalized_syntax(leaf)?;
        let mut components = Path::new(leaf).components();
        ensure!(
            matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none(),
            "Host metadata operation requires one normal component"
        );
        ensure!(
            leaf.encode_utf16().count() <= 255,
            "Host metadata leaf exceeds the Windows component limit"
        );
        Ok(OsStr::new(leaf))
    }

    pub(super) fn metadata_names(
        directory: &GuardedDirectory,
        max_entries: usize,
        d: &Deadline,
    ) -> Result<Vec<String>> {
        d.check()?;
        let name_limit = max_entries
            .checked_mul(510)
            .context("Host directory name budget overflow")?;
        let entries = namespace::list(parent_handle(directory)?, max_entries, name_limit, d)?;
        let mut names = Vec::new();
        let mut observed_length = 0u64;
        for entry in entries {
            d.check()?;
            let leaf = entry
                .name
                .to_str()
                .context("Host entry name must be UTF-8")?;
            metadata_leaf(leaf)?;
            ensure!(
                entry.attributes & ATTRIBUTE_REPARSE_POINT == 0,
                "Host directory reparse entry refused"
            );
            // Enumeration attributes can lag an open reparse writer. Check
            // the current physical child before accepting its name.
            metadata_entry_info(directory, leaf, d)?.context("Host metadata entry disappeared")?;
            // Preserve the existing enumeration overflow guard. These observed
            // lengths do not replace either caller's fresh per-entry stat.
            observed_length = observed_length
                .checked_add(entry.byte_length)
                .context("Host directory byte length overflow")?;
            names.push(leaf.to_owned());
        }
        names.sort();
        d.check()?;
        Ok(names)
    }

    pub(super) fn metadata_entry_info(
        directory: &GuardedDirectory,
        leaf: &str,
        d: &Deadline,
    ) -> Result<Option<EntryInfo>> {
        d.check()?;
        let leaf = metadata_leaf(leaf)?;
        let opened = match namespace::open_at(
            parent_handle(directory)?,
            leaf,
            OpenRequest {
                kind: EntryKind::Any,
                disposition: Disposition::Open,
                access: FILE_READ_ATTRIBUTES,
                share: 7,
            },
            d,
        ) {
            Ok(opened) => opened,
            Err(error) if missing(&error) => {
                d.check()?;
                return Ok(None);
            }
            Err(error) => return Err(error).context("Cannot inspect held host metadata entry"),
        };
        let metadata = opened
            .file
            .metadata()
            .context("Cannot query held host metadata entry attributes")?;
        ensure!(
            metadata.is_file() || metadata.is_dir(),
            "Host entry type refused"
        );
        let (_, info) = information(&opened.file, metadata.is_dir())?;
        d.check()?;
        Ok(Some(EntryInfo {
            directory: info.attributes & ATTRIBUTE_DIRECTORY != 0,
            length: u64::from(info.size_high) << 32 | u64::from(info.size_low),
        }))
    }

    pub(super) fn metadata_read_bytes(
        directory: &GuardedDirectory,
        leaf: &str,
        max_bytes: usize,
        d: &Deadline,
    ) -> Result<Option<Vec<u8>>> {
        d.check()?;
        let leaf = metadata_leaf(leaf)?;
        let opened = match namespace::open_at(
            parent_handle(directory)?,
            leaf,
            OpenRequest {
                kind: EntryKind::File,
                disposition: Disposition::Open,
                access: GENERIC_READ,
                share: SHARE_READ,
            },
            d,
        ) {
            Ok(opened) => opened,
            Err(error) if missing(&error) => {
                d.check()?;
                return Ok(None);
            }
            Err(error) => return Err(error).context("Cannot read held host metadata entry"),
        };
        let (identity, info) = information(&opened.file, false)?;
        let length = u64::from(info.size_high) << 32 | u64::from(info.size_low);
        ensure!(length <= max_bytes as u64, "Host file byte limit exceeded");
        let content = bytes(&opened.file, max_bytes, d)?;
        let (current_identity, current_info) = information(&opened.file, false)?;
        let current_length =
            u64::from(current_info.size_high) << 32 | u64::from(current_info.size_low);
        ensure!(
            content.len() as u64 == length
                && current_length == length
                && current_identity == identity,
            "Host metadata file changed while acquiring snapshot"
        );
        d.check()?;
        Ok(Some(content))
    }

    fn parent_and_path(path: &Path, d: &Deadline) -> Result<(GuardedDirectory, PathBuf)> {
        checked_path(path)?;
        let parent = guarded_directory(path.parent().context("Host file has no parent")?, d)?;
        let path = parent
            .path
            .join(path.file_name().context("Host file name missing")?);
        Ok((parent, path))
    }

    pub(super) fn entry_info(path: &Path, d: &Deadline) -> Result<Option<EntryInfo>> {
        d.check()?;
        let (parent, path) = match parent_and_path(path, d) {
            Ok(value) => value,
            Err(error) if missing(&error) => return Ok(None),
            Err(error) => return Err(error),
        };
        let opened = match namespace::open_at(
            parent_handle(&parent)?,
            path.file_name().context("Host entry leaf missing")?,
            OpenRequest {
                kind: EntryKind::Any,
                disposition: Disposition::Open,
                access: FILE_READ_ATTRIBUTES,
                share: 7,
            },
            d,
        ) {
            Ok(opened) => opened,
            Err(error) if missing(&error) => return Ok(None),
            Err(error) => return Err(error).context("Cannot inspect anchored host entry"),
        };
        let metadata = opened
            .file
            .metadata()
            .context("Cannot query anchored host entry attributes")?;
        ensure!(
            metadata.is_file() || metadata.is_dir(),
            "Host entry type refused"
        );
        let (_, info) = information(&opened.file, metadata.is_dir())?;
        d.check()?;
        Ok(Some(EntryInfo {
            directory: info.attributes & ATTRIBUTE_DIRECTORY != 0,
            length: u64::from(info.size_high) << 32 | u64::from(info.size_low),
        }))
    }

    pub(super) fn create_directory(path: &Path, d: &Deadline) -> Result<GuardedDirectory> {
        d.check()?;
        let (mut parent, path) = parent_and_path(path, d)?;
        let opened = namespace::open_at(
            parent_handle(&parent)?,
            path.file_name().context("Host directory leaf missing")?,
            OpenRequest {
                kind: EntryKind::Directory,
                disposition: Disposition::Create,
                access: GENERIC_READ | FILE_TRAVERSE,
                share: SHARE_READ,
            },
            d,
        )
        .context("Cannot create anchored host directory exclusively")?;
        ensure!(opened.created, "Host directory was not created exclusively");
        let identity = information(&opened.file, true)?.0;
        let canonical = namespace::final_path(&opened.file, d)?;
        checked_path(&canonical)?;
        parent._handles.push(opened.file);
        parent.path = canonical;
        parent.identity = identity;
        d.check()?;
        Ok(parent)
    }

    #[cfg(test)]
    pub(super) fn directory_entries(
        path: &Path,
        max_entries: usize,
        d: &Deadline,
    ) -> Result<Vec<PathBuf>> {
        let directory = guarded_directory(path, d)?;
        let name_limit = max_entries
            .checked_mul(510)
            .context("Host directory name budget overflow")?;
        let entries = namespace::list(parent_handle(&directory)?, max_entries, name_limit, d)?;
        let mut paths = Vec::new();
        let mut observed_length = 0u64;
        for entry in entries {
            d.check()?;
            let leaf = entry
                .name
                .to_str()
                .context("Host entry name must be UTF-8")?;
            crate::paths::normalized_syntax(leaf)?;
            ensure!(
                entry.attributes & ATTRIBUTE_REPARSE_POINT == 0,
                "Host directory reparse entry refused"
            );
            observed_length = observed_length
                .checked_add(entry.byte_length)
                .context("Host directory byte length overflow")?;
            paths.push(directory.path.join(&entry.name));
        }
        paths.sort();
        d.check()?;
        Ok(paths)
    }

    pub(super) fn open_state_lock(path: &Path, d: &Deadline) -> Result<File> {
        let (parent, path) = parent_and_path(path, d)?;
        let opened = namespace::open_at(
            parent_handle(&parent)?,
            path.file_name().context("Host lock leaf missing")?,
            OpenRequest {
                kind: EntryKind::File,
                disposition: Disposition::OpenIf,
                access: GENERIC_READ | GENERIC_WRITE,
                share: 0,
            },
            d,
        )
        .context("Cannot acquire anchored host writer lock")?;
        verify_state_lock_handle(&opened.file, d)?;
        Ok(opened.file)
    }

    fn bytes(handle: &File, max_bytes: usize, d: &Deadline) -> Result<Vec<u8>> {
        let mut handle = handle;
        handle.seek(SeekFrom::Start(0))?;
        let mut result = Vec::new();
        let mut chunk = [0u8; 65_536];
        loop {
            d.check()?;
            let count = handle.read(&mut chunk)?;
            if count == 0 {
                break;
            }
            ensure!(
                count <= max_bytes.saturating_sub(result.len()),
                "Host file byte limit exceeded"
            );
            result.extend_from_slice(&chunk[..count]);
        }
        Ok(result)
    }

    pub(super) fn read_file(
        path: &Path,
        max_bytes: usize,
        delete_access: bool,
        d: &Deadline,
    ) -> Result<Option<HeldFile>> {
        d.check()?;
        let (parent, path) = parent_and_path(path, d)?;
        #[cfg(test)]
        run_namespace_hook()?;
        let handle = match namespace::open_at(
            parent_handle(&parent)?,
            path.file_name().context("Host file leaf missing")?,
            OpenRequest {
                kind: EntryKind::File,
                disposition: Disposition::Open,
                access: GENERIC_READ | if delete_access { DELETE } else { 0 },
                share: SHARE_READ,
            },
            d,
        ) {
            Ok(opened) => opened.file,
            Err(error) if missing(&error) => return Ok(None),
            Err(error) => return Err(error).context("Cannot acquire host file guard"),
        };
        let (identity, info) = information(&handle, false)?;
        ensure!(
            !delete_access || info.attributes & ATTRIBUTE_READONLY == 0,
            "Readonly host target mutation refused"
        );
        let length = u64::from(info.size_high) << 32 | u64::from(info.size_low);
        ensure!(length <= max_bytes as u64, "Host file byte limit exceeded");
        let bytes = bytes(&handle, max_bytes, d)?;
        ensure!(
            bytes.len() as u64 == length && information(&handle, false)?.0 == identity,
            "Host file changed while acquiring guard"
        );
        Ok(Some(HeldFile {
            path,
            identity,
            hash: crate::model::hash(&bytes),
            byte_length: bytes.len(),
            bytes,
            _handle: handle,
            _parent: parent,
            _delete_access: delete_access,
            _writable: false,
            _executable_binding: false,
        }))
    }

    pub(super) fn read_executable_binding(
        path: &Path,
        max_bytes: usize,
        d: &Deadline,
    ) -> Result<Option<HeldFile>> {
        d.check()?;
        let (parent, path) = parent_and_path(path, d)?;
        #[cfg(test)]
        run_namespace_hook()?;
        let mut handle = match namespace::open_at(
            parent_handle(&parent)?,
            path.file_name().context("Host executable leaf missing")?,
            OpenRequest {
                kind: EntryKind::File,
                disposition: Disposition::Open,
                access: GENERIC_READ,
                share: SHARE_READ,
            },
            d,
        ) {
            Ok(opened) => opened.file,
            Err(error) if missing(&error) => return Ok(None),
            Err(error) => return Err(error).context("Cannot acquire host executable guard"),
        };
        let (identity, info) = information_with_links(&handle, false, true)?;
        let expected = u64::from(info.size_high) << 32 | u64::from(info.size_low);
        ensure!(
            expected <= max_bytes as u64,
            "Host executable byte limit exceeded"
        );
        let mut digest = Sha256::new();
        let mut byte_length = 0usize;
        let mut chunk = [0u8; 65_536];
        loop {
            d.check()?;
            let count = handle.read(&mut chunk)?;
            if count == 0 {
                break;
            }
            ensure!(
                count <= max_bytes.saturating_sub(byte_length),
                "Host executable byte limit exceeded"
            );
            byte_length += count;
            digest.update(&chunk[..count]);
        }
        ensure!(
            byte_length as u64 == expected
                && information_with_links(&handle, false, true)?.0 == identity,
            "Host executable changed while acquiring guard"
        );
        Ok(Some(HeldFile {
            path,
            identity,
            hash: hex::encode(digest.finalize()),
            bytes: Vec::new(),
            byte_length,
            _handle: handle,
            _parent: parent,
            _delete_access: false,
            _writable: false,
            _executable_binding: true,
        }))
    }

    #[derive(PartialEq, Eq)]
    struct DaclState {
        control: u16,
        bytes: Option<Vec<u8>>,
        owner: Option<Vec<u8>>,
        group: Option<Vec<u8>>,
    }
    struct Descriptor(*mut c_void);
    impl Drop for Descriptor {
        fn drop(&mut self) {
            unsafe { LocalFree(self.0) };
        }
    }
    fn security(handle: &File) -> Result<(Descriptor, DaclState)> {
        // GetSecurityInfo may convert legacy explicit ACEs to INHERITED_ACE.
        // Query the stored self-relative descriptor through the pinned handle
        // instead, so copy/readback and durable fingerprints retain those flags.
        let mut needed = 0;
        let result = unsafe {
            GetKernelObjectSecurity(
                handle.as_raw_handle(),
                DACL_INFORMATION | 3,
                ptr::null_mut(),
                0,
                &mut needed,
            )
        };
        let error = std::io::Error::last_os_error();
        if result != 0 || error.raw_os_error() != Some(122) {
            return Err(error).context("Cannot read host owner/group/DACL");
        }
        ensure!(
            (20..=1_048_576).contains(&needed),
            "Host security descriptor size refused"
        );
        let capacity = needed;
        // LPTR gives suitably aligned, zero-initialized LocalFree-owned memory.
        let descriptor = unsafe { LocalAlloc(0x40, capacity as usize) };
        ensure!(
            !descriptor.is_null(),
            "Host security descriptor allocation failed"
        );
        let descriptor = Descriptor(descriptor);
        let result = unsafe {
            GetKernelObjectSecurity(
                handle.as_raw_handle(),
                DACL_INFORMATION | 3,
                descriptor.0,
                capacity,
                &mut needed,
            )
        };
        if result == 0 {
            return Err(std::io::Error::last_os_error())
                .context("Cannot read host owner/group/DACL");
        }
        ensure!(
            (20..=capacity).contains(&needed),
            "Host security descriptor changed size"
        );
        let size = unsafe { GetSecurityDescriptorLength(descriptor.0) } as usize;
        ensure!(
            (20..=needed as usize).contains(&size),
            "Host security descriptor size refused"
        );
        let (mut control, mut revision, mut present, mut defaulted, mut dacl) =
            (0, 0, 0, 0, ptr::null_mut());
        ensure!(
            unsafe { GetSecurityDescriptorControl(descriptor.0, &mut control, &mut revision) } != 0,
            "Cannot inspect host DACL control"
        );
        ensure!(
            unsafe {
                GetSecurityDescriptorDacl(descriptor.0, &mut present, &mut dacl, &mut defaulted)
            } != 0,
            "Cannot inspect host DACL"
        );
        let bytes = if present != 0 && !dacl.is_null() {
            let start = dacl as usize;
            let base = descriptor.0 as usize;
            ensure!(
                start >= base && start.checked_add(8).is_some_and(|end| end <= base + size),
                "Host DACL bounds refused"
            );
            let length =
                unsafe { ptr::read_unaligned(dacl.cast::<u8>().add(2).cast::<u16>()) } as usize;
            ensure!(
                length >= 8
                    && start
                        .checked_add(length)
                        .is_some_and(|end| end <= base + size),
                "Host DACL length refused"
            );
            Some(unsafe { std::slice::from_raw_parts(dacl.cast::<u8>(), length) }.to_vec())
        } else {
            None
        };
        let (mut owner, mut group) = (ptr::null_mut(), ptr::null_mut());
        ensure!(
            unsafe { GetSecurityDescriptorOwner(descriptor.0, &mut owner, &mut defaulted) } != 0,
            "Cannot inspect host file owner"
        );
        ensure!(
            unsafe { GetSecurityDescriptorGroup(descriptor.0, &mut group, &mut defaulted) } != 0,
            "Cannot inspect host file group"
        );
        let owner = sid_bytes(&descriptor, size, owner)?;
        let group = sid_bytes(&descriptor, size, group)?;
        Ok((
            descriptor,
            DaclState {
                control: control & DACL_CONTROL,
                bytes,
                owner,
                group,
            },
        ))
    }

    fn sid_bytes(
        descriptor: &Descriptor,
        size: usize,
        sid: *mut c_void,
    ) -> Result<Option<Vec<u8>>> {
        if sid.is_null() {
            return Ok(None);
        }
        let base = descriptor.0 as usize;
        let start = sid as usize;
        ensure!(
            start >= base && start.checked_add(8).is_some_and(|end| end <= base + size),
            "Host SID bounds refused"
        );
        let length = unsafe { GetLengthSid(sid) } as usize;
        ensure!(
            (8..=68).contains(&length)
                && start
                    .checked_add(length)
                    .is_some_and(|end| end <= base + size),
            "Host SID length refused"
        );
        Ok(Some(
            unsafe { std::slice::from_raw_parts(sid.cast::<u8>(), length) }.to_vec(),
        ))
    }

    #[derive(PartialEq, Eq)]
    struct LabelState {
        control: u16,
        bytes: Option<Vec<u8>>,
    }

    fn mandatory_label(handle: &File) -> Result<LabelState> {
        let mut raw = ptr::null_mut();
        // LABEL_SECURITY_INFORMATION needs READ_CONTROL, unlike a full audit SACL.
        let result = unsafe {
            GetSecurityInfo(
                handle.as_raw_handle(),
                1,
                0x10,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                &mut raw,
            )
        };
        if result != 0 {
            return Err(std::io::Error::from_raw_os_error(result as i32))
                .context("Unsupported original mandatory integrity label inspection");
        }
        ensure!(
            !raw.is_null(),
            "Mandatory integrity label descriptor missing"
        );
        let descriptor = Descriptor(raw);
        let size = unsafe { GetSecurityDescriptorLength(descriptor.0) } as usize;
        ensure!(
            (20..=1_048_576).contains(&size),
            "Mandatory integrity label descriptor size refused"
        );
        let (mut control, mut revision, mut present, mut defaulted, mut sacl) =
            (0, 0, 0, 0, ptr::null_mut());
        ensure!(
            unsafe { GetSecurityDescriptorControl(descriptor.0, &mut control, &mut revision) } != 0,
            "Cannot inspect mandatory integrity label control"
        );
        ensure!(
            unsafe {
                GetSecurityDescriptorSacl(descriptor.0, &mut present, &mut sacl, &mut defaulted)
            } != 0,
            "Cannot inspect mandatory integrity label"
        );
        let bytes = if present != 0 && !sacl.is_null() {
            let base = descriptor.0 as usize;
            let start = sacl as usize;
            ensure!(
                start >= base && start.checked_add(8).is_some_and(|end| end <= base + size),
                "Mandatory integrity label ACL bounds refused"
            );
            let length =
                unsafe { ptr::read_unaligned(sacl.cast::<u8>().add(2).cast::<u16>()) } as usize;
            ensure!(
                length >= 8
                    && start
                        .checked_add(length)
                        .is_some_and(|end| end <= base + size),
                "Mandatory integrity label ACL length refused"
            );
            Some(unsafe { std::slice::from_raw_parts(sacl.cast::<u8>(), length) }.to_vec())
        } else {
            None
        };
        Ok(LabelState {
            control: control & 0x2a30,
            bytes,
        })
    }

    fn handle_security_fingerprint(handle: &File) -> Result<String> {
        let (_, state) = security(handle)?;
        let label = mandatory_label(handle)?;
        Ok(crate::model::identity(&(
            "host-output-security-v1",
            state.owner,
            state.group,
            state.control,
            state.bytes,
            label.control,
            label.bytes,
        )))
    }

    pub(super) fn security_fingerprint(file: &HeldFile, d: &Deadline) -> Result<String> {
        ensure!(
            !file._executable_binding,
            "Executable binding is not a host output"
        );
        validate_current(file, d)?;
        let fingerprint = handle_security_fingerprint(&file._handle)?;
        d.check()?;
        Ok(fingerprint)
    }

    fn disposition(handle: &File) -> Result<()> {
        let delete: u8 = 1;
        // FILE_DISPOSITION_INFO contains exactly one BOOLEAN, not a Win32 BOOL.
        ensure!(
            unsafe {
                SetFileInformationByHandle(
                    handle.as_raw_handle(),
                    4,
                    ptr::from_ref(&delete).cast(),
                    1,
                )
            } != 0,
            "Owned host file deletion failed: {}",
            std::io::Error::last_os_error()
        );
        Ok(())
    }

    fn verify_created_security(
        handle: &File,
        copied: Option<&(Descriptor, DaclState)>,
        label: Option<&LabelState>,
    ) -> Result<()> {
        if let Some((descriptor, expected)) = copied {
            let initial = security(handle)?.1;
            ensure!(
                initial.owner == expected.owner && initial.group == expected.group,
                "Unsupported original owner/group preservation"
            );
            if initial != *expected {
                let (mut present, mut defaulted, mut dacl) = (0, 0, ptr::null_mut());
                ensure!(
                    unsafe {
                        GetSecurityDescriptorDacl(
                            descriptor.0,
                            &mut present,
                            &mut dacl,
                            &mut defaulted,
                        )
                    } != 0
                        && present != 0,
                    "Exact absent-DACL preservation is unavailable"
                );
                let control = if expected.control & 0x1000 != 0 {
                    0x8000_0000
                } else {
                    0x2000_0000
                };
                let result = unsafe {
                    SetSecurityInfo(
                        handle.as_raw_handle(),
                        1,
                        DACL_INFORMATION | control,
                        ptr::null_mut(),
                        ptr::null_mut(),
                        dacl,
                        ptr::null_mut(),
                    )
                };
                if result != 0 {
                    return Err(std::io::Error::from_raw_os_error(result as i32))
                        .context("Exact original DACL/inheritance preservation is unavailable");
                }
            }
            ensure!(
                security(handle)?.1 == *expected,
                "Exact original DACL/inheritance preservation is unavailable"
            );
        }
        if let Some(expected) = label {
            ensure!(
                mandatory_label(handle)? == *expected,
                "Unsupported original mandatory integrity label preservation"
            );
        }
        Ok(())
    }

    pub(super) fn allocate_output(
        path: &Path,
        original: Option<&HeldFile>,
        d: &Deadline,
    ) -> Result<ArmedOutput> {
        d.check()?;
        if let Some(original) = original {
            ensure!(
                !original._executable_binding,
                "Executable binding is not editable host content"
            );
            ensure!(
                information(&original._handle, false)?.1.attributes & ATTRIBUTE_READONLY == 0,
                "Readonly host target mutation refused"
            );
            validate_current(original, d)?;
        }
        let copied = original
            .map(|original| security(&original._handle))
            .transpose()?;
        let label = original
            .map(|original| mandatory_label(&original._handle))
            .transpose()?;
        let (parent, path) = parent_and_path(path, d)?;
        #[cfg(test)]
        run_namespace_hook()?;
        // This fixed constructor arms kernel cleanup before returning the empty
        // object. Any error below closes that same armed handle, without bytes.
        let opened = unsafe {
            namespace::create_armed_file_at_with_security(
                parent_handle(&parent)?,
                path.file_name().context("Host output leaf missing")?,
                copied
                    .as_ref()
                    .map_or(ptr::null(), |(sd, _)| sd.0.cast_const()),
                d,
            )
        }
        .context("Cannot allocate empty armed host output exclusively")?;
        ensure!(opened.created, "Host output allocation was not exclusive");
        let handle = opened.file;
        let (identity, info) = information(&handle, false)?;
        ensure!(
            info.size_high == 0 && info.size_low == 0,
            "Armed host output is not empty"
        );
        ensure!(
            info.attributes & ATTRIBUTE_READONLY == 0,
            "Readonly host output refused"
        );
        verify_created_security(&handle, copied.as_ref(), label.as_ref())?;
        let security_hash = handle_security_fingerprint(&handle)?;
        d.check()?;
        let (current_id, current) = information(&handle, false)?;
        ensure!(
            current_id == identity && current.size_high == 0 && current.size_low == 0,
            "Armed host output changed before registration"
        );
        Ok(ArmedOutput {
            file: HeldFile {
                path,
                identity,
                hash: crate::model::hash([]),
                bytes: Vec::new(),
                byte_length: 0,
                _handle: handle,
                _parent: parent,
                _delete_access: true,
                _writable: true,
                _executable_binding: false,
            },
            security_hash,
        })
    }

    pub(super) fn retain_output(
        output: ArmedOutput,
        registration: &super::super::DurableOutputRegistration,
        d: &Deadline,
    ) -> Result<RegisteredOutput> {
        d.check()?;
        ensure!(
            registration.matches(&output.file.path, output.identity())
                && registration.security_hash() == output.security_hash,
            "Durable host output registration mismatch"
        );
        ensure!(
            output.is_empty()
                && output.file.bytes.is_empty()
                && output.file._writable
                && output.file._delete_access
                && !output.file._executable_binding,
            "Armed host output must be empty and exclusively writable"
        );
        validate_current(&output.file, d)?;
        ensure!(
            handle_security_fingerprint(&output.file._handle)? == output.security_hash,
            "Armed host output security fingerprint mismatch"
        );
        // Perform all validation and allocation before disarming. Once the
        // native call succeeds, return the owned registered object even if its
        // deadline expires; its exact identity is already durably registered.
        let after_hash = registration.after_hash().to_owned();
        namespace::clear_creation_delete_on_close(&output.file._handle, d)?;
        #[cfg(test)]
        run_output_retain_hook();
        Ok(RegisteredOutput {
            file: output.file,
            after_hash,
            security_hash: output.security_hash,
        })
    }

    fn validate_registered(
        output: &mut RegisteredOutput,
        canonical: &[u8],
        d: &Deadline,
    ) -> Result<()> {
        d.check()?;
        ensure!(
            crate::model::valid_id(&output.after_hash)
                && crate::model::hash(canonical) == output.after_hash,
            "Registered host output canonical hash mismatch"
        );
        ensure!(
            output.file._writable && output.file._delete_access && !output.file._executable_binding,
            "Registered host output requires a writable DELETE guard"
        );
        let (identity, info) = information(&output.file._handle, false)?;
        ensure!(
            identity == output.file.identity,
            "Registered host output identity mismatch"
        );
        ensure!(
            info.attributes & ATTRIBUTE_READONLY == 0,
            "Readonly host output mutation refused"
        );
        ensure!(
            handle_security_fingerprint(&output.file._handle)? == output.security_hash,
            "Registered host output security fingerprint mismatch"
        );
        let length = u64::from(info.size_high) << 32 | u64::from(info.size_low);
        ensure!(
            length <= canonical.len() as u64,
            "Registered host output exceeds canonical length"
        );
        let current = bytes(&output.file._handle, canonical.len(), d)?;
        ensure!(
            current.len() as u64 == length && canonical.starts_with(&current),
            "Registered host output is not a raw canonical prefix"
        );
        let (current_id, current_info) = information(&output.file._handle, false)?;
        ensure!(
            current_id == output.file.identity
                && current_info.attributes & ATTRIBUTE_READONLY == 0
                && (u64::from(current_info.size_high) << 32 | u64::from(current_info.size_low))
                    == length,
            "Registered host output changed during validation"
        );
        ensure!(
            handle_security_fingerprint(&output.file._handle)? == output.security_hash,
            "Registered host output security changed during validation"
        );
        // Refresh from the actual held object, including a previous partial write.
        output.file.hash = crate::model::hash(&current);
        output.file.byte_length = current.len();
        output.file.bytes = current;
        d.check()
    }

    pub(super) fn open_registered_output(
        path: &Path,
        exact_id: &str,
        original: Option<&HeldFile>,
        canonical: &[u8],
        require_empty: bool,
        registration: &super::super::DurableOutputRegistration,
        d: &Deadline,
    ) -> Result<Option<RegisteredOutput>> {
        d.check()?;
        ensure!(
            registration.matches(path, exact_id),
            "Durable host output registration mismatch"
        );
        ensure!(
            crate::model::hash(canonical) == registration.after_hash(),
            "Registered host output canonical hash mismatch"
        );
        let original_security = if let Some(original) = original {
            Some(security_fingerprint(original, d)?)
        } else {
            None
        };
        let (parent, path) = match parent_and_path(path, d) {
            Ok(value) => value,
            Err(error) if missing(&error) => return Ok(None),
            Err(error) => return Err(error),
        };
        ensure!(
            registration.matches(&path, exact_id),
            "Canonical durable host output path mismatch"
        );
        #[cfg(test)]
        run_namespace_hook()?;
        let handle = match namespace::open_at(
            parent_handle(&parent)?,
            path.file_name()
                .context("Registered host output leaf missing")?,
            OpenRequest {
                kind: EntryKind::File,
                disposition: Disposition::Open,
                access: GENERIC_READ | GENERIC_WRITE | DELETE,
                share: SHARE_READ,
            },
            d,
        ) {
            Ok(opened) => opened.file,
            Err(error) if missing(&error) => return Ok(None),
            Err(error) => return Err(error).context("Cannot acquire registered host output guard"),
        };
        let (identity, _) = information(&handle, false)?;
        ensure!(
            identity == exact_id,
            "Registered host output identity mismatch"
        );
        let actual_security = handle_security_fingerprint(&handle)?;
        ensure!(
            actual_security == registration.security_hash(),
            "Registered host output durable security fingerprint mismatch"
        );
        if let Some(expected) = original_security {
            ensure!(
                actual_security == expected,
                "Registered host output original security mismatch"
            );
        }
        let mut output = RegisteredOutput {
            file: HeldFile {
                path,
                identity,
                hash: crate::model::hash([]),
                bytes: Vec::new(),
                byte_length: 0,
                _handle: handle,
                _parent: parent,
                _delete_access: true,
                _writable: true,
                _executable_binding: false,
            },
            after_hash: registration.after_hash().to_owned(),
            security_hash: actual_security,
        };
        validate_registered(&mut output, canonical, d)?;
        ensure!(
            !require_empty || output.is_empty(),
            "Registered-stage host output is not empty"
        );
        Ok(Some(output))
    }

    pub(super) fn append_registered(
        output: &mut RegisteredOutput,
        canonical: &[u8],
        d: &Deadline,
    ) -> Result<usize> {
        validate_registered(output, canonical, d)?;
        let start = output.file.byte_length;
        let end = canonical.len().min(start.saturating_add(65_536));
        if start == end {
            return Ok(0);
        }
        output.file._handle.seek(SeekFrom::Start(start as u64))?;
        let mut offset = start;
        while offset < end {
            d.check()?;
            let written = output.file._handle.write(&canonical[offset..end])?;
            if written == 0 {
                return Err(std::io::Error::from(std::io::ErrorKind::WriteZero))
                    .context("Canonical host output append made no progress");
            }
            output
                .file
                .bytes
                .extend_from_slice(&canonical[offset..offset + written]);
            offset += written;
            output.file.byte_length = offset;
            output.file.hash = crate::model::hash(&output.file.bytes);
        }
        output.file._handle.sync_all()?;
        d.check()?;
        Ok(end - start)
    }

    pub(super) fn finalize_registered(
        mut output: RegisteredOutput,
        canonical: &[u8],
        d: &Deadline,
    ) -> Result<HeldFile> {
        validate_registered(&mut output, canonical, d)?;
        ensure!(
            output.file.bytes == canonical,
            "Registered host output is incomplete"
        );
        output.file._handle.sync_all()?;
        d.check()?;
        Ok(output.file)
    }

    pub(super) fn discard_registered(
        mut output: RegisteredOutput,
        canonical: &[u8],
        d: &Deadline,
    ) -> Result<()> {
        validate_registered(&mut output, canonical, d)?;
        disposition(&output.file._handle)
    }

    pub(super) fn prepare_file(
        path: &Path,
        original: Option<&HeldFile>,
        output: &[u8],
        d: &Deadline,
    ) -> Result<HeldFile> {
        d.check()?;
        if let Some(original) = original {
            ensure!(
                !original._executable_binding,
                "Executable binding cannot be used as editable host content"
            );
            ensure!(
                information(&original._handle, false)?.1.attributes & ATTRIBUTE_READONLY == 0,
                "Readonly host target mutation refused"
            );
            validate_current(original, d)?;
        }
        let copied = original
            .map(|original| security(&original._handle))
            .transpose()?;
        let label = original
            .map(|original| mandatory_label(&original._handle))
            .transpose()?;
        let (parent, path) = parent_and_path(path, d)?;
        #[cfg(test)]
        run_namespace_hook()?;
        // The descriptor remains owned by `copied` throughout the synchronous
        // native create. No human bytes are written before its readback checks.
        let opened = unsafe {
            namespace::open_at_with_security(
                parent_handle(&parent)?,
                path.file_name().context("Host prepared leaf missing")?,
                OpenRequest {
                    kind: EntryKind::File,
                    disposition: Disposition::Create,
                    access: GENERIC_READ | GENERIC_WRITE | DELETE | WRITE_DAC,
                    share: SHARE_READ,
                },
                copied
                    .as_ref()
                    .map_or(ptr::null(), |(sd, _)| sd.0.cast_const()),
                d,
            )
        }
        .context("Cannot create host prepared file exclusively")?;
        ensure!(
            opened.created,
            "Prepared host file was not created exclusively"
        );
        let mut handle = opened.file;
        let preparation = (|| -> Result<String> {
            let identity = information(&handle, false)?.0;
            if let Some((descriptor, expected)) = &copied {
                let initial = security(&handle)?.1;
                ensure!(
                    initial.owner == expected.owner && initial.group == expected.group,
                    "Unsupported original owner/group preservation"
                );
                if initial != *expected {
                    let (mut present, mut defaulted, mut dacl) = (0, 0, ptr::null_mut());
                    ensure!(
                        unsafe {
                            GetSecurityDescriptorDacl(
                                descriptor.0,
                                &mut present,
                                &mut dacl,
                                &mut defaulted,
                            )
                        } != 0
                            && present != 0,
                        "Exact absent-DACL preservation is unavailable"
                    );
                    let control = if expected.control & 0x1000 != 0 {
                        0x8000_0000
                    } else {
                        0x2000_0000
                    };
                    let result = unsafe {
                        SetSecurityInfo(
                            handle.as_raw_handle(),
                            1,
                            DACL_INFORMATION | control,
                            ptr::null_mut(),
                            ptr::null_mut(),
                            dacl,
                            ptr::null_mut(),
                        )
                    };
                    if result != 0 {
                        return Err(std::io::Error::from_raw_os_error(result as i32)).context(
                            "Exact original DACL/inheritance preservation is unavailable",
                        );
                    }
                }
                let actual = security(&handle)?.1;
                ensure!(
                    actual == *expected,
                    "Exact original DACL/inheritance preservation is unavailable (expected control {:04x}, actual {:04x}; ACL hashes {:?}/{:?})",
                    expected.control,
                    actual.control,
                    expected.bytes.as_deref().map(crate::model::hash),
                    actual.bytes.as_deref().map(crate::model::hash)
                );
            }
            if let Some(expected) = &label {
                ensure!(
                    mandatory_label(&handle)? == *expected,
                    "Unsupported original mandatory integrity label preservation"
                );
            }
            // Security is verified while the newly created file is still empty.
            for chunk in output.chunks(65_536) {
                d.check()?;
                handle.write_all(chunk)?;
            }
            handle.sync_all()?;
            d.check()?;
            ensure!(
                information(&handle, false)?.0 == identity,
                "Prepared host file identity changed"
            );
            Ok(identity)
        })();
        let identity = match preparation {
            Ok(identity) => identity,
            Err(error) => {
                let cleanup = disposition(&handle);
                drop(handle);
                if let Err(cleanup) = cleanup {
                    return Err(error.context(format!(
                        "Owned prepared-file cleanup also failed: {cleanup:#}"
                    )));
                }
                return Err(error);
            }
        };
        Ok(HeldFile {
            path,
            identity,
            hash: crate::model::hash(output),
            bytes: output.to_vec(),
            byte_length: output.len(),
            _handle: handle,
            _parent: parent,
            _delete_access: true,
            _writable: true,
            _executable_binding: false,
        })
    }

    fn validate_current(file: &HeldFile, d: &Deadline) -> Result<()> {
        d.check()?;
        let (identity, information) = information(&file._handle, false)?;
        ensure!(
            identity == file.identity,
            "Owned host file identity mismatch"
        );
        ensure!(
            !file._delete_access || information.attributes & ATTRIBUTE_READONLY == 0,
            "Readonly host target mutation refused"
        );
        let current = bytes(&file._handle, file.bytes.len(), d)?;
        ensure!(
            current.len() == file.bytes.len() && crate::model::hash(&current) == file.hash,
            "Owned host file hash mismatch"
        );
        Ok(())
    }

    pub(super) fn rename_no_replace(
        file: &mut HeldFile,
        destination: &Path,
        d: &Deadline,
    ) -> Result<()> {
        ensure!(
            file._delete_access,
            "Host rename requires a held DELETE handle"
        );
        validate_current(file, d)?;
        let (parent, destination) = parent_and_path(destination, d)?;
        ensure!(
            parent.identity == file._parent.identity,
            "Host-install rename must remain in the same directory"
        );
        let wide: Vec<u16> = destination
            .file_name()
            .context("Host rename leaf missing")?
            .encode_wide()
            .collect();
        let name_bytes = wide
            .len()
            .checked_mul(2)
            .context("Host rename path length overflow")?;
        let size =
            (offset_of!(RenameInformation, name) + name_bytes).max(size_of::<RenameInformation>());
        let mut storage = vec![0u64; size.div_ceil(size_of::<u64>())];
        let information = storage.as_mut_ptr().cast::<RenameInformation>();
        // u64 storage supplies at least the HANDLE alignment on supported Windows.
        unsafe {
            ptr::addr_of_mut!((*information).flags).write(0);
            ptr::addr_of_mut!((*information).root_directory).write(ptr::null_mut());
            ptr::addr_of_mut!((*information).name_length).write(u32::try_from(name_bytes)?);
            ptr::copy_nonoverlapping(
                wide.as_ptr(),
                ptr::addr_of_mut!((*information).name).cast::<u16>(),
                wide.len(),
            );
        }
        d.check()?;
        // The documented same-directory form uses a simple leaf and NULL root.
        // Parent identity was checked while both parent guards remain held.
        // Zero ReplaceIfExists atomically preserves an occupied destination.
        let mut completion = Box::new(IoStatusBlock {
            status_or_pointer: 0xc000_0001,
            information: 0,
        });
        let mut status = unsafe {
            NtSetInformationFile(
                file._handle.as_raw_handle(),
                &mut *completion,
                information.cast(),
                u32::try_from(size)?,
                10,
            )
        };
        if status == 0x103 {
            // Handles are opened synchronously. Drain an unexpected pending
            // completion before its input/status buffers may leave this scope.
            if unsafe { WaitForSingleObject(file._handle.as_raw_handle(), u32::MAX) } != 0 {
                // Completion is unknown. Retain both buffers rather than let
                // an outstanding kernel operation access released memory.
                std::mem::forget(completion);
                std::mem::forget(storage);
                anyhow::bail!("Native host rename completion wait failed; recovery is required");
            }
            status = completion.status_or_pointer as i32;
        }
        if status < 0 {
            return Err(std::io::Error::from_raw_os_error(
                unsafe { RtlNtStatusToDosError(status) } as i32,
            ))
            .context(format!(
                "Host native no-replace rename failed (NTSTATUS {:08x})",
                status as u32
            ));
        }
        let completed = completion.status_or_pointer as i32;
        ensure!(
            completed >= 0 && completed != 0x103,
            "Native host rename has no successful synchronous completion"
        );
        file.path = destination;
        validate_current(file, d)
    }

    pub(super) fn delete_owned(file: HeldFile, d: &Deadline) -> Result<()> {
        ensure!(
            file._delete_access,
            "Host deletion requires a held DELETE handle"
        );
        validate_current(&file, d)?;
        disposition(&file._handle)
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::{
        ffi::c_void,
        fs::{self, OpenOptions},
        os::windows::{
            ffi::OsStrExt,
            fs::{MetadataExt, OpenOptionsExt},
            io::AsRawHandle,
        },
    };

    include!("host_metadata_directory_tests.rs");

    fn output_registration(
        binding: &OutputBinding,
        canonical: &[u8],
    ) -> super::super::DurableOutputRegistration {
        // Fixture-only construction; production mints this after journal readback.
        super::super::DurableOutputRegistration {
            plan_id: crate::model::hash(b"fixture-plan"),
            journal_id: crate::model::hash(b"fixture-journal"),
            root_index: 0,
            attempt: 0,
            path: binding.path.clone(),
            identity: binding.identity.clone(),
            after_hash: crate::model::hash(canonical),
            security_hash: binding.security_hash.clone(),
        }
    }

    fn persistent_output_fixture(
        path: &Path,
        prefix: &[u8],
        d: &Deadline,
    ) -> Result<OutputBinding> {
        let file = prepare_file(path, None, prefix, d)?;
        let binding = OutputBinding {
            path: file.path.clone(),
            identity: file.identity.clone(),
            byte_length: file.byte_length,
            security_hash: security_fingerprint(&file, d)?,
        };
        drop(file);
        Ok(binding)
    }

    #[test]
    fn staged_armed_output_is_empty_secured_guarded_and_deleted_on_drop() -> Result<()> {
        for protected in [false, true] {
            let fixture = tempfile::tempdir()?;
            let original_path = fixture.path().join("AGENTS.md");
            let output_path = fixture.path().join("output.after");
            fs::write(&original_path, b"human configuration")?;
            if protected {
                make_protected_fixture_dacl(&original_path)?;
            }
            let expected_dacl = dacl_state(&original_path)?;
            let expected_owner = owner_group_state(&original_path)?;
            let expected_label = label_state(&original_path)?;
            let d = Deadline::new(30_000);
            let original = read_file(&original_path, 1024, true, &d)?.expect("original");
            let output = allocate_output(&output_path, Some(&original), &d)?;
            assert!(output.is_empty());
            assert_eq!(output.len(), 0);
            assert_eq!(output.binding().byte_length, 0);
            assert_eq!(fs::read(&output_path)?, b"");
            assert!(
                dacl_state(&output_path)? == expected_dacl,
                "Armed DACL changed"
            );
            assert!(
                owner_group_state(&output_path)? == expected_owner,
                "Armed owner/group changed"
            );
            assert!(
                label_state(&output_path)? == expected_label,
                "Armed label changed"
            );
            assert!(OpenOptions::new().write(true).open(&output_path).is_err());
            assert!(fs::remove_file(&output_path).is_err());
            drop(output);
            assert!(
                !output_path.try_exists()?,
                "Unregistered allocation survived close"
            );
            assert_eq!(fs::read(&original_path)?, b"human configuration");
        }
        Ok(())
    }

    #[test]
    fn staged_armed_output_rejects_wrong_registration_or_expired_retain_before_persistence()
    -> Result<()> {
        let fixture = tempfile::tempdir()?;
        let d = Deadline::new(30_000);
        for mode in ["path", "identity", "deadline", "hash", "security-hash"] {
            let path = fixture.path().join(format!("{mode}.after"));
            let output = allocate_output(&path, None, &d)?;
            let mut token = output_registration(&output.binding(), b"canonical");
            match mode {
                "path" => token.path = fixture.path().join("other.after"),
                "identity" => token.identity.push('0'),
                "hash" => token.after_hash = "invalid".into(),
                "security-hash" => token.security_hash = crate::model::hash(b"foreign security"),
                _ => {}
            }
            let expired = Deadline::new(0);
            let budget = if mode == "deadline" { &expired } else { &d };
            assert!(output.retain(&token, budget).is_err());
            assert!(
                !path.try_exists()?,
                "Rejected registration left a persistent allocation"
            );
        }
        let path = fixture.path().join("expired-allocation.after");
        assert!(allocate_output(&path, None, &Deadline::new(0)).is_err());
        assert!(!path.try_exists()?);
        Ok(())
    }

    #[test]
    fn staged_retains_owned_output_when_cancellation_arrives_after_disarm() -> Result<()> {
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        };
        let fixture = tempfile::tempdir()?;
        let path = fixture.path().join("late-cancel.after");
        let d = Deadline::new(30_000);
        let armed = allocate_output(&path, None, &d)?;
        let binding = armed.binding();
        let token = output_registration(&binding, b"canonical");
        let cancel = Arc::new(AtomicBool::new(false));
        let mut outcome = None;
        let scope = Deadline::run_bounded(&d, cancel.clone(), || {
            let callback_cancel = cancel.clone();
            windows::set_output_retain_hook(Some(Box::new(move || {
                callback_cancel.store(true, Ordering::SeqCst);
            })));
            outcome = Some(armed.retain(&token, &Deadline::new(30_000)));
            Ok(())
        });
        windows::set_output_retain_hook(None);
        let output = outcome.expect("retain outcome captured")?;
        assert!(
            scope.is_err(),
            "Post-disarm cancellation was not observed by the outer scope"
        );
        assert_eq!(output.identity(), binding.identity);
        assert!(output.is_empty());
        drop(output);
        let reopened = read_file(&path, 1024, true, &d)?.expect("durably registered survivor");
        assert_eq!(reopened.identity, binding.identity);
        assert!(reopened.bytes.is_empty());
        reopened.delete_owned(&d)?;
        Ok(())
    }

    #[test]
    fn staged_registered_output_appends_raw_canonical_chunks_and_finishes_same_identity()
    -> Result<()> {
        let fixture = tempfile::tempdir()?;
        let path = fixture.path().join("output.after");
        let d = Deadline::new(30_000);
        // The first chunk ends inside a UTF-8 scalar; recovery compares raw bytes.
        let mut canonical = vec![b'a'; 65_535];
        canonical.extend_from_slice("\u{1f980}tail".as_bytes());
        let armed = allocate_output(&path, None, &d)?;
        let binding = armed.binding();
        assert_eq!(armed.identity(), binding.identity);
        let token = output_registration(&binding, &canonical);
        let mut output = armed.retain(&token, &d)?;
        assert!(output.is_empty());
        assert_eq!(output.append_chunk(&canonical, &d)?, 65_536);
        assert_eq!(output.len(), 65_536);
        assert_eq!(fs::read(&path)?, canonical[..65_536]);
        assert!(output.append_chunk(&canonical, &Deadline::new(0)).is_err());
        let mut changed_suffix = canonical.clone();
        *changed_suffix.last_mut().expect("suffix") = b'!';
        assert!(output.append_chunk(&changed_suffix, &d).is_err());
        assert_eq!(fs::read(&path)?, canonical[..65_536]);
        assert_eq!(
            output.append_chunk(&canonical, &d)?,
            canonical.len() - 65_536
        );
        assert_eq!(output.append_chunk(&canonical, &d)?, 0);
        assert_eq!(output.identity(), binding.identity);
        let complete = output.finalize(&canonical, &d)?;
        assert_eq!(complete.identity, binding.identity);
        assert_eq!(complete.bytes, canonical);
        assert_eq!(complete.hash, token.after_hash);
        complete.delete_owned(&d)?;
        assert!(!path.try_exists()?);
        Ok(())
    }

    #[test]
    fn staged_registered_prefix_reopens_without_truncation_and_discards_by_held_identity()
    -> Result<()> {
        let fixture = tempfile::tempdir()?;
        let d = Deadline::new(30_000);
        let canonical = b"human configuration plus owned instruction";
        for prefix in [b"".as_slice(), &canonical[..9]] {
            let path = fixture
                .path()
                .join(format!("output-{}.after", prefix.len()));
            let binding = persistent_output_fixture(&path, prefix, &d)?;
            let token = output_registration(&binding, canonical);
            let mut output = open_registered_output(
                &binding.path,
                &binding.identity,
                None,
                canonical,
                prefix.is_empty(),
                &token,
                &d,
            )?
            .expect("registered output");
            assert_eq!(output.len(), prefix.len());
            assert_eq!(output.identity(), binding.identity);
            assert_eq!(
                fs::read(&path)?,
                prefix,
                "Reopening truncated the registered prefix"
            );
            assert!(OpenOptions::new().write(true).open(&path).is_err());
            assert!(fs::remove_file(&path).is_err());
            if prefix.is_empty() {
                assert_eq!(output.append_chunk(canonical, &d)?, canonical.len());
                let complete = output.finalize(canonical, &d)?;
                assert_eq!(complete.identity, binding.identity);
                complete.delete_owned(&d)?;
            } else {
                output.discard(canonical, &d)?;
            }
            assert!(!path.try_exists()?);
        }
        let missing = fixture.path().join("missing.after");
        let binding = persistent_output_fixture(&missing, b"", &d)?;
        let token = output_registration(&binding, canonical);
        fs::remove_file(&missing)?;
        assert!(
            open_registered_output(
                &binding.path,
                &binding.identity,
                None,
                canonical,
                false,
                &token,
                &d
            )?
            .is_none()
        );
        Ok(())
    }

    #[test]
    fn staged_registered_reopen_refuses_wrong_id_stage_hash_and_noncanonical_bytes() -> Result<()> {
        let fixture = tempfile::tempdir()?;
        let d = Deadline::new(30_000);
        let canonical = b"expected canonical";
        for (mode, prefix) in [
            ("valid", &canonical[..4]),
            ("wrong-id", &canonical[..4]),
            ("registered-nonempty", &canonical[..4]),
            ("wrong-hash", &canonical[..4]),
            ("foreign-prefix", b"foreign".as_slice()),
            ("oversize", b"expected canonical extra".as_slice()),
        ] {
            let path = fixture.path().join(format!("{mode}.after"));
            let binding = persistent_output_fixture(&path, prefix, &d)?;
            let mut token = output_registration(&binding, canonical);
            if mode == "wrong-hash" {
                token.after_hash = crate::model::hash(b"different suffix");
            }
            let id = if mode == "wrong-id" {
                "foreign-id"
            } else {
                &binding.identity
            };
            let result = open_registered_output(
                &binding.path,
                id,
                None,
                canonical,
                mode == "registered-nonempty",
                &token,
                &d,
            );
            if mode == "valid" {
                drop(result?.expect("positive reopen control"));
            } else {
                assert!(
                    result.is_err(),
                    "Invalid registered binding was accepted: {mode}"
                );
            }
            assert_eq!(fs::read(&path)?, prefix, "Rejected reopen changed bytes");
        }
        Ok(())
    }

    #[test]
    fn staged_registered_reopen_checks_original_security_before_any_mutation() -> Result<()> {
        let fixture = tempfile::tempdir()?;
        let original_path = fixture.path().join("AGENTS.md");
        fs::write(&original_path, b"human configuration")?;
        let d = Deadline::new(30_000);
        let original = read_file(&original_path, 1024, true, &d)?.expect("original");
        let canonical = b"canonical output";
        for (mode, has_original) in [
            ("normal", false),
            ("dacl", false),
            ("group", false),
            ("label", false),
            ("normal", true),
            ("dacl", true),
            ("group", true),
            ("label", true),
        ] {
            let path = fixture.path().join(format!("{mode}-{has_original}.after"));
            let original = has_original.then_some(&original);
            let prepared = prepare_file(&path, original, b"can", &d)?;
            let binding = OutputBinding {
                path: prepared.path.clone(),
                identity: prepared.identity.clone(),
                byte_length: 3,
                security_hash: security_fingerprint(&prepared, &d)?,
            };
            drop(prepared);
            match mode {
                "dacl" => make_protected_fixture_dacl(&path)?,
                "group" => make_everyone_fixture_group(&path)?,
                "label" => make_low_fixture_label(&path)?,
                _ => {}
            }
            let token = output_registration(&binding, canonical);
            let result = open_registered_output(
                &binding.path,
                &binding.identity,
                original,
                canonical,
                false,
                &token,
                &d,
            );
            if mode == "normal" {
                drop(result?.expect("normal security control"));
            } else {
                assert!(
                    result.is_err(),
                    "Changed registered security was accepted: {mode}"
                );
            }
            assert_eq!(fs::read(&path)?, b"can");
        }
        Ok(())
    }

    #[test]
    fn staged_registered_output_rechecks_security_and_full_hash_before_writes_or_cleanup()
    -> Result<()> {
        let fixture = tempfile::tempdir()?;
        let d = Deadline::new(30_000);
        let canonical = b"canonical output";
        let path = fixture.path().join("security.after");
        let armed = allocate_output(&path, None, &d)?;
        let token = output_registration(&armed.binding(), canonical);
        let mut output = armed.retain(&token, &d)?;
        make_protected_fixture_dacl(&path)?;
        assert!(output.append_chunk(canonical, &d).is_err());
        assert!(output.discard(canonical, &d).is_err());
        assert_eq!(
            fs::read(&path)?,
            b"",
            "Security refusal wrote or deleted bytes"
        );

        let path = fixture.path().join("hash.after");
        let armed = allocate_output(&path, None, &d)?;
        let token = output_registration(&armed.binding(), canonical);
        let output = armed.retain(&token, &d)?;
        assert!(output.finalize(b"different canonical", &d).is_err());
        assert_eq!(fs::read(&path)?, b"");

        let path = fixture.path().join("discard-hash.after");
        let armed = allocate_output(&path, None, &d)?;
        let token = output_registration(&armed.binding(), canonical);
        let output = armed.retain(&token, &d)?;
        assert!(output.discard(b"different canonical", &d).is_err());
        assert_eq!(
            fs::read(&path)?,
            b"",
            "Wrong-canonical discard removed the output"
        );
        Ok(())
    }

    #[test]
    fn staged_existing_occupant_and_replaced_registered_identity_are_preserved() -> Result<()> {
        let fixture = tempfile::tempdir()?;
        let d = Deadline::new(30_000);
        let path = fixture.path().join("output.after");
        fs::write(&path, b"foreign occupant")?;
        assert!(allocate_output(&path, None, &d).is_err());
        assert_eq!(fs::read(&path)?, b"foreign occupant");
        fs::remove_file(&path)?;

        let canonical = b"canonical output";
        let binding = persistent_output_fixture(&path, b"can", &d)?;
        let token = output_registration(&binding, canonical);
        drop(
            open_registered_output(
                &binding.path,
                &binding.identity,
                None,
                canonical,
                false,
                &token,
                &d,
            )?
            .expect("positive exact-ID control"),
        );
        let saved = fixture.path().join("saved-original.after");
        fs::rename(&path, &saved)?;
        // A valid canonical prefix isolates identity from the content oracle.
        fs::write(&path, b"can")?;
        let replacement = read_file(&path, 1024, false, &d)?.expect("foreign replacement");
        assert_ne!(replacement.identity, binding.identity);
        drop(replacement);
        assert!(
            open_registered_output(
                &binding.path,
                &binding.identity,
                None,
                canonical,
                false,
                &token,
                &d
            )
            .is_err()
        );
        assert_eq!(fs::read(&path)?, b"can");
        assert_eq!(fs::read(&saved)?, b"can");
        Ok(())
    }

    #[test]
    fn staged_registered_reopen_refuses_hardlinks_readonly_and_incomplete_finalization()
    -> Result<()> {
        let fixture = tempfile::tempdir()?;
        let d = Deadline::new(30_000);
        let canonical = b"canonical output";
        let path = fixture.path().join("partial.after");
        let binding = persistent_output_fixture(&path, b"can", &d)?;
        let token = output_registration(&binding, canonical);
        let output = open_registered_output(
            &binding.path,
            &binding.identity,
            None,
            canonical,
            false,
            &token,
            &d,
        )?
        .expect("partial output");
        assert!(output.finalize(canonical, &d).is_err());
        assert_eq!(
            fs::read(&path)?,
            b"can",
            "Finalization accepted or changed an incomplete prefix"
        );
        fs::hard_link(&path, fixture.path().join("alias.after"))?;
        assert!(
            open_registered_output(
                &binding.path,
                &binding.identity,
                None,
                canonical,
                false,
                &token,
                &d
            )
            .is_err()
        );
        fs::remove_file(fixture.path().join("alias.after"))?;
        let permissions = fs::metadata(&path)?.permissions();
        let mut readonly = permissions.clone();
        readonly.set_readonly(true);
        fs::set_permissions(&path, readonly)?;
        let refused = open_registered_output(
            &binding.path,
            &binding.identity,
            None,
            canonical,
            false,
            &token,
            &d,
        )
        .is_err();
        fs::set_permissions(&path, permissions)?;
        assert!(refused);
        assert_eq!(fs::read(&path)?, b"can");
        Ok(())
    }

    #[link(name = "Advapi32")]
    unsafe extern "system" {
        fn GetFileSecurityW(
            path: *const u16,
            information: u32,
            descriptor: *mut c_void,
            length: u32,
            needed: *mut u32,
        ) -> i32;
        fn GetSecurityDescriptorDacl(
            descriptor: *mut c_void,
            present: *mut i32,
            dacl: *mut *mut c_void,
            defaulted: *mut i32,
        ) -> i32;
        fn GetSecurityDescriptorControl(
            descriptor: *mut c_void,
            control: *mut u16,
            revision: *mut u32,
        ) -> i32;
        fn ConvertStringSecurityDescriptorToSecurityDescriptorW(
            text: *const u16,
            revision: u32,
            descriptor: *mut *mut c_void,
            size: *mut u32,
        ) -> i32;
        fn SetFileSecurityW(path: *const u16, information: u32, descriptor: *mut c_void) -> i32;
        fn GetSecurityDescriptorOwner(
            descriptor: *mut c_void,
            owner: *mut *mut c_void,
            defaulted: *mut i32,
        ) -> i32;
        fn GetSecurityDescriptorGroup(
            descriptor: *mut c_void,
            group: *mut *mut c_void,
            defaulted: *mut i32,
        ) -> i32;
        fn GetLengthSid(sid: *mut c_void) -> u32;
        fn GetSecurityDescriptorSacl(
            descriptor: *mut c_void,
            present: *mut i32,
            sacl: *mut *mut c_void,
            defaulted: *mut i32,
        ) -> i32;
    }
    #[link(name = "Kernel32")]
    unsafe extern "system" {
        fn LocalFree(memory: *mut c_void) -> *mut c_void;
        fn DeviceIoControl(
            handle: *mut c_void,
            code: u32,
            input: *const c_void,
            input_size: u32,
            output: *mut c_void,
            output_size: u32,
            returned: *mut u32,
            overlapped: *mut c_void,
        ) -> i32;
    }

    fn dacl_state(path: &Path) -> Result<(u16, Option<Vec<u8>>)> {
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut length = 0;
        // The first call only determines the required self-relative descriptor length.
        unsafe { GetFileSecurityW(wide.as_ptr(), 4, std::ptr::null_mut(), 0, &mut length) };
        anyhow::ensure!(
            length > 0 && length <= 1_048_576,
            "Invalid fixture descriptor size"
        );
        let mut storage = vec![0u32; (length as usize).div_ceil(4)];
        let descriptor = storage.as_mut_ptr().cast::<c_void>();
        anyhow::ensure!(
            unsafe { GetFileSecurityW(wide.as_ptr(), 4, descriptor, length, &mut length) } != 0,
            "Fixture descriptor read failed"
        );
        let (mut present, mut defaulted, mut dacl, mut control, mut revision) =
            (0, 0, std::ptr::null_mut(), 0, 0);
        anyhow::ensure!(
            unsafe {
                GetSecurityDescriptorDacl(descriptor, &mut present, &mut dacl, &mut defaulted)
            } != 0,
            "Fixture DACL read failed"
        );
        anyhow::ensure!(
            unsafe { GetSecurityDescriptorControl(descriptor, &mut control, &mut revision) } != 0,
            "Fixture control read failed"
        );
        let bytes = if present != 0 && !dacl.is_null() {
            // ACL header contains its byte length at offset two; storage owns the descriptor.
            let size = unsafe { std::ptr::read_unaligned(dacl.cast::<u8>().add(2).cast::<u16>()) }
                as usize;
            anyhow::ensure!((8..=65_535).contains(&size), "Invalid fixture ACL size");
            Some(unsafe { std::slice::from_raw_parts(dacl.cast::<u8>(), size) }.to_vec())
        } else {
            None
        };
        Ok((control & 0x150c, bytes))
    }

    fn make_protected_fixture_dacl(path: &Path) -> Result<()> {
        let text: Vec<u16> = "D:P(A;;FA;;;WD)".encode_utf16().chain(Some(0)).collect();
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut descriptor = std::ptr::null_mut();
        anyhow::ensure!(
            unsafe {
                ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    text.as_ptr(),
                    1,
                    &mut descriptor,
                    std::ptr::null_mut(),
                )
            } != 0,
            "Fixture descriptor creation failed"
        );
        let result = unsafe { SetFileSecurityW(wide.as_ptr(), 4 | 0x8000_0000, descriptor) };
        // The conversion API allocates this descriptor with LocalAlloc.
        unsafe { LocalFree(descriptor) };
        anyhow::ensure!(result != 0, "Fixture DACL setup failed");
        Ok(())
    }

    fn make_legacy_fixture_dacl(path: &Path, directory: bool) -> Result<()> {
        // Matching explicit child ACEs exercise GetSecurityInfo's inheritance
        // conversion. SetFileSecurityW deliberately keeps the legacy form.
        let sddl = if directory {
            "D:P(A;OICI;FA;;;WD)"
        } else {
            "D:(A;;FA;;;WD)"
        };
        let text: Vec<u16> = sddl.encode_utf16().chain(Some(0)).collect();
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut descriptor = std::ptr::null_mut();
        anyhow::ensure!(
            unsafe {
                ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    text.as_ptr(),
                    1,
                    &mut descriptor,
                    std::ptr::null_mut(),
                )
            } != 0,
            "Legacy fixture descriptor creation failed"
        );
        let information = if directory { 4 | 0x8000_0000 } else { 4 };
        let result = unsafe { SetFileSecurityW(wide.as_ptr(), information, descriptor) };
        unsafe { LocalFree(descriptor) };
        anyhow::ensure!(result != 0, "Legacy fixture DACL setup failed");
        Ok(())
    }

    #[test]
    fn legacy_explicit_ace_flags_survive_armed_and_prepared_output() -> Result<()> {
        let fixture = tempfile::tempdir()?;
        make_legacy_fixture_dacl(fixture.path(), true)?;
        let path = fixture.path().join("AGENTS.md");
        fs::write(&path, b"human configuration")?;
        make_legacy_fixture_dacl(&path, false)?;
        let before = dacl_state(&path)?;
        assert_eq!(before.0, 4, "Fixture must use the legacy inheritance model");
        let acl = before.1.as_ref().expect("fixture DACL");
        assert_eq!(u16::from_le_bytes([acl[4], acl[5]]), 1);
        assert_eq!(acl[9], 0, "Fixture ACE must remain explicit");
        let d = Deadline::new(30_000);
        let original = read_file(&path, 1024, true, &d)?.expect("existing target");
        let armed_path = fixture.path().join("armed.after");
        let armed = allocate_output(&armed_path, Some(&original), &d)?;
        assert_eq!(dacl_state(&armed_path)?, before, "Armed ACE flags changed");
        drop(armed);
        assert!(!armed_path.try_exists()?);
        let prepared_path = fixture.path().join("prepared.after");
        let prepared = prepare_file(&prepared_path, Some(&original), b"installed", &d)?;
        assert_eq!(
            dacl_state(&prepared_path)?,
            before,
            "Prepared ACE flags changed"
        );
        assert_eq!(prepared.bytes, b"installed");
        assert_eq!(dacl_state(&path)?, before, "Original security changed");
        assert_eq!(fs::read(&path)?, b"human configuration");
        Ok(())
    }

    fn owner_group_state(path: &Path) -> Result<(Vec<u8>, Vec<u8>)> {
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut length = 0;
        unsafe { GetFileSecurityW(wide.as_ptr(), 3, std::ptr::null_mut(), 0, &mut length) };
        anyhow::ensure!(
            length > 0 && length <= 1_048_576,
            "Invalid fixture descriptor size"
        );
        let mut storage = vec![0u32; (length as usize).div_ceil(4)];
        let descriptor = storage.as_mut_ptr().cast::<c_void>();
        anyhow::ensure!(
            unsafe { GetFileSecurityW(wide.as_ptr(), 3, descriptor, length, &mut length) } != 0,
            "Fixture owner/group read failed"
        );
        let (mut owner, mut group, mut defaulted) = (std::ptr::null_mut(), std::ptr::null_mut(), 0);
        anyhow::ensure!(
            unsafe { GetSecurityDescriptorOwner(descriptor, &mut owner, &mut defaulted) } != 0,
            "Fixture owner read failed"
        );
        anyhow::ensure!(
            unsafe { GetSecurityDescriptorGroup(descriptor, &mut group, &mut defaulted) } != 0,
            "Fixture group read failed"
        );
        let sid = |pointer: *mut c_void| -> Result<Vec<u8>> {
            anyhow::ensure!(!pointer.is_null(), "Fixture SID absent");
            let size = unsafe { GetLengthSid(pointer) } as usize;
            anyhow::ensure!((8..=68).contains(&size), "Invalid fixture SID size");
            Ok(unsafe { std::slice::from_raw_parts(pointer.cast::<u8>(), size) }.to_vec())
        };
        Ok((sid(owner)?, sid(group)?))
    }

    fn make_everyone_fixture_group(path: &Path) -> Result<()> {
        let text: Vec<u16> = "G:WD".encode_utf16().chain(Some(0)).collect();
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut descriptor = std::ptr::null_mut();
        anyhow::ensure!(
            unsafe {
                ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    text.as_ptr(),
                    1,
                    &mut descriptor,
                    std::ptr::null_mut(),
                )
            } != 0,
            "Fixture group descriptor creation failed"
        );
        let result = unsafe { SetFileSecurityW(wide.as_ptr(), 2, descriptor) };
        unsafe { LocalFree(descriptor) };
        anyhow::ensure!(result != 0, "Fixture group setup failed");
        Ok(())
    }

    fn label_state(path: &Path) -> Result<(u16, Option<Vec<u8>>)> {
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut length = 0;
        unsafe { GetFileSecurityW(wide.as_ptr(), 0x10, std::ptr::null_mut(), 0, &mut length) };
        anyhow::ensure!(
            length > 0 && length <= 1_048_576,
            "Invalid fixture label descriptor size"
        );
        let mut storage = vec![0u32; (length as usize).div_ceil(4)];
        let descriptor = storage.as_mut_ptr().cast::<c_void>();
        anyhow::ensure!(
            unsafe { GetFileSecurityW(wide.as_ptr(), 0x10, descriptor, length, &mut length) } != 0,
            "Fixture label read failed: {}",
            std::io::Error::last_os_error()
        );
        let (mut present, mut defaulted, mut sacl, mut control, mut revision) =
            (0, 0, std::ptr::null_mut(), 0, 0);
        anyhow::ensure!(
            unsafe {
                GetSecurityDescriptorSacl(descriptor, &mut present, &mut sacl, &mut defaulted)
            } != 0,
            "Fixture label SACL read failed"
        );
        anyhow::ensure!(
            unsafe { GetSecurityDescriptorControl(descriptor, &mut control, &mut revision) } != 0,
            "Fixture label control read failed"
        );
        let bytes = if present != 0 && !sacl.is_null() {
            let size = unsafe { std::ptr::read_unaligned(sacl.cast::<u8>().add(2).cast::<u16>()) }
                as usize;
            anyhow::ensure!(
                (8..=65_535).contains(&size),
                "Invalid fixture label ACL size"
            );
            Some(unsafe { std::slice::from_raw_parts(sacl.cast::<u8>(), size) }.to_vec())
        } else {
            None
        };
        Ok((control & 0x2a30, bytes))
    }

    fn make_low_fixture_label(path: &Path) -> Result<()> {
        let text: Vec<u16> = "S:(ML;;NW;;;LW)".encode_utf16().chain(Some(0)).collect();
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut descriptor = std::ptr::null_mut();
        anyhow::ensure!(
            unsafe {
                ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    text.as_ptr(),
                    1,
                    &mut descriptor,
                    std::ptr::null_mut(),
                )
            } != 0,
            "Fixture low label descriptor creation failed"
        );
        let result = unsafe { SetFileSecurityW(wide.as_ptr(), 0x10, descriptor) };
        let error = std::io::Error::last_os_error();
        unsafe { LocalFree(descriptor) };
        anyhow::ensure!(
            result != 0,
            "Fixture low label setup requires WRITE_OWNER: {error}"
        );
        Ok(())
    }

    fn begin_owned_directory_retarget(
        directory: &Path,
        target: &Path,
        access: u32,
        required: bool,
    ) -> Result<Option<File>> {
        let writer = match OpenOptions::new()
            .access_mode(access)
            .share_mode(7)
            .custom_flags(0x0220_0000)
            .open(directory)
        {
            Ok(writer) => writer,
            Err(error) if !required && matches!(error.raw_os_error(), Some(5 | 32)) => {
                return Ok(None);
            }
            Err(error) => return Err(error.into()),
        };
        let canonical = target.canonicalize()?;
        let canonical = canonical.to_str().expect("fixture path is UTF-8");
        let print = canonical.strip_prefix(r"\\?\").unwrap_or(canonical);
        let substitute: Vec<u16> = format!(r"\??\{print}").encode_utf16().collect();
        let print: Vec<u16> = print.encode_utf16().collect();
        let substitute_bytes = substitute.len() * 2;
        let print_bytes = print.len() * 2;
        let data_length = 8 + substitute_bytes + 2 + print_bytes + 2;
        let mut input = Vec::new();
        input.extend_from_slice(&0xa000_0003u32.to_le_bytes());
        input.extend_from_slice(&u16::try_from(data_length)?.to_le_bytes());
        input.extend_from_slice(&0u16.to_le_bytes());
        input.extend_from_slice(&0u16.to_le_bytes());
        input.extend_from_slice(&u16::try_from(substitute_bytes)?.to_le_bytes());
        input.extend_from_slice(&u16::try_from(substitute_bytes + 2)?.to_le_bytes());
        input.extend_from_slice(&u16::try_from(print_bytes)?.to_le_bytes());
        for unit in substitute
            .into_iter()
            .chain(Some(0))
            .chain(print)
            .chain(Some(0))
        {
            input.extend_from_slice(&unit.to_le_bytes());
        }
        let mut storage = vec![0u32; input.len().div_ceil(4)];
        unsafe {
            std::ptr::copy_nonoverlapping(
                input.as_ptr(),
                storage.as_mut_ptr().cast::<u8>(),
                input.len(),
            )
        };
        let mut returned = 0;
        let set = unsafe {
            DeviceIoControl(
                writer.as_raw_handle(),
                0x0009_00a4,
                storage.as_ptr().cast(),
                u32::try_from(input.len())?,
                std::ptr::null_mut(),
                0,
                &mut returned,
                std::ptr::null_mut(),
            )
        };
        if set == 0 {
            if required {
                return Err(anyhow::Error::from(std::io::Error::last_os_error())
                    .context("Unheld fixture mountpoint retarget failed"));
            }
            return Ok(None);
        }
        Ok(Some(writer))
    }

    fn clear_owned_directory_retarget(writer: &File) -> Result<()> {
        // Restore only the tag on this same owned directory object before asserting.
        let delete = [0xa000_0003u32, 0];
        let mut returned = 0;
        anyhow::ensure!(
            unsafe {
                DeviceIoControl(
                    writer.as_raw_handle(),
                    0x0009_00ac,
                    delete.as_ptr().cast(),
                    8,
                    std::ptr::null_mut(),
                    0,
                    &mut returned,
                    std::ptr::null_mut(),
                )
            } != 0,
            "Owned fixture reparse cleanup failed: {}",
            std::io::Error::last_os_error()
        );
        Ok(())
    }

    fn try_owned_directory_retarget(
        directory: &Path,
        target: &Path,
        access: u32,
        required: bool,
    ) -> Result<bool> {
        let Some(writer) = begin_owned_directory_retarget(directory, target, access, required)?
        else {
            return Ok(false);
        };
        clear_owned_directory_retarget(&writer)?;
        Ok(true)
    }

    fn owned_private_reparse_tag(handle: &File, delete: bool) -> Result<()> {
        // The installed SDK specifies a nonzero GUID for a private tag.
        // This fixture uses no name-surrogate flag and no filesystem driver.
        let mut input = [
            0x0000_0042u32,
            0,
            0x6f77_6e65,
            0x642d_6669,
            0x7874_7572,
            0x652d_696f,
        ];
        let mut returned = 0;
        anyhow::ensure!(
            unsafe {
                DeviceIoControl(
                    handle.as_raw_handle(),
                    if delete { 0x0009_00ac } else { 0x0009_00a4 },
                    input.as_mut_ptr().cast(),
                    std::mem::size_of_val(&input) as u32,
                    std::ptr::null_mut(),
                    0,
                    &mut returned,
                    std::ptr::null_mut(),
                )
            } != 0,
            "Owned fixture private reparse tag operation failed: {}",
            std::io::Error::last_os_error()
        );
        Ok(())
    }

    fn raw_exclusive_lock(path: &Path) -> Result<File> {
        Ok(OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(0)
            .custom_flags(0x0020_0000)
            .open(path)?)
    }

    #[test]
    fn raw_empty_single_link_state_lock_handle_is_accepted() -> Result<()> {
        let fixture = tempfile::tempdir()?;
        let path = fixture.path().join("writer.lock");
        fs::write(&path, [])?;
        let lock = raw_exclusive_lock(&path)?;
        verify_state_lock_handle(&lock, &Deadline::new(30_000))?;
        assert!(OpenOptions::new().write(true).open(&path).is_err());
        assert!(fs::remove_file(&path).is_err());
        Ok(())
    }

    #[test]
    fn raw_hardlinked_state_lock_handle_is_refused() -> Result<()> {
        let fixture = tempfile::tempdir()?;
        let path = fixture.path().join("writer.lock");
        fs::write(&path, [])?;
        fs::hard_link(&path, fixture.path().join("alias"))?;
        let lock = raw_exclusive_lock(&path)?;
        assert!(verify_state_lock_handle(&lock, &Deadline::new(30_000)).is_err());
        Ok(())
    }

    #[test]
    fn raw_nonempty_state_lock_handle_is_refused() -> Result<()> {
        let fixture = tempfile::tempdir()?;
        let path = fixture.path().join("writer.lock");
        fs::write(&path, b"foreign bytes")?;
        let lock = raw_exclusive_lock(&path)?;
        assert!(verify_state_lock_handle(&lock, &Deadline::new(30_000)).is_err());
        assert_eq!(lock.metadata()?.len(), 13);
        Ok(())
    }

    #[test]
    fn raw_reparse_state_lock_handle_is_refused() -> Result<()> {
        let fixture = tempfile::tempdir()?;
        let path = fixture.path().join("writer.lock");
        fs::write(&path, [])?;
        let lock = raw_exclusive_lock(&path)?;
        owned_private_reparse_tag(&lock, false)?;
        let is_reparse = lock.metadata()?.file_attributes() & 0x400 != 0;
        let accepted = verify_state_lock_handle(&lock, &Deadline::new(30_000)).is_ok();
        owned_private_reparse_tag(&lock, true)?;
        assert!(
            is_reparse,
            "Fixture must present a real regular-file reparse tag"
        );
        assert!(
            !accepted,
            "State lock validator accepted a reparse file handle"
        );
        Ok(())
    }

    #[test]
    fn anchored_entry_information_reports_presence_type_and_length() -> Result<()> {
        let fixture = tempfile::tempdir()?;
        let path = fixture.path().join("entry");
        fs::write(&path, b"abc")?;
        let d = Deadline::new(30_000);
        let file = entry_info(&path, &d)?.expect("existing file");
        assert!(!file.directory);
        assert_eq!(file.length, 3);
        assert!(
            entry_info(fixture.path(), &d)?
                .expect("directory")
                .directory
        );
        assert!(entry_info(&fixture.path().join("missing"), &d)?.is_none());
        Ok(())
    }

    #[test]
    fn anchored_directory_creation_returns_a_live_guard_and_never_replaces() -> Result<()> {
        let fixture = tempfile::tempdir()?;
        let path = fixture.path().join("created");
        let d = Deadline::new(30_000);
        let created = create_directory(&path, &d)?;
        assert!(entry_info(&path, &d)?.expect("created directory").directory);
        assert!(create_directory(&path, &d).is_err());
        assert!(fs::rename(&path, fixture.path().join("moved")).is_err());
        drop(created);
        fs::rename(&path, fixture.path().join("moved"))?;
        Ok(())
    }

    #[test]
    fn anchored_directory_enumeration_is_bounded_and_returns_only_direct_children() -> Result<()> {
        let fixture = tempfile::tempdir()?;
        fs::write(fixture.path().join("a"), b"a")?;
        fs::create_dir(fixture.path().join("b"))?;
        fs::write(fixture.path().join("b").join("nested"), b"nested")?;
        let d = Deadline::new(30_000);
        let mut names = directory_entries(fixture.path(), 2, &d)?;
        names.sort();
        assert_eq!(
            names
                .iter()
                .map(|path| path.file_name().expect("leaf"))
                .collect::<Vec<_>>(),
            vec![std::ffi::OsStr::new("a"), std::ffi::OsStr::new("b")]
        );
        assert!(directory_entries(fixture.path(), 1, &d).is_err());
        Ok(())
    }

    #[test]
    fn anchored_lock_opening_keeps_the_verified_exclusive_file_and_refuses_foreign_bytes()
    -> Result<()> {
        let fixture = tempfile::tempdir()?;
        let path = fixture.path().join("writer.lock");
        let d = Deadline::new(30_000);
        let lock = open_state_lock(&path, &d)?;
        verify_state_lock_handle(&lock, &d)?;
        assert!(OpenOptions::new().write(true).open(&path).is_err());
        assert!(fs::remove_file(&path).is_err());
        drop(lock);
        fs::write(&path, b"foreign lock bytes")?;
        assert!(open_state_lock(&path, &d).is_err());
        assert_eq!(fs::read(&path)?, b"foreign lock bytes");
        Ok(())
    }

    fn install_retarget_hook(
        selected: &Path,
        foreign: &Path,
    ) -> std::rc::Rc<std::cell::RefCell<Option<File>>> {
        let writer = std::rc::Rc::new(std::cell::RefCell::new(None));
        let slot = writer.clone();
        let selected = selected.to_path_buf();
        let foreign = foreign.to_path_buf();
        windows::set_namespace_hook(Some(Box::new(move || {
            let held = begin_owned_directory_retarget(&selected, &foreign, 0x100, true)?
                .ok_or_else(|| anyhow::anyhow!("Owned fixture attribute retarget unavailable"))?;
            *slot.borrow_mut() = Some(held);
            Ok(())
        })));
        writer
    }

    fn finish_retarget_hook(writer: std::rc::Rc<std::cell::RefCell<Option<File>>>) -> Result<()> {
        windows::set_namespace_hook(None);
        let writer = writer.borrow_mut().take().ok_or_else(|| {
            anyhow::anyhow!("Owned fixture must retarget after the parent guard is acquired")
        })?;
        clear_owned_directory_retarget(&writer)
    }

    #[test]
    fn guarded_read_never_follows_retag_after_parent_guard_acquisition() -> Result<()> {
        let fixture = tempfile::tempdir()?;
        let selected = fixture.path().join("selected");
        let foreign = fixture.path().join("foreign");
        fs::create_dir(&selected)?;
        fs::create_dir(&foreign)?;
        fs::write(foreign.join("AGENTS.md"), b"foreign marker")?;
        let writer = install_retarget_hook(&selected, &foreign);
        let result = read_file(
            &selected.join("AGENTS.md"),
            1024,
            false,
            &Deadline::new(30_000),
        );
        let escaped = matches!(&result, Ok(Some(_)));
        drop(result);
        finish_retarget_hook(writer)?;
        assert!(
            !escaped,
            "Guarded read traversed a newly retagged parent into foreign content"
        );
        assert_eq!(fs::read(foreign.join("AGENTS.md"))?, b"foreign marker");
        assert!(!selected.join("AGENTS.md").exists());
        Ok(())
    }

    #[test]
    fn prepared_file_never_escapes_retag_after_parent_guard_acquisition() -> Result<()> {
        let fixture = tempfile::tempdir()?;
        let selected = fixture.path().join("selected");
        let foreign = fixture.path().join("foreign");
        fs::create_dir(&selected)?;
        fs::create_dir(&foreign)?;
        let writer = install_retarget_hook(&selected, &foreign);
        let d = Deadline::new(30_000);
        let result = prepare_file(&selected.join("after"), None, b"owned fixture bytes", &d);
        let escaped = foreign.join("after").exists();
        let cleanup = match result {
            Ok(prepared) => prepared.delete_owned(&d),
            Err(_) => Ok(()),
        };
        finish_retarget_hook(writer)?;
        cleanup?;
        assert!(
            !escaped,
            "Prepared file was created through a newly retagged parent in a foreign directory"
        );
        assert!(!foreign.join("after").exists());
        Ok(())
    }

    #[test]
    fn held_target_denies_external_writes_and_deletion() -> Result<()> {
        let fixture = tempfile::tempdir()?;
        let path = fixture.path().join("AGENTS.md");
        fs::write(&path, b"original")?;
        let d = Deadline::new(30_000);
        let held = read_file(&path, 1024, true, &d)?.expect("existing target");
        assert_eq!(held.bytes, b"original");
        assert!(OpenOptions::new().write(true).open(&path).is_err());
        assert!(fs::remove_file(&path).is_err());
        drop(held);
        assert_eq!(fs::read(&path)?, b"original");
        Ok(())
    }

    #[test]
    fn own_handle_rename_retains_identity_and_write_guard() -> Result<()> {
        let fixture = tempfile::tempdir()?;
        let path = fixture.path().join("AGENTS.md");
        let backup = fixture.path().join("original.backup");
        fs::write(&path, b"original")?;
        let d = Deadline::new(30_000);
        let _directory = guarded_directory(fixture.path(), &d)?;
        let mut held = read_file(&path, 1024, true, &d)?.expect("existing target");
        let identity = held.identity.clone();
        held.rename_no_replace(&backup, &d)?;
        assert!(!path.try_exists()?);
        assert_eq!(held.path.file_name(), backup.file_name());
        assert_eq!(held.identity, identity);
        assert_eq!(fs::read(&backup)?, b"original");
        assert!(OpenOptions::new().write(true).open(&backup).is_err());
        drop(held);
        let reopened = read_file(&backup, 1024, false, &d)?.expect("preserved original");
        assert_eq!(reopened.identity, identity);
        Ok(())
    }

    #[test]
    fn no_replace_preserves_foreign_occupant_and_original_backup() -> Result<()> {
        let fixture = tempfile::tempdir()?;
        let path = fixture.path().join("AGENTS.md");
        let backup = fixture.path().join("original.backup");
        let temporary = fixture.path().join("prepared.tmp");
        fs::write(&path, b"original")?;
        let d = Deadline::new(30_000);
        let _directory = guarded_directory(fixture.path(), &d)?;
        let mut original = read_file(&path, 1024, true, &d)?.expect("existing target");
        let mut prepared = prepare_file(&temporary, Some(&original), b"installed", &d)?;
        original.rename_no_replace(&backup, &d)?;
        fs::write(&path, b"foreign")?;
        assert!(prepared.rename_no_replace(&path, &d).is_err());
        assert_eq!(fs::read(&path)?, b"foreign");
        assert_eq!(fs::read(&backup)?, b"original");
        assert_eq!(fs::read(&temporary)?, b"installed");
        Ok(())
    }

    #[test]
    fn prepared_output_keeps_guard_through_promotion_and_owned_delete() -> Result<()> {
        let fixture = tempfile::tempdir()?;
        let path = fixture.path().join("AGENTS.md");
        let temporary = fixture.path().join("prepared.tmp");
        let d = Deadline::new(30_000);
        let _directory = guarded_directory(fixture.path(), &d)?;
        let mut prepared = prepare_file(&temporary, None, b"installed", &d)?;
        prepared.sync_all(&d)?;
        prepared.rename_no_replace(&path, &d)?;
        assert_eq!(fs::read(&path)?, b"installed");
        assert!(OpenOptions::new().write(true).open(&path).is_err());
        prepared.delete_owned(&d)?;
        assert!(!path.try_exists()?);
        Ok(())
    }

    #[test]
    fn directory_guard_pins_each_existing_ancestor() -> Result<()> {
        let fixture = tempfile::tempdir()?;
        let ancestor = fixture.path().join("parent");
        let selected = ancestor.join("selected");
        fs::create_dir_all(&selected)?;
        let d = Deadline::new(30_000);
        let guard = guarded_directory(&selected, &d)?;
        assert!(!guard.identity.is_empty());
        assert!(fs::rename(&ancestor, fixture.path().join("moved")).is_err());
        drop(guard);
        fs::rename(&ancestor, fixture.path().join("moved"))?;
        Ok(())
    }

    #[test]
    fn multiply_linked_files_are_refused_and_prepare_never_overwrites() -> Result<()> {
        let fixture = tempfile::tempdir()?;
        let path = fixture.path().join("AGENTS.md");
        fs::write(&path, b"foreign")?;
        fs::hard_link(&path, fixture.path().join("alias"))?;
        let d = Deadline::new(30_000);
        assert!(read_file(&path, 1024, true, &d).is_err());
        assert!(prepare_file(&path, None, b"replacement", &d).is_err());
        assert_eq!(fs::read(&path)?, b"foreign");
        Ok(())
    }

    #[test]
    fn readonly_guard_can_pin_running_executable() -> Result<()> {
        let d = Deadline::new(30_000);
        let held = read_file(&std::env::current_exe()?, 512 * 1024 * 1024, false, &d)?
            .expect("running executable");
        assert!(!held.identity.is_empty());
        assert!(!held.bytes.is_empty());
        Ok(())
    }

    #[test]
    fn prepared_output_preserves_dacl_entries_and_inheritance_control() -> Result<()> {
        for protected in [false, true] {
            let fixture = tempfile::tempdir()?;
            let path = fixture.path().join("AGENTS.md");
            let temporary = fixture.path().join("prepared.tmp");
            fs::write(&path, b"original")?;
            if protected {
                make_protected_fixture_dacl(&path)?;
            }
            let before = dacl_state(&path)?;
            assert_eq!(before.0 & 0x1000 != 0, protected);
            let d = Deadline::new(30_000);
            let original = read_file(&path, 1024, true, &d)?.expect("existing target");
            let prepared = prepare_file(&temporary, Some(&original), b"installed", &d)?;
            assert!(
                dacl_state(&temporary)? == before,
                "Prepared DACL bytes/control changed"
            );
            assert_eq!(prepared.bytes, b"installed");
        }
        Ok(())
    }

    #[test]
    fn bounded_reads_reject_oversize_without_mutation() -> Result<()> {
        let fixture = tempfile::tempdir()?;
        let path = fixture.path().join("AGENTS.md");
        fs::write(&path, b"123456789")?;
        let d = Deadline::new(30_000);
        assert!(read_file(&path, 8, false, &d).is_err());
        assert_eq!(fs::read(&path)?, b"123456789");
        Ok(())
    }

    #[test]
    fn prepared_output_preserves_owner_and_nondefault_group_before_human_bytes() -> Result<()> {
        let fixture = tempfile::tempdir()?;
        let path = fixture.path().join("AGENTS.md");
        let normal = fixture.path().join("normal.tmp");
        let special = fixture.path().join("special.tmp");
        fs::write(&path, b"human configuration")?;
        let d = Deadline::new(30_000);
        let normal_security = owner_group_state(&path)?;
        {
            let original = read_file(&path, 1024, true, &d)?.expect("existing original");
            let prepared = prepare_file(&normal, Some(&original), b"installed", &d)?;
            assert!(
                owner_group_state(&normal)? == normal_security,
                "Normal owner/group representation changed"
            );
            prepared.delete_owned(&d)?;
        }
        make_everyone_fixture_group(&path)?;
        let expected = owner_group_state(&path)?;
        // S-1-1-0 (Everyone) is hand-encoded, independently of the backend.
        assert!(
            expected.1 == [1, 1, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0],
            "Fixture group is not Everyone"
        );
        assert!(
            expected.1 != normal_security.1,
            "Fixture must use a nondefault group"
        );
        let original = read_file(&path, 1024, true, &d)?.expect("existing original");
        let prepared = prepare_file(&special, Some(&original), b"installed", &d)?;
        assert!(
            owner_group_state(&special)? == expected,
            "Nondefault owner/group representation changed"
        );
        assert_eq!(prepared.bytes, b"installed");
        Ok(())
    }

    #[test]
    fn executable_binding_streams_hardlinked_bytes_and_denies_alias_writers() -> Result<()> {
        let fixture = tempfile::tempdir()?;
        let path = fixture.path().join("host.exe");
        let alias = fixture.path().join("alias.exe");
        fs::write(&path, b"abc")?;
        fs::hard_link(&path, &alias)?;
        let d = Deadline::new(30_000);
        assert!(read_file(&path, 1024, false, &d).is_err());
        let held = read_executable_binding(&path, 1024, &d)?.expect("existing executable binding");
        assert_eq!(held.byte_length, 3);
        assert!(held.bytes.is_empty());
        assert_eq!(
            held.hash,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert!(OpenOptions::new().write(true).open(&alias).is_err());
        fs::remove_file(&alias)?;
        assert!(!alias.try_exists()?);
        assert!(fs::remove_file(&path).is_err());
        assert!(OpenOptions::new().write(true).open(&path).is_err());
        let rebound = read_executable_binding(&path, 1024, &d)?.expect("same pinned executable");
        assert_eq!(rebound.identity, held.identity);
        assert_eq!(rebound.hash, held.hash);
        assert_eq!(rebound.byte_length, 3);
        Ok(())
    }

    #[test]
    fn explicit_low_integrity_label_is_preserved_or_refused_before_human_bytes() -> Result<()> {
        let fixture = tempfile::tempdir()?;
        let path = fixture.path().join("AGENTS.md");
        let temporary = fixture.path().join("prepared.tmp");
        fs::write(&path, b"human configuration")?;
        make_low_fixture_label(&path)?;
        let expected = label_state(&path)?;
        let acl = expected.1.as_ref().expect("explicit label ACL");
        assert!(
            acl.len() > 8 && acl[8] == 0x11,
            "Explicit mandatory-label ACE required"
        );
        let d = Deadline::new(30_000);
        let original = read_file(&path, 1024, true, &d)?.expect("existing labeled original");
        match prepare_file(
            &temporary,
            Some(&original),
            b"human configuration plus owned instruction",
            &d,
        ) {
            Ok(prepared) => {
                assert!(
                    label_state(&temporary)? == expected,
                    "Mandatory integrity label changed"
                );
                assert_eq!(
                    prepared.bytes,
                    b"human configuration plus owned instruction"
                );
            }
            Err(error) => {
                assert!(
                    error.to_string().contains("mandatory integrity label"),
                    "Label refusal must identify unsupported label preservation"
                );
                assert!(
                    !temporary.try_exists()?,
                    "Unsupported label preparation must remove its owned empty temporary file"
                );
            }
        }
        assert_eq!(fs::read(&path)?, b"human configuration");
        Ok(())
    }

    #[test]
    fn readonly_existing_target_is_refused_before_any_mutation() -> Result<()> {
        let fixture = tempfile::tempdir()?;
        let path = fixture.path().join("AGENTS.md");
        fs::write(&path, b"readonly human configuration")?;
        let original = fs::metadata(&path)?.permissions();
        let mut readonly = original.clone();
        readonly.set_readonly(true);
        fs::set_permissions(&path, readonly)?;
        let d = Deadline::new(30_000);
        let planning_read_allowed = read_file(&path, 1024, false, &d)?.is_some();
        let write_refused = read_file(&path, 1024, true, &d).is_err();
        fs::set_permissions(&path, original)?;
        assert!(planning_read_allowed);
        assert!(write_refused);
        assert_eq!(fs::read(&path)?, b"readonly human configuration");
        Ok(())
    }

    #[test]
    fn directory_attribute_access_can_retag_an_empty_guarded_directory() -> Result<()> {
        let fixture = tempfile::tempdir()?;
        let destination = fixture.path().join("destination");
        fs::create_dir(&destination)?;
        fs::write(destination.join("AGENTS.md"), b"foreign target")?;
        let mut attribute_empty_retarget = None;
        let mut outcomes = Vec::new();
        for (label, access) in [
            ("write", 0x4000_0000),
            ("read", 0x8000_0000),
            ("write_attributes", 0x100),
            ("zero", 0),
        ] {
            let control = fixture.path().join(format!("control-{label}"));
            fs::create_dir(&control)?;
            let unheld =
                try_owned_directory_retarget(&control, &destination, access, label == "write")?;
            eprintln!("Reparse access characterization {label}: unheld_set={unheld}");
            for populated in [false, true] {
                let selected = fixture.path().join(format!("{label}-{populated}"));
                fs::create_dir(&selected)?;
                if populated {
                    fs::write(selected.join("AGENTS.md"), b"original target")?;
                }
                let d = Deadline::new(30_000);
                let _guard = guarded_directory(&selected, &d)?;
                let retargeted =
                    try_owned_directory_retarget(&selected, &destination, access, false)?;
                outcomes.push((label, populated, retargeted));
                if label == "write_attributes" && !populated {
                    attribute_empty_retarget = Some(retargeted);
                }
            }
        }
        eprintln!("Held directory reparse characterization: {outcomes:?}");
        assert_eq!(
            attribute_empty_retarget,
            Some(true),
            "Sharing modes do not deny WRITE_ATTRIBUTES; preserve the observed rerouting threat"
        );
        Ok(())
    }
}
