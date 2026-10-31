//! Reset only the owning instance's ephemeral totals under a separate host grant.
use super::*;
use crate::session_dashboard::management::{Reply, Request};

impl Session {
    pub(super) fn dashboard_tool_usage_reset(
        &mut self,
        request: &Request,
        expected_revision: &str,
        deadline: &Deadline,
    ) -> Result<Reply> {
        let granted = self
            .host
            .dashboard
            .as_ref()
            .and_then(|config| config.management.as_ref())
            .is_some_and(|config| config.tool_usage_reset);
        if !granted || self.require_tool("ci_session", deadline).is_err() {
            return Ok(Reply::fault(
                403,
                "tool_usage_reset_not_granted",
                Some(&request.id),
                true,
            ));
        }
        if self.tool_usage.is_none() {
            return Ok(Reply::fault(
                409,
                "tool_usage_statistics_unavailable",
                Some(&request.id),
                true,
            ));
        }
        // The Session stays on its owning thread between this comparison and invoke.
        if expected_revision != self.tool_usage_revision.to_string() {
            return Ok(Reply::fault(
                409,
                "tool_usage_statistics_changed_refresh",
                Some(&request.id),
                true,
            ));
        }
        let data = self.invoke("clearToolUsage", &json!({}), deadline)?;
        self.check_tool_state(deadline)?;
        crate::session_dashboard::check_binding(
            &self.engine.root,
            &self.engine.store.workspace_id,
            &self.state.id,
            &self.persisted,
            deadline,
        )
        .map_err(|_| anyhow::anyhow!("Dashboard binding changed after tool statistics reset"))?;
        Ok(Reply {
            status: 200,
            completion: None,
            value: json!({"schema":"native_dashboard_management_v1","id":request.id,"outcome":"completed",
                "metadataChanged":false,"fileChanged":false,"sourceWritten":false,"toolUsageChanged":true,
                "sessionStateSha256":crate::model::hash(&self.persisted),"revision":self.state.revision,"data":data}),
        })
    }
}
