//! Test-only Windows capability gate for prepared-output ownership transfer.
//! These tests do not install anything or expose a production failure hook.
//! Native APIs: https://learn.microsoft.com/en-us/windows/win32/api/winternl/nf-winternl-ntcreatefile
//! https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntddk/ns-ntddk-_file_disposition_information_ex
//! https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_disposition_info

#![cfg(windows)]

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    ffi::c_void,
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    mem::size_of,
    os::windows::{
        ffi::OsStrExt,
        fs::{MetadataExt, OpenOptionsExt},
        io::{AsRawHandle, FromRawHandle},
    },
    path::PathBuf,
    process::{Child, Command, Stdio},
    ptr,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

const PREFIX: &str = "YOKE_OUTPUT_DISPOSITION_V1 ";
const OUTPUT: &str = "prepared.after";
const CONTENT: &[u8] = b"owned primitive fixture bytes\r\n";
const MAX_LINE: usize = 4096;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
enum Action {
    Armed,
    LegacyFalse,
    ExZero,
    ExOnCloseOnly,
    DisarmThenLegacyDelete,
    DisarmThenExDelete,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Request {
    root: PathBuf,
    nonce: String,
    action: Action,
    populate: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Facts {
    identity: String,
    length: u64,
    links: u32,
    delete_pending: bool,
    native_mode_status: i32,
    native_mode_completion_status: i32,
    native_mode: u32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Observation {
    nonce: String,
    action: Action,
    windows_version: [u32; 3],
    filesystem: String,
    operation_status: i64,
    operation_completion_status: Option<i32>,
    operation_succeeded: bool,
    before: Facts,
    after: Facts,
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
struct FileIdInfo {
    volume: u64,
    id: [u8; 16],
}

#[repr(C)]
struct FileStandardInfo {
    allocation_size: i64,
    end_of_file: i64,
    links: u32,
    delete_pending: u8,
    directory: u8,
}

#[repr(C)]
struct OsVersionInfo {
    size: u32,
    major: u32,
    minor: u32,
    build: u32,
    platform: u32,
    service_pack: [u16; 128],
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
        size: u32,
        class: u32,
    ) -> i32;
    fn NtQueryInformationFile(
        file: *mut c_void,
        completion: *mut IoStatusBlock,
        information: *mut c_void,
        size: u32,
        class: u32,
    ) -> i32;
    fn RtlGetVersion(version: *mut OsVersionInfo) -> i32;
}

#[link(name = "Kernel32")]
unsafe extern "system" {
    fn GetFileInformationByHandleEx(
        file: *mut c_void,
        class: u32,
        information: *mut c_void,
        size: u32,
    ) -> i32;
    fn SetFileInformationByHandle(
        file: *mut c_void,
        class: u32,
        information: *const c_void,
        size: u32,
    ) -> i32;
    fn GetVolumeInformationByHandleW(
        file: *mut c_void,
        volume_name: *mut u16,
        volume_capacity: u32,
        serial: *mut u32,
        maximum_component: *mut u32,
        flags: *mut u32,
        filesystem_name: *mut u16,
        filesystem_capacity: u32,
    ) -> i32;
}

fn completion() -> IoStatusBlock {
    IoStatusBlock {
        status_or_pointer: 0xc000_0001,
        information: 0,
    }
}

fn facts(file: &File, allow_owned_deleted_link: bool) -> Result<Facts> {
    let mut id = FileIdInfo {
        volume: 0,
        id: [0; 16],
    };
    let mut standard = FileStandardInfo {
        allocation_size: 0,
        end_of_file: 0,
        links: 0,
        delete_pending: 0,
        directory: 0,
    };
    for (class, output, length) in [
        (
            18,
            (&mut id as *mut FileIdInfo).cast::<c_void>(),
            size_of::<FileIdInfo>(),
        ),
        (
            1,
            (&mut standard as *mut FileStandardInfo).cast::<c_void>(),
            size_of::<FileStandardInfo>(),
        ),
    ] {
        let ok = unsafe {
            GetFileInformationByHandleEx(file.as_raw_handle(), class, output, length as u32)
        };
        ensure!(
            ok != 0,
            "Fixture handle query failed: {}",
            std::io::Error::last_os_error()
        );
    }
    ensure!(
        standard.directory == 0
            && (standard.links == 1 || (allow_owned_deleted_link && standard.links == 0))
            && standard.end_of_file >= 0,
        "Probe output is not a single-link ordinary file"
    );
    let mut mode = 0u32;
    let mut status = completion();
    let mode_status = unsafe {
        NtQueryInformationFile(
            file.as_raw_handle(),
            &mut status,
            (&mut mode as *mut u32).cast(),
            size_of::<u32>() as u32,
            16,
        )
    };
    Ok(Facts {
        identity: format!("{:016x}:{}", id.volume, hex::encode(id.id)),
        length: standard.end_of_file as u64,
        links: standard.links,
        delete_pending: standard.delete_pending != 0,
        native_mode_status: mode_status,
        native_mode_completion_status: status.status_or_pointer as u32 as i32,
        native_mode: mode,
    })
}

fn create_armed(parent: &File) -> Result<File> {
    let leaf: Vec<u16> = std::ffi::OsStr::new(OUTPUT).encode_wide().collect();
    let name = UnicodeString {
        length: (leaf.len() * 2) as u16,
        maximum_length: (leaf.len() * 2) as u16,
        buffer: leaf.as_ptr(),
    };
    let attributes = ObjectAttributes {
        length: size_of::<ObjectAttributes>() as u32,
        root_directory: parent.as_raw_handle(),
        object_name: &name,
        attributes: 0x40 | 0x1000, // OBJ_CASE_INSENSITIVE | OBJ_DONT_REPARSE
        security_descriptor: ptr::null(),
        security_quality_of_service: ptr::null(),
    };
    let mut raw = ptr::null_mut();
    let mut status = completion();
    let result = unsafe {
        NtCreateFile(
            &mut raw,
            0xc000_0000 | 0x0001_0000 | 0x0004_0000 | 0x0010_0000,
            &attributes,
            &mut status,
            ptr::null(),
            0x80,
            1, // FILE_SHARE_READ, matching the production prepared-output guard
            2, // FILE_CREATE: an existing occupant is never opened or replaced
            0x40 | 0x20 | 0x0020_0000 | 0x1000,
            // NON_DIRECTORY | SYNCHRONOUS_NONALERT | OPEN_REPARSE_POINT | DELETE_ON_CLOSE
            ptr::null(),
            0,
        )
    };
    ensure!(
        result >= 0 && !raw.is_null() && raw != (-1isize) as *mut c_void,
        "Owned FILE_CREATE+DELETE_ON_CLOSE failed with NTSTATUS {result:#010x}"
    );
    let file = unsafe { File::from_raw_handle(raw) };
    ensure!(
        status.information == 2 && (status.status_or_pointer as u32 as i32) >= 0,
        "Native fixture creation did not report a new file"
    );
    Ok(file)
}

fn extended_disposition(file: &File, flags: u32) -> (bool, i64, Option<i32>) {
    let mut status = completion();
    let result = unsafe {
        NtSetInformationFile(
            file.as_raw_handle(),
            &mut status,
            (&flags as *const u32).cast(),
            4,
            64,
        )
    };
    let completed = status.status_or_pointer as u32 as i32;
    (
        result >= 0 && result != 0x103 && completed >= 0 && completed != 0x103,
        result as i64,
        Some(completed),
    )
}

fn legacy_disposition(file: &File, delete: u8) -> (bool, i64, Option<i32>) {
    let result = unsafe {
        SetFileInformationByHandle(file.as_raw_handle(), 4, (&delete as *const u8).cast(), 1)
    };
    (
        result != 0,
        if result != 0 {
            0
        } else {
            std::io::Error::last_os_error().raw_os_error().unwrap_or(-1) as i64
        },
        None,
    )
}

fn apply_action(file: &File, action: Action) -> (bool, i64, Option<i32>) {
    match action {
        Action::Armed => (true, 0, None),
        Action::LegacyFalse => legacy_disposition(file, 0),
        Action::ExZero | Action::ExOnCloseOnly => {
            let flags = if action == Action::ExOnCloseOnly {
                8u32
            } else {
                0u32
            };
            extended_disposition(file, flags)
        }
        Action::DisarmThenLegacyDelete | Action::DisarmThenExDelete => {
            let disarm = extended_disposition(file, 8);
            if !disarm.0 {
                return disarm;
            }
            if action == Action::DisarmThenLegacyDelete {
                legacy_disposition(file, 1)
            } else {
                extended_disposition(file, 1)
            }
        }
    }
}

fn windows_version() -> Result<[u32; 3]> {
    let mut version = OsVersionInfo {
        size: size_of::<OsVersionInfo>() as u32,
        major: 0,
        minor: 0,
        build: 0,
        platform: 0,
        service_pack: [0; 128],
    };
    let status = unsafe { RtlGetVersion(&mut version) };
    ensure!(status >= 0, "Fixture version query failed: {status:#010x}");
    Ok([version.major, version.minor, version.build])
}

fn filesystem(parent: &File) -> Result<String> {
    let mut name = [0u16; 64];
    let ok = unsafe {
        GetVolumeInformationByHandleW(
            parent.as_raw_handle(),
            ptr::null_mut(),
            0,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            name.as_mut_ptr(),
            name.len() as u32,
        )
    };
    ensure!(
        ok != 0,
        "Fixture filesystem query failed: {}",
        std::io::Error::last_os_error()
    );
    let end = name
        .iter()
        .position(|unit| *unit == 0)
        .context("Unterminated filesystem name")?;
    Ok(String::from_utf16(&name[..end])?)
}

fn bounded_line(input: &mut impl BufRead) -> Result<Vec<u8>> {
    let mut line = Vec::new();
    input
        .take((MAX_LINE + 1) as u64)
        .read_until(b'\n', &mut line)?;
    ensure!(
        !line.is_empty() && line.len() <= MAX_LINE && line.ends_with(b"\n"),
        "Probe input line is missing or too large"
    );
    Ok(line)
}

/// Only the integration-test executable has this stdin-driven worker. It is not
/// linked into the product, and accepts only a parent-owned nonce-bound fixture.
#[test]
#[ignore = "spawned only by owned-temp disposition capability tests"]
fn disposition_probe_worker() -> Result<()> {
    let mut input = BufReader::new(std::io::stdin().lock());
    let request: Request = serde_json::from_slice(&bounded_line(&mut input)?)?;
    ensure!(
        request.nonce.len() <= 128 && request.nonce.starts_with("yoke-output-disposition-"),
        "Unrecognized probe fixture nonce"
    );
    ensure!(
        request.root.file_name().and_then(|name| name.to_str()) == Some(request.nonce.as_str()),
        "Probe root does not match its explicit fixture nonce"
    );
    ensure!(
        request.root.is_absolute() && request.root.canonicalize()? == request.root,
        "Probe fixture must use its exact canonical absolute path"
    );
    ensure!(
        fs::read(request.root.join("permit"))? == request.nonce.as_bytes(),
        "Probe fixture has no matching parent-created permit"
    );
    let metadata = fs::symlink_metadata(&request.root)?;
    ensure!(
        metadata.is_dir() && metadata.file_attributes() & 0x400 == 0,
        "Probe root must be an ordinary owned fixture directory"
    );
    let parent = OpenOptions::new()
        .read(true)
        .share_mode(1)
        .custom_flags(0x0200_0000 | 0x0020_0000)
        .open(&request.root)?;
    let mut file = create_armed(&parent)?;
    let before = facts(&file, false)?;
    ensure!(
        before.length == 0,
        "New armed output already contains bytes"
    );
    let (operation_succeeded, operation_status, operation_completion_status) =
        apply_action(&file, request.action);
    if request.populate && operation_succeeded {
        file.write_all(CONTENT)?;
        file.sync_all()?;
    }
    let after = facts(
        &file,
        operation_succeeded
            && matches!(
                request.action,
                Action::DisarmThenLegacyDelete | Action::DisarmThenExDelete
            ),
    )?;
    ensure!(
        before.identity == after.identity,
        "Disposition changed the held file identity"
    );
    let observation = Observation {
        nonce: request.nonce,
        action: request.action,
        windows_version: windows_version()?,
        filesystem: filesystem(&parent)?,
        operation_status,
        operation_completion_status,
        operation_succeeded,
        before,
        after,
    };
    println!("\n{PREFIX}{}", serde_json::to_string(&observation)?);
    std::io::stdout().flush()?;
    // The parent kills this child while blocked here, or explicitly asks for
    // process::exit. Neither path runs Rust destructors for the owned file.
    ensure!(
        bounded_line(&mut input)? == b"exit\n",
        "Invalid probe exit request"
    );
    std::process::exit(23);
}

struct ProbeChild {
    child: Child,
    fixture: Option<tempfile::TempDir>,
}

impl Drop for ProbeChild {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
        }
        let until = Instant::now() + Duration::from_secs(2);
        while Instant::now() < until {
            if self.child.try_wait().ok().flatten().is_some() {
                return;
            }
            thread::sleep(Duration::from_millis(5));
        }
        // Never block the test thread indefinitely in wait/join. If an OS
        // operation prevents confirmed termination, retain this owned fixture
        // rather than attempting directory cleanup beneath a live process.
        if let Some(fixture) = self.fixture.take() {
            eprintln!(
                "Owned probe {} did not stop; preserved fixture {}",
                self.child.id(),
                fixture.keep().display()
            );
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Termination {
    Exit,
    Kill,
}

fn probe(
    action: Action,
    termination: Termination,
    populate: bool,
) -> Result<(Observation, Option<Facts>)> {
    // tempfile honors the root coordinator's selected TEMP/TMP scratch drive.
    let fixture = tempfile::Builder::new()
        .prefix("yoke-output-disposition-")
        .tempdir()?;
    let root = fixture.path().canonicalize()?;
    let nonce = root
        .file_name()
        .and_then(|name| name.to_str())
        .context("Fixture nonce is not UTF-8")?
        .to_owned();
    fs::write(root.join("permit"), nonce.as_bytes())?;
    let mut child = Command::new(std::env::current_exe()?)
        .args([
            "--ignored",
            "--exact",
            "disposition_probe_worker",
            "--nocapture",
            "--test-threads=1",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let output = child.stdout.take().context("Probe stdout missing")?;
    let (tx, rx) = mpsc::channel();
    let _reader = thread::spawn(move || {
        let result = (|| -> Result<Observation> {
            let mut input = BufReader::new(output.take(65_536));
            loop {
                let line = bounded_line(&mut input)?;
                if let Some(json) = line.strip_prefix(PREFIX.as_bytes()) {
                    return Ok(serde_json::from_slice(json)?);
                }
            }
        })();
        let _ = tx.send(result.map_err(|error| format!("{error:#}")));
    });
    let mut owned = ProbeChild {
        child,
        fixture: Some(fixture),
    };
    let request = Request {
        root: root.clone(),
        nonce: nonce.clone(),
        action,
        populate,
    };
    let stdin = owned.child.stdin.as_mut().context("Probe stdin missing")?;
    serde_json::to_writer(&mut *stdin, &request)?;
    stdin.write_all(b"\n")?;
    stdin.flush()?;
    let observation = rx
        .recv_timeout(Duration::from_secs(15))
        .context("Probe did not reach the bounded real-process barrier")?
        .map_err(anyhow::Error::msg)?;
    ensure!(
        observation.nonce == nonce && observation.action == action,
        "Child observation does not match this owned fixture"
    );
    match termination {
        Termination::Exit => {
            let stdin = owned.child.stdin.as_mut().context("Probe stdin missing")?;
            stdin.write_all(b"exit\n")?;
            stdin.flush()?;
        }
        Termination::Kill => owned
            .child
            .kill()
            .context("Could not kill the owned probe process")?,
    }
    let until = Instant::now() + Duration::from_secs(5);
    let status = loop {
        if let Some(status) = owned.child.try_wait()? {
            break status;
        }
        ensure!(
            Instant::now() < until,
            "Owned probe did not terminate within five seconds"
        );
        thread::sleep(Duration::from_millis(5));
    };
    match termination {
        Termination::Exit => ensure!(
            status.code() == Some(23),
            "Child did not execute process::exit(23): {status}"
        ),
        Termination::Kill => ensure!(
            !status.success(),
            "Killed probe unexpectedly reported success"
        ),
    }
    // No parent or sibling ever opened/duplicated the output while the child
    // held it. The observation below therefore follows the last handle close.
    let survivor = match File::open(root.join(OUTPUT)) {
        Ok(mut file) => {
            let snapshot = facts(&file, false)?;
            let mut bytes = Vec::new();
            Read::by_ref(&mut file).take(1024).read_to_end(&mut bytes)?;
            ensure!(
                bytes == if populate { CONTENT } else { b"" },
                "Surviving probe output has unexpected bytes"
            );
            Some(snapshot)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error).context("Could not inspect output after the probe exited"),
    };
    eprintln!(
        "disposition capability {action:?}/{termination:?}/populate={populate}: {} survivor={survivor:?}",
        serde_json::to_string(&observation)?
    );
    Ok((observation, survivor))
}

#[test]
fn armed_empty_output_is_deleted_after_real_process_exit() -> Result<()> {
    let (_, survivor) = probe(Action::Armed, Termination::Exit, false)?;
    ensure!(
        survivor.is_none(),
        "Creation-time delete-on-close did not delete the owned empty output"
    );
    Ok(())
}

#[test]
fn armed_empty_output_is_deleted_after_real_process_kill() -> Result<()> {
    let (_, survivor) = probe(Action::Armed, Termination::Kill, false)?;
    ensure!(
        survivor.is_none(),
        "Creation-time delete-on-close did not survive process termination"
    );
    Ok(())
}

#[test]
fn legacy_false_does_not_disarm_creation_time_delete_on_close() -> Result<()> {
    for termination in [Termination::Exit, Termination::Kill] {
        let (_, survivor) = probe(Action::LegacyFalse, termination, false)?;
        ensure!(
            survivor.is_none(),
            "Legacy Boolean unexpectedly cleared creation-time delete-on-close"
        );
    }
    Ok(())
}

#[test]
fn extended_zero_flags_are_characterized_after_exit_and_kill() -> Result<()> {
    for termination in [Termination::Exit, Termination::Kill] {
        let (observation, survivor) = probe(Action::ExZero, termination, false)?;
        if let Some(survivor) = survivor {
            ensure!(
                observation.operation_succeeded && survivor.identity == observation.before.identity,
                "EX zero flags left an unexpected replacement identity"
            );
        }
    }
    Ok(())
}

fn require_same_handle_disarm(termination: Termination, populate: bool) -> Result<()> {
    let (observation, survivor) = probe(Action::ExOnCloseOnly, termination, populate)?;
    ensure!(
        observation.operation_succeeded,
        "Required same-handle EX ON_CLOSE-only disarm is unsupported: NTSTATUS {:#010x}; Windows {:?}, filesystem {}",
        observation.operation_status,
        observation.windows_version,
        observation.filesystem
    );
    let survivor = survivor.context("Required EX ON_CLOSE-only disarm returned success but output was deleted at real process termination")?;
    ensure!(
        survivor.identity == observation.before.identity
            && survivor.identity == observation.after.identity,
        "The original held identity did not survive same-handle disarm"
    );
    ensure!(
        survivor.length == if populate { CONTENT.len() as u64 } else { 0 },
        "Disarmed output length changed across process termination"
    );
    Ok(())
}

#[test]
fn required_same_handle_disarm_survives_real_process_exit() -> Result<()> {
    require_same_handle_disarm(Termination::Exit, false)
}

#[test]
fn required_same_handle_disarm_survives_real_process_kill() -> Result<()> {
    require_same_handle_disarm(Termination::Kill, false)
}

#[test]
fn required_same_handle_disarm_preserves_subsequent_bytes_after_real_process_exit() -> Result<()> {
    require_same_handle_disarm(Termination::Exit, true)
}

#[test]
fn required_same_handle_disarm_preserves_subsequent_bytes_after_real_process_kill() -> Result<()> {
    require_same_handle_disarm(Termination::Kill, true)
}

#[test]
fn disarmed_output_can_be_explicitly_deleted_by_legacy_boolean_on_the_same_handle() -> Result<()> {
    for termination in [Termination::Exit, Termination::Kill] {
        let (observation, survivor) = probe(Action::DisarmThenLegacyDelete, termination, false)?;
        ensure!(
            observation.operation_succeeded && survivor.is_none(),
            "Disarmed output could not be explicitly deleted through the original handle"
        );
    }
    Ok(())
}

#[test]
fn disarmed_output_can_be_explicitly_deleted_by_extended_delete_on_the_same_handle() -> Result<()> {
    for termination in [Termination::Exit, Termination::Kill] {
        let (observation, survivor) = probe(Action::DisarmThenExDelete, termination, false)?;
        ensure!(
            observation.operation_succeeded && survivor.is_none(),
            "Disarmed output could not be deleted with native extended DELETE"
        );
    }
    Ok(())
}
