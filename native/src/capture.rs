use crate::{
    model::{Omission, Snapshot, SourceFile, identity},
    paths::{is_link, normalized, protected},
    store::Store,
};
use anyhow::{Context, Result, bail, ensure};
use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::{
    fs,
    io::Read,
    path::Path,
    time::{Duration, Instant},
};
#[path = "capture_directory_identity.rs"]
pub(crate) mod directory_identity;
pub use directory_identity::IgnoredDirectoryIdentity;
#[path = "capture_repository_scope.rs"]
pub(crate) mod repository_scope;
pub use repository_scope::RepositoryScope;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DeadlineFailure {
    Cancelled,
    Expired,
}

#[derive(Clone)]
pub struct Deadline {
    end: Instant,
    cancellations: Vec<std::sync::Arc<std::sync::atomic::AtomicBool>>,
}
thread_local! {
    static HOST_DEADLINE: std::cell::RefCell<Option<Deadline>> = const {std::cell::RefCell::new(None)};
}
impl Deadline {
    pub(crate) fn intersection(&self, other: &Self) -> Result<Self> {
        let mut result = self.clone();
        result.end = result.end.min(other.end);
        for flag in &other.cancellations {
            if !result
                .cancellations
                .iter()
                .any(|f| std::sync::Arc::ptr_eq(f, flag))
            {
                result.cancellations.push(std::sync::Arc::clone(flag));
            }
        }
        ensure!(
            result.cancellations.len() <= 64,
            "Host deadline nesting capacity exceeded"
        );
        result.check()?;
        Ok(result)
    }
    pub fn new(ms: u64) -> Self {
        Self::from_duration(Duration::from_millis(ms))
    }
    pub(crate) fn from_duration(duration: Duration) -> Self {
        let now = Instant::now();
        let mut result = Self {
            end: now.checked_add(duration).unwrap_or(now),
            cancellations: Vec::new(),
        };
        HOST_DEADLINE.with(|scope| {
            if let Some(parent) = scope.borrow().as_ref() {
                result.end = result.end.min(parent.end);
                result.cancellations.clone_from(&parent.cancellations);
            }
        });
        result
    }
    pub(crate) fn failure(&self) -> Option<DeadlineFailure> {
        if self.cancelled() {
            Some(DeadlineFailure::Cancelled)
        } else if Instant::now() >= self.end {
            Some(DeadlineFailure::Expired)
        } else {
            None
        }
    }
    pub fn check(&self) -> Result<()> {
        ensure!(!self.cancelled(), "Request cancelled by host");
        ensure!(Instant::now() < self.end, "Request deadline exceeded");
        Ok(())
    }
    pub fn remaining(&self) -> Duration {
        if self.cancelled() {
            Duration::ZERO
        } else {
            self.end.saturating_duration_since(Instant::now())
        }
    }
    fn cancelled(&self) -> bool {
        self.cancellations
            .iter()
            .any(|flag| flag.load(std::sync::atomic::Ordering::SeqCst))
    }
    /// Bound nested work on this thread. New child deadlines inherit the ceiling
    /// and cancellation; explicit Deadline clones carry these to worker threads.
    /// Unrelated threads intentionally receive no implicit host context.
    pub fn run_bounded<T>(
        parent: &Self,
        cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
        run: impl FnOnce() -> Result<T>,
    ) -> Result<T> {
        struct Restore(Option<Deadline>);
        impl Drop for Restore {
            fn drop(&mut self) {
                HOST_DEADLINE.with(|scope| {
                    scope.replace(self.0.take());
                });
            }
        }
        let mut bounded = parent.clone();
        HOST_DEADLINE.with(|scope| {
            if let Some(outer) = scope.borrow().as_ref() {
                bounded.end = bounded.end.min(outer.end);
                for flag in &outer.cancellations {
                    if !bounded
                        .cancellations
                        .iter()
                        .any(|existing| std::sync::Arc::ptr_eq(flag, existing))
                    {
                        bounded.cancellations.push(std::sync::Arc::clone(flag));
                    }
                }
            }
        });
        if !bounded
            .cancellations
            .iter()
            .any(|existing| std::sync::Arc::ptr_eq(&cancel, existing))
        {
            bounded.cancellations.push(cancel);
        }
        ensure!(
            bounded.cancellations.len() <= 64,
            "Host deadline nesting capacity exceeded"
        );
        bounded.check()?;
        let _restore = Restore(HOST_DEADLINE.with(|scope| scope.replace(Some(bounded.clone()))));
        let result = run();
        if result
            .as_ref()
            .err()
            .is_some_and(crate::process::cleanup_unproven)
        {
            // Cancellation is not proof that an owned process has stopped.
            // Preserve the stronger typed failure after the cleanup attempt.
            return result;
        }
        bounded.check()?;
        result
    }
}
#[cfg(test)]
#[path = "capture_deadline_cause_tests.rs"]
mod deadline_cause_tests;

