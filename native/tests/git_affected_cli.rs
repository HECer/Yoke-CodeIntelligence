//! CLI contracts for canonical affected output and refusal before Store initialization.
use yoke as test_yoke;
#[path = "support/git_affected_fixture.rs"]
mod fixture;

use fixture::{Edit, Fixture, PATH, hierarchy, state};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};
use tempfile::TempDir;
use yoke::{
    capture::Deadline,
    git_workflows::{
        self,
        affected_details::{self, AffectedOptions},
    },
    model::hash,
};

const INPUT: &str = "git_affected_details_input_invalid";
const CAPACITY: &str = "git_affected_details_capacity_exceeded";
const DEADLINE: &str = "git_affected_details_deadline_exceeded";
const MAX_OPTIONS: usize = 64_000;

fn deadline() -> Deadline {
    Deadline::new(30_000)
}

fn command(root: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_yoke"));
    command.arg("--root").arg(root).args(["--timeout", "30000"]);
    command
}

fn affected(f: &Fixture) -> Command {
    let mut command = command(f.dir.path());
    command.args(["--max-bytes", "4000000", "git", "affected", f.id()]);
    command
}

fn options_path(f: &Fixture, bytes: &[u8]) -> PathBuf {
    let path = f.dir.path().join("R5B_OPTIONS_DO_NOT_ECHO.json");
    fs::write(&path, bytes).unwrap();
    path
}

fn closed_options(options: &AffectedOptions) -> Vec<u8> {
    let value = serde_json::to_value(options).unwrap();
    assert_eq!(
        value.as_object().unwrap().len(),
        13,
        "Frozen required option fields"
    );
    serde_json::to_vec(options).unwrap()
}

fn with_lf(bytes: &[u8]) -> Vec<u8> {
    let mut value = bytes.to_vec();
    value.push(b'\n');
    value
}

fn success(output: Output) -> Vec<u8> {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    output.stdout
}

fn refused(output: Output, marker: &str) {
    assert!(!output.status.success());
    assert!(
        output.stdout.is_empty(),
        "Refusal must have no partial canonical bytes"
    );
    assert_eq!(output.stderr, format!("yoke: {marker}\n").as_bytes());
}

#[test]
fn affected_default_and_explicit_depth_options_match_complete_library_canonical_bytes() {
    let f = hierarchy(Edit::Derived, true);
    let before = state(&f.engine);
    let default = AffectedOptions::default();
    let expected =
        affected_details::derive(&f.engine.store, f.id(), &default, &deadline()).unwrap();
    assert_eq!(
        success(affected(&f).output().unwrap()),
        with_lf(expected.canonical_bytes())
    );
    let finite = AffectedOptions {
        max_hierarchy_depth: 1,
        max_incoming_depth: 1,
        ..Default::default()
    };
    let bytes = closed_options(&finite);
    let path = options_path(&f, &bytes);
    let expected = affected_details::derive(&f.engine.store, f.id(), &finite, &deadline()).unwrap();
    assert_eq!(
        success(affected(&f).arg("--options").arg(&path).output().unwrap()),
        with_lf(expected.canonical_bytes())
    );
    assert_eq!(fs::read(path).unwrap(), bytes);
    assert_eq!(state(&f.engine), before);
}

#[test]
fn affected_stdout_cap_includes_lf_and_refuses_one_byte_below_without_clamping() {
    let f = hierarchy(Edit::Derived, true);
    let before = state(&f.engine);
    let options = AffectedOptions {
        max_hierarchy_depth: 2,
        max_incoming_depth: 2,
        ..Default::default()
    };
    let path = options_path(&f, &closed_options(&options));
    let expected =
        affected_details::derive(&f.engine.store, f.id(), &options, &deadline()).unwrap();
    let exact = with_lf(expected.canonical_bytes());
    assert!(exact.len() > 1 && exact.len() < 4_000_000);
    for (limit, succeeds) in [(exact.len(), true), (exact.len() - 1, false)] {
        let out = command(f.dir.path())
            .args([
                "--max-bytes",
                &limit.to_string(),
                "git",
                "affected",
                f.id(),
                "--options",
            ])
            .arg(&path)
            .output()
            .unwrap();
        if succeeds {
            assert_eq!(success(out), exact);
        } else {
            refused(out, CAPACITY);
        }
        assert_eq!(state(&f.engine), before);
    }
    // A later uncapped request must reproduce the same complete result/identity.
    assert_eq!(
        success(affected(&f).arg("--options").arg(path).output().unwrap()),
        exact
    );
    assert_eq!(state(&f.engine), before);
}

