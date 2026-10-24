//! Snapshot-bound supplied test observations and explicit trusted Node collection.
use crate::{
    capture::Deadline,
    model::{Snapshot, hash, identity},
    paths::{is_link, line_starts, normalized, scoped},
    store::Store,
};
use anyhow::{Context, Result, ensure};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    path::Path,
    process::Command,
};
const KIND: &str = "native_test_map_v1";
const MAX_BYTES: usize = 24_000_000;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Selection {
    pub test_id: String,
    pub path: String,
    pub name_pattern: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FileHits {
    path: String,
    content_hash: String,
    lines: Vec<usize>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Observation {
    test_id: String,
    snapshot_id: String,
    executed: bool,
    passed: bool,
    files: Vec<FileHits>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Input {
    snapshot_id: String,
    tests: Vec<Observation>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MapRecord {
    workspace_id: String,
    snapshot_id: String,
    artifact_id: String,
    tests: Vec<Observation>,
    #[serde(default)]
    jobs: Vec<Value>,
}
fn scope(store: &Store, s: &Snapshot, d: &Deadline) -> Result<()> {
    d.check()?;
    store.set_deadline(d)?;
    ensure!(
        s.workspace_id == store.workspace_id && identity(&store.snapshot(&s.id)?) == identity(s),
        "Snapshot scope differs from immutable store"
    );
    Ok(())
}
fn test_id(id: &str) -> Result<()> {
    ensure!(
        !id.is_empty() && id.len() <= 500 && !id.chars().any(char::is_control),
        "Invalid test ID"
    );
    Ok(())
}
fn validated(store: &Store, s: &Snapshot, input: Input, d: &Deadline) -> Result<Vec<Observation>> {
    ensure!(
        input.snapshot_id == s.id && input.tests.len() <= 10_000,
        "Invalid coverage snapshot or test budget"
    );
    let mut ids = BTreeSet::new();
    let sources: BTreeMap<_, _> = s.files.iter().map(|f| (&f.path, f)).collect();
    let mut counts = BTreeMap::new();
    let mut tests = input.tests;
    for t in &mut tests {
        d.check()?;
        test_id(&t.test_id)?;
        ensure!(
            ids.insert(t.test_id.clone())
                && t.snapshot_id == s.id
                && t.executed
                && t.files.len() <= s.files.len(),
            "Duplicate, foreign, or unexecuted test observation"
        );
        let mut seen = BTreeSet::new();
        for f in &mut t.files {
            d.check()?;
            normalized(&f.path)?;
            ensure!(seen.insert(f.path.clone()), "Duplicate coverage path");
            let source = sources
                .get(&f.path)
                .ok_or_else(|| anyhow::anyhow!("Coverage path absent from snapshot"))?;
            ensure!(
                f.content_hash == source.hash && f.lines.len() <= 1_000_000,
                "Coverage hash or line budget mismatch"
            );
            let count = if let Some(count) = counts.get(&f.path) {
                *count
            } else {
                let text = String::from_utf8(store.read_blob(&source.hash)?)?;
                let count = line_starts(&text).len();
                counts.insert(f.path.clone(), count);
                count
            };
            ensure!(
                f.lines.iter().all(|l| *l >= 1 && *l <= count),
                "Coverage line outside captured source"
            );
            f.lines.sort_unstable();
            f.lines.dedup();
        }
        t.files.sort_by(|a, b| a.path.cmp(&b.path));
    }
    tests.sort_by(|a, b| a.test_id.cmp(&b.test_id));
    Ok(tests)
}
fn artifact_value(record: &MapRecord) -> Value {
    json!({"kind":KIND,"workspaceId":record.workspace_id,"snapshotId":record.snapshot_id,"tests":record.tests,"sourceBinding":"hash_verified","assertion":"supplied_observations_not_execution_attestation","exhaustive":false})
}
pub fn import(store: &Store, s: &Snapshot, input: Value, d: &Deadline) -> Result<Value> {
    scope(store, s, d)?;
    ensure!(
        serde_json::to_vec(&input)?.len() <= MAX_BYTES,
        "Coverage input byte budget exceeded"
    );
    let tests = validated(store, s, serde_json::from_value(input)?, d)?;
    publish(store, s, tests, vec![], d)
}
fn publish(
    store: &Store,
    s: &Snapshot,
    tests: Vec<Observation>,
    jobs: Vec<Value>,
    d: &Deadline,
) -> Result<Value> {
    let mut record = MapRecord {
        workspace_id: s.workspace_id.clone(),
        snapshot_id: s.id.clone(),
        artifact_id: String::new(),
        tests,
        jobs,
    };
    let bytes = serde_json::to_vec(&artifact_value(&record))?;
    ensure!(
        bytes.len() <= MAX_BYTES,
        "Coverage output byte budget exceeded"
    );
    d.check()?;
    record.artifact_id = store.artifact(&bytes)?;
    ensure!(
        serde_json::to_vec(&record)?.len() <= MAX_BYTES,
        "Stored coverage byte budget exceeded"
    );
    store.record(KIND, &s.id, &record)?;
    Ok(
        json!({"snapshotId":s.id,"artifactId":record.artifact_id,"testCount":record.tests.len(),"sourceBinding":"hash_verified","assertion":"supplied_observations_not_execution_attestation","exhaustive":false}),
    )
}
fn loaded(store: &Store, s: &Snapshot, d: &Deadline) -> Result<Option<MapRecord>> {
    let value: Option<String> = store
        .db
        .query_row(
            "SELECT value FROM records WHERE kind=?1 AND id=?2",
            params![KIND, s.id],
            |row| row.get(0),
        )
        .optional()?;
    let Some(value) = value else { return Ok(None) };
    ensure!(value.len() <= MAX_BYTES, "Stored map byte budget exceeded");
    let record: MapRecord = serde_json::from_str(&value)?;
    ensure!(
        record.snapshot_id == s.id && record.workspace_id == s.workspace_id,
        "Stored coverage scope mismatch"
    );
    let checked = validated(
        store,
        s,
        Input {
            snapshot_id: record.snapshot_id.clone(),
            tests: record.tests.clone(),
        },
        d,
    )?;
    ensure!(
        serde_json::to_value(&checked)? == serde_json::to_value(&record.tests)?,
        "Stored map is not canonical"
    );
    let artifact = store.read_artifact(&record.artifact_id)?;
    ensure!(
        artifact.len() <= MAX_BYTES
            && serde_json::from_slice::<Value>(&artifact)? == artifact_value(&record),
        "Stored map artifact binding differs"
    );
    Ok(Some(record))
}
fn context_path(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
    path.split('/')
        .any(|p| matches!(p, "test" | "tests" | "__tests__"))
        || name.contains(".test.")
        || name.contains(".spec.")
        || name.starts_with("test_")
        || name.ends_with("_test.go")
        || matches!(
            name.as_str(),
            "package.json"
                | "package-lock.json"
                | "pnpm-lock.yaml"
                | "yarn.lock"
                | "go.mod"
                | "go.sum"
                | "pyproject.toml"
                | "pytest.ini"
                | "cargo.toml"
                | "cargo.lock"
                | "deno.json"
                | "deno.jsonc"
        )
        || name.starts_with("tsconfig")
        || name.contains("jest.config")
        || name.contains("vitest.config")
        || name.contains("babel.config")
        || name.contains("webpack.config")
        || name.contains(".yoke")
}
pub fn select(
    store: &Store,
    base: &Snapshot,
    target: &Snapshot,
    known: &[String],
    d: &Deadline,
) -> Result<Value> {
    scope(store, base, d)?;
    scope(store, target, d)?;
    ensure!(known.len() <= 10_000, "Known test budget exceeded");
    let mut unique = BTreeSet::new();
    for id in known {
        test_id(id)?;
        ensure!(unique.insert(id), "Duplicate known test ID");
    }
    let map = loaded(store, base, d)?;
    let before: BTreeMap<_, _> = base.files.iter().map(|f| (&f.path, &f.hash)).collect();
    let after: BTreeMap<_, _> = target.files.iter().map(|f| (&f.path, &f.hash)).collect();
    let paths: BTreeSet<_> = before.keys().chain(after.keys()).copied().collect();
    let changed: Vec<String> = paths
        .into_iter()
        .filter(|p| before.get(p) != after.get(p))
        .cloned()
        .collect();
    let covered: BTreeSet<_> = map
        .iter()
        .flat_map(|m| m.tests.iter())
        .flat_map(|t| t.files.iter().map(|f| f.path.as_str()))
        .collect();
    let unknown: Vec<_> = changed
        .iter()
        .filter(|p| !covered.contains(p.as_str()))
        .cloned()
        .collect();
    let mut selected: BTreeSet<String> = map
        .iter()
        .flat_map(|m| m.tests.iter())
        .filter(|t| !t.passed || t.files.iter().any(|f| changed.contains(&f.path)))
        .map(|t| t.test_id.clone())
        .collect();
    let broadened = map.is_none()
        || !unknown.is_empty()
        || changed.iter().any(|p| {
            context_path(p)
                || base
                    .files
                    .iter()
                    .chain(&target.files)
                    .any(|f| f.path == *p && f.language == "config")
        })
        || known.iter().any(|id| {
            !map.as_ref()
                .is_some_and(|m| m.tests.iter().any(|t| &t.test_id == id))
        });
    if broadened {
        selected.extend(known.iter().cloned());
        selected.extend(
            map.iter()
                .flat_map(|m| m.tests.iter())
                .map(|t| t.test_id.clone()),
        );
    }
    d.check()?;
    Ok(
        json!({"baseSnapshotId":base.id,"targetSnapshotId":target.id,"tests":selected,"changedPaths":changed,"unknownPaths":unknown,"broadened":broadened,"reason":if broadened{"unknown_coverage_or_execution_context"}else if changed.is_empty(){"no_captured_change_failed_tests_retained"}else{"recorded_per_test_dependencies"},"exhaustive":false,"warning":"Dynamic coverage describes supplied/observed runs only. The full suite remains the final release gate."}),
    )
}
// This host-authored reporter never forwards source test:stdout as protocol records.
const REPORTER: &str = r#"export default async function* report(events) {
 for await (const event of events) {
  if (event.type === 'test:summary' || event.type === 'test:fail' || event.type === 'test:stderr')
   yield JSON.stringify(event, (_k,v) => v instanceof Error ? {name:v.name,message:v.message,stack:v.stack} : v) + '\n';
 }
}
"#;
fn executable(store: &Store, name: &str) -> Result<std::path::PathBuf> {
    ensure!(
        !name.is_empty() && name.len() <= 4096 && !name.chars().any(char::is_control),
        "Invalid host Node executable"
    );
    let path = Path::new(name);
    if path.is_absolute() {
        let resolved = path.canonicalize().context("Node executable unavailable")?;
        ensure!(resolved.is_file(), "Node executable is not a file");
        return Ok(resolved);
    }
    ensure!(
        path.components().count() == 1 && !name.contains(['\\', '/']),
        "Relative Node paths are refused; select an absolute host executable"
    );
    let workspace = store
        .dir
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Store workspace absent"))?
        .canonicalize()?;
    for directory in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
        if !directory.is_absolute() {
            continue;
        }
        let Ok(directory) = directory.canonicalize() else {
            continue;
        };
        if directory.starts_with(&workspace) {
            continue;
        }
        let names = vec![name.to_string()];
        #[cfg(windows)]
        let names = {
            let mut names = names;
            if path.extension().is_none() {
                names.insert(0, format!("{name}.exe"));
            }
            names
        };
        for candidate in names {
            let candidate = directory.join(candidate);
            if let Ok(resolved) = candidate.canonicalize()
                && resolved.is_file()
                && !resolved.starts_with(&workspace)
            {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if fs::metadata(&resolved)?.permissions().mode() & 0o111 == 0 {
                        continue;
                    }
                }
                return Ok(resolved);
            }
        }
    }
    anyhow::bail!("Host Node executable unavailable outside analyzed source")
}
fn complete(out: &crate::process::CapturedOutput) -> bool {
    out.stdin_complete
        && out.output_complete
        && !out.timed_out
        && !out.output_exceeded
        && !out.io_error
}
fn bounded_read(path: &Path, limit: usize, d: &Deadline) -> Result<Vec<u8>> {
    let mut file = fs::File::open(path)?.take((limit as u64).saturating_add(1));
    let mut bytes = Vec::new();
    let mut buffer = [0; 8192];
    loop {
        d.check()?;
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        ensure!(
            bytes.len().saturating_add(count) <= limit,
            "Executed file grew beyond its bound"
        );
        bytes.extend_from_slice(&buffer[..count]);
    }
    d.check()?;
    Ok(bytes)
}
fn integrity(root: &Path, s: &Snapshot, d: &Deadline) -> Result<()> {
    let sources: BTreeMap<_, _> = s.files.iter().map(|f| (f.path.as_str(), f)).collect();
    let mut pending = vec![root.to_path_buf()];
    let mut seen = BTreeSet::new();
    let mut entries = 0usize;
    while let Some(dir) = pending.pop() {
        d.check()?;
        ensure!(
            !is_link(&fs::symlink_metadata(&dir)?),
            "Executed snapshot directory became a link"
        );
        for entry in fs::read_dir(&dir)? {
            d.check()?;
            entries += 1;
            ensure!(
                entries <= 1_000_000,
                "Executed snapshot directory budget exceeded"
            );
            let entry = entry?;
            let meta = fs::symlink_metadata(entry.path())?;
            ensure!(!is_link(&meta), "Executed snapshot contains a link");
            if meta.is_dir() {
                pending.push(entry.path());
                continue;
            }
            ensure!(
                meta.is_file(),
                "Executed snapshot contains a nonregular file"
            );
            let relative = entry
                .path()
                .strip_prefix(root)?
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("Executed snapshot non-UTF8 path"))?
                .replace('\\', "/");
            normalized(&relative)?;
            let file = sources
                .get(relative.as_str())
                .ok_or_else(|| anyhow::anyhow!("Executed snapshot gained an uncaptured file"))?;
            ensure!(
                meta.len() == file.size as u64
                    && hash(bounded_read(&entry.path(), file.size, d)?) == file.hash,
                "Executed source was changed; coverage cannot be hash-bound"
            );
            seen.insert(relative);
        }
    }
    ensure!(seen.len() == s.files.len(), "Executed source was deleted");
    Ok(())
}
fn coverage(
    store: &Store,
    s: &Snapshot,
    root: &Path,
    directory: &Path,
    d: &Deadline,
) -> Result<BTreeMap<String, BTreeSet<usize>>> {
    let mut names: Vec<_> = fs::read_dir(directory)?
        .map(|r| r.map(|e| e.path()))
        .collect::<std::io::Result<_>>()?;
    names.sort();
    ensure!(names.len() <= 1000, "V8 coverage file budget exceeded");
    let mut hits: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
    let mut total = 0u64;
    let mut range_count = 0usize;
    for path in names {
        d.check()?;
        if path.extension().and_then(|p| p.to_str()) != Some("json") {
            continue;
        }
        let meta = fs::symlink_metadata(&path)?;
        ensure!(
            meta.is_file() && !is_link(&meta) && meta.len() <= 16_000_000,
            "Invalid or oversized V8 coverage file"
        );
        total += meta.len();
        ensure!(total <= 64_000_000, "V8 coverage byte budget exceeded");
        let bytes = bounded_read(&path, usize::try_from(meta.len())?, d)?;
        ensure!(
            bytes.len() as u64 == meta.len(),
            "V8 coverage file changed while being read"
        );
        let value: Value = serde_json::from_slice(&bytes)?;
        let scripts = value["result"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("Malformed V8 coverage result"))?;
        ensure!(scripts.len() <= 100_000, "V8 script budget exceeded");
        for script in scripts {
            d.check()?;
            let Some(url) = script["url"].as_str() else {
                continue;
            };
            if url.len() > 8192 || !url.starts_with("file:") {
                continue;
            }
            let Ok(url) = url::Url::parse(url) else {
                continue;
            };
            let Ok(path) = url.to_file_path() else {
                continue;
            };
            let Ok(relative) = path.strip_prefix(root) else {
                continue;
            };
            let Some(relative) = relative.to_str() else {
                continue;
            };
            let relative = relative.replace('\\', "/");
            if normalized(&relative).is_err() {
                continue;
            }
            let Some(file) = s.files.iter().find(|f| f.path == relative) else {
                continue;
            };
            let text = String::from_utf8(store.read_blob(&file.hash)?)?;
            let functions = script["functions"]
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("Malformed captured V8 functions"))?;
            ensure!(functions.len() <= 100_000, "V8 function budget exceeded");
            let mut ranges = Vec::new();
            for function in functions {
                let values = function["ranges"]
                    .as_array()
                    .ok_or_else(|| anyhow::anyhow!("Malformed captured V8 ranges"))?;
                for range in values {
                    range_count += 1;
                    ensure!(range_count <= 1_000_000, "V8 range budget exceeded");
                    let integer = |key: &str| -> Result<usize> {
                        usize::try_from(
                            range[key]
                                .as_u64()
                                .ok_or_else(|| anyhow::anyhow!("Invalid V8 offset"))?,
                        )
                        .context("V8 offset exceeds native size")
                    };
                    let count = range["count"]
                        .as_u64()
                        .ok_or_else(|| anyhow::anyhow!("Invalid V8 hit count"))?;
                    ranges.push((integer("startOffset")?, integer("endOffset")?, count));
                }
            }
            let lines = v8_line_hits(&text, &ranges, d)?;
            hits.entry(relative).or_default().extend(lines);
        }
    }
    Ok(hits)
}
pub fn collect_node(
    store: &Store,
    s: &Snapshot,
    selections: &[Selection],
    node: &str,
    trust: bool,
    d: &Deadline,
) -> Result<Value> {
    ensure!(trust, "Node collection requires explicit host trust");
    scope(store, s, d)?;
    ensure!(
        !selections.is_empty() && selections.len() <= 100,
        "Select 1–100 Node test selections"
    );
    let mut ids = BTreeSet::new();
    for selection in selections {
        test_id(&selection.test_id)?;
        ensure!(
            ids.insert(&selection.test_id),
            "Duplicate test selection ID"
        );
        normalized(&selection.path)?;
        ensure!(
            s.files.iter().any(|f| f.path == selection.path),
            "Selected test absent from snapshot"
        );
        ensure!(
            matches!(
                Path::new(&selection.path)
                    .extension()
                    .and_then(|p| p.to_str()),
                Some("js" | "mjs" | "cjs")
            ),
            "Node collector supports JavaScript test files only; import other runners explicitly"
        );
        if let Some(pattern) = &selection.name_pattern {
            ensure!(
                !pattern.is_empty()
                    && pattern.len() <= 1000
                    && !pattern.chars().any(char::is_control),
                "Invalid bounded Node name pattern"
            );
        }
    }
    let node = executable(store, node)?;
    let directory = tempfile::Builder::new()
        .prefix("yoke-test-map-")
        .tempdir()?;
    let mut version = Command::new(&node);
    version.arg("--version").current_dir(directory.path());
    let version = crate::process::run_capture(
        version,
        &[],
        d,
        crate::process::CaptureLimits {
            stdin_bytes: 0,
            stdout_bytes: 4096,
            stderr_bytes: 4096,
        },
    )?;
    ensure!(
        complete(&version) && version.exit_code == Some(0),
        "Node version capability check failed"
    );
    let version = String::from_utf8(version.stdout)?;
    let pieces: Vec<_> = version
        .trim()
        .strip_prefix('v')
        .ok_or_else(|| anyhow::anyhow!("Invalid Node version"))?
        .split('.')
        .collect();
    let major: usize = pieces
        .first()
        .ok_or_else(|| anyhow::anyhow!("Missing Node major"))?
        .parse()?;
    let minor: usize = pieces
        .get(1)
        .ok_or_else(|| anyhow::anyhow!("Missing Node minor"))?
        .parse()?;
    ensure!(
        major > 22 || major == 22 && minor >= 16,
        "Node 22.16+ required for actual test collection"
    );
    let reporter = directory.path().join("reporter.mjs");
    fs::write(&reporter, REPORTER)?;
    let reporter_url = url::Url::from_file_path(&reporter)
        .map_err(|_| anyhow::anyhow!("Host reporter path cannot be represented as a file URL"))?;
    let mut observations = Vec::new();
    let mut jobs = Vec::new();
    for (index, selection) in selections.iter().enumerate() {
        d.check()?;
        let job = directory.path().join(index.to_string());
        let root = job.join("tree");
        let coverage_dir = job.join("coverage");
        fs::create_dir_all(&coverage_dir)?;
        crate::changes::materialize(store, s, &root, d)?;
        let path = scoped(&root, &selection.path)?;
        let mut command = Command::new(&node);
        command
            .arg("--test")
            .arg("--test-reporter")
            .arg(reporter_url.as_str());
        if let Some(pattern) = &selection.name_pattern {
            command.arg("--test-name-pattern").arg(pattern);
        }
        command
            .arg(&path)
            .current_dir(&root)
            .env("NODE_V8_COVERAGE", &coverage_dir);
        let out = crate::process::run_capture(
            command,
            &[],
            d,
            crate::process::CaptureLimits {
                stdin_bytes: 0,
                stdout_bytes: 1_000_000,
                stderr_bytes: 1_000_000,
            },
        )
        .context("Trusted Node test command unavailable")?;
        // Reject mutation even when the mutated file did not appear in coverage URLs.
        integrity(&root, s, d)?;
        ensure!(
            complete(&out),
            "Node collection did not complete (timedOut={}, outputExceeded={}, ioError={})",
            out.timed_out,
            out.output_exceeded,
            out.io_error
        );
        let hits = coverage(store, s, &root, &coverage_dir, d)?;
        ensure!(
            hits.contains_key(&selection.path),
            "Node did not produce source-bound coverage for the selected test file (exit {:?}, stderr: {})",
            out.exit_code,
            String::from_utf8_lossy(&out.stderr)
                .chars()
                .take(4096)
                .collect::<String>()
        );
        let stdout =
            std::str::from_utf8(&out.stdout).context("Node reporter emitted non-UTF8 protocol")?;
        let mut summaries = Vec::new();
        for line in stdout.lines() {
            d.check()?;
            if line.trim().is_empty() {
                continue;
            }
            let event: Value =
                serde_json::from_str(line).context("Malformed Node reporter protocol")?;
            if event["type"] == "test:summary"
                && event["data"]["file"]
                    .as_str()
                    .is_some_and(|file| Path::new(file) == path)
            {
                summaries.push(event["data"].clone());
            }
        }
        let summary = if summaries.len() == 1 {
            Some(&summaries[0])
        } else {
            None
        };
        let count = |field: &str| {
            summary
                .and_then(|v| v["counts"][field].as_u64())
                .unwrap_or(0)
        };
        let executed = count("passed") + count("failed") + count("cancelled");
        let passed = out.exit_code == Some(0)
            && complete(&out)
            && summary.is_some_and(|v| v["success"] == true)
            && executed > 0
            && count("passed") > 0
            && count("failed") == 0
            && count("cancelled") == 0;
        let files = hits
            .into_iter()
            .map(|(path, lines)| {
                let file = s
                    .files
                    .iter()
                    .find(|f| f.path == path)
                    .expect("coverage source checked");
                FileHits {
                    path,
                    content_hash: file.hash.clone(),
                    lines: lines.into_iter().collect(),
                }
            })
            .collect();
        observations.push(Observation {
            test_id: selection.test_id.clone(),
            snapshot_id: s.id.clone(),
            executed: true,
            passed,
            files,
        });
        let log = json!({"kind":"native_test_log_v1","snapshotId":s.id,"selection":selection,"node":node,"nodeVersion":version.trim(),"exitCode":out.exit_code,"stdout":stdout,"stderr":String::from_utf8_lossy(&out.stderr),"outputComplete":out.output_complete,"outputExceeded":out.output_exceeded,"timedOut":out.timed_out,"ioError":out.io_error});
        let artifact = store.artifact(&serde_json::to_vec(&log)?)?;
        jobs.push(json!({"testId":selection.test_id,"path":selection.path,"selection":selection.name_pattern.as_deref().unwrap_or("test_file"),"exitCode":out.exit_code,"passed":passed,"testCount":executed,"discoveredTestCount":count("tests"),"skippedTestCount":count("skipped"),"outputComplete":out.output_complete,"outputExceeded":out.output_exceeded,"timedOut":out.timed_out,"sourceIntegrity":true,"logArtifactId":artifact}));
    }
    let tests = validated(
        store,
        s,
        Input {
            snapshot_id: s.id.clone(),
            tests: observations,
        },
        d,
    )?;
    let mut result = publish(store, s, tests, jobs.clone(), d)?;
    result["jobs"] = json!(jobs);
    result["granularity"] = json!(if selections.iter().any(|s| s.name_pattern.is_some()) {
        "isolated_name_pattern_selection"
    } else {
        "test_file"
    });
    result["assertion"] = json!("locally_collected_trusted_execution_not_remote_attestation");
    result["networkIsolation"] = json!("not_enforced_trusted_host_process");
    result["limitations"] = json!([
        "A copied directory is not an OS sandbox. Tests retain host filesystem and network privileges.",
        "Coverage lines sample the first nonwhitespace UTF-16 position using the narrowest active range. This describes observed selections and is not exhaustive statement or per-assertion attribution.",
        "Source stdout cannot supply reporter protocol records. No zero-executed or skipped-only selection is labeled passed."
    ]);
    Ok(result)
}
/// Map V8 UTF-16 ranges to native CR/LF lines at their first nonwhitespace character.
/// Nested zero-hit ranges override enclosing hits; equal widths choose zero conservatively.
pub fn v8_line_hits(
    text: &str,
    ranges: &[(usize, usize, u64)],
    d: &Deadline,
) -> Result<Vec<usize>> {
    ensure!(
        text.len() <= 16_000_000 && ranges.len() <= 1_000_000,
        "V8 line mapping budget exceeded"
    );
    d.check()?;
    let total = text.encode_utf16().count();
    let mut requested = Vec::with_capacity(ranges.len().saturating_mul(2));
    type CoverageEvent = (bool, (usize, u64, usize));
    let mut events: BTreeMap<usize, Vec<CoverageEvent>> = BTreeMap::new();
    for (id, &(start, end, count)) in ranges.iter().enumerate() {
        if id.is_multiple_of(512) {
            d.check()?;
        }
        ensure!(
            start <= end && end <= total,
            "V8 range outside source UTF-16 units"
        );
        requested.extend([start, end]);
        if start < end {
            let key = (end - start, count, id);
            events.entry(start).or_default().push((true, key));
            events.entry(end).or_default().push((false, key));
        }
    }
    requested.sort_unstable();
    requested.dedup();
    let mut wanted = 0usize;
    let mut units = 0usize;
    for (index, c) in text.chars().enumerate() {
        if index.is_multiple_of(4096) {
            d.check()?;
        }
        while requested.get(wanted).is_some_and(|p| *p <= units) {
            ensure!(
                requested[wanted] == units,
                "V8 range splits a UTF-16 surrogate pair"
            );
            wanted += 1;
        }
        units += c.len_utf16();
    }
    while let Some(point) = requested.get(wanted) {
        ensure!(*point == units, "V8 range splits a UTF-16 surrogate pair");
        wanted += 1;
    }
    let starts = line_starts(text);
    let mut active: BTreeSet<(usize, u64, usize)> = BTreeSet::new();
    let mut events = events.into_iter().peekable();
    let mut line_units = 0usize;
    let mut hits = Vec::new();
    for (index, &start) in starts.iter().enumerate() {
        d.check()?;
        let end = starts.get(index + 1).copied().unwrap_or(text.len());
        let line = &text[start..end];
        let mut leading = 0usize;
        let mut sample = None;
        for c in line.chars() {
            if !c.is_whitespace() {
                sample = Some(line_units + leading);
                break;
            }
            leading += c.len_utf16();
        }
        line_units += line.encode_utf16().count();
        let Some(sample) = sample else { continue };
        while events.peek().is_some_and(|(point, _)| *point <= sample) {
            let (_, changes) = events.next().unwrap();
            for (insert, key) in changes {
                if insert {
                    active.insert(key);
                } else {
                    active.remove(&key);
                }
            }
        }
        if active.first().is_some_and(|(_, count, _)| *count > 0) {
            hits.push(index + 1);
        }
    }
    d.check()?;
    Ok(hits)
}