pub fn language(path: &str) -> &'static str {
    language_at(
        crate::analysis::AnalyzerEpoch::active().expect("known active analyzer"),
        path,
    )
}
pub fn language_at(epoch: crate::analysis::AnalyzerEpoch, path: &str) -> &'static str {
    match epoch {
        crate::analysis::AnalyzerEpoch::V8 | crate::analysis::AnalyzerEpoch::V9 => {
            language_v8(path)
        }
    }
}
fn language_v8(path: &str) -> &'static str {
    if path.to_ascii_lowercase().ends_with(".blade.php") {
        return "blade";
    }
    match path
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "rs" => "rust",
        "ts" | "mts" | "cts" => "typescript",
        "tsx" => "tsx",
        "js" | "mjs" | "cjs" => "javascript",
        "jsx" => "jsx",
        "py" | "pyi" => "python",
        "go" => "go",
        "c" | "h" => "c",
        "cpp" | "cc" | "cxx" | "hpp" | "hh" | "hxx" | "bb" => "cpp",
        "java" => "java",
        "cs" => "csharp",
        "rb" => "ruby",
        "php" => "php",
        "swift" => "swift",
        "scala" | "sc" => "scala",
        "lua" => "lua",
        "sh" | "bash" => "bash",
        "ex" | "exs" => "elixir",
        "sol" => "solidity",
        "ml" => "ocaml",
        "mli" => "ocaml-interface",
        "zig" => "zig",
        "nix" => "nix",
        "r" => "r",
        "kt" | "kts" => "kotlin",
        "dart" => "dart",
        "clj" | "cljs" | "cljc" => "clojure",
        "ps1" | "psm1" | "psd1" => "powershell",
        "jl" => "julia",
        "m" | "mm" => "objc",
        "f" | "for" | "f77" | "f90" | "f95" | "f03" | "f08" => "fortran",
        "erl" | "hrl" => "erlang",
        "lisp" | "lsp" | "cl" => "commonlisp",
        "pas" | "pp" | "dpr" | "dpk" => "pascal",
        "cob" | "cbl" | "cpy" => "cobol",
        "vb" => "vbnet",
        "v" | "sv" | "svh" | "vh" => "verilog",
        "tf" | "tfvars" | "hcl" => "hcl",
        "cls" | "trigger" => "apex",
        "dm" => "dreammaker",
        "robot" | "resource" => "robot",
        "vue" => "vue",
        "svelte" => "svelte",
        "astro" => "astro",
        "ejs" => "ejs",
        "cshtml" | "razor" => "razor",
        "xaml" | "csproj" | "fsproj" | "vbproj" | "slnx" => "config",
        "md" | "mdx" | "rst" => "markdown",
        "sql" => "sql",
        "json" | "yaml" | "yml" | "toml" | "xml" => "config",
        extension => catalog_language(extension).unwrap_or("text"),
    }
}
fn catalog_language(extension: &str) -> Option<&'static str> {
    // Analyzer-8 source classification is independent of future backend catalog
    // changes. The captured map preserves ambiguous extensions as unknown.
    static EXTENSIONS: std::sync::OnceLock<BTreeMap<String, Option<String>>> =
        std::sync::OnceLock::new();
    let extensions = EXTENSIONS.get_or_init(|| {
        serde_json::from_str(include_str!("capture_language_v8.json"))
            .expect("compiled analyzer-8 extension map")
    });
    extensions.get(extension).and_then(Option::as_deref)
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CaptureOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_encoding: Option<crate::source_encoding::Policy>,
    #[serde(default, skip_serializing_if = "crate::git_ignore_policy::disabled")]
    pub include_git_ignored: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_exclusions: Option<crate::source_exclusions::Policy>,
    #[serde(default)]
    pub only_dirs: Vec<String>,
    #[serde(default)]
    pub include_dirs: Vec<String>,
    #[serde(default)]
    pub language_overrides: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub read_only_dependency_dirs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub include_ignored_dirs: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ignored_directory_identity: Option<IgnoredDirectoryIdentity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository_scope: Option<RepositoryScope>,
}
impl CaptureOptions {
    pub fn validated(&self) -> Result<Self> {
        ensure!(
            self.only_dirs.len()
                + self.include_dirs.len()
                + self.read_only_dependency_dirs.len()
                + self.include_ignored_dirs.len()
                <= 256
                && self.language_overrides.len() <= 256,
            "Capture option budget exceeded"
        );
        let normalize_dirs = |dirs: &[String]| -> Result<Vec<String>> {
            let mut out = dirs
                .iter()
                .map(|path| crate::paths::normalized_syntax(path))
                .collect::<Result<Vec<_>>>()?;
            out.sort();
            out.dedup();
            Ok(out)
        };
        if let Some(scope) = &self.repository_scope {
            scope.validated()?;
            ensure!(
                !self.include_git_ignored
                    && self.include_ignored_dirs.is_empty()
                    && self.ignored_directory_identity.is_none(),
                "Repository capture cannot combine ignored-source overrides"
            );
        }
        let only_dirs = normalize_dirs(&self.only_dirs)?;
        let include_dirs = normalize_dirs(&self.include_dirs)?;
        let include_ignored_dirs = normalize_dirs(&self.include_ignored_dirs)?;
        ensure!(
            self.ignored_directory_identity.is_none()
                || (!include_ignored_dirs.is_empty() && !self.include_git_ignored),
            "Ignored directory identity requires explicit directory selectors"
        );
        ensure!(
            !self.include_git_ignored || include_ignored_dirs.is_empty(),
            "git_ignore_policy_cannot_combine_read_only_ignored_directory_override"
        );
        for directory in &include_ignored_dirs {
            normalized(directory)?;
            ensure!(
                !directory.split('/').any(|part| matches!(
                    part,
                    ".git" | ".yoke" | ".code-intelligence" | ".venv" | "__pycache__"
                )),
                "Hard-excluded ignored source selector"
            );
        }
        let mut read_only_dependency_dirs = self
            .read_only_dependency_dirs
            .iter()
            .map(|name| name.to_ascii_lowercase())
            .collect::<Vec<_>>();
        ensure!(
            read_only_dependency_dirs
                .iter()
                .all(|name| matches!(name.as_str(), "node_modules" | "target")),
            "Only named source dependency directories may be read-only"
        );
        read_only_dependency_dirs.sort();
        read_only_dependency_dirs.dedup();
        let directory_scope = Self {
            read_only_dependency_dirs: read_only_dependency_dirs.clone(),
            ..Default::default()
        };
        for directory in &only_dirs {
            read_path_kind(directory, &directory_scope)?;
        }
        for directory in &include_dirs {
            normalized(directory)?;
        }
        ensure!(
            include_dirs
                .iter()
                .all(|dir| !dir.contains('/') && !dir.starts_with('.')),
            "Include directories must be bare non-dot directory names"
        );
        let mut language_overrides = BTreeMap::new();
        for (extension, language) in &self.language_overrides {
            ensure!(
                !extension.is_empty()
                    && extension.len() <= 40
                    && extension
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'),
                "Invalid extension override"
            );
            ensure!(
                crate::analysis::grammar(language).is_some()
                    || crate::server_catalog::entries()
                        .iter()
                        .any(|entry| entry.languages.contains(&language.as_str()))
                    || matches!(
                        language.as_str(),
                        "vue"
                            | "svelte"
                            | "astro"
                            | "ejs"
                            | "blade"
                            | "razor"
                            | "cobol"
                            | "dreammaker"
                            | "robot"
                    )
                    || matches!(language.as_str(), "text" | "config" | "markdown" | "sql"),
                "Unknown language override"
            );
            ensure!(
                language_overrides
                    .insert(extension.to_ascii_lowercase(), language.clone())
                    .is_none(),
                "Duplicate normalized extension override"
            );
        }
        let source_exclusions = self
            .source_exclusions
            .as_ref()
            .map(crate::source_exclusions::Policy::canonical)
            .transpose()?
            .flatten();
        crate::source_exclusions::Compiled::new(source_exclusions.as_ref(), &|| Ok(()))?;
        Ok(Self {
            include_git_ignored: self.include_git_ignored,
            source_encoding: self
                .source_encoding
                .and_then(crate::source_encoding::Policy::canonical),
            source_exclusions,
            only_dirs,
            include_dirs,
            language_overrides,
            read_only_dependency_dirs,
            include_ignored_dirs,
            ignored_directory_identity: self.ignored_directory_identity,
            repository_scope: self.repository_scope.clone(),
        })
    }
    fn ignored_capture(&self) -> bool {
        self.include_git_ignored || !self.include_ignored_dirs.is_empty()
    }
    fn ignored_member_allowed(&self, path: &str) -> bool {
        self.include_git_ignored || self.ignored_prefix(path)
    }
    fn ignored_prefix(&self, path: &str) -> bool {
        if self.ignored_directory_identity.is_some() {
            return directory_identity::physical_prefix(&self.include_ignored_dirs, path);
        }
        self.include_ignored_dirs.iter().any(|directory| {
            let mut actual = path.split('/');
            directory.split('/').all(|expected| {
                actual.next().is_some_and(|actual| {
                    if cfg!(windows) {
                        actual
                            .chars()
                            .flat_map(char::to_uppercase)
                            .eq(expected.chars().flat_map(char::to_uppercase))
                    } else {
                        actual == expected
                    }
                })
            })
        })
    }
    fn includes(&self, path: &str) -> bool {
        let matches = |dir: &String| {
            path == dir
                || path
                    .strip_prefix(dir)
                    .is_some_and(|tail| tail.starts_with('/'))
        };
        (self.only_dirs.is_empty() || self.only_dirs.iter().any(matches))
            && self
                .repository_scope
                .as_ref()
                .is_none_or(|scope| scope.includes(path))
    }
    pub fn language_for(&self, path: &str) -> String {
        self.language_for_at(
            crate::analysis::AnalyzerEpoch::active().expect("known active analyzer"),
            path,
        )
    }
    pub(crate) fn language_for_at(
        &self,
        epoch: crate::analysis::AnalyzerEpoch,
        path: &str,
    ) -> String {
        let extension = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
        self.language_overrides
            .get(&extension)
            .cloned()
            .unwrap_or_else(|| language_at(epoch, path).into())
    }
}
fn read_path_kind(path: &str, options: &CaptureOptions) -> Result<bool> {
    crate::paths::normalized_syntax(path)?;
    let mut read_only = options.ignored_prefix(path)
        || options
            .repository_scope
            .as_ref()
            .is_some_and(|scope| scope.read_only(path));
    for component in path.split('/') {
        if protected(component) {
            let dependency = component.to_ascii_lowercase();
            ensure!(
                matches!(dependency.as_str(), "target" | "node_modules")
                    && options.read_only_dependency_dirs.contains(&dependency),
                "Protected captured source component"
            );
            read_only = true;
        }
    }
    Ok(read_only)
}
pub const OPTIONS_RECORD_KIND: &str = "native_capture_options_v1";
#[path = "capture_ignored.rs"]
mod ignored;
pub const IGNORED_RECORD_KIND: &str = ignored::RECORD_KIND;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CaptureOptionsRecord {
    pub snapshot_id: String,
    pub workspace_id: String,
    pub analyzer: String,
    pub options_hash: String,
    pub options: CaptureOptions,
}
pub fn capture_with_options(
    root: &Path,
    store: &Store,
    options: &CaptureOptions,
    deadline: &Deadline,
) -> Result<Snapshot> {
    capture_with_options_at(
        crate::analysis::AnalyzerEpoch::active()?,
        root,
        store,
        options,
        deadline,
    )
}
/// Explicit host-selected policy for a new capture; historical readers infer
/// their policy from the immutable source proof rather than this selection.
pub fn capture_with_options_at(
    epoch: crate::analysis::AnalyzerEpoch,
    root: &Path,
    store: &Store,
    options: &CaptureOptions,
    deadline: &Deadline,
) -> Result<Snapshot> {
    ensure!(
        options.repository_scope.is_none(),
        "Repository capture requires explicit host Git inspection"
    );
    capture_inner(
        epoch,
        root,
        store,
        &options.validated()?,
        deadline,
        None,
        true,
    )
}
pub const DERIVED_OPTIONS_RECORD_KIND: &str = "native_capture_scope_derived_v1";
pub(crate) fn validate_retention_record(
    store: &Store,
    kind: &str,
    id: &str,
    value: &serde_json::Value,
) -> Result<()> {
    store.check_deadline()?;
    ensure!(crate::model::valid_id(id), "Invalid capture retention ID");
    if kind == IGNORED_RECORD_KIND {
        return ignored::retention(store, id, value);
    }
    if kind == OPTIONS_RECORD_KIND {
        let record: CaptureOptionsRecord = serde_json::from_value(value.clone())?;
        ensure!(
            serde_json::to_value(&record)? == *value
                && record.snapshot_id == id
                && record.workspace_id == store.workspace_id
                && record.options == record.options.validated()?
                && record.options_hash == identity(&record.options)
                && (record.analyzer == crate::model::ANALYZER
                    || matches!(
                        record.analyzer.as_str(),
                        "yoke-tree-sitter-6"
                            | "yoke-tree-sitter-7"
                            | "yoke-tree-sitter-8"
                            | "yoke-tree-sitter-9"
                    )),
            "Invalid capture retention provenance"
        );
        let published: bool = store.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM snapshots WHERE id=?1)",
            [id],
            |row| row.get(0),
        )?;
        if published {
            options_for_snapshot(store, &store.snapshot(id)?)?;
        }
        // An unpublished stable capture carries configuration metadata only.
        // Its absent manifest cannot establish source or blob membership.
    } else {
        ensure!(
            kind == DERIVED_OPTIONS_RECORD_KIND,
            "Unknown capture retention kind"
        );
        let record: DerivedCaptureOptionsRecord = serde_json::from_value(value.clone())?;
        ensure!(
            serde_json::to_value(record)? == *value,
            "Noncanonical capture provenance"
        );
        options_for_snapshot(store, &store.snapshot(id)?)?;
    }
    Ok(())
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DerivedCaptureOptionsRecord {
    pub snapshot_id: String,
    pub workspace_id: String,
    pub origin_snapshot_id: String,
    pub origin_snapshot_hash: String,
    pub derived_snapshot_hash: String,
    pub options_hash: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OverlayCaptureRecord {
    snapshot_id: String,
    session_id: String,
    revision: u64,
    versions: BTreeMap<String, u32>,
    base_snapshot_id: String,
}
fn record_exists(store: &Store, kind: &str, id: &str) -> Result<bool> {
    Ok(store.db.query_row(
        "SELECT EXISTS(SELECT 1 FROM records WHERE kind=?1 AND id=?2)",
        rusqlite::params![kind, id],
        |r| r.get(0),
    )?)
}
fn validate_manifest_options_at(
    store: &Store,
    snapshot: &Snapshot,
    options: &CaptureOptions,
    historical: bool,
) -> Result<()> {
    validate_manifest_options_with(snapshot, options, historical, &|| store.check_deadline())
}
fn validate_manifest_options_with(
    snapshot: &Snapshot,
    options: &CaptureOptions,
    historical: bool,
    check: &impl Fn() -> Result<()>,
) -> Result<()> {
    let exclusions =
        crate::source_exclusions::Compiled::new(options.source_exclusions.as_ref(), check)?;
    ensure!(
        snapshot.files.len() <= 100_000,
        "Capture manifest capacity exceeded"
    );
    let mut languages = BTreeMap::new();
    for (index, file) in snapshot.files.iter().enumerate() {
        if index.is_multiple_of(64) {
            check()?;
        }
        ensure!(
            crate::paths::normalized_syntax(&file.path)? == file.path,
            "Unsafe capture manifest path"
        );
        read_path_kind(&file.path, options)?;
        let extension = Path::new(&file.path)
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let language_matches = if historical && !options.language_overrides.contains_key(&extension)
        {
            file.language == "text"
                || *languages.entry(file.language.clone()).or_insert_with(|| {
                    CaptureOptions {
                        language_overrides: BTreeMap::from([(
                            "proof".into(),
                            file.language.clone(),
                        )]),
                        ..Default::default()
                    }
                    .validated()
                    .is_ok()
                })
        } else {
            options.language_for(&file.path) == file.language
        };
        ensure!(
            options.includes(&file.path)
                && !exclusions.excludes(&file.path, check)?
                && language_matches,
            "Snapshot language or scope mismatch"
        );
    }
    Ok(())
}
struct ResolvedCaptureScope {
    options: CaptureOptions,
    historical: bool,
    ignored: Option<BTreeSet<String>>,
}
fn capture_scope_at(
    store: &Store,
    snapshot: &Snapshot,
    depth: usize,
    current_analyzer: &str,
) -> Result<(CaptureOptions, bool)> {
    let scope = resolve_capture_scope_at(store, snapshot, depth, current_analyzer)?;
    Ok((scope.options, scope.historical))
}
fn resolve_capture_scope_at(
    store: &Store,
    snapshot: &Snapshot,
    depth: usize,
    current_analyzer: &str,
) -> Result<ResolvedCaptureScope> {
    store.check_deadline()?;
    ensure!(
        matches!(
            current_analyzer,
            "yoke-tree-sitter-4"
                | "yoke-tree-sitter-6"
                | "yoke-tree-sitter-7"
                | "yoke-tree-sitter-8"
                | "yoke-tree-sitter-9"
        ),
        "Unknown capture reader epoch"
    );
    ensure!(
        depth < 16 && snapshot.workspace_id == store.workspace_id,
        "Capture options origin chain or workspace mismatch"
    );
    let mut ignored_members = None;
    let (options, historical) = if record_exists(store, IGNORED_RECORD_KIND, &snapshot.id)? {
        ensure!(
            !record_exists(store, OPTIONS_RECORD_KIND, &snapshot.id)?
                && !record_exists(store, DERIVED_OPTIONS_RECORD_KIND, &snapshot.id)?,
            "Conflicting ignored capture proofs"
        );
        let record = ignored::read(store, &snapshot.id)?;
        ignored_members = Some(ignored::validate(
            &record,
            snapshot,
            current_analyzer,
            &|| store.check_deadline(),
        )?);
        let historical = record.analyzer != current_analyzer;
        (record.options, historical)
    } else if record_exists(store, OPTIONS_RECORD_KIND, &snapshot.id)? {
        let record: CaptureOptionsRecord = store.get_record(OPTIONS_RECORD_KIND, &snapshot.id)?;
        let options = record.options.validated()?;
        ensure!(
            !options.ignored_capture(),
            "Ignored capture requires its explicit typed proof"
        );
        ensure!(
            record.snapshot_id == snapshot.id
                && record.workspace_id == snapshot.workspace_id
                && options == record.options
                && record.options_hash == identity(&options),
            "Capture options identity mismatch"
        );
        ensure!(
            record.analyzer == current_analyzer
                || matches!(
                    record.analyzer.as_str(),
                    "yoke-tree-sitter-6"
                        | "yoke-tree-sitter-7"
                        | "yoke-tree-sitter-8"
                        | "yoke-tree-sitter-9"
                ),
            "Unknown configured capture analyzer"
        );
        ensure!(
            snapshot.kind == "disk"
                && snapshot.id
                    == identity(&(
                        &store.workspace_id,
                        "disk",
                        &record.analyzer,
                        &snapshot.files,
                        &snapshot.omissions,
                        &record.options_hash
                    )),
            "Configured snapshot identity mismatch"
        );
        let historical = matches!(
            record.analyzer.as_str(),
            "yoke-tree-sitter-6"
                | "yoke-tree-sitter-7"
                | "yoke-tree-sitter-8"
                | "yoke-tree-sitter-9"
        ) && record.analyzer != current_analyzer;
        if historical {
            ensure!(
                identity(&store.snapshot(&snapshot.id)?) == identity(snapshot),
                "Historical capture manifest mismatch"
            );
        }
        (options, historical)
    } else if record_exists(store, DERIVED_OPTIONS_RECORD_KIND, &snapshot.id)? {
        let record: DerivedCaptureOptionsRecord =
            store.get_record(DERIVED_OPTIONS_RECORD_KIND, &snapshot.id)?;
        ensure!(
            record.snapshot_id == snapshot.id
                && record.workspace_id == snapshot.workspace_id
                && record.derived_snapshot_hash == identity(snapshot),
            "Derived capture options identity mismatch"
        );
        let stored = store.snapshot(&snapshot.id)?;
        ensure!(
            identity(&stored) == record.derived_snapshot_hash,
            "Derived capture manifest mismatch"
        );
        let origin = store.snapshot(&record.origin_snapshot_id)?;
        ensure!(
            record.origin_snapshot_hash == identity(&origin),
            "Capture origin manifest mismatch"
        );
        let scope = resolve_capture_scope_at(store, &origin, depth + 1, current_analyzer)?;
        let (options, historical) = (scope.options, scope.historical);
        ignored::transition(&origin, snapshot, &options, scope.ignored.as_ref(), &|| {
            store.check_deadline()
        })?;
        ignored_members = scope.ignored;
        ensure!(
            record.options_hash == identity(&options),
            "Derived capture options hash mismatch"
        );
        (options, historical)
    } else if snapshot.kind == "overlay" {
        let raw: String = store.db.query_row(
            "SELECT value FROM records WHERE kind='overlay-snapshot' AND id=?1",
            [&snapshot.id],
            |row| row.get(0),
        )?;
        let record: OverlayCaptureRecord =
            crate::json::decode(crate::json::parse(raw.as_bytes(), 4_200_000)?)?;
        ensure!(
            record.snapshot_id == snapshot.id
                && crate::model::valid_id(&record.session_id)
                && crate::model::valid_id(&record.base_snapshot_id)
                && record.versions.len() <= 1000
                && record
                    .versions
                    .values()
                    .all(|version| *version <= i32::MAX as u32),
            "Invalid overlay capture receipt"
        );
        ensure!(
            identity(&store.snapshot(&snapshot.id)?) == identity(snapshot),
            "Overlay capture manifest mismatch"
        );
        let base = store.snapshot(&record.base_snapshot_id)?;
        ensure!(
            base.kind == "disk"
                && snapshot.omissions == base.omissions
                && snapshot.capture_consistency == "overlay-from-stable-disk-and-immutable-buffers",
            "Overlay origin or consistency mismatch"
        );
        let scope = resolve_capture_scope_at(store, &base, depth + 1, current_analyzer)?;
        let options = scope.options;
        ignored::transition(&base, snapshot, &options, scope.ignored.as_ref(), &|| {
            store.check_deadline()
        })?;
        ignored_members = scope.ignored;
        ensure!(
            record_exists(store, OPTIONS_RECORD_KIND, &base.id)?
                || record_exists(store, IGNORED_RECORD_KIND, &base.id)?
                || [
                    current_analyzer,
                    "yoke-tree-sitter-4",
                    "yoke-tree-sitter-6",
                    "yoke-tree-sitter-7",
                    "yoke-tree-sitter-8",
                    "yoke-tree-sitter-9"
                ]
                .into_iter()
                .any(|epoch| base.id
                    == identity(&(
                        &store.workspace_id,
                        "disk",
                        epoch,
                        &base.files,
                        &base.omissions
                    ))),
            "Overlay origin has no native capture proof"
        );
        let epoch = [
            current_analyzer,
            "yoke-tree-sitter-4",
            "yoke-tree-sitter-6",
            "yoke-tree-sitter-7",
            "yoke-tree-sitter-8",
            "yoke-tree-sitter-9",
        ]
        .into_iter()
        .find(|epoch| {
            snapshot.id
                == identity(&(
                    epoch,
                    &base.id,
                    &record.session_id,
                    record.revision,
                    &record.versions,
                    &snapshot.files,
                ))
        })
        .ok_or_else(|| anyhow::anyhow!("Overlay capture identity mismatch"))?;
        let mut overlay_files = BTreeMap::new();
        for file in &snapshot.files {
            store.check_deadline()?;
            ensure!(
                overlay_files.insert(file.path.as_str(), file).is_none(),
                "Duplicate overlay source path"
            );
        }
        for path in record.versions.keys() {
            store.check_deadline()?;
            normalized(path)?;
            ensure!(
                overlay_files.contains_key(path.as_str()),
                "Overlay version outside manifest"
            );
        }
        for file in &base.files {
            store.check_deadline()?;
            let overlaid = overlay_files
                .remove(file.path.as_str())
                .ok_or_else(|| anyhow::anyhow!("Overlay dropped captured source"))?;
            ensure!(
                record.versions.contains_key(&file.path) || overlaid == file,
                "Unversioned overlay source changed"
            );
        }
        for path in overlay_files.keys() {
            store.check_deadline()?;
            ensure!(
                record.versions.contains_key(*path),
                "Unversioned new overlay source"
            );
        }
        (options, epoch != current_analyzer)
    } else {
        let historical = snapshot.kind == "disk"
            && [
                "yoke-tree-sitter-4",
                "yoke-tree-sitter-6",
                "yoke-tree-sitter-7",
                "yoke-tree-sitter-8",
                "yoke-tree-sitter-9",
            ]
            .into_iter()
            .any(|analyzer| {
                analyzer != current_analyzer
                    && snapshot.id
                        == identity(&(
                            &store.workspace_id,
                            "disk",
                            analyzer,
                            &snapshot.files,
                            &snapshot.omissions,
                        ))
            });
        if historical {
            ensure!(
                identity(&store.snapshot(&snapshot.id)?) == identity(snapshot),
                "Historical default capture manifest mismatch"
            );
        }
        if snapshot.kind == "disk" {
            ensure!(
                historical
                    || snapshot.id
                        == identity(&(
                            &store.workspace_id,
                            "disk",
                            current_analyzer,
                            &snapshot.files,
                            &snapshot.omissions
                        )),
                "Missing or unknown native capture proof"
            );
        }
        (CaptureOptions::default(), historical)
    };
    validate_manifest_options_at(store, snapshot, &options, historical)?;
    Ok(ResolvedCaptureScope {
        options,
        historical,
        ignored: ignored_members,
    })
}
fn options_chain(store: &Store, snapshot: &Snapshot, depth: usize) -> Result<CaptureOptions> {
    Ok(capture_scope_at(store, snapshot, depth, crate::model::ANALYZER)?.0)
}

#[cfg(test)]
mod epoch_migration_tests {
    use super::*;
    #[test]
    fn authentic_native8_scope_proofs_remain_valid_for_a9_reader() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/analyzer8/authentic-analyzer8.json"
        ))
        .unwrap();
        for case in fixture["cases"].as_array().unwrap() {
            let root = tempfile::tempdir().unwrap();
            let mut store = Store::open(root.path()).unwrap();
            store.workspace_id = case["workspaceId"].as_str().unwrap().into();
            for row in case["snapshots"].as_array().unwrap() {
                store
                    .db
                    .execute(
                        "INSERT INTO snapshots(id,value) VALUES(?1,?2)",
                        rusqlite::params![
                            row["id"].as_str().unwrap(),
                            row["value"].as_str().unwrap()
                        ],
                    )
                    .unwrap();
            }
            for row in case["records"].as_array().unwrap() {
                store
                    .db
                    .execute(
                        "INSERT INTO records(kind,id,value) VALUES(?1,?2,?3)",
                        rusqlite::params![
                            row["kind"].as_str().unwrap(),
                            row["id"].as_str().unwrap(),
                            row["value"].as_str().unwrap()
                        ],
                    )
                    .unwrap();
            }
            for row in case["snapshots"].as_array().unwrap() {
                let snapshot: Snapshot =
                    serde_json::from_str(row["value"].as_str().unwrap()).unwrap();
                let baseline =
                    capture_scope_at(&store, &snapshot, 0, "yoke-tree-sitter-8").unwrap();
                let historical = capture_scope_at(&store, &snapshot, 0, "yoke-tree-sitter-9")
                    .unwrap_or_else(|e| {
                        panic!(
                            "authentic {} {} proof rejected: {e}",
                            case["name"], snapshot.kind
                        )
                    });
                assert_eq!(historical.0, baseline.0);
                assert!(historical.1);
            }
        }
    }
    #[test]
    fn authentic_archived8_scope_proofs_remain_valid_for_a9_reader() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/analyzer8/authentic-analyzer8.json"
        ))
        .unwrap();
        for case in fixture["cases"].as_array().unwrap() {
            for row in case["registrations"].as_array().unwrap() {
                let registration: serde_json::Value =
                    serde_json::from_str(row["payload"].as_str().unwrap()).unwrap();
                let snapshot: Snapshot =
                    serde_json::from_value(registration["snapshot"].clone()).unwrap();
                validate_archived_capture_scope_at(
                    &snapshot,
                    &registration["archive"]["captureScope"],
                    crate::analysis::AnalyzerEpoch::V9,
                    &Deadline::new(20000),
                )
                .unwrap_or_else(|e| {
                    panic!(
                        "authentic {} archived {} proof rejected: {e}",
                        case["name"], snapshot.kind
                    )
                });
            }
        }
    }
}
pub fn options_for_snapshot(store: &Store, snapshot: &Snapshot) -> Result<CaptureOptions> {
    options_chain(store, snapshot, 0)
}
/// Publication constructs a new disk manifest; replay reads an existing one.
/// A typed capture receipt may select its own reader only for that first
/// publication, with all tuple, options, scope and member checks still applied.
pub(crate) fn options_for_publication(
    store: &Store,
    snapshot: &Snapshot,
) -> Result<CaptureOptions> {
    store.check_deadline()?;
    let published: bool = store.db.query_row(
        "SELECT EXISTS(SELECT 1 FROM snapshots WHERE id=?1)",
        [&snapshot.id],
        |row| row.get(0),
    )?;
    if !published && snapshot.kind == "disk" {
        let analyzer = if record_exists(store, IGNORED_RECORD_KIND, &snapshot.id)? {
            Some(ignored::read(store, &snapshot.id)?.analyzer)
        } else if record_exists(store, OPTIONS_RECORD_KIND, &snapshot.id)? {
            Some(
                store
                    .get_record::<CaptureOptionsRecord>(OPTIONS_RECORD_KIND, &snapshot.id)?
                    .analyzer,
            )
        } else {
            None
        };
        if let Some(analyzer) = analyzer {
            return Ok(capture_scope_at(store, snapshot, 0, &analyzer)?.0);
        }
    }
    options_for_snapshot(store, snapshot)
}
pub(crate) fn options_for_snapshot_at(
    store: &Store,
    snapshot: &Snapshot,
    reader: crate::analysis::AnalyzerEpoch,
) -> Result<CaptureOptions> {
    Ok(capture_scope_at(store, snapshot, 0, reader.producer())?.0)
}
/// Recover a source-acquisition policy, not a derived graph's extraction policy.
/// Scope-only derivatives and overlays inherit their acquisition origin; an
/// exact owned Git source receipt independently proves its recorded policy.
pub fn capture_epoch_for_snapshot_with_deadline(
    store: &Store,
    snapshot: &Snapshot,
    deadline: &Deadline,
) -> Result<crate::analysis::AnalyzerEpoch> {
    deadline.check()?;
    store.set_deadline(deadline)?;
    // Validate the complete scope lineage before considering any policy root.
    // This retains path, ignored-member, manifest/hash and chain-depth checks.
    let options_hash = identity(&source_scope_policy(store, snapshot)?.options);
    let mut current = std::borrow::Cow::Borrowed(snapshot);
    for _ in 0..16 {
        deadline.check()?;
        ensure!(
            current.workspace_id == store.workspace_id,
            "Capture acquisition workspace mismatch"
        );
        if crate::git_workflows::has_source_proof(store, &current.id)? {
            // A present but malformed/unsupported Git proof must fail rather
            // than falling through to its older inherited disk origin.
            return crate::git_workflows::validated_source_epoch(store, &current, deadline);
        }
        let origin = match current.kind.as_str() {
            "disk" => {
                // Only disk capture identities are shared by these selectors.
                // The extraction selector must never receive a scope-only node.
                return analyzer_epoch_for_snapshot_with_deadline(store, &current, deadline);
            }
            "derived" => {
                let record: DerivedCaptureOptionsRecord =
                    store.get_record(DERIVED_OPTIONS_RECORD_KIND, &current.id)?;
                let origin = store.snapshot(&record.origin_snapshot_id)?;
                ensure!(
                    record.snapshot_id == current.id
                        && record.workspace_id == current.workspace_id
                        && record.derived_snapshot_hash == identity(current.as_ref())
                        && record.origin_snapshot_hash == identity(&origin)
                        && record.options_hash == options_hash,
                    "Capture acquisition origin binding mismatch"
                );
                origin
            }
            "overlay" => {
                let record: OverlayCaptureRecord =
                    store.get_record("overlay-snapshot", &current.id)?;
                let base = store.snapshot(&record.base_snapshot_id)?;
                ensure!(
                    record.snapshot_id == current.id
                        && base.kind == "disk"
                        && [
                            "yoke-tree-sitter-4",
                            "yoke-tree-sitter-6",
                            "yoke-tree-sitter-7",
                            "yoke-tree-sitter-8",
                            "yoke-tree-sitter-9"
                        ]
                        .into_iter()
                        .any(|producer| {
                            current.id
                                == identity(&(
                                    producer,
                                    &base.id,
                                    &record.session_id,
                                    record.revision,
                                    &record.versions,
                                    &current.files,
                                ))
                        }),
                    "Capture acquisition overlay binding mismatch"
                );
                base
            }
            _ => bail!("Snapshot has no proven native capture acquisition origin"),
        };
        current = std::borrow::Cow::Owned(origin);
    }
    bail!("Capture acquisition origin chain capacity exceeded")
}
/// Git replay selection performs bounded commit/tree/blob proof verification.
pub fn analyzer_epoch_for_snapshot_with_deadline(
    store: &Store,
    snapshot: &Snapshot,
    deadline: &Deadline,
) -> Result<crate::analysis::AnalyzerEpoch> {
    deadline.check()?;
    store.set_deadline(deadline)?;
    if crate::git_workflows::has_source_proof(store, &snapshot.id)? {
        return crate::git_workflows::validated_source_epoch(store, snapshot, deadline);
    }
    deadline.check()?;
    analyzer_epoch_for_snapshot(store, snapshot)
}
/// Select replay semantics from exact native capture identity, never from a
/// graph's unsigned producer labels or an extension-classification match.
pub fn analyzer_epoch_for_snapshot(
    store: &Store,
    snapshot: &Snapshot,
) -> Result<crate::analysis::AnalyzerEpoch> {
    analyzer_epoch_for_snapshot_at(store, snapshot, crate::analysis::AnalyzerEpoch::active()?)
}
pub(crate) fn analyzer_epoch_for_snapshot_at(
    store: &Store,
    snapshot: &Snapshot,
    reader: crate::analysis::AnalyzerEpoch,
) -> Result<crate::analysis::AnalyzerEpoch> {
    use crate::analysis::AnalyzerEpoch;
    store.check_deadline()?;
    source_scope_policy_at(store, snapshot, reader)?;
    let published: bool = store.db.query_row(
        "SELECT EXISTS(SELECT 1 FROM snapshots WHERE id=?1)",
        [&snapshot.id],
        |row| row.get(0),
    )?;
    if published {
        ensure!(
            identity(&store.snapshot(&snapshot.id)?) == identity(snapshot),
            "Analyzer selection requires the exact immutable manifest"
        );
    }
    match snapshot.kind.as_str() {
        "disk" if record_exists(store, IGNORED_RECORD_KIND, &snapshot.id)? => {
            AnalyzerEpoch::parse(&ignored::read(store, &snapshot.id)?.analyzer)
        }
        "disk" if record_exists(store, OPTIONS_RECORD_KIND, &snapshot.id)? => {
            let record: CaptureOptionsRecord =
                store.get_record(OPTIONS_RECORD_KIND, &snapshot.id)?;
            AnalyzerEpoch::parse(&record.analyzer)
        }
        "disk" => [AnalyzerEpoch::V8, AnalyzerEpoch::V9]
            .into_iter()
            .find(|epoch| {
                snapshot.id
                    == identity(&(
                        &snapshot.workspace_id,
                        "disk",
                        epoch.producer(),
                        &snapshot.files,
                        &snapshot.omissions,
                    ))
            })
            .ok_or_else(|| {
                anyhow::anyhow!("Captured source epoch has no supported extraction replay policy")
            }),
        "derived" if record_exists(store, DERIVED_OPTIONS_RECORD_KIND, &snapshot.id)? => {
            let record: DerivedCaptureOptionsRecord =
                store.get_record(DERIVED_OPTIONS_RECORD_KIND, &snapshot.id)?;
            let origin = store.snapshot(&record.origin_snapshot_id)?;
            [AnalyzerEpoch::V8,AnalyzerEpoch::V9].into_iter().find(|epoch|
                snapshot.id == epoch.derived_snapshot_id(&origin,&snapshot.files))
                .ok_or_else(|| anyhow::anyhow!("Derived source epoch is not proven; Git/custom historical graphs require their own producer receipt"))
        }
        "overlay" => {
            let record: OverlayCaptureRecord =
                store.get_record("overlay-snapshot", &snapshot.id)?;
            [AnalyzerEpoch::V8, AnalyzerEpoch::V9]
                .into_iter()
                .find(|epoch| {
                    snapshot.id
                        == identity(&(
                            epoch.producer(),
                            &record.base_snapshot_id,
                            &record.session_id,
                            record.revision,
                            &record.versions,
                            &snapshot.files,
                        ))
                })
                .ok_or_else(|| anyhow::anyhow!("Overlay extraction epoch is not proven"))
        }
        _ => anyhow::bail!("Snapshot has no proven native extraction epoch"),
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CapturedSourcePolicy {
    /// A dependency or foreign/legacy virtual source restriction, never a write grant.
    pub read_only: bool,
}
#[cfg(windows)]
pub(crate) fn canonical_write_path(store: &Store, path: &str) -> Result<String> {
    let root = store.dir.parent().context("Missing workspace root")?;
    let mut existing = crate::paths::scoped(root, path)?;
    let mut suffix = Vec::new();
    loop {
        store.check_deadline()?;
        match fs::symlink_metadata(&existing) {
            Ok(metadata) => {
                ensure!(!is_link(&metadata), "Write alias reparse point refused");
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                suffix.push(
                    existing
                        .file_name()
                        .context("Missing write ancestor")?
                        .to_owned(),
                );
                ensure!(
                    existing.pop() && existing.starts_with(root),
                    "Write path escaped root"
                );
            }
            Err(error) => return Err(error.into()),
        }
    }
    let canonical_root = root.canonicalize()?;
    let mut canonical = existing.canonicalize()?;
    for component in suffix.into_iter().rev() {
        canonical.push(component);
    }
    let relative = canonical
        .strip_prefix(canonical_root)
        .context("Write alias escaped workspace")?
        .to_str()
        .context("Non-UTF8 write alias")?
        .replace('\\', "/");
    normalized(&relative)?;
    Ok(relative)
}
/// Reuse one verified immutable scope for streaming many sources. No source bytes
/// are cached and request data cannot change its captured host configuration.
pub struct CapturedSourceScope<'a> {
    store: &'a Store,
    options: CaptureOptions,
    files: BTreeMap<&'a str, &'a SourceFile>,
    virtual_read_only: bool,
    prospective_native: bool,
    ignored: Option<BTreeSet<String>>,
    exclusions: crate::source_exclusions::Compiled,
}
impl CapturedSourceScope<'_> {
    pub fn source_encoding(&self) -> Option<crate::source_encoding::Policy> {
        self.options.source_encoding
    }
    pub fn require_legacy_text(&self, consumer: &str) -> Result<()> {
        ensure!(
            self.source_encoding().is_none(),
            "source_encoding_capability_unavailable: {consumer} requires legacy UTF-8 source proof"
        );
        Ok(())
    }
    pub fn source_view(&self, path: &str) -> Result<crate::source_encoding::View> {
        crate::source_encoding::View::decode(
            self.read_source(path)?,
            self.source_encoding(),
            &|| self.store.check_deadline(),
        )
    }
    fn includes(&self, path: &str) -> Result<bool> {
        Ok(self.options.includes(path)
            && !self
                .exclusions
                .excludes(path, &|| self.store.check_deadline())?)
    }
    /// Filtering metadata from the original immutable native capture, not authority.
    pub fn ignored_at_capture(&self, path: &str) -> Result<bool> {
        self.file(path)?;
        Ok(self
            .ignored
            .as_ref()
            .is_some_and(|paths| paths.contains(path)))
    }
    pub(crate) fn index_uses_git_ignored(&self) -> bool {
        self.options.include_git_ignored
    }
    /// Host-only write restriction for existing or prospective paths; no read grant.
    /// Virtual snapshot provenance is handled separately by guarded edit callers.
    pub fn check_host_write_scope(&self, path: &str) -> Result<()> {
        self.store.check_deadline()?;
        normalized(path)?;
        ensure!(
            self.includes(path)?
                && !read_path_kind(path, &self.options)?
                && !self
                    .ignored
                    .as_ref()
                    .is_some_and(|members| members.contains(path)),
            "Captured host scope is read-only or excludes this write path"
        );
        #[cfg(windows)]
        {
            // Resolve existing Windows directory aliases before admitting a future
            // path. This is a write restriction, never a captured read grant.
            let relative = canonical_write_path(self.store, path)?;
            ensure!(
                self.includes(&relative)?
                    && !read_path_kind(&relative, &self.options)?
                    && !self
                        .ignored
                        .as_ref()
                        .is_some_and(|members| members.contains(&relative)),
                "Canonical host scope is read-only or excludes this write path"
            );
        }
        if self.options.ignored_directory_identity.is_some() {
            directory_identity::check_captured_write(
                self.store,
                &self.options.include_ignored_dirs,
                path,
            )?;
        }
        Ok(())
    }
    /// Prospective metadata only: no origin membership, filesystem or blob grant.
    pub fn prospective_path_policy(&self, path: &str) -> Result<(String, CapturedSourcePolicy)> {
        self.prospective_path_policy_at(crate::analysis::AnalyzerEpoch::active()?, path)
    }
    pub(crate) fn prospective_path_policy_at(
        &self,
        epoch: crate::analysis::AnalyzerEpoch,
        path: &str,
    ) -> Result<(String, CapturedSourcePolicy)> {
        self.store.check_deadline()?;
        ensure!(
            self.prospective_native,
            "Prospective source scope requires native capture provenance"
        );
        read_path_kind(path, &self.options)?;
        ensure!(
            self.includes(path)?,
            "New read path outside captured host scope"
        );
        let mut components = path.split('/').peekable();
        while let Some(component) = components.next() {
            if components.peek().is_some() {
                ensure!(
                    !matches!(component, "build" | "dist" | "vendor")
                        || self
                            .options
                            .include_dirs
                            .iter()
                            .any(|name| name == component),
                    "New read path outside captured scanner scope"
                );
            }
        }
        Ok((
            self.options.language_for_at(epoch, path),
            CapturedSourcePolicy { read_only: true },
        ))
    }
    pub fn read_source(&self, path: &str) -> Result<Vec<u8>> {
        let file = self.file(path)?;
        let blob = self.store.dir.join("blobs").join(&file.hash);
        let metadata = fs::symlink_metadata(&blob)?;
        ensure!(
            metadata.is_file() && !is_link(&metadata) && metadata.len() == file.size as u64,
            "Captured blob type or size mismatch"
        );
        let mut input = fs::File::open(blob)?;
        let opened = input.metadata()?;
        ensure!(
            opened.is_file() && !is_link(&opened) && opened.len() == file.size as u64,
            "Opened captured blob type or size mismatch"
        );
        let mut bytes = Vec::with_capacity(file.size);
        let mut chunk = [0u8; 65_536];
        loop {
            self.store.check_deadline()?;
            let remaining = file.size + 1 - bytes.len();
            let read = input.read(&mut chunk[..remaining.min(65_536)])?;
            if read == 0 {
                break;
            }
            ensure!(
                bytes.len() + read <= file.size,
                "Captured blob grew past expected size"
            );
            bytes.extend_from_slice(&chunk[..read]);
        }
        ensure!(
            bytes.len() == file.size && crate::model::hash(&bytes) == file.hash,
            "Captured blob content mismatch"
        );
        Ok(bytes)
    }
    pub fn file(&self, path: &str) -> Result<&SourceFile> {
        self.path_policy(path)?;
        self.files
            .get(path)
            .copied()
            .ok_or_else(|| anyhow::anyhow!("Source path outside captured manifest"))
    }
    pub fn path_policy(&self, path: &str) -> Result<CapturedSourcePolicy> {
        self.store.check_deadline()?;
        ensure!(
            self.files.contains_key(path),
            "Source path outside captured manifest"
        );
        let dependency = read_path_kind(path, &self.options)?;
        ensure!(
            self.includes(path)?,
            "Source path outside captured host scope"
        );
        Ok(CapturedSourcePolicy {
            read_only: dependency
                || self.virtual_read_only
                || self
                    .ignored
                    .as_ref()
                    .is_some_and(|members| members.contains(path)),
        })
    }
}
pub fn source_scope_policy<'a>(
    store: &'a Store,
    snapshot: &'a Snapshot,
) -> Result<CapturedSourceScope<'a>> {
    source_scope_policy_at(store, snapshot, crate::analysis::AnalyzerEpoch::active()?)
}
pub(crate) fn source_scope_policy_at<'a>(
    store: &'a Store,
    snapshot: &'a Snapshot,
    reader: crate::analysis::AnalyzerEpoch,
) -> Result<CapturedSourceScope<'a>> {
    let resolved = resolve_capture_scope_at(store, snapshot, 0, reader.producer())?;
    let options = resolved.options;
    let native = match snapshot.kind.as_str() {
        "disk"
            if record_exists(store, OPTIONS_RECORD_KIND, &snapshot.id)?
                || record_exists(store, IGNORED_RECORD_KIND, &snapshot.id)? =>
        {
            true
        }
        "derived" if record_exists(store, DERIVED_OPTIONS_RECORD_KIND, &snapshot.id)? => true,
        "overlay" if record_exists(store, "overlay-snapshot", &snapshot.id)? => true,
        "disk" => [
            reader.producer(),
            "yoke-tree-sitter-4",
            "yoke-tree-sitter-6",
            "yoke-tree-sitter-7",
            "yoke-tree-sitter-8",
            "yoke-tree-sitter-9",
        ]
        .into_iter()
        .any(|analyzer| {
            snapshot.id
                == identity(&(
                    &store.workspace_id,
                    "disk",
                    analyzer,
                    &snapshot.files,
                    &snapshot.omissions,
                ))
        }),
        _ => false,
    };
    if !native {
        ensure!(
            options.read_only_dependency_dirs.is_empty()
                && matches!(snapshot.kind.as_str(), "derived" | "federated")
                && identity(&store.snapshot(&snapshot.id)?) == identity(snapshot),
            "Source scope has no captured authority proof"
        );
    }
    let mut files = BTreeMap::new();
    for file in &snapshot.files {
        store.check_deadline()?;
        ensure!(
            crate::model::valid_id(&file.hash)
                && file.size <= 4_000_000
                && files.insert(file.path.as_str(), file).is_none(),
            "Invalid captured source membership"
        );
    }
    let exclusions =
        crate::source_exclusions::Compiled::new(options.source_exclusions.as_ref(), &|| {
            store.check_deadline()
        })?;
    Ok(CapturedSourceScope {
        store,
        options,
        files,
        virtual_read_only: !native || snapshot.kind != "disk",
        prospective_native: native && matches!(snapshot.kind.as_str(), "disk" | "derived"),
        ignored: resolved.ignored,
        exclusions,
    })
}
/// Separate signed foreign namespace: only federation's non-forgeable, current
/// host-authorized query capability admits this scope. No native publication,
/// dependency exception, ignored-file override or prospective source authority.
pub(crate) fn federated_query_scope<'a>(
    verified: &'a crate::federation::VerifiedQuerySource<'_>,
) -> Result<CapturedSourceScope<'a>> {
    let (store, snapshot) = verified.source_parts();
    let mut files = BTreeMap::new();
    for file in &snapshot.files {
        store.check_deadline()?;
        normalized(&file.path)?;
        ensure!(
            crate::model::valid_id(&file.hash)
                && file.size <= 4_000_000
                && files.insert(file.path.as_str(), file).is_none(),
            "Invalid verified foreign source membership"
        );
    }
    Ok(CapturedSourceScope {
        store,
        options: CaptureOptions::default(),
        files,
        virtual_read_only: true,
        prospective_native: false,
        // Receiver ignores were already checked for every file. Foreign source
        // carries no native ignored-at-capture inclusion metadata.
        ignored: None,
        exclusions: crate::source_exclusions::Compiled::default(),
    })
}
pub fn source_path_policy(
    store: &Store,
    snapshot: &Snapshot,
    path: &str,
) -> Result<CapturedSourcePolicy> {
    source_scope_policy(store, snapshot)?.path_policy(path)
}
/// Admission metadata for a guarded producer of a future immutable manifest.
/// This grants neither filesystem/blob access nor membership in the origin.
pub fn captured_read_path_policy(
    store: &Store,
    origin: &Snapshot,
    path: &str,
) -> Result<(String, CapturedSourcePolicy)> {
    source_scope_policy(store, origin)?.prospective_path_policy(path)
}
pub fn validate_language(store: &Store, snapshot: &Snapshot, path: &str) -> Result<String> {
    let file = snapshot
        .files
        .iter()
        .find(|f| f.path == path)
        .ok_or_else(|| anyhow::anyhow!("Snapshot path not found"))?;
    let (options, historical) = capture_scope_at(store, snapshot, 0, crate::model::ANALYZER)?;
    ensure!(
        historical || file.language == options.language_for(path),
        "Snapshot language mismatch"
    );
    Ok(file.language.clone())
}
pub fn language_for_snapshot(store: &Store, snapshot: &Snapshot, path: &str) -> Result<String> {
    ensure!(
        normalized(path)? == path && !protected(path),
        "Unsafe new capture path"
    );
    let options = options_for_snapshot(store, snapshot)?;
    let exclusions =
        crate::source_exclusions::Compiled::new(options.source_exclusions.as_ref(), &|| {
            store.check_deadline()
        })?;
    ensure!(
        options.includes(path) && !exclusions.excludes(path, &|| store.check_deadline())?,
        "New path outside capture scope"
    );
    Ok(options.language_for(path))
}
pub fn capture_for_snapshot(
    root: &Path,
    store: &Store,
    snapshot: &Snapshot,
    deadline: &Deadline,
) -> Result<Snapshot> {
    let epoch = capture_epoch_for_snapshot_with_deadline(store, snapshot, deadline)?;
    let options = options_for_snapshot(store, snapshot)?;
    if options.repository_scope.is_some() {
        // This already checked immutable selection is reused only for a
        // source staleness observation. It neither refreshes Git nor publishes
        // a new source proof/graph or grants a broader current host selection.
        capture_inner(epoch, root, store, &options, deadline, None, false)
    } else {
        capture_with_options_at(epoch, root, store, &options, deadline)
    }
}
/// Called by guarded host workflows after publishing the derived manifest.
/// This only carries capture scope; it never grants permission to write source.
pub fn inherit_options(store: &Store, origin: &Snapshot, derived: &Snapshot) -> Result<()> {
    store.check_deadline()?;
    ensure!(
        origin.id != derived.id && derived.workspace_id == store.workspace_id,
        "Invalid capture scope inheritance"
    );
    ensure!(
        identity(&store.snapshot(&origin.id)?) == identity(origin)
            && identity(&store.snapshot(&derived.id)?) == identity(derived),
        "Capture scope inheritance requires persisted exact manifests"
    );
    // Reserve one chain level before publishing the create-only proof.
    let scope = resolve_capture_scope_at(store, origin, 1, crate::model::ANALYZER)?;
    let (options, historical) = (scope.options, scope.historical);
    ignored::transition(origin, derived, &options, scope.ignored.as_ref(), &|| {
        store.check_deadline()
    })?;
    validate_manifest_options_at(store, derived, &options, historical)?;
    let record = DerivedCaptureOptionsRecord {
        snapshot_id: derived.id.clone(),
        workspace_id: store.workspace_id.clone(),
        origin_snapshot_id: origin.id.clone(),
        origin_snapshot_hash: identity(origin),
        derived_snapshot_hash: identity(derived),
        options_hash: identity(&options),
    };
    store.db.execute(
        "INSERT INTO records(kind,id,value) VALUES(?1,?2,?3) ON CONFLICT(kind,id) DO NOTHING",
        rusqlite::params![
            DERIVED_OPTIONS_RECORD_KIND,
            derived.id,
            serde_json::to_string(&record)?
        ],
    )?;
    ensure!(
        store
            .get_record::<DerivedCaptureOptionsRecord>(DERIVED_OPTIONS_RECORD_KIND, &derived.id)?
            == record,
        "Immutable derived capture option collision"
    );
    options_for_snapshot(store, derived)?;
    Ok(())
}

