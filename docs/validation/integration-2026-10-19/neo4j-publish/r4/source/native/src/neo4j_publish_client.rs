//! Explicit host operations. Read recovery never resends a write.
use super::{
    Mode, Operation, Permissions, Plan, checks,
    journal::{self, Journal},
    transport::Session,
};
use crate::capture::Deadline;
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use std::path::Path;
fn reviewed(plan: &Plan, expected_plan_id: &str, d: &Deadline) -> Result<()> {
    plan.validate(d)?;
    ensure!(
        expected_plan_id == plan.id(),
        "neo4j_publish_reviewed_plan_mismatch"
    );
    Ok(())
}
pub fn approve(
    plan: &Plan,
    expected_plan_id: &str,
    path: &Path,
    expires_at_ms: u64,
    permissions: &Permissions,
    d: &Deadline,
) -> Result<Value> {
    ensure!(
        permissions.allow_approval && permissions.allow_journal,
        "neo4j_publish_approval_and_journal_permissions_required"
    );
    reviewed(plan, expected_plan_id, d)?;
    let at = journal::now()?;
    ensure!(
        expires_at_ms > at && expires_at_ms - at <= 3_600_000,
        "neo4j_publish_approval_expiry_invalid"
    );
    let mut journal = Journal::open(path, plan, true, d)?;
    journal.approve(plan, expires_at_ms, d)?;
    Ok(journal.summary(plan))
}
pub fn inspect_server(plan: &Plan, permissions: &Permissions, d: &Deadline) -> Result<Value> {
    plan.validate(d)?;
    let session = Session::open(plan.target(), permissions, d)?;
    let schema = checks::schema(&session, true, d)?;
    let scope = checks::scope(&session, plan, d)?;
    Ok(
        json!({"kind":"native_neo4j_server_inspection_v1","planId":plan.id(),"schema":schema,"scope":scope,"writesRequested":0,"serverAttestation":"unverified","sourceAuthority":false,"complete":false,"modelCalls":0}),
    )
}
pub fn apply(
    plan: &Plan,
    expected_plan_id: &str,
    path: &Path,
    permissions: &Permissions,
    d: &Deadline,
) -> Result<Value> {
    ensure!(
        permissions.allow_write && permissions.allow_journal,
        "neo4j_publish_write_and_journal_permissions_required"
    );
    ensure!(
        plan.mode() != Mode::Bootstrap || permissions.allow_schema,
        "neo4j_publish_schema_permission_required"
    );
    reviewed(plan, expected_plan_id, d)?;
    let mut journal = Journal::open(path, plan, false, d)?;
    journal.writable()?;
    let operations = super::operations(plan.mode());
    if journal.completed() == operations.len() {
        return Ok(journal.summary(plan));
    }
    let session = Session::open(plan.target(), permissions, d)?;
    checks::schema(&session, plan.mode() == Mode::Bootstrap, d)?;
    let scope = checks::scope(&session, plan, d)?;
    if plan.mode() == Mode::Bootstrap {
        ensure!(scope.is_none(), "neo4j_publish_namespace_already_exists");
    } else {
        let scope = scope.ok_or_else(|| anyhow::anyhow!("neo4j_publish_namespace_required"))?;
        ensure!(
            scope["activeRevision"] == plan.expected_revision().unwrap()
                || scope["activeRevision"] == plan.revision().unwrap(),
            "neo4j_publish_stale_precondition"
        );
    }
    for operation in operations.into_iter().skip(journal.completed()) {
        d.check()?;
        journal.writable()?;
        let request = super::request(plan, operation, d)?;
        // This durable intent precedes send. Any transport/protocol/server error
        // leaves it pending; no automatic replay or local "failed" reset occurs.
        journal.intent(plan, operation, d)?;
        let response = session.query(&request, d)?;
        checks::confirm_response(plan, operation, &response, d)?;
        journal.confirmed(plan, operation, &response, false, &Deadline::new(1000))?;
    }
    let mut result = journal.summary(plan);
    result["allOperationsConfirmed"] = json!(true);
    result["serverAttestation"] = json!("unverified");
    result["modelCalls"] = json!(0);
    Ok(result)
}
pub fn recover(
    plan: &Plan,
    expected_plan_id: &str,
    path: &Path,
    permissions: &Permissions,
    d: &Deadline,
) -> Result<Value> {
    ensure!(
        permissions.allow_recovery && permissions.allow_journal,
        "neo4j_publish_recovery_and_journal_permissions_required"
    );
    reviewed(plan, expected_plan_id, d)?;
    let mut journal = Journal::open(path, plan, false, d)?;
    let Some(operation) = journal.pending() else {
        return Ok(journal.summary(plan));
    };
    let session = Session::open(plan.target(), permissions, d)?;
    let present = match operation {
        Operation::Publish => {
            checks::schema(&session, false, d)?;
            checks::publication_present(&session, plan, d)?
        }
        Operation::Namespace => checks::scope(&session, plan, d)?
            .is_some_and(|s| s["createdByPlan"] == plan.id() && s["activeRevision"] == "absent"),
        _ => {
            // An existing matching schema object establishes the requested
            // postcondition, without claiming which client created it.
            let schema = checks::schema(&session, true, d)?;
            let query = match operation {
                Operation::ScopeConstraint
                | Operation::ElementConstraint
                | Operation::ObservationConstraint => super::program::SCHEMA_READ,
                _ => super::program::INDEX_READ,
            };
            let response = checks::read(&session, query, json!({}), d)?;
            let (_, rows) = super::transport::response_data(&response)?;
            let name = match operation {
                Operation::ScopeConstraint => "yoke_native_scope_key_v1",
                Operation::ElementConstraint => "yoke_native_element_key_v1",
                Operation::ObservationConstraint => "yoke_native_observation_key_v1",
                Operation::ElementRevisionIndex => "yoke_native_element_revision_v1",
                Operation::ObservationRevisionIndex => "yoke_native_observation_revision_v1",
                _ => unreachable!(),
            };
            let _ = schema;
            rows.iter().any(|r| r[0] == name)
        }
    };
    ensure!(
        present,
        "neo4j_publish_result_still_unknown_no_write_replayed"
    );
    let proof = json!({"operation":operation,"matchingServerPostconditionObserved":true,"serverAttestation":"unverified"});
    journal.confirmed(plan, operation, &proof, true, &Deadline::new(1000))?;
    let mut result = journal.summary(plan);
    result["recoveryWritesRequested"] = json!(0);
    Ok(result)
}
