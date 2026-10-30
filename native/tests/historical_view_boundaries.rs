//! Historical comparison continuity, capture-policy and epoch boundary tests.
//! Declarations bind exact primary files; capture controls use published policies.
//! Owned local fixture workers are explicit setup for checked archive inputs.
#[path = "support/comparison_error_redaction.rs"]
mod error_oracle;
#[path = "support/historical_fixture_state.rs"]
mod oracle;
#[path = "support/meaning_comparison_fixture.rs"]
mod support;

use std::{collections::BTreeMap, fs};
use yoke::{
    Engine,
    analysis::AnalyzerEpoch,
    capture::{self, CaptureOptions},
    graph_view::{self, BuildOptions, View},
    graph_view_archive::{self as archive, CheckedMeaningArchive, Limits},
    model::Snapshot,
    view_comparison::{self, ComparisonOptions, DeclaredFileContinuity, SelectionPin},
};

fn declared(before: &Snapshot, from: &str, after: &Snapshot, to: &str) -> DeclaredFileContinuity {
    let from = before.files.iter().find(|file| file.path == from).unwrap();
    let to = after.files.iter().find(|file| file.path == to).unwrap();
    DeclaredFileContinuity {
        before_path: from.path.clone(),
        before_hash: from.hash.clone(),
        after_path: to.path.clone(),
        after_hash: to.hash.clone(),
    }
}

fn self_archived(
    checked: &CheckedMeaningArchive,
    declarations: Vec<DeclaredFileContinuity>,
) -> anyhow::Result<view_comparison::Comparison> {
    error_oracle::checked(view_comparison::compare_meaning_archives(
        checked,
        checked,
        &ComparisonOptions {
            declared_file_continuity: declarations,
            ..Default::default()
        },
        &support::d(),
    ))
}

