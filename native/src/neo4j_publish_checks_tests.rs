use super::*;
#[test]
fn implicit_write_confirmation_refuses_open_transactions_read_only_and_unbookmarked_receipts() {
    let d = Deadline::new(30000);
    let target=serde_json::from_value(json!({"baseUrl":"http://127.0.0.1:7474/","address":"127.0.0.1:7474","database":"neo4j","namespace":"owned","user":"neo4j","useSystemRoots":false})).unwrap();
    let plan = Plan::bootstrap(target, &d).unwrap();
    let receipt = json!({"scopeKey":plan.scope(),"schemaVersion":1,"createdByPlan":plan.id(),"activeRevision":"absent"});
    let valid = json!({"data":{"fields":["receipt"],"values":[[receipt]]},"queryType":"rw","bookmarks":["owned-post-commit-bookmark"]});
    confirm_response(&plan, super::super::Operation::Namespace, &valid, &d).unwrap();
    for (key, bad) in [
        ("transaction", json!({"id":"still-open"})),
        ("queryType", json!("r")),
        ("bookmarks", json!([])),
        ("bookmarks", json!([null])),
    ] {
        let mut value = valid.clone();
        value[key] = bad;
        assert!(confirm_response(&plan, super::super::Operation::Namespace, &value, &d).is_err());
    }
    let mut missing = valid.clone();
    missing.as_object_mut().unwrap().remove("bookmarks");
    assert!(confirm_response(&plan, super::super::Operation::Namespace, &missing, &d).is_err());
    let mut altered = valid;
    altered["data"]["values"][0][0]["activeRevision"] = json!("unreviewed");
    assert!(confirm_response(&plan, super::super::Operation::Namespace, &altered, &d).is_err());
    let schema = json!({"data":{"fields":[],"values":[]},"queryType":"s","bookmarks":["owned-schema-bookmark"]});
    confirm_response(&plan, super::super::Operation::ScopeConstraint, &schema, &d).unwrap();
    let mut wrong = schema;
    wrong["queryType"] = json!("r");
    assert!(confirm_response(&plan, super::super::Operation::ScopeConstraint, &wrong, &d).is_err());
}
