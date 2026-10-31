use super::*;
use serde_json::json;

fn config(v: Value) -> Config {
    Config::from_yaml(&v).unwrap().unwrap()
}
fn chosen(v: Value, paths: &[&str]) -> Option<String> {
    select(&config(v), paths.iter().copied(), 0, &Deadline::new(10000))
        .unwrap()
        .backend_id
}
#[test]
fn counts_precede_priority_and_public_order_breaks_the_last_tie() {
    assert_eq!(
        chosen(json!({"python":100}), &["a.rs", "b.rs", "a.py"]).as_deref(),
        Some("rust")
    );
    assert_eq!(
        chosen(json!({"python":3}), &["a.rs", "a.py"]).as_deref(),
        Some("python")
    );
    assert_eq!(
        chosen(json!({"vbnet":2}), &["a.cs"]).as_deref(),
        Some("csharp")
    );
}
#[test]
fn zero_and_negative_exclude_secondary_and_nonprogramming_defaults_stay_dormant() {
    assert_eq!(chosen(json!({}), &["a.py"]).as_deref(), Some("python"));
    assert_eq!(
        chosen(json!({"python":0,"python_basedpyright":3}), &["a.py"]).as_deref(),
        Some("python_basedpyright")
    );
    assert_eq!(chosen(json!({"python":-2}), &["a.py"]), None);
    assert_eq!(chosen(json!({}), &["a.graphql", "a.json", "a.html"]), None);
    assert_eq!(
        chosen(json!({"graphql":3}), &["a.graphql"]).as_deref(),
        Some("graphql")
    );
}
#[test]
fn supersets_win_only_for_more_files_unless_the_host_changes_the_tie_priority() {
    assert_eq!(chosen(json!({}), &["a.cs"]).as_deref(), Some("csharp"));
    assert_eq!(
        chosen(json!({}), &["a.cs", "b.vb"]).as_deref(),
        Some("vbnet")
    );
    assert_eq!(chosen(json!({}), &["a.ts"]).as_deref(), Some("typescript"));
    assert_eq!(
        chosen(json!({}), &["a.ts", "b.vue"]).as_deref(),
        Some("vue")
    );
    assert_eq!(chosen(json!({"vue":3}), &["a.ts"]).as_deref(), Some("vue"));
}
#[test]
fn matcher_case_and_multicomponent_suffixes_preserve_opaque_unicode_paths() {
    assert!(matches_backend("ada", "src/\u{130} \u{1f4a1} e\u{301}.ADB"));
    assert!(!matches_backend("rust", "src/owned.RS"));
    assert!(matches_backend("erlang", "src/owned.app.src"));
    assert!(!matches_backend("svelte", "src/owned.tsx"));
    assert!(matches_backend("vue", "src/owned.tsx"));
    assert!(matches_backend("typescript_tsgo", "src/owned.ctsx"));
}
#[test]
fn declaration_data_covers_all_current_ids_without_runtime_authority() {
    assert_eq!(rows().len(), 80);
    assert!(
        rows()
            .iter()
            .all(|r| crate::server_catalog::get(&r.id).is_some())
    );
    assert_eq!(contract_binding().len(), 64);
}
#[test]
fn malformed_policy_and_unsafe_or_duplicate_paths_refuse_without_clipping() {
    for value in [
        json!(true),
        json!([]),
        json!({"unknown":2}),
        json!({"rust":true}),
        json!({"rust":"2"}),
        json!({"rust":1.5}),
        json!({"rust":2147483648i64}),
    ] {
        assert!(Config::from_yaml(&value).is_err(), "{value}");
    }
    for paths in [
        vec!["../bad.rs"],
        vec!["/bad.rs"],
        vec!["x\0.rs"],
        vec!["a.rs", "a.rs"],
    ] {
        assert!(select(&Config::default(), paths, 0, &Deadline::new(10000)).is_err());
    }
    assert!(select(&Config::default(), ["a.rs"], 0, &Deadline::new(0)).is_err());
    assert!(
        select(
            &Config::default(),
            ["a.rs"],
            MAX_FILES + 1,
            &Deadline::new(10000)
        )
        .is_err()
    );
}
#[test]
fn decision_hash_is_order_independent_and_partial_capture_is_not_full_detection() {
    let d = Deadline::new(10000);
    let a = select(&Config::default(), ["a.rs", "b.py"], 3, &d).unwrap();
    let b = select(&Config::default(), ["b.py", "a.rs"], 3, &d).unwrap();
    assert_eq!(a, b);
    assert_eq!(a.omitted_paths, 3);
    assert_eq!(a.captured_files, 2);
    assert_eq!(a.matched_files, 1);
    let mut altered = a.clone();
    altered.backend_id = Some("unknown".into());
    assert!(altered.validate().is_err());
    altered = a.clone();
    altered.matched_files = 0;
    assert!(altered.validate().is_err());
}
