// STAGED ONLY: future items for the existing Windows host_install_io::tests
// module. Not registered, compiled or executed. No production implementation.
// Requires the separately gated MetadataDirectory API from io-plan.md and the
// existing owned-fixture begin/clear reparse and protected-DACL helpers.
// Common future root filter: metadata_directory_

struct MetadataDirectoryRetag(Option<File>);

impl MetadataDirectoryRetag {
    fn mount(directory: &Path, target: &Path) -> Result<Self> {
        let file = begin_owned_directory_retarget(directory, target, 0x100, true)?
            .ok_or_else(|| anyhow::anyhow!("Owned attribute-only retag must succeed"))?;
        Ok(Self(Some(file)))
    }

    fn clear(mut self) -> Result<()> {
        clear_owned_directory_retarget(
            &self
                .0
                .take()
                .ok_or_else(|| anyhow::anyhow!("Owned retag handle missing"))?,
        )
    }
}

impl Drop for MetadataDirectoryRetag {
    fn drop(&mut self) {
        if let Some(file) = self.0.take() {
            // Panic/error fallback only. Every normal test path checks clear().
            let _ = clear_owned_directory_retarget(&file);
        }
    }
}

fn metadata_directory_retag_fixture() -> Result<(tempfile::TempDir, PathBuf, PathBuf)> {
    let fixture = tempfile::tempdir()?;
    let selected = fixture.path().join("selected");
    let outside = fixture.path().join("outside");
    let control = fixture.path().join("control");
    for path in [&selected, &outside, &control] {
        fs::create_dir(path)?;
    }
    fs::write(outside.join("foreign.json"), b"foreign marker")?;
    let redirect = MetadataDirectoryRetag::mount(&control, &outside)?;
    let redirected = fs::read(control.join("foreign.json"));
    redirect.clear()?;
    assert_eq!(
        redirected?, b"foreign marker",
        "Unheld positive control must really traverse the owned mountpoint"
    );
    Ok((fixture, selected, outside))
}

#[test]
fn metadata_directory_read_bytes_rereads_changed_content_on_the_same_reader() -> Result<()> {
    // Catches retaining the first row's bytes or File handle across read calls.
    let fixture = tempfile::tempdir()?;
    let path = fixture.path().join("event.json");
    fs::write(&path, b"{\"v\":1}")?;
    let d = Deadline::new(30_000);
    let directory = open_metadata_directory(fixture.path(), &d)?;
    assert_eq!(
        directory
            .read_bytes("event.json", 64, &d)?
            .expect("first row"),
        b"{\"v\":1}"
    );
    fs::write(&path, b"{\"v\":2}")?;
    assert_eq!(
        directory
            .read_bytes("event.json", 64, &d)?
            .expect("fresh row"),
        b"{\"v\":2}"
    );
    Ok(())
}

#[test]
fn metadata_directory_read_bytes_observes_a_new_physical_file_at_the_same_leaf() -> Result<()> {
    // Catches retaining a previously accepted object instead of reopening leaf.
    let fixture = tempfile::tempdir()?;
    let path = fixture.path().join("event.json");
    fs::write(&path, b"before")?;
    let d = Deadline::new(30_000);
    let directory = open_metadata_directory(fixture.path(), &d)?;
    assert_eq!(
        directory
            .read_bytes("event.json", 64, &d)?
            .expect("first row"),
        b"before"
    );
    let mut original = read_file(&path, 64, true, &d)?.expect("owned fixture row");
    let original_id = original.identity.clone();
    original.rename_no_replace(&fixture.path().join("preserved.json"), &d)?;
    drop(original);
    fs::write(&path, b"after!")?;
    let replacement = read_file(&path, 64, false, &d)?.expect("replacement fixture");
    assert_ne!(
        replacement.identity, original_id,
        "Fixture must replace the physical object"
    );
    drop(replacement);
    assert_eq!(
        directory
            .read_bytes("event.json", 64, &d)?
            .expect("replacement row"),
        b"after!"
    );
    assert_eq!(fs::read(fixture.path().join("preserved.json"))?, b"before");
    Ok(())
}

