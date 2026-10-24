//! Actual Git/index/blob and CLI contracts, with separately owned merge/worktree fixtures.
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
use yoke::{
    Engine,
    analysis::AnalyzerEpoch,
    capture::Deadline,
    git_workflows::{
        HostGit,
        conflicts::{self, Options, ReadRequest},
    },
    model::{Snapshot, hash, identity},
};
const FILE: &str = "café.rs";
fn text(n: u8) -> String {
    format!("// café 🦀\r\npub fn value()->u32{{{n}}}\r\n")
}
fn command(g: &HostGit, root: &Path, args: &[&str], input: &[u8], ok: bool) -> Vec<u8> {
    let null = if cfg!(windows) { "NUL" } else { "/dev/null" };
    let mut child = Command::new(&g.executable)
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", null)
        .env("GIT_CONFIG_SYSTEM", null)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .args([
            "-c",
            "init.templateDir=",
            "-c",
            "core.autocrlf=false",
            "-c",
            "core.hooksPath=",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "user.name=Conflict Fixture",
            "-c",
            "user.email=fixture@example.invalid",
        ])
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    let result = child.wait_with_output().unwrap();
    if ok {
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    } else {
        assert_eq!(
            result.status.code(),
            Some(1),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    result.stdout
}
fn git(g: &HostGit, root: &Path, args: &[&str]) -> String {
    String::from_utf8(command(g, root, args, &[], true))
        .unwrap()
        .trim()
        .into()
}
struct Fixture {
    owner: tempfile::TempDir,
    root: PathBuf,
    engine: Engine,
    scope: Snapshot,
    git: HostGit,
}
impl Fixture {
    fn new(format: &str, epoch: AnalyzerEpoch) -> Self {
        let owner = tempfile::tempdir().unwrap();
        let root = owner.path().canonicalize().unwrap().join("repo");
        std::fs::create_dir(&root).unwrap();
        let g = HostGit::discover().unwrap();
        git(
            &g,
            &root,
            &[
                "init",
                "-q",
                "-b",
                "main",
                &format!("--object-format={format}"),
            ],
        );
        std::fs::write(root.join(".gitignore"), ".yoke/\n").unwrap();
        std::fs::write(root.join(FILE), text(0)).unwrap();
        git(&g, &root, &["add", "--all"]);
        git(&g, &root, &["commit", "-q", "-m", "base"]);
        git(&g, &root, &["branch", "side"]);
        std::fs::write(root.join(FILE), text(1)).unwrap();
        git(&g, &root, &["commit", "-q", "-am", "ours"]);
        git(&g, &root, &["checkout", "-q", "side"]);
        std::fs::write(root.join(FILE), text(2)).unwrap();
        git(&g, &root, &["commit", "-q", "-am", "theirs"]);
        git(&g, &root, &["checkout", "-q", "main"]);
        command(&g, &root, &["merge", "--no-edit", "side"], &[], false);
        let mut engine = Engine::open(&root).unwrap();
        engine.capture_epoch = epoch;
        let scope = engine.index(&Deadline::new(30_000)).unwrap().0;
        Self {
            owner,
            root,
            engine,
            scope,
            git: g,
        }
    }
    fn scope_ids(&self) -> Vec<String> {
        vec![self.scope.id.clone()]
    }
    fn inspect(&self, options: &Options) -> anyhow::Result<conflicts::ConflictReport> {
        let ids = if options.allow_git_sources {
            self.scope_ids()
        } else {
            vec![]
        };
        conflicts::inspect(
            &self.engine.store,
            &self.root,
            &ids,
            &self.git,
            options,
            &Deadline::new(30_000),
        )
    }
    fn facts(&self) -> Value {
        let mut tables = BTreeMap::new();
        for table in ["snapshots", "graphs", "records", "meta", "cache"] {
            let cols = if table == "records" {
                "kind,id,value"
            } else if ["meta", "cache"].contains(&table) {
                "key,value"
            } else {
                "id,value"
            };
            let mut q = self
                .engine
                .store
                .db
                .prepare(&format!("SELECT {cols} FROM {table} ORDER BY 1,2"))
                .unwrap();
            let count = if table == "records" { 3 } else { 2 };
            let rows = q
                .query_map([], |r| {
                    (0..count)
                        .map(|n| r.get::<_, String>(n))
                        .collect::<Result<Vec<_>, _>>()
                })
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            tables.insert(table, rows);
        }
        let mut blobs = BTreeMap::new();
        for entry in std::fs::read_dir(self.root.join(".yoke/blobs")).unwrap() {
            let p = entry.unwrap().path();
            if p.is_file() {
                blobs.insert(
                    p.file_name().unwrap().to_string_lossy().to_string(),
                    hash(std::fs::read(p).unwrap()),
                );
            }
        }
        let index = PathBuf::from(git(
            &self.git,
            &self.root,
            &["rev-parse", "--path-format=absolute", "--git-path", "index"],
        ));
        assert!(
            index
                .canonicalize()
                .unwrap()
                .starts_with(self.owner.path().canonicalize().unwrap())
        );
        json!({"tables":tables,"blobs":blobs,"index":hash(std::fs::read(index).unwrap()),"head":git(&self.git,&self.root,&["rev-parse","HEAD"]),"latest":self.engine.store.latest().unwrap()})
    }
}
fn sources() -> Options {
    Options {
        allow_index_read: true,
        allow_git_sources: true,
        ..Default::default()
    }
}
fn metadata() -> Options {
    Options {
        allow_index_read: true,
        ..Default::default()
    }
}
#[test]
fn actual_sha1_sha256_conflicts_preserve_index_and_source_spaces_and_replay_epochs8_and9() {
    for (format, epoch) in [("sha1", AnalyzerEpoch::V8), ("sha256", AnalyzerEpoch::V9)] {
        let f = Fixture::new(format, epoch);
        let before = f.facts();
        let working = std::fs::read(f.root.join(FILE)).unwrap();
        assert!(String::from_utf8_lossy(&working).contains("<<<<<<<"));
        let report = f.inspect(&sources()).unwrap();
        let v = report.value();
        assert_eq!(v["objectFormat"], format);
        assert_eq!(v["conflicts"].as_array().unwrap().len(), 1);
        assert_eq!(v["conflicts"][0]["stagePresence"], json!([1, 2, 3]));
        assert_eq!(v["conflicts"][0]["indexPattern"], "three_way_unmerged");
        for (i, expected) in [text(0), text(1), text(2)].into_iter().enumerate() {
            let row = &v["stageRows"][i];
            assert_eq!(row["path"], FILE);
            assert_eq!(row["stage"], i + 1);
            assert_eq!(row["quote"]["text"], expected);
            assert_eq!(row["contentSha256"], hash(expected.as_bytes()));
            assert_eq!(row["gitBlobHashVerified"], true);
            assert_eq!(row["quote"]["coordinateSpace"], "git_blob_utf8");
            assert_eq!(row["scopeSnapshotIds"], json!([f.scope.id]));
            assert_ne!(row["contentSha256"], hash(&working));
        }
        assert_eq!(v["matchingIndexObservations"], 2);
        assert_eq!(v["complete"], false);
        assert_eq!(v["canonicalSnapshotSourcesRead"], false);
        assert_eq!(v["workingFilesRead"], false);
        let mut copy = v.clone();
        let id = copy.as_object_mut().unwrap().remove("resultId").unwrap();
        assert_eq!(id, identity(&copy));
        assert_eq!(
            serde_json::from_slice::<Value>(report.canonical_bytes()).unwrap(),
            *v
        );
        assert_eq!(
            f.inspect(&sources()).unwrap().canonical_bytes(),
            report.canonical_bytes()
        );
        assert_eq!(f.facts(), before);
        assert_eq!(std::fs::read(f.root.join(FILE)).unwrap(), working);
        let meta = f.inspect(&metadata()).unwrap();
        assert_eq!(meta.value()["consumed"]["uniqueBlobObjects"], 0);
        assert!(
            meta.value()["stageRows"]
                .as_array()
                .unwrap()
                .iter()
                .all(|r| r["quote"].is_null())
        );
    }
}
#[test]
fn exact_source_pages_bind_the_observed_index_and_never_use_changed_working_files() {
    let f = Fixture::new("sha1", AnalyzerEpoch::V8);
    let full = f.inspect(&sources()).unwrap();
    let mut o = sources();
    let start = text(1).find('🦀').unwrap();
    let request = ReadRequest {
        schema_version: 1,
        expected_index_hash: full.value()["indexHash"].as_str().unwrap().into(),
        path: FILE.into(),
        stage: 2,
        start_byte: start,
        max_bytes: 4,
    };
    o.read_request = Some(request.clone());
    let page = f.inspect(&o).unwrap();
    assert_eq!(page.value()["selectedSourcePage"]["range"]["text"], "🦀");
    assert_eq!(page.value()["consumed"]["uniqueBlobObjects"], 1);
    // The current worktree text is deliberately removed after its scope capture.
    // Only immutable stage objects supply this command's quoted source space.
    std::fs::rename(f.root.join(FILE), f.owner.path().join("held-original.rs")).unwrap();
    assert_eq!(
        f.inspect(&o).unwrap().canonical_bytes(),
        page.canonical_bytes()
    );
    let mut stale = request.clone();
    stale.expected_index_hash = "0".repeat(64);
    o.read_request = Some(stale);
    assert!(
        f.inspect(&o)
            .unwrap_err()
            .to_string()
            .contains("index_changed")
    );
    let mut invalid = request.clone();
    invalid.start_byte = start + 1;
    o.read_request = Some(invalid);
    assert!(f.inspect(&o).is_err());
    let mut zero = request;
    zero.max_bytes = 0;
    o.read_request = Some(zero);
    let empty = f.inspect(&o).unwrap();
    assert_eq!(empty.value()["selectedSourcePage"]["range"]["text"], "");
    assert_eq!(
        empty.value()["selectedSourcePage"]["range"]["completeSource"],
        false
    );
}
#[test]
fn cumulative_blob_quote_scope_and_output_limits_are_independent_and_never_write_state() {
    let f = Fixture::new("sha1", AnalyzerEpoch::V8);
    let full = f.inspect(&sources()).unwrap();
    let before = f.facts();
    for fault in [
        "index",
        "paths",
        "rows",
        "objects",
        "blob",
        "source",
        "scope_files",
        "scope_bytes",
        "output",
    ] {
        let mut o = sources();
        let c = &full.value()["consumed"];
        match fault {
            "index" => {
                o.limits.max_index_bytes =
                    c["indexBytesPerObservation"].as_u64().unwrap() as usize - 1
            }
            "paths" => o.limits.max_paths = 0,
            "rows" => o.limits.max_stage_rows = 2,
            "objects" => o.limits.max_blob_objects = 2,
            "blob" => o.limits.max_blob_bytes = 1,
            "source" => {
                o.limits.max_source_bytes = c["verifiedGitBlobBytes"].as_u64().unwrap() as usize - 1
            }
            "scope_files" => o.limits.max_scope_files = 0,
            "scope_bytes" => o.limits.max_scope_metadata_bytes = 1,
            "output" => o.limits.max_output_bytes = 1,
            _ => unreachable!(),
        }
        assert!(f.inspect(&o).is_err(), "{fault}");
        assert_eq!(f.facts(), before, "{fault}");
    }
    let mut no_quotes = sources();
    no_quotes.limits.max_quote_bytes = 0;
    let v = f.inspect(&no_quotes).unwrap();
    assert!(
        v.value()["stageRows"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["quote"].is_null())
    );
    assert_eq!(v.value()["consumed"]["quotedBytes"], 0);
    assert_eq!(f.facts(), before);
    // The selected numeric limit is itself serialized. Calibrate equal-width
    // values before checking the exact library envelope and one byte below it.
    let mut exact = sources();
    exact.limits.max_output_bytes = 10_000;
    let size = f.inspect(&exact).unwrap().canonical_bytes().len();
    exact.limits.max_output_bytes = size;
    let size = f.inspect(&exact).unwrap().canonical_bytes().len();
    exact.limits.max_output_bytes = size;
    assert_eq!(f.inspect(&exact).unwrap().canonical_bytes().len(), size);
    assert_eq!(size.to_string().len(), (size - 1).to_string().len());
    exact.limits.max_output_bytes = size - 1;
    assert!(f.inspect(&exact).is_err());
    assert_eq!(f.facts(), before);
}
#[test]
fn permission_root_scope_caller_transaction_deadline_and_sql_policy_fail_closed() {
    use rusqlite::limits::Limit;
    let f = Fixture::new("sha1", AnalyzerEpoch::V8);
    let before = f.facts();
    assert!(
        f.inspect(&Options::default())
            .unwrap_err()
            .to_string()
            .contains("permission_required")
    );
    f.engine.store.db.execute_batch("BEGIN;").unwrap();
    assert!(f.inspect(&sources()).is_err());
    f.engine.store.db.execute_batch("ROLLBACK;").unwrap();
    assert!(
        conflicts::inspect(
            &f.engine.store,
            &f.root,
            &f.scope_ids(),
            &f.git,
            &sources(),
            &Deadline::new(0)
        )
        .is_err()
    );
    let other = tempfile::tempdir().unwrap();
    assert!(
        conflicts::inspect(
            &f.engine.store,
            other.path(),
            &f.scope_ids(),
            &f.git,
            &sources(),
            &Deadline::new(10_000)
        )
        .is_err()
    );
    let prior = f
        .engine
        .store
        .db
        .set_limit(Limit::SQLITE_LIMIT_LENGTH, 32)
        .unwrap();
    assert!(f.inspect(&sources()).is_err());
    assert_eq!(
        f.engine.store.db.limit(Limit::SQLITE_LIMIT_LENGTH).unwrap(),
        32
    );
    f.engine
        .store
        .db
        .set_limit(Limit::SQLITE_LIMIT_LENGTH, prior)
        .unwrap();
    assert_eq!(f.facts(), before);
    let bad = HostGit {
        executable: f.owner.path().join("missing-git.exe"),
    };
    let mut o = sources();
    o.limits.max_scope_metadata_bytes = 1;
    assert!(
        conflicts::inspect(
            &f.engine.store,
            &f.root,
            &f.scope_ids(),
            &bad,
            &o,
            &Deadline::new(10_000)
        )
        .unwrap_err()
        .to_string()
        .contains("capacity_exceeded")
    );
}
#[test]
fn cli_uses_existing_readonly_state_preserves_exact_envelope_and_refuses_unused_global_authority() {
    let f = Fixture::new("sha1", AnalyzerEpoch::V8);
    let before = f.facts();
    let expected = f.inspect(&sources()).unwrap();
    let call = |max: usize, extra: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_yoke"))
            .arg("--root")
            .arg(&f.root)
            .args(["--max-bytes", &max.to_string()])
            .args(extra)
            .args([
                "git",
                "conflicts",
                "--trust-git",
                "--allow-index-read",
                "--allow-git-sources",
                "--scope-snapshot",
                &f.scope.id,
            ])
            .output()
            .unwrap()
    };
    let out = call(expected.canonical_bytes().len() + 1, &[]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        &out.stdout[..out.stdout.len() - 1],
        expected.canonical_bytes()
    );
    assert_eq!(out.stdout.last(), Some(&b'\n'));
    let out = call(expected.canonical_bytes().len(), &[]);
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    for flags in [
        vec!["--allow-writes"],
        vec!["--analyzer-epoch", "9"],
        vec!["--snapshot", f.scope.id.as_str()],
        vec!["--trust-servers"],
    ] {
        let out = call(4_000_000, &flags);
        assert!(!out.status.success());
        assert!(out.stdout.is_empty());
    }
    let source_request = if cfg!(windows) {
        f.owner.path().join("page.json")
    } else {
        f.owner.path().canonicalize().unwrap().join("page.json")
    };
    let r = ReadRequest {
        schema_version: 1,
        expected_index_hash: expected.value()["indexHash"].as_str().unwrap().into(),
        path: FILE.into(),
        stage: 3,
        start_byte: 0,
        max_bytes: 12,
    };
    std::fs::write(&source_request, serde_json::to_vec(&r).unwrap()).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(&f.root)
        .args([
            "--max-bytes",
            "4000000",
            "git",
            "conflicts",
            "--trust-git",
            "--allow-index-read",
            "--allow-git-sources",
            "--scope-snapshot",
            &f.scope.id,
            "--read-source",
        ])
        .arg(&source_request)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["selectedSourcePage"]["request"]["stage"], 3);
    assert_eq!(v["consumed"]["uniqueBlobObjects"], 1);
    assert_eq!(f.facts(), before);
}
#[test]
fn cli_default_denials_bad_host_paths_and_closed_limits_do_not_initialize_state() {
    let root = tempfile::tempdir().unwrap();
    for flags in [vec![], vec!["--allow-index-read"], vec!["--trust-git"]] {
        let out = Command::new(env!("CARGO_BIN_EXE_yoke"))
            .arg("--root")
            .arg(root.path())
            .args(["git", "conflicts"])
            .args(flags)
            .output()
            .unwrap();
        assert!(!out.status.success());
        assert!(out.stdout.is_empty());
        assert!(String::from_utf8_lossy(&out.stderr).contains("permission_required"));
        assert!(!root.path().join(".yoke").exists());
    }
    let out = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(root.path())
        .args([
            "git",
            "conflicts",
            "--allow-index-read",
            "--trust-git",
            "--limits",
            "relative.json",
        ])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("input_invalid"));
    assert!(!root.path().join(".yoke").exists());
    let file = root.path().join("limits.json");
    let mut v = serde_json::to_value(conflicts::Limits::default()).unwrap();
    v["allowGitSources"] = json!(true);
    std::fs::write(&file, serde_json::to_vec(&v).unwrap()).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(root.path())
        .args([
            "git",
            "conflicts",
            "--allow-index-read",
            "--trust-git",
            "--limits",
        ])
        .arg(&file)
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    assert!(!root.path().join(".yoke").exists());
}
#[test]
fn a_clean_index_is_an_observation_and_never_a_claim_that_branch_merging_is_safe() {
    let f = Fixture::new("sha1", AnalyzerEpoch::V8);
    git(&f.git, &f.root, &["merge", "--abort"]);
    let before = f.facts();
    for o in [metadata(), sources()] {
        let v = f.inspect(&o).unwrap();
        assert!(v.value()["conflicts"].as_array().unwrap().is_empty());
        assert!(v.value()["stageRows"].as_array().unwrap().is_empty());
        assert_eq!(v.value()["indexObservationComplete"], true);
        assert_eq!(v.value()["complete"], false);
        assert_eq!(v.value()["workingTreeResolutionVerified"], false);
    }
    assert_eq!(f.facts(), before);
}
#[test]
fn actual_index_stage_matrix_modes_and_unadmitted_shared_objects_preserve_every_position() {
    let mut f = Fixture::new("sha1", AnalyzerEpoch::V8);
    let names = (1u8..=7)
        .map(|mask| (mask, format!("pattern{mask}.rs")))
        .collect::<Vec<_>>();
    for path in names
        .iter()
        .map(|(_, p)| p.as_str())
        .chain(["link.rs", "sub.rs"])
    {
        std::fs::write(f.root.join(path), "pub fn captured(){}\r\n").unwrap();
    }
    f.scope = f.engine.index(&Deadline::new(30_000)).unwrap().0;
    git(
        &f.git,
        &f.root,
        &["update-index", "--force-remove", "--", FILE],
    );
    let oid = String::from_utf8(command(
        &f.git,
        &f.root,
        &["hash-object", "-w", "--stdin"],
        text(9).as_bytes(),
        true,
    ))
    .unwrap()
    .trim()
    .to_owned();
    let link = String::from_utf8(command(
        &f.git,
        &f.root,
        &["hash-object", "-w", "--stdin"],
        b"../never-open-private-target",
        true,
    ))
    .unwrap()
    .trim()
    .to_owned();
    let private = String::from_utf8(command(
        &f.git,
        &f.root,
        &["hash-object", "-w", "--stdin"],
        b"private fixture bytes that must never be read",
        true,
    ))
    .unwrap()
    .trim()
    .to_owned();
    let commit = git(&f.git, &f.root, &["rev-parse", "HEAD"]);
    let mut input = String::new();
    for (mask, path) in &names {
        for stage in 1..=3 {
            if mask & (1 << (stage - 1)) != 0 {
                input.push_str(&format!("100644 {oid} {stage}\t{path}\0"));
            }
        }
    }
    input.push_str(&format!("120000 {link} 2\tlink.rs\0"));
    input.push_str(&format!("160000 {commit} 2\tsub.rs\0"));
    input.push_str(&format!("100644 {oid} 2\t.env\0"));
    input.push_str(&format!("100644 {private} 2\tcredentials.json\0"));
    input.push_str(&format!("100644 {private} 3\toutside.rs\0"));
    command(
        &f.git,
        &f.root,
        &["update-index", "-z", "--index-info"],
        input.as_bytes(),
        true,
    );
    let before = f.facts();
    let v = f.inspect(&sources()).unwrap();
    let v = v.value();
    assert_eq!(v["consumed"]["unmergedPaths"], 12);
    assert_eq!(v["consumed"]["uniqueBlobObjects"], 1);
    let rows = v["stageRows"].as_array().unwrap();
    for (mask, path) in &names {
        let c = v["conflicts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["path"] == *path)
            .unwrap();
        let stages = (1u8..=3)
            .filter(|s| mask & (1 << (s - 1)) != 0)
            .collect::<Vec<_>>();
        assert_eq!(c["stagePresence"], json!(stages));
        for i in c["stageRowOrdinals"].as_array().unwrap() {
            let row = &rows[i.as_u64().unwrap() as usize];
            assert_eq!(row["path"], *path);
            assert_eq!(row["quote"]["text"], text(9));
        }
        assert_eq!(c["indexResolved"], false);
        assert_eq!(c["causeInferred"], false);
    }
    for path in ["link.rs", "sub.rs"] {
        let row = rows.iter().find(|r| r["path"] == path).unwrap();
        assert_eq!(row["quote"], Value::Null);
        assert_eq!(row["sourceUnavailableReason"], "unsupported_index_mode");
    }
    for path in [".env", "credentials.json"] {
        let row = rows
            .iter()
            .find(|r| r["pathSha256"] == hash(path.as_bytes()))
            .unwrap();
        assert_eq!(row["path"], Value::Null);
        assert_eq!(row["quote"], Value::Null);
    }
    let row = rows.iter().find(|r| r["path"] == "outside.rs").unwrap();
    assert_eq!(
        row["sourceUnavailableReason"],
        "outside_selected_captured_paths"
    );
    assert!(
        !serde_json::to_string(v)
            .unwrap()
            .contains("private fixture bytes")
    );
    assert_eq!(f.facts(), before);
}
#[test]
fn linked_worktrees_are_explicit_independent_index_observations_not_paths_followed_from_git_data() {
    let owner = tempfile::tempdir().unwrap();
    let top = if cfg!(windows) {
        owner.path().to_path_buf()
    } else {
        owner.path().canonicalize().unwrap()
    };
    let main = top.join("main");
    let linked = top.join("linked");
    std::fs::create_dir(&main).unwrap();
    let g = HostGit::discover().unwrap();
    git(&g, &main, &["init", "-q", "-b", "main"]);
    std::fs::write(main.join(".gitignore"), ".yoke/\n").unwrap();
    std::fs::write(main.join(FILE), text(0)).unwrap();
    git(&g, &main, &["add", "--all"]);
    git(&g, &main, &["commit", "-q", "-m", "base"]);
    git(
        &g,
        &main,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "side",
            linked.to_str().unwrap(),
        ],
    );
    std::fs::write(main.join(FILE), text(1)).unwrap();
    git(&g, &main, &["commit", "-q", "-am", "main side"]);
    std::fs::write(linked.join(FILE), text(2)).unwrap();
    git(&g, &linked, &["commit", "-q", "-am", "linked side"]);
    command(&g, &linked, &["merge", "--no-edit", "main"], &[], false);
    assert!(linked.join(".git").is_file());
    let mut m = Engine::open(&main).unwrap();
    let ms = m.index(&Deadline::new(30_000)).unwrap().0;
    let mut l = Engine::open(&linked).unwrap();
    l.capture_epoch = AnalyzerEpoch::V9;
    let ls = l.index(&Deadline::new(30_000)).unwrap().0;
    assert_ne!(m.store.workspace_id, l.store.workspace_id);
    let main_index = hash(std::fs::read(main.join(".git/index")).unwrap());
    let mv = conflicts::inspect(
        &m.store,
        &main,
        &[ms.id],
        &g,
        &sources(),
        &Deadline::new(30_000),
    )
    .unwrap();
    assert!(mv.value()["conflicts"].as_array().unwrap().is_empty());
    let lv = conflicts::inspect(
        &l.store,
        &linked,
        std::slice::from_ref(&ls.id),
        &g,
        &sources(),
        &Deadline::new(30_000),
    )
    .unwrap();
    assert_eq!(lv.value()["conflicts"].as_array().unwrap().len(), 1);
    assert_eq!(lv.value()["stageRows"][1]["quote"]["text"], text(2));
    assert_eq!(lv.value()["stageRows"][2]["quote"]["text"], text(1));
    assert_eq!(lv.value()["worktreeSelection"], "explicit_host_root");
    assert_eq!(
        hash(std::fs::read(main.join(".git/index")).unwrap()),
        main_index
    );
    assert!(
        conflicts::inspect(
            &m.store,
            &linked,
            &[ls.id],
            &g,
            &sources(),
            &Deadline::new(10_000)
        )
        .is_err()
    );
    let nested = linked.join("nested");
    std::fs::create_dir(&nested).unwrap();
    let n = Engine::open(&nested).unwrap();
    assert!(
        conflicts::inspect(
            &n.store,
            &nested,
            &[],
            &g,
            &metadata(),
            &Deadline::new(10_000)
        )
        .is_err()
    );
}
#[test]
fn foreign_or_unproved_scope_metadata_never_authorizes_current_local_git_sources() {
    let f = Fixture::new("sha1", AnalyzerEpoch::V8);
    assert!(f.inspect(&sources()).is_ok());
    let mut foreign = f.scope.clone();
    foreign.id = "4".repeat(64);
    foreign.kind = "federated".into();
    f.engine
        .store
        .db
        .execute(
            "INSERT INTO snapshots(id,value) VALUES(?1,?2)",
            rusqlite::params![foreign.id, serde_json::to_string(&foreign).unwrap()],
        )
        .unwrap();
    assert!(
        conflicts::inspect(
            &f.engine.store,
            &f.root,
            &[foreign.id],
            &f.git,
            &sources(),
            &Deadline::new(10_000)
        )
        .is_err(),
        "Imported snapshot labels cannot grant reading current local Git blobs"
    );
}
#[test]
fn native_git_and_overlay_scope_lineage_remains_valid_but_a_foreign_derived_wrapper_does_not() {
    let mut f = Fixture::new("sha1", AnalyzerEpoch::V8);
    let d = Deadline::new(30_000);
    let commit = git(&f.git, &f.root, &["rev-parse", "HEAD"]);
    let source = yoke::git_workflows::capture_commit_at(
        AnalyzerEpoch::V8,
        &mut f.engine,
        &f.git,
        &f.scope,
        &commit,
        &d,
    )
    .unwrap()
    .0;
    let report = conflicts::inspect(
        &f.engine.store,
        &f.root,
        &[source.id],
        &f.git,
        &sources(),
        &d,
    )
    .unwrap();
    assert_eq!(report.value()["conflicts"].as_array().unwrap().len(), 1);
    let session = yoke::editor::open(&mut f.engine, &d).unwrap();
    yoke::editor::update(
        &f.engine,
        &session.id,
        &json!({"path":FILE,"version":1,"text":"pub fn unsaved(){}\n"}),
        &d,
    )
    .unwrap();
    let overlay = yoke::editor::capture(&mut f.engine, &session.id, &d).unwrap();
    let report = conflicts::inspect(
        &f.engine.store,
        &f.root,
        &[overlay.id],
        &f.git,
        &sources(),
        &d,
    )
    .unwrap();
    assert_eq!(report.value()["stageRows"][1]["quote"]["text"], text(1));
    let mut foreign = f.scope.clone();
    foreign.id = "6".repeat(64);
    foreign.kind = "federated".into();
    f.engine
        .store
        .db
        .execute(
            "INSERT INTO snapshots(id,value) VALUES(?1,?2)",
            rusqlite::params![foreign.id, serde_json::to_string(&foreign).unwrap()],
        )
        .unwrap();
    let mut wrapped = foreign.clone();
    wrapped.id = "7".repeat(64);
    wrapped.kind = "derived".into();
    f.engine
        .store
        .db
        .execute(
            "INSERT INTO snapshots(id,value) VALUES(?1,?2)",
            rusqlite::params![wrapped.id, serde_json::to_string(&wrapped).unwrap()],
        )
        .unwrap();
    yoke::capture::inherit_options(&f.engine.store, &foreign, &wrapped).unwrap();
    assert!(
        conflicts::inspect(
            &f.engine.store,
            &f.root,
            &[wrapped.id],
            &f.git,
            &sources(),
            &d
        )
        .is_err()
    );
}
#[test]
fn explicit_captured_readonly_dependencies_are_read_permissions_and_never_write_permissions() {
    let mut f = Fixture::new("sha1", AnalyzerEpoch::V8);
    let path = "node_modules/pkg/index.js";
    std::fs::create_dir_all(f.root.join("node_modules/pkg")).unwrap();
    std::fs::write(f.root.join(path), "export function captured(){}\n").unwrap();
    f.engine.capture_options.read_only_dependency_dirs = vec!["node_modules".into()];
    f.scope = f.engine.index(&Deadline::new(30_000)).unwrap().0;
    assert!(f.scope.files.iter().any(|file| file.path == path));
    let oid = String::from_utf8(command(
        &f.git,
        &f.root,
        &["hash-object", "-w", "--stdin"],
        b"export function currentIndex(){}\r\n",
        true,
    ))
    .unwrap()
    .trim()
    .to_owned();
    command(
        &f.git,
        &f.root,
        &["update-index", "-z", "--index-info"],
        format!("100644 {oid} 2\t{path}\0").as_bytes(),
        true,
    );
    let before = f.facts();
    let report = f.inspect(&sources()).unwrap();
    let row = report.value()["stageRows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["path"] == path)
        .unwrap();
    assert_eq!(row["quote"]["text"], "export function currentIndex(){}\r\n");
    assert_eq!(f.facts(), before);
    let mut o = sources();
    o.read_request = Some(ReadRequest {
        schema_version: 1,
        expected_index_hash: report.value()["indexHash"].as_str().unwrap().into(),
        path: path.into(),
        stage: 2,
        start_byte: 0,
        max_bytes: 6,
    });
    assert_eq!(
        f.inspect(&o).unwrap().value()["selectedSourcePage"]["range"]["text"],
        "export"
    );
}
