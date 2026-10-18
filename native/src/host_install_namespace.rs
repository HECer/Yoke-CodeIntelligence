//! Handle-relative Windows namespace operations for host-owned installation.
//! Paths returned by this module identify an internal volume anchor, not a new
//! authorization to access caller-supplied device paths.

use crate::capture::Deadline;
use anyhow::Result;
use std::{
    ffi::{OsStr, OsString, c_void},
    fs::File,
    path::PathBuf,
};

#[derive(Clone, Copy)]
pub(super) enum EntryKind {
    File,
    Directory,
    Any,
}

#[derive(Clone, Copy)]
pub(super) enum Disposition {
    Open,
    Create,
    OpenIf,
}

#[derive(Clone, Copy)]
pub(super) struct OpenRequest {
    pub(super) kind: EntryKind,
    pub(super) disposition: Disposition,
    pub(super) access: u32,
    pub(super) share: u32,
}

pub(super) struct Opened {
    pub(super) file: File,
    pub(super) created: bool,
}

pub(super) struct DirectoryEntry {
    pub(super) name: OsString,
    pub(super) attributes: u32,
    pub(super) byte_length: u64,
}

pub(super) fn open_at(
    parent: &File,
    leaf: &OsStr,
    request: OpenRequest,
    d: &Deadline,
) -> Result<Opened> {
    // The null descriptor grants no additional rights and needs no borrowed
    // descriptor lifetime beyond this synchronous call.
    unsafe { open_at_with_security(parent, leaf, request, std::ptr::null(), d) }
}

/// # Safety
/// `descriptor` must be null or point to a valid self-relative Windows security
/// descriptor that remains readable for the duration of this synchronous call.
pub(super) unsafe fn open_at_with_security(
    parent: &File,
    leaf: &OsStr,
    request: OpenRequest,
    descriptor: *const c_void,
    d: &Deadline,
) -> Result<Opened> {
    #[cfg(windows)]
    return unsafe { windows::open_at(parent, leaf, request, descriptor, d) };
    #[cfg(not(windows))]
    {
        let _ = (parent, leaf, request, descriptor);
        d.check()?;
        anyhow::bail!("Host namespace operations are Windows-only")
    }
}

/// Create one new, empty prepared-output file with creation-time cleanup armed.
/// Existing occupants are never opened, replaced, or granted cleanup authority.
///
/// # Safety
/// `descriptor` must be null or a valid self-relative Windows security descriptor
/// that remains readable for the duration of this synchronous call.
pub(super) unsafe fn create_armed_file_at_with_security(
    parent: &File,
    leaf: &OsStr,
    descriptor: *const c_void,
    d: &Deadline,
) -> Result<Opened> {
    #[cfg(windows)]
    return unsafe { windows::create_armed_file_at_with_security(parent, leaf, descriptor, d) };
    #[cfg(not(windows))]
    {
        let _ = (parent, leaf, descriptor);
        d.check()?;
        anyhow::bail!("Host namespace operations are Windows-only")
    }
}

/// Clear only creation-time cleanup on this same held output file. Registration
/// authority and source/security validation belong to the owning IO state type.
pub(super) fn clear_creation_delete_on_close(file: &File, d: &Deadline) -> Result<()> {
    #[cfg(windows)]
    return windows::clear_creation_delete_on_close(file, d);
    #[cfg(not(windows))]
    {
        let _ = file;
        d.check()?;
        anyhow::bail!("Host namespace operations are Windows-only")
    }
}

pub(super) fn final_path(file: &File, d: &Deadline) -> Result<PathBuf> {
    #[cfg(windows)]
    return windows::final_path(file, d);
    #[cfg(not(windows))]
    {
        let _ = file;
        d.check()?;
        anyhow::bail!("Host namespace operations are Windows-only")
    }
}

pub(super) fn list(
    directory: &File,
    max_entries: usize,
    max_total_name_bytes: usize,
    d: &Deadline,
) -> Result<Vec<DirectoryEntry>> {
    #[cfg(windows)]
    return windows::list(directory, max_entries, max_total_name_bytes, d);
    #[cfg(not(windows))]
    {
        let _ = (directory, max_entries, max_total_name_bytes);
        d.check()?;
        anyhow::bail!("Host namespace operations are Windows-only")
    }
}

#[cfg(windows)]
mod windows {
    use super::*;
    use anyhow::{Context, ensure};
    use std::{
        mem::{offset_of, size_of},
        os::windows::{
            ffi::{OsStrExt, OsStringExt},
            io::{AsRawHandle, FromRawHandle},
        },
        ptr,
        sync::{Mutex, TryLockError},
        time::Duration,
    };

    const SYNCHRONIZE: u32 = 0x0010_0000;
    const SYNCHRONOUS_NONALERT: u32 = 0x20;
    const OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const OBJ_CASE_INSENSITIVE: u32 = 0x40;
    const OBJ_DONT_REPARSE: u32 = 0x1000;

    #[derive(Clone, Copy)]
    enum CreationCleanup {
        Persistent,
        ArmedOutput,
    }

    #[cfg(test)]
    thread_local! {
        pub(super) static AFTER_ARMED_CREATE: std::cell::RefCell<Option<Box<dyn FnOnce()>>> = const { std::cell::RefCell::new(None) };
        pub(super) static AFTER_CREATION_CLEANUP_CLEAR: std::cell::RefCell<Option<Box<dyn FnOnce()>>> = const { std::cell::RefCell::new(None) };
    }

    #[repr(C)]
    struct UnicodeString {
        length: u16,
        maximum_length: u16,
        buffer: *const u16,
    }

    #[repr(C)]
    struct ObjectAttributes {
        length: u32,
        root_directory: *mut c_void,
        object_name: *const UnicodeString,
        attributes: u32,
        security_descriptor: *const c_void,
        security_quality_of_service: *const c_void,
    }