#[test]
fn metadata_directory_names_reenumerate_added_and_removed_foreign_leaves() -> Result<()> {
    // Catches cached directory entries or enumeration that fails to restart.
    let fixture = tempfile::tempdir()?;
    fs::write(fixture.path().join("a.json"), b"a")?;
    let d = Deadline::new(30_000);
    let directory = open_metadata_directory(fixture.path(), &d)?;
    assert_eq!(directory.names(8, &d)?, vec!["a.json"]);
    fs::write(fixture.path().join("b.json"), b"foreign new row")?;
    assert_eq!(directory.names(8, &d)?, vec!["a.json", "b.json"]);
    fs::remove_file(fixture.path().join("a.json"))?;
    assert_eq!(directory.names(8, &d)?, vec!["b.json"]);
    assert!(directory.read_bytes("a.json", 64, &d)?.is_none());
    assert!(directory.entry_info("a.json", &d)?.is_none());
    assert_eq!(fs::read(fixture.path().join("b.json"))?, b"foreign new row");
    Ok(())
}

#[test]
fn metadata_directory_entry_info_queries_current_size_on_every_pass() -> Result<()> {
    // Catches trusting list sizes or a cached first stat in reserve's second pass.
    let fixture = tempfile::tempdir()?;
    let path = fixture.path().join("a.json");
    fs::write(&path, b"a")?;
    let d = Deadline::new(30_000);
    let directory = open_metadata_directory(fixture.path(), &d)?;
    assert_eq!(directory.names(1, &d)?, vec!["a.json"]);
    assert_eq!(
        directory
            .entry_info("a.json", &d)?
            .expect("first stat")
            .length,
        1
    );
    fs::write(&path, b"longer")?;
    assert_eq!(
        directory
            .entry_info("a.json", &d)?
            .expect("second stat")
            .length,
        6
    );
    assert_eq!(
        directory.read_bytes("a.json", 6, &d)?.expect("fresh bytes"),
        b"longer"
    );
    Ok(())
}

#[test]
fn metadata_directory_held_parent_retag_never_reads_a_foreign_row() -> Result<()> {
    // Catches rebuilding a path or walking the constructor path before a read.
    let (_fixture, selected, outside) = metadata_directory_retag_fixture()?;
    let d = Deadline::new(30_000);
    let directory = open_metadata_directory(&selected, &d)?;
    assert!(directory.names(1, &d)?.is_empty());
    assert!(directory.read_bytes("foreign.json", 64, &d)?.is_none());
    let redirect = MetadataDirectoryRetag::mount(&selected, &outside)?;
    let outcome = directory.read_bytes("foreign.json", 64, &d);
    redirect.clear()?;
    assert!(
        !matches!(outcome, Ok(Some(_))),
        "A held-parent read must use the original empty object or refuse"
    );
    assert_eq!(fs::read(outside.join("foreign.json"))?, b"foreign marker");
    assert!(!selected.join("foreign.json").exists());
    Ok(())
}

#[test]
fn metadata_directory_held_parent_retag_never_enumerates_foreign_names() -> Result<()> {
    // Catches re-enumerating the constructor pathname rather than its handle.
    let (_fixture, selected, outside) = metadata_directory_retag_fixture()?;
    let d = Deadline::new(30_000);
    let directory = open_metadata_directory(&selected, &d)?;
    assert!(directory.names(1, &d)?.is_empty());
    let redirect = MetadataDirectoryRetag::mount(&selected, &outside)?;
    let outcome = directory.names(8, &d);
    redirect.clear()?;
    if let Ok(names) = outcome {
        assert!(
            names.is_empty(),
            "Enumeration escaped into the foreign directory"
        );
    }
    assert_eq!(fs::read(outside.join("foreign.json"))?, b"foreign marker");
    Ok(())
}

