//! Exact saved-source owner interpretation over genuine owned Git publications.
use yoke as test_yoke;
#[path = "support/git_impact_fixture.rs"]
mod fixture;
use yoke::{
    capture::Deadline,
    git_workflows::{areas, owners},
};

#[test]
fn explicit_empty_rule_has_its_own_exact_quote_and_preserves_legacy_parent() {
    let rules = "# β 💡\r\n* @global-owner\r\n/src/a.rs\r\n";
    let f = fixture::Fixture::new(
        &[
            ("src/a.rs", "pub fn a() -> u32 { 1 }\n"),
            ("CODEOWNERS", rules),
        ],
        &[
            ("src/a.rs", "pub fn a() -> u32 { 2 }\n"),
            ("CODEOWNERS", rules),
        ],
    );
    assert!(
        f.report["ownerSuggestions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["owner"] == "@global-owner")
    );
    let before = fixture::state(&f.engine);
    let parent = areas::derive(
        &f.engine.store,
        f.id(),
        &[],
        &Default::default(),
        &Deadline::new(30_000),
    )
    .unwrap();
    let value = owners::derive(&f.engine.store, f.id(), &Deadline::new(30_000)).unwrap();
    assert_eq!(value["impactAreas"], *parent.value());
    for row in value["ownership"].as_array().unwrap() {
        assert_eq!(row["status"], "explicit_empty_owner_list");
        assert_eq!(row["ownerLabels"], serde_json::json!([]));
        assert_eq!(row["winningRule"]["quote"], "/src/a.rs\r\n");
        let start = row["winningRule"]["startByte"].as_u64().unwrap() as usize;
        let end = row["winningRule"]["endByte"].as_u64().unwrap() as usize;
        assert_eq!(&rules.as_bytes()[start..end], b"/src/a.rs\r\n");
    }
    assert_eq!(fixture::state(&f.engine), before);
    assert_eq!(value["complete"], false);
}

fn d() -> Deadline {
    Deadline::new(30_000)
}
fn one(rules: &str) -> fixture::Fixture {
    fixture::Fixture::new(
        &[
            ("src/a.rs", "pub fn a() -> u32 { 1 }\n"),
            ("CODEOWNERS", rules),
        ],
        &[
            ("src/a.rs", "pub fn a() -> u32 { 2 }\n"),
            ("CODEOWNERS", rules),
        ],
    )
}
fn derive(f: &fixture::Fixture) -> serde_json::Value {
    owners::derive(&f.engine.store, f.id(), &d()).unwrap()
}

#[test]
fn same_label_from_distinct_rules_keeps_each_path_and_original_token_range() {
    let rules = "# café 💡\r\n/src/a.rs @same-owner\r\n/src/b.rs @same-owner @org/team\r\n";
    let f = fixture::Fixture::new(
        &[
            ("src/a.rs", "pub fn a() { let n = 1; }\n"),
            ("src/b.rs", "pub fn b() { let n = 1; }\n"),
            ("CODEOWNERS", rules),
        ],
        &[
            ("src/a.rs", "pub fn a() { let n = 2; }\n"),
            ("src/b.rs", "pub fn b() { let n = 2; }\n"),
            ("CODEOWNERS", rules),
        ],
    );
    let value = derive(&f);
    assert_eq!(value["ownership"].as_array().unwrap().len(), 4);
    for row in value["ownership"].as_array().unwrap() {
        let rule = &row["winningRule"];
        let expected = if row["path"] == "src/a.rs" { 2 } else { 3 };
        assert_eq!(rule["line"], expected);
        let begin = rule["startByte"].as_u64().unwrap() as usize;
        let end = rule["endByte"].as_u64().unwrap() as usize;
        assert_eq!(
            &rules.as_bytes()[begin..end],
            rule["quote"].as_str().unwrap().as_bytes()
        );
        for token in rule["ownerTokens"].as_array().unwrap() {
            let b = token["startByte"].as_u64().unwrap() as usize;
            let e = token["endByte"].as_u64().unwrap() as usize;
            assert_eq!(&rules[b..e], token["label"].as_str().unwrap());
        }
    }
    assert_eq!(value["ownerIdentityVerified"], false);
    assert_eq!(value["githubPermissionVerified"], false);
}

#[test]
fn base_and_head_rules_stay_qualified_and_live_rule_changes_cannot_override_them() {
    let base = "* @base-owner\r\n";
    let head = "* @head-owner\r\n";
    let f = fixture::Fixture::new(
        &[
            ("src/a.rs", "pub fn a() { let n = 1; }\n"),
            ("CODEOWNERS", base),
        ],
        &[
            ("src/a.rs", "pub fn a() { let n = 2; }\n"),
            ("CODEOWNERS", head),
        ],
    );
    let first = derive(&f);
    std::fs::write(f.dir.path().join("CODEOWNERS"), "* @live-authority\n").unwrap();
    std::fs::write(f.dir.path().join("src/a.rs"), "not the captured source").unwrap();
    let before = fixture::state(&f.engine);
    let after = derive(&f);
    assert_eq!(first, after);
    assert_eq!(fixture::state(&f.engine), before);
    for row in after["ownership"].as_array().unwrap() {
        let expected = if row["side"] == "base" {
            "@base-owner"
        } else {
            "@head-owner"
        };
        assert_eq!(row["ownerLabels"], serde_json::json!([expected]));
    }
    assert_eq!(
        std::fs::read_to_string(f.dir.path().join("CODEOWNERS")).unwrap(),
        "* @live-authority\n"
    );
}

#[test]
fn unsupported_active_rules_remain_indeterminate_with_all_supported_matches_visible() {
    for unsupported in [
        "!src/a.rs @maybe",
        "[a-z].rs @maybe",
        "path\\ with @maybe",
        "src/a.rs not-an-owner",
    ] {
        let f = one(&format!("* @known\n{unsupported}\n/src/a.rs @later\n"));
        let value = derive(&f);
        for row in value["ownership"].as_array().unwrap() {
            assert_eq!(row["status"], "unsupported_rules_present");
            assert!(row["winningRule"].is_null());
            assert!(row["ownerLabels"].is_null());
            assert_eq!(
                row["matchingSupportedRuleOrdinals"],
                serde_json::json!([0, 2])
            );
        }
        assert_eq!(value["ruleSources"][0]["rules"][1]["supported"], false);
    }
}

#[test]
fn first_captured_path_precedence_and_uncaptured_higher_rules_remain_visible() {
    let f = fixture::Fixture::new(
        &[
            ("src/a.rs", "pub fn a() { let n = 1; }\n"),
            (".github/CODEOWNERS", "* @github\n"),
            ("CODEOWNERS", "* @root\n"),
            ("docs/CODEOWNERS", "* @docs\n"),
        ],
        &[
            ("src/a.rs", "pub fn a() { let n = 2; }\n"),
            (".github/CODEOWNERS", "* @github\n"),
            ("CODEOWNERS", "* @root\n"),
            ("docs/CODEOWNERS", "* @docs\n"),
        ],
    );
    let value = derive(&f);
    assert_eq!(value["ruleSources"][0]["sourcePath"], ".github/CODEOWNERS");
    for row in value["ownership"].as_array().unwrap() {
        assert_eq!(row["ownerLabels"], serde_json::json!(["@github"]));
        assert_eq!(row["uncapturedPrecedenceMayDiffer"], false);
    }
    let root = one("* @root\n");
    let value = derive(&root);
    assert_eq!(
        value["ruleSources"][0]["uncapturedHigherPriorityPaths"],
        serde_json::json!([".github/CODEOWNERS"])
    );
    for row in value["ownership"].as_array().unwrap() {
        assert_eq!(row["uncapturedPrecedenceMayDiffer"], true);
    }
    let absent = fixture::Fixture::new(
        &[("src/a.rs", "pub fn a() { let n = 1; }\n")],
        &[("src/a.rs", "pub fn a() { let n = 2; }\n")],
    );
    for row in derive(&absent)["ownership"].as_array().unwrap() {
        assert_eq!(row["status"], "no_captured_rules");
        assert!(row["ownerLabels"].is_null());
    }
}

#[test]
fn independent_rule_budgets_are_exact_and_zero_never_means_default() {
    let f = one("* @global\r\n/src/a.rs\r\n");
    let before = fixture::state(&f.engine);
    let value = derive(&f);
    let counts = &value["consumed"];
    let mut exact = owners::Options::default();
    exact.limits.max_rule_bytes = counts["ruleBytes"].as_u64().unwrap() as usize;
    exact.limits.max_rules = counts["rules"].as_u64().unwrap() as usize;
    exact.limits.max_owner_tokens = counts["ownerTokens"].as_u64().unwrap() as usize;
    exact.limits.max_matches = counts["matches"].as_u64().unwrap() as usize;
    exact.limits.max_match_work = counts["matchWork"].as_u64().unwrap() as usize;
    assert!(owners::derive_with_options(&f.engine.store, f.id(), &exact, &d()).is_ok());
    for field in [
        "maxRuleBytes",
        "maxRules",
        "maxOwnerTokens",
        "maxMatches",
        "maxMatchWork",
    ] {
        for zero in [false, true] {
            let mut v = serde_json::to_value(&exact).unwrap();
            let n = v["limits"][field].as_u64().unwrap();
            v["limits"][field] = serde_json::json!(if zero { 0 } else { n - 1 });
            let options = serde_json::from_value(v).unwrap();
            let error =
                owners::derive_with_options(&f.engine.store, f.id(), &options, &d()).unwrap_err();
            assert_eq!(error.to_string(), "git_owners_capacity_exceeded", "{field}");
        }
    }
    for _ in 0..3 {
        let value = owners::derive_with_options(&f.engine.store, f.id(), &exact, &d()).unwrap();
        exact.limits.max_output_bytes = serde_json::to_vec(&value).unwrap().len();
    }
    let result = owners::derive_with_options(&f.engine.store, f.id(), &exact, &d()).unwrap();
    assert_eq!(
        serde_json::to_vec(&result).unwrap().len(),
        exact.limits.max_output_bytes
    );
    exact.limits.max_output_bytes -= 1;
    assert_eq!(
        owners::derive_with_options(&f.engine.store, f.id(), &exact, &d())
            .unwrap_err()
            .to_string(),
        "git_owners_capacity_exceeded"
    );
    assert_eq!(fixture::state(&f.engine), before);
}

#[test]
fn caller_transaction_sql_length_cancellation_and_deadline_are_preserved() {
    use rusqlite::limits::Limit;
    let f = one("* @owner\n");
    let before = fixture::state(&f.engine);
    f.engine.store.db.execute_batch("BEGIN DEFERRED").unwrap();
    assert!(owners::derive(&f.engine.store, f.id(), &d()).is_err());
    assert!(!f.engine.store.db.is_autocommit());
    f.engine.store.db.execute_batch("ROLLBACK").unwrap();
    let prior = f
        .engine
        .store
        .db
        .set_limit(Limit::SQLITE_LIMIT_LENGTH, 32)
        .unwrap();
    assert!(owners::derive(&f.engine.store, f.id(), &d()).is_err());
    assert_eq!(
        f.engine.store.db.limit(Limit::SQLITE_LIMIT_LENGTH).unwrap(),
        32
    );
    f.engine
        .store
        .db
        .set_limit(Limit::SQLITE_LIMIT_LENGTH, prior)
        .unwrap();
    assert_eq!(
        owners::derive(&f.engine.store, f.id(), &Deadline::new(0))
            .unwrap_err()
            .to_string(),
        "git_owners_deadline_exceeded"
    );
    let flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let cancel = Deadline::run_bounded(&d(), flag.clone(), || Ok(d())).unwrap();
    flag.store(true, std::sync::atomic::Ordering::SeqCst);
    assert_eq!(
        owners::derive(&f.engine.store, f.id(), &cancel)
            .unwrap_err()
            .to_string(),
        "git_owners_deadline_exceeded"
    );
    let mut options = owners::Options::default();
    options.limits.max_matches = 0;
    assert!(owners::derive_with_options(&f.engine.store, f.id(), &options, &d()).is_err());
    assert!(f.engine.store.db.is_autocommit());
    assert_eq!(
        f.engine.store.db.limit(Limit::SQLITE_LIMIT_LENGTH).unwrap(),
        prior
    );
    assert_eq!(fixture::state(&f.engine), before);
}

#[test]
fn tampered_rule_cas_refuses_without_empty_or_live_fallback() {
    let rules = "* @captured\n";
    let f = one(rules);
    let blob = f
        .engine
        .store
        .dir
        .join("blobs")
        .join(yoke::model::hash(rules));
    assert!(blob.is_file());
    std::fs::write(&blob, "* @tampered\n").unwrap();
    let before = fixture::state(&f.engine);
    assert_eq!(
        owners::derive(&f.engine.store, f.id(), &d())
            .unwrap_err()
            .to_string(),
        "git_owners_source_refused"
    );
    assert_eq!(fixture::state(&f.engine), before);
}

#[test]
fn cli_is_offline_readonly_and_uses_exact_newline_inclusive_output_budget() {
    let f = one("* @owner\n");
    let empty = tempfile::tempdir().unwrap();
    let before = fixture::state(&f.engine);
    let run = |flags: &[&str], budget: usize| {
        std::process::Command::new(env!("CARGO_BIN_EXE_yoke"))
            .env("PATH", empty.path())
            .arg("--root")
            .arg(f.dir.path())
            .args(["--max-bytes", &budget.to_string()])
            .args(flags)
            .args(["git", "owners", f.id()])
            .output()
            .unwrap()
    };
    let out = run(&[], 4_000_000);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value, derive(&f));
    for field in ["gitSubprocesses", "modelCalls", "networkRequests"] {
        assert_eq!(value[field], 0);
    }
    assert!(run(&[], out.stdout.len()).status.success());
    let small = run(&[], out.stdout.len() - 1);
    assert!(!small.status.success());
    assert!(small.stdout.is_empty());
    for flags in [
        vec!["--allow-writes"],
        vec!["--snapshot", "bad"],
        vec!["--capture-config", "missing"],
        vec!["--language-server-config", "missing"],
        vec!["--trust-servers"],
        vec!["--analyzer-epoch", "9"],
    ] {
        let out = run(&flags, 4_000_000);
        assert!(!out.status.success());
        assert!(out.stdout.is_empty());
    }
    assert_eq!(fixture::state(&f.engine), before);
}

#[test]
fn invalid_options_unknown_protocol_fields_and_missing_store_cannot_initialize_state() {
    for raw in [
        b"{}".as_slice(),
        b"{\"limits\":{},\"limits\":{}}",
        b"{\"areas\":{},\"allowWrites\":true}",
        b"\xff",
    ] {
        assert!(owners::Options::parse(raw).is_err());
    }
    let f = one("* @owner\n");
    let missing = tempfile::tempdir().unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(missing.path())
        .args(["git", "owners", f.id()])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    assert!(!missing.path().join(".yoke").exists());
    let mut options = owners::Options::default();
    options.limits.max_rules = 4097;
    assert!(owners::derive_with_options(&f.engine.store, f.id(), &options, &d()).is_err());
}
