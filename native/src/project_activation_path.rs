//! Held physical path selections. No executable payload is read or hashed.
use crate::{capture::Deadline, model, paths::is_link};
use anyhow::{Context, Result, ensure};
use std::{
    fs::{File, OpenOptions},
    path::{Component, Path, PathBuf},
};

pub(super) struct Pin {
    path: PathBuf,
    identity: String,
    directory: bool,
    held: Vec<(PathBuf, File, String)>,
}
impl Pin {
    pub(super) fn directory(path: &Path, d: &Deadline) -> Result<Self> {
        Self::open(path, true, d)
    }
    pub(super) fn file(path: &Path, d: &Deadline) -> Result<Self> {
        Self::open(path, false, d)
    }
    fn open(path: &Path, directory: bool, d: &Deadline) -> Result<Self> {
        d.check()?;
        ensure!(
            path.as_os_str().len() <= 4096
                && path.components().count() <= 128
                && path.is_absolute()
                && !path
                    .components()
                    .any(|c| matches!(c, Component::ParentDir | Component::CurDir)),
            "activation_absolute_path_required"
        );
        let mut held = Vec::new();
        for (i, ancestor) in path.ancestors().enumerate() {
            d.check()?;
            let meta =
                std::fs::symlink_metadata(ancestor).context("activation_path_unavailable")?;
            let is_dir = i > 0 || directory;
            ensure!(
                !is_link(&meta)
                    && if is_dir {
                        meta.is_dir()
                    } else {
                        meta.is_file()
                    },
                "activation_path_link_or_kind_refused"
            );
            let file = open(ancestor, is_dir)?;
            let id = identity(&file, is_dir)?;
            held.push((ancestor.to_owned(), file, id));
        }
        let canonical = path.canonicalize()?;
        ensure!(
            canonical.as_os_str().len() <= 4096 && canonical.components().count() <= 128,
            "activation_physical_path_capacity"
        );
        let identity = model::identity(&(
            canonical
                .to_str()
                .context("activation_path_utf8_required")?,
            &held[0].2,
        ));
        let pin = Self {
            path: canonical,
            identity,
            directory,
            held,
        };
        pin.check(path, d)?;
        Ok(pin)
    }
    pub(super) fn binding(&self) -> &str {
        &self.identity
    }
    pub(super) fn path(&self) -> &Path {
        &self.path
    }
    pub(super) fn check(&self, selected: &Path, d: &Deadline) -> Result<()> {
        d.check()?;
        ensure!(
            selected.is_absolute() && selected.canonicalize()? == self.path,
            "activation_root_changed"
        );
        for (i, (path, held, expected)) in self.held.iter().enumerate() {
            d.check()?;
            let directory = i > 0 || self.directory;
            let meta = std::fs::symlink_metadata(path)?;
            ensure!(
                !is_link(&meta)
                    && if directory {
                        meta.is_dir()
                    } else {
                        meta.is_file()
                    },
                "activation_path_link_or_kind_refused"
            );
            let current = open(path, directory)?;
            ensure!(
                identity(held, directory)? == *expected
                    && identity(&current, directory)? == *expected,
                "activation_physical_identity_changed"
            );
        }
        Ok(())
    }
}
fn open(path: &Path, directory: bool) -> Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // Read/share-write, no delete sharing: keep selected names physically held.
        options
            .share_mode(1 | 2)
            .custom_flags(0x00200000 | if directory { 0x02000000 } else { 0 });
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | if directory { libc::O_DIRECTORY } else { 0 });
    }
    options
        .open(path)
        .context("activation_path_handle_unavailable")
}
fn identity(file: &File, directory: bool) -> Result<String> {
    let meta = file.metadata()?;
    ensure!(
        if directory {
            meta.is_dir()
        } else {
            meta.is_file()
        },
        "activation_held_path_kind_changed"
    );
    #[cfg(windows)]
    {
        use std::{ffi::c_void, os::windows::io::AsRawHandle};
        #[repr(C)]
        struct Info {
            attributes: u32,
            times: [u32; 6],
            volume: u32,
            size: [u32; 2],
            links: u32,
            index: [u32; 2],
        }
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetFileInformationByHandle(file: *mut c_void, info: *mut Info) -> i32;
        }
        let mut info = Info {
            attributes: 0,
            times: [0; 6],
            volume: 0,
            size: [0; 2],
            links: 0,
            index: [0; 2],
        };
        ensure!(
            unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } != 0,
            "activation_identity_unavailable"
        );
        ensure!(
            info.attributes & 0x400 == 0,
            "activation_reparse_point_refused"
        );
        return Ok(if directory {
            model::identity(&(info.volume, info.index))
        } else {
            model::identity(&(
                info.volume,
                info.index,
                info.size,
                [info.times[4], info.times[5]],
            ))
        });
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Ok(if directory {
            model::identity(&(meta.dev(), meta.ino()))
        } else {
            model::identity(&(
                meta.dev(),
                meta.ino(),
                meta.len(),
                meta.mtime(),
                meta.mtime_nsec(),
            ))
        })
    }
    #[cfg(not(any(windows, unix)))]
    {
        anyhow::bail!("activation_physical_identity_unavailable")
    }
}
