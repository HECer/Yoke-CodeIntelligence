//! Pin an explicit local host input and its directory chain before reading bytes.
use crate::{capture::Deadline, paths::is_link};
use anyhow::{Context, Result, ensure};
#[cfg(windows)]
use std::fs::OpenOptions;
use std::{
    fs::{self, File},
    io::{Read, Seek},
    path::{Component, Path, PathBuf},
};

struct Guard {
    path: PathBuf,
    file: File,
    identity: (u64, u64),
    directory: bool,
    content: Option<ContentState>,
}
#[derive(PartialEq, Eq)]
struct ContentState {
    bytes: u64,
    modified: std::time::SystemTime,
    #[cfg(unix)]
    changed: (i64, i64),
}
fn content_state(meta: &fs::Metadata) -> Result<ContentState> {
    Ok(ContentState {
        bytes: meta.len(),
        modified: meta.modified()?,
        #[cfg(unix)]
        changed: {
            use std::os::unix::fs::MetadataExt;
            (meta.ctime(), meta.ctime_nsec())
        },
    })
}
struct Input {
    held: Vec<Guard>,
}
fn open(parent: Option<&File>, path: &Path, directory: bool) -> Result<File> {
    #[cfg(unix)]
    {
        use std::{
            ffi::CString,
            os::{
                fd::{AsRawFd, FromRawFd},
                unix::ffi::OsStrExt,
            },
        };
        let selected = if parent.is_some() {
            Path::new(path.file_name().context("Declared path component absent")?)
        } else {
            path
        };
        let name = CString::new(selected.as_os_str().as_bytes())?;
        let flags = libc::O_RDONLY
            | libc::O_CLOEXEC
            | libc::O_NOFOLLOW
            | libc::O_NONBLOCK
            | if directory { libc::O_DIRECTORY } else { 0 };
        let fd = unsafe {
            libc::openat(
                parent.map_or(libc::AT_FDCWD, |p| p.as_raw_fd()),
                name.as_ptr(),
                flags,
            )
        };
        ensure!(
            fd >= 0,
            "Declared selected handle could not be opened without links"
        );
        Ok(unsafe { File::from_raw_fd(fd) })
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        let _ = parent;
        let mut options = OpenOptions::new();
        // Deny write/delete sharing while the complete selected name chain is held.
        options
            .read(true)
            .share_mode(1)
            .custom_flags(0x0020_0000 | if directory { 0x0200_0000 } else { 0 });
        Ok(options.open(path)?)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (parent, path, directory);
        anyhow::bail!("Declared host-file identity unsupported on this platform")
    }
}
#[cfg(all(test, windows))]
mod windows_tests {
    use super::*;
    #[test]
    fn selected_names_deny_write_and_delete_until_all_pins_are_released() {
        let root = tempfile::TempDir::new().unwrap();
        let parent = root.path().join("selected");
        fs::create_dir(&parent).unwrap();
        let path = parent.join("input.json");
        fs::write(&path, b"original").unwrap();
        let pin = prepare(&path, 100, &Deadline::new(30000), &mut || Ok(())).unwrap();
        assert!(OpenOptions::new().write(true).open(&path).is_err());
        assert!(fs::rename(&path, parent.join("moved.json")).is_err());
        assert!(fs::rename(&parent, root.path().join("moved-parent")).is_err());
        pin.check(&Deadline::new(30000)).unwrap();
        drop(pin);
        fs::write(&path, b"reviewed").unwrap();
        assert_eq!(
            read(&path, 100, &Deadline::new(30000)).unwrap(),
            b"reviewed"
        );
    }
}
fn identity(file: &File, directory: bool) -> Result<(u64, u64)> {
    let meta = file.metadata()?;
    ensure!(
        !is_link(&meta)
            && if directory {
                meta.is_dir()
            } else {
                meta.is_file()
            },
        "Declared selected handle is a link or wrong kind"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        ensure!(
            directory || meta.nlink() == 1,
            "Declared host input has multiple links"
        );
        Ok((meta.dev(), meta.ino()))
    }
    #[cfg(windows)]
    {
        use std::{ffi::c_void, os::windows::io::AsRawHandle};
        #[repr(C)]
        #[derive(Default)]
        struct Info {
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
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetFileInformationByHandle(file: *mut c_void, info: *mut Info) -> i32;
        }
        let mut info = Info::default();
        ensure!(
            unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } != 0,
            "Declared selected file identity unavailable"
        );
        ensure!(
            info.attributes & 0x400 == 0
                && (directory || info.links == 1)
                && (info.index_high != 0 || info.index_low != 0),
            "Declared selected reparse, multiply linked or unidentified file refused"
        );
        Ok((
            u64::from(info.volume),
            (u64::from(info.index_high) << 32) | u64::from(info.index_low),
        ))
    }
    #[cfg(not(any(unix, windows)))]
    {
        anyhow::bail!("Declared host-file identity unsupported on this platform")
    }
}
impl Input {
    fn check(&self, deadline: &Deadline) -> Result<()> {
        for (index, guard) in self.held.iter().enumerate() {
            deadline.check()?;
            let metadata = fs::symlink_metadata(&guard.path)?;
            ensure!(
                !is_link(&metadata),
                "Declared selected path changed to a link"
            );
            let parent = index.checked_sub(1).map(|i| &self.held[i].file);
            let current = open(parent, &guard.path, guard.directory)?;
            ensure!(
                identity(&guard.file, guard.directory)? == guard.identity
                    && identity(&current, guard.directory)? == guard.identity,
                "Declared selected physical identity changed"
            );
            if let Some(expected) = &guard.content {
                ensure!(
                    content_state(&guard.file.metadata()?)? == *expected
                        && content_state(&current.metadata()?)? == *expected,
                    "Declared selected content changed during read"
                );
            }
        }
        Ok(())
    }
}
fn prepare(
    path: &Path,
    max: usize,
    deadline: &Deadline,
    before_leaf: &mut dyn FnMut() -> Result<()>,
) -> Result<Input> {
    deadline.check()?;
    let path = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    };
    let path: PathBuf = path
        .components()
        .filter(|component| !matches!(component, Component::CurDir))
        .map(|component| component.as_os_str())
        .collect();
    ensure!(
        path.to_str().is_some_and(|s| s.len() <= 32768)
            && path.components().count() <= 128
            && !path.components().any(|c| matches!(c, Component::ParentDir)),
        "Declared host input path capacity or parent traversal refused"
    );
    #[cfg(windows)]
    {
        use std::path::Prefix;
        let local = match path.components().next() {
            Some(Component::Prefix(prefix)) => match prefix.kind() {
                Prefix::Disk(_) | Prefix::VerbatimDisk(_) => true,
                Prefix::Verbatim(name) => name.to_str().is_some_and(|s| {
                    s.starts_with("Volume{") && s.ends_with('}') && !s.contains(['\\', '/'])
                }),
                _ => false,
            },
            _ => false,
        };
        ensure!(local, "Declared host input requires a local disk path");
    }
    let parent = path.parent().context("Declared host input parent absent")?;
    let mut directories: Vec<_> = parent.ancestors().map(Path::to_owned).collect();
    directories.reverse();
    let mut held: Vec<Guard> = Vec::new();
    for directory in directories {
        deadline.check()?;
        let file = open(held.last().map(|p| &p.file), &directory, true)?;
        let id = identity(&file, true)?;
        held.push(Guard {
            path: directory,
            file,
            identity: id,
            directory: true,
            content: None,
        });
    }
    let before = fs::symlink_metadata(&path)?;
    ensure!(
        before.is_file() && !is_link(&before) && before.len() <= max as u64,
        "Declared host input must be a bounded regular file"
    );
    before_leaf()?;
    let file = open(held.last().map(|p| &p.file), &path, false)?;
    let id = identity(&file, false)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        ensure!(
            id == (before.dev(), before.ino()),
            "Declared input identity changed before opening"
        );
    }
    ensure!(
        file.metadata()?.len() == before.len(),
        "Declared host input changed before read"
    );
    let content = content_state(&file.metadata()?)?;
    ensure!(
        content == content_state(&before)?,
        "Declared input content changed before opening"
    );
    held.push(Guard {
        path,
        file,
        identity: id,
        directory: false,
        content: Some(content),
    });
    let selected = Input { held };
    selected.check(deadline)?;
    Ok(selected)
}
pub(super) fn read(path: &Path, max: usize, deadline: &Deadline) -> Result<Vec<u8>> {
    read_with_hook(path, max, deadline, &mut || Ok(()))
}
pub(super) fn read_with_hook(
    path: &Path,
    max: usize,
    deadline: &Deadline,
    after_chunk: &mut dyn FnMut() -> Result<()>,
) -> Result<Vec<u8>> {
    let mut input = prepare(path, max, deadline, &mut || Ok(()))?;
    let bytes = read_pass(
        input
            .held
            .last_mut()
            .context("Declared host input handle absent")?,
        max,
        deadline,
        after_chunk,
    )?;
    input.check(deadline)?;
    let held = input
        .held
        .last_mut()
        .context("Declared host input handle absent")?;
    held.file.rewind()?;
    let repeated = read_pass(held, max, deadline, &mut || Ok(()))?;
    ensure!(
        bytes == repeated,
        "Declared input is not a stable original document"
    );
    input.check(deadline)?;
    Ok(bytes)
}
fn read_pass(
    held: &mut Guard,
    max: usize,
    deadline: &Deadline,
    after_chunk: &mut dyn FnMut() -> Result<()>,
) -> Result<Vec<u8>> {
    let file = &mut held.file;
    let size = file.metadata()?.len();
    let mut bytes = Vec::new();
    let mut chunk = [0; 64000];
    loop {
        deadline.check()?;
        let count = file.read(&mut chunk)?;
        if count == 0 {
            break;
        }
        ensure!(
            bytes.len().saturating_add(count) <= max,
            "Declared host input grew beyond capacity"
        );
        bytes.extend_from_slice(&chunk[..count]);
        after_chunk()?;
    }
    ensure!(
        bytes.len() as u64 == size && file.metadata()?.len() == size,
        "Declared host input size changed during read"
    );
    Ok(bytes)
}
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    #[test]
    fn replacement_after_metadata_cannot_select_a_symlink_target_or_swapped_parent() {
        use std::os::unix::fs::symlink;
        let root = tempfile::TempDir::new().unwrap();
        let selected = root.path().join("selected");
        fs::create_dir(&selected).unwrap();
        let path = selected.join("input.json");
        let other = root.path().join("other.json");
        fs::write(&path, b"original").unwrap();
        fs::write(&other, b"unselect").unwrap();
        assert!(
            prepare(&path, 100, &Deadline::new(30000), &mut || {
                fs::remove_file(&path)?;
                symlink(&other, &path)?;
                Ok(())
            })
            .is_err()
        );
        fs::remove_file(&path).unwrap();
        fs::write(&path, b"original").unwrap();
        let moved = root.path().join("moved");
        let unselected = root.path().join("unselected");
        fs::create_dir(&unselected).unwrap();
        fs::write(unselected.join("input.json"), b"unselect").unwrap();
        assert!(
            prepare(&path, 100, &Deadline::new(30000), &mut || {
                fs::rename(&selected, &moved)?;
                symlink(&unselected, &selected)?;
                Ok(())
            })
            .is_err()
        );
        assert_eq!(fs::read(&other).unwrap(), b"unselect");
    }
}