#[test]
fn h06_genuine_primary_controls_precede_malformed_and_nonbijective_declarations() {
    let f = support::fixture(AnalyzerEpoch::V9);
    let prepared = archive::prepare_published(
        &f.engine.store,
        &f.view.id,
        Limits::default(),
        &support::d(),
    )
    .unwrap();
    let checked = archive::parse(prepared.bytes(), Limits::default(), &support::d()).unwrap();
    let baseline = oracle::state(&f.engine);
    let calls = fs::read(&f.marker).unwrap();
    let one = self_archived(
        &checked,
        vec![declared(
            checked.snapshot(),
            "a.rs",
            checked.snapshot(),
            "a.rs",
        )],
    )
    .unwrap();
    assert_eq!(
        one.value()["declaredFileContinuity"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let two = self_archived(
        &checked,
        vec![
            declared(checked.snapshot(), "a.rs", checked.snapshot(), "a.rs"),
            declared(checked.snapshot(), "b.rs", checked.snapshot(), "b.rs"),
        ],
    )
    .unwrap();
    assert_eq!(
        two.value()["declaredFileContinuity"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    for malformed in [
        "", "../a.rs", "/a.rs", "C:/a.rs", "a\\rs", "a/./b.rs", "a//b.rs", "a.rs/",
    ] {
        let mut entry = declared(checked.snapshot(), "a.rs", checked.snapshot(), "a.rs");
        entry.before_path = malformed.into();
        assert!(
            self_archived(&checked, vec![entry]).is_err(),
            "Malformed before path was accepted"
        );
        let mut entry = declared(checked.snapshot(), "a.rs", checked.snapshot(), "a.rs");
        entry.after_path = malformed.into();
        assert!(
            self_archived(&checked, vec![entry]).is_err(),
            "Malformed after path was accepted"
        );
    }
    let uppercase = "F".repeat(64);
    for malformed in ["", "not-a-sha256", uppercase.as_str()] {
        let mut entry = declared(checked.snapshot(), "a.rs", checked.snapshot(), "a.rs");
        entry.before_hash = malformed.into();
        assert!(
            self_archived(&checked, vec![entry]).is_err(),
            "Malformed before hash was accepted"
        );
        let mut entry = declared(checked.snapshot(), "a.rs", checked.snapshot(), "a.rs");
        entry.after_hash = malformed.into();
        assert!(
            self_archived(&checked, vec![entry]).is_err(),
            "Malformed after hash was accepted"
        );
    }
    for (from, to) in [
        (["a.rs", "a.rs"], ["a.rs", "a.rs"]),
        (["a.rs", "a.rs"], ["a.rs", "b.rs"]),
        (["a.rs", "b.rs"], ["a.rs", "a.rs"]),
    ] {
        let entries = (0..2)
            .map(|index| {
                declared(
                    checked.snapshot(),
                    from[index],
                    checked.snapshot(),
                    to[index],
                )
            })
            .collect();
        assert!(
            self_archived(&checked, entries).is_err(),
            "Duplicate or same-side reuse was accepted"
        );
    }
    assert_eq!(oracle::state(&f.engine), baseline);
    assert_eq!(fs::read(&f.marker).unwrap(), calls);
}

#[test]
fn h06_historical_union_membership_does_not_satisfy_current_primary_binding() {
    let f = support::fixture(AnalyzerEpoch::V9);
    let prepared = archive::prepare_published(
        &f.engine.store,
        &f.view.id,
        Limits::default(),
        &support::d(),
    )
    .unwrap();
    let checked = archive::parse(prepared.bytes(), Limits::default(), &support::d()).unwrap();
    let wire: serde_json::Value = serde_json::from_slice(checked.bytes()).unwrap();
    let old_hash = yoke::model::hash(support::HISTORY_OLD);
    let new_hash = yoke::model::hash(support::HISTORY_NEW);
    assert_eq!(wire["sourceObjects"][&old_hash], support::HISTORY_OLD);
    assert_eq!(wire["sourceObjects"][&new_hash], support::HISTORY_NEW);
    let current = checked
        .snapshot()
        .files
        .iter()
        .find(|file| file.path == "history.rs")
        .unwrap();
    assert_eq!(current.hash, new_hash);
    assert_ne!(old_hash, new_hash);
    let baseline = oracle::state(&f.engine);
    self_archived(
        &checked,
        vec![declared(
            checked.snapshot(),
            "history.rs",
            checked.snapshot(),
            "history.rs",
        )],
    )
    .unwrap();
    let mut wrong_before = declared(
        checked.snapshot(),
        "history.rs",
        checked.snapshot(),
        "history.rs",
    );
    wrong_before.before_hash = old_hash.clone();
    let error = match self_archived(&checked, vec![wrong_before]) {
        Ok(value) => {
            drop(value);
            panic!("Historical before hash was accepted as PRIMARY")
        }
        Err(error) => error,
    };
    assert!(format!("{error:#}").contains("comparison_primary_file_binding_mismatch"));
    let mut wrong_after = declared(
        checked.snapshot(),
        "history.rs",
        checked.snapshot(),
        "history.rs",
    );
    wrong_after.after_hash = old_hash;
    let error = match self_archived(&checked, vec![wrong_after]) {
        Ok(value) => {
            drop(value);
            panic!("Historical after hash was accepted as PRIMARY")
        }
        Err(error) => error,
    };
    assert!(format!("{error:#}").contains("comparison_primary_file_binding_mismatch"));
    assert_eq!(oracle::state(&f.engine), baseline);
}

fn captured_view(
    engine: &mut Engine,
    options: CaptureOptions,
    epoch: AnalyzerEpoch,
) -> (Snapshot, View) {
    let expected = options.validated().unwrap();
    engine.capture_options = options;
    engine.capture_epoch = epoch;
    let (snapshot, graph, _, _) = engine.index(&support::d()).unwrap();
    assert_eq!(
        capture::options_for_snapshot(&engine.store, &snapshot)
            .unwrap()
            .validated()
            .unwrap(),
        expected
    );
    assert_eq!(
        capture::analyzer_epoch_for_snapshot(&engine.store, &snapshot).unwrap(),
        epoch
    );
    let view = graph_view::build(
        &engine.store,
        &snapshot,
        &graph,
        &BuildOptions::default(),
        &support::d(),
    )
    .unwrap();
    (snapshot, view)
}

fn compare_views(
    engine: &Engine,
    before: &View,
    after: &View,
) -> anyhow::Result<view_comparison::Comparison> {
    error_oracle::checked(view_comparison::compare_published(
        &engine.store,
        &SelectionPin::from_view(before),
        &SelectionPin::from_view(after),
        &ComparisonOptions::default(),
        &support::d(),
    ))
}

fn source_root() -> (tempfile::TempDir, Engine) {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/a.rs"), support::A).unwrap();
    let engine = Engine::open(dir.path()).unwrap();
    (dir, engine)
}

#[test]
fn h06_one_hundred_real_primary_declarations_pass_and_one_hundred_one_refuse() {
    let dir = tempfile::tempdir().unwrap();
    for index in 0..101 {
        fs::write(dir.path().join(format!("f{index:03}.rs")), support::A).unwrap();
    }
    let mut engine = Engine::open(dir.path()).unwrap();
    let (snapshot, view) = captured_view(&mut engine, CaptureOptions::default(), AnalyzerEpoch::V9);
    assert_eq!(snapshot.files.len(), 101);
    let entries = || {
        snapshot
            .files
            .iter()
            .map(|file| declared(&snapshot, &file.path, &snapshot, &file.path))
    };
    let baseline = oracle::state(&engine);
    let exact = ComparisonOptions {
        declared_file_continuity: entries().take(100).collect(),
        ..Default::default()
    };
    let pin = SelectionPin::from_view(&view);
    let positive = error_oracle::checked(view_comparison::compare_published(
        &engine.store,
        &pin,
        &pin,
        &exact,
        &support::d(),
    ))
    .unwrap();
    assert_eq!(
        positive.value()["declaredFileContinuity"]
            .as_array()
            .unwrap()
            .len(),
        100
    );
    let over = ComparisonOptions {
        declared_file_continuity: entries().collect(),
        ..Default::default()
    };
    assert_eq!(over.declared_file_continuity.len(), 101);
    assert!(
        error_oracle::checked(view_comparison::compare_published(
            &engine.store,
            &pin,
            &pin,
            &over,
            &support::d()
        ))
        .is_err()
    );
    assert_eq!(oracle::state(&engine), baseline);
}

// Independent complete policy literals: no CaptureOptions serialization and no
// returned comparison value is used to construct an expected policy digest.
fn literal_policy_hash(literal: &str, proven_epoch: AnalyzerEpoch) -> String {
    let mut policy: serde_json::Value = serde_json::from_str(literal).unwrap();
    assert_eq!(policy.as_object().unwrap().len(), 3);
    assert_eq!(policy["profile"], "yoke_view_comparison_policy_1");
    assert_eq!(policy["analyzerEpoch"], proven_epoch.producer());
    let fields = policy["captureOptions"].as_object().unwrap();
    assert_eq!(fields.len(), 5);
    for field in [
        "onlyDirs",
        "includeDirs",
        "languageOverrides",
        "readOnlyDependencyDirs",
        "includeIgnoredDirs",
    ] {
        assert!(fields.contains_key(field));
    }
    // Explicitly canonicalize ALL nested objects in both serde_json map modes.
    // Array ordering and explicit empty fields remain significant.
    policy.sort_all_objects();
    yoke::model::identity(&policy)
}

#[test]
fn h14_each_of_the_five_genuine_capture_option_fields_is_a_compatibility_boundary() {
    let (_dir, mut engine) = source_root();
    let (base_snapshot, base) =
        captured_view(&mut engine, CaptureOptions::default(), AnalyzerEpoch::V9);
    let first = compare_views(&engine, &base, &base).unwrap();
    let base_epoch = capture::analyzer_epoch_for_snapshot(&engine.store, &base_snapshot).unwrap();
    let expected_default = literal_policy_hash(
        r#"{"profile":"yoke_view_comparison_policy_1","analyzerEpoch":"yoke-tree-sitter-9","captureOptions":{"onlyDirs":[],"includeDirs":[],"languageOverrides":{},"readOnlyDependencyDirs":[],"includeIgnoredDirs":[]}}"#,
        base_epoch,
    );
    assert_eq!(
        first.value()["before"]["comparisonPolicyHash"],
        expected_default
    );
    assert_eq!(
        first.value()["after"]["comparisonPolicyHash"],
        expected_default
    );
    assert_eq!(
        literal_policy_hash(
            r#"{"captureOptions":{"includeIgnoredDirs":[],"readOnlyDependencyDirs":[],"languageOverrides":{},"includeDirs":[],"onlyDirs":[]},"analyzerEpoch":"yoke-tree-sitter-9","profile":"yoke_view_comparison_policy_1"}"#,
            base_epoch,
        ),
        expected_default,
        "Differently ordered complete and nested objects have one canonical digest"
    );
    let base_hash = first.value()["before"]["comparisonPolicyHash"]
        .as_str()
        .unwrap()
        .to_owned();
    let choices = [
        CaptureOptions {
            only_dirs: vec!["src".into()],
            ..Default::default()
        },
        CaptureOptions {
            include_dirs: vec!["unused".into()],
            ..Default::default()
        },
        CaptureOptions {
            language_overrides: BTreeMap::from([("rs".into(), "rust".into())]),
            ..Default::default()
        },
        CaptureOptions {
            read_only_dependency_dirs: vec!["target".into()],
            ..Default::default()
        },
        CaptureOptions {
            include_ignored_dirs: vec!["unused".into()],
            ..Default::default()
        },
    ];
    let expected_policies = [
        r#"{"profile":"yoke_view_comparison_policy_1","analyzerEpoch":"yoke-tree-sitter-9","captureOptions":{"onlyDirs":["src"],"includeDirs":[],"languageOverrides":{},"readOnlyDependencyDirs":[],"includeIgnoredDirs":[]}}"#,
        r#"{"profile":"yoke_view_comparison_policy_1","analyzerEpoch":"yoke-tree-sitter-9","captureOptions":{"onlyDirs":[],"includeDirs":["unused"],"languageOverrides":{},"readOnlyDependencyDirs":[],"includeIgnoredDirs":[]}}"#,
        r#"{"profile":"yoke_view_comparison_policy_1","analyzerEpoch":"yoke-tree-sitter-9","captureOptions":{"onlyDirs":[],"includeDirs":[],"languageOverrides":{"rs":"rust"},"readOnlyDependencyDirs":[],"includeIgnoredDirs":[]}}"#,
        r#"{"profile":"yoke_view_comparison_policy_1","analyzerEpoch":"yoke-tree-sitter-9","captureOptions":{"onlyDirs":[],"includeDirs":[],"languageOverrides":{},"readOnlyDependencyDirs":["target"],"includeIgnoredDirs":[]}}"#,
        r#"{"profile":"yoke_view_comparison_policy_1","analyzerEpoch":"yoke-tree-sitter-9","captureOptions":{"onlyDirs":[],"includeDirs":[],"languageOverrides":{},"readOnlyDependencyDirs":[],"includeIgnoredDirs":["unused"]}}"#,
    ];
    assert_eq!(choices.len(), expected_policies.len());
    for (options, expected_literal) in choices.into_iter().zip(expected_policies) {
        assert_ne!(options.validated().unwrap(), CaptureOptions::default());
        let (snapshot, changed) = captured_view(&mut engine, options, AnalyzerEpoch::V9);
        assert_eq!(
            snapshot.files, base_snapshot.files,
            "Policy mismatch control has identical genuine source files"
        );
        let baseline = oracle::state(&engine);
        let own = compare_views(&engine, &changed, &changed).unwrap();
        let epoch = capture::analyzer_epoch_for_snapshot(&engine.store, &snapshot).unwrap();
        let expected = literal_policy_hash(expected_literal, epoch);
        assert_eq!(own.value()["before"]["comparisonPolicyHash"], expected);
        assert_eq!(own.value()["after"]["comparisonPolicyHash"], expected);
        assert_ne!(own.value()["before"]["comparisonPolicyHash"], base_hash);
        let error = match compare_views(&engine, &base, &changed) {
            Ok(value) => {
                drop(value);
                panic!("Genuine option mismatch was accepted")
            }
            Err(error) => error,
        };
        assert!(format!("{error:#}").contains("comparison_policy_mismatch"));
        assert_eq!(oracle::state(&engine), baseline);
    }
}

#[test]
fn h14_normalized_options_replay_identically_including_both_empty_optional_fields() {
    let (_dir, mut engine) = source_root();
    let canonical = CaptureOptions {
        only_dirs: vec!["src".into()],
        include_dirs: vec!["cache".into(), "vendor".into()],
        ..Default::default()
    };
    let repeated = CaptureOptions {
        only_dirs: vec!["src".into(), "src".into()],
        include_dirs: vec!["vendor".into(), "cache".into(), "vendor".into()],
        ..Default::default()
    };
    assert_eq!(
        canonical.validated().unwrap(),
        repeated.validated().unwrap()
    );
    assert!(
        canonical.read_only_dependency_dirs.is_empty() && canonical.include_ignored_dirs.is_empty()
    );
    let (a_snapshot, a) = captured_view(&mut engine, canonical, AnalyzerEpoch::V9);
    let (b_snapshot, b) = captured_view(&mut engine, repeated, AnalyzerEpoch::V9);
    assert_eq!(a_snapshot.id, b_snapshot.id);
    let baseline = oracle::state(&engine);
    let result = compare_views(&engine, &a, &b).unwrap();
    let a_epoch = capture::analyzer_epoch_for_snapshot(&engine.store, &a_snapshot).unwrap();
    let b_epoch = capture::analyzer_epoch_for_snapshot(&engine.store, &b_snapshot).unwrap();
    let expected_a = literal_policy_hash(
        r#"{"profile":"yoke_view_comparison_policy_1","analyzerEpoch":"yoke-tree-sitter-9","captureOptions":{"onlyDirs":["src"],"includeDirs":["cache","vendor"],"languageOverrides":{},"readOnlyDependencyDirs":[],"includeIgnoredDirs":[]}}"#,
        a_epoch,
    );
    let expected_b = literal_policy_hash(
        r#"{"captureOptions":{"includeIgnoredDirs":[],"readOnlyDependencyDirs":[],"languageOverrides":{},"includeDirs":["cache","vendor"],"onlyDirs":["src"]},"analyzerEpoch":"yoke-tree-sitter-9","profile":"yoke_view_comparison_policy_1"}"#,
        b_epoch,
    );
    assert_eq!(expected_a, expected_b);
    assert_eq!(result.value()["before"]["comparisonPolicyHash"], expected_a);
    assert_eq!(result.value()["after"]["comparisonPolicyHash"], expected_b);
    assert_eq!(
        result.value()["before"]["comparisonPolicyHash"],
        result.value()["after"]["comparisonPolicyHash"]
    );
    assert_eq!(oracle::state(&engine), baseline);
}

#[test]
fn h14_individually_valid_epoch8_and_epoch9_refuse_cross_epoch_in_both_orders() {
    let (_dir, mut engine) = source_root();
    let (_, v8) = captured_view(&mut engine, CaptureOptions::default(), AnalyzerEpoch::V8);
    let (_, v9) = captured_view(&mut engine, CaptureOptions::default(), AnalyzerEpoch::V9);
    let baseline = oracle::state(&engine);
    let eight = compare_views(&engine, &v8, &v8).unwrap();
    let nine = compare_views(&engine, &v9, &v9).unwrap();
    assert_ne!(
        eight.value()["before"]["comparisonPolicyHash"],
        nine.value()["before"]["comparisonPolicyHash"]
    );
    assert!(compare_views(&engine, &v8, &v9).is_err());
    assert!(compare_views(&engine, &v9, &v8).is_err());
    assert_eq!(oracle::state(&engine), baseline);
}