#[test]
fn affected_options_are_closed_duplicate_safe_and_admitted_at_exact_raw_byte_boundary() {
    let f = hierarchy(Edit::Derived, true);
    let before = state(&f.engine);
    let options = AffectedOptions::default();
    let expected =
        affected_details::derive(&f.engine.store, f.id(), &options, &deadline()).unwrap();
    let valid = closed_options(&options);
    let path = options_path(&f, &valid);
    assert_eq!(
        success(affected(&f).arg("--options").arg(&path).output().unwrap()),
        with_lf(expected.canonical_bytes())
    );

    let value: Value = serde_json::from_slice(&valid).unwrap();
    let mut unknown = value.clone();
    unknown["R5B_OPTIONS_DO_NOT_ECHO"] = json!(true);
    let mut missing = value.clone();
    missing.as_object_mut().unwrap().remove("maxEdgeVisits");
    let mut wrong_type = value.clone();
    wrong_type["maxStates"] = json!("R5B_OPTIONS_DO_NOT_ECHO");
    let mut wrong_policy = value.clone();
    wrong_policy["policy"] = json!("R5B_POLICY_DO_NOT_ECHO");
    let mut over_max = value.clone();
    over_max["maxHierarchyDepth"] = json!(17);
    let mut negative = value.clone();
    negative["maxIncomingDepth"] = json!(-1);
    let mut fractional = value.clone();
    fractional["maxRows"] = json!(0.5);
    let duplicate = format!(
        "{{\"policy\":\"captured_hierarchy_v1\",{}",
        &std::str::from_utf8(&valid).unwrap()[1..]
    );
    let overflow = std::str::from_utf8(&valid)
        .unwrap()
        .replace("\"maxStates\":20000", "\"maxStates\":184467440737095516160");
    assert_ne!(overflow.as_bytes(), valid);
    let mut invalid = vec![
        duplicate.into_bytes(),
        overflow.into_bytes(),
        vec![0xff],
        b"{}".to_vec(),
    ];
    invalid.extend(
        [
            unknown,
            missing,
            wrong_type,
            wrong_policy,
            over_max,
            negative,
            fractional,
        ]
        .iter()
        .map(|v| serde_json::to_vec(v).unwrap()),
    );
    for bytes in invalid {
        fs::write(&path, bytes).unwrap();
        refused(
            affected(&f).arg("--options").arg(&path).output().unwrap(),
            INPUT,
        );
        assert_eq!(state(&f.engine), before);
    }
    let mut exact = valid;
    assert!(exact.len() < MAX_OPTIONS);
    exact.resize(MAX_OPTIONS, b' ');
    assert_eq!(serde_json::from_slice::<Value>(&exact).unwrap(), value);
    fs::write(&path, &exact).unwrap();
    assert_eq!(
        success(affected(&f).arg("--options").arg(&path).output().unwrap()),
        with_lf(expected.canonical_bytes())
    );
    exact.push(b' ');
    assert_eq!(exact.len(), MAX_OPTIONS + 1);
    assert_eq!(
        serde_json::from_slice::<Value>(&exact).unwrap(),
        value,
        "One-over remains valid closed JSON"
    );
    fs::write(&path, exact).unwrap();
    refused(
        affected(&f).arg("--options").arg(&path).output().unwrap(),
        INPUT,
    );
    refused(
        affected(&f)
            .arg("--options")
            .arg(f.dir.path().join("R5B_MISSING_OPTIONS_DO_NOT_ECHO"))
            .output()
            .unwrap(),
        INPUT,
    );
    assert_eq!(state(&f.engine), before);
}

