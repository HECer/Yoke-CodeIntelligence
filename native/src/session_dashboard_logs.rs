//! Private log payloads are read only after admission by the original Session.
use super::*;
use crate::{
    host_install::{documents::ProjectRoot, logs::Access},
    session_dashboard::management::{Reply, Request},
};

pub(super) struct Runtime {
    access: Access,
    maximum: usize,
}
impl Runtime {
    pub(super) fn capture(
        host: &HostProfile,
        engine: &Engine,
        d: &Deadline,
    ) -> Result<Option<Self>> {
        let Some(config) = host
            .dashboard
            .as_ref()
            .and_then(|c| c.management.as_ref())
            .and_then(|c| c.raw_logs.as_ref())
        else {
            return Ok(None);
        };
        let mut roots = vec![ProjectRoot {
            name: "default".into(),
            path: engine.root.clone(),
        }];
        roots.extend(host.projects.iter().map(|p| ProjectRoot {
            name: p.name.clone(),
            path: p.root.clone(),
        }));
        Ok(Some(Self {
            access: Access::open(config, &roots, d)?,
            maximum: config.max_window_bytes,
        }))
    }
    pub(super) fn binding(&self) -> &str {
        self.access.binding()
    }
}
impl Session {
    pub(crate) fn dashboard_logs_granted(&self) -> bool {
        self.dashboard_logs.is_some()
    }
    pub(super) fn dashboard_logs_view(&self) -> Option<Value> {
        let runtime = self.dashboard_logs.as_ref()?;
        Some(
            json!({"choices":runtime.access.selections().map(|l| json!({"id":l.id,"label":l.label})).collect::<Vec<_>>(),
            "maxWindowBytes":runtime.maximum,"automaticCapture":false,"automaticPolling":false,"immutableSnapshot":false,"writes":false}),
        )
    }
    pub(super) fn dashboard_log_action(
        &mut self,
        request: &Request,
        id: &str,
        maximum: Option<usize>,
        d: &Deadline,
    ) -> Result<Reply> {
        if self.require_tool("ci_session", d).is_err() || self.dashboard_logs.is_none() {
            return Ok(Reply::fault(
                403,
                "current_policy_denied",
                Some(&request.id),
                true,
            ));
        }
        let activity = self
            .dashboard
            .as_ref()
            .and_then(|h| h.begin("dashboard.logs.read"));
        let result = self
            .dashboard_logs
            .as_ref()
            .context("Dashboard log selection unavailable")?
            .access
            .read(id, maximum, d)
            .and_then(|o| Ok(json!({"observation":serde_json::to_value(o)?})));
        self.finish_dashboard_activity(activity, &result);
        let data = match result {
            Ok(value) => value,
            Err(_) => {
                return Ok(Reply::fault(
                    422,
                    "selected_log_unavailable_or_budget_denied",
                    Some(&request.id),
                    true,
                ));
            }
        };
        if self.check_tool_state(d).is_err()
            || crate::session_dashboard::check_binding(
                &self.engine.root,
                &self.engine.store.workspace_id,
                &self.state.id,
                &self.persisted,
                d,
            )
            .is_err()
        {
            return Ok(Reply::fault(
                409,
                "session_changed_after_operation_refresh",
                Some(&request.id),
                true,
            ));
        }
        Ok(Reply {
            status: 200,
            completion: None,
            value: json!({"schema":"native_dashboard_management_v1","id":request.id,"outcome":"completed",
            "metadataChanged":false,"fileChanged":false,"sourceWritten":false,"sessionStateSha256":crate::model::hash(&self.persisted),"revision":self.state.revision,"data":data}),
        })
    }
}