#[test]
fn metadata_directory_held_parent_retag_never_stats_a_foreign_leaf() -> Result<()> {
    // Catches an absolute-path stat after the parent was already authorized.
    let (_fixture, selected, outside) = metadata_directory_retag_fixture()?;
    let d = Deadline::new(30_000);
    let directory = open_metadata_directory(&selected, &d)?;
    assert!(directory.entry_info("foreign.json", &d)?.is_none());
    let redirect = MetadataDirectoryRetag::mount(&selected, &outside)?;
    let outcome = directory.entry_info("foreign.json", &d);
    redirect.clear()?;
    assert!(
        !matches!(outcome, Ok(Some(_))),
        "A held-parent stat must inspect the original object or refuse"
    );
    assert_eq!(fs::read(outside.join("foreign.json"))?, b"foreign marker");
    Ok(())
}

#[test]
fn metadata_directory_reparse_child_is_an_error_not_missing_or_ordinary() -> Result<()> {
    // Catches dropping enumeration reparse checks or following child reparses.
    let fixture = tempfile::tempdir()?;
    let selected = fixture.path().join("selected");
    let outside = fixture.path().join("outside");
    fs::create_dir(&selected)?;
    fs::create_dir(&outside)?;
    fs::write(outside.join("foreign.json"), b"foreign marker")?;
    let d = Deadline::new(30_000);
    let directory = open_metadata_directory(&selected, &d)?;
    assert!(directory.names(1, &d)?.is_empty());
    let link = selected.join("link");
    fs::create_dir(&link)?;
    let redirect = MetadataDirectoryRetag::mount(&link, &outside)?;
    let control = fs::read(link.join("foreign.json"));
    let physical_attributes = redirect
        .0
        .as_ref()
        .map(|file| file.metadata().map(|metadata| metadata.file_attributes()));
    let raw_entries = namespace::list(
        directory
            ._directory
            ._handles
            .last()
            .expect("Owned metadata directory handle"),
        8,
        8 * 510,
        &d,
    );
    let raw_observation = raw_entries.as_ref().map(|entries| {
        entries
            .iter()
            .map(|entry| (entry.name.to_str(), entry.attributes, entry.byte_length))
            .collect::<Vec<_>>()
    });
    let names = directory.names(8, &d);
    let stat = directory.entry_info("link", &d);
    let read = directory.read_bytes("link", 64, &d);
    let names_refused = names.is_err();
    let stat_refused = stat.is_err();
    let read_refused = read.is_err();
    let stat_observation = match &stat {
        Ok(Some(info)) => format!(
            "present(directory={},length={})",
            info.directory, info.length
        ),
        Ok(None) => "missing".to_owned(),
        Err(error) => format!("error({error:#})"),
    };
    let read_observation = match &read {
        Ok(Some(bytes)) => format!("present(length={})", bytes.len()),
        Ok(None) => "missing".to_owned(),
        Err(error) => format!("error({error:#})"),
    };
    eprintln!(
        "Owned reparse diagnostic: physical_handle_attributes={physical_attributes:?}; raw_held_entries={raw_observation:?}; names={names:?}; stat={stat_observation}; read={read_observation}"
    );
    redirect.clear()?;
    assert_eq!(
        control?, b"foreign marker",
        "Child reparse positive control must redirect"
    );
    assert!(
        names_refused,
        "Reparse child was silently accepted by enumeration"
    );
    assert!(
        stat_refused,
        "Reparse child was accepted as ordinary or missing"
    );
    assert!(
        read_refused,
        "Reparse child was accepted as readable or missing"
    );
    assert_eq!(fs::read(outside.join("foreign.json"))?, b"foreign marker");
    Ok(())
}