    #[repr(C)]
    struct IoStatusBlock {
        status_or_pointer: usize,
        information: usize,
    }

    #[repr(C)]
    struct FullDirectoryInformation {
        next_entry_offset: u32,
        file_index: u32,
        creation_time: i64,
        last_access_time: i64,
        last_write_time: i64,
        change_time: i64,
        end_of_file: i64,
        allocation_size: i64,
        attributes: u32,
        name_length: u32,
        ea_size: u32,
        name: [u16; 1],
    }

    #[link(name = "ntdll")]
    unsafe extern "system" {
        fn NtCreateFile(
            file: *mut *mut c_void,
            access: u32,
            attributes: *const ObjectAttributes,
            completion: *mut IoStatusBlock,
            allocation_size: *const i64,
            file_attributes: u32,
            share: u32,
            disposition: u32,
            options: u32,
            extended_attributes: *const c_void,
            extended_attributes_length: u32,
        ) -> i32;
        fn NtSetInformationFile(
            file: *mut c_void,
            completion: *mut IoStatusBlock,
            information: *const c_void,
            length: u32,
            class: i32,
        ) -> i32;
        fn RtlNtStatusToDosError(status: i32) -> u32;
    }

    #[link(name = "Kernel32")]
    unsafe extern "system" {
        fn GetFinalPathNameByHandleW(
            file: *mut c_void,
            path: *mut u16,
            capacity: u32,
            flags: u32,
        ) -> u32;
        fn GetFileInformationByHandleEx(
            file: *mut c_void,
            class: i32,
            information: *mut c_void,
            size: u32,
        ) -> i32;
    }

    fn checked_leaf(leaf: &OsStr) -> Result<Vec<u16>> {
        let text = leaf.to_str().context("Host namespace leaf must be UTF-8")?;
        ensure!(
            !text.contains(['/', '\\']),
            "Host namespace operation requires one leaf"
        );
        crate::paths::normalized_syntax(text)?;
        let wide: Vec<u16> = leaf.encode_wide().collect();
        ensure!(
            !wide.is_empty() && wide.len() <= 255,
            "Host namespace leaf exceeds the Windows component limit"
        );
        Ok(wide)
    }

    fn status_error(status: i32) -> anyhow::Error {
        let code = unsafe { RtlNtStatusToDosError(status) };
        std::io::Error::from_raw_os_error(code as i32).into()
    }

    pub(super) unsafe fn open_at(
        parent: &File,
        leaf: &OsStr,
        request: OpenRequest,
        descriptor: *const c_void,
        d: &Deadline,
    ) -> Result<Opened> {
        unsafe {
            open_at_impl(
                parent,
                leaf,
                request,
                descriptor,
                CreationCleanup::Persistent,
                d,
            )
        }
    }

    pub(super) unsafe fn create_armed_file_at_with_security(
        parent: &File,
        leaf: &OsStr,
        descriptor: *const c_void,
        d: &Deadline,
    ) -> Result<Opened> {
        // This is deliberately a fixed CREATE-only operation. Ordinary opens
        // cannot request cleanup authority over an existing occupant.
        let request = OpenRequest {
            kind: EntryKind::File,
            disposition: Disposition::Create,
            access: 0x8000_0000 | 0x4000_0000 | 0x0001_0000 | 0x0004_0000,
            share: 1,
        };
        unsafe {
            open_at_impl(
                parent,
                leaf,
                request,
                descriptor,
                CreationCleanup::ArmedOutput,
                d,
            )
        }
    }

    unsafe fn open_at_impl(
        parent: &File,
        leaf: &OsStr,
        request: OpenRequest,
        descriptor: *const c_void,
        cleanup: CreationCleanup,
        d: &Deadline,
    ) -> Result<Opened> {
        d.check()?;
        let wide = checked_leaf(leaf)?;
        ensure!(
            parent.metadata()?.is_dir(),
            "Host namespace parent is not a directory"
        );
        ensure!(
            request.share & !7 == 0,
            "Invalid host namespace sharing mask"
        );
        let length = u16::try_from(wide.len() * 2)?;
        let name = UnicodeString {
            length,
            maximum_length: length,
            buffer: wide.as_ptr(),
        };
        let attributes = ObjectAttributes {
            length: size_of::<ObjectAttributes>() as u32,
            root_directory: parent.as_raw_handle(),
            object_name: &name,
            attributes: OBJ_CASE_INSENSITIVE | OBJ_DONT_REPARSE,
            security_descriptor: descriptor,
            security_quality_of_service: ptr::null(),
        };
        let kind = match request.kind {
            EntryKind::File => 0x40,   // FILE_NON_DIRECTORY_FILE
            EntryKind::Directory => 1, // FILE_DIRECTORY_FILE
            EntryKind::Any => 0,
        };
        let disposition = match request.disposition {
            Disposition::Open => 1,
            Disposition::Create => 2,
            Disposition::OpenIf => 3,
        };
        let creation_cleanup = match cleanup {
            CreationCleanup::Persistent => 0,
            CreationCleanup::ArmedOutput => 0x1000, // FILE_DELETE_ON_CLOSE
        };
        let mut completion = IoStatusBlock {
            status_or_pointer: 0xc000_0001,
            information: 0,
        };
        let mut raw = ptr::null_mut();
        d.check()?;
        // IRP_MJ_CREATE is synchronous even when a filesystem driver pends its
        // work. The I/O manager completes it before returning to this caller.
        // All borrowed name/security buffers therefore live through completion.
        // https://learn.microsoft.com/en-us/windows-hardware/drivers/ifs/breaking-oplocks
        let status = unsafe {
            NtCreateFile(
                &mut raw,
                request.access | SYNCHRONIZE,
                &attributes,
                &mut completion,
                ptr::null(),
                0x80,
                request.share,
                disposition,
                kind | OPEN_REPARSE_POINT | SYNCHRONOUS_NONALERT | creation_cleanup,
                ptr::null(),
                0,
            )
        };
        if status < 0 {
            return Err(status_error(status)).context("Anchored host namespace open failed");
        }
        ensure!(
            !raw.is_null() && raw != (-1isize) as *mut c_void,
            "Native host namespace open returned no owned handle"
        );
        // The successful native call transfers exactly one owning handle.
        let file = unsafe { File::from_raw_handle(raw) };
        let completed_status = completion.status_or_pointer as u32 as i32;
        if completed_status < 0 {
            return Err(status_error(completed_status))
                .context("Anchored host namespace completion failed");
        }
        ensure!(
            matches!(completion.information, 1 | 2),
            "Unexpected host namespace open outcome"
        );
        #[cfg(test)]
        if matches!(cleanup, CreationCleanup::ArmedOutput) {
            let hook = AFTER_ARMED_CREATE.with(|slot| slot.borrow_mut().take());
            if let Some(hook) = hook {
                hook();
            }
        }
        // Do not discard a successfully created object after a late OS return.
        // Its caller must receive the handle and preserve cleanup authority.
        // The caller verifies metadata/security and checks the deadline before
        // writing content, deleting an owned failed preparation by this handle.
        Ok(Opened {
            file,
            created: completion.information == 2,
        })
    }

