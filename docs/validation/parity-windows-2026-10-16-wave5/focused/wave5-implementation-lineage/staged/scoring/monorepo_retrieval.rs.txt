#[path = "support/monorepo_retrieval_fixture.rs"]
mod fixture;
use fixture::*;
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs};
use yoke::{
    capture::Deadline,
    model::{hash, identity},
    retrieval,
};

fn d() -> Deadline {
    Deadline::new(30000)
}
fn invoke(f: &Fixture, args: &Value) -> Value {
    retrieval::find(&f.engine.store, &f.snapshot, &f.graph, args, &d())
        .expect("Explicit manifest-balanced retrieval must be implemented")
}
fn names(result: &Value) -> Vec<&str> {
    result["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["name"].as_str().unwrap())
        .collect()
}
fn balanced(mut args: Value) -> Value {
    args["scopeMode"] = json!("manifest_balanced_v1");
    args
}
fn assert_original_bindings(f: &Fixture, result: &Value) {
    for item in result["items"].as_array().unwrap() {
        let id = item["id"].as_str().unwrap();
        let entity = f
            .graph
            .entities
            .iter()
            .find(|e| e.id == id)
            .expect("Every ranked item must be an actual captured entity");
        let text = &f.texts[&entity.path];
        assert_eq!(item["snapshotId"], f.snapshot.id);
        assert_eq!(item["path"], entity.path);
        assert_eq!(item["startByte"], entity.start_byte);
        assert_eq!(item["endByte"], entity.end_byte);
        assert_eq!(item["contentHash"], hash(text));
        if let Some(body) = item.get("body") {
            let end = item["bodyEndByte"].as_u64().unwrap() as usize;
            assert_eq!(body.as_str().unwrap(), &text[entity.start_byte..end]);
        }
    }
}
#[test]
fn same_31_source_corpus_has_literal_global_control_and_one_head_per_package() {
    let f = literal31();
    let before = state(&f.engine);
    let graph_id = identity(&f.graph);
    let base = query();
    let global = invoke(&f, &base);
    assert_eq!(
        names(&global),
        ["big_first", "big_second"],
        "The global control must really omit the small package"
    );
    assert_eq!(global["candidateCount"], 31);
    for (index, score) in [(0, 0.025006807306715315), (1, 0.0221843728261494)] {
        assert!((global["items"][index]["bm25Score"].as_f64().unwrap() - score).abs() < 1e-12);
    }
    let mut explicit_global = base.clone();
    explicit_global["scopeMode"] = json!("global");
    let same = invoke(&f, &explicit_global);
    assert_eq!(
        serde_json::to_vec(&global).unwrap(),
        serde_json::to_vec(&same).unwrap()
    );
    assert!(
        same.get("retrievalScope").is_none(),
        "Native global must not probe or add metadata"
    );
    let result = invoke(&f, &balanced(base));
    assert_eq!(names(&result), ["big_first", "small_only"]);
    assert_eq!(
        result["ranking"],
        "source_body_bm25_manifest_balanced_graph_v1"
    );
    assert_eq!(result["retrievalScope"]["actualPackageRootCount"], 2);
    assert_eq!(result["retrievalScope"]["queryEligibleScopeCount"], 2);
    assert_eq!(
        result["retrievalScope"]["selectionReason"],
        "explicit_balanced_request"
    );
    for item in result["items"].as_array().unwrap() {
        assert_eq!(item["localRank"], 1);
        assert_eq!(item["queryCoverageCount"], 1);
        assert_eq!(item["rankingScore"], 1.0);
        assert_eq!(item["normalizedBm25Score"], 1.0);
        assert_eq!(item["graphScore"], 0.0);
        assert_eq!(item["normalizedGraphScore"], 0.0);
        assert!(item["scopeId"].as_str().is_some_and(yoke::model::valid_id));
    }
    assert_eq!(result["candidateCount"], 31);
    assert_eq!(result["perFileRejectedCount"], 0);
    assert_eq!(result["limitOmittedCount"], 29);
    // Independent sole-document corpus: N=df=1, tf=1, L=average, hence ln(4/3).
    // Global BM25 followed only by local normalization would keep .006434426...
    // and must fail this assertion despite still returning the two correct heads.
    let small = result["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["name"] == "small_only")
        .unwrap();
    assert!((small["bm25Score"].as_f64().unwrap() - 0.28768207245178085).abs() < 1e-12);
    assert_original_bindings(&f, &global);
    assert_original_bindings(&f, &result);
    assert_eq!(state(&f.engine), before);
    assert_eq!(identity(&f.graph), graph_id);
}
#[test]
fn balanced_unicode_normalization_preserves_actual_crlf_utf8_body_coordinates() {
    let text = "// Überschrift\r\npub fn café() {\r\n    let label = \"Über Zahlung\";\r\n}\r\n";
    let texts = BTreeMap::from([
        (
            "package.json".into(),
            "{\"workspaces\":[\"packages/*\"]}".into(),
        ),
        (
            "packages/a/package.json".into(),
            "{\"name\":\"café\"}".into(),
        ),
        (
            "packages/b/package.json".into(),
            "{\"name\":\"naïve\"}".into(),
        ),
        ("packages/a/unicode.rs".into(), text.into()),
        (
            "packages/b/unrelated.rs".into(),
            "pub fn unrelated() { let label = \"naïve\"; }\r\n".into(),
        ),
    ]);
    let f = captured(texts, false);
    let before = state(&f.engine);
    for query in ["cafe", "CAFÉ", "cafe\u{301}"] {
        let args = balanced(
            json!({"query":query,"relativePath":"packages","pathGlob":"**/*.rs","graphWeight":0,"includeBody":true}),
        );
        let result = invoke(&f, &args);
        assert_eq!(names(&result), ["café"]);
        let item = &result["items"][0];
        assert_eq!(item["startByte"], 17);
        assert_eq!(item["endByte"], text.len() - 2);
        assert_eq!(item["startLine"], 2);
        assert_eq!(item["endLine"], 4);
        assert_eq!(item["body"].as_str().unwrap(), &text[17..text.len() - 2]);
        assert!(item["body"].as_str().unwrap().contains("\r\n"));
        assert!(item["body"].as_str().unwrap().contains("Über Zahlung"));
        assert_eq!(item["contentHash"], hash(text));
        assert_eq!(item["bodyTruncated"], false);
        assert_eq!(item["queryCoverageCount"], 1);
        assert_original_bindings(&f, &result);
    }
    assert_eq!(state(&f.engine), before);
}
#[test]
fn balanced_ownership_and_bodies_use_the_historic_capture_after_live_marker_and_source_change() {
    let f = literal31();
    let original = invoke(&f, &balanced(query()));
    let before = state(&f.engine);
    fs::write(
        f.directory.path().join("package.json"),
        "{malformed-live-manifest",
    )
    .unwrap();
    fs::write(
        f.directory.path().join("packages/small/one.rs"),
        "fn replaced_live() {}\n",
    )
    .unwrap();
    let replay = invoke(&f, &balanced(query()));
    assert_eq!(
        serde_json::to_vec(&replay).unwrap(),
        serde_json::to_vec(&original).unwrap()
    );
    assert_original_bindings(&f, &replay);
    assert_eq!(state(&f.engine), before);
}
#[test]
fn explicit_balanced_keeps_zero_and_one_query_scope_as_valid_degenerate_results() {
    let f = literal31();
    let before = state(&f.engine);
    let mut one = query();
    one["relativePath"] = json!("packages/small");
    let result = invoke(&f, &balanced(one));
    assert_eq!(names(&result), ["small_only"]);
    assert_eq!(result["retrievalScope"]["queryEligibleScopeCount"], 1);
    assert_eq!(
        result["retrievalScope"]["effectiveMode"],
        "manifest_balanced_v1"
    );
    let mut zero = query();
    zero["pathGlob"] = json!("**/*.go");
    let result = invoke(&f, &balanced(zero));
    assert!(result["items"].as_array().unwrap().is_empty());
    assert_eq!(result["retrievalScope"]["queryEligibleScopeCount"], 0);
    assert_eq!(result["retrievalScope"]["fallback"], Value::Null);
    assert_eq!(state(&f.engine), before);
}
#[test]
fn actual_parallel_calls_propagate_into_seedless_packages_and_weight_zero_skips_graph_work() {
    let texts = BTreeMap::from([
        (
            "package.json".into(),
            "{\"workspaces\":[\"packages/*\"]}".into(),
        ),
        ("packages/a/package.json".into(), "{\"name\":\"a\"}".into()),
        ("packages/b/package.json".into(), "{\"name\":\"b\"}".into()),
        ("packages/c/package.json".into(), "{\"name\":\"c\"}".into()),
        (
            "packages/a/one.rs".into(),
            "pub fn seeded(){ payment(); helper(); helper(); observer(); }\n".into(),
        ),
        (
            "packages/b/one.rs".into(),
            "pub fn helper(){ let other=1; }\n".into(),
        ),
        (
            "packages/c/one.rs".into(),
            "pub fn observer(){ let other=2; }\n".into(),
        ),
    ]);
    let f = captured(texts, false);
    let find_id = |name: &str| {
        f.graph
            .entities
            .iter()
            .find(|e| e.name == name && e.kind != "file")
            .unwrap()
            .id
            .as_str()
    };
    let a = find_id("seeded");
    let b = find_id("helper");
    let c = find_id("observer");
    assert_eq!(
        f.graph
            .observations
            .iter()
            .filter(|e| e.source == a && e.target.as_deref() == Some(b))
            .count(),
        2,
        "A real captured parallel-call fixture is required"
    );
    assert_eq!(
        f.graph
            .observations
            .iter()
            .filter(|e| e.source == a && e.target.as_deref() == Some(c))
            .count(),
        1
    );
    let before = state(&f.engine);
    let original = identity(&f.graph);
    let mut args = query();
    args["limit"] = json!(10);
    args["includeBody"] = json!(false);
    let zero = invoke(&f, &balanced(args.clone()));
    assert_eq!(names(&zero), ["seeded"]);
    assert_eq!(zero["graphIterations"], 0);
    assert_eq!(zero["items"][0]["graphScore"], 0.0);
    args["graphWeight"] = json!(0.5);
    let result = invoke(&f, &balanced(args));
    assert_eq!(names(&result), ["seeded", "helper", "observer"]);
    assert_eq!(result["graphIterations"], 24);
    for (name, expected) in [
        ("seeded", 0.5498366539741663),
        ("helper", 0.30010889735055557),
        ("observer", 0.15005444867527779),
    ] {
        let item = result["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["name"] == name)
            .unwrap();
        assert!(
            (item["graphScore"].as_f64().unwrap() - expected).abs() < 1e-12,
            "{name}: {item}"
        );
    }
    assert_eq!(identity(&f.graph), original);
    assert_eq!(state(&f.engine), before);
}
#[test]
fn real_32m_source_admission_is_protected_before_ranking_and_releases_unused_marker_reserve() {
    let mut texts = eight_package_markers();
    for (path, name, bytes) in [
        ("a/00-large.rs", "a_large", 4_000_000),
        ("a/01-small.rs", "a_small", 500_000),
        ("b/00.rs", "b0", 3_000_000),
        ("b/01.rs", "b1", 500_000),
        ("b/02.rs", "b2", 1_000_000),
        ("c/00.rs", "c0", 1_000_000),
        ("c/01.rs", "c1", 3_000_000),
        ("d/00.rs", "d", 3_500_000),
        ("e/00.rs", "e", 3_500_000),
        ("f/00.rs", "f", 3_500_000),
        ("g/00.rs", "g", 3_500_000),
        ("h/00.rs", "h", 3_500_000),
    ] {
        texts.insert(path.into(), function_body(name, bytes));
    }
    let f = captured(texts, false);
    let before = state(&f.engine);
    let args = balanced(
        json!({"query":"payment","pathGlob":"**/*.rs","limit":200,"perFileLimit":200,"graphWeight":0}),
    );
    let result = invoke(&f, &args);
    let a = &result["retrievalScope"]["admission"];
    assert_eq!(a["markerBytesCharged"], 4_000_000);
    assert_eq!(a["bodyInputBytesCharged"], 27_500_000);
    assert_eq!(a["inputBytesCharged"], 31_500_000);
    assert_eq!(a["admittedSourceBytes"], 27_500_000);
    assert_eq!(a["admittedSourceCount"], 11);
    assert_eq!(a["omittedSourceCount"], 1);
    assert_eq!(
        result["retrievalScope"]["sourceOmissions"][0]["path"],
        "c/01.rs"
    );
    assert_eq!(
        result["retrievalScope"]["sourceOmissions"][0]["reason"],
        "source_budget"
    );
    assert!(names(&result).contains(&"a_small"));
    assert!(names(&result).contains(&"a_large"));
    assert!(!names(&result).contains(&"c1"));
    assert_original_bindings(&f, &result);
    let mut narrow = args;
    narrow["relativePath"] = json!("a");
    let result = invoke(&f, &narrow);
    let a = &result["retrievalScope"]["admission"];
    assert_eq!(result["retrievalScope"]["queryEligibleScopeCount"], 1);
    assert_eq!(a["bodyInputBytesCharged"], 4_500_000);
    assert_eq!(a["inputBytesCharged"], 8_500_000);
    assert_eq!(a["omittedSourceCount"], 0);
    assert_eq!(state(&f.engine), before);
}
#[test]
fn actual_nested_declarations_charge_overlap_under_the_independent_32m_document_budget() {
    let mut texts = eight_package_markers();
    for scope in ["a", "b", "c", "d", "e", "f", "g", "h"] {
        texts.insert(
            format!("{scope}/main.rs"),
            nested_body(
                &format!("outer_{scope}"),
                &format!("inner_{scope}"),
                if scope == "a" { 4_000_000 } else { 3_000_000 },
            ),
        );
    }
    let f = captured(texts, false);
    for scope in ["a", "b", "c", "d", "e", "f", "g", "h"] {
        let parent = f
            .graph
            .entities
            .iter()
            .find(|e| e.name == format!("outer_{scope}"))
            .unwrap();
        let child = f
            .graph
            .entities
            .iter()
            .find(|e| e.name == format!("inner_{scope}"))
            .unwrap();
        assert_eq!(parent.start_byte, 0);
        assert_eq!(
            parent.end_byte,
            if scope == "a" { 4_000_000 } else { 3_000_000 }
        );
        assert_eq!(child.start_byte, 100);
        assert_eq!(child.end_byte, 1_000_100);
    }
    let before = state(&f.engine);
    let result = invoke(
        &f,
        &balanced(
            json!({"query":"payment","pathGlob":"**/*.rs","limit":200,"perFileLimit":200,"graphWeight":0}),
        ),
    );
    let a = &result["retrievalScope"]["admission"];
    assert_eq!(a["inputBytesCharged"], 29_000_000);
    assert_eq!(a["admittedSourceBytes"], 25_000_000);
    assert_eq!(a["documentBytesCharged"], 32_000_000);
    assert_eq!(a["admittedDocumentCount"], 15);
    assert_eq!(a["omittedDocumentCount"], 1);
    assert_eq!(a["omittedSourceCount"], 0);
    assert_eq!(result["items"].as_array().unwrap().len(), 15);
    assert!(!names(&result).contains(&"inner_a"));
    assert!(names(&result).contains(&"outer_a"));
    assert_eq!(state(&f.engine), before);
}