#[test]
fn affected_static_domain_zero_deadline_and_zero_stdout_capacity_do_not_initialize_a_missing_store()
{
    let dir = TempDir::new().unwrap();
    let id = "a".repeat(64);
    for flags in [
        vec!["--max-bytes", "0"],
        vec!["--timeout", "0"],
        vec!["--snapshot", "R5B_DOMAIN_DO_NOT_ECHO"],
        vec!["--capture-config", "R5B_DOMAIN_DO_NOT_ECHO"],
        vec!["--analyzer-epoch", "9"],
        vec!["--language-server-config", "R5B_DOMAIN_DO_NOT_ECHO"],
        vec!["--allow-writes"],
        vec!["--trust-servers"],
        vec!["--include-ignored-dir", "R5B_DOMAIN_DO_NOT_ECHO"],
    ] {
        // Use a fresh command so zero timeout is not a duplicate of helper flags.
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_yoke"));
        cmd.arg("--root")
            .arg(dir.path())
            .args(&flags)
            .args(["git", "affected", &id]);
        let marker = match flags[0] {
            "--max-bytes" => CAPACITY,
            "--timeout" => DEADLINE,
            _ => INPUT,
        };
        if matches!(flags[0], "--max-bytes" | "--timeout") {
            cmd.arg("--options")
                .arg(dir.path().join("R5B_MISSING_OPTIONS_DO_NOT_ECHO"));
        }
        refused(cmd.output().unwrap(), marker);
        assert!(!dir.path().join(".yoke").exists());
    }
    for flags in [
        vec!["--format", "json"],
        vec!["--format", "html"],
        vec!["--path", "R5B_DOMAIN_DO_NOT_ECHO"],
        vec!["--provider", "R5B_DOMAIN_DO_NOT_ECHO"],
    ] {
        // Root decision: syntactically unknown flags retain ordinary Clap
        // diagnostics. They must fail before run(), Store opening or output.
        let out = command(dir.path())
            .args(["git", "affected", &id])
            .args(flags)
            .output()
            .unwrap();
        assert!(!out.status.success());
        assert!(out.stdout.is_empty());
        assert!(!out.stderr.is_empty());
        assert!(!dir.path().join(".yoke").exists());
    }
    for report in ["R5B_INVALID_REPORT_DO_NOT_ECHO".to_owned(), id] {
        refused(
            command(dir.path())
                .args(["git", "affected", &report])
                .output()
                .unwrap(),
            INPUT,
        );
        assert!(!dir.path().join(".yoke").exists());
    }
}

#[test]
fn affected_zero_depths_are_preserved_while_zero_required_credits_refuse_without_partial_bytes() {
    let f = hierarchy(Edit::Derived, true);
    let before = state(&f.engine);
    let depths = AffectedOptions {
        max_hierarchy_depth: 0,
        max_incoming_depth: 0,
        ..Default::default()
    };
    let path = options_path(&f, &closed_options(&depths));
    let expected = affected_details::derive(&f.engine.store, f.id(), &depths, &deadline()).unwrap();
    assert_eq!(
        success(affected(&f).arg("--options").arg(&path).output().unwrap()),
        with_lf(expected.canonical_bytes())
    );
    for key in [
        "maxOriginalSeeds",
        "maxStates",
        "maxEdgeVisits",
        "maxRows",
        "maxOutputBytes",
    ] {
        let mut options = serde_json::to_value(AffectedOptions::default()).unwrap();
        options[key] = json!(0);
        fs::write(&path, serde_json::to_vec(&options).unwrap()).unwrap();
        refused(
            affected(&f).arg("--options").arg(&path).output().unwrap(),
            CAPACITY,
        );
        assert_eq!(state(&f.engine), before);
    }
}

#[test]
fn affected_replays_selected_history_without_git_live_source_or_latest_capture_fallback() {
    let mut f = hierarchy(Edit::Derived, true);
    let options = AffectedOptions::default();
    let expected =
        affected_details::derive(&f.engine.store, f.id(), &options, &deadline()).unwrap();
    let expected = with_lf(expected.canonical_bytes());
    assert_eq!(success(affected(&f).output().unwrap()), expected);
    fs::write(
        f.dir.path().join(PATH),
        "// R5B_LIVE_SOURCE_DO_NOT_ECHO\nfunction unrelated_latest() {}\n",
    )
    .unwrap();
    let (latest, _, _, _) = f.engine.index(&deadline()).unwrap();
    assert_ne!(latest.id, f.report["headSnapshotId"]);
    assert_eq!(
        f.engine.store.latest().unwrap().as_deref(),
        Some(latest.id.as_str())
    );
    fs::remove_file(f.dir.path().join(PATH)).unwrap();
    // Keep the genuine owned Git data outside the selected workspace. A fresh
    // empty PATH also removes the ordinary executable lookup route in the child.
    let retained_git = TempDir::new().unwrap();
    let git_directory = f.dir.path().join(".git");
    assert_eq!(git_directory.parent(), Some(f.dir.path()));
    fs::rename(
        &git_directory,
        retained_git.path().join("retained-owned-git"),
    )
    .unwrap();
    assert!(!git_directory.exists() && !f.dir.path().join(PATH).exists());
    let empty_path = TempDir::new().unwrap();
    let before = state(&f.engine);
    assert_eq!(
        success(
            affected(&f)
                .env("PATH", empty_path.path())
                .output()
                .unwrap()
        ),
        expected
    );
    assert_eq!(state(&f.engine), before);
    assert_eq!(
        f.engine.store.latest().unwrap().as_deref(),
        Some(latest.id.as_str())
    );
    assert_eq!(
        git_workflows::load(&f.engine.store, f.id(), &deadline()).unwrap(),
        f.report
    );
    assert_eq!(
        fs::read(retained_git.path().join("retained-owned-git/index")).unwrap(),
        f.index_bytes
    );
}

