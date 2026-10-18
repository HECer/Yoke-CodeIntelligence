//! Run against OLD production before feature/stub promotion, then freeze the literal hash.
#[path = "support/monorepo_retrieval_fixture.rs"]
mod fixture;
use fixture::*;
use yoke::{capture::Deadline, model::hash, retrieval};
const PRE_FEATURE_GLOBAL_JSON_SHA256: &str =
    "56a374f44ff295af6cbcd19f71f529d545abe156c0ea739936cb4e069d88aed3";
#[test]
fn native_global_complete_json_matches_pre_feature_frozen_oracle() {
    let f = literal31();
    let before = state(&f.engine);
    let result = retrieval::find(
        &f.engine.store,
        &f.snapshot,
        &f.graph,
        &query(),
        &Deadline::new(30000),
    )
    .unwrap();
    let names = result["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(names, ["big_first", "big_second"]);
    assert_eq!(result["candidateCount"], 31);
    let actual = hash(serde_json::to_vec(&result).unwrap());
    assert_eq!(
        actual, PRE_FEATURE_GLOBAL_JSON_SHA256,
        "Freeze this literal OLD-production hash before promoting new mode stubs"
    );
    assert_eq!(state(&f.engine), before);
}
