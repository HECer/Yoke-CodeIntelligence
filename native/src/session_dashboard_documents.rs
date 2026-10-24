//! The Session owner admits exact host-selected documents before reading payloads.
use super::*;
use crate::host_install::documents::{
    Access, Plan, ProjectRoot, RecoveryDirection, Scope, WriteAuthority,
};
use crate::session_dashboard::management::{DocumentRequest, Reply, Request};
use std::time::{Duration, Instant};

struct Prepared {
    plan: Plan,
    project: String,
    session_state: String,
    expires: Instant,
}
pub(super) struct Runtime {
    pub(super) access: Access,
    plans: BTreeMap<String, Prepared>,
    pub(super) workspace_plans: BTreeMap<String, super::dashboard_project_documents::Prepared>,
    pub(super) maximum: usize,
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
            .and_then(|c| c.documents.as_ref())
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
            plans: BTreeMap::new(),
            workspace_plans: BTreeMap::new(),
            maximum: config.max_document_bytes,
        }))
    }
    pub(super) fn binding(&self) -> &str {
        self.access.binding()
    }
    pub(super) fn clear_plans(&mut self) {
        self.plans.clear();
        self.workspace_plans.clear();
    }
    pub(super) fn prune_plans(&mut self) {
        let now = Instant::now();
        self.plans.retain(|_, p| p.expires > now);
        self.workspace_plans.retain(|_, p| p.expires > now);
    }
    pub(super) fn proposal_count(&self) -> usize {
        self.plans.len() + self.workspace_plans.len()
    }
    fn permitted(&self, id: &str, project: &str) -> Result<()> {
        let selection = self.access.selection(id)?;
        ensure!(
            matches!(&selection.scope, Scope::Global {})
                || matches!(&selection.scope,Scope::Project {project:selected} if selected==project),
            "Dashboard document project is not active"
        );
        Ok(())
    }
    fn writable(&self, id: &str) -> Result<()> {
        let selection = self.access.selection(id)?;
        ensure!(
            selection.writable
                && matches!(selection.scope, Scope::Global {})
                && self.access.journal_binding().is_some(),
            "Global host document write selection and journal required; project files require workspace approval"
        );
        Ok(())
    }
    fn execute(
        &mut self,
        envelope: &Request,
        action: &DocumentRequest,
        d: &Deadline,
    ) -> Result<Value> {
        let id = action.document_id();
        self.permitted(id, &envelope.project)?;
        self.prune_plans();
        match action {
            DocumentRequest::Read { .. } => Ok(json!({"document":self.access.read(id,d)?})),
            DocumentRequest::Plan {
                expected_revision,
                content,
                ..
            } => {
                self.writable(id)?;
                let plan = self.access.plan(id, expected_revision, content, d)?;
                self.plans.retain(|_, p| p.plan.document_id != id);
                ensure!(
                    self.proposal_count() < 16,
                    "Dashboard reviewed proposal capacity exceeded"
                );
                let value = json!({"plan":plan,"expiresAfterMs":600000,"fileChanged":false,"metadataChanged":false});
                self.plans.insert(
                    plan.id.clone(),
                    Prepared {
                        plan,
                        project: envelope.project.clone(),
                        session_state: envelope.session_state_sha256.clone(),
                        expires: Instant::now() + Duration::from_secs(600),
                    },
                );
                Ok(value)
            }
            DocumentRequest::Apply { plan_id, .. } => {
                self.writable(id)?;
                let prepared = self
                    .plans
                    .get(plan_id)
                    .context("Dashboard reviewed proposal unavailable")?;
                ensure!(
                    prepared.plan.document_id == id
                        && prepared.project == envelope.project
                        && prepared.session_state == envelope.session_state_sha256,
                    "Dashboard reviewed proposal Session changed"
                );
                let authority = WriteAuthority::new(&self.access)?;
                let approval = authority.approve(&self.access, &prepared.plan, d)?;
                let receipt = self
                    .access
                    .apply(&authority, &approval, &prepared.plan, d)?;
                self.plans.remove(plan_id);
                Ok(
                    json!({"receipt":receipt,"fileChanged":receipt.file_changed,"metadataChanged":receipt.metadata_changed}),
                )
            }
            DocumentRequest::Inspect { .. } => {
                anyhow::bail!("Workspace approval inspection requires a project document")
            }
            DocumentRequest::Pending { cursor, .. } => {
                ensure!(
                    cursor.is_none(),
                    "Global document recovery does not use workspace cursors"
                );
                self.writable(id)?;
                if self.access.history_archive_blocked(id, d)? {
                    return Ok(json!({"journals":[],"blockedByHistoryArchive":true,
                        "complete":false,"fileChanged":false,"metadataChanged":false,"sourceWritten":false}));
                }
                if self.access.binding_migration_blocked(id, d)? {
                    return Ok(json!({"journals":[],"blockedByBindingMigration":true,
                        "complete":false,"fileChanged":false,"metadataChanged":false,"sourceWritten":false}));
                }
                let owner = self.access.owner_binding(id, d)?;
                if owner
                    .as_deref()
                    .is_some_and(|binding| binding != self.access.binding())
                {
                    return Ok(
                        json!({"journals":[],"blockedByDifferentAuthority":true,"originalAuthority":owner}),
                    );
                }
                Ok(json!({"journals":self.access.pending_journals(id,d)?}))
            }
            DocumentRequest::Recover {
                journal_id,
                expected_event,
                direction,
                ..
            } => {
                self.writable(id)?;
                let journal = self.access.inspect_journal(journal_id, d)?;
                ensure!(
                    journal.plan.document_id == id,
                    "Dashboard journal document differs"
                );
                let direction = match direction.as_str() {
                    "resume" => RecoveryDirection::Resume,
                    "rollback" => RecoveryDirection::Rollback,
                    _ => anyhow::bail!("Invalid recovery direction"),
                };
                let authority = WriteAuthority::new(&self.access)?;
                let approval = authority.approve_recovery_at(
                    &self.access,
                    journal_id,
                    expected_event,
                    direction,
                    d,
                )?;
                let receipt = self.access.recover(&authority, &approval, d)?;
                Ok(
                    json!({"receipt":receipt,"fileChanged":receipt.file_changed,"metadataChanged":receipt.metadata_changed}),
                )
            }
        }
    }
}
impl Session {
    pub(crate) fn dashboard_documents_granted(&self) -> bool {
        self.dashboard_documents.is_some()
    }
    pub(super) fn dashboard_documents_view(&self) -> Option<Value> {
        let documents = self.dashboard_documents.as_ref()?;
        let project_writes = self.host_allow_writes && self.require_edit_permission().is_ok();
        let choices=documents.access.selections().filter(|s|matches!(&s.scope,Scope::Global {}) || matches!(&s.scope,Scope::Project {project} if *project==self.state.active_project)).map(|s|json!({"id":s.id,"label":s.label,"scope":s.scope,"writable":s.writable && if matches!(s.scope,Scope::Global {}) { documents.access.journal_binding().is_some() } else { project_writes },"projectWriteApprovalRequired":matches!(s.scope,Scope::Project {..})})).collect::<Vec<_>>();
        let project_writes = choices
            .iter()
            .any(|c| c["scope"]["kind"] == "project" && c["writable"] == true);
        Some(
            json!({"choices":choices,"maxDocumentBytes":documents.maximum,"appliesTo":"selected_file_bytes_only","reloadsRunningConfiguration":false,"projectWrites":project_writes,"sourceWritten":false}),
        )
    }
    pub(super) fn dashboard_document_action(
        &mut self,
        request: &Request,
        action: &DocumentRequest,
        d: &Deadline,
    ) -> Result<Reply> {
        let denied = self.require_tool("ci_session", d).is_err()
            || self.dashboard_documents.as_ref().is_none_or(|runtime| {
                runtime
                    .permitted(action.document_id(), &request.project)
                    .is_err()
            });
        if denied {
            return Ok(Reply::fault(
                403,
                "current_policy_denied",
                Some(&request.id),
                true,
            ));
        }
        let project = self.dashboard_documents.as_ref().is_some_and(|runtime| {
            runtime
                .access
                .selection(action.document_id())
                .is_ok_and(|s| matches!(s.scope, Scope::Project { .. }))
        });
        if project && !matches!(action, DocumentRequest::Read { .. }) {
            return self.dashboard_project_document_action(request, action, d);
        }
        let operation = match action {
            DocumentRequest::Read { .. } => "dashboard.documents.read",
            DocumentRequest::Plan { .. } => "dashboard.documents.plan",
            DocumentRequest::Apply { .. } => "dashboard.documents.apply",
            DocumentRequest::Inspect { .. } => "dashboard.documents.inspect",
            DocumentRequest::Pending { .. } => "dashboard.documents.pending",
            DocumentRequest::Recover { .. } => "dashboard.documents.recover",
        };
        let write = matches!(
            action,
            DocumentRequest::Apply { .. } | DocumentRequest::Recover { .. }
        );
        let activity = self
            .dashboard
            .as_ref()
            .and_then(|handle| handle.begin(operation));
        let result = self
            .dashboard_documents
            .as_mut()
            .context("Dashboard document authority unavailable")?
            .execute(request, action, d);
        self.finish_dashboard_activity(activity, &result);
        let data = match result {
            Ok(value) => value,
            Err(_) => {
                return Ok(Reply::fault(
                    422,
                    if write {
                        "document_outcome_unconfirmed_inspect_recovery"
                    } else {
                        "document_unavailable_or_stale"
                    },
                    Some(&request.id),
                    !write,
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
                !write,
            ));
        }
        Ok(Reply {
            status: 200,
            completion: None,
            value: json!({"schema":"native_dashboard_management_v1","id":request.id,"outcome":"completed","metadataChanged":data["metadataChanged"]==true,"fileChanged":data["fileChanged"]==true,"sourceWritten":false,"sessionStateSha256":crate::model::hash(&self.persisted),"revision":self.state.revision,"data":data}),
        })
    }
}