fn scan(
    epoch: crate::analysis::AnalyzerEpoch,
    root: &Path,
    store: &Store,
    deadline: &Deadline,
    options: &CaptureOptions,
) -> Result<(Vec<SourceFile>, Vec<Omission>, BTreeSet<String>)> {
    let exclusions =
        crate::source_exclusions::Compiled::new(options.source_exclusions.as_ref(), &|| {
            deadline.check()
        })?;
    let included_names = options.include_dirs.clone();
    let dependencies = options.read_only_dependency_dirs.clone();
    let walker = WalkBuilder::new(root)
        .hidden(false)
        .follow_links(false)
        .git_ignore(true)
        .git_global(false)
        .git_exclude(true)
        .require_git(false)
        .filter_entry(move |entry| {
            !entry
                .path()
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| {
                    matches!(
                        n,
                        ".git" | ".yoke" | ".code-intelligence" | ".venv" | "__pycache__"
                    ) || matches!(n.to_ascii_lowercase().as_str(), "node_modules" | "target")
                        && !dependencies.iter().any(|name| name.eq_ignore_ascii_case(n))
                        || matches!(n, "build" | "dist" | "vendor")
                            && !included_names.iter().any(|name| name == n)
                })
        })
        .build();
    let mut selected_omissions = Vec::new();
    let mut walks = vec![(false, walker)];
    if let Some(scope) = &options.repository_scope {
        walks.clear();
        for path in &scope.files {
            deadline.check()?;
            let selected = crate::paths::scoped(root, path)?;
            match fs::symlink_metadata(&selected) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    selected_omissions.push(Omission {
                        path: path.clone(),
                        reason: "repository-captured-file-missing".into(),
                    });
                    continue;
                }
                Err(error) => return Err(error.into()),
                Ok(metadata) if !metadata.is_file() || is_link(&metadata) => {
                    selected_omissions.push(Omission {
                        path: path.clone(),
                        reason: "repository-captured-file-not-regular".into(),
                    });
                    continue;
                }
                Ok(_) => {}
            }
            let walk = WalkBuilder::new(selected)
                .hidden(false)
                .follow_links(false)
                .ignore(false)
                .git_ignore(false)
                .git_global(false)
                .git_exclude(false)
                .require_git(false)
                .parents(false)
                .build();
            walks.push((false, walk));
        }
    }
    if options.include_git_ignored {
        let included_names = options.include_dirs.clone();
        let dependencies = options.read_only_dependency_dirs.clone();
        let additional = WalkBuilder::new(root)
            .hidden(false)
            .follow_links(false)
            .git_ignore(false)
            .git_global(false)
            .git_exclude(false)
            .require_git(false)
            .filter_entry(move |entry| {
                !entry
                    .path()
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| {
                        matches!(
                            n,
                            ".git" | ".yoke" | ".code-intelligence" | ".venv" | "__pycache__"
                        ) || matches!(n.to_ascii_lowercase().as_str(), "node_modules" | "target")
                            && !dependencies.iter().any(|name| name.eq_ignore_ascii_case(n))
                            || matches!(n, "build" | "dist" | "vendor")
                                && !included_names.iter().any(|name| name == n)
                    })
            })
            .build();
        walks.push((true, additional));
    }
    for directory in &options.include_ignored_dirs {
        deadline.check()?;
        let selected = crate::paths::scoped(root, directory)?;
        let metadata = match fs::symlink_metadata(&selected) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        ensure!(
            metadata.is_dir() && !is_link(&metadata),
            "Ignored capture selector must be a regular directory"
        );
        let canonical_root = root.canonicalize()?;
        let canonical_selected = selected.canonicalize()?;
        let selected = root.join(
            canonical_selected
                .strip_prefix(&canonical_root)
                .context("Ignored selector escaped canonical workspace")?,
        );
        let included = options.include_dirs.clone();
        let dependencies = options.read_only_dependency_dirs.clone();
        let walk = WalkBuilder::new(selected)
            .hidden(false)
            .follow_links(false)
            .git_ignore(false)
            .git_global(false)
            .git_exclude(false)
            .ignore(false)
            .parents(false)
            .filter_entry(move |entry| {
                !entry
                    .path()
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| {
                        matches!(
                            name,
                            ".git" | ".yoke" | ".code-intelligence" | ".venv" | "__pycache__"
                        ) || matches!(
                            name.to_ascii_lowercase().as_str(),
                            "node_modules" | "target"
                        ) && !dependencies
                            .iter()
                            .any(|dependency| dependency.eq_ignore_ascii_case(name))
                            || matches!(name, "build" | "dist" | "vendor")
                                && !included.iter().any(|included| included == name)
                    })
            })
            .build();
        walks.push((true, walk));
    }
    let mut files = Vec::new();
    let mut omissions = options
        .repository_scope
        .as_ref()
        .map(RepositoryScope::omissions)
        .transpose()?
        .unwrap_or_default();
    omissions.extend(selected_omissions);
    let mut total = 0usize;
    let mut captured = BTreeSet::new();
    let mut ignored_paths = BTreeSet::new();
    for entry in walks.into_iter().flat_map(|(selected, walker)| {
        walker.map(move |entry| entry.map(|entry| (entry, selected)))
    }) {
        deadline.check()?;
        ensure!(
            omissions.len() < 100_000,
            "Capture omission capacity exceeded"
        );
        let (entry, selected) = entry?;
        if entry.depth() == 0 && !selected && options.repository_scope.is_none() {
            continue;
        }
        let relative = entry
            .path()
            .strip_prefix(root)?
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Non-UTF-8 path"))?
            .replace('\\', "/");
        let metadata = fs::symlink_metadata(entry.path())?;
        if is_link(&metadata) {
            omissions.push(Omission {
                path: relative,
                reason: "link-or-reparse-point".into(),
            });
            continue;
        }
        if metadata.is_dir() {
            continue;
        }
        if crate::paths::normalized_syntax(&relative).is_ok()
            && read_path_kind(&relative, options).is_err()
        {
            omissions.push(Omission {
                path: relative,
                reason: "protected-path".into(),
            });
            continue;
        }
        if crate::paths::normalized_syntax(&relative).is_err() {
            omissions.push(Omission {
                path: relative,
                reason: "unsupported-path".into(),
            });
            continue;
        }
        if !metadata.is_file() {
            omissions.push(Omission {
                path: relative,
                reason: "not-regular-file".into(),
            });
            continue;
        }
        if !options.includes(&relative) {
            continue;
        }
        if exclusions.excludes(&relative, &|| deadline.check())? {
            omissions.push(Omission {
                path: relative,
                reason: "host-source-exclusion".into(),
            });
            continue;
        }
        if captured.contains(&relative) {
            continue;
        }
        if metadata.len() > 4_000_000 {
            omissions.push(Omission {
                path: relative,
                reason: "file-size-limit".into(),
            });
            continue;
        }
        let mut bytes = Vec::new();
        if options.source_encoding.is_some() {
            // Raw allocation is admitted independently of the later text/map.
            bytes.reserve_exact(4_000_000);
            let mut input = fs::File::open(entry.path())?;
            let mut buffer = [0; 16384];
            loop {
                deadline.check()?;
                let read = input.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                ensure!(
                    read <= 4_000_000 - bytes.len(),
                    "File grew past capture limit"
                );
                bytes.extend_from_slice(&buffer[..read]);
            }
        } else {
            fs::File::open(entry.path())?
                .take(4_000_001)
                .read_to_end(&mut bytes)?;
        }
        ensure!(bytes.len() <= 4_000_000, "File grew past capture limit");
        if options.source_encoding.is_none()
            && (bytes.contains(&0) || std::str::from_utf8(&bytes).is_err())
        {
            omissions.push(Omission {
                path: relative,
                reason: "binary-or-invalid-utf8".into(),
            });
            continue;
        }
        let view =
            match crate::source_encoding::View::decode(bytes, options.source_encoding, &|| {
                deadline.check()
            }) {
                Ok(view) => view,
                Err(error) => {
                    deadline.check()?;
                    omissions.push(Omission {
                        path: relative,
                        reason: format!("source-decoding-refused: {error}"),
                    });
                    continue;
                }
            };
        if view.text().contains('\0') {
            omissions.push(Omission {
                path: relative,
                reason: "binary-decoded-nul".into(),
            });
            continue;
        }
        if crate::paths::obvious_secret(view.text()) {
            omissions.push(Omission {
                path: relative,
                reason: "obvious-secret".into(),
            });
            continue;
        }
        let bytes = view.raw();
        total = total
            .checked_add(bytes.len())
            .ok_or_else(|| anyhow::anyhow!("Snapshot size overflow"))?;
        ensure!(
            total <= 512_000_000 && files.len() < 100_000,
            "Snapshot capture budget exceeded"
        );
        let hash = store.blob(bytes)?;
        captured.insert(relative.clone());
        if selected {
            ignored_paths.insert(relative.clone());
        }
        #[cfg(unix)]
        let mode = {
            use std::os::unix::fs::PermissionsExt;
            metadata.permissions().mode() & 0o777
        };
        #[cfg(not(unix))]
        let mode = 0o644;
        files.push(SourceFile {
            language: options.language_for_at(epoch, &relative),
            path: relative,
            hash,
            size: bytes.len(),
            mode,
        });
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    omissions.sort_by(|a, b| a.path.cmp(&b.path));
    omissions.dedup_by(|left, right| left.path == right.path && left.reason == right.reason);
    Ok((files, omissions, ignored_paths))
}
pub fn capture(root: &Path, store: &Store, deadline: &Deadline) -> Result<Snapshot> {
    capture_with_options(root, store, &CaptureOptions::default(), deadline)
}
pub(crate) fn capture_repository_options_at(
    epoch: crate::analysis::AnalyzerEpoch,
    root: &Path,
    store: &Store,
    options: &CaptureOptions,
    deadline: &Deadline,
    recheck: &dyn Fn() -> Result<()>,
) -> Result<Snapshot> {
    ensure!(
        options.repository_scope.is_some() && store.db.is_autocommit(),
        "Repository capture requires owned host inspection outside a transaction"
    );
    capture_inner(
        epoch,
        root,
        store,
        &options.validated()?,
        deadline,
        Some(recheck),
        true,
    )
}
fn capture_inner(
    epoch: crate::analysis::AnalyzerEpoch,
    root: &Path,
    store: &Store,
    options: &CaptureOptions,
    deadline: &Deadline,
    recheck: Option<&dyn Fn() -> Result<()>>,
    publish_options: bool,
) -> Result<Snapshot> {
    for _ in 0..3 {
        let selected = directory_identity::resolve_options(root, store, options, deadline)?;
        let effective = selected.as_ref();
        let (files, omissions, ignored_paths) = scan(epoch, root, store, deadline, effective)?;
        let second = scan(epoch, root, store, deadline, effective)?;
        if options.ignored_directory_identity.is_some()
            && effective
                != directory_identity::resolve_options(root, store, options, deadline)?.as_ref()
        {
            continue;
        }
        let options = effective;
        if files == second.0 && omissions == second.1 && ignored_paths == second.2 {
            if let Some(recheck) = recheck {
                recheck()?;
            }
            if options.ignored_capture() {
                return ignored::capture(
                    epoch,
                    store,
                    options,
                    files,
                    omissions,
                    &ignored_paths,
                    deadline,
                );
            }
            let mut id = identity(&(
                &store.workspace_id,
                "disk",
                epoch.producer(),
                &files,
                &omissions,
            ));
            if options != &CaptureOptions::default() {
                let options_hash = identity(options);
                id = identity(&(
                    &store.workspace_id,
                    "disk",
                    epoch.producer(),
                    &files,
                    &omissions,
                    &options_hash,
                ));
                if publish_options {
                    let record = CaptureOptionsRecord {
                        snapshot_id: id.clone(),
                        workspace_id: store.workspace_id.clone(),
                        analyzer: epoch.producer().into(),
                        options_hash,
                        options: options.clone(),
                    };
                    let raw = serde_json::to_string(&record)?;
                    store.db.execute("INSERT INTO records(kind,id,value) VALUES(?1,?2,?3) ON CONFLICT(kind,id) DO NOTHING",rusqlite::params![OPTIONS_RECORD_KIND,id,raw])?;
                    ensure!(
                        store.get_record::<CaptureOptionsRecord>(OPTIONS_RECORD_KIND, &id)?
                            == record,
                        "Immutable capture option collision"
                    );
                }
            }
            return Ok(Snapshot {
                id,
                workspace_id: store.workspace_id.clone(),
                kind: "disk".into(),
                files,
                omissions,
                capture_consistency: "stable-double-scan".into(),
            });
        }
    }
    bail!("Workspace did not stabilize during capture")
}

