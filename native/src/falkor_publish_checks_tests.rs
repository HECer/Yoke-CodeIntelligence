use super::*;
#[test]
fn pending_schema_is_visible_but_never_ready_and_foreign_or_duplicate_shapes_refuse() {
    let mut constraints = json!([
        [
            "type",
            "label",
            "propertyCount",
            "property0",
            "entitytype",
            "status"
        ],
        [
            ["UNIQUE", "YokeNativeScope", 1, "key", "NODE", "OPERATIONAL"],
            [
                "UNIQUE",
                "YokeNativeElement",
                1,
                "key",
                "NODE",
                "OPERATIONAL"
            ],
            [
                "UNIQUE",
                "YOKE_NATIVE_OBSERVATION",
                1,
                "key",
                "RELATIONSHIP",
                "OPERATIONAL"
            ]
        ],
        []
    ]);
    let indexes = json!([
        [
            "label",
            "property",
            "typeCount",
            "type0",
            "entitytype",
            "status"
        ],
        [
            ["YokeNativeScope", "key", 1, "RANGE", "NODE", "OPERATIONAL"],
            [
                "YokeNativeElement",
                "key",
                1,
                "RANGE",
                "NODE",
                "OPERATIONAL"
            ],
            [
                "YokeNativeElement",
                "revisionId",
                1,
                "RANGE",
                "NODE",
                "OPERATIONAL"
            ],
            [
                "YOKE_NATIVE_OBSERVATION",
                "key",
                1,
                "RANGE",
                "RELATIONSHIP",
                "OPERATIONAL"
            ],
            [
                "YOKE_NATIVE_OBSERVATION",
                "revisionId",
                1,
                "RANGE",
                "RELATIONSHIP",
                "OPERATIONAL"
            ]
        ],
        []
    ]);
    let state = schema_values(wire(&constraints), wire(&indexes), false).unwrap();
    assert_eq!(state.confirmed.len(), 8);
    assert_eq!(state.value["allRequiredPresent"], true);
    for status in ["PENDING", "UNDER CONSTRUCTION"] {
        constraints[1][0][5] = json!(status);
        assert!(schema_values(wire(&constraints), wire(&indexes), false).is_err());
        let state = schema_values(wire(&constraints), wire(&indexes), true).unwrap();
        assert_eq!(state.value["allObservedOperational"], false);
        assert!(!state.confirmed.contains(&Operation::ScopeConstraint));
    }
    constraints[1][0][5] = json!("FAILED");
    assert!(schema_values(wire(&constraints), wire(&indexes), true).is_err());
    constraints[1][0][5] = json!("OPERATIONAL");
    let mut foreign = indexes.clone();
    foreign[1][0][1] = json!("foreignProperty");
    assert!(schema_values(wire(&constraints), wire(&foreign), true).is_err());
    let mut duplicate = indexes.clone();
    duplicate[1]
        .as_array_mut()
        .unwrap()
        .push(indexes[1][0].clone());
    assert!(schema_values(wire(&constraints), wire(&duplicate), true).is_err());
    let mut missing = indexes;
    missing[1].as_array_mut().unwrap().pop();
    assert!(schema_values(wire(&constraints), wire(&missing), false).is_err());
    assert_eq!(
        schema_values(wire(&constraints), wire(&missing), true)
            .unwrap()
            .value["allRequiredPresent"],
        false
    );
}
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
