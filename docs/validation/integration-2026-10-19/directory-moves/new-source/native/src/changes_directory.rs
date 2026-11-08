//! Checked physical membership for a single captured directory relocation.
//! Still unpublished until the caller binds it into one guarded change plan.
use super::{Plan, PlannedFile};
use crate::{
    capture::Deadline,
    model::{Snapshot, SourceFile, identity, valid_id},
    paths::{is_link, normalized, scoped},
    store::Store,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

const INVALID: &str = "changes_directory_invalid";
const STALE: &str = "changes_directory_stale";
const CAPACITY: &str = "changes_directory_capacity_exceeded";
const MAX_FILES: usize = 1000;
const MAX_DIRECTORIES: usize = 2000;
const MAX_BYTES: usize = 16_000_000;
#[cfg(test)]
thread_local! {
    static CANCEL_AFTER_RENAME: std::cell::RefCell<Option<std::sync::Arc<std::sync::atomic::AtomicBool>>> = const {std::cell::RefCell::new(None)};
}
#[cfg(test)]
#[path = "changes_directory_tests.rs"]
mod tests;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DirectoryBinding {
    path: String,
    identity: String,
    mode: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DirectoryMove {
    source: String,
    destination: String,
    source_parent_identity: String,
    destination_parent_identity: String,
    directories: Vec<DirectoryBinding>,
    #[serde(deserialize_with = "strict_files")]
    files: Vec<SourceFile>,
}
fn strict_files<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Vec<SourceFile>, D::Error> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct File {
        path: String,
        hash: String,
        size: usize,
        language: String,
        mode: u32,
    }
    Ok(Vec::<File>::deserialize(deserializer)?
        .into_iter()
        .map(|file| SourceFile {
            path: file.path,
            hash: file.hash,
            size: file.size,
            language: file.language,
            mode: file.mode,
        })
        .collect())
}

/// Only a successful host filesystem and captured-source selection creates this.
#[derive(Clone)]
pub(crate) struct SelectedDirectoryMove(DirectoryMove);
impl SelectedDirectoryMove {
    pub(crate) fn record(&self) -> &DirectoryMove {
        &self.0
    }
}

fn below(path: &str, parent: &str) -> bool {
    path.strip_prefix(parent)
        .is_some_and(|suffix| suffix.starts_with('/'))
}
fn parent(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(parent, _)| parent)
}
fn mapped(path: &str, source: &str, destination: &str) -> Result<String> {
    ensure!(path == source || below(path, source), INVALID);
    Ok(format!("{destination}{}", &path[source.len()..]))
}
fn missing(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(false),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(error) => Err(error.into()),
    }
}
fn mode(metadata: &fs::Metadata) -> u32 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o777
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        0o644
    }
}
fn directory_binding(root: &Path, relative: &str, d: &Deadline) -> Result<DirectoryBinding> {
    d.check()?;
    let path = if relative.is_empty() {
        root.to_path_buf()
    } else {
        scoped(root, relative)?
    };
    let metadata = fs::symlink_metadata(&path)?;
    ensure!(metadata.is_dir() && !is_link(&metadata), INVALID);
    ensure!(
        path.canonicalize()? == root.canonicalize()?.join(relative),
        "changes_directory_noncanonical_host_spelling"
    );
    let identity = directory_identity(&path)?;
    d.check()?;
    Ok(DirectoryBinding {
        path: relative.into(),
        identity,
        mode: mode(&metadata),
    })
}
#[cfg(unix)]
fn directory_identity(path: &Path) -> Result<String> {
    use std::os::unix::fs::MetadataExt;
    let file = fs::File::open(path)?;
    let metadata = file.metadata()?;
    ensure!(metadata.is_dir(), INVALID);
    Ok(identity(&(
        "directory-unix-v1",
        metadata.dev(),
        metadata.ino(),
    )))
}
#[cfg(windows)]
fn directory_identity(path: &Path) -> Result<String> {
    use std::{
        ffi::c_void,
        os::windows::{fs::OpenOptionsExt, io::AsRawHandle},
    };
    // FILE_ID_INFO retains the full128-bit ID on both NTFS and ReFS.
    #[repr(C)]
    struct FileId {
        volume: u64,
        id: [u8; 16],
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetFileInformationByHandleEx(
            handle: *mut c_void,
            class: i32,
            data: *mut c_void,
            size: u32,
        ) -> i32;
    }
    let file = fs::OpenOptions::new()
        .read(true)
        .share_mode(7)
        .custom_flags(0x0200_0000 | 0x0020_0000)
        .open(path)?;
    let metadata = file.metadata()?;
    ensure!(metadata.is_dir() && !is_link(&metadata), INVALID);
    let mut id = FileId {
        volume: 0,
        id: [0; 16],
    };
    ensure!(
        unsafe {
            GetFileInformationByHandleEx(
                file.as_raw_handle(),
                18,
                std::ptr::from_mut(&mut id).cast(),
                std::mem::size_of::<FileId>() as u32,
            )
        } != 0,
        INVALID
    );
    Ok(identity(&("directory-windows-v1", id.volume, id.id)))
}
#[cfg(not(any(unix, windows)))]
fn directory_identity(_path: &Path) -> Result<String> {
    anyhow::bail!("changes_directory_platform_unavailable")
}