    pub(super) fn clear_creation_delete_on_close(file: &File, d: &Deadline) -> Result<()> {
        let flags = 8u32; // FILE_DISPOSITION_ON_CLOSE, without FILE_DISPOSITION_DELETE
        let mut completion = IoStatusBlock {
            status_or_pointer: 0xc000_0001,
            information: 0,
        };
        d.check()?;
        // Nt/ZwSetInformationFile is always synchronous. Both the scalar input
        // and completion block stay alive until the operation has completed.
        // https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntifs/nf-ntifs-ioisoperationsynchronous
        let status = unsafe {
            NtSetInformationFile(
                file.as_raw_handle(),
                &mut completion,
                ptr::from_ref(&flags).cast(),
                size_of::<u32>() as u32,
                64, // FileDispositionInformationEx, native FILE_INFORMATION_CLASS
            )
        };
        if status < 0 {
            return Err(status_error(status)).context("Registered host output retention failed");
        }
        let completed_status = completion.status_or_pointer as u32 as i32;
        if completed_status < 0 {
            return Err(status_error(completed_status))
                .context("Registered host output retention completion failed");
        }
        #[cfg(test)]
        {
            let hook = AFTER_CREATION_CLEANUP_CLEAR.with(|slot| slot.borrow_mut().take());
            if let Some(hook) = hook {
                hook();
            }
        }
        // The caller still owns this exact file. A late cancellation must not
        // turn successful retention into a misleading failed transition.
        Ok(())
    }

    fn checked_volume_path(text: &str) -> Result<()> {
        let tail = text
            .strip_prefix(r"\\?\Volume{")
            .context("Host path has no stable volume GUID anchor")?;
        let (guid, relative) = tail
            .split_once("}\\")
            .context("Host volume GUID anchor is malformed")?;
        ensure!(guid.len() == 36, "Host volume GUID length refused");
        for (index, byte) in guid.bytes().enumerate() {
            ensure!(
                if matches!(index, 8 | 13 | 18 | 23) {
                    byte == b'-'
                } else {
                    byte.is_ascii_hexdigit()
                },
                "Host volume GUID syntax refused"
            );
        }
        if !relative.is_empty() {
            for part in relative.split('\\') {
                checked_leaf(OsStr::new(part))?;
            }
        }
        Ok(())
    }

    pub(super) fn final_path(file: &File, d: &Deadline) -> Result<PathBuf> {
        d.check()?;
        let mut path = vec![0u16; 512];
        loop {
            d.check()?;
            let length = unsafe {
                GetFinalPathNameByHandleW(
                    file.as_raw_handle(),
                    path.as_mut_ptr(),
                    u32::try_from(path.len())?,
                    1, // VOLUME_NAME_GUID | FILE_NAME_NORMALIZED
                )
            };
            if length == 0 {
                return Err(std::io::Error::last_os_error())
                    .context("Handle-derived host volume path unavailable");
            }
            d.check()?;
            let length = length as usize;
            ensure!(length < 32_768, "Host namespace path length exceeded");
            if length >= path.len() {
                path.resize(length + 1, 0);
                continue;
            }
            let text = String::from_utf16(&path[..length])
                .context("Handle-derived host path must be UTF-8")?;
            checked_volume_path(&text)?;
            return Ok(PathBuf::from(text));
        }
    }

    // Restart/resume state belongs to the Windows directory file object. A
    // shared guard may be used by more than one caller, so each entire scan is
    // serialized rather than interleaving another caller's restart operation.
    static ENUMERATION: Mutex<()> = Mutex::new(());