#[test]
fn metadata_directory_hardlinked_rows_are_refused_without_changing_either_name() -> Result<()> {
    use anyhow::Context as _;

    // Catches omitting information() single-link checks from either leaf method.
    let fixture =
        tempfile::tempdir().context("Hardlink fixture: create owned temporary directory")?;
    let path = fixture.path().join("a.json");
    let alias = fixture.path().join("alias.json");
    fs::write(&path, b"immutable row").context("Hardlink fixture: write single-link row")?;
    let d = Deadline::new(30_000);
    let directory = open_metadata_directory(fixture.path(), &d)
        .context("Hardlink fixture: open held metadata directory")?;
    assert_eq!(
        directory
            .read_bytes("a.json", 64, &d)
            .context("Hardlink fixture: initial single-link read")?
            .expect("single-link row"),
        b"immutable row"
    );
    let directory = match fs::hard_link(&path, &alias) {
        Ok(()) => directory,
        Err(error) if error.raw_os_error() == Some(32) => {
            assert!(
                !alias.exists(),
                "Blocked late setup unexpectedly created an alias"
            );
            drop(directory);
            fs::hard_link(&path, &alias).context(
                "Hardlink fixture: create persistent alias after releasing directory guard",
            )?;
            open_metadata_directory(fixture.path(), &d)
                .context("Hardlink fixture: reopen reader over the existing hardlink")?
        }
        Err(error) => {
            return Err(error).context("Hardlink fixture: unexpected late alias creation error");
        }
    };
    assert!(directory.read_bytes("a.json", 64, &d).is_err());
    assert!(directory.entry_info("a.json", &d).is_err());
    assert_eq!(
        fs::read(&path).context("Hardlink fixture: verify original bytes")?,
        b"immutable row"
    );
    assert_eq!(
        fs::read(&alias).context("Hardlink fixture: verify alias bytes")?,
        b"immutable row"
    );
    Ok(())
}

#[test]
fn metadata_directory_leaf_methods_refuse_noncanonical_or_multiple_components() -> Result<()> {
    // Catches treating a normalized relative path as an authorized single leaf.
    let fixture = tempfile::tempdir()?;
    let selected = fixture.path().join("selected");
    fs::create_dir(&selected)?;
    fs::write(fixture.path().join("foreign.json"), b"foreign marker")?;
    fs::write(selected.join("ok.json"), b"ok")?;
    let d = Deadline::new(30_000);
    let directory = open_metadata_directory(&selected, &d)?;
    assert_eq!(
        directory
            .read_bytes("ok.json", 2, &d)?
            .expect("normal leaf"),
        b"ok"
    );
    for invalid in [
        "",
        ".",
        "..",
        "../foreign.json",
        "a/b",
        r"a\b",
        "/foreign.json",
        r"C:\foreign.json",
        "C:relative",
        "a:stream",
        "CON",
        "name.",
        "name ",
    ] {
        assert!(
            directory.read_bytes(invalid, 64, &d).is_err(),
            "Invalid leaf accepted"
        );
        assert!(
            directory.entry_info(invalid, &d).is_err(),
            "Invalid stat leaf accepted"
        );
    }
    let oversized = "x".repeat(256);
    assert!(directory.read_bytes(&oversized, 64, &d).is_err());
    assert!(directory.entry_info(&oversized, &d).is_err());
    assert_eq!(
        fs::read(fixture.path().join("foreign.json"))?,
        b"foreign marker"
    );
    Ok(())
}

