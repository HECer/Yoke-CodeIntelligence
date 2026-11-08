//! Explicit Windows directory normalization; stored policies remain data-only.
use super::{CaptureOptions, Deadline};
use crate::store::Store;
#[cfg(windows)]
use anyhow::Context;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{borrow::Cow, path::Path};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum IgnoredDirectoryIdentity {
    #[serde(rename = "windows-physical-v1")]
    WindowsPhysicalV1,
}

// Captured selectors and source names have already been physically resolved.
// Replay must not depend on the reader's platform case-folding rules.
pub(super) fn physical_prefix(directories: &[String], path: &str) -> bool {
    directories.iter().any(|directory| {
        let mut actual = path.split('/');
        directory
            .split('/')
            .all(|expected| actual.next() == Some(expected))
    })
}

#[cfg(windows)]
fn physical_directory(
    root: &Path,
    relative: &str,
    check: &impl Fn() -> Result<()>,
) -> Result<String> {
    check()?;
    let selected = crate::paths::scoped(root, relative)?;
    let metadata = std::fs::symlink_metadata(&selected)
        .context("Selected physical ignored directory is unavailable")?;
    ensure!(
        metadata.is_dir() && !crate::paths::is_link(&metadata),
        "Selected physical ignored source must be a regular directory"
    );
    check()?;
    let root = root.canonicalize()?;
    let selected = selected.canonicalize()?;
    let relative = selected
        .strip_prefix(root)
        .context("Selected ignored directory escaped workspace")?
        .to_str()
        .context("Selected ignored directory must be UTF-8")?
        .replace('\\', "/");
    // Physical normalization cannot make an alias a protected-source read grant.
    crate::paths::normalized(&relative)?;
    check()?;
    Ok(relative)
}

/// Only explicit live selections are resolved. Historical/archived reads use
/// the exact names already bound into the capture's options and source proof.
pub(super) fn resolve_options<'a>(
    root: &Path,
    store: &Store,
    options: &'a CaptureOptions,
    d: &Deadline,
) -> Result<Cow<'a, CaptureOptions>> {
    if options.ignored_directory_identity.is_none() {
        return Ok(Cow::Borrowed(options));
    }
    d.check()?;
    let resolved = options.validated()?;
    #[cfg(not(windows))]
    {
        let _ = (root, store, resolved);
        anyhow::bail!("Live physical ignored-directory selection requires Windows");
    }
    #[cfg(windows)]
    {
        ensure!(
            root.canonicalize()?
                == store
                    .dir
                    .parent()
                    .context("Missing capture workspace root")?
                    .canonicalize()?,
            "Physical directory selection requires the bound capture workspace"
        );
        let directories = resolved
            .include_ignored_dirs
            .iter()
            .map(|directory| physical_directory(root, directory, &|| d.check()))
            .collect::<Result<Vec<_>>>()?;
        let resolved = CaptureOptions {
            include_ignored_dirs: directories,
            ..resolved
        }
        .validated()?;
        d.check()?;
        Ok(Cow::Owned(resolved))
    }
}

fn check_write_prefixes(
    store: &Store,
    directories: &[String],
    path: &str,
    check: &impl Fn() -> Result<()>,
    message: &'static str,
) -> Result<()> {
    check()?;
    crate::paths::normalized(path)?;
    ensure!(!physical_prefix(directories, path), message);
    #[cfg(not(windows))]
    {
        let _ = store;
        anyhow::bail!("Live physical ignored-directory write checks require Windows");
    }
    #[cfg(windows)]
    {
        let root = store.dir.parent().context("Missing write workspace root")?;
        let canonical = super::canonical_write_path(store, path)?;
        for directory in directories {
            let selected = physical_directory(root, directory, check)?;
            ensure!(
                !physical_prefix(std::slice::from_ref(&selected), &canonical),
                message
            );
        }
        check()?;
        Ok(())
    }
}

/// Independent invocation restriction, never recovered from a stored plan.
pub(crate) fn check_current_write(store: &Store, path: &str, d: &Deadline) -> Result<()> {
    let Some(directories) = &store.current_physical_ignored_dirs else {
        return Ok(());
    };
    check_write_prefixes(
        store,
        directories,
        path,
        &|| d.check(),
        "ignored_directory_is_read_only_in_current_host_selection",
    )
}

pub(super) fn check_captured_write(
    store: &Store,
    directories: &[String],
    path: &str,
) -> Result<()> {
    check_write_prefixes(
        store,
        directories,
        path,
        &|| store.check_deadline(),
        "captured_physical_ignored_directory_is_read_only",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stored_physical_names_are_literal_and_preserve_unicode_and_component_boundaries() {
        for (selected, other) in [("ß", "SS"), ("ﬃ", "FFI"), ("İ", "i"), ("é", "É")] {
            let directories = vec![format!("src/{selected}")];
            assert!(physical_prefix(
                &directories,
                &format!("src/{selected}/a.rs")
            ));
            assert!(!physical_prefix(&directories, &format!("src/{other}/a.rs")));
            assert!(!physical_prefix(
                &directories,
                &format!("src/{selected}-extra/a.rs")
            ));
        }
        let directories = vec!["src/ignored".into()];
        assert!(!physical_prefix(&directories, "SRC/IGNORED/a.rs"));
        assert!(!physical_prefix(&directories, "src/ignore/a.rs"));
        assert!(!physical_prefix(&directories, "other/src/ignored/a.rs"));
    }
}