    pub(super) fn list(
        directory: &File,
        max_entries: usize,
        max_total_name_bytes: usize,
        d: &Deadline,
    ) -> Result<Vec<DirectoryEntry>> {
        d.check()?;
        ensure!(
            max_entries <= 100_000,
            "Host namespace entry budget refused"
        );
        ensure!(
            max_total_name_bytes <= 16_777_216,
            "Host namespace name budget refused"
        );
        ensure!(
            directory.metadata()?.is_dir(),
            "Host namespace scan requires a directory"
        );
        let _scan = loop {
            d.check()?;
            match ENUMERATION.try_lock() {
                Ok(scan) => break scan,
                Err(TryLockError::Poisoned(_)) => {
                    anyhow::bail!("Host namespace enumeration lock poisoned")
                }
                Err(TryLockError::WouldBlock) => {
                    std::thread::sleep(d.remaining().min(Duration::from_millis(1)));
                }
            }
        };
        d.check()?;
        let mut storage = vec![0u64; 65_536 / 8];
        let mut entries = Vec::new();
        let mut name_bytes = 0usize;
        let mut restart = true;
        loop {
            d.check()?;
            storage.fill(0);
            let result = unsafe {
                GetFileInformationByHandleEx(
                    directory.as_raw_handle(),
                    if restart { 15 } else { 14 },
                    storage.as_mut_ptr().cast(),
                    (storage.len() * 8) as u32,
                )
            };
            if result == 0 {
                let error = std::io::Error::last_os_error();
                if error.raw_os_error() == Some(18) {
                    d.check()?;
                    return Ok(entries);
                }
                return Err(error).context("Handle-based host enumeration failed");
            }
            restart = false;
            d.check()?;
            // FILE_FULL_DIR_INFO records share the documented native layout.
            // The buffer is 8-byte aligned and every field/name is bounded
            // before an unaligned read or slice is formed.
            let bytes = unsafe {
                std::slice::from_raw_parts(storage.as_ptr().cast::<u8>(), storage.len() * 8)
            };
            let fixed = offset_of!(FullDirectoryInformation, name);
            let mut offset = 0usize;
            loop {
                d.check()?;
                ensure!(
                    offset
                        .checked_add(fixed)
                        .is_some_and(|end| end <= bytes.len()),
                    "Host directory record header is truncated"
                );
                let header = bytes[offset..].as_ptr().cast::<FullDirectoryInformation>();
                let next =
                    unsafe { ptr::read_unaligned(ptr::addr_of!((*header).next_entry_offset)) }
                        as usize;
                let length =
                    unsafe { ptr::read_unaligned(ptr::addr_of!((*header).name_length)) } as usize;
                ensure!(
                    length > 0 && length.is_multiple_of(2),
                    "Host directory name length refused"
                );
                let end = offset
                    .checked_add(fixed)
                    .and_then(|start| start.checked_add(length))
                    .context("Host directory name length overflow")?;
                ensure!(end <= bytes.len(), "Host directory name is truncated");
                if next != 0 {
                    ensure!(
                        next.is_multiple_of(8)
                            && next >= fixed + length
                            && offset
                                .checked_add(next)
                                .is_some_and(|end| end <= bytes.len()),
                        "Host directory record offset refused"
                    );
                }
                let raw_name = &bytes[offset + fixed..end];
                let wide: Vec<u16> = raw_name
                    .chunks_exact(2)
                    .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                    .collect();
                let name = OsString::from_wide(&wide);
                if name != OsStr::new(".") && name != OsStr::new("..") {
                    checked_leaf(&name)?;
                    ensure!(
                        entries.len() < max_entries,
                        "Host directory entry limit exceeded"
                    );
                    name_bytes = name_bytes
                        .checked_add(length)
                        .context("Host directory name budget overflow")?;
                    ensure!(
                        name_bytes <= max_total_name_bytes,
                        "Host directory name byte limit exceeded"
                    );
                    let attributes =
                        unsafe { ptr::read_unaligned(ptr::addr_of!((*header).attributes)) };
                    let size = unsafe { ptr::read_unaligned(ptr::addr_of!((*header).end_of_file)) };
                    ensure!(size >= 0, "Host directory entry size refused");
                    entries.push(DirectoryEntry {
                        name,
                        attributes,
                        byte_length: size as u64,
                    });
                }
                if next == 0 {
                    break;
                }
                offset += next;
            }
        }
    }

    #[cfg(test)]
    mod contention_tests {
        use super::*;
        use std::{fs::OpenOptions, os::windows::fs::OpenOptionsExt, sync::mpsc, time::Duration};

        #[test]
        fn enumeration_mutex_wait_observes_the_callers_deadline() -> Result<()> {
            let fixture = tempfile::tempdir()?;
            let directory = OpenOptions::new()
                .read(true)
                .custom_flags(0x0220_0000)
                .open(fixture.path())?;
            let held_scan = ENUMERATION.lock().expect("unpoisoned fixture lock");
            let (started_tx, started_rx) = mpsc::channel();
            let (result_tx, result_rx) = mpsc::channel();
            let worker = std::thread::spawn(move || {
                let deadline = Deadline::new(25);
                started_tx.send(()).expect("fixture observer alive");
                let result = list(&directory, 10, 1024, &deadline);
                let _ = result_tx.send(result.is_err());
            });
            let started = started_rx.recv_timeout(Duration::from_secs(2));
            let observed = result_rx.recv_timeout(Duration::from_millis(500));
            // Always release and join before asserting, including against the
            // original blocking lock, so this regression test cannot hang.
            drop(held_scan);
            worker.join().expect("namespace fixture worker panicked");
            assert!(started.is_ok(), "Contention worker did not start");
            assert_eq!(
                observed.ok(),
                Some(true),
                "Expired scan remained blocked behind another directory scan"
            );
            Ok(())
        }
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use anyhow::{Context, ensure};
    use std::{
        fs::{self, OpenOptions},
        io::{Read, Write},
        os::windows::{fs::OpenOptionsExt, io::AsRawHandle},
        path::Path,
        ptr,
    };

    #[link(name = "Kernel32")]
    unsafe extern "system" {
        fn DeviceIoControl(
            file: *mut c_void,
            code: u32,
            input: *const c_void,
            input_size: u32,
            output: *mut c_void,
            output_size: u32,
            returned: *mut u32,
            overlapped: *mut c_void,
        ) -> i32;
        fn GetFileInformationByHandleEx(
            file: *mut c_void,
            class: i32,
            information: *mut c_void,
            size: u32,
        ) -> i32;
        fn SetFileInformationByHandle(
            file: *mut c_void,
            class: i32,
            information: *const c_void,
            size: u32,
        ) -> i32;
    }

    const READ: u32 = 0x8000_0000;
    const WRITE: u32 = 0x4000_0000;

