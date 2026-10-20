use super::*;
fn wire(value: &Value) -> Wire {
    match value {
        Value::String(s) => Wire::Bulk(Some(s.as_bytes().to_vec())),
        Value::Number(n) => Wire::Integer(n.as_i64().unwrap()),
        Value::Array(v) => Wire::Array(Some(v.iter().map(wire).collect())),
        Value::Null => Wire::Bulk(None),
        _ => panic!("fixture must use exact scalar response types"),
    }
}
#[test]
fn receipt_requires_exact_fields_types_cardinality_and_reviewed_identity() {
    let d = Deadline::new(30000);
    let target=serde_json::from_value(json!({"baseUrl":"redis://127.0.0.1:6379/","address":"127.0.0.1:6379","namespace":"owned","user":"default","useSystemRoots":false})).unwrap();
    let plan = Plan::bootstrap(target, &d).unwrap();
    let value = json!([
        [
            "scopeKey",
            "schemaVersion",
            "createdByPlan",
            "activeRevision"
        ],
        [[plan.scope(), 1, plan.id(), "absent"]],
        ["Nodes created: 1"]
    ]);
    confirm_response(&plan, Operation::Initialize, &wire(&value), &d).unwrap();
    for (row, field) in [
        (0, json!("foreign")),
        (1, json!("1")),
        (2, json!("unreviewed")),
        (3, json!(null)),
    ] {
        let mut bad = value.clone();
        bad[1][0][row] = field;
        assert!(confirm_response(&plan, Operation::Initialize, &wire(&bad), &d).is_err());
    }
    let mut bad = value.clone();
    bad[0][0] = json!("createdByPlan");
    assert!(confirm_response(&plan, Operation::Initialize, &wire(&bad), &d).is_err());
    let mut bad = value.clone();
    bad[1].as_array_mut().unwrap().push(value[1][0].clone());
    assert!(confirm_response(&plan, Operation::Initialize, &wire(&bad), &d).is_err());
    assert!(confirm_response(&plan, Operation::Initialize, &Wire::Array(None), &d).is_err());
    assert!(
        confirm_response(
            &plan,
            Operation::ScopeConstraint,
            &wire(&json!("FAILED")),
            &d
        )
        .is_err()
    );
}
