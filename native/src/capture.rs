use crate::{
    model::{Omission, Snapshot, SourceFile, identity},
    paths::{is_link, normalized, protected},
    store::Store,
};
use anyhow::{Result, bail, ensure};
use ignore::WalkBuilder;
use std::{
    fs,
    io::Read,
    path::Path,
    time::{Duration, Instant},
};

#[derive(Clone)]
pub struct Deadline {
    end: Instant,
}
impl Deadline {
    pub fn new(ms: u64) -> Self {
        Self {
            end: Instant::now() + Duration::from_millis(ms),
        }
    }
    pub fn check(&self) -> Result<()> {
        ensure!(Instant::now() < self.end, "Request deadline exceeded");
        Ok(())
    }
    pub fn remaining(&self) -> Duration {
        self.end.saturating_duration_since(Instant::now())
    }
}
pub fn language(path: &str) -> &'static str {
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
        "cpp" | "cc" | "cxx" | "hpp" => "cpp",
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
        "md" | "mdx" | "rst" => "markdown",
        "sql" => "sql",
        "json" | "yaml" | "yml" | "toml" | "xml" => "config",
        _ => "text",
    }
}
fn scan(
    root: &Path,
    store: &Store,
    deadline: &Deadline,
) -> Result<(Vec<SourceFile>, Vec<Omission>)> {
    let walker = WalkBuilder::new(root)
        .hidden(false)
        .follow_links(false)
        .git_ignore(true)
        .git_global(false)
        .git_exclude(true)
        .require_git(false)
        .filter_entry(|entry| {
            !entry
                .path()
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| {
                    matches!(
                        n,
                        ".git"
                            | ".yoke"
                            | ".code-intelligence"
                            | "node_modules"
                            | "target"
                            | ".venv"
                            | "__pycache__"
                    )
                })
        })
        .build();
    let mut files = Vec::new();
    let mut omissions = Vec::new();
    let mut total = 0usize;
    for entry in walker {
        deadline.check()?;
        let entry = entry?;
        if entry.depth() == 0 {
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
        if protected(&relative) {
            omissions.push(Omission {
                path: relative,
                reason: "protected-path".into(),
            });
            continue;
        }
        if normalized(&relative).is_err() {
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
        if metadata.len() > 4_000_000 {
            omissions.push(Omission {
                path: relative,
                reason: "file-size-limit".into(),
            });
            continue;
        }
        let mut bytes = Vec::new();
        fs::File::open(entry.path())?
            .take(4_000_001)
            .read_to_end(&mut bytes)?;
        ensure!(bytes.len() <= 4_000_000, "File grew past capture limit");
        if bytes.contains(&0) || std::str::from_utf8(&bytes).is_err() {
            omissions.push(Omission {
                path: relative,
                reason: "binary-or-invalid-utf8".into(),
            });
            continue;
        }
        if crate::paths::obvious_secret(std::str::from_utf8(&bytes)?) {
            omissions.push(Omission {
                path: relative,
                reason: "obvious-secret".into(),
            });
            continue;
        }
        total = total
            .checked_add(bytes.len())
            .ok_or_else(|| anyhow::anyhow!("Snapshot size overflow"))?;
        ensure!(
            total <= 512_000_000 && files.len() < 100_000,
            "Snapshot capture budget exceeded"
        );
        let hash = store.blob(&bytes)?;
        #[cfg(unix)]
        let mode = {
            use std::os::unix::fs::PermissionsExt;
            metadata.permissions().mode() & 0o777
        };
        #[cfg(not(unix))]
        let mode = 0o644;
        files.push(SourceFile {
            language: language(&relative).into(),
            path: relative,
            hash,
            size: bytes.len(),
            mode,
        });
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    omissions.sort_by(|a, b| a.path.cmp(&b.path));
    Ok((files, omissions))
}
pub fn capture(root: &Path, store: &Store, deadline: &Deadline) -> Result<Snapshot> {
    for _ in 0..3 {
        let (files, omissions) = scan(root, store, deadline)?;
        let second = scan(root, store, deadline)?;
        if (files.clone(), omissions.clone()) == second {
            let id = identity(&(
                &store.workspace_id,
                "disk",
                crate::model::ANALYZER,
                &files,
                &omissions,
            ));
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