    fn parent(path: &Path) -> Result<File> {
        Ok(OpenOptions::new()
            .access_mode(READ | 0x20)
            .share_mode(1)
            .custom_flags(0x0220_0000)
            .open(path)?)
    }

    fn request(kind: EntryKind, disposition: Disposition) -> OpenRequest {
        OpenRequest {
            kind,
            disposition,
            access: READ | WRITE,
            share: 1,
        }
    }

    fn normal_probe(parent: &File, path: &Path, d: &Deadline) -> Result<()> {
        let opened = open_at(
            parent,
            OsStr::new("probe"),
            request(EntryKind::File, Disposition::Create),
            d,
        )?;
        ensure!(opened.created, "Normal exclusive creation must work");
        drop(opened);
        fs::remove_file(path.join("probe"))?;
        Ok(())
    }

    struct Retarget(File);
    impl Retarget {
        fn mount(directory: &Path, target: &Path) -> Result<Self> {
            let writer = OpenOptions::new()
                .access_mode(0x100) // FILE_WRITE_ATTRIBUTES is exempt from sharing.
                .share_mode(7)
                .custom_flags(0x0220_0000)
                .open(directory)?;
            let canonical = target.canonicalize()?;
            let canonical = canonical
                .to_str()
                .context("UTF-8 fixture target required")?;
            let target = canonical.strip_prefix(r"\\?\").unwrap_or(canonical);
            let substitute: Vec<u16> = format!(r"\??\{target}").encode_utf16().collect();
            let print: Vec<u16> = target.encode_utf16().collect();
            let substitute_bytes = substitute.len() * 2;
            let print_bytes = print.len() * 2;
            let mut bytes = Vec::new();
            bytes.extend_from_slice(&0xa000_0003u32.to_le_bytes());
            bytes.extend_from_slice(
                &u16::try_from(8 + substitute_bytes + 2 + print_bytes + 2)?.to_le_bytes(),
            );
            bytes.extend_from_slice(&0u16.to_le_bytes());
            bytes.extend_from_slice(&0u16.to_le_bytes());
            bytes.extend_from_slice(&u16::try_from(substitute_bytes)?.to_le_bytes());
            bytes.extend_from_slice(&u16::try_from(substitute_bytes + 2)?.to_le_bytes());
            bytes.extend_from_slice(&u16::try_from(print_bytes)?.to_le_bytes());
            for unit in substitute
                .into_iter()
                .chain(Some(0))
                .chain(print)
                .chain(Some(0))
            {
                bytes.extend_from_slice(&unit.to_le_bytes());
            }
            let mut input = vec![0u64; bytes.len().div_ceil(8)];
            unsafe {
                ptr::copy_nonoverlapping(
                    bytes.as_ptr(),
                    input.as_mut_ptr().cast::<u8>(),
                    bytes.len(),
                );
            }
            let mut returned = 0;
            ensure!(
                unsafe {
                    DeviceIoControl(
                        writer.as_raw_handle(),
                        0x0009_00a4,
                        input.as_ptr().cast(),
                        u32::try_from(bytes.len())?,
                        ptr::null_mut(),
                        0,
                        &mut returned,
                        ptr::null_mut(),
                    )
                } != 0,
                "Owned mountpoint positive control failed: {}",
                std::io::Error::last_os_error()
            );
            Ok(Self(writer))
        }
    }
    impl Drop for Retarget {
        fn drop(&mut self) {
            let data = [0xa000_0003u32, 0];
            let mut returned = 0;
            unsafe {
                DeviceIoControl(
                    self.0.as_raw_handle(),
                    0x0009_00ac,
                    data.as_ptr().cast(),
                    8,
                    ptr::null_mut(),
                    0,
                    &mut returned,
                    ptr::null_mut(),
                );
            }
        }
    }

    fn adversarial_fixture() -> Result<(tempfile::TempDir, PathBuf, PathBuf)> {
        let fixture = tempfile::tempdir()?;
        let selected = fixture.path().join("selected");
        let outside = fixture.path().join("outside");
        let control = fixture.path().join("control");
        for path in [&selected, &outside, &control] {
            fs::create_dir(path)?;
        }
        fs::write(outside.join("AGENTS.md"), b"Foreign human bytes")?;
        {
            let _retarget = Retarget::mount(&control, &outside)?;
            ensure!(
                fs::read(control.join("AGENTS.md"))? == b"Foreign human bytes",
                "Unheld retarget must genuinely redirect an ordinary path open"
            );
        }
        Ok((fixture, selected, outside))
    }

    #[test]
    fn normal_relative_creation_and_open_preserve_contents_and_created_outcome() -> Result<()> {
        let fixture = tempfile::tempdir()?;
        let parent = parent(fixture.path())?;
        let d = Deadline::new(10_000);
        let mut created = open_at(
            &parent,
            OsStr::new("Report.txt"),
            request(EntryKind::File, Disposition::Create),
            &d,
        )?;
        assert!(created.created);
        created.file.write_all(b"Exact original bytes")?;
        drop(created);
        let mut opened = open_at(
            &parent,
            OsStr::new("report.txt"),
            request(EntryKind::File, Disposition::Open),
            &d,
        )?;
        assert!(!opened.created);
        let mut content = String::new();
        opened.file.read_to_string(&mut content)?;
        assert_eq!(content, "Exact original bytes");
        Ok(())
    }

    #[test]
    fn exclusive_creation_never_truncates_an_existing_leaf() -> Result<()> {
        let fixture = tempfile::tempdir()?;
        fs::write(fixture.path().join("owned.txt"), b"Keep these exact bytes")?;
        let parent = parent(fixture.path())?;
        let d = Deadline::new(10_000);
        let opened = open_at(
            &parent,
            OsStr::new("owned.txt"),
            request(EntryKind::File, Disposition::Open),
            &d,
        )?;
        drop(opened);
        assert!(
            open_at(
                &parent,
                OsStr::new("owned.txt"),
                request(EntryKind::File, Disposition::Create),
                &d,
            )
            .is_err()
        );
        assert_eq!(
            fs::read(fixture.path().join("owned.txt"))?,
            b"Keep these exact bytes"
        );
        Ok(())
    }