fn inventory(root: &Path) -> BTreeMap<String, Option<String>> {
    fn visit(root: &Path, dir: &Path, values: &mut BTreeMap<String, Option<String>>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let meta = fs::symlink_metadata(&path).unwrap();
            assert!(!yoke::paths::is_link(&meta));
            let name = path
                .strip_prefix(root)
                .unwrap()
                .to_str()
                .unwrap()
                .to_owned();
            if meta.is_dir() {
                values.insert(name, None);
                visit(root, &path, values);
            } else {
                assert!(meta.is_file());
                values.insert(name, Some(hash(fs::read(path).unwrap())));
            }
        }
    }
    let mut values = BTreeMap::new();
    visit(root, root, &mut values);
    values
}

#[test]
fn affected_routing_refuses_a_meta_only_wal_store_before_any_physical_initialization() {
    // A genuine positive comes first; the negative catches bypassing the shared
    // strict existing-state guard and entering the normal Store constructor.
    let positive = hierarchy(Edit::Derived, true);
    let expected = affected_details::derive(
        &positive.engine.store,
        positive.id(),
        &AffectedOptions::default(),
        &deadline(),
    )
    .unwrap();
    assert_eq!(
        success(affected(&positive).output().unwrap()),
        with_lf(expected.canonical_bytes())
    );
    let dir = TempDir::new().unwrap();
    let store = dir.path().join(".yoke");
    fs::create_dir(&store).unwrap();
    for name in ["blobs", "trees"] {
        fs::create_dir(store.join(name)).unwrap();
    }
    for name in ["clients.lock", "maintenance.lock"] {
        fs::write(store.join(name), []).unwrap();
    }
    let db = rusqlite::Connection::open(store.join("index.sqlite")).unwrap();
    db.execute_batch(
        "CREATE TABLE meta(key TEXT PRIMARY KEY,value TEXT NOT NULL); PRAGMA journal_mode=WAL;",
    )
    .unwrap();
    db.execute("INSERT INTO meta VALUES('workspace',?1)", ["a".repeat(64)])
        .unwrap();
    drop(db);
    assert!(!store.join("index.sqlite-wal").exists() && !store.join("index.sqlite-shm").exists());
    let before = inventory(dir.path());
    refused(
        command(dir.path())
            .args(["git", "affected", positive.id()])
            .output()
            .unwrap(),
        INPUT,
    );
    assert_eq!(
        inventory(dir.path()),
        before,
        "No schema/locks/directories/sidecars created before refusal"
    );
}

#[cfg(windows)]
#[test]
fn affected_options_leaf_and_ancestor_reparse_points_refuse_after_real_options_success() {
    let f = hierarchy(Edit::Derived, true);
    let before = state(&f.engine);
    let options = AffectedOptions {
        max_hierarchy_depth: 1,
        max_incoming_depth: 1,
        ..Default::default()
    };
    let expected =
        affected_details::derive(&f.engine.store, f.id(), &options, &deadline()).unwrap();
    let host = TempDir::new().unwrap();
    let real = host.path().join("real-options");
    fs::create_dir(&real).unwrap();
    let source = real.join("options.json");
    let bytes = closed_options(&options);
    fs::write(&source, &bytes).unwrap();
    assert_eq!(
        success(affected(&f).arg("--options").arg(&source).output().unwrap()),
        with_lf(expected.canonical_bytes())
    );
    let leaf = host.path().join("linked-options.json");
    let ancestor = host.path().join("linked-options-directory");
    std::os::windows::fs::symlink_file(&source, &leaf)
        .expect("Real Windows file reparse fixture prerequisite");
    std::os::windows::fs::symlink_dir(&real, &ancestor)
        .expect("Real Windows directory reparse fixture prerequisite");
    assert!(yoke::paths::is_link(&fs::symlink_metadata(&leaf).unwrap()));
    assert!(yoke::paths::is_link(
        &fs::symlink_metadata(&ancestor).unwrap()
    ));
    for path in [&leaf, &ancestor.join("options.json")] {
        assert_eq!(
            fs::read(path).unwrap(),
            bytes,
            "Alias must resolve genuine valid options without the guard"
        );
        refused(
            affected(&f).arg("--options").arg(path).output().unwrap(),
            INPUT,
        );
        assert_eq!(fs::read(&source).unwrap(), bytes);
        assert_eq!(state(&f.engine), before);
    }
}
