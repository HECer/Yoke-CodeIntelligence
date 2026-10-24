//! Exact saved-source owner interpretation over genuine owned Git publications.
use yoke as test_yoke;
#[path="support/git_impact_fixture.rs"]
mod fixture;
use yoke::{capture::Deadline,git_workflows::{owners,areas}};

#[test]
fn explicit_empty_rule_has_its_own_exact_quote_and_preserves_legacy_parent() {
    let rules="# β 💡\r\n* @global-owner\r\n/src/a.rs\r\n";
    let f=fixture::Fixture::new(&[("src/a.rs","pub fn a() -> u32 { 1 }\n"),("CODEOWNERS",rules)],&[("src/a.rs","pub fn a() -> u32 { 2 }\n"),("CODEOWNERS",rules)]);
    assert!(f.report["ownerSuggestions"].as_array().unwrap().iter().any(|r|r["owner"]=="@global-owner"));
    let before=fixture::state(&f.engine);
    let parent=areas::derive(&f.engine.store,f.id(),&[],&Default::default(),&Deadline::new(30_000)).unwrap();
    let value=owners::derive(&f.engine.store,f.id(),&Deadline::new(30_000)).unwrap();
    assert_eq!(value["impactAreas"],*parent.value());
    for row in value["ownership"].as_array().unwrap() {
        assert_eq!(row["status"],"explicit_empty_owner_list");
        assert_eq!(row["ownerLabels"],serde_json::json!([]));
        assert_eq!(row["winningRule"]["quote"],"/src/a.rs\r\n");
        let start=row["winningRule"]["startByte"].as_u64().unwrap() as usize;
        let end=row["winningRule"]["endByte"].as_u64().unwrap() as usize;
        assert_eq!(&rules.as_bytes()[start..end],b"/src/a.rs\r\n");
    }
    assert_eq!(fixture::state(&f.engine),before);
    assert_eq!(value["complete"],false);
}