    #[test]
    fn open_if_reports_creation_then_reuses_the_same_owned_lock() -> Result<()> {
        let fixture = tempfile::tempdir()?;
        let parent = parent(fixture.path())?;
        let d = Deadline::new(10_000);
        let mut req = request(EntryKind::File, Disposition::OpenIf);
        req.share = 0;
        let first = open_at(&parent, OsStr::new("writer.lock"), req, &d)?;
        assert!(first.created);
        assert!(open_at(&parent, OsStr::new("writer.lock"), req, &d).is_err());
        drop(first);
        let second = open_at(&parent, OsStr::new("writer.lock"), req, &d)?;
        assert!(!second.created);
        assert_eq!(second.file.metadata()?.len(), 0);
        Ok(())
    }

    #[test]
    fn normal_directory_creation_and_entry_kind_checks_preserve_types() -> Result<()> {
        let fixture = tempfile::tempdir()?;
        let parent = parent(fixture.path())?;
        let d = Deadline::new(10_000);
        let mut req = request(EntryKind::Directory, Disposition::Create);
        req.access = READ | 0x20;
        let directory = open_at(&parent, OsStr::new("state"), req, &d)?;
        assert!(directory.created);
        assert!(directory.file.metadata()?.is_dir());
        drop(directory);
        req.disposition = Disposition::Open;
        let opened = open_at(&parent, OsStr::new("state"), req, &d)?;
        assert!(!opened.created);
        drop(opened);
        assert!(
            open_at(
                &parent,
                OsStr::new("state"),
                request(EntryKind::File, Disposition::Open),
                &d
            )
            .is_err()
        );
        req.kind = EntryKind::Any;
        let any = open_at(&parent, OsStr::new("state"), req, &d)?;
        assert!(any.file.metadata()?.is_dir());
        Ok(())
    }

    #[test]
    fn retagged_held_parent_never_redirects_file_creation() -> Result<()> {
        let (_fixture, selected, outside) = adversarial_fixture()?;
        let parent = parent(&selected)?;
        let d = Deadline::new(10_000);
        normal_probe(&parent, &selected, &d)?;
        let retarget = Retarget::mount(&selected, &outside)?;
        let outcome = unsafe {
            open_at_with_security(
                &parent,
                OsStr::new("created.txt"),
                request(EntryKind::File, Disposition::Create),
                ptr::null(),
                &d,
            )
        };
        let wrote = if let Ok(mut opened) = outcome {
            opened.file.write_all(b"Owned publication")?;
            true
        } else {
            false
        };
        drop(retarget);
        assert!(!outside.join("created.txt").try_exists()?);
        assert_eq!(fs::read(outside.join("AGENTS.md"))?, b"Foreign human bytes");
        if wrote {
            assert_eq!(
                fs::read(selected.join("created.txt"))?,
                b"Owned publication"
            );
        }
        Ok(())
    }

    #[test]
    fn retagged_held_parent_never_reads_a_foreign_leaf() -> Result<()> {
        let (_fixture, selected, outside) = adversarial_fixture()?;
        let parent = parent(&selected)?;
        let d = Deadline::new(10_000);
        normal_probe(&parent, &selected, &d)?;
        let retarget = Retarget::mount(&selected, &outside)?;
        let result = open_at(
            &parent,
            OsStr::new("AGENTS.md"),
            request(EntryKind::File, Disposition::Open),
            &d,
        );
        drop(retarget);
        assert!(
            result.is_err(),
            "The original physical directory has no AGENTS.md"
        );
        assert_eq!(fs::read(outside.join("AGENTS.md"))?, b"Foreign human bytes");
        Ok(())
    }

    #[test]
    fn retagged_held_parent_never_redirects_directory_creation() -> Result<()> {
        let (_fixture, selected, outside) = adversarial_fixture()?;
        let parent = parent(&selected)?;
        let d = Deadline::new(10_000);
        normal_probe(&parent, &selected, &d)?;
        let retarget = Retarget::mount(&selected, &outside)?;
        let mut req = request(EntryKind::Directory, Disposition::Create);
        req.access = READ | 0x20;
        let result = open_at(&parent, OsStr::new("state-child"), req, &d);
        let created = result.is_ok();
        drop(result);
        drop(retarget);
        assert!(!outside.join("state-child").try_exists()?);
        if created {
            assert!(selected.join("state-child").is_dir());
        }
        assert_eq!(fs::read(outside.join("AGENTS.md"))?, b"Foreign human bytes");
        Ok(())
    }

    #[test]
    fn handle_enumeration_does_not_follow_a_retagged_parent() -> Result<()> {
        let (_fixture, selected, outside) = adversarial_fixture()?;
        let parent = parent(&selected)?;
        let d = Deadline::new(10_000);
        assert!(list(&parent, 10, 1024, &d)?.is_empty());
        let retarget = Retarget::mount(&selected, &outside)?;
        let result = list(&parent, 10, 1024, &d);
        drop(retarget);
        if let Ok(entries) = result {
            assert!(entries.is_empty(), "Original physical directory is empty");
        }
        assert_eq!(fs::read(outside.join("AGENTS.md"))?, b"Foreign human bytes");
        Ok(())
    }

    #[test]
    fn handle_path_uses_a_stable_volume_anchor_and_original_physical_name() -> Result<()> {
        let (_fixture, selected, outside) = adversarial_fixture()?;
        let parent = parent(&selected)?;
        let d = Deadline::new(10_000);
        let before = final_path(&parent, &d)?;
        let text = before
            .to_str()
            .context("UTF-8 internal fixture path required")?;
        assert!(text.starts_with(r"\\?\Volume{"));
        assert!(text.ends_with(r"\selected"));
        let retarget = Retarget::mount(&selected, &outside)?;
        let after = final_path(&parent, &d);
        drop(retarget);
        if let Ok(after) = after {
            assert_eq!(after, before);
        }
        Ok(())
    }