#[test]
fn metadata_directory_enumeration_preserves_count_bounds_and_direct_names() -> Result<()> {
    // Catches omitted entry limits, unsorted names or recursive enumeration.
    let fixture = tempfile::tempdir()?;
    fs::write(fixture.path().join("a.json"), b"a")?;
    fs::create_dir(fixture.path().join("b"))?;
    fs::write(fixture.path().join("b").join("nested.json"), b"nested")?;
    let d = Deadline::new(30_000);
    let directory = open_metadata_directory(fixture.path(), &d)?;
    assert_eq!(directory.names(2, &d)?, vec!["a.json", "b"]);
    assert!(directory.names(1, &d).is_err());
    assert!(directory.names(usize::MAX, &d).is_err());
    assert!(
        directory
            .entry_info("b", &d)?
            .expect("directory entry")
            .directory
    );
    assert!(directory.read_bytes("b", 64, &d).is_err());
    Ok(())
}

#[test]
fn metadata_directory_byte_bounds_are_raw_bytes_and_empty_files_can_use_zero() -> Result<()> {
    // Catches character-count limits, unchecked size/read bounds or empty errors.
    let fixture = tempfile::tempdir()?;
    fs::write(
        fixture.path().join("unicode.json"),
        [0xf0, 0x9f, 0xa6, 0x80],
    )?;
    fs::write(fixture.path().join("empty.json"), b"")?;
    let d = Deadline::new(30_000);
    let directory = open_metadata_directory(fixture.path(), &d)?;
    assert_eq!(
        directory
            .read_bytes("unicode.json", 4, &d)?
            .expect("four raw bytes"),
        [0xf0, 0x9f, 0xa6, 0x80]
    );
    assert!(directory.read_bytes("unicode.json", 3, &d).is_err());
    assert_eq!(directory.read_bytes("empty.json", 0, &d)?, Some(Vec::new()));
    assert!(directory.read_bytes("missing.json", 0, &d)?.is_none());
    Ok(())
}

#[test]
fn metadata_directory_expired_calls_fail_without_mutation_or_empty_success() -> Result<()> {
    // Catches deadline bypass by a previously opened directory or cached result.
    let fixture = tempfile::tempdir()?;
    fs::write(fixture.path().join("a.json"), b"row")?;
    let d = Deadline::new(30_000);
    let directory = open_metadata_directory(fixture.path(), &d)?;
    assert_eq!(
        directory.read_bytes("a.json", 3, &d)?.expect("normal read"),
        b"row"
    );
    let expired = Deadline::new(0);
    assert!(open_metadata_directory(fixture.path(), &expired).is_err());
    assert!(directory.names(8, &expired).is_err());
    assert!(directory.entry_info("a.json", &expired).is_err());
    assert!(directory.read_bytes("a.json", 3, &expired).is_err());
    assert_eq!(fs::read(fixture.path().join("a.json"))?, b"row");
    assert_eq!(directory.names(1, &d)?, vec!["a.json"]);
    Ok(())
}

#[test]
fn metadata_directory_stat_remains_compatible_with_a_live_exclusive_state_lock() -> Result<()> {
    // Catches using GENERIC_READ or restrictive sharing for metadata-only stat.
    let fixture = tempfile::tempdir()?;
    let d = Deadline::new(30_000);
    let lock = open_state_lock(&fixture.path().join("writer.lock"), &d)?;
    let directory = open_metadata_directory(fixture.path(), &d)?;
    assert_eq!(directory.names(1, &d)?, vec!["writer.lock"]);
    let info = directory
        .entry_info("writer.lock", &d)?
        .expect("live lock metadata");
    assert!(!info.directory);
    assert_eq!(info.length, 0);
    assert!(directory.read_bytes("writer.lock", 0, &d).is_err());
    drop(lock);
    assert_eq!(
        directory.read_bytes("writer.lock", 0, &d)?,
        Some(Vec::new())
    );
    Ok(())
}

