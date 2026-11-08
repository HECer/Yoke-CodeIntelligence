use yoke as test_yoke;
#[path = "support/git_impact_fixture.rs"]
mod fixture;
use fixture::{Fixture, MOVED_BASE, MOVED_HEAD, state};
use serde_json::json;
use yoke::{capture::Deadline, git_workflows};

#[test]
fn owned_move_report_replays_both_original_sources_and_all_parallel_occurrences() {
    let f = Fixture::moved();
    assert_eq!(
        f.report["changes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["status"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["deleted", "added"]
    );
    for (side, path, text) in [
        ("base", "src/alt-ü.rs", MOVED_BASE),
        ("head", "src/neu-λ.rs", MOVED_HEAD),
    ] {
        let snapshot = f
            .engine
            .store
            .snapshot(f.report[format!("{side}SnapshotId")].as_str().unwrap())
            .unwrap();
        let (captured, hash) = yoke::query::source(&f.engine.store, &snapshot, path).unwrap();
        assert_eq!(captured, text);
        assert_eq!(hash, yoke::model::hash(text));
        let calls: Vec<_> = f.report["impactObservations"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row["side"] == side && row["relation"] == "calls")
            .collect();
        assert_eq!(calls.len(), 2, "genuine parallel calls on each side");
        assert_ne!(calls[0]["id"], calls[1]["id"]);
        for call in calls {
            let evidence = call["evidence"].as_array().unwrap();
            assert!(!evidence.is_empty());
            for anchor in evidence {
                let start = anchor["startByte"].as_u64().unwrap() as usize;
                let end = anchor["endByte"].as_u64().unwrap() as usize;
                assert_eq!(&text[start..end], "changed()");
            }
        }
    }
    let before = state(&f.engine);
    assert_eq!(
        git_workflows::load(&f.engine.store, f.id(), &Deadline::new(30_000)).unwrap(),
        f.report
    );
    assert_eq!(state(&f.engine), before);
    assert_eq!(
        std::fs::read(f.dir.path().join(".git/index")).unwrap(),
        f.index_bytes
    );
}

#[test]
fn independent_quote_oracle_distinguishes_unicode_bytes_crlf_empty_and_null() {
    assert_eq!(
        fixture::expected_range("é\r\nλ\n", 0, 2),
        json!({"startByte":0,"endByte":2,"startLine":1,"endLine":1})
    );
    assert_eq!(
        fixture::expected_range("é\r\nλ\n", 4, 6),
        json!({"startByte":4,"endByte":6,"startLine":2,"endLine":2})
    );
    assert_eq!(
        fixture::expected_range("é\r\nλ\n", 4, 4),
        json!({"startByte":4,"endByte":4,"startLine":2,"endLine":2})
    );
    let f = Fixture::new(
        &[("same.rs", "fn one() {}\n")],
        &[("same.rs", "fn one() {}\nfn two() {}\n")],
    );
    let hunks = f.report["changes"][0]["hunks"].as_array().unwrap();
    assert!(
        hunks
            .iter()
            .any(|hunk| hunk["base"]["startByte"] == hunk["base"]["endByte"])
    );
    let deleted = Fixture::new(&[("gone.rs", "fn gone() {}\n")], &[]);
    assert!(deleted.report["changes"][0]["hunks"][0]["head"].is_null());
}

#[test]
fn configured_real_capture_retains_the_policy_record_used_by_private_denial_cases() {
    let f = Fixture::moved_scoped();
    let head = f.report["headSnapshotId"].as_str().unwrap();
    let record: yoke::capture::CaptureOptionsRecord = f
        .engine
        .store
        .get_record(yoke::capture::OPTIONS_RECORD_KIND, head)
        .unwrap();
    assert_eq!(record.options.only_dirs, ["src"]);
    assert_eq!(record.options_hash, yoke::model::identity(&record.options));
    for side in ["base", "head"] {
        let snapshot = f
            .engine
            .store
            .snapshot(f.report[format!("{side}SnapshotId")].as_str().unwrap())
            .unwrap();
        let scope = yoke::capture::source_scope_policy(&f.engine.store, &snapshot).unwrap();
        for file in &snapshot.files {
            assert!(file.path.starts_with("src/"));
            scope.path_policy(&file.path).unwrap();
            assert_eq!(
                scope.read_source(&file.path).unwrap(),
                f.text(side, &file.path).as_bytes()
            );
        }
    }
    assert_eq!(
        git_workflows::load(&f.engine.store, f.id(), &Deadline::new(30_000)).unwrap(),
        f.report
    );
}
