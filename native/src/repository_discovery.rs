//! Host-selected, bounded marker discovery. Candidates never grant source authority.
use crate::{capture::Deadline, paths};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct Options {
    pub max_depth: usize,
    pub max_entries: usize,
    pub max_directories: usize,
    pub max_repositories: usize,
    pub max_report_bytes: usize,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            max_depth: 4,
            max_entries: 20_000,
            max_directories: 2_000,
            max_repositories: 256,
            max_report_bytes: 1_000_000,
        }
    }
}
impl Options {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.max_depth <= 32
                && (1..=100_000).contains(&self.max_entries)
                && (1..=10_000).contains(&self.max_directories)
                && self.max_repositories <= 1_000
                && (1..=4_000_000).contains(&self.max_report_bytes),
            "repository_discovery_invalid_limits"
        );
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Marker {
    GitDirectory,
    GitFile,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Candidate {
    /// "." denotes the explicit search root; other values are relative paths.
    pub path: String,
    pub marker: Marker,
    /// Nearest ancestor candidate, not a proven Git superproject relationship.
    pub parent: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Omission {
    pub path: String,
    pub reason: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Report {
    pub producer: String,
    pub search_root: String,
    pub options: Options,
    pub candidates: Vec<Candidate>,
    pub omissions: Vec<Omission>,
    pub entries_visited: usize,
    pub directories_visited: usize,
    /// Marker metadata does not prove repository validity, source coverage or freshness.
    pub complete: bool,
    pub registration_granted: bool,
    pub source_access_granted: bool,
}
impl Report {
    /// Exact selection from this report only. This is metadata, never host registration.
    pub fn select(&self, path: &str) -> Result<&Candidate> {
        if path != "." {
            paths::normalized(path)?;
        }
        self.candidates
            .iter()
            .find(|candidate| candidate.path == path)
            .context("repository_discovery_candidate_not_found")
    }
}

struct Scan<'a> {
    root: &'a Path,
    deadline: &'a Deadline,
    report: Report,
    reserved_bytes: usize,
}
impl Scan<'_> {
    fn reserve(&mut self, value: &impl Serialize) -> Result<()> {
        self.reserved_bytes = self
            .reserved_bytes
            .checked_add(serde_json::to_vec(value)?.len() + 1)
            .context("repository_discovery_capacity_exceeded")?;
        ensure!(
            self.reserved_bytes <= self.report.options.max_report_bytes,
            "repository_discovery_capacity_exceeded"
        );
        Ok(())
    }
    fn omit(&mut self, path: String, reason: &str) -> Result<()> {
        let omission = Omission {
            path,
            reason: reason.into(),
        };
        self.reserve(&omission)?;
        self.report.omissions.push(omission);
        Ok(())
    }
    fn directory(&mut self, relative: &str, depth: usize, parent: Option<&str>) -> Result<()> {
        self.deadline.check()?;
        ensure!(
            self.report.directories_visited < self.report.options.max_directories,
            "repository_discovery_capacity_exceeded"
        );
        let directory = if relative == "." {
            self.root.to_owned()
        } else {
            paths::scoped(self.root, relative)?
        };
        let metadata = fs::symlink_metadata(&directory)?;
        ensure!(
            metadata.is_dir() && !paths::is_link(&metadata),
            "repository_discovery_directory_changed"
        );
        ensure!(
            directory.canonicalize()?.starts_with(self.root),
            "repository_discovery_directory_changed"
        );
        self.report.directories_visited += 1;
        let marker_path = if relative == "." {
            ".git".to_owned()
        } else {
            format!("{relative}/.git")
        };
        self.deadline.check()?;
        let marker = match fs::symlink_metadata(directory.join(".git")) {
            Ok(metadata) if paths::is_link(&metadata) => {
                self.omit(marker_path, "marker-link-or-reparse-point")?;
                None
            }
            Ok(metadata) if metadata.is_dir() => Some(Marker::GitDirectory),
            Ok(metadata) if metadata.is_file() => Some(Marker::GitFile),
            Ok(_) => {
                self.omit(marker_path, "marker-not-regular")?;
                None
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        let next_parent = if let Some(marker) = marker {
            ensure!(
                self.report.candidates.len() < self.report.options.max_repositories,
                "repository_discovery_capacity_exceeded"
            );
            let candidate = Candidate {
                path: relative.into(),
                marker,
                parent: parent.map(str::to_owned),
            };
            self.reserve(&candidate)?;
            self.report.candidates.push(candidate);
            Some(relative)
        } else {
            parent
        };
        // Bound enumeration before sorting. read_dir order is not a stable selector.
        let mut children = Vec::new();
        for entry in fs::read_dir(&directory)? {
            self.deadline.check()?;
            ensure!(
                self.report.entries_visited < self.report.options.max_entries,
                "repository_discovery_capacity_exceeded"
            );
            self.report.entries_visited += 1;
            children.push(entry?);
        }
        children.sort_by_key(|entry| entry.file_name());
        for entry in children {
            self.deadline.check()?;
            let name = entry.file_name();
            let name = name
                .to_str()
                .context("repository_discovery_non_utf8_path")?;
            // The marker's contents and its children are never inspected.
            if name == ".git" || cfg!(windows) && name.eq_ignore_ascii_case(".git") {
                continue;
            }
            let path = if relative == "." {
                name.to_owned()
            } else {
                format!("{relative}/{name}")
            };
            if paths::normalized_syntax(&path).is_err() {
                self.omit(path, "unsupported-path")?;
                continue;
            }
            if paths::protected(&path) {
                self.omit(path, "protected-path")?;
                continue;
            }
            let metadata = fs::symlink_metadata(entry.path())?;
            if paths::is_link(&metadata) {
                self.omit(path, "link-or-reparse-point")?;
            } else if metadata.is_dir() {
                if depth == self.report.options.max_depth {
                    self.omit(path, "depth-limit")?;
                } else {
                    self.directory(&path, depth + 1, next_parent)?;
                }
            }
        }
        Ok(())
    }
}

/// Inspect names and marker metadata below one explicit host directory. No Store,
/// Git command, marker/config/source read, import, registration or activation occurs.
/// Metadata checks are best effort against concurrent filesystem replacement.
pub fn discover(root: &Path, options: &Options, deadline: &Deadline) -> Result<Report> {
    options.validate()?;
    deadline.check()?;
    ensure!(
        root.is_absolute()
            && !root
                .components()
                .any(|part| matches!(part, std::path::Component::ParentDir)),
        "repository_discovery_explicit_absolute_root_required"
    );
    for ancestor in root.ancestors() {
        deadline.check()?;
        let metadata = fs::symlink_metadata(ancestor)?;
        ensure!(
            metadata.is_dir() && !paths::is_link(&metadata),
            "repository_discovery_root_link_refused"
        );
        if let Some(name) = ancestor.file_name().and_then(|name| name.to_str()) {
            ensure!(
                !paths::protected(name),
                "repository_discovery_protected_root"
            );
        }
    }
    let root = root.canonicalize()?;
    let search_root = root
        .to_str()
        .context("repository_discovery_non_utf8_path")?
        .to_owned();
    let reserved_bytes = 1024 + serde_json::to_vec(&search_root)?.len();
    ensure!(
        reserved_bytes <= options.max_report_bytes,
        "repository_discovery_capacity_exceeded"
    );
    let mut scan = Scan {
        root: &root,
        deadline,
        reserved_bytes,
        report: Report {
            producer: "yoke-repository-marker-discovery-1".into(),
            search_root,
            options: options.clone(),
            candidates: Vec::new(),
            omissions: Vec::new(),
            entries_visited: 0,
            directories_visited: 0,
            complete: false,
            registration_granted: false,
            source_access_granted: false,
        },
    };
    scan.directory(".", 0, None)?;
    scan.report.candidates.sort_by(|a, b| a.path.cmp(&b.path));
    scan.report.omissions.sort_by(|a, b| a.path.cmp(&b.path));
    ensure!(
        serde_json::to_vec(&scan.report)?.len() <= options.max_report_bytes,
        "repository_discovery_capacity_exceeded"
    );
    deadline.check()?;
    Ok(scan.report)
}
