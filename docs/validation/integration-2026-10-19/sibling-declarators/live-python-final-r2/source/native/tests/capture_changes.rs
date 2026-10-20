use serde_json::json;
use std::{collections::BTreeMap, fs};
use tempfile::TempDir;
use yoke::{
    Engine,
    capture::{CaptureOptions, Deadline},
};
#[test]
fn host_language_overrides_survive_guarded_derived_snapshots_and_workspace_apply() {
    let dir = TempDir::new().unwrap();
    fs::create_dir(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/a.mine"), "fn alpha() {}\n").unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    engine.capture_options = CaptureOptions {
        only_dirs: vec!["src".into()],
        language_overrides: BTreeMap::from([("mine".into(), "rust".into())]),
        ..Default::default()
    };
    let (base, _, _, _) = engine.index(&Deadline::new(20000)).unwrap();
    let result=engine.invoke("ci_change_prepare",json!({"snapshotId":base.id,"edits":[{"kind":"create","path":"src/new.mine","text":"fn beta() {}\n"}]})).unwrap();
    assert_eq!(result["status"], "ok", "{result}");
    let plan: &str = result["data"]["plan"]["id"].as_str().unwrap();
    let derived = engine
        .store
        .snapshot(result["data"]["plan"]["resultSnapshotId"].as_str().unwrap())
        .unwrap();
    assert_eq!(
        derived
            .files
            .iter()
            .find(|f| f.path == "src/new.mine")
            .unwrap()
            .language,
        "rust"
    );
    assert_eq!(
        yoke::capture::options_for_snapshot(&engine.store, &derived).unwrap(),
        engine.capture_options
    );
    engine.approve(plan, false).unwrap();
    let verification=yoke::verify::execute(&engine,plan,"node",&["-e".into(),"const f=require('fs');process.exit(f.readFileSync('src/new.mine','utf8')==='fn beta() {}\\n'?0:1)".into()],true,&Deadline::new(20000)).unwrap();
    assert_eq!(verification["status"], "passed", "{verification}");
    engine.allow_writes = true;
    let applied = engine
        .invoke(
            "ci_change_apply",
            json!({"planId":plan,"idempotencyKey":"captured-overrides","mode":"workspace"}),
        )
        .unwrap();
    assert_eq!(applied["status"], "ok", "{applied}");
    assert_eq!(
        fs::read_to_string(dir.path().join("src/new.mine")).unwrap(),
        "fn beta() {}\n"
    );
}