    #[test]
    fn enumeration_limits_apply_to_real_entries_and_return_exact_metadata() -> Result<()> {
        let fixture = tempfile::tempdir()?;
        fs::write(fixture.path().join("a"), b"ab")?;
        fs::write(fixture.path().join("long-name"), b"c")?;
        let parent = parent(fixture.path())?;
        let d = Deadline::new(10_000);
        let mut entries = list(&parent, 2, 100, &d)?;
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, OsStr::new("a"));
        assert_eq!(entries[0].byte_length, 2);
        assert_eq!(entries[0].attributes & 0x10, 0);
        assert_eq!(entries[1].name, OsStr::new("long-name"));
        assert_eq!(entries[1].byte_length, 1);
        assert!(list(&parent, 1, 100, &d).is_err());
        assert!(list(&parent, 2, 3, &d).is_err());
        Ok(())
    }

    #[test]
    fn invalid_leaves_and_expired_deadlines_never_create_entries() -> Result<()> {
        let fixture = tempfile::tempdir()?;
        let parent = parent(fixture.path())?;
        let d = Deadline::new(10_000);
        normal_probe(&parent, fixture.path(), &d)?;
        for leaf in ["..", "../escape", "a/b", r"a\b", "x:stream", "NUL", "evil."] {
            assert!(
                open_at(
                    &parent,
                    OsStr::new(leaf),
                    request(EntryKind::File, Disposition::Create),
                    &d,
                )
                .is_err(),
                "Unsafe namespace leaf was accepted: {leaf}"
            );
        }
        let expired = Deadline::new(0);
        assert!(
            open_at(
                &parent,
                OsStr::new("late"),
                request(EntryKind::File, Disposition::Create),
                &expired
            )
            .is_err()
        );
        assert!(list(&parent, 10, 100, &expired).is_err());
        assert!(final_path(&parent, &expired).is_err());
        assert!(fs::read_dir(fixture.path())?.next().is_none());
        Ok(())
    }

    #[derive(Debug, Default, PartialEq, Eq)]
    #[repr(C)]
    struct FixtureFileId {
        volume: u64,
        id: [u8; 16],
    }

    fn file_identity(file: &File) -> Result<FixtureFileId> {
        let mut id = FixtureFileId::default();
        ensure!(
            unsafe {
                GetFileInformationByHandleEx(
                    file.as_raw_handle(),
                    18,
                    ptr::from_mut(&mut id).cast(),
                    std::mem::size_of::<FixtureFileId>() as u32,
                )
            } != 0,
            "Fixture file identity unavailable: {}",
            std::io::Error::last_os_error()
        );
        Ok(id)
    }

    fn armed(parent: &File, leaf: &str, d: &Deadline) -> Result<Opened> {
        unsafe { create_armed_file_at_with_security(parent, OsStr::new(leaf), ptr::null(), d) }
    }

    fn armed_fixture_file(path: &Path) -> Result<File> {
        // Independent real Windows fixture: an unavailable product constructor
        // must not mask a missing clear operation.
        Ok(OpenOptions::new()
            .read(true)
            .write(true)
            .access_mode(READ | WRITE | 0x0001_0000 | 0x0004_0000)
            .share_mode(1)
            .custom_flags(0x0400_0000 | 0x0020_0000)
            .create_new(true)
            .open(path)?)
    }

    struct ClearLifecycleHooks;
    impl Drop for ClearLifecycleHooks {
        fn drop(&mut self) {
            windows::AFTER_ARMED_CREATE.with(|slot| {
                slot.borrow_mut().take();
            });
            windows::AFTER_CREATION_CLEANUP_CLEAR.with(|slot| {
                slot.borrow_mut().take();
            });
        }
    }

    #[test]
    fn namespace_lifecycle_armed_empty_creation_deletes_on_last_close() -> Result<()> {
        let fixture = tempfile::tempdir()?;
        let directory = parent(fixture.path())?;
        let d = Deadline::new(10_000);
        let opened = armed(&directory, "attempt.after", &d)?;
        assert!(opened.created);
        assert_eq!(opened.file.metadata()?.len(), 0);
        assert!(fixture.path().join("attempt.after").try_exists()?);
        drop(opened);
        assert!(!fixture.path().join("attempt.after").try_exists()?);
        Ok(())
    }

    #[test]
    fn namespace_lifecycle_retention_keeps_exact_identity_and_subsequent_bytes() -> Result<()> {
        let fixture = tempfile::tempdir()?;
        let d = Deadline::new(10_000);
        let mut file = armed_fixture_file(&fixture.path().join("attempt.after"))?;
        let identity = file_identity(&file)?;
        clear_creation_delete_on_close(&file, &d)?;
        assert_eq!(file_identity(&file)?, identity);
        file.write_all(b"Registered fixture content")?;
        file.sync_all()?;
        drop(file);
        let file = File::open(fixture.path().join("attempt.after"))?;
        assert_eq!(file_identity(&file)?, identity);
        assert_eq!(
            fs::read(fixture.path().join("attempt.after"))?,
            b"Registered fixture content"
        );
        Ok(())
    }

    #[test]
    fn namespace_lifecycle_exclusive_creation_preserves_foreign_file_and_directory() -> Result<()> {
        let fixture = tempfile::tempdir()?;
        let directory = parent(fixture.path())?;
        let d = Deadline::new(10_000);
        drop(armed(&directory, "positive-control", &d)?);
        fs::write(fixture.path().join("occupied"), b"Foreign fixture bytes")?;
        fs::create_dir(fixture.path().join("directory"))?;
        assert!(armed(&directory, "occupied", &d).is_err());
        assert!(armed(&directory, "directory", &d).is_err());
        assert_eq!(
            fs::read(fixture.path().join("occupied"))?,
            b"Foreign fixture bytes"
        );
        assert!(fixture.path().join("directory").is_dir());
        Ok(())
    }

