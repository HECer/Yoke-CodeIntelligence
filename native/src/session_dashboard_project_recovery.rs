//! Only the original Session owner admits bounded, selection-bound recovery.
use super::*;
use crate::session_dashboard::management::{DocumentRequest, Request};

impl Session {
    pub(super) fn project_document_recovery(
        &mut self,
        _request: &Request,
        action: &DocumentRequest,
        d: &Deadline,
        attempt: &mut Option<Value>,
    ) -> Result<Value> {
        let binding = self.project_document_binding(action.document_id(), d)?;
        if matches!(action, DocumentRequest::Recover { .. }) {
            self.require_tool("ci_change_apply", d)?;
        }
        let engine = self.active_engine_at(d)?;
        ensure!(
            engine.allow_writes,
            "Current Engine workspace write permission required"
        );
        engine.sync_source_exclusions(d)?;
        let bound = engine.store.inherited_deadline(d)?;
        let prior = engine.store.replace_deadline(Some(bound.clone()));
        let result = (|| -> Result<Value> {
            match action {
                DocumentRequest::Pending { cursor, .. } => crate::changes::document_journal_page(
                    &engine.store,
                    &binding,
                    cursor.as_deref(),
                    &bound,
                ),
                DocumentRequest::Recover {
                    journal_id,
                    expected_event,
                    direction,
                    ..
                } => {
                    ensure!(
                        direction == "rollback",
                        "Workspace dashboard recovery supports only rollback"
                    );
                    let review = crate::changes::review_document_journal(
                        &engine.store,
                        journal_id,
                        &binding,
                        &bound,
                    )?;
                    ensure!(
                        review.event_id() == expected_event,
                        "Workspace journal review changed"
                    );
                    let mut started = false;
                    let recovered = crate::changes::recover_document(
                        &engine.store,
                        &engine.root,
                        &review,
                        engine.allow_writes,
                        &bound,
                        &mut started,
                    );
                    if started {
                        *attempt = Some(json!({"workspacePlanId":review.plan().id,
                            "possibleJournalId":journal_id,"journalCreationConfirmed":true,
                            "workflow":"inspect_dashboard_workspace_recovery_or_native_host"}));
                    }
                    let (receipt, source_written) = recovered?;
                    Ok(json!({"receipt":receipt,"workspacePlanId":review.plan().id,
                        "workspaceRecovery":"reviewed_dashboard_rollback",
                        "fileChanged":source_written,"sourceWritten":source_written,"metadataChanged":true}))
                }
                _ => anyhow::bail!("Unsupported workspace recovery operation"),
            }
        })();
        engine.store.replace_deadline(prior);
        result
    }
}