#[test]
fn metadata_directory_read_refuses_a_preexisting_foreign_data_writer() -> Result<()> {
    // Catches widening read sharing to allow a foreign DATA_WRITE handle.
    let fixture = tempfile::tempdir()?;
    let path = fixture.path().join("a.json");
    fs::write(&path, b"row")?;
    let d = Deadline::new(30_000);
    let directory = open_metadata_directory(fixture.path(), &d)?;
    assert_eq!(
        directory.read_bytes("a.json", 3, &d)?.expect("normal read"),
        b"row"
    );
    let writer = OpenOptions::new().write(true).share_mode(7).open(&path)?;
    assert!(directory.read_bytes("a.json", 3, &d).is_err());
    assert_eq!(
        directory
            .entry_info("a.json", &d)?
            .expect("metadata-only stat")
            .length,
        3
    );
    drop(writer);
    assert_eq!(
        directory
            .read_bytes("a.json", 3, &d)?
            .expect("after writer closes"),
        b"row"
    );
    assert_eq!(fs::read(&path)?, b"row");
    Ok(())
}

#[test]
fn metadata_directory_read_rechecks_access_after_a_fixture_acl_revocation() -> Result<()> {
    // Catches retaining a previously accepted handle or bypassing current access.
    let fixture = tempfile::tempdir()?;
    let path = fixture.path().join("a.json");
    fs::write(&path, b"row")?;
    make_protected_fixture_dacl(&path)?;
    let d = Deadline::new(30_000);
    let directory = open_metadata_directory(fixture.path(), &d)?;
    assert_eq!(
        directory.read_bytes("a.json", 3, &d)?.expect("normal read"),
        b"row"
    );
    let text: Vec<u16> = "D:P(D;;FR;;;WD)".encode_utf16().chain(Some(0)).collect();
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
        "Owned deny-read fixture descriptor creation failed"
    );
    let changed = unsafe { SetFileSecurityW(wide.as_ptr(), 4 | 0x8000_0000, descriptor) };
    unsafe { LocalFree(descriptor) };
    anyhow::ensure!(changed != 0, "Owned deny-read fixture setup failed");
    let refused = directory.read_bytes("a.json", 3, &d);
    // Restore this owned fixture before assertions/TempDir cleanup.
    make_protected_fixture_dacl(&path)?;
    let error = match refused {
        Err(error) => error,
        Ok(_) => anyhow::bail!("Metadata read reused access from before ACL revocation"),
    };
    assert_eq!(
        error
            .downcast_ref::<std::io::Error>()
            .and_then(std::io::Error::raw_os_error),
        Some(5),
        "ACL refusal must be access denial, not an unrelated failure"
    );
    assert_eq!(
        directory
            .read_bytes("a.json", 3, &d)?
            .expect("restored access"),
        b"row"
    );
    Ok(())
}

#[test]
fn metadata_directory_names_reject_non_utf8_leaf_names_without_silent_omission() -> Result<()> {
    // Catches lossy conversion or treating an unrepresentable child as absent.
    use std::os::windows::ffi::OsStringExt;
    let fixture = tempfile::tempdir()?;
    fs::write(fixture.path().join("ok.json"), b"ok")?;
    let d = Deadline::new(30_000);
    let directory = open_metadata_directory(fixture.path(), &d)?;
    assert_eq!(directory.names(2, &d)?, vec!["ok.json"]);
    let invalid = std::ffi::OsString::from_wide(&[0xd800]);
    fs::write(fixture.path().join(invalid), b"owned invalid-name fixture")?;
    assert!(directory.names(2, &d).is_err());
    assert_eq!(fs::read(fixture.path().join("ok.json"))?, b"ok");
    Ok(())
}

mod metadata_directory_mapping_fixture {
    use std::{
        ffi::c_void,
        fs::OpenOptions,
        os::windows::{fs::OpenOptionsExt, io::AsRawHandle},
        path::Path,
    };

