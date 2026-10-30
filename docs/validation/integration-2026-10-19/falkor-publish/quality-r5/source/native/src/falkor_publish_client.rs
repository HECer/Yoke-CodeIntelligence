//! Host-reviewed writes and positive read-only recovery; uncertain effects are never resent.
use super::{
    Mode, Operation, Permissions, Plan, checks,
    journal::{self, Journal},
    transport::Session,
};
use crate::capture::Deadline;
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use std::path::Path;
fn reviewed(plan: &Plan, id: &str, d: &Deadline) -> Result<()> {
    plan.validate(d)?;
    ensure!(plan.id() == id, "falkor_publish_reviewed_plan_mismatch");
    // Admit every complete frame before approval, journal creation, or any connection.
    for op in super::operations(plan.mode()) {
        super::command(plan, op, d)?;
    }
    Ok(())
}
pub fn inspect_journal(plan: &Plan, path: &Path, p: &Permissions, d: &Deadline) -> Result<Value> {
    ensure!(
        p.allow_journal,
        "falkor_publish_journal_permission_required"
    );
    Ok(Journal::open(path, plan, false, d)?.summary(plan))
}
pub fn approve(
    plan: &Plan,
    id: &str,
    path: &Path,
    expires_at_ms: u64,
    p: &Permissions,
    d: &Deadline,
) -> Result<Value> {
    ensure!(
        p.allow_approval && p.allow_journal,
        "falkor_publish_approval_and_journal_permissions_required"
    );
    reviewed(plan, id, d)?;
    let at = journal::now()?;
    ensure!(
        expires_at_ms > at && expires_at_ms - at <= 3_600_000,
        "falkor_publish_approval_expiry_invalid"
    );
    let mut journal = Journal::open(path, plan, true, d)?;
    journal.approve(plan, expires_at_ms, d)?;
    Ok(journal.summary(plan))
}
pub fn inspect_server(plan: &Plan, p: &Permissions, d: &Deadline) -> Result<Value> {
    plan.validate(d)?;
    let mut session = Session::open(plan.target(), p, d)?;
    let exists = session.exists(plan.graph_key(), d)?;
    let (scope, schema) = if exists {
        (
            checks::scope(&mut session, plan, d)?,
            checks::schema(&mut session, plan, true, d)?.value,
        )
    } else {
        (None, Value::Null)
    };
    Ok(
        json!({"kind":"native_falkor_server_inspection_v1","planId":plan.id(),"graphExists":exists,"scope":scope,"schema":schema,"writesRequested":0,"serverAttestation":"unverified","sourceAuthority":false,"complete":false,"modelCalls":0}),
    )
}
pub fn apply(plan: &Plan, id: &str, path: &Path, p: &Permissions, d: &Deadline) -> Result<Value> {
    ensure!(
        p.allow_write && p.allow_journal,
        "falkor_publish_write_and_journal_permissions_required"
    );
    ensure!(
        plan.mode() != Mode::Bootstrap || p.allow_schema,
        "falkor_publish_schema_permission_required"
    );
    reviewed(plan, id, d)?;
    let mut journal = Journal::open(path, plan, false, d)?;
    journal.writable()?;
    let operations = super::operations(plan.mode());
    if journal.completed() == operations.len() {
        return Ok(journal.summary(plan));
    }
    let mut session = Session::open(plan.target(), p, d)?;
    let exists = session.exists(plan.graph_key(), d)?;
    if plan.mode() == Mode::Bootstrap && journal.completed() == 0 {
        ensure!(!exists, "falkor_publish_namespace_already_exists");
    } else {
        ensure!(exists, "falkor_publish_namespace_required");
        let scope = checks::scope(&mut session, plan, d)?
            .ok_or_else(|| anyhow::anyhow!("falkor_publish_namespace_required"))?;
        if plan.mode() == Mode::Bootstrap {
            ensure!(
                scope["createdByPlan"] == plan.id(),
                "falkor_publish_scope_conflict"
            );
        } else {
            ensure!(
                scope["activeRevision"] == plan.expected_revision().unwrap()
                    || scope["activeRevision"] == plan.revision().unwrap(),
                "falkor_publish_stale_precondition"
            );
        }
        checks::schema(&mut session, plan, plan.mode() == Mode::Bootstrap, d)?;
    }
    for op in operations.into_iter().skip(journal.completed()) {
        d.check()?;
        journal.writable()?;
        let command = super::command(plan, op, d)?;
        // Fresh graph admission queues exactly one initialization under WATCH.
        // Its durable local intent is flushed before EXEC can create the graph.
        if op == Operation::Initialize {
            session.queue_initialize(plan.graph_key(), &command, d)?;
        }
        journal.intent(plan, op, d)?;
        let response = if op == Operation::Initialize {
            session.execute_initialize(d)?
        } else {
            session.send(&command, d)?
        };
        let receipt = checks::confirm_response(plan, op, &response, d)?;
        let postcondition = if matches!(op, Operation::Initialize | Operation::Publish) {
            Value::Null
        } else {
            checks::wait_operation(&mut session, plan, op, d)?
        };
        journal.confirmed(
            plan,
            op,
            &json!({"response":receipt,"postcondition":postcondition}),
            false,
            &Deadline::new(1000),
        )?;
    }
    let mut result = journal.summary(plan);
    result["allOperationsConfirmed"] = json!(true);
    result["serverAttestation"] = json!("unverified");
    result["modelCalls"] = json!(0);
    Ok(result)
}
pub fn recover(plan: &Plan, id: &str, path: &Path, p: &Permissions, d: &Deadline) -> Result<Value> {
    ensure!(
        p.allow_recovery && p.allow_journal,
        "falkor_publish_recovery_and_journal_permissions_required"
    );
    reviewed(plan, id, d)?;
    let mut journal = Journal::open(path, plan, false, d)?;
    let Some(op) = journal.pending() else {
        return Ok(journal.summary(plan));
    };
    let mut session = Session::open(plan.target(), p, d)?;
    let exists = session.exists(plan.graph_key(), d)?;
    let present = if !exists {
        false
    } else {
        match op {
            Operation::Publish => {
                checks::schema(&mut session, plan, false, d)?;
                checks::publication_present(&mut session, plan, d)?
            }
            Operation::Initialize => checks::scope(&mut session, plan, d)?
                .is_some_and(|s| s["createdByPlan"] == plan.id()),
            _ => {
                let owned = checks::scope(&mut session, plan, d)?
                    .is_some_and(|s| s["createdByPlan"] == plan.id());
                owned
                    && checks::schema(&mut session, plan, true, d)?
                        .confirmed
                        .contains(&op)
            }
        }
    };
    ensure!(
        present,
        "falkor_publish_outcome_unconfirmed_no_automatic_resend"
    );
    journal.confirmed(plan,op,&json!({"fixedReadOnlyPostconditionObserved":true,"operation":op,"serverAttestation":"unverified"}),true,&Deadline::new(1000))?;
    let mut result = journal.summary(plan);
    result["writesRequested"] = json!(0);
    Ok(result)
}
