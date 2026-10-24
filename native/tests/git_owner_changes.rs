//! Owner-rule changes include unchanged captured files; no identity/review claim.
use yoke as test_yoke;
#[path = "support/git_impact_fixture.rs"]
mod fixture;
use serde_json::{Value, json};
use yoke::{
    capture::Deadline,
    git_workflows::{
        areas,
        owners::{self, changes},
    },
};
fn d() -> Deadline {
    Deadline::new(30_000)
}
fn changed() -> fixture::Fixture {
    fixture::Fixture::new(
        &[
            ("src/a.rs", "// owned Î» ðŸ’¡\r\npub fn a()->u32{1}\r\n"),
            ("CODEOWNERS", "* @base-owner\r\n"),
        ],
        &[
            ("src/a.rs", "// owned Î» ðŸ’¡\r\npub fn a()->u32{1}\r\n"),
            ("CODEOWNERS", "* @head-owner\r\n"),
        ],
    )
}
fn row<'a>(v: &'a Value, path: &str) -> &'a Value {
    v["changes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["path"] == path)
        .unwrap()
}
#[test]
fn rule_only_changes_cover_unchanged_unicode_sources_and_preserve_original_parent_and_owners() {
    let f = changed();
    let before = fixture::state(&f.engine);
    let old = owners::derive(&f.engine.store, f.id(), &d()).unwrap();
    assert!(
        old["ownership"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["path"] != "src/a.rs")
    );
    let parent = areas::derive(&f.engine.store, f.id(), &[], &Default::default(), &d()).unwrap();
    let report = changes::derive(&f.engine.store, f.id(), &d()).unwrap();
    let v = report.value();
    assert_eq!(v["impactAreas"], *parent.value());
    assert_eq!(v["parentResultId"], parent.value()["resultId"]);
    let a = row(v, "src/a.rs");
    assert_eq!(a["change"], "authored_candidates_changed");
    assert_eq!(a["sourceContentChanged"], false);
    assert_eq!(a["base"]["ownerLabels"], json!(["@base-owner"]));
    assert_eq!(a["head"]["ownerLabels"], json!(["@head-owner"]));
    assert_eq!(a["base"]["contentHash"], a["head"]["contentHash"]);
    assert_eq!(v["allCapturedPathsEvaluated"], true);
    assert_eq!(v["capturedSourceBodiesVerified"], true);
    for key in [
        "complete",
        "ownerIdentityVerified",
        "githubPermissionVerified",
        "reviewRequirementEstablished",
        "stateWritten",
        "sourceWritten",
        "latestChanged",
    ] {
        assert_eq!(v[key], false, "{key}")
    }
    for key in ["modelCalls", "networkRequests", "gitSubprocesses"] {
        assert_eq!(v[key], 0, "{key}")
    }
    let mut without_id = v.clone();
    let id = without_id
        .as_object_mut()
        .unwrap()
        .remove("resultId")
        .unwrap();
    assert_eq!(id, yoke::model::identity(&without_id));
    assert_eq!(serde_json::to_vec(v).unwrap(), report.canonical_bytes());
    assert_eq!(owners::derive(&f.engine.store, f.id(), &d()).unwrap(), old);
    let repeat = changes::derive(&f.engine.store, f.id(), &d()).unwrap();
    assert_eq!(repeat.canonical_bytes(), report.canonical_bytes());
    assert_eq!(fixture::state(&f.engine), before);
}
#[test]
fn original_winning_line_and_token_bytes_survive_relocation_empty_lists_and_duplicate_labels() {
    let f = fixture::Fixture::new(
        &[
            ("src/Î».rs", "pub fn a(){}\n"),
            ("CODEOWNERS", "# cafÃ© ðŸ’¡\r\n/src/Î».rs @same @same\r\n"),
        ],
        &[
            ("src/Î».rs", "pub fn a(){}\n"),
            (
                "CODEOWNERS",
                "# prefix ðŸ’¡\r\n# cafÃ© ðŸ’¡\r\n/src/Î».rs @same @same\r\n",
            ),
        ],
    );
    let report = changes::derive(&f.engine.store, f.id(), &d()).unwrap();
    let a = row(report.value(), "src/Î».rs");
    assert_eq!(a["change"], "winning_rule_evidence_changed");
    assert_eq!(a["sourceContentChanged"], false);
    for side in ["base", "head"] {
        let r = &a[side]["winningRule"];
        let text = f.text(side, "CODEOWNERS");
        let start = r["startByte"].as_u64().unwrap() as usize;
        let end = r["endByte"].as_u64().unwrap() as usize;
        assert_eq!(
            &text.as_bytes()[start..end],
            r["quote"].as_str().unwrap().as_bytes()
        );
        assert_eq!(a[side]["ownerLabels"], json!(["@same", "@same"]));
        for token in r["ownerTokens"].as_array().unwrap() {
            let b = token["startByte"].as_u64().unwrap() as usize;
            let e = token["endByte"].as_u64().unwrap() as usize;
            assert_eq!(&text[b..e], token["label"].as_str().unwrap());
        }
    }
    let empty = fixture::Fixture::new(
        &[("src/a.rs", "pub fn a(){}\n"), ("CODEOWNERS", "* @same\n")],
        &[
            ("src/a.rs", "pub fn a(){}\n"),
            ("CODEOWNERS", "* @same\n/src/a.rs\n"),
        ],
    );
    let report = changes::derive(&empty.engine.store, empty.id(), &d()).unwrap();
    let a = row(report.value(), "src/a.rs");
    assert_eq!(a["change"], "authored_candidates_changed");
    assert_eq!(a["head"]["status"], "explicit_empty_owner_list");
    assert_eq!(a["head"]["ownerLabels"], json!([]));
}
#[test]
fn unrelated_container_hash_churn_does_not_change_a_stable_winner_and_unchanged_rows_are_explicit()
{
    let f = fixture::Fixture::new(
        &[("src/a.rs", "pub fn a(){}\n"), ("CODEOWNERS", "* @same\n")],
        &[
            ("src/a.rs", "pub fn a(){}\n"),
            ("CODEOWNERS", "* @same\n/docs/ @docs\n"),
        ],
    );
    let report = changes::derive(&f.engine.store, f.id(), &d()).unwrap();
    assert!(report.value()["changes"].as_array().unwrap().is_empty());
    assert_ne!(
        report.value()["ruleSources"][0]["contentHash"],
        report.value()["ruleSources"][1]["contentHash"]
    );
    let options = changes::Options {
        include_unchanged: true,
        ..Default::default()
    };
    let all = changes::derive_with_options(&f.engine.store, f.id(), &options, &d()).unwrap();
    let a = row(all.value(), "src/a.rs");
    assert_eq!(a["change"], "unchanged_selected_assignment");
    assert_eq!(a["sourceContentChanged"], false);
    let added = fixture::Fixture::new(
        &[
            ("src/old.rs", "pub fn old(){}\n"),
            ("CODEOWNERS", "* @same\n"),
        ],
        &[
            ("src/new.rs", "pub fn new(){}\n"),
            ("CODEOWNERS", "* @same\n"),
        ],
    );
    let report = changes::derive(&added.engine.store, added.id(), &d()).unwrap();
    assert_eq!(
        row(report.value(), "src/new.rs")["change"],
        "captured_path_added"
    );
    assert_eq!(
        row(report.value(), "src/old.rs")["change"],
        "captured_path_removed"
    );
}
#[test]
fn unsupported_and_missing_rules_remain_indeterminate_and_do_not_hide_supported_candidates() {
    let f = fixture::Fixture::new(
        &[("src/a.rs", "pub fn a(){}\n"), ("CODEOWNERS", "* @same\n")],
        &[
            ("src/a.rs", "pub fn a(){}\n"),
            ("CODEOWNERS", "!private/** @unsupported\n* @candidate\n"),
        ],
    );
    let report = changes::derive(&f.engine.store, f.id(), &d()).unwrap();
    let a = row(report.value(), "src/a.rs");
    assert_eq!(a["change"], "indeterminate_selected_rules");
    assert_eq!(a["indeterminate"], true);
    assert_eq!(a["head"]["ownerLabels"], Value::Null);
    assert_eq!(a["head"]["matchingSupportedRuleOrdinals"], json!([1]));
    let absent = fixture::Fixture::new(
        &[("src/a.rs", "pub fn a(){}\n")],
        &[("src/a.rs", "pub fn a(){let n=1;}\n")],
    );
    let report = changes::derive(&absent.engine.store, absent.id(), &d()).unwrap();
    let a = row(report.value(), "src/a.rs");
    assert_eq!(a["change"], "indeterminate_selected_rules");
    assert_eq!(a["base"]["status"], "no_captured_rules");
    assert_eq!(a["base"]["ownerLabels"], Value::Null);
}
#[test]
fn source_tampering_whole_budgets_deadlines_and_caller_transactions_refuse_without_writes() {
    let f = changed();
    let before = fixture::state(&f.engine);
    let good = changes::derive(&f.engine.store, f.id(), &d()).unwrap();
    let raw = serde_json::to_value(changes::Options::default()).unwrap();
    for field in [
        "maxPaths",
        "maxManifestEntries",
        "maxRows",
        "maxFileBytes",
        "maxSourceBytes",
        "maxRuleBytes",
        "maxRules",
        "maxOwnerTokens",
        "maxMatches",
        "maxMatchWork",
        "maxOutputBytes",
    ] {
        let mut value = raw.clone();
        value["limits"][field] = json!(0);
        let options: changes::Options = serde_json::from_value(value).unwrap();
        assert_eq!(
            changes::derive_with_options(&f.engine.store, f.id(), &options, &d())
                .unwrap_err()
                .to_string(),
            "git_owner_changes_capacity_exceeded",
            "{field}"
        );
        assert_eq!(fixture::state(&f.engine), before);
    }
    let mut tight = changes::Options::default();
    tight.limits.max_output_bytes = 1;
    assert!(changes::derive_with_options(&f.engine.store, f.id(), &tight, &d()).is_err());
    assert_eq!(
        changes::derive(&f.engine.store, f.id(), &Deadline::new(0))
            .unwrap_err()
            .to_string(),
        "git_owner_changes_deadline_exceeded"
    );
    f.engine.store.set_deadline(&d()).unwrap();
    f.engine.store.db.execute_batch("BEGIN").unwrap();
    assert_eq!(
        changes::derive(&f.engine.store, f.id(), &d())
            .unwrap_err()
            .to_string(),
        "git_owner_changes_input_invalid"
    );
    assert!(!f.engine.store.db.is_autocommit());
    f.engine.store.db.execute_batch("ROLLBACK").unwrap();
    assert_eq!(fixture::state(&f.engine), before);
    let report = changes::derive(&f.engine.store, f.id(), &d()).unwrap();
    let hash = row(report.value(), "src/a.rs")["base"]["contentHash"]
        .as_str()
        .unwrap();
    let blob = f.engine.store.dir.join("blobs").join(hash);
    let original = std::fs::read(&blob).unwrap();
    std::fs::write(&blob, b"tampered unchanged source").unwrap();
    assert_eq!(
        changes::derive(&f.engine.store, f.id(), &d())
            .unwrap_err()
            .to_string(),
        "git_owner_changes_source_refused"
    );
    std::fs::write(blob, original).unwrap();
    assert_eq!(
        changes::derive(&f.engine.store, f.id(), &d())
            .unwrap()
            .canonical_bytes(),
        good.canonical_bytes()
    );
    assert_eq!(fixture::state(&f.engine), before);
}
#[test]
fn actual_cli_is_offline_readonly_newline_budgeted_and_host_options_fail_before_store_open() {
    let f = changed();
    let empty = tempfile::tempdir().unwrap();
    let before = fixture::state(&f.engine);
    let run = |flags: &[&str], budget: usize| {
        std::process::Command::new(env!("CARGO_BIN_EXE_yoke"))
            .env("PATH", empty.path())
            .arg("--root")
            .arg(f.dir.path())
            .args(["--max-bytes", &budget.to_string()])
            .args(flags)
            .args(["git", "owner-changes", f.id()])
            .output()
            .unwrap()
    };
    let out = run(&[], 8_000_000);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        v,
        *changes::derive(&f.engine.store, f.id(), &d())
            .unwrap()
            .value()
    );
    assert_eq!(row(&v, "src/a.rs")["sourceContentChanged"], false);
    assert!(run(&[], out.stdout.len()).status.success());
    let small = run(&[], out.stdout.len() - 1);
    assert!(!small.status.success());
    assert!(small.stdout.is_empty());
    for flag in ["--allow-writes", "--trust-servers"] {
        let bad = run(&[flag], 8_000_000);
        assert!(!bad.status.success());
        assert!(bad.stdout.is_empty())
    }
    assert_eq!(fixture::state(&f.engine), before);
    let missing = empty.path().join("missing");
    let malformed = empty.path().join("private.json");
    std::fs::write(&malformed, b"{not JSON private config").unwrap();
    let bad = std::process::Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(&missing)
        .args(["git", "owner-changes", f.id(), "--options"])
        .arg(&malformed)
        .output()
        .unwrap();
    assert!(!bad.status.success());
    assert!(bad.stdout.is_empty());
    assert_eq!(
        String::from_utf8(bad.stderr).unwrap().trim(),
        "yoke: git_owner_changes_input_invalid"
    );
    assert!(!missing.exists());
    for bytes in [
        br#"{"areas":{},"limits":{},"allowWrites":true}"#.as_slice(),
        br#"{"areas":{},"areas":{},"limits":{}}"#.as_slice(),
        br#"{}"#.as_slice(),
    ] {
        assert!(changes::Options::parse(bytes).is_err())
    }
}