#[cfg(test)]
mod analyzer_migration_tests {
    use super::*;
    #[test]
    fn released_analyzer_four_archives_preserve_unknown_vue_and_powershell_languages() {
        let root = tempfile::tempdir().unwrap();
        fs::write(
            root.path().join("old.vue"),
            "<script>function original() {}</script>",
        )
        .unwrap();
        fs::write(root.path().join("old.ps1"), "function Original { }\n").unwrap();
        let store = Store::open(root.path()).unwrap();
        let mut archived = capture(root.path(), &store, &Deadline::new(5000)).unwrap();
        for file in &mut archived.files {
            file.language = "text".into();
        }
        // Same tuple and unchanged SourceFile/Snapshot schemas as released 0.5.
        archived.id = identity(&(
            &store.workspace_id,
            "disk",
            "yoke-tree-sitter-4",
            &archived.files,
            &archived.omissions,
        ));
        store
            .db
            .execute(
                "INSERT INTO snapshots VALUES(?1,?2)",
                rusqlite::params![archived.id, serde_json::to_string(&archived).unwrap()],
            )
            .unwrap();
        for active in [
            "yoke-tree-sitter-6",
            "yoke-tree-sitter-7",
            "yoke-tree-sitter-8",
        ] {
            let (options, historical) = capture_scope_at(&store, &archived, 0, active).unwrap();
            assert_eq!(options, CaptureOptions::default());
            assert!(historical);
            let mut tampered = archived.clone();
            tampered.files[0].language = "php".into();
            assert!(capture_scope_at(&store, &tampered, 0, active).is_err());
        }
        assert_eq!(
            store
                .snapshot(&archived.id)
                .unwrap()
                .files
                .iter()
                .map(|file| file.language.as_str())
                .collect::<Vec<_>>(),
            ["text", "text"]
        );
        assert_eq!(
            validate_language(&store, &archived, "old.vue").unwrap(),
            "text"
        );
        assert_eq!(
            validate_language(&store, &archived, "old.ps1").unwrap(),
            "text"
        );
    }
    #[test]
    fn authentic_analyzer_six_languages_survive_seven_with_exact_scope_proof() {
        for configured in [false, true] {
            let root = tempfile::tempdir().unwrap();
            fs::create_dir(root.path().join("src")).unwrap();
            fs::write(root.path().join("src/main.hcl"), "value = 1\n").unwrap();
            fs::write(root.path().join("src/main.custom"), "fn actual() {}\n").unwrap();
            let store = Store::open(root.path()).unwrap();
            let options = if configured {
                CaptureOptions {
                    only_dirs: vec!["src".into()],
                    language_overrides: BTreeMap::from([("custom".into(), "rust".into())]),
                    ..Default::default()
                }
            } else {
                CaptureOptions::default()
            };
            let mut historical =
                capture_with_options(root.path(), &store, &options, &Deadline::new(5000)).unwrap();
            historical
                .files
                .iter_mut()
                .find(|file| file.path.ends_with(".hcl"))
                .unwrap()
                .language = "text".into();
            let options_hash = identity(&options);
            historical.id = if configured {
                identity(&(
                    &store.workspace_id,
                    "disk",
                    "yoke-tree-sitter-6",
                    &historical.files,
                    &historical.omissions,
                    &options_hash,
                ))
            } else {
                identity(&(
                    &store.workspace_id,
                    "disk",
                    "yoke-tree-sitter-6",
                    &historical.files,
                    &historical.omissions,
                ))
            };
            store
                .db
                .execute(
                    "INSERT INTO snapshots VALUES(?1,?2)",
                    rusqlite::params![historical.id, serde_json::to_string(&historical).unwrap()],
                )
                .unwrap();
            if configured {
                store
                    .record(
                        OPTIONS_RECORD_KIND,
                        &historical.id,
                        &CaptureOptionsRecord {
                            snapshot_id: historical.id.clone(),
                            workspace_id: store.workspace_id.clone(),
                            analyzer: "yoke-tree-sitter-6".into(),
                            options_hash,
                            options: options.clone(),
                        },
                    )
                    .unwrap();
            }
            assert!(
                capture_scope_at(&store, &historical, 0, "yoke-tree-sitter-6").is_err(),
                "Current classification remains strict"
            );
            let (recovered, past) =
                capture_scope_at(&store, &historical, 0, "yoke-tree-sitter-7").unwrap();
            assert_eq!(recovered, options);
            assert!(past);
            let mut unknown = historical.clone();
            unknown.id = identity(&("unknown-analyzer", &historical.id));
            assert!(capture_scope_at(&store, &unknown, 0, "yoke-tree-sitter-7").is_err());
            if configured {
                let mut overridden = historical.clone();
                overridden
                    .files
                    .iter_mut()
                    .find(|file| file.path.ends_with(".custom"))
                    .unwrap()
                    .language = "php".into();
                assert!(
                    validate_manifest_options_at(&store, &overridden, &options, true).is_err(),
                    "Historical scope never relaxes explicit host language override"
                );
                overridden.files[0].path = "outside/secret.hcl".into();
                assert!(
                    validate_manifest_options_at(&store, &overridden, &options, true).is_err(),
                    "Historical scope never widens directory membership"
                );
            }
            let mut tampered = historical.clone();
            tampered.files[0].language = "php".into();
            assert!(capture_scope_at(&store, &tampered, 0, "yoke-tree-sitter-7").is_err());
            let mut foreign = historical.clone();
            foreign.kind = "federated".into();
            assert!(capture_scope_at(&store, &foreign, 0, "yoke-tree-sitter-7").is_err());
        }
    }
    #[test]
    fn authentic_analyzer_seven_scope_survives_eight_without_relabeling() {
        for configured in [false, true] {
            let root = tempfile::tempdir().unwrap();
            fs::create_dir(root.path().join("src")).unwrap();
            fs::write(root.path().join("src/main.hcl"), "value = 1\n").unwrap();
            fs::write(root.path().join("src/main.custom"), "fn actual() {}\n").unwrap();
            let store = Store::open(root.path()).unwrap();
            let options = if configured {
                CaptureOptions {
                    only_dirs: vec!["src".into()],
                    language_overrides: BTreeMap::from([("custom".into(), "rust".into())]),
                    ..Default::default()
                }
            } else {
                CaptureOptions::default()
            };
            let mut historical =
                capture_with_options(root.path(), &store, &options, &Deadline::new(5000)).unwrap();
            let options_hash = identity(&options);
            historical.id = if configured {
                identity(&(
                    &store.workspace_id,
                    "disk",
                    "yoke-tree-sitter-7",
                    &historical.files,
                    &historical.omissions,
                    &options_hash,
                ))
            } else {
                identity(&(
                    &store.workspace_id,
                    "disk",
                    "yoke-tree-sitter-7",
                    &historical.files,
                    &historical.omissions,
                ))
            };
            store
                .db
                .execute(
                    "INSERT INTO snapshots VALUES(?1,?2)",
                    rusqlite::params![historical.id, serde_json::to_string(&historical).unwrap()],
                )
                .unwrap();
            if configured {
                store
                    .record(
                        OPTIONS_RECORD_KIND,
                        &historical.id,
                        &CaptureOptionsRecord {
                            snapshot_id: historical.id.clone(),
                            workspace_id: store.workspace_id.clone(),
                            analyzer: "yoke-tree-sitter-7".into(),
                            options_hash,
                            options: options.clone(),
                        },
                    )
                    .unwrap();
            }
            let (recovered, past) =
                capture_scope_at(&store, &historical, 0, "yoke-tree-sitter-8").unwrap();
            assert_eq!(recovered, options);
            assert!(past);
            assert_eq!(
                historical
                    .files
                    .iter()
                    .find(|f| f.path.ends_with(".hcl"))
                    .unwrap()
                    .language,
                "hcl"
            );
            let session_id = "c".repeat(64);
            let versions = BTreeMap::<String, u32>::new();
            let mut overlay = historical.clone();
            overlay.kind = "overlay".into();
            overlay.capture_consistency = "overlay-from-stable-disk-and-immutable-buffers".into();
            overlay.id = identity(&(
                "yoke-tree-sitter-7",
                &historical.id,
                &session_id,
                1u64,
                &versions,
                &overlay.files,
            ));
            store
                .db
                .execute(
                    "INSERT INTO snapshots VALUES(?1,?2)",
                    rusqlite::params![overlay.id, serde_json::to_string(&overlay).unwrap()],
                )
                .unwrap();
            store.record("overlay-snapshot",&overlay.id,&serde_json::json!({"snapshotId":overlay.id,"sessionId":session_id,"revision":1,"versions":versions,"baseSnapshotId":historical.id})).unwrap();
            let (overlay_options, overlay_past) =
                capture_scope_at(&store, &overlay, 0, "yoke-tree-sitter-8").unwrap();
            assert_eq!(overlay_options, options);
            assert!(overlay_past);
            let mut invalid_overlay = overlay.clone();
            invalid_overlay.files[0].language = "php".into();
            assert!(capture_scope_at(&store, &invalid_overlay, 0, "yoke-tree-sitter-8").is_err());
            let mut unknown = historical.clone();
            unknown.id = identity(&("unknown-analyzer", &historical.id));
            assert!(source_scope_policy(&store, &unknown).is_err());
            if configured {
                let mut overridden = historical.clone();
                overridden
                    .files
                    .iter_mut()
                    .find(|file| file.path.ends_with(".custom"))
                    .unwrap()
                    .language = "php".into();
                assert!(
                    validate_manifest_options_at(&store, &overridden, &options, true).is_err(),
                    "Historical scope never relaxes explicit host language override"
                );
                overridden.files[0].path = "outside/secret.hcl".into();
                assert!(
                    validate_manifest_options_at(&store, &overridden, &options, true).is_err(),
                    "Historical scope never widens directory membership"
                );
            }
            let mut tampered = historical.clone();
            tampered.files[0].language = "php".into();
            assert!(capture_scope_at(&store, &tampered, 0, "yoke-tree-sitter-8").is_err());
            let mut foreign = historical.clone();
            foreign.kind = "federated".into();
            assert!(source_scope_policy(&store, &foreign).is_err());
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ArchivedCaptureScope {
    schema_version: u8,
    nodes: Vec<ArchivedCaptureNode>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ArchivedCaptureNode {
    snapshot: Snapshot,
    proof: ArchivedCaptureProof,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum ArchivedCaptureProof {
    Git {
        record: crate::git_workflows::ArchivedGitSource,
    },
    IgnoredNative {
        record: ignored::Record,
    },
    Native {
        options: Option<CaptureOptionsRecord>,
    },
    Derived {
        record: DerivedCaptureOptionsRecord,
    },
    Overlay {
        record: OverlayCaptureRecord,
    },
}
fn archived_native_options_at(
    snapshot: &Snapshot,
    record: Option<&CaptureOptionsRecord>,
    reader: &str,
) -> Result<(CaptureOptions, bool)> {
    ensure!(
        snapshot.kind == "disk",
        "Archived native origin must be disk capture"
    );
    let epochs = [
        reader,
        "yoke-tree-sitter-4",
        "yoke-tree-sitter-6",
        "yoke-tree-sitter-7",
        "yoke-tree-sitter-8",
        "yoke-tree-sitter-9",
    ];
    if let Some(record) = record {
        let options = record.options.validated()?;
        ensure!(
            !options.ignored_capture(),
            "Archived ignored capture requires its typed proof"
        );
        ensure!(
            record.snapshot_id == snapshot.id
                && record.workspace_id == snapshot.workspace_id
                && record.options == options
                && record.options_hash == identity(&options),
            "Archived capture options binding mismatch"
        );
        ensure!(
            record.analyzer == reader
                || matches!(
                    record.analyzer.as_str(),
                    "yoke-tree-sitter-6"
                        | "yoke-tree-sitter-7"
                        | "yoke-tree-sitter-8"
                        | "yoke-tree-sitter-9"
                ),
            "Unknown archived configured analyzer"
        );
        ensure!(
            snapshot.id
                == identity(&(
                    &snapshot.workspace_id,
                    "disk",
                    &record.analyzer,
                    &snapshot.files,
                    &snapshot.omissions,
                    &record.options_hash
                )),
            "Archived configured native identity mismatch"
        );
        Ok((options, record.analyzer != reader))
    } else {
        let epoch = epochs
            .into_iter()
            .find(|epoch| {
                snapshot.id
                    == identity(&(
                        &snapshot.workspace_id,
                        "disk",
                        epoch,
                        &snapshot.files,
                        &snapshot.omissions,
                    ))
            })
            .ok_or_else(|| anyhow::anyhow!("Archived default native identity mismatch"))?;
        Ok((CaptureOptions::default(), epoch != reader))
    }
}
struct ArchivedProofBudget<'a> {
    bytes: usize,
    next_check: usize,
    deadline: &'a Deadline,
}
impl std::io::Write for ArchivedProofBudget<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let total = self
            .bytes
            .checked_add(bytes.len())
            .ok_or_else(|| std::io::Error::other("Archived capture proof byte overflow"))?;
        if total > 64_000_000 {
            return Err(std::io::Error::other(
                "Archived capture proof byte budget exceeded",
            ));
        }
        if total >= self.next_check {
            self.deadline.check().map_err(std::io::Error::other)?;
            self.next_check = total.saturating_add(64 * 1024);
        }
        self.bytes = total;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
/// A captured scope receipt for offline validation; no filesystem or source-byte grant.
pub(crate) fn archived_capture_scope(
    store: &Store,
    snapshot: &Snapshot,
    deadline: &Deadline,
) -> Result<serde_json::Value> {
    source_scope_policy(store, snapshot)?;
    ensure!(
        identity(&store.snapshot(&snapshot.id)?) == identity(snapshot),
        "Archived capture requires immutable published manifest"
    );
    let mut current = snapshot.clone();
    let mut nodes = Vec::new();
    let mut budget = ArchivedProofBudget {
        bytes: 1024,
        next_check: 0,
        deadline,
    };
    let mut visited = BTreeSet::new();
    loop {
        deadline.check()?;
        ensure!(
            nodes.len() < 16 && visited.insert(current.id.clone()),
            "Archived capture origin chain exceeded or cyclic"
        );
        let (proof, origin) = if current.kind == "disk"
            && record_exists(store, IGNORED_RECORD_KIND, &current.id)?
        {
            (
                ArchivedCaptureProof::IgnoredNative {
                    record: ignored::read(store, &current.id)?,
                },
                None,
            )
        } else if current.kind == "disk" {
            let options = if record_exists(store, OPTIONS_RECORD_KIND, &current.id)? {
                Some(store.get_record::<CaptureOptionsRecord>(OPTIONS_RECORD_KIND, &current.id)?)
            } else {
                None
            };
            (ArchivedCaptureProof::Native { options }, None)
        } else if current.kind == "derived"
            && crate::git_workflows::has_source_proof(store, &current.id)?
        {
            let record = crate::git_workflows::archive_source(store, &current, deadline)?;
            let origin = store.snapshot(record.origin_id())?;
            (ArchivedCaptureProof::Git { record }, Some(origin))
        } else if current.kind == "derived"
            && record_exists(store, DERIVED_OPTIONS_RECORD_KIND, &current.id)?
        {
            let record: DerivedCaptureOptionsRecord =
                store.get_record(DERIVED_OPTIONS_RECORD_KIND, &current.id)?;
            let origin = store.snapshot(&record.origin_snapshot_id)?;
            (ArchivedCaptureProof::Derived { record }, Some(origin))
        } else if current.kind == "overlay" {
            let record: OverlayCaptureRecord = store.get_record("overlay-snapshot", &current.id)?;
            let origin = store.snapshot(&record.base_snapshot_id)?;
            (ArchivedCaptureProof::Overlay { record }, Some(origin))
        } else {
            anyhow::bail!("Archived source has no native capture origin proof");
        };
        let node = ArchivedCaptureNode {
            snapshot: current,
            proof,
        };
        serde_json::to_writer(&mut budget, &node)?;
        nodes.push(node);
        if let Some(origin) = origin {
            current = origin;
        } else {
            break;
        }
    }
    let proof = serde_json::to_value(ArchivedCaptureScope {
        schema_version: 1,
        nodes,
    })?;
    validate_archived_capture_scope(snapshot, &proof, deadline)?;
    Ok(proof)
}
fn validate_archived_manifest_shape(source: &Snapshot, deadline: &Deadline) -> Result<()> {
    ensure!(
        crate::model::valid_id(&source.id)
            && crate::model::valid_id(&source.workspace_id)
            && source.files.len() <= 100_000,
        "Invalid archived source identity/capacity"
    );
    let mut paths = BTreeSet::new();
    let mut total = 0usize;
    for file in &source.files {
        deadline.check()?;
        ensure!(
            paths.insert(&file.path)
                && crate::model::valid_id(&file.hash)
                && file.size <= 4_000_000,
            "Invalid archived source member"
        );
        total = total
            .checked_add(file.size)
            .ok_or_else(|| anyhow::anyhow!("Archived source byte overflow"))?;
        ensure!(
            total <= 512_000_000,
            "Archived source byte capacity exceeded"
        );
    }
    Ok(())
}
/// Validate original scope without opening its workspace. Invalid non-null proofs never fall back.
/// Callers decode untrusted receipts with the bounded JSON parser before passing a Value.
pub(crate) fn validate_archived_capture_scope(
    snapshot: &Snapshot,
    value: &serde_json::Value,
    deadline: &Deadline,
) -> Result<()> {
    validate_archived_capture_scope_at(
        snapshot,
        value,
        crate::analysis::AnalyzerEpoch::active()?,
        deadline,
    )
}

/// Recover source-acquisition options from an exact offline scope proof, without live I/O.
/// Options recovery does not establish an extraction epoch or grant access to live source.
pub fn archived_capture_options(
    snapshot: &Snapshot,
    scope: &serde_json::Value,
    deadline: &Deadline,
) -> Result<CaptureOptions> {
    archived_capture_options_at(
        snapshot,
        scope,
        crate::analysis::AnalyzerEpoch::active()?,
        deadline,
    )
}

/// Recover a replay policy from exact archived capture identities, never graph labels.
/// Known historical policies without an installed extractor remain explicitly unreplayed.
pub(crate) fn archived_analyzer_epoch(
    snapshot: &Snapshot,
    value: &serde_json::Value,
    deadline: &Deadline,
) -> Result<Option<crate::analysis::AnalyzerEpoch>> {
    use crate::analysis::AnalyzerEpoch;
    validate_archived_capture_scope_at(snapshot, value, AnalyzerEpoch::V9, deadline)?;
    fn default_epoch(source: &Snapshot) -> Option<AnalyzerEpoch> {
        [AnalyzerEpoch::V8, AnalyzerEpoch::V9]
            .into_iter()
            .find(|epoch| {
                source.id
                    == identity(&(
                        &source.workspace_id,
                        "disk",
                        epoch.producer(),
                        &source.files,
                        &source.omissions,
                    ))
            })
    }
    fn node_epoch(
        nodes: &[ArchivedCaptureNode],
        index: usize,
        deadline: &Deadline,
    ) -> Result<Option<AnalyzerEpoch>> {
        deadline.check()?;
        let node = &nodes[index];
        Ok(match &node.proof {
            ArchivedCaptureProof::Native {
                options: Some(record),
            } => AnalyzerEpoch::parse(&record.analyzer).ok(),
            ArchivedCaptureProof::Native { options: None } => default_epoch(&node.snapshot),
            ArchivedCaptureProof::IgnoredNative { record } => {
                AnalyzerEpoch::parse(&record.analyzer).ok()
            }
            ArchivedCaptureProof::Derived { .. } => {
                let origin = &nodes[index + 1].snapshot;
                let epoch = [AnalyzerEpoch::V8, AnalyzerEpoch::V9]
                    .into_iter()
                    .find(|epoch| {
                        node.snapshot.id == epoch.derived_snapshot_id(origin, &node.snapshot.files)
                    })
                    .ok_or_else(|| {
                        anyhow::anyhow!("Archived derived extraction identity is unproven")
                    })?;
                // The old unversioned derived tuple also predates epoch8. Its origin
                // must establish a supported extraction generation before replay.
                if epoch == AnalyzerEpoch::V8 && node_epoch(nodes, index + 1, deadline)?.is_none() {
                    None
                } else {
                    Some(epoch)
                }
            }
            ArchivedCaptureProof::Git { record } => record.epoch(),
            ArchivedCaptureProof::Overlay { record } => [AnalyzerEpoch::V8, AnalyzerEpoch::V9]
                .into_iter()
                .find(|epoch| {
                    node.snapshot.id
                        == identity(&(
                            epoch.producer(),
                            &record.base_snapshot_id,
                            &record.session_id,
                            record.revision,
                            &record.versions,
                            &node.snapshot.files,
                        ))
                }),
        })
    }
    if value.is_null() {
        return Ok(default_epoch(snapshot));
    }
    let archive: ArchivedCaptureScope = serde_json::from_value(value.clone())?;
    node_epoch(&archive.nodes, 0, deadline)
}
pub(crate) fn validate_archived_capture_scope_at(
    snapshot: &Snapshot,
    value: &serde_json::Value,
    epoch: crate::analysis::AnalyzerEpoch,
    deadline: &Deadline,
) -> Result<()> {
    archived_capture_options_at(snapshot, value, epoch, deadline).map(|_| ())
}

fn archived_capture_options_at(
    snapshot: &Snapshot,
    value: &serde_json::Value,
    epoch: crate::analysis::AnalyzerEpoch,
    deadline: &Deadline,
) -> Result<CaptureOptions> {
    let reader = epoch.producer();
    deadline.check()?;
    validate_archived_manifest_shape(snapshot, deadline)?;
    if !value.is_null() {
        let nodes = value
            .get("nodes")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| anyhow::anyhow!("Archived capture chain missing"))?;
        ensure!(
            value
                .get("schemaVersion")
                .and_then(serde_json::Value::as_u64)
                == Some(1)
                && !nodes.is_empty()
                && nodes.len() <= 16,
            "Unknown archived capture proof schema/capacity"
        );
        for node in nodes {
            deadline.check()?;
            ensure!(
                node.get("snapshot")
                    .and_then(|s| s.get("files"))
                    .and_then(serde_json::Value::as_array)
                    .is_some_and(|files| files.len() <= 100_000),
                "Archived raw source capacity exceeded"
            );
        }
    }

    let mut budget = ArchivedProofBudget {
        bytes: 0,
        next_check: 0,
        deadline,
    };
    serde_json::to_writer(&mut budget, snapshot)?;
    serde_json::to_writer(&mut budget, value)?;
    if value.is_null() {
        let (options, historical) = archived_native_options_at(snapshot, None, reader)?;
        validate_manifest_options_with(snapshot, &options, historical, &|| deadline.check())?;
        return Ok(options);
    }
    let archive: ArchivedCaptureScope = serde_json::from_value(value.clone())?;
    ensure!(
        serde_json::to_value(&archive)? == *value
            && archive.schema_version == 1
            && !archive.nodes.is_empty()
            && archive.nodes.len() <= 16,
        "Unknown archived capture proof schema"
    );
    ensure!(
        identity(&archive.nodes[0].snapshot) == identity(snapshot),
        "Archived source/proof manifest mismatch"
    );
    let mut ignored_members = None;
    let mut options = None;
    let mut historical = false;
    let mut visited = BTreeSet::new();
    for index in (0..archive.nodes.len()).rev() {
        deadline.check()?;
        let node = &archive.nodes[index];
        let source = &node.snapshot;
        ensure!(
            source.workspace_id == snapshot.workspace_id
                && crate::model::valid_id(&source.id)
                && crate::model::valid_id(&source.workspace_id)
                && visited.insert(source.id.clone()),
            "Archived capture workspace/identity/chain mismatch"
        );
        validate_archived_manifest_shape(source, deadline)?;
        match &node.proof {
            ArchivedCaptureProof::Git { record } => {
                ensure!(
                    source.kind == "derived" && index + 1 < archive.nodes.len(),
                    "Archived Git origin missing"
                );
                let origin = &archive.nodes[index + 1].snapshot;
                let inherited = options
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("Archived Git origin scope missing"))?;
                ignored::transition(origin, source, inherited, ignored_members.as_ref(), &|| {
                    deadline.check()
                })?;
                crate::git_workflows::validate_archived_source(
                    source, record, origin, inherited, deadline,
                )?;
            }
            ArchivedCaptureProof::IgnoredNative { record } => {
                ensure!(
                    index + 1 == archive.nodes.len(),
                    "Archived ignored native proof is not terminal"
                );
                ignored_members = Some(ignored::validate(record, source, reader, &|| {
                    deadline.check()
                })?);
                options = Some(record.options.clone());
                historical = record.analyzer != reader;
            }
            ArchivedCaptureProof::Native { options: record } => {
                ensure!(
                    index + 1 == archive.nodes.len(),
                    "Archived native proof is not terminal"
                );
                let recovered = archived_native_options_at(source, record.as_ref(), reader)?;
                options = Some(recovered.0);
                historical = recovered.1;
            }
            ArchivedCaptureProof::Derived { record } => {
                ensure!(
                    source.kind == "derived" && index + 1 < archive.nodes.len(),
                    "Archived derived origin missing"
                );
                let origin = &archive.nodes[index + 1].snapshot;
                let inherited = options
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("Archived derived options missing"))?;
                ignored::transition(origin, source, inherited, ignored_members.as_ref(), &|| {
                    deadline.check()
                })?;
                ensure!(
                    record.snapshot_id == source.id
                        && record.workspace_id == source.workspace_id
                        && record.origin_snapshot_id == origin.id
                        && record.origin_snapshot_hash == identity(origin)
                        && record.derived_snapshot_hash == identity(source)
                        && record.options_hash == identity(inherited),
                    "Archived derived capture binding mismatch"
                );
            }
            ArchivedCaptureProof::Overlay { record } => {
                ensure!(
                    source.kind == "overlay" && index + 1 < archive.nodes.len(),
                    "Archived overlay origin missing"
                );
                let base = &archive.nodes[index + 1].snapshot;
                ignored::transition(
                    base,
                    source,
                    options
                        .as_ref()
                        .ok_or_else(|| anyhow::anyhow!("Archived overlay options missing"))?,
                    ignored_members.as_ref(),
                    &|| deadline.check(),
                )?;
                ensure!(
                    base.kind == "disk"
                        && record.snapshot_id == source.id
                        && record.base_snapshot_id == base.id
                        && crate::model::valid_id(&record.session_id)
                        && record.versions.len() <= 1000
                        && record
                            .versions
                            .values()
                            .all(|version| *version <= i32::MAX as u32)
                        && source.omissions == base.omissions
                        && source.capture_consistency
                            == "overlay-from-stable-disk-and-immutable-buffers",
                    "Archived overlay receipt mismatch"
                );
                let epoch = [
                    reader,
                    "yoke-tree-sitter-4",
                    "yoke-tree-sitter-6",
                    "yoke-tree-sitter-7",
                    "yoke-tree-sitter-8",
                    "yoke-tree-sitter-9",
                ]
                .into_iter()
                .find(|epoch| {
                    source.id
                        == identity(&(
                            epoch,
                            &base.id,
                            &record.session_id,
                            record.revision,
                            &record.versions,
                            &source.files,
                        ))
                })
                .ok_or_else(|| anyhow::anyhow!("Archived overlay native identity mismatch"))?;
                let mut files: BTreeMap<_, _> = source
                    .files
                    .iter()
                    .map(|file| (file.path.as_str(), file))
                    .collect();
                for path in record.versions.keys() {
                    deadline.check()?;
                    normalized(path)?;
                    ensure!(
                        files.contains_key(path.as_str()),
                        "Archived overlay version outside manifest"
                    );
                }
                for file in &base.files {
                    deadline.check()?;
                    let current = files
                        .remove(file.path.as_str())
                        .ok_or_else(|| anyhow::anyhow!("Archived overlay dropped source"))?;
                    ensure!(
                        record.versions.contains_key(&file.path) || current == file,
                        "Archived unversioned overlay changed"
                    );
                }
                for path in files.keys() {
                    deadline.check()?;
                    ensure!(
                        record.versions.contains_key(*path),
                        "Archived new overlay lacks version"
                    );
                }
                historical = epoch != reader;
            }
        }
        validate_manifest_options_with(
            source,
            options
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Archived capture options missing"))?,
            historical,
            &|| deadline.check(),
        )?;
    }
    options.ok_or_else(|| anyhow::anyhow!("Archived capture options missing"))
}

#[cfg(test)]
mod archive_scope_tests {
    use super::*;
    fn persist(store: &Store, snapshot: &Snapshot) {
        store
            .db
            .execute(
                "INSERT INTO snapshots VALUES(?1,?2)",
                rusqlite::params![snapshot.id, serde_json::to_string(snapshot).unwrap()],
            )
            .unwrap();
    }
    #[test]
    fn exact_native_archive_identity_denies_resealed_protected_members_and_future_fields() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("main.rs"), "fn actual(){}\n").unwrap();
        let store = Store::open(root.path()).unwrap();
        let deadline = Deadline::new(10000);
        let snapshot = capture(root.path(), &store, &deadline).unwrap();
        persist(&store, &snapshot);
        let proof = archived_capture_scope(&store, &snapshot, &deadline).unwrap();
        validate_archived_capture_scope(&snapshot, &proof, &deadline).unwrap();
        validate_archived_capture_scope(&snapshot, &serde_json::Value::Null, &deadline).unwrap();
        let mut resealed = snapshot.clone();
        resealed.files[0].path = ".env".into();
        let mut altered = proof.clone();
        altered["nodes"][0]["snapshot"] = serde_json::to_value(&resealed).unwrap();
        assert!(validate_archived_capture_scope(&resealed, &altered, &deadline).is_err());
        assert!(
            validate_archived_capture_scope(&resealed, &serde_json::Value::Null, &deadline)
                .is_err()
        );
        let mut unknown = proof.clone();
        unknown["nodes"][0]["snapshot"]["futureSourceGrant"] = serde_json::json!(true);
        assert!(validate_archived_capture_scope(&snapshot, &unknown, &deadline).is_err());
        let mut unknown = snapshot.clone();
        unknown.id = identity(&("future-analyzer", &snapshot.files));
        assert!(
            validate_archived_capture_scope(&unknown, &serde_json::Value::Null, &deadline).is_err()
        );
    }
    #[test]
    fn offline_dependency_and_derived_scope_proofs_require_exact_options_and_origin_chain() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("target")).unwrap();
        fs::write(root.path().join("target/main.custom"), "fn actual(){}\n").unwrap();
        let store = Store::open(root.path()).unwrap();
        let deadline = Deadline::new(10000);
        let options = CaptureOptions {
            only_dirs: vec!["target".into()],
            read_only_dependency_dirs: vec!["target".into()],
            language_overrides: BTreeMap::from([("custom".into(), "rust".into())]),
            ..Default::default()
        };
        let snapshot = capture_with_options(root.path(), &store, &options, &deadline).unwrap();
        persist(&store, &snapshot);
        let mut derived = snapshot.clone();
        derived.kind = "derived".into();
        derived.id = identity(&("guarded-producer", &snapshot.id, &derived.files));
        persist(&store, &derived);
        inherit_options(&store, &snapshot, &derived).unwrap();
        let proof = archived_capture_scope(&store, &derived, &deadline).unwrap();
        let configured = archived_capture_scope(&store, &snapshot, &deadline).unwrap();
        drop(store);
        validate_archived_capture_scope(&derived, &proof, &deadline).unwrap();
        validate_archived_capture_scope(&snapshot, &configured, &deadline).unwrap();
        assert!(
            validate_archived_capture_scope(&snapshot, &serde_json::Value::Null, &deadline)
                .is_err()
        );
        let mut wrong = proof.clone();
        wrong["nodes"][0]["proof"]["record"]["optionsHash"] = serde_json::json!("0".repeat(64));
        assert!(validate_archived_capture_scope(&derived, &wrong, &deadline).is_err());
        let mut wrong = configured.clone();
        wrong["nodes"][0]["proof"]["options"]["options"]["readOnlyDependencyDirs"] =
            serde_json::json!([]);
        assert!(validate_archived_capture_scope(&snapshot, &wrong, &deadline).is_err());
    }
    #[test]
    fn released_capture_and_overlay_receipts_remain_exact_offline_after_workspace_closes() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("old.ps1"), "function Original {}\n").unwrap();
        let store = Store::open(root.path()).unwrap();
        let deadline = Deadline::new(10000);
        let mut base = capture(root.path(), &store, &deadline).unwrap();
        base.files[0].language = "text".into();
        base.id = identity(&(
            &store.workspace_id,
            "disk",
            "yoke-tree-sitter-4",
            &base.files,
            &base.omissions,
        ));
        persist(&store, &base);
        let session_id = "c".repeat(64);
        let versions = BTreeMap::<String, u32>::new();
        let mut overlay = base.clone();
        overlay.kind = "overlay".into();
        overlay.capture_consistency = "overlay-from-stable-disk-and-immutable-buffers".into();
        overlay.id = identity(&(
            "yoke-tree-sitter-4",
            &base.id,
            &session_id,
            1u64,
            &versions,
            &overlay.files,
        ));
        persist(&store, &overlay);
        store.record("overlay-snapshot",&overlay.id,&serde_json::json!({"snapshotId":overlay.id,"sessionId":session_id,"revision":1,"versions":versions,"baseSnapshotId":base.id})).unwrap();
        let proof = archived_capture_scope(&store, &overlay, &deadline).unwrap();
        drop(store);
        validate_archived_capture_scope(&overlay, &proof, &deadline).unwrap();
        validate_archived_capture_scope(&base, &serde_json::Value::Null, &deadline).unwrap();
        assert_eq!(
            proof["nodes"][0]["snapshot"]["files"][0]["language"],
            "text"
        );
        let mut tampered = proof.clone();
        tampered["nodes"][0]["proof"]["record"]["revision"] = serde_json::json!(2);
        assert!(validate_archived_capture_scope(&overlay, &tampered, &deadline).is_err());
        assert!(
            validate_archived_capture_scope(&overlay, &serde_json::Value::Null, &deadline).is_err()
        );
    }
}

#[cfg(test)]
#[path = "git_ignore_policy_tests.rs"]
mod git_ignore_policy_tests;