    #[test]
    fn namespace_lifecycle_armed_creation_never_escapes_a_retagged_held_parent() -> Result<()> {
        let (_fixture, selected, outside) = adversarial_fixture()?;
        let directory = parent(&selected)?;
        let d = Deadline::new(10_000);
        drop(armed(&directory, "positive-control", &d)?);
        let retarget = Retarget::mount(&selected, &outside)?;
        let opened = armed(&directory, "attempt.after", &d);
        // Inspect while a returned handle is still held: delete-on-close must
        // not conceal an escaped create by removing it before this assertion.
        assert!(!outside.join("attempt.after").try_exists()?);
        assert_eq!(fs::read(outside.join("AGENTS.md"))?, b"Foreign human bytes");
        if let Ok(opened) = opened {
            assert!(opened.created);
            assert_eq!(opened.file.metadata()?.len(), 0);
            drop(opened);
        }
        drop(retarget);
        assert!(!selected.join("attempt.after").try_exists()?);
        Ok(())
    }

    #[test]
    fn namespace_lifecycle_expired_deadline_neither_creates_nor_disarms() -> Result<()> {
        let fixture = tempfile::tempdir()?;
        let directory = parent(fixture.path())?;
        let file = armed_fixture_file(&fixture.path().join("attempt.after"))?;
        let expired = Deadline::new(0);
        assert!(armed(&directory, "late.after", &expired).is_err());
        assert!(clear_creation_delete_on_close(&file, &expired).is_err());
        drop(file);
        assert!(!fixture.path().join("attempt.after").try_exists()?);
        assert!(!fixture.path().join("late.after").try_exists()?);
        Ok(())
    }

    #[test]
    fn namespace_lifecycle_populated_retained_output_supports_existing_owned_cleanup() -> Result<()>
    {
        let fixture = tempfile::tempdir()?;
        let d = Deadline::new(10_000);
        let mut file = armed_fixture_file(&fixture.path().join("attempt.after"))?;
        clear_creation_delete_on_close(&file, &d)?;
        file.write_all(b"Partial registered fixture output")?;
        file.sync_all()?;
        let delete = 1u8;
        ensure!(
            unsafe {
                SetFileInformationByHandle(
                    file.as_raw_handle(),
                    4,
                    ptr::from_ref(&delete).cast(),
                    1,
                )
            } != 0,
            "Disarmed populated fixture cleanup failed: {}",
            std::io::Error::last_os_error()
        );
        drop(file);
        assert!(!fixture.path().join("attempt.after").try_exists()?);
        Ok(())
    }

    #[test]
    fn namespace_lifecycle_successful_create_returns_ownership_after_host_cancellation()
    -> Result<()> {
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        };
        let fixture = tempfile::tempdir()?;
        let directory = parent(fixture.path())?;
        let outer = Deadline::new(10_000);
        let cancelled = Arc::new(AtomicBool::new(false));
        let hook_cancel = Arc::clone(&cancelled);
        let _hooks = ClearLifecycleHooks;
        windows::AFTER_ARMED_CREATE.with(|slot| {
            slot.replace(Some(Box::new(move || {
                hook_cancel.store(true, Ordering::SeqCst)
            })));
        });
        let mut outcome = None;
        let mut cancelled_after_call = false;
        let scope = Deadline::run_bounded(&outer, cancelled, || {
            let deadline = Deadline::new(10_000);
            outcome = Some(armed(&directory, "attempt.after", &deadline));
            cancelled_after_call = deadline.check().is_err();
            Ok(())
        });
        let opened = outcome.context("Create call did not run")??;
        assert!(
            cancelled_after_call,
            "Create completion hook did not cancel the host scope"
        );
        assert!(
            scope.is_err(),
            "The outer scope must still observe cancellation"
        );
        assert!(opened.created);
        assert_eq!(opened.file.metadata()?.len(), 0);
        drop(opened);
        assert!(!fixture.path().join("attempt.after").try_exists()?);
        Ok(())
    }

    #[test]
    fn namespace_lifecycle_successful_clear_reports_retention_after_host_cancellation() -> Result<()>
    {
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        };
        let fixture = tempfile::tempdir()?;
        let outer = Deadline::new(10_000);
        let file = armed_fixture_file(&fixture.path().join("attempt.after"))?;
        let identity = file_identity(&file)?;
        let cancelled = Arc::new(AtomicBool::new(false));
        let hook_cancel = Arc::clone(&cancelled);
        let _hooks = ClearLifecycleHooks;
        windows::AFTER_CREATION_CLEANUP_CLEAR.with(|slot| {
            slot.replace(Some(Box::new(move || {
                hook_cancel.store(true, Ordering::SeqCst)
            })));
        });
        let mut outcome = None;
        let mut cancelled_after_call = false;
        let scope = Deadline::run_bounded(&outer, cancelled, || {
            let deadline = Deadline::new(10_000);
            outcome = Some(clear_creation_delete_on_close(&file, &deadline));
            cancelled_after_call = deadline.check().is_err();
            Ok(())
        });
        outcome.context("Clear call did not run")??;
        assert!(
            cancelled_after_call,
            "Clear completion hook did not cancel the host scope"
        );
        assert!(
            scope.is_err(),
            "The outer scope must still observe cancellation"
        );
        assert_eq!(file_identity(&file)?, identity);
        drop(file);
        let file = File::open(fixture.path().join("attempt.after"))?;
        assert_eq!(file_identity(&file)?, identity);
        assert_eq!(file.metadata()?.len(), 0);
        Ok(())
    }
}
