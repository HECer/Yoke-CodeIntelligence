//! An accepted stop targets only the connection whose driver installed it.
use super::*;
use crate::session_dashboard::management::{Reply, Request, ShutdownScope, ShutdownSignal};

impl Session {
    pub(crate) fn dashboard_shutdown_granted(&self) -> bool {
        self.dashboard_shutdown.is_some()
    }
    /// JSON opts in to the feature but cannot install or choose a stop target.
    /// Library hosts without a built-in owning driver retain explicit refusal.
    pub(crate) fn bind_dashboard_shutdown(&mut self, scope: ShutdownScope) -> Result<()> {
        if !self
            .host
            .dashboard
            .as_ref()
            .and_then(|c| c.management.as_ref())
            .is_some_and(|c| c.shutdown)
        {
            return Ok(());
        }
        ensure!(
            self.dashboard.is_none() && self.dashboard_shutdown.is_none(),
            "Install owning shutdown driver before dashboard open"
        );
        self.dashboard_shutdown = Some(ShutdownSignal::new(scope));
        Ok(())
    }
    pub(crate) fn dashboard_shutdown_pending(&self) -> bool {
        self.dashboard_shutdown
            .as_ref()
            .is_some_and(|s| s.requested())
    }
    pub(crate) fn dashboard_shutdown_ready(&self) -> bool {
        self.dashboard_shutdown.as_ref().is_some_and(|s| s.ready())
    }
    pub(super) fn dashboard_shutdown_view(&self) -> Option<Value> {
        let signal = self.dashboard_shutdown.as_ref()?;
        Some(
            json!({"scope":signal.scope.name(),"pending":signal.requested(),"serviceShutdown":false,"requiresConfirmation":true}),
        )
    }
    pub(super) fn dashboard_shutdown_action(
        &mut self,
        request: &Request,
        d: &Deadline,
    ) -> Result<Reply> {
        let granted = self
            .host
            .dashboard
            .as_ref()
            .and_then(|c| c.management.as_ref())
            .is_some_and(|c| c.shutdown);
        if !granted
            || self.require_tool("ci_session", d).is_err()
            || self.dashboard_shutdown.is_none()
        {
            return Ok(Reply::fault(
                403,
                "owning_shutdown_not_granted_or_driver_unavailable",
                Some(&request.id),
                true,
            ));
        }
        let signal = self.dashboard_shutdown.as_ref().unwrap().clone();
        let activity = self
            .dashboard
            .as_ref()
            .and_then(|h| h.begin("dashboard.shutdown"));
        let completion = signal.request(d)?;
        let data = json!({"shutdownAccepted":true,"scope":signal.scope.name(),"serviceShutdown":false,"processExitConfirmed":false,"connectionCloseConfirmed":false});
        self.finish_dashboard_activity(activity, &Ok(data.clone()));
        Ok(Reply {
            status: 200,
            completion: Some(completion),
            value: json!({"schema":"native_dashboard_management_v1","id":request.id,"outcome":"completed",
            "metadataChanged":false,"fileChanged":false,"sourceWritten":false,"sessionStateSha256":crate::model::hash(&self.persisted),"revision":self.state.revision,"data":data}),
        })
    }
}