fn admit_files(files: &[SourceFile], d: &Deadline) -> Result<()> {
    ensure!(
        !files.is_empty(),
        "changes_directory_empty_source_unsupported"
    );
    ensure!(files.len() <= MAX_FILES, CAPACITY);
    let mut bytes = 0usize;
    let mut previous = None;
    for file in files {
        d.check()?;
        ensure!(
            normalized(&file.path)? == file.path
                && valid_id(&file.hash)
                && file.size <= 4_000_000
                && file.mode <= 0o777
                && previous.is_none_or(|p: &str| p < file.path.as_str()),
            INVALID
        );
        previous = Some(file.path.as_str());
        bytes = bytes.checked_add(file.size).context(CAPACITY)?;
        ensure!(bytes <= MAX_BYTES, CAPACITY);
    }
    Ok(())
}

impl DirectoryMove {
    pub(crate) fn source(&self) -> &str {
        &self.source
    }
    pub(crate) fn destination(&self) -> &str {
        &self.destination
    }
    pub(crate) fn files(&self) -> &[SourceFile] {
        &self.files
    }
    pub(crate) fn source_for_destination(&self, path: &str) -> Option<&SourceFile> {
        let suffix = path
            .strip_prefix(&self.destination)
            .filter(|suffix| suffix.starts_with('/'))?;
        let source = format!("{}{suffix}", self.source);
        self.files
            .binary_search_by(|file| file.path.cmp(&source))
            .ok()
            .map(|index| &self.files[index])
    }
    pub(crate) fn destination_path(&self, original: &str) -> Result<String> {
        mapped(original, &self.source, &self.destination)
    }
    pub(crate) fn validate(&self, base: &Snapshot, d: &Deadline) -> Result<()> {
        d.check()?;
        ensure!(
            normalized(&self.source)? == self.source
                && normalized(&self.destination)? == self.destination
                && !self.source.eq_ignore_ascii_case(&self.destination)
                && !below(
                    &self.source.to_ascii_lowercase(),
                    &self.destination.to_ascii_lowercase()
                )
                && !below(
                    &self.destination.to_ascii_lowercase(),
                    &self.source.to_ascii_lowercase()
                )
                && valid_id(&self.destination_parent_identity)
                && valid_id(&self.source_parent_identity),
            INVALID
        );
        admit_files(&self.files, d)?;
        let expected: Vec<_> = base
            .files
            .iter()
            .filter(|f| below(&f.path, &self.source))
            .collect();
        ensure!(
            expected.len() == self.files.len()
                && expected.iter().zip(&self.files).all(|(a, b)| *a == b),
            INVALID
        );
        ensure!(
            !self.directories.is_empty() && self.directories.len() <= MAX_DIRECTORIES,
            CAPACITY
        );
        let mut previous = None;
        let mut names = BTreeSet::new();
        let mut ids = BTreeSet::new();
        for directory in &self.directories {
            d.check()?;
            ensure!(
                normalized(&directory.path)? == directory.path
                    && (directory.path == self.source || below(&directory.path, &self.source))
                    && valid_id(&directory.identity)
                    && directory.mode <= 0o777
                    && previous.is_none_or(|p: &str| p < directory.path.as_str())
                    && ids.insert(&directory.identity),
                INVALID
            );
            previous = Some(directory.path.as_str());
            names.insert(directory.path.as_str());
        }
        ensure!(
            self.directories[0].path == self.source
                && !ids.contains(&self.destination_parent_identity)
                && !ids.contains(&self.source_parent_identity),
            INVALID
        );
        for directory in self.directories.iter().skip(1) {
            ensure!(names.contains(parent(&directory.path)), INVALID);
        }
        for file in &self.files {
            ensure!(
                names.contains(parent(&file.path)) && !names.contains(file.path.as_str()),
                INVALID
            );
        }
        d.check()
    }
    pub(crate) fn validate_result(
        &self,
        base: &Snapshot,
        result: &Snapshot,
        d: &Deadline,
    ) -> Result<()> {
        self.validate(base, d)?;
        ensure!(
            !base
                .files
                .iter()
                .any(|file| file.path == self.destination || below(&file.path, &self.destination))
                && !result
                    .files
                    .iter()
                    .any(|file| file.path == self.source || below(&file.path, &self.source)),
            INVALID
        );
        let projected = self.projected_files(result, d)?;
        ensure!(
            result
                .files
                .iter()
                .filter(|file| below(&file.path, &self.destination))
                .count()
                == projected.len(),
            INVALID
        );
        for (before, after) in self.files.iter().zip(projected) {
            ensure!(before.language == after.language, INVALID);
        }
        Ok(())
    }
    pub(crate) fn select(
        root: &Path,
        store: &Store,
        base: &Snapshot,
        source: &str,
        destination: &str,
        d: &Deadline,
    ) -> Result<SelectedDirectoryMove> {
        let source = normalized(source)?;
        let destination = normalized(destination)?;
        let scope = crate::capture::source_scope_policy(store, base)?;
        let options = crate::capture::options_for_snapshot(store, base)?;
        ensure!(
            !options
                .include_ignored_dirs
                .iter()
                .any(|selected| selected == &source || below(selected, &source)),
            "changes_directory_ignored_capture_root_move_unavailable"
        );
        scope.check_host_write_scope(&source)?;
        scope.check_host_write_scope(&destination)?;
        ensure!(
            missing(&scoped(root, &destination)?)?,
            "changes_directory_destination_occupied"
        );
        let destination_parent = directory_binding(root, parent(&destination), d)?;
        let source_parent = directory_binding(root, parent(&source), d)?;
        let files: Vec<_> = base
            .files
            .iter()
            .filter(|f| below(&f.path, &source))
            .cloned()
            .collect();
        admit_files(&files, d)?;
        for file in &files {
            scope.file(&file.path)?;
            scope.check_host_write_scope(&file.path)?;
            scope.check_host_write_scope(&mapped(&file.path, &source, &destination)?)?;
        }
        let directories = inventory(root, &source, &files, None, d)?;
        let record = Self {
            source,
            destination,
            source_parent_identity: source_parent.identity,
            destination_parent_identity: destination_parent.identity,
            directories,
            files,
        };
        record.validate(base, d)?;
        // All membership and byte admission precedes original-file body reads.
        check_bodies(root, &record.files, d)?;
        record.check_before(root, d)?;
        Ok(SelectedDirectoryMove(record))
    }
    pub(crate) fn check_before(&self, root: &Path, d: &Deadline) -> Result<()> {
        ensure!(missing(&scoped(root, &self.destination)?)?, STALE);
        let target_parent = directory_binding(root, parent(&self.destination), d)?;
        let source_parent = directory_binding(root, parent(&self.source), d)?;
        ensure!(
            target_parent.identity == self.destination_parent_identity
                && source_parent.identity == self.source_parent_identity,
            STALE
        );
        inventory(root, &self.source, &self.files, Some(&self.directories), d)?;
        check_bodies(root, &self.files, d)
    }
    pub(crate) fn projected_files(
        &self,
        snapshot: &Snapshot,
        d: &Deadline,
    ) -> Result<Vec<SourceFile>> {
        let mut files = Vec::with_capacity(self.files.len());
        for original in &self.files {
            d.check()?;
            let path = self.destination_path(&original.path)?;
            let file = snapshot
                .files
                .iter()
                .find(|f| f.path == path)
                .context(INVALID)?;
            ensure!(file.mode == original.mode, INVALID);
            files.push(file.clone());
        }
        admit_files(&files, d)?;
        Ok(files)
    }
    pub(crate) fn check_after(&self, root: &Path, result: &Snapshot, d: &Deadline) -> Result<()> {
        self.check_parents(root, d)?;
        ensure!(missing(&scoped(root, &self.source)?)?, STALE);
        let files = self.projected_files(result, d)?;
        let directories = self.projected_directories()?;
        inventory(root, &self.destination, &files, Some(&directories), d)?;
        check_bodies(root, &files, d)
    }
    fn check_parents(&self, root: &Path, d: &Deadline) -> Result<()> {
        ensure!(
            directory_binding(root, parent(&self.source), d)?.identity
                == self.source_parent_identity
                && directory_binding(root, parent(&self.destination), d)?.identity
                    == self.destination_parent_identity,
            STALE
        );
        Ok(())
    }
    pub(crate) fn rename(&self, root: &Path, d: &Deadline) -> Result<()> {
        self.check_before(root, d)?;
        fs::rename(
            scoped(root, &self.source)?,
            scoped(root, &self.destination)?,
        )?;
        sync_parents(root, &self.source, &self.destination)?;
        #[cfg(test)]
        CANCEL_AFTER_RENAME.with(|slot| {
            if let Some(flag) = slot.borrow().as_ref() {
                flag.store(true, std::sync::atomic::Ordering::SeqCst);
            }
        });
        d.check()
    }
    pub(crate) fn handles(&self, path: &str) -> bool {
        self.files
            .binary_search_by(|file| file.path.as_str().cmp(path))
            .is_ok()
            || self.source_for_destination(path).is_some()
    }
    pub(crate) fn apply_file(
        &self,
        store: &Store,
        root: &Path,
        file: &PlannedFile,
        d: &Deadline,
    ) -> Result<bool> {
        d.check()?;
        if self
            .files
            .binary_search_by(|source| source.path.cmp(&file.path))
            .is_ok()
        {
            ensure!(missing(&scoped(root, &file.path)?)?, STALE);
            return Ok(true);
        }
        if let Some(original) = self.source_for_destination(&file.path) {
            let write = PlannedFile {
                path: file.path.clone(),
                before_hash: Some(original.hash.clone()),
                after_hash: file.after_hash.clone(),
                mode: original.mode,
            };
            ensure!(super::matches_file(root, &write, false)?, STALE);
            if write.before_hash != write.after_hash {
                super::write_file(store, root, &write, true)?;
            }
            return Ok(true);
        }
        Ok(false)
    }
    pub(crate) fn materialize_directories(&self, root: &Path, d: &Deadline) -> Result<()> {
        let entries = self.projected_directories()?;
        for entry in &entries {
            d.check()?;
            let path = scoped(root, &entry.path)?;
            fs::create_dir_all(&path)?;
            let metadata = fs::symlink_metadata(&path)?;
            ensure!(metadata.is_dir() && !is_link(&metadata), INVALID);
        }
        #[cfg(unix)]
        for entry in entries.iter().rev() {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(
                scoped(root, &entry.path)?,
                fs::Permissions::from_mode(entry.mode),
            )?;
        }
        d.check()
    }
    pub(crate) fn check_materialized(
        &self,
        root: &Path,
        result: &Snapshot,
        d: &Deadline,
    ) -> Result<()> {
        ensure!(missing(&scoped(root, &self.source)?)?, STALE);
        let files = self.projected_files(result, d)?;
        let actual = inventory(root, &self.destination, &files, None, d)?;
        let expected = self.projected_directories()?;
        ensure!(
            actual.len() == expected.len()
                && actual
                    .iter()
                    .zip(&expected)
                    .all(|(a, b)| a.path == b.path && a.mode == b.mode),
            STALE
        );
        check_bodies(root, &files, d)
    }
    pub(crate) fn recover(
        &self,
        store: &Store,
        root: &Path,
        plan: &Plan,
        d: &Deadline,
    ) -> Result<()> {
        self.check_parents(root, d)?;
        let source_missing = missing(&scoped(root, &self.source)?)?;
        let destination_missing = missing(&scoped(root, &self.destination)?)?;
        ensure!(
            source_missing != destination_missing,
            "changes_directory_recovery_conflict"
        );
        if destination_missing {
            return self.check_before(root, d);
        }
        let result = store.snapshot(&plan.result_snapshot_id)?;
        let after = self.projected_files(&result, d)?;
        let mut before = Vec::with_capacity(self.files.len());
        let mut observed = Vec::with_capacity(self.files.len());
        let mut total = 0usize;
        for original in &self.files {
            d.check()?;
            let path = self.destination_path(&original.path)?;
            let actual = fs::symlink_metadata(scoped(root, &path)?)?;
            ensure!(
                actual.is_file() && !is_link(&actual) && actual.len() <= 4_000_000,
                STALE
            );
            total = total
                .checked_add(usize::try_from(actual.len())?)
                .context(CAPACITY)?;
            ensure!(total <= 2 * MAX_BYTES, CAPACITY);
            before.push(SourceFile {
                path: path.clone(),
                ..original.clone()
            });
            observed.push(SourceFile {
                path,
                size: actual.len() as usize,
                ..original.clone()
            });
        }
        inventory(
            root,
            &self.destination,
            &observed,
            Some(&self.projected_directories()?),
            d,
        )?;
        // Admit every file as original or planned bytes before the first restore.
        for (before, after) in before.iter().zip(&after) {
            let path = scoped(root, &before.path)?;
            ensure!(
                crate::verify::file_integrity(before, &path, d)?
                    || crate::verify::file_integrity(after, &path, d)?,
                "changes_directory_foreign_write_preserved"
            );
        }
        for (before, after) in before.iter().zip(&after).rev() {
            d.check()?;
            let path = scoped(root, &before.path)?;
            if crate::verify::file_integrity(before, &path, d)? {
                continue;
            }
            ensure!(
                crate::verify::file_integrity(after, &path, d)?,
                "changes_directory_foreign_write_preserved"
            );
            super::write_file(
                store,
                root,
                &PlannedFile {
                    path: before.path.clone(),
                    before_hash: Some(before.hash.clone()),
                    after_hash: Some(after.hash.clone()),
                    mode: before.mode,
                },
                false,
            )?;
        }
        inventory(
            root,
            &self.destination,
            &before,
            Some(&self.projected_directories()?),
            d,
        )?;
        check_bodies(root, &before, d)?;
        ensure!(missing(&scoped(root, &self.source)?)?, STALE);
        self.check_parents(root, d)?;
        fs::rename(
            scoped(root, &self.destination)?,
            scoped(root, &self.source)?,
        )?;
        sync_parents(root, &self.source, &self.destination)?;
        self.check_before(root, d)
    }
    fn projected_directories(&self) -> Result<Vec<DirectoryBinding>> {
        self.directories
            .iter()
            .map(|entry| {
                Ok(DirectoryBinding {
                    path: mapped(&entry.path, &self.source, &self.destination)?,
                    ..entry.clone()
                })
            })
            .collect()
    }
}