    #[link(name = "Kernel32")]
    unsafe extern "system" {
        fn CreateFileMappingW(
            file: *mut c_void,
            security: *const c_void,
            protection: u32,
            maximum_high: u32,
            maximum_low: u32,
            name: *const u16,
        ) -> *mut c_void;
        fn MapViewOfFile(
            mapping: *mut c_void,
            access: u32,
            offset_high: u32,
            offset_low: u32,
            length: usize,
        ) -> *mut c_void;
        fn FlushViewOfFile(base: *const c_void, length: usize) -> i32;
        fn UnmapViewOfFile(base: *const c_void) -> i32;
        fn CloseHandle(handle: *mut c_void) -> i32;
    }

    // Test-only ABI/constants checked against locally installed winapi0.3.9
    // memoryapi.rs and windows-sys0.61.2 Win32/System/Memory/mod.rs.
    pub(super) struct WritableMapping {
        mapping: *mut c_void,
        base: *mut c_void,
        length: usize,
    }

    impl WritableMapping {
        pub(super) fn new(path: &Path, length: usize) -> anyhow::Result<Self> {
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .share_mode(7)
                .open(path)?;
            anyhow::ensure!(
                length > 0 && file.metadata()?.len() == length as u64,
                "Owned mapping fixture must have exact nonzero length"
            );
            let mapping = unsafe {
                CreateFileMappingW(
                    file.as_raw_handle(),
                    std::ptr::null(),
                    4,
                    0,
                    0,
                    std::ptr::null(),
                )
            };
            anyhow::ensure!(!mapping.is_null(), "Owned writable mapping creation failed");
            let base = unsafe { MapViewOfFile(mapping, 2, 0, 0, length) };
            if base.is_null() {
                unsafe { CloseHandle(mapping) };
                anyhow::bail!("Owned writable mapping view failed");
            }
            // Deliberately close the DATA_WRITE File while its mapping survives.
            drop(file);
            Ok(Self {
                mapping,
                base,
                length,
            })
        }

        pub(super) fn replace(&mut self, bytes: &[u8]) -> anyhow::Result<()> {
            anyhow::ensure!(bytes.len() == self.length, "Mapping fixture cannot resize");
            unsafe {
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), self.base.cast::<u8>(), bytes.len());
            }
            anyhow::ensure!(
                unsafe { FlushViewOfFile(self.base, self.length) } != 0,
                "Owned mapped fixture flush failed"
            );
            Ok(())
        }
    }

    impl Drop for WritableMapping {
        fn drop(&mut self) {
            unsafe {
                UnmapViewOfFile(self.base);
                CloseHandle(self.mapping);
            }
        }
    }
}

#[test]
fn metadata_directory_rereads_bytes_changed_by_a_preexisting_writable_mapping() -> Result<()> {
    // Catches a trusted previous byte/hash snapshot despite surviving mapped writes.
    let fixture = tempfile::tempdir()?;
    let path = fixture.path().join("a.json");
    fs::write(&path, b"AAAA")?;
    let d = Deadline::new(30_000);
    let directory = open_metadata_directory(fixture.path(), &d)?;
    assert_eq!(
        directory.read_bytes("a.json", 4, &d)?.expect("normal read"),
        b"AAAA"
    );
    let mut mapping = metadata_directory_mapping_fixture::WritableMapping::new(&path, 4)?;
    mapping.replace(b"BBBB")?;
    assert_eq!(
        fs::read(&path)?,
        b"BBBB",
        "Mapped positive control must alter real bytes"
    );
    match directory.read_bytes("a.json", 4, &d) {
        Ok(Some(bytes)) => assert_eq!(
            bytes, b"BBBB",
            "Mapped mutation was replaced by cached bytes"
        ),
        Err(error) => assert!(
            error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|error| matches!(error.raw_os_error(), Some(5 | 32 | 1224))),
            "Only a concrete access/sharing/mapped-file refusal is safe here"
        ),
        Ok(None) => anyhow::bail!("An existing mapped row was reported missing"),
    }
    drop(mapping);
    assert_eq!(
        directory
            .read_bytes("a.json", 4, &d)?
            .expect("fresh after unmap"),
        b"BBBB"
    );
    Ok(())
}
