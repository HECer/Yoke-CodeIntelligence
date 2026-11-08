//! Exact fixed-schema and scoped server-response checks; never source attestation.
use super::{
    Mode, Plan, program,
    transport::{Session, response_data},
};
use crate::capture::Deadline;
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use std::collections::BTreeMap;
pub(super) fn read(
    session: &Session,
    statement: &str,
    parameters: Value,
    d: &Deadline,
) -> Result<Value> {
    session.query(&json!({"statement":statement,"parameters":parameters}), d)
}
fn rows(value: &Value, fields: Value) -> Result<&[Value]> {
    let (actual, rows) = response_data(value)?;
    ensure!(
        actual == fields.as_array().unwrap(),
        "neo4j_publish_response_invalid"
    );
    Ok(rows)
}
pub(super) fn schema(session: &Session, allow_missing: bool, d: &Deadline) -> Result<Value> {
    let constraints = read(session, program::SCHEMA_READ, json!({}), d)?;
    let indexes = read(session, program::INDEX_READ, json!({}), d)?;
    let mut expected = BTreeMap::new();
    for (name, kind, entity, label) in [
        ("scope", "UNIQUENESS", "NODE", "YokeNativeScope"),
        ("element", "UNIQUENESS", "NODE", "YokeNativeElement"),
        (
            "observation",
            "RELATIONSHIP_UNIQUENESS",
            "RELATIONSHIP",
            "YOKE_NATIVE_OBSERVATION",
        ),
    ] {
        let name = format!("yoke_native_{name}_key_v1");
        expected.insert(name.clone(), json!([name, kind, entity, [label], ["key"]]));
    }
    let mut seen = std::collections::BTreeSet::new();
    for row in rows(
        &constraints,
        json!(["name", "type", "entityType", "labelsOrTypes", "properties"]),
    )? {
        let name = row[0]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("neo4j_publish_schema_conflict"))?;
        ensure!(
            expected.get(name) == Some(row) && seen.insert(name.to_owned()),
            "neo4j_publish_schema_conflict"
        );
    }
    let constraint_count = seen.len();
    ensure!(
        allow_missing || constraint_count == expected.len(),
        "neo4j_publish_schema_required"
    );
    expected.clear();
    for (name, entity, label) in [
        ("scope", "NODE", "YokeNativeScope"),
        ("element", "NODE", "YokeNativeElement"),
        ("observation", "RELATIONSHIP", "YOKE_NATIVE_OBSERVATION"),
    ] {
        let key = format!("yoke_native_{name}_key_v1");
        expected.insert(
            key.clone(),
            json!([key, "RANGE", entity, [label], ["key"], key]),
        );
    }
    for (name, entity, label) in [
        ("element", "NODE", "YokeNativeElement"),
        ("observation", "RELATIONSHIP", "YOKE_NATIVE_OBSERVATION"),
    ] {
        let key = format!("yoke_native_{name}_revision_v1");
        expected.insert(
            key.clone(),
            json!([
                key,
                "RANGE",
                entity,
                [label],
                ["scopeKey", "revisionId"],
                null
            ]),
        );
    }
    seen.clear();
    let mut online = true;
    for row in rows(
        &indexes,
        json!([
            "name",
            "type",
            "entityType",
            "labelsOrTypes",
            "properties",
            "owningConstraint",
            "state"
        ]),
    )? {
        let cells = row.as_array().unwrap();
        let name = cells[0]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("neo4j_publish_schema_conflict"))?;
        ensure!(
            expected.get(name) == Some(&json!(&cells[..6])) && seen.insert(name.to_owned()),
            "neo4j_publish_schema_conflict"
        );
        ensure!(
            matches!(cells[6].as_str(), Some("ONLINE" | "POPULATING")),
            "neo4j_publish_schema_conflict"
        );
        online &= cells[6] == "ONLINE";
    }
    ensure!(
        allow_missing || seen.len() == expected.len(),
        "neo4j_publish_schema_required"
    );
    ensure!(allow_missing || online, "neo4j_publish_schema_not_online");
    Ok(
        json!({"constraintCount":constraint_count,"indexCount":seen.len(),"allRequiredPresent":constraint_count==3&&seen.len()==5,"allObservedIndexesOnline":online,"serverAttestation":"unverified"}),
    )
}
pub(super) fn scope(session: &Session, plan: &Plan, d: &Deadline) -> Result<Option<Value>> {
    let value = read(
        session,
        program::SCOPE_READ,
        json!({"scopeKey":plan.scope()}),
        d,
    )?;
    let rows = rows(
        &value,
        json!([
            "scopeKey",
            "ownerKind",
            "schemaVersion",
            "activeRevision",
            "activeRevisionKey",
            "createdByPlan"
        ]),
    )?;
    ensure!(rows.len() <= 1, "neo4j_publish_scope_conflict");
    let Some(row) = rows.first() else {
        return Ok(None);
    };
    let revision = row[3]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("neo4j_publish_scope_conflict"))?;
    ensure!(
        row[0] == plan.scope()
            && row[1] == "yoke-native-neo4j-v1"
            && row[2] == 1
            && (revision == "absent" || crate::model::valid_id(revision))
            && row[4]
                == if revision == "absent" {
                    json!("absent")
                } else {
                    json!(program::revision_key(plan.scope(), revision))
                }
            && row[5].as_str().is_some_and(crate::model::valid_id),
        "neo4j_publish_scope_conflict"
    );
    Ok(Some(
        json!({"scopeKey":row[0],"activeRevision":row[3],"createdByPlan":row[5]}),
    ))
}
pub(super) fn expected_receipt(plan: &Plan, d: &Deadline) -> Result<Value> {
    if plan.mode() == Mode::Bootstrap {
        return Ok(
            json!({"scopeKey":plan.scope(),"schemaVersion":1,"createdByPlan":plan.id(),"activeRevision":"absent"}),
        );
    }
    let p = program::parameters(plan, d)?;
    Ok(
        json!({"scopeKey":plan.scope(),"revisionId":plan.revision(),"payloadHash":plan.payload().unwrap()["payloadHash"],"planId":plan.id(),"entityCount":p["entities"].as_array().unwrap().len(),"observationCount":p["observations"].as_array().unwrap().len(),"evidenceCount":p["evidence"].as_array().unwrap().len(),"unresolvedCount":p["unresolved"].as_array().unwrap().len()}),
    )
}
pub(super) fn confirm_response(
    plan: &Plan,
    op: super::Operation,
    value: &Value,
    d: &Deadline,
) -> Result<()> {
    if matches!(op, super::Operation::Namespace | super::Operation::Publish) {
        let rows = rows(value, json!(["receipt"]))?;
        ensure!(
            rows == [json!([expected_receipt(plan, d)?])],
            "neo4j_publish_receipt_invalid"
        );
    } else {
        let (fields, rows) = response_data(value)?;
        ensure!(
            fields.is_empty() && rows.is_empty(),
            "neo4j_publish_receipt_invalid"
        )
    }
    Ok(())
}
pub(super) fn publication_present(session: &Session, plan: &Plan, d: &Deadline) -> Result<bool> {
    let Some(scope) = scope(session, plan, d)? else {
        return Ok(false);
    };
    // A later publication may already be active. The original immutable
    // revision remains a positive historical postcondition, never a rollback.
    let _ = scope;
    let value = read(
        session,
        program::REVISION_READ,
        json!({"revisionKey":program::revision_key(plan.scope(),plan.revision().unwrap())}),
        d,
    )?;
    let rows = rows(
        &value,
        json!(["rowJson", "rowHash", "scopeKey", "revisionId"]),
    )?;
    let p = program::parameters(plan, d)?;
    Ok(rows
        == [json!([
            p["revision"]["props"]["rowJson"],
            p["revision"]["rowHash"],
            plan.scope(),
            plan.revision()
        ])])
}
