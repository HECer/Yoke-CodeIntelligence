//! Explicit host Git index observations. They are not captured source authority.
use super::{HostGit, oid_valid};
use crate::{capture::Deadline, model::identity, paths};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct Limits {
    pub max_repositories: usize,
    pub max_entries: usize,
    pub max_index_bytes: usize,
    pub max_depth: usize,
    pub max_output_bytes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_repositories: 64,
            max_entries: 20_000,
            max_index_bytes: 8_000_000,
            max_depth: 16,
            max_output_bytes: 1_000_000,
        }
    }
}
impl Limits {
    pub fn validate(&self) -> Result<()> {
        let hard = Self::default();
        ensure!(
            self.max_repositories <= hard.max_repositories
                && self.max_entries <= hard.max_entries
                && self.max_index_bytes <= hard.max_index_bytes
                && self.max_depth <= hard.max_depth
                && self.max_output_bytes <= 4_000_000,
            "repository_scope_invalid_limits"
        );
        Ok(())
    }
}
/// Rust/CLI host inputs. A serialized report cannot construct these grants.
#[derive(Default)]
pub struct Options {
    pub allow_index_read: bool,
    pub follow_submodules: bool,
    pub follow_nested_repositories: bool,
    pub limits: Limits,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Row {
    path: String,
    tag: String,
    mode: Option<String>,
    object_id: Option<String>,
    stage: Option<u8>,
    directory: bool,
}
fn parse_checked(
    raw: &[u8],
    width: usize,
    maximum: usize,
    check: &impl Fn() -> Result<()>,
) -> Result<Vec<Row>> {
    ensure!(
        matches!(width, 40 | 64),
        "repository_scope_protocol_invalid"
    );
    ensure!(
        raw.is_empty() || raw.last() == Some(&0),
        "repository_scope_protocol_invalid"
    );
    if raw.is_empty() {
        return Ok(Vec::new());
    }
    let mut rows = Vec::new();
    let mut seen = BTreeSet::new();
    for bytes in raw[..raw.len() - 1].split(|b| *b == 0) {
        check()?;
        ensure!(rows.len() < maximum, "repository_scope_capacity_exceeded");
        let text = std::str::from_utf8(bytes).context("repository_scope_protocol_invalid")?;
        ensure!(
            text.len() >= 3 && text.as_bytes()[1] == b' ',
            "repository_scope_protocol_invalid"
        );
        let tag = &text[..1];
        let (path, mode, object_id, stage) = if tag == "?" {
            (&text[2..], None, None, None)
        } else {
            ensure!(
                matches!(tag, "H" | "S" | "M"),
                "repository_scope_protocol_invalid"
            );
            let (info, path) = text[2..]
                .split_once('\t')
                .context("repository_scope_protocol_invalid")?;
            let fields = info.split(' ').collect::<Vec<_>>();
            ensure!(
                fields.len() == 3
                    && matches!(fields[0], "100644" | "100755" | "120000" | "160000")
                    && fields[1].len() == width
                    && oid_valid(fields[1])
                    && matches!(fields[2], "0" | "1" | "2" | "3"),
                "repository_scope_protocol_invalid"
            );
            (
                path,
                Some(fields[0].to_owned()),
                Some(fields[1].to_owned()),
                Some(fields[2].parse::<u8>()?),
            )
        };
        let directory = tag == "?" && path.ends_with('/');
        let path = if directory {
            &path[..path.len() - 1]
        } else {
            path
        };
        paths::normalized_syntax(path).context("repository_scope_protocol_invalid")?;
        ensure!(
            seen.insert((path.to_owned(), stage)),
            "repository_scope_protocol_invalid"
        );
        rows.push(Row {
            path: path.into(),
            tag: tag.into(),
            mode,
            object_id,
            stage,
            directory,
        });
    }
    rows.sort_by(|a, b| (&a.path, a.stage).cmp(&(&b.path, b.stage)));
    for pair in rows.windows(2) {
        if pair[0].path == pair[1].path {
            ensure!(
                pair[0].stage.is_some_and(|s| s > 0) && pair[1].stage.is_some_and(|s| s > 0),
                "repository_scope_protocol_invalid"
            );
        }
    }
    Ok(rows)
}

fn parse(raw: &[u8], width: usize, maximum: usize, d: &Deadline) -> Result<Vec<Row>> {
    parse_checked(raw, width, maximum, &|| d.check())
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Repository {
    path: String,
    object_format: String,
    index_hash: String,
    /// Exact NUL-framed metadata, never Git blob or working source bytes.
    index_listing: String,
    rows: Vec<Row>,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Boundary {
    path: String,
    owner_repository: String,
    kind: String,
    follow_requested: bool,
    state: String,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct File {
    path: String,
    owner_repository: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Omission {
    path: String,
    reason: String,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Data {
    producer: String,
    root: String,
    follow_submodules: bool,
    follow_nested_repositories: bool,
    repositories: Vec<Repository>,
    boundaries: Vec<Boundary>,
    files: Vec<File>,
    omissions: Vec<Omission>,
    limits: Limits,
    acquired_index_bytes: usize,
    complete: bool,
    source_access_granted: bool,
    state_paths_excluded: bool,
    capture_policy_published: bool,
}
/// Created only by a current explicit host inspection; no Deserialize authority.
#[derive(Debug)]
pub struct Observation {
    value: serde_json::Value,
    bytes: Vec<u8>,
}
impl Observation {
    pub fn value(&self) -> &serde_json::Value {
        &self.value
    }
    pub fn canonical_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// Pure stored metadata validation. It never resolves a historical path or
/// creates an Observation capable of starting a fresh capture.
pub(crate) fn capture_parts(
    value: &serde_json::Value,
    check: &impl Fn() -> Result<()>,
) -> Result<(Vec<String>, Vec<String>, String)> {
    check()?;
    ensure!(
        serde_json::to_vec(value)?.len() <= 4_000_000,
        "repository_scope_capacity_exceeded"
    );
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Envelope {
        id: String,
        observation: Data,
    }
    let record: Envelope = serde_json::from_value(value.clone())?;
    let data = record.observation;
    data.limits.validate()?;
    ensure!(
        serde_json::to_vec(value)?.len() <= data.limits.max_output_bytes,
        "repository_scope_stored_proof_invalid"
    );
    ensure!(
        data.producer == "yoke-git-repository-scope-observation-1"
            && !data.complete
            && !data.source_access_granted
            && data.state_paths_excluded
            && !data.capture_policy_published
            && record.id == identity(&("native_repository_scope_observation_v1", &data)),
        "repository_scope_stored_proof_invalid"
    );
    ensure!(
        serde_json::json!({"id":record.id,"observation":data}) == *value,
        "repository_scope_stored_proof_invalid"
    );
    ensure!(
        (data.root.starts_with('/')
            || data.root.starts_with("\\\\")
            || (data.root.as_bytes().get(1) == Some(&b':')
                && data
                    .root
                    .as_bytes()
                    .first()
                    .is_some_and(u8::is_ascii_alphabetic)
                && data
                    .root
                    .as_bytes()
                    .get(2)
                    .is_some_and(|b| matches!(*b, b'/' | b'\\'))))
            && data.root.len() <= 4096
            && !data.root.chars().any(char::is_control),
        "repository_scope_stored_proof_invalid"
    );
    ensure!(
        !data.repositories.is_empty()
            && data.repositories.len() <= data.limits.max_repositories
            && data.repositories[0].path == ".",
        "repository_scope_stored_proof_invalid"
    );
    let mut repositories = BTreeMap::new();
    let mut metadata_rows = BTreeMap::<(&str, &str), Vec<&Row>>::new();
    let mut entries = 0usize;
    let mut acquired = 0usize;
    for repository in &data.repositories {
        check()?;
        if repository.path != "." {
            paths::normalized(&repository.path)?;
        }
        ensure!(
            repositories
                .insert(repository.path.as_str(), repository)
                .is_none(),
            "repository_scope_stored_proof_invalid"
        );
        let width = match repository.object_format.as_str() {
            "sha1" => 40,
            "sha256" => 64,
            _ => anyhow::bail!("repository_scope_stored_proof_invalid"),
        };
        let rows = parse_checked(
            repository.index_listing.as_bytes(),
            width,
            data.limits.max_entries.saturating_sub(entries),
            check,
        )?;
        entries += rows.len();
        acquired += 2 * repository.index_listing.len();
        for row in &repository.rows {
            metadata_rows
                .entry((&repository.path, &row.path))
                .or_default()
                .push(row);
        }
        ensure!(
            repository.rows == rows
                && repository.index_hash == crate::model::hash(repository.index_listing.as_bytes()),
            "repository_scope_stored_proof_invalid"
        );
    }
    ensure!(
        acquired == data.acquired_index_bytes && acquired <= data.limits.max_index_bytes,
        "repository_scope_stored_proof_invalid"
    );
    let local_path = |owner: &str, path: &str| -> Result<String> {
        paths::normalized(path)?;
        if owner == "." {
            Ok(path.into())
        } else {
            Ok(path
                .strip_prefix(&format!("{owner}/"))
                .context("repository_scope_stored_proof_invalid")?
                .into())
        }
    };
    let mut boundary_paths = BTreeSet::new();
    let mut followed = BTreeSet::new();
    for boundary in &data.boundaries {
        check()?;
        ensure!(
            repositories.contains_key(boundary.owner_repository.as_str()),
            "repository_scope_stored_proof_invalid"
        );
        let local = local_path(&boundary.owner_repository, &boundary.path)?;
        let rows = metadata_rows
            .get(&(boundary.owner_repository.as_str(), local.as_str()))
            .cloned()
            .unwrap_or_default();
        let gitlink = rows.iter().any(|r| r.mode.as_deref() == Some("160000"));
        let nested = rows.iter().any(|r| r.directory);
        let requested = if gitlink {
            data.follow_submodules
        } else {
            data.follow_nested_repositories
        };
        ensure!(
            (gitlink || nested)
                && boundary.kind
                    == if gitlink {
                        "gitlink"
                    } else {
                        "nested-clone-candidate"
                    }
                && boundary.follow_requested == requested
                && boundary_paths.insert(boundary.path.as_str()),
            "repository_scope_stored_proof_invalid"
        );
        match boundary.state.as_str() {
            "followed" => {
                ensure!(
                    requested
                        && repositories.contains_key(boundary.path.as_str())
                        && rows.iter().all(|r| r.stage.is_none_or(|s| s == 0)),
                    "repository_scope_stored_proof_invalid"
                );
                followed.insert(boundary.path.as_str());
            }
            "not-followed" => ensure!(!requested, "repository_scope_stored_proof_invalid"),
            "uninitialized-or-marker-missing" | "marker-not-regular" => {
                ensure!(requested, "repository_scope_stored_proof_invalid")
            }
            "unmerged-gitlink" => ensure!(
                gitlink && rows.iter().any(|r| r.stage.is_some_and(|s| s != 0)),
                "repository_scope_stored_proof_invalid"
            ),
            _ => anyhow::bail!("repository_scope_stored_proof_invalid"),
        }
    }
    ensure!(
        repositories
            .keys()
            .all(|p| *p == "." || followed.contains(p)),
        "repository_scope_stored_proof_invalid"
    );
    for path in &followed {
        let mut current = *path;
        let mut depth = 0;
        while current != "." {
            depth += 1;
            ensure!(
                depth <= data.limits.max_depth,
                "repository_scope_stored_proof_invalid"
            );
            let parent = data
                .boundaries
                .iter()
                .find(|b| b.path == current && b.state == "followed")
                .context("repository_scope_stored_proof_invalid")?;
            current = &parent.owner_repository;
        }
    }
    let mut files = BTreeSet::new();
    for file in &data.files {
        check()?;
        let local = local_path(&file.owner_repository, &file.path)?;
        ensure!(
            repositories.contains_key(file.owner_repository.as_str()),
            "repository_scope_stored_proof_invalid"
        );
        ensure!(
            files.insert(file.path.clone())
                && !boundary_paths.contains(file.path.as_str())
                && metadata_rows
                    .get(&(file.owner_repository.as_str(), local.as_str()))
                    .is_some_and(|rows| rows
                        .iter()
                        .any(|r| !r.directory && r.mode.as_deref() != Some("160000"))),
            "repository_scope_stored_proof_invalid"
        );
    }
    for omission in &data.omissions {
        check()?;
        paths::normalized_syntax(&omission.path)?;
        ensure!(
            matches!(
                omission.reason.as_str(),
                "protected-path"
                    | "working-file-missing"
                    | "link-or-reparse-point"
                    | "not-regular-file"
            ),
            "repository_scope_stored_proof_invalid"
        );
    }
    Ok((
        files.into_iter().collect(),
        followed.into_iter().map(str::to_owned).collect(),
        data.root,
    ))
}
fn root_guard(root: &Path, d: &Deadline) -> Result<PathBuf> {
    d.check()?;
    ensure!(
        root.is_absolute()
            && !root
                .components()
                .any(|p| matches!(p, std::path::Component::ParentDir)),
        "repository_scope_absolute_root_required"
    );
    let text = root.to_str().context("repository_scope_root_invalid")?;
    ensure!(
        text.len() <= 4096 && !text.chars().any(char::is_control),
        "repository_scope_root_invalid"
    );
    for ancestor in root.ancestors() {
        d.check()?;
        let meta = fs::symlink_metadata(ancestor)?;
        ensure!(
            meta.is_dir() && !paths::is_link(&meta),
            "repository_scope_root_link_refused"
        );
        if let Some(name) = ancestor.file_name().and_then(|n| n.to_str()) {
            ensure!(!paths::protected(name), "repository_scope_protected_root");
        }
    }
    Ok(root.canonicalize()?)
}
struct Inspect<'a> {
    root: &'a Path,
    remaining_index_bytes: &'a std::cell::Cell<usize>,
    git: &'a HostGit,
    d: &'a Deadline,
    options: &'a Options,
    data: Data,
    active: BTreeSet<PathBuf>,
    output_reserved: usize,
    entries: usize,
}
impl Inspect<'_> {
    fn reserve(&mut self, value: &impl Serialize) -> Result<()> {
        self.output_reserved = self
            .output_reserved
            .checked_add(serde_json::to_vec(value)?.len() + 1)
            .context("repository_scope_capacity_exceeded")?;
        ensure!(
            self.output_reserved <= self.options.limits.max_output_bytes,
            "repository_scope_capacity_exceeded"
        );
        Ok(())
    }
    fn omit(&mut self, path: String, reason: &str) -> Result<()> {
        let row = Omission {
            path,
            reason: reason.into(),
        };
        self.reserve(&row)?;
        self.data.omissions.push(row);
        Ok(())
    }
    fn listing(&mut self, root: &Path) -> Result<Vec<u8>> {
        let left = self.remaining_index_bytes.get();
        ensure!(left > 0, "repository_scope_capacity_exceeded");
        let raw = self.git.run(
            root,
            &[
                "ls-files",
                "-t",
                "--stage",
                "--cached",
                "--others",
                "--exclude-standard",
                "-z",
                "--",
                ".",
                ":(exclude,icase,glob)**/.yoke/**",
                ":(exclude,icase,glob)**/.yoke",
            ],
            &[],
            left.min(4_000_000),
            self.d,
        )?;
        self.remaining_index_bytes.set(left - raw.len());
        self.data.acquired_index_bytes += raw.len();
        Ok(raw)
    }
    fn walk(&mut self, relative: &str, depth: usize) -> Result<()> {
        self.d.check()?;
        ensure!(
            depth <= self.options.limits.max_depth
                && self.data.repositories.len() < self.options.limits.max_repositories,
            "repository_scope_capacity_exceeded"
        );
        let selected = if relative == "." {
            self.root.to_owned()
        } else {
            paths::scoped(self.root, relative)?
        };
        let root = root_guard(&selected, self.d)?;
        ensure!(
            root.starts_with(self.root) && self.active.insert(root.clone()),
            "repository_scope_root_changed"
        );
        let top = self
            .git
            .run(&root, &["rev-parse", "--show-toplevel"], &[], 8192, self.d)?;
        let top = std::str::from_utf8(&top)?.trim_end_matches(['\r', '\n']);
        ensure!(
            Path::new(top).is_absolute() && root_guard(Path::new(top), self.d)? == root,
            "repository_scope_root_changed"
        );
        let format = self.git.run(
            &root,
            &["rev-parse", "--show-object-format"],
            &[],
            64,
            self.d,
        )?;
        let format = std::str::from_utf8(&format)?.trim_end_matches(['\r', '\n']);
        let width = match format {
            "sha1" => 40,
            "sha256" => 64,
            _ => anyhow::bail!("repository_scope_protocol_invalid"),
        };
        let raw = self.listing(&root)?;
        let rows = parse(
            &raw,
            width,
            self.options.limits.max_entries.saturating_sub(self.entries),
            self.d,
        )?;
        self.entries += rows.len();
        let mut grouped = BTreeMap::<String, Vec<&Row>>::new();
        for row in &rows {
            grouped.entry(row.path.clone()).or_default().push(row);
        }
        let repository = Repository {
            path: relative.into(),
            object_format: format.into(),
            index_hash: crate::model::hash(&raw),
            index_listing: std::str::from_utf8(&raw)?.into(),
            rows: rows.clone(),
        };
        self.reserve(&repository)?;
        self.data.repositories.push(repository);
        for (path, rows) in grouped {
            self.d.check()?;
            let full = if relative == "." {
                path.clone()
            } else {
                format!("{relative}/{path}")
            };
            if paths::protected(&full) {
                self.omit(full, "protected-path")?;
                continue;
            }
            let selected = paths::scoped(self.root, &full)?;
            let gitlink = rows.iter().any(|r| r.mode.as_deref() == Some("160000"));
            let nested = rows.iter().any(|r| r.directory);
            if gitlink || nested {
                let follow = if gitlink {
                    self.options.follow_submodules
                } else {
                    self.options.follow_nested_repositories
                };
                let mut boundary = Boundary {
                    path: full.clone(),
                    owner_repository: relative.into(),
                    kind: if gitlink {
                        "gitlink"
                    } else {
                        "nested-clone-candidate"
                    }
                    .into(),
                    follow_requested: follow,
                    state: "not-followed".into(),
                };
                if rows.iter().any(|r| r.stage.is_some_and(|s| s != 0)) {
                    boundary.state = "unmerged-gitlink".into();
                } else if follow {
                    let marker = fs::symlink_metadata(selected.join(".git"));
                    match marker {
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                            boundary.state = "uninitialized-or-marker-missing".into()
                        }
                        Err(e) => return Err(e.into()),
                        Ok(m) if paths::is_link(&m) || !(m.is_file() || m.is_dir()) => {
                            boundary.state = "marker-not-regular".into()
                        }
                        Ok(_) => {
                            self.walk(&full, depth + 1)?;
                            boundary.state = "followed".into();
                        }
                    }
                }
                self.reserve(&boundary)?;
                self.data.boundaries.push(boundary);
                continue;
            }
            let metadata = match fs::symlink_metadata(&selected) {
                Ok(m) => m,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    self.omit(full, "working-file-missing")?;
                    continue;
                }
                Err(e) => return Err(e.into()),
            };
            if paths::is_link(&metadata) {
                self.omit(full, "link-or-reparse-point")?;
                continue;
            }
            if !metadata.is_file() {
                self.omit(full, "not-regular-file")?;
                continue;
            }
            let file = File {
                path: full,
                owner_repository: relative.into(),
            };
            self.reserve(&file)?;
            self.data.files.push(file);
        }
        let checked = self.listing(&root)?;
        ensure!(
            raw == checked && root_guard(&selected, self.d)? == root,
            "repository_scope_index_changed"
        );
        Ok(())
    }
}
pub fn inspect(root: &Path, git: &HostGit, options: &Options, d: &Deadline) -> Result<Observation> {
    let remaining = std::cell::Cell::new(options.limits.max_index_bytes);
    inspect_budgeted(root, git, options, d, &remaining)
}
fn inspect_budgeted(
    root: &Path,
    git: &HostGit,
    options: &Options,
    d: &Deadline,
    remaining: &std::cell::Cell<usize>,
) -> Result<Observation> {
    ensure!(
        options.allow_index_read,
        "repository_scope_index_permission_required"
    );
    options.limits.validate()?;
    d.check()?;
    let root = root_guard(root, d)?;
    ensure!(
        options.limits.max_output_bytes >= 2048,
        "repository_scope_capacity_exceeded"
    );
    let mut state = Inspect {
        root: &root,
        remaining_index_bytes: remaining,
        git,
        d,
        options,
        active: BTreeSet::new(),
        output_reserved: 2048,
        entries: 0,
        data: Data {
            producer: "yoke-git-repository-scope-observation-1".into(),
            root: root
                .to_str()
                .context("repository_scope_root_invalid")?
                .into(),
            follow_submodules: options.follow_submodules,
            follow_nested_repositories: options.follow_nested_repositories,
            repositories: Vec::new(),
            boundaries: Vec::new(),
            files: Vec::new(),
            omissions: Vec::new(),
            limits: options.limits.clone(),
            acquired_index_bytes: 0,
            complete: false,
            source_access_granted: false,
            state_paths_excluded: true,
            capture_policy_published: false,
        },
    };
    state.walk(".", 0)?;
    state.data.repositories.sort_by(|a, b| a.path.cmp(&b.path));
    state.data.boundaries.sort_by(|a, b| a.path.cmp(&b.path));
    state.data.files.sort_by(|a, b| a.path.cmp(&b.path));
    state.data.omissions.sort_by(|a, b| a.path.cmp(&b.path));
    let id = identity(&("native_repository_scope_observation_v1", &state.data));
    let value = serde_json::json!({"id":id,"observation":state.data});
    let bytes = serde_json::to_vec(&value)?;
    ensure!(
        bytes.len() <= options.limits.max_output_bytes,
        "repository_scope_capacity_exceeded"
    );
    d.check()?;
    Ok(Observation { value, bytes })
}

/// Explicit host working-source capture. Immutable evidence replays without Git;
/// current Git authority is required again for each new capture.
pub fn capture(
    engine: &mut crate::Engine,
    git: &HostGit,
    options: &Options,
    d: &Deadline,
) -> Result<(crate::model::Snapshot, crate::model::Graph, usize, usize)> {
    ensure!(
        engine.capture_options.repository_scope.is_none() && engine.store.db.is_autocommit(),
        "repository_capture_requires_fresh_host_selection"
    );
    let remaining = std::cell::Cell::new(options.limits.max_index_bytes);
    let observation = inspect_budgeted(&engine.root, git, options, d, &remaining)?;
    let selected = crate::capture::RepositoryScope::physical(&engine.root, &observation, d)?;
    capture_selected(engine, git, options, d, &remaining, selected)
}
/// Preflight all current metadata and physical selections before creating state.
pub fn capture_root(
    root: &Path,
    git: &HostGit,
    options: &Options,
    d: &Deadline,
) -> Result<(crate::model::Snapshot, crate::model::Graph, usize, usize)> {
    let remaining = std::cell::Cell::new(options.limits.max_index_bytes);
    let observation = inspect_budgeted(root, git, options, d, &remaining)?;
    let selected = crate::capture::RepositoryScope::physical(root, &observation, d)?;
    let mut engine = crate::Engine::open_with_timeout(root, d.remaining())?;
    capture_selected(&mut engine, git, options, d, &remaining, selected)
}
fn capture_selected(
    engine: &mut crate::Engine,
    git: &HostGit,
    options: &Options,
    d: &Deadline,
    remaining: &std::cell::Cell<usize>,
    selected: crate::capture::RepositoryScope,
) -> Result<(crate::model::Snapshot, crate::model::Graph, usize, usize)> {
    engine.sync_source_exclusions(d)?;
    engine.store.set_deadline(d)?;
    let mut effective = engine.capture_options.clone();
    effective.repository_scope = Some(selected.clone());
    let recheck = || -> Result<()> {
        let current = inspect_budgeted(&engine.root, git, options, d, remaining)?;
        let current = crate::capture::RepositoryScope::physical(&engine.root, &current, d)?;
        ensure!(
            current == selected,
            "repository_capture_index_or_membership_changed"
        );
        Ok(())
    };
    let snapshot = crate::capture::capture_repository_options_at(
        engine.capture_epoch,
        &engine.root,
        &engine.store,
        &effective,
        d,
        &recheck,
    )?;
    let result = engine.analyze_snapshot_at(engine.capture_epoch, snapshot, d)?;
    engine.store.db.execute("INSERT INTO meta(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",rusqlite::params![crate::freshness::BASELINE_KEY,result.0.id])?;
    d.check()?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stored_metadata_replays_without_a_live_root_and_rehashed_forgeries_still_refuse() {
        let raw = format!("H 100644 {} 0\tmain.rs\0", "a".repeat(40));
        let data = Data {
            producer: "yoke-git-repository-scope-observation-1".into(),
            root: "/offline-metadata-root".into(),
            follow_submodules: false,
            follow_nested_repositories: false,
            repositories: vec![Repository {
                path: ".".into(),
                object_format: "sha1".into(),
                index_hash: crate::model::hash(raw.as_bytes()),
                index_listing: raw.clone(),
                rows: parse(raw.as_bytes(), 40, 1, &Deadline::new(5000)).unwrap(),
            }],
            boundaries: Vec::new(),
            files: vec![File {
                path: "main.rs".into(),
                owner_repository: ".".into(),
            }],
            omissions: Vec::new(),
            limits: Limits::default(),
            acquired_index_bytes: 2 * raw.len(),
            complete: false,
            source_access_granted: false,
            state_paths_excluded: true,
            capture_policy_published: false,
        };
        let id = identity(&("native_repository_scope_observation_v1", &data));
        let base = serde_json::json!({"id":id,"observation":data});
        assert_eq!(capture_parts(&base, &|| Ok(())).unwrap().0, ["main.rs"]);
        let rehash = |value: &mut serde_json::Value| {
            let data: Data = serde_json::from_value(value["observation"].clone()).unwrap();
            value["id"] = identity(&("native_repository_scope_observation_v1", &data)).into();
        };
        for case in 0..6 {
            let mut bad = base.clone();
            match case {
                0 => bad["observation"]["sourceAccessGranted"] = true.into(),
                1 => {
                    bad["observation"]["repositories"][0]["indexListing"] = "? ../outside\0".into()
                }
                2 => {
                    bad["observation"]["repositories"][0]["rows"][0]["objectId"] =
                        "b".repeat(40).into()
                }
                3 => bad["observation"]["files"][0]["path"] = "uncaptured.rs".into(),
                4 => bad["observation"]["acquiredIndexBytes"] = (2 * raw.len() + 1).into(),
                5 => bad["observation"]["statePathsExcluded"] = false.into(),
                _ => unreachable!(),
            }
            rehash(&mut bad);
            assert!(capture_parts(&bad, &|| Ok(())).is_err(), "forgery {case}");
        }
        assert!(capture_parts(&base, &|| anyhow::bail!("expired parent request")).is_err());
    }
    #[test]
    fn strict_nul_protocol_preserves_parallel_stages_unicode_and_gitlink_modes() {
        let raw = format!(
            "H 160000 {} 0\tsub/child\0M 100644 {} 1\tcafé.rs\0M 100644 {} 2\tcafé.rs\0? nested π/\0",
            "a".repeat(40),
            "b".repeat(40),
            "c".repeat(40)
        );
        let rows = parse(raw.as_bytes(), 40, 4, &Deadline::new(5000)).unwrap();
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0].stage, Some(1));
        assert_eq!(rows[1].stage, Some(2));
        assert!(rows[2].directory);
        assert_eq!(rows[3].mode.as_deref(), Some("160000"));
    }
    #[test]
    fn malformed_unknown_duplicate_traversal_and_expired_protocols_refuse() {
        let oid = "a".repeat(40);
        for raw in [
            "\0".into(),
            "? path".into(),
            "? ../outside\0".into(),
            "? name//\0".into(),
            "? /absolute\0".into(),
            "? .\\escape\0".into(),
            "? a\nname\0".into(),
            "? one\0? one\0".into(),
            format!("X 100644 {oid} 0\tfile\0"),
            format!("H 040000 {oid} 0\tfile\0"),
            format!("H 100644 {oid} 4\tfile\0"),
            format!("H 100644 {oid} 0\tfile\0M 100644 {oid} 1\tfile\0"),
        ] {
            assert!(
                parse(raw.as_bytes(), 40, 100, &Deadline::new(5000)).is_err(),
                "{raw:?}"
            );
        }
        assert!(parse(b"? file\0", 40, 0, &Deadline::new(5000)).is_err());
        assert!(parse(b"? file\0", 40, 1, &Deadline::new(0)).is_err());
        assert!(parse(b"? \xff\0", 40, 1, &Deadline::new(5000)).is_err());
        assert!(parse(b"", 41, 1, &Deadline::new(5000)).is_err());
    }
}
