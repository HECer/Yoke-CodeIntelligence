//! Only the host thread owning this non-Send Session executes management work.
use super::*;
use crate::session_dashboard::management::{Action, Mailbox, Reply, Request};

impl Session {
    pub(crate) fn close_dashboard_management(&mut self) {
        // Stop the HTTP worker before dropping its receiving mailbox. Pending
        // requests observe closure, and cannot be applied by a later driver.
        self.dashboard = None;
        self.dashboard_management = None;
        self.dashboard_shutdown = None;
        if let Some(documents) = &mut self.dashboard_documents {
            documents.clear_plans();
        }
    }
    pub fn dashboard_management_requested(&self) -> bool {
        self.host
            .dashboard
            .as_ref()
            .is_some_and(|c| c.management.is_some())
    }
    /// The host must regularly poll on this same owning thread, including while
    /// its protocol input is idle. JSON alone cannot install that driver.
    pub fn enable_dashboard_management(&mut self, deadline: &Deadline) -> Result<()> {
        self.check_tool_state(deadline)?;
        ensure!(
            self.dashboard.is_none(),
            "Install dashboard management before opening its listener"
        );
        ensure!(
            self.dashboard_management_requested(),
            "Dashboard management host opt-in required"
        );
        if self.dashboard_management.is_none() {
            self.dashboard_management = Some(Mailbox::new());
        }
        Ok(())
    }
    pub(super) fn dashboard_management_view(&self) -> Option<Value> {
        let config = self.host.dashboard.as_ref()?.management.as_ref()?;
        let mut choices = MODES.iter().map(|s| s.to_string()).collect::<BTreeSet<_>>();
        if let Some(steering) = &self.host.steering {
            choices.extend(steering.modes.keys().cloned());
        }
        Some(
            json!({"enabled":self.dashboard_management.is_some(),"sessionControls":config.session_controls,
            "memories":config.memories,"configurationFiles":self.dashboard_documents_view().unwrap_or(json!(false)),"languageSelection":self.dashboard_language_view().unwrap_or(json!(false)),"rawLogs":self.dashboard_logs_view().unwrap_or(json!(false)),"shutdown":self.dashboard_shutdown_view().unwrap_or(json!(false)),
            "operationTimeoutMs":config.operation_timeout_ms,"sessionStateSha256":crate::model::hash(&self.persisted),
            "modeChoices":choices,"modeResetAvailable":self.steering_enabled(),"memoryWritesGranted":self.host.notes_policy.writable,
            "noMemoriesMode":self.state.modes.iter().any(|m|m=="no-memories")}),
        )
    }
    /// Returns whether the actual selected tool catalog changed. At most four
    /// tickets are admitted per poll; their original deadlines never restart.
    pub fn poll_dashboard_management(&mut self, deadline: &Deadline) -> Result<bool> {
        let mut changed = false;
        for _ in 0..4 {
            let Some(pending) = self.dashboard_management.as_ref().and_then(|m| m.next()) else {
                break;
            };
            if !pending.begin() {
                let id = pending.request.id.clone();
                pending.finish(Reply::fault(
                    408,
                    "expired_before_execution",
                    Some(&id),
                    true,
                ));
                continue;
            }
            let id = pending.request.id.clone();
            let before = crate::protocol::session_tools(self, self.prompt_profile);
            let result = (|| -> Result<Reply> {
                let bound = deadline.intersection(&pending.deadline)?;
                let bound = if let Some(closed) = &pending.transport_closed {
                    Deadline::run_bounded(&bound, closed.clone(), || Ok(Deadline::new(600_000)))?
                } else {
                    bound
                };
                Deadline::run_bounded(&bound, pending.cancel.clone(), || {
                    Deadline::run_bounded(&bound, pending.access_closed.clone(), || {
                        let current = Deadline::new(600_000);
                        self.dashboard_action(&pending.request, &current)
                    })
                })
            })();
            changed |= before != crate::protocol::session_tools(self, self.prompt_profile);
            let reply = result.unwrap_or_else(|_| {
                Reply::fault(
                    408,
                    "outcome_unconfirmed_refresh_before_retry",
                    Some(&id),
                    false,
                )
            });
            pending.finish(reply);
        }
        Ok(changed)
    }
    fn dashboard_action(&mut self, request: &Request, deadline: &Deadline) -> Result<Reply> {
        deadline.check()?;
        request.validate()?;
        if request.session_state_sha256 != crate::model::hash(&self.persisted)
            || request.project != self.state.active_project
            || self.check_tool_state(deadline).is_err()
            || crate::session_dashboard::check_binding(
                &self.engine.root,
                &self.engine.store.workspace_id,
                &self.state.id,
                &self.persisted,
                deadline,
            )
            .is_err()
        {
            return Ok(Reply::fault(
                409,
                "session_changed_refresh",
                Some(&request.id),
                true,
            ));
        }
        let config = self
            .host
            .dashboard
            .as_ref()
            .and_then(|c| c.management.as_ref())
            .context("Dashboard management unavailable")?
            .clone();
        if self.dashboard_shutdown_pending() {
            return Ok(Reply::fault(
                409,
                "owning_session_closing",
                Some(&request.id),
                true,
            ));
        }
        let (operation, args, tool, api, write) = match &request.action {
            Action::Shutdown {} => return self.dashboard_shutdown_action(request, deadline),
            Action::ReadLog { log_id, max_bytes } => {
                return self.dashboard_log_action(request, log_id, *max_bytes, deadline);
            }
            Action::Document { request: document } => {
                return self.dashboard_document_action(request, document, deadline);
            }
            Action::ListMemories { topic } => (
                "notes.list",
                json!({"topic":topic}),
                "list_memories",
                Some("mem.list_memories"),
                false,
            ),
            Action::ReadMemory { name } => (
                "notes.read",
                json!({"name":name}),
                "read_memory",
                Some("mem.read_memory"),
                false,
            ),
            Action::CreateMemory { name, content } => (
                "notes.write",
                json!({"name":name,"content":content,"expectedAbsent":true}),
                "write_memory",
                Some("mem.write_memory"),
                true,
            ),
            Action::WriteMemory {
                name,
                content,
                expected_revision,
            } => (
                "notes.write",
                json!({"name":name,"content":content,"expectedRevision":expected_revision}),
                "write_memory",
                Some("mem.write_memory"),
                true,
            ),
            Action::DeleteMemory {
                name,
                expected_revision,
            } => (
                "notes.delete",
                json!({"name":name,"expectedRevision":expected_revision}),
                "delete_memory",
                Some("mem.delete_memory"),
                true,
            ),
            Action::RenameMemory {
                old_name,
                new_name,
                expected_revision,
            } => (
                "notes.rename",
                json!({"oldName":old_name,"newName":new_name,"expectedRevision":expected_revision}),
                "rename_memory",
                Some("mem.rename_memory"),
                true,
            ),
            Action::SwitchModes { modes } => (
                "switchModes",
                json!({"modes":modes}),
                "ci_session",
                None,
                true,
            ),
            Action::ResetModes {} => (
                "switchModes",
                json!({"reset":true}),
                "ci_session",
                None,
                true,
            ),
            Action::ActivateProject { project } => (
                "activate",
                json!({"project":project}),
                "activate_project",
                None,
                true,
            ),
            Action::SelectLanguageServers { selection } => (
                "dashboard.languages",
                serde_json::to_value(selection)?,
                "ci_session",
                None,
                true,
            ),
            Action::ResetLanguageServers {} => (
                "dashboard.languages.reset",
                Value::Null,
                "ci_session",
                None,
                true,
            ),
        };
        let memory = operation.starts_with("notes.");
        if (memory && (!config.memories || (write && !self.host.notes_policy.writable)))
            || (!memory && !config.session_controls)
            || (operation.starts_with("dashboard.languages")
                && config
                    .language_servers
                    .as_ref()
                    .is_none_or(|grant| match &request.action {
                        Action::SelectLanguageServers { selection } => {
                            grant.permits(selection).is_err()
                        }
                        _ => false,
                    }))
            || self.require_tool(tool, deadline).is_err()
            || api.is_some_and(|api| self.require_repl_api(api).is_err())
        {
            return Ok(Reply::fault(
                403,
                "current_policy_denied",
                Some(&request.id),
                true,
            ));
        }
        let activity = if operation.starts_with("dashboard.languages") {
            self.dashboard
                .as_ref()
                .and_then(|handle| handle.begin(operation))
        } else {
            None
        };
        let result = match &request.action {
            Action::SelectLanguageServers { selection } => {
                self.dashboard_select_languages(Some(selection.clone()), deadline)
            }
            Action::ResetLanguageServers {} => self.dashboard_select_languages(None, deadline),
            _ => self.invoke(operation, &args, deadline),
        };
        if operation.starts_with("dashboard.languages") {
            self.finish_dashboard_activity(activity, &result);
        }
        let reply = match result {
            Ok(mut data) => {
                let metadata_changed = if operation.starts_with("dashboard.languages") {
                    data["changed"] == true
                } else {
                    write
                };
                if self.check_tool_state(deadline).is_err()
                    || crate::session_dashboard::check_binding(
                        &self.engine.root,
                        &self.engine.store.workspace_id,
                        &self.state.id,
                        &self.persisted,
                        deadline,
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
                if !memory {
                    data = json!({"activeProject":self.state.active_project,"modes":self.state.modes,"revision":self.state.revision});
                }
                Reply {
                    status: 200,
                    completion: None,
                    value: json!({"schema":"native_dashboard_management_v1","id":request.id,
                    "outcome":"completed","metadataChanged":metadata_changed,"sourceWritten":false,
                    "sessionStateSha256":crate::model::hash(&self.persisted),"revision":self.state.revision,"data":data}),
                }
            }
            Err(error) => match error.to_string().as_str() {
                "Stale note revision"
                | "Named note already exists"
                | "Rename destination exists" => {
                    Reply::fault(409, "memory_changed_refresh", Some(&request.id), true)
                }
                "Named note unavailable" => {
                    Reply::fault(404, "memory_unavailable", Some(&request.id), true)
                }
                _ => Reply::fault(
                    422,
                    if write {
                        "operation_unconfirmed_refresh_before_retry"
                    } else {
                        "operation_refused"
                    },
                    Some(&request.id),
                    !write,
                ),
            },
        };
        Ok(reply)
    }
}

#[cfg(test)]
mod shutdown_tests {
    use super::*;
    use crate::session_dashboard::management::ShutdownScope;
    fn fixture(granted: bool, denied: bool) -> (tempfile::TempDir, Session) {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("main.rs"), b"fn untouched() {}\r\n").unwrap();
        let mut host = json!({"dashboard":{"management":{"shutdown":granted,"sessionControls":false,"memories":false}},"steering":{}});
        if denied {
            host["steering"]["global"] = json!({"excludedTools":["ci_session"]});
        }
        let mut session = Session::open(
            Engine::open(root.path()).unwrap(),
            serde_json::from_value(host).unwrap(),
            Some("owned-shutdown"),
            &Deadline::new(20_000),
        )
        .unwrap();
        session
            .enable_dashboard_management(&Deadline::new(20_000))
            .unwrap();
        (root, session)
    }
    fn request(session: &Session) -> Request {
        crate::json::decode(json!({"id":"a".repeat(64),"sessionStateSha256":crate::model::hash(&session.persisted),"project":session.state.active_project,"action":{"kind":"shutdown"}})).unwrap()
    }
    #[test]
    fn host_grant_alone_never_installs_a_shutdown_target_or_bypasses_current_policy() {
        for (grant, bind, denied) in [
            (false, true, false),
            (true, false, false),
            (true, true, true),
        ] {
            let (_root, mut session) = fixture(grant, denied);
            if bind {
                session
                    .bind_dashboard_shutdown(ShutdownScope::HttpSession)
                    .unwrap();
            }
            let request = request(&session);
            let result = session
                .dashboard_action(&request, &Deadline::new(2000))
                .unwrap();
            assert_eq!(result.status, 403);
            assert_eq!(result.value["notApplied"], true);
            assert!(!session.dashboard_shutdown_pending());
            assert!(!session.dashboard_shutdown_ready());
        }
    }
    #[test]
    fn stale_or_expired_shutdown_does_not_stop_then_acceptance_quiesces_without_changing_source() {
        let (root, mut session) = fixture(true, false);
        let (_other_root, mut other) = fixture(true, false);
        session
            .bind_dashboard_shutdown(ShutdownScope::HttpSession)
            .unwrap();
        other
            .bind_dashboard_shutdown(ShutdownScope::StdioConnection)
            .unwrap();
        let before = session.persisted.clone();
        let mut request = request(&session);
        request.session_state_sha256 = "b".repeat(64);
        let stale = session
            .dashboard_action(&request, &Deadline::new(2000))
            .unwrap();
        assert_eq!(stale.status, 409);
        assert!(!session.dashboard_shutdown_pending());
        request.session_state_sha256 = crate::model::hash(&session.persisted);
        assert!(
            session
                .dashboard_action(&request, &Deadline::new(0))
                .is_err()
        );
        assert!(!session.dashboard_shutdown_pending());
        let mut accepted = session
            .dashboard_action(&request, &Deadline::new(2000))
            .unwrap();
        assert_eq!(accepted.status, 200);
        assert_eq!(accepted.value["data"]["scope"], "owning_http_mcp_session");
        assert_eq!(accepted.value["data"]["serviceShutdown"], false);
        assert_eq!(accepted.value["data"]["processExitConfirmed"], false);
        assert!(session.dashboard_shutdown_pending() && !session.dashboard_shutdown_ready());
        assert_eq!(
            session
                .dashboard_action(&request, &Deadline::new(2000))
                .unwrap()
                .status,
            409
        );
        assert_eq!(session.persisted, before);
        accepted.completion.take().unwrap().finish();
        assert!(session.dashboard_shutdown_ready());
        assert!(!other.dashboard_shutdown_pending());
        assert_eq!(
            std::fs::read(root.path().join("main.rs")).unwrap(),
            b"fn untouched() {}\r\n"
        );
    }
}
