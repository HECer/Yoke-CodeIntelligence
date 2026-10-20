//! Fixed scalar projections retain the server's actual precision, never source authority.
use super::{
    Mode, Operation, Plan, program,
    transport::{Session, data, json_value},
};
use crate::{capture::Deadline, falkor_wire::Value as Wire};
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use std::collections::BTreeSet;
#[cfg(test)]
#[path = "falkor_publish_checks_tests.rs"]
mod tests;
pub(super) fn read(
    session: &mut Session,
    plan: &Plan,
    statement: &str,
    params: Value,
    d: &Deadline,
) -> Result<Wire> {
    session.send(
        &program::query(plan.graph_key(), statement, &params, true, d)?,
        d,
    )
}
fn rows(value: &Wire, fields: Value) -> Result<Vec<Value>> {
    let (actual, rows) = data(value)?;
    ensure!(
        json_value(&Wire::Array(Some(actual.to_vec())))? == fields,
        "falkor_publish_response_invalid"
    );
    rows.iter().map(json_value).collect()
}
pub(super) struct Schema {
    pub(super) value: Value,
    pub(super) confirmed: Vec<Operation>,
}
pub(super) fn schema(
    session: &mut Session,
    plan: &Plan,
    allow_missing: bool,
    d: &Deadline,
) -> Result<Schema> {
    let constraints = read(session, plan, program::CONSTRAINT_READ, json!({}), d)?;
    let indexes = read(session, plan, program::INDEX_READ, json!({}), d)?;
    schema_values(constraints, indexes, allow_missing)
}
fn schema_values(constraints: Wire, indexes: Wire, allow_missing: bool) -> Result<Schema> {
    let mut confirmed = Vec::new();
    let mut seen = BTreeSet::new();
    let mut constraint_count = 0;
    let mut all_operational = true;
    for row in rows(
        &constraints,
        json!([
            "type",
            "label",
            "propertyCount",
            "property0",
            "entitytype",
            "status"
        ]),
    )? {
        let op = match row[1].as_str() {
            Some("YokeNativeScope") => Operation::ScopeConstraint,
            Some("YokeNativeElement") => Operation::ElementConstraint,
            Some("YOKE_NATIVE_OBSERVATION") => Operation::ObservationConstraint,
            _ => anyhow::bail!("falkor_publish_schema_conflict"),
        };
        let entity = if op == Operation::ObservationConstraint {
            "RELATIONSHIP"
        } else {
            "NODE"
        };
        ensure!(
            row[0] == "UNIQUE"
                && row[2] == 1
                && row[3] == "key"
                && row[4] == entity
                && seen.insert(row[1].as_str().unwrap().to_owned()),
            "falkor_publish_schema_conflict"
        );
        ensure!(
            matches!(
                row[5].as_str(),
                Some("OPERATIONAL" | "PENDING" | "UNDER CONSTRUCTION")
            ),
            "falkor_publish_schema_conflict"
        );
        constraint_count += 1;
        if row[5] == "OPERATIONAL" {
            confirmed.push(op)
        } else {
            all_operational = false
        }
    }
    let mut seen = BTreeSet::new();
    let mut index_count = 0;
    for row in rows(
        &indexes,
        json!([
            "label",
            "property",
            "typeCount",
            "type0",
            "entitytype",
            "status"
        ]),
    )? {
        let op = match (row[0].as_str(), row[1].as_str()) {
            (Some("YokeNativeScope"), Some("key")) => Operation::ScopeIndex,
            (Some("YokeNativeElement"), Some("key")) => Operation::ElementIndex,
            (Some("YOKE_NATIVE_OBSERVATION"), Some("key")) => Operation::ObservationIndex,
            (Some("YokeNativeElement"), Some("revisionId")) => Operation::ElementRevisionIndex,
            (Some("YOKE_NATIVE_OBSERVATION"), Some("revisionId")) => {
                Operation::ObservationRevisionIndex
            }
            _ => anyhow::bail!("falkor_publish_schema_conflict"),
        };
        let entity = if matches!(
            op,
            Operation::ObservationIndex | Operation::ObservationRevisionIndex
        ) {
            "RELATIONSHIP"
        } else {
            "NODE"
        };
        ensure!(
            row[2] == 1
                && row[3] == "RANGE"
                && row[4] == entity
                && seen.insert((
                    row[0].as_str().unwrap().to_owned(),
                    row[1].as_str().unwrap().to_owned()
                )),
            "falkor_publish_schema_conflict"
        );
        ensure!(
            matches!(row[5].as_str(), Some("OPERATIONAL" | "UNDER CONSTRUCTION")),
            "falkor_publish_schema_conflict"
        );
        index_count += 1;
        if row[5] == "OPERATIONAL" {
            confirmed.push(op)
        } else {
            all_operational = false
        }
    }
    ensure!(
        allow_missing || constraint_count == 3 && index_count == 5,
        "falkor_publish_schema_required"
    );
    ensure!(
        allow_missing || all_operational,
        "falkor_publish_schema_not_operational"
    );
    Ok(Schema {
        value: json!({"constraintCount":constraint_count,"indexedPropertyCount":index_count,"allRequiredPresent":constraint_count==3&&index_count==5,"allObservedOperational":all_operational,"serverAttestation":"unverified"}),
        confirmed,
    })
}
pub(super) fn wait_operation(
    session: &mut Session,
    plan: &Plan,
    op: Operation,
    d: &Deadline,
) -> Result<Value> {
    loop {
        d.check()?;
        let state = schema(session, plan, true, d)?;
        if state.confirmed.contains(&op) {
            return Ok(state.value);
        }
        std::thread::sleep(d.remaining().min(std::time::Duration::from_millis(10)));
    }
}
pub(super) fn scope(session: &mut Session, plan: &Plan, d: &Deadline) -> Result<Option<Value>> {
    // A dedicated graph admits exactly one owned root, even if a foreign root has another key.
    let value = read(session, plan, program::SCOPE_READ, json!({}), d)?;
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
    ensure!(rows.len() <= 1, "falkor_publish_scope_conflict");
    let Some(row) = rows.first() else {
        return Ok(None);
    };
    let revision = row[3]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("falkor_publish_scope_conflict"))?;
    ensure!(
        row[0] == plan.scope()
            && row[1] == "yoke-native-falkor-v1"
            && row[2] == 1
            && (revision == "absent" || crate::model::valid_id(revision))
            && row[4]
                == if revision == "absent" {
                    json!("absent")
                } else {
                    json!(program::revision_key(plan.scope(), revision))
                }
            && row[5].as_str().is_some_and(crate::model::valid_id),
        "falkor_publish_scope_conflict"
    );
    Ok(Some(
        json!({"scopeKey":row[0],"activeRevision":row[3],"createdByPlan":row[5]}),
    ))
}
pub(super) fn confirm_response(
    plan: &Plan,
    op: Operation,
    value: &Wire,
    d: &Deadline,
) -> Result<Value> {
    if matches!(op, Operation::Initialize | Operation::Publish) {
        let (fields, expected) = if op == Operation::Initialize {
            (
                json!([
                    "scopeKey",
                    "schemaVersion",
                    "createdByPlan",
                    "activeRevision"
                ]),
                json!([plan.scope(), 1, plan.id(), "absent"]),
            )
        } else {
            let p = program::parameters(plan, d)?;
            (
                json!([
                    "scopeKey",
                    "revisionId",
                    "payloadHash",
                    "planId",
                    "entityCount",
                    "observationCount",
                    "evidenceCount",
                    "unresolvedCount"
                ]),
                json!([
                    plan.scope(),
                    plan.revision(),
                    plan.payload().unwrap()["payloadHash"],
                    plan.id(),
                    p["entities"].as_array().unwrap().len(),
                    p["observations"].as_array().unwrap().len(),
                    p["evidence"].as_array().unwrap().len(),
                    p["unresolved"].as_array().unwrap().len()
                ]),
            )
        };
        ensure!(
            rows(value, fields)? == [expected],
            "falkor_publish_receipt_invalid"
        );
    } else if matches!(
        op,
        Operation::ScopeConstraint
            | Operation::ElementConstraint
            | Operation::ObservationConstraint
    ) {
        ensure!(
            matches!(json_value(value)?.as_str(), Some("PENDING" | "OK")),
            "falkor_publish_receipt_invalid"
        );
    } else {
        let Wire::Array(Some(parts)) = value else {
            anyhow::bail!("falkor_publish_receipt_invalid")
        };
        ensure!(parts.len() == 1, "falkor_publish_receipt_invalid");
        let Wire::Array(Some(stats)) = &parts[0] else {
            anyhow::bail!("falkor_publish_receipt_invalid")
        };
        ensure!(
            !stats.is_empty() && stats.len() <= 64,
            "falkor_publish_receipt_invalid"
        );
        for stat in stats {
            super::transport::text(stat)?;
        }
    }
    json_value(value)
}
pub(super) fn publication_present(
    session: &mut Session,
    plan: &Plan,
    d: &Deadline,
) -> Result<bool> {
    ensure!(
        plan.mode() == Mode::Publish,
        "falkor_publish_program_invalid"
    );
    if scope(session, plan, d)?.is_none() {
        return Ok(false);
    }
    let value = read(
        session,
        plan,
        program::REVISION_READ,
        json!({"revisionKey":program::revision_key(plan.scope(),plan.revision().unwrap())}),
        d,
    )?;
    let observed = rows(
        &value,
        json!([
            "rowJson",
            "rowHash",
            "scopeKey",
            "revisionId",
            "elementCount",
            "observationCount"
        ]),
    )?;
    let p = program::parameters(plan, d)?;
    Ok(observed
        == [json!([
            p["revision"]["props"]["rowJson"],
            p["revision"]["rowHash"],
            plan.scope(),
            plan.revision(),
            p["entities"].as_array().unwrap().len()
                + p["evidence"].as_array().unwrap().len()
                + p["unresolved"].as_array().unwrap().len()
                + 1,
            p["observations"].as_array().unwrap().len()
        ])])
}