fn sync_parents(root: &Path, source: &str, destination: &str) -> Result<()> {
    #[cfg(unix)]
    {
        let mut parents = BTreeSet::new();
        parents.insert(parent(source));
        parents.insert(parent(destination));
        for parent in parents {
            let path = if parent.is_empty() {
                root.to_path_buf()
            } else {
                scoped(root, parent)?
            };
            fs::File::open(path)?.sync_all()?;
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (root, source, destination);
    }
    Ok(())
}

/// Inventories every entry; ignored, protected, linked and uncaptured files refuse.
/// The expected complete directory set also preserves otherwise invisible empty dirs.
fn inventory(
    root: &Path,
    start: &str,
    expected_files: &[SourceFile],
    expected_dirs: Option<&[DirectoryBinding]>,
    d: &Deadline,
) -> Result<Vec<DirectoryBinding>> {
    let expected: BTreeMap<_, _> = expected_files
        .iter()
        .map(|file| (file.path.as_str(), file))
        .collect();
    let mut queue = vec![start.to_owned()];
    let mut directories = Vec::new();
    let mut seen = BTreeSet::new();
    while let Some(relative) = queue.pop() {
        d.check()?;
        ensure!(directories.len() < MAX_DIRECTORIES, CAPACITY);
        let binding = directory_binding(root, &relative, d)?;
        directories.push(binding);
        for entry in fs::read_dir(scoped(root, &relative)?)? {
            d.check()?;
            let entry = entry?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| anyhow::anyhow!(INVALID))?;
            let path = normalized(&format!("{relative}/{name}"))?;
            let metadata = fs::symlink_metadata(entry.path())?;
            ensure!(!is_link(&metadata), INVALID);
            if metadata.is_dir() {
                ensure!(queue.len() + directories.len() < MAX_DIRECTORIES, CAPACITY);
                queue.push(path);
            } else {
                let file = expected
                    .get(path.as_str())
                    .context("changes_directory_uncaptured_entry")?;
                ensure!(
                    metadata.is_file()
                        && metadata.len() == file.size as u64
                        && mode(&metadata) == file.mode
                        && seen.insert(path),
                    STALE
                );
                ensure!(seen.len() <= MAX_FILES, CAPACITY);
            }
        }
    }
    ensure!(seen.len() == expected.len(), STALE);
    directories.sort_by(|a, b| a.path.cmp(&b.path));
    if let Some(expected) = expected_dirs {
        ensure!(directories == expected, STALE);
    }
    Ok(directories)
}
fn check_bodies(root: &Path, files: &[SourceFile], d: &Deadline) -> Result<()> {
    for file in files {
        ensure!(
            crate::verify::file_integrity(file, &scoped(root, &file.path)?, d)?,
            STALE
        );
    }
    d.check()
}
