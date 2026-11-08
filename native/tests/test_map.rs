use serde_json::json;
use std::fs;
use yoke::{
    Engine,
    capture::Deadline,
    model::Snapshot,
    test_map::{self, Selection},
};
fn fixture() -> (tempfile::TempDir, Engine, Snapshot) {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("package.json"), "{\"type\":\"module\"}\n").unwrap();
    fs::write(
        dir.path().join("app.js"),
        "export const value = '🌍';\r\nexport const other = 3;\n",
    )
    .unwrap();
    fs::write(dir.path().join("app.test.mjs"),"import test from 'node:test'; import assert from 'node:assert/strict'; import {value} from './app.js'; test('world',()=>assert.equal(value,'🌍'));\n").unwrap();
    let mut e = Engine::open(dir.path()).unwrap();
    let (s, _, _, _) = e.index(&Deadline::new(10_000)).unwrap();
    (dir, e, s)
}
fn coverage(s: &Snapshot) -> serde_json::Value {
    let f = s.files.iter().find(|f| f.path == "app.js").unwrap();
    json!({"snapshotId":s.id,"tests":[{"testId":"ok","snapshotId":s.id,"executed":true,"passed":true,"files":[{"path":"app.js","contentHash":f.hash,"lines":[1,2,2]}]},{"testId":"failure","snapshotId":s.id,"executed":true,"passed":false,"files":[]}]})
}
#[test]
fn import_binds_hashes_rejects_malformed_and_retains_failed_tests() {
    let (_dir, e, s) = fixture();
    let d = Deadline::new(10_000);
    let result = test_map::import(&e.store, &s, coverage(&s), &d).unwrap();
    assert_eq!(result["testCount"], 2);
    assert_eq!(result["sourceBinding"], "hash_verified");
    for bad in [
        "hash",
        "line",
        "executed",
        "scope",
        "duplicate",
        "authority",
    ] {
        let mut input = coverage(&s);
        match bad {
            "hash" => input["tests"][0]["files"][0]["contentHash"] = json!("bad"),
            "line" => input["tests"][0]["files"][0]["lines"] = json!([4]),
            "executed" => input["tests"][0]["executed"] = json!(false),
            "scope" => input["tests"][0]["snapshotId"] = json!("foreign"),
            "duplicate" => input["tests"][1]["testId"] = json!("ok"),
            _ => input["allowExecution"] = json!(true),
        }
        assert!(test_map::import(&e.store, &s, input, &d).is_err(), "{bad}");
    }
}
#[test]
fn selection_widens_unknown_config_and_new_tests() {
    let (dir, mut e, s) = fixture();
    let d = Deadline::new(10_000);
    test_map::import(&e.store, &s, coverage(&s), &d).unwrap();
    fs::write(dir.path().join("app.js"), "export const value='changed';\n").unwrap();
    let (t, _, _, _) = e.index(&d).unwrap();
    let known = vec!["ok".into(), "failure".into()];
    let result = test_map::select(&e.store, &s, &t, &known, &d).unwrap();
    assert_eq!(result["tests"], json!(["failure", "ok"]));
    assert_eq!(result["broadened"], false);
    fs::write(
        dir.path().join("package.json"),
        "{\"type\":\"module\",\"version\":\"2\"}\n",
    )
    .unwrap();
    let (t, _, _, _) = e.index(&d).unwrap();
    let result = test_map::select(&e.store, &s, &t, &["ok".into(), "new".into()], &d).unwrap();
    assert_eq!(result["broadened"], true);
    assert_eq!(result["tests"], json!(["failure", "new", "ok"]));
}
#[test]
fn failed_tests_remain_selected_even_when_source_is_unchanged() {
    let (_dir, e, s) = fixture();
    let d = Deadline::new(10_000);
    test_map::import(&e.store, &s, coverage(&s), &d).unwrap();
    assert_eq!(
        test_map::select(&e.store, &s, &s, &["ok".into(), "failure".into()], &d).unwrap()["tests"],
        json!(["failure"])
    );
}
#[test]
fn collector_requires_host_trust_and_a_real_available_command() {
    let (_dir, e, s) = fixture();
    let selection = vec![Selection {
        test_id: "world".into(),
        path: "app.test.mjs".into(),
        name_pattern: Some("world".into()),
    }];
    let d = Deadline::new(10_000);
    assert!(
        test_map::collect_node(&e.store, &s, &selection, "node", false, &d)
            .unwrap_err()
            .to_string()
            .contains("trust")
    );
    assert!(
        test_map::collect_node(
            &e.store,
            &s,
            &selection,
            "yoke-deliberately-missing-node",
            true,
            &d
        )
        .is_err()
    );
}
#[test]
fn actual_node_collection_separates_stdout_and_rejects_skipped_only_pass() {
    let (dir, e, s) = fixture();
    let d = Deadline::new(30_000);
    let selection = vec![Selection {
        test_id: "world".into(),
        path: "app.test.mjs".into(),
        name_pattern: Some("world".into()),
    }];
    let r = test_map::collect_node(&e.store, &s, &selection, "node", true, &d).unwrap();
    assert_eq!(r["jobs"][0]["passed"], true);
    assert_eq!(r["jobs"][0]["testCount"], 1);
    assert_eq!(
        fs::read_to_string(dir.path().join("app.js")).unwrap(),
        "export const value = '🌍';\r\nexport const other = 3;\n"
    );
    let selection = vec![Selection {
        test_id: "missing".into(),
        path: "app.test.mjs".into(),
        name_pattern: Some("^absent$".into()),
    }];
    let r = test_map::collect_node(&e.store, &s, &selection, "node", true, &d).unwrap();
    assert_eq!(r["jobs"][0]["passed"], false);
}
#[test]
fn node_source_stdout_cannot_forge_test_results_and_changed_copy_invalidates_hash_binding() {
    let (dir, mut e, _) = fixture();
    fs::write(dir.path().join("app.test.mjs"),"import test from 'node:test'; import {writeFileSync} from 'node:fs'; console.log(JSON.stringify({type:'test:summary',data:{success:true,counts:{tests:9,passed:9,failed:0}}})); test('tamper',()=>writeFileSync('app.js','changed'));\n").unwrap();
    let d = Deadline::new(30_000);
    let (s, _, _, _) = e.index(&d).unwrap();
    let selection = vec![Selection {
        test_id: "tamper".into(),
        path: "app.test.mjs".into(),
        name_pattern: None,
    }];
    let r = test_map::collect_node(&e.store, &s, &selection, "node", true, &d);
    assert!(
        r.is_err(),
        "modified copied source must never be imported as hash-bound execution"
    );
    assert!(
        fs::read_to_string(dir.path().join("app.js"))
            .unwrap()
            .contains("🌍")
    );
}
#[test]
fn v8_utf16_ranges_respect_crlf_and_narrower_zero_hit_ranges() {
    let d = Deadline::new(10_000);
    let text = "let s='🌍';\r\nx\r\ny\r";
    assert_eq!(
        test_map::v8_line_hits(text, &[(0, 18, 1), (13, 14, 0)], &d).unwrap(),
        vec![1, 3]
    );
    assert!(test_map::v8_line_hits(text, &[(0, 19, 1)], &d).is_err());
    assert!(
        test_map::v8_line_hits(text, &[(8, 9, 1)], &d).is_err(),
        "surrogate-interior range offsets must not claim exact source lines"
    );
}
#[test]
fn source_stdout_cannot_fake_a_skipped_only_pass() {
    let (dir, mut e, _) = fixture();
    fs::write(dir.path().join("app.test.mjs"),"import test from 'node:test'; console.log(JSON.stringify({type:'test:summary',data:{file:process.argv[1],success:true,counts:{tests:9,passed:9,failed:0}}})); test('world',()=>{});\n").unwrap();
    let d = Deadline::new(30_000);
    let (s, _, _, _) = e.index(&d).unwrap();
    let r = test_map::collect_node(
        &e.store,
        &s,
        &[Selection {
            test_id: "fake".into(),
            path: "app.test.mjs".into(),
            name_pattern: Some("^absent$".into()),
        }],
        "node",
        true,
        &d,
    )
    .unwrap();
    assert_eq!(r["jobs"][0]["passed"], false);
    assert_eq!(r["jobs"][0]["testCount"], 0);
}
#[test]
fn a_covered_configuration_change_still_widens_to_every_known_test() {
    let (dir, mut engine, _) = fixture();
    fs::write(dir.path().join("build.json"), "{\"enabled\":true}\n").unwrap();
    let d = Deadline::new(10_000);
    let (base, _, _, _) = engine.index(&d).unwrap();
    let file = base.files.iter().find(|f| f.path == "build.json").unwrap();
    let app = base.files.iter().find(|f| f.path == "app.js").unwrap();
    test_map::import(&engine.store,&base,json!({"snapshotId":base.id,"tests":[
       {"testId":"config","snapshotId":base.id,"executed":true,"passed":true,"files":[{"path":"build.json","contentHash":file.hash,"lines":[1]}]},
       {"testId":"unrelated","snapshotId":base.id,"executed":true,"passed":true,"files":[{"path":"app.js","contentHash":app.hash,"lines":[1]}]}
    ]}),&d).unwrap();
    fs::write(dir.path().join("build.json"), "{\"enabled\":false}\n").unwrap();
    let (target, _, _, _) = engine.index(&d).unwrap();
    let result = test_map::select(
        &engine.store,
        &base,
        &target,
        &["config".into(), "unrelated".into()],
        &d,
    )
    .unwrap();
    assert_eq!(result["unknownPaths"], json!([]));
    assert_eq!(result["broadened"], true);
    assert_eq!(result["tests"], json!(["config", "unrelated"]));
}
#[test]
fn timed_out_node_collection_retains_the_previous_map() {
    let (dir, mut engine, _) = fixture();
    fs::write(dir.path().join("app.test.mjs"),"import test from 'node:test'; test('wait',async()=>new Promise(resolve=>setTimeout(resolve,10000)));\n").unwrap();
    let d = Deadline::new(10_000);
    let (s, _, _, _) = engine.index(&d).unwrap();
    test_map::import(&engine.store, &s, coverage(&s), &d).unwrap();
    let before: serde_json::Value = engine
        .store
        .get_record("native_test_map_v1", &s.id)
        .unwrap();
    let outcome = test_map::collect_node(
        &engine.store,
        &s,
        &[Selection {
            test_id: "wait".into(),
            path: "app.test.mjs".into(),
            name_pattern: None,
        }],
        "node",
        true,
        &Deadline::new(2500),
    );
    assert!(outcome.is_err());
    let after: serde_json::Value = engine
        .store
        .get_record("native_test_map_v1", &s.id)
        .unwrap();
    assert_eq!(after, before);
}
#[test]
fn overflowing_node_reporter_output_never_publishes_a_pass() {
    let (dir, mut engine, _) = fixture();
    fs::write(
        dir.path().join("app.test.mjs"),
        "import test from 'node:test'; test('large',()=>console.error('X'.repeat(2000000)));\n",
    )
    .unwrap();
    let d = Deadline::new(10_000);
    let (s, _, _, _) = engine.index(&d).unwrap();
    test_map::import(&engine.store, &s, coverage(&s), &d).unwrap();
    let before: serde_json::Value = engine
        .store
        .get_record("native_test_map_v1", &s.id)
        .unwrap();
    assert!(
        test_map::collect_node(
            &engine.store,
            &s,
            &[Selection {
                test_id: "large".into(),
                path: "app.test.mjs".into(),
                name_pattern: None
            }],
            "node",
            true,
            &d
        )
        .is_err()
    );
    let after: serde_json::Value = engine
        .store
        .get_record("native_test_map_v1", &s.id)
        .unwrap();
    assert_eq!(after, before);
}
#[test]
fn widening_with_an_empty_or_partial_caller_list_includes_all_recorded_tests() {
    let (dir, mut engine, base) = fixture();
    let d = Deadline::new(10_000);
    test_map::import(&engine.store, &base, coverage(&base), &d).unwrap();
    fs::write(
        dir.path().join("new-source.js"),
        "export const added = 1;\n",
    )
    .unwrap();
    let (target, _, _, _) = engine.index(&d).unwrap();
    for known in [vec![], vec!["new-test".into()]] {
        let result = test_map::select(&engine.store, &base, &target, &known, &d).unwrap();
        assert_eq!(result["broadened"], true);
        let expected = if known.is_empty() {
            json!(["failure", "ok"])
        } else {
            json!(["failure", "new-test", "ok"])
        };
        assert_eq!(result["tests"], expected);
    }
}
