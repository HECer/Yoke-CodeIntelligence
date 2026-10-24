//! Project files use the original workspace plan/approval/verification writer.
//! The browser can prepare and inspect a plan, but cannot approve or execute it.
use super::*;
use crate::Store;
use crate::host_install::documents::{Plan as DocumentPlan, Scope};
use crate::session_dashboard::management::{DocumentRequest, Reply, Request};
use std::time::{Duration, Instant};

#[derive(Clone)]
pub(super) struct Prepared {
    document: DocumentPlan,
    workspace: crate::changes::Plan,
    project: String,
    session_state: String,
    pub(super) expires: Instant,
}

fn gates(store: &Store, plan: &crate::changes::Plan) -> Value {
    let approved = store
        .get_record::<crate::changes::Approval>("approval", &plan.id)
        .is_ok_and(|a| a.plan_id == plan.id && (plan.warnings.is_empty() || a.allow_test_changes));
    let verified = store
        .get_record::<Value>("verification", &plan.id)
        .is_ok_and(|v| {
            v["status"] == "passed"
                && v["level"] == "executed"
                && v["snapshotId"] == plan.result_snapshot_id
        });
    json!({"hostApprovalReady":approved,"executedVerificationReady":verified,
        "browserCanApprove":false,"browserCanExecuteVerification":false})
}

fn reviewed_value(prepared: &Prepared, store: &Store) -> Result<Value> {
    let mut value = serde_json::to_value(&prepared.document)?;
    value["workspacePlanId"] = json!(prepared.workspace.id);
    value["baseSnapshotId"] = json!(prepared.workspace.base_snapshot_id);
    value["resultSnapshotId"] = json!(prepared.workspace.result_snapshot_id);
    value["workspaceGates"] = gates(store, &prepared.workspace);
    value["workspaceRecovery"] = json!("original_host_cli");
    Ok(value)
}

impl Session {
    fn project_document_permission(&self, id: &str, d: &Deadline) -> Result<()> {
        self.require_edit_permission()?;
        ensure!(
            self.host_allow_writes,
            "Independent host workspace writes required"
        );
        let selected = self
            .dashboard_documents
            .as_ref()
            .context("Dashboard documents unavailable")?
            .access
            .selection(id)?;
        ensure!(
            selected.writable
                && matches!(&selected.scope, Scope::Project { project } if project == &self.state.active_project),
            "Writable active project document selection required"
        );
        self.require_tool("ci_session", d)
    }

    fn execute_project_document(
        &mut self,
        request: &Request,
        action: &DocumentRequest,
        d: &Deadline,
        attempt: &mut Option<Value>,
    ) -> Result<Value> {
        let id = action.document_id();
        self.project_document_permission(id, d)?;
        self.dashboard_documents
            .as_mut()
            .context("Dashboard documents unavailable")?
            .prune_plans();
        match action {
            DocumentRequest::Plan {
                expected_revision,
                content,
                ..
            } => {
                self.require_tool("ci_change_prepare", d)?;
                let engine = self.active_engine_at(d)?;
                ensure!(
                    engine.allow_writes,
                    "Current Engine workspace write grant required"
                );
                let root = engine.root.clone();
                let access = &self.dashboard_documents.as_ref().unwrap().access;
                let path = access.project_path(id, &request.project, &root, d)?;
                let before = access.read(id, d)?;
                ensure!(
                    before.revision == *expected_revision,
                    "Stale project document revision"
                );
                let provisional = access.plan(id, expected_revision, content, d)?;
                let result = self.call_active("ci_change_prepare", json!({
                    "edits":[{"kind":"replace","path":path,"expectedHash":provisional.before_hash,
                    "startByte":0,"endByte":before.byte_length,"text":content}],"maxBytes":4_000_000
                }), d)?;
                ensure!(
                    result["status"] == "ok",
                    "Project document omitted, stale or plan unavailable"
                );
                let workspace: crate::changes::Plan =
                    serde_json::from_value(result["data"]["plan"].clone())?;
                ensure!(
                    workspace.files.len() == 1 && workspace.directory_move.is_none(),
                    "Project document plan must affect exactly one file"
                );
                let file = &workspace.files[0];
                ensure!(
                    file.path == path
                        && file.before_hash.as_deref() == Some(before.sha256.as_str()),
                    "Project document captured membership differs"
                );
                let after = file
                    .after_hash
                    .as_deref()
                    .context("Project document deletion refused")?;
                let effective = self.active_engine_at(d)?.store.read_blob(after)?;
                let effective = std::str::from_utf8(&effective)?;
                // Native source line-ending policy may normalize the proposal.
                // Publish exactly those stored after-bytes for browser review.
                let document = self.dashboard_documents.as_ref().unwrap().access.plan(
                    id,
                    expected_revision,
                    effective,
                    d,
                )?;
                ensure!(
                    document.after_hash == after,
                    "Project document after-bytes differ"
                );
                let prepared = Prepared {
                    document,
                    workspace,
                    project: request.project.clone(),
                    session_state: request.session_state_sha256.clone(),
                    expires: Instant::now() + Duration::from_secs(600),
                };
                let value = reviewed_value(&prepared, &self.active_engine_at(d)?.store)?;
                let runtime = self.dashboard_documents.as_mut().unwrap();
                runtime
                    .workspace_plans
                    .retain(|_, p| p.document.document_id != id);
                ensure!(
                    runtime.proposal_count() < 16,
                    "Dashboard reviewed proposal capacity exceeded"
                );
                runtime
                    .workspace_plans
                    .insert(prepared.document.id.clone(), prepared);
                Ok(
                    json!({"plan":value,"expiresAfterMs":600000,"fileChanged":false,
                    "metadataChanged":true,"sourceWritten":false}),
                )
            }
            DocumentRequest::Apply { plan_id, .. } | DocumentRequest::Inspect { plan_id, .. } => {
                let prepared = self
                    .dashboard_documents
                    .as_ref()
                    .unwrap()
                    .workspace_plans
                    .get(plan_id)
                    .context("Dashboard reviewed workspace proposal unavailable")?
                    .clone();
                ensure!(
                    prepared.document.document_id == id
                        && prepared.project == request.project
                        && prepared.session_state == request.session_state_sha256,
                    "Dashboard reviewed workspace proposal Session changed"
                );
                let root = self.active_engine_at(d)?.root.clone();
                let access = &self.dashboard_documents.as_ref().unwrap().access;
                let path = access.project_path(id, &request.project, &root, d)?;
                ensure!(
                    access.read(id, d)?.revision == prepared.document.before_revision,
                    "Project document physical, security or byte revision changed"
                );
                let engine = self.active_engine_at(d)?;
                ensure!(
                    engine.allow_writes,
                    "Current Engine workspace write grant required"
                );
                let checked =
                    crate::changes::load_checked_plan(&engine.store, &prepared.workspace.id, d)?;
                ensure!(
                    serde_json::to_value(&checked)? == serde_json::to_value(&prepared.workspace)?
                        && checked.files.len() == 1
                        && checked.files[0].path == path,
                    "Project document native plan changed"
                );
                if matches!(action, DocumentRequest::Inspect { .. }) {
                    return Ok(json!({"plan":reviewed_value(&prepared,&engine.store)?,
                        "fileChanged":false,"metadataChanged":false,"sourceWritten":false}));
                }
                self.require_tool("ci_change_apply", d)?;
                let current_gates = gates(&self.active_engine_at(d)?.store, &prepared.workspace);
                ensure!(
                    current_gates["hostApprovalReady"] == true
                        && current_gates["executedVerificationReady"] == true,
                    "Exact host approval and executed verification required"
                );
                d.check()?;
                let key = format!("dashboard-{}", plan_id);
                *attempt = Some(json!({"workspacePlanId":prepared.workspace.id,
                    "possibleJournalId":crate::model::identity(&(&prepared.workspace.id,&key)),
                    "workflow":"original_native_host_recovery",
                    "journalCreationConfirmed":false}));
                // Never approve, synthesize verification or use the global-file
                // writer here. The native writer rechecks all original gates,
                // current full base snapshot, WriterLock and durable journals.
                let result = self.call_active(
                    "ci_change_apply",
                    json!({
                        "planId":prepared.workspace.id,"idempotencyKey":key,
                        "mode":"workspace","maxBytes":4_000_000
                    }),
                    d,
                )?;
                ensure!(
                    result["status"] == "ok" && result["data"]["receipt"]["status"] == "applied",
                    "Workspace outcome unconfirmed; inspect native recovery journal"
                );
                let receipt = result["data"]["receipt"].clone();
                self.dashboard_documents
                    .as_mut()
                    .unwrap()
                    .workspace_plans
                    .remove(plan_id);
                Ok(
                    json!({"receipt":receipt,"workspacePlanId":prepared.workspace.id,
                    "workspaceRecovery":"original_host_cli","fileChanged":true,
                    "metadataChanged":true,"sourceWritten":true}),
                )
            }
            DocumentRequest::Pending { .. } => Ok(json!({
                "workspaceRecoveryRequiresHost":true,"recoveryAvailableInDashboard":false,
                "workflow":"native_workspace_host_recovery","complete":false,
                "fileChanged":false,"metadataChanged":false,"sourceWritten":false
            })),
            DocumentRequest::Recover { .. } => {
                anyhow::bail!("Project recovery requires the original native host recovery command")
            }
            DocumentRequest::Read { .. } => {
                unreachable!("Reads retain the selected-document route")
            }
        }
    }

    pub(super) fn dashboard_project_document_action(
        &mut self,
        request: &Request,
        action: &DocumentRequest,
        d: &Deadline,
    ) -> Result<Reply> {
        let operation = match action {
            DocumentRequest::Plan { .. } => "dashboard.documents.plan",
            DocumentRequest::Inspect { .. } => "dashboard.documents.inspect",
            DocumentRequest::Apply { .. } => "dashboard.documents.apply",
            DocumentRequest::Pending { .. } => "dashboard.documents.pending",
            DocumentRequest::Recover { .. } => "dashboard.documents.recover",
            DocumentRequest::Read { .. } => unreachable!(),
        };
        let activity = self
            .dashboard
            .as_ref()
            .and_then(|handle| handle.begin(operation));
        let mut attempt = None;
        let result = self.execute_project_document(request, action, d, &mut attempt);
        self.finish_dashboard_activity(activity, &result);
        let data = match result {
            Ok(value) => value,
            Err(_) => {
                let mut reply = Reply::fault(
                    422,
                    if attempt.is_some() {
                        "workspace_outcome_unconfirmed_use_native_recovery"
                    } else {
                        "workspace_document_or_host_gates_unavailable"
                    },
                    Some(&request.id),
                    attempt.is_none(),
                );
                if let Some(recovery) = attempt {
                    reply.value["recovery"] = recovery;
                }
                return Ok(reply);
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
            let mut reply = Reply::fault(
                409,
                "session_changed_after_operation_refresh",
                Some(&request.id),
                attempt.is_none(),
            );
            if let Some(recovery) = attempt {
                reply.value["recovery"] = recovery;
            }
            return Ok(reply);
        }
        Ok(Reply {
            status: 200,
            completion: None,
            value: json!({"schema":"native_dashboard_management_v1","id":request.id,
                "outcome":"completed","metadataChanged":data["metadataChanged"]==true,
                "fileChanged":data["fileChanged"]==true,"sourceWritten":data["sourceWritten"]==true,
                "sessionStateSha256":crate::model::hash(&self.persisted),
                "revision":self.state.revision,"data":data}),
        })
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::fs;

    fn d() -> Deadline {
        Deadline::new(120_000)
    }
    fn fixture(writes: bool, selected: bool) -> (tempfile::TempDir, Session) {
        let root = tempfile::tempdir().unwrap();
        fs::write(
            root.path().join("settings-π.json"),
            "\u{feff}{\"value\":\"old 🦀\"}\r\n",
        )
        .unwrap();
        fs::write(root.path().join("other.rs"), "fn original() {}\n").unwrap();
        let host: HostProfile = serde_json::from_value(json!({"dashboard":{"management":{
            "documents":{"documents":[{"id":"settings","label":"Selected project settings",
                "path":root.path().join("settings-π.json"),"scope":{"kind":"project","project":"default"},
                "writable":selected}]}}}})).unwrap();
        let mut engine = Engine::open(root.path()).unwrap();
        engine.allow_writes = writes;
        let session = Session::open(engine, host, Some("workspace-documents"), &d()).unwrap();
        (root, session)
    }
    fn call(s: &mut Session, action: Value) -> Reply {
        let envelope: Request = serde_json::from_value(json!({"id":"a".repeat(64),
            "sessionStateSha256":crate::model::hash(&s.persisted),"project":s.state.active_project,
            "action":{"kind":"document","request":action}}))
        .unwrap();
        envelope.validate().unwrap();
        let crate::session_dashboard::management::Action::Document { request } = &envelope.action
        else {
            unreachable!()
        };
        s.dashboard_document_action(&envelope, request, &d())
            .unwrap()
    }
    fn plan(s: &mut Session) -> Value {
        let loaded = call(s, json!({"kind":"read","documentId":"settings"}));
        assert_eq!(loaded.status, 200);
        let result = call(
            s,
            json!({"kind":"plan","documentId":"settings",
            "expectedRevision":loaded.value["data"]["document"]["revision"],
            "content":"\u{feff}{\"value\":\"new 🦀 <script>inert</script>\"}\r\n"}),
        );
        assert_eq!(result.status, 200, "{}", result.value);
        assert_eq!(result.value["sourceWritten"], false);
        result.value["data"]["plan"].clone()
    }
    fn verify_host(s: &mut Session, id: &str) {
        // The explicitly selected current test harness prints help in the
        // owned copied tree. It runs no test or analyzed project logic and
        // needs no separately installed shell, compiler or Python runtime.
        let command = std::env::current_exe().unwrap();
        let result = crate::verify::execute(
            s.active_engine_at(&d()).unwrap(),
            id,
            command.to_str().unwrap(),
            &["--help".into()],
            true,
            &d(),
        )
        .unwrap();
        assert_eq!(result["status"], "passed", "{result}");
        assert_eq!(result["level"], "executed");
    }

    #[test]
    fn project_writer_requires_independent_grants_and_never_uses_global_journals() {
        for (writes, selected) in [(false, true), (true, false)] {
            let (root, mut s) = fixture(writes, selected);
            let before = fs::read(root.path().join("settings-π.json")).unwrap();
            let loaded = call(&mut s, json!({"kind":"read","documentId":"settings"}));
            let refused = call(
                &mut s,
                json!({"kind":"plan","documentId":"settings",
                "expectedRevision":loaded.value["data"]["document"]["revision"],"content":"unapproved"}),
            );
            assert_eq!(refused.status, 422);
            assert_eq!(refused.value["notApplied"], true);
            assert_eq!(
                fs::read(root.path().join("settings-π.json")).unwrap(),
                before
            );
            assert_eq!(
                s.dashboard_documents_view().unwrap()["projectWrites"],
                false
            );
        }
        let (root, mut s) = fixture(true, true);
        let before = fs::read(root.path().join("settings-π.json")).unwrap();
        let p = plan(&mut s);
        assert_eq!(
            fs::read(root.path().join("settings-π.json")).unwrap(),
            before
        );
        assert_ne!(p["id"], p["workspacePlanId"]);
        assert_eq!(p["workspaceGates"]["hostApprovalReady"], false);
        assert_eq!(p["workspaceGates"]["executedVerificationReady"], false);
        assert!(
            s.dashboard_documents
                .as_ref()
                .unwrap()
                .access
                .journal_binding()
                .is_none()
        );
        let pending = call(&mut s, json!({"kind":"pending","documentId":"settings"}));
        assert_eq!(pending.value["data"]["recoveryAvailableInDashboard"], false);
        assert_eq!(pending.value["data"]["complete"], false);
        assert!(pending.value["data"].get("journals").is_none());
    }

    #[test]
    fn exact_host_approval_and_real_executed_verification_gate_unicode_project_bytes() {
        let (root, mut s) = fixture(true, true);
        let p = plan(&mut s);
        let id = p["workspacePlanId"].as_str().unwrap();
        let apply = json!({"kind":"apply","documentId":"settings","planId":p["id"]});
        let refused = call(&mut s, apply.clone());
        assert_eq!(refused.status, 422);
        assert_eq!(refused.value["notApplied"], true);
        s.active_engine_at(&d())
            .unwrap()
            .approve(id, false)
            .unwrap();
        assert_eq!(call(&mut s, apply.clone()).status, 422);
        let checked = crate::changes::load_checked_plan(&s.engine.store, id, &d()).unwrap();
        crate::changes::verify_syntax(&s.engine.store, &checked).unwrap();
        assert_eq!(call(&mut s, apply.clone()).status, 422);
        verify_host(&mut s, id);
        let inspected = call(
            &mut s,
            json!({"kind":"inspect","documentId":"settings","planId":p["id"]}),
        );
        assert_eq!(
            inspected.value["data"]["plan"]["workspaceGates"]["hostApprovalReady"],
            true
        );
        assert_eq!(
            inspected.value["data"]["plan"]["workspaceGates"]["executedVerificationReady"],
            true
        );
        let applied = call(&mut s, apply);
        assert_eq!(applied.status, 200, "{}", applied.value);
        assert_eq!(applied.value["sourceWritten"], true);
        assert_eq!(applied.value["fileChanged"], true);
        assert_eq!(
            fs::read(root.path().join("settings-π.json")).unwrap(),
            p["content"].as_str().unwrap().as_bytes()
        );
        assert_eq!(
            fs::read(root.path().join("other.rs")).unwrap(),
            b"fn original() {}\n"
        );
        let receipt: crate::changes::Receipt =
            serde_json::from_value(applied.value["data"]["receipt"].clone()).unwrap();
        let journal: crate::changes::Receipt =
            s.engine.store.get_record("journal", &receipt.id).unwrap();
        assert_eq!(journal.status, "applied");
        assert_eq!(journal.changed_paths, vec!["settings-π.json"]);
    }

    #[test]
    fn changed_whole_workspace_refuses_even_with_approved_verified_selected_file() {
        let (root, mut s) = fixture(true, true);
        let before = fs::read(root.path().join("settings-π.json")).unwrap();
        let p = plan(&mut s);
        let id = p["workspacePlanId"].as_str().unwrap();
        s.engine.approve(id, false).unwrap();
        verify_host(&mut s, id);
        fs::write(root.path().join("other.rs"), "fn foreign_change() {}\n").unwrap();
        let refused = call(
            &mut s,
            json!({"kind":"apply","documentId":"settings","planId":p["id"]}),
        );
        assert_eq!(refused.status, 422);
        assert_eq!(refused.value["notApplied"], false);
        assert_eq!(refused.value["recovery"]["workspacePlanId"], id);
        assert_eq!(refused.value["recovery"]["journalCreationConfirmed"], false);
        assert!(crate::model::valid_id(
            refused.value["recovery"]["possibleJournalId"]
                .as_str()
                .unwrap()
        ));
        assert_eq!(
            fs::read(root.path().join("settings-π.json")).unwrap(),
            before
        );
        assert_eq!(
            fs::read(root.path().join("other.rs")).unwrap(),
            b"fn foreign_change() {}\n"
        );
    }

    #[test]
    fn selected_file_revision_expiry_and_proposal_session_are_not_rebindable() {
        let (root, mut s) = fixture(true, true);
        let p = plan(&mut s);
        let id = p["id"].as_str().unwrap();
        fs::write(root.path().join("settings-π.json"), "{\"foreign\":true}\n").unwrap();
        let refused = call(
            &mut s,
            json!({"kind":"inspect","documentId":"settings","planId":id}),
        );
        assert_eq!(refused.status, 422);
        assert_eq!(refused.value["notApplied"], true);
        let p = plan(&mut s);
        let id = p["id"].as_str().unwrap();
        s.dashboard_documents
            .as_mut()
            .unwrap()
            .workspace_plans
            .get_mut(id)
            .unwrap()
            .session_state = "b".repeat(64);
        assert_eq!(
            call(
                &mut s,
                json!({"kind":"inspect","documentId":"settings","planId":id})
            )
            .status,
            422
        );
        s.dashboard_documents
            .as_mut()
            .unwrap()
            .workspace_plans
            .get_mut(id)
            .unwrap()
            .expires = Instant::now() - Duration::from_secs(1);
        assert_eq!(
            call(
                &mut s,
                json!({"kind":"apply","documentId":"settings","planId":id})
            )
            .status,
            422
        );
    }

    #[test]
    fn protocol_cannot_supply_source_paths_commands_or_workspace_grants() {
        let (_root, mut s) = fixture(true, true);
        let p = plan(&mut s);
        for kind in ["plan", "inspect", "apply"] {
            let base = if kind == "plan" {
                json!({"kind":kind,"documentId":"settings","expectedRevision":p["beforeRevision"],"content":"inert"})
            } else {
                json!({"kind":kind,"documentId":"settings","planId":p["id"]})
            };
            for field in [
                "path",
                "command",
                "approval",
                "verification",
                "allowWrites",
                "snapshotId",
                "project",
            ] {
                let mut bad = base.clone();
                bad[field] = json!("untrusted");
                assert!(
                    serde_json::from_value::<DocumentRequest>(bad).is_err(),
                    "{kind}/{field}"
                );
            }
        }
        let envelope:Request=serde_json::from_value(json!({"id":"a".repeat(64),"sessionStateSha256":"b".repeat(64),"project":"default","action":{"kind":"document","request":serde_json::to_value(json!({"kind":"plan","documentId":"settings","expectedRevision":p["beforeRevision"],"content":"🦀".repeat(16001)})).unwrap()}})).unwrap();
        assert!(envelope.validate().is_err());
    }

    fn reopen(root: &std::path::Path, profile: Value) -> Session {
        let mut engine = Engine::open(root).unwrap();
        engine.allow_writes = true;
        Session::open(
            engine,
            serde_json::from_value(profile).unwrap(),
            Some("project-policy"),
            &d(),
        )
        .unwrap()
    }

    #[test]
    fn current_modes_read_only_and_native_tool_exclusions_cannot_be_bypassed() {
        for selected in ["prepare", "apply", "readOnly"] {
            let (root, base) = fixture(true, true);
            let mut profile = serde_json::to_value(&base.host).unwrap();
            if selected == "readOnly" {
                profile["primaryReadOnly"] = json!(true);
            } else {
                profile["steering"] = json!({"global":{"excludedTools":[if selected=="prepare" {"ci_change_prepare"} else {"ci_change_apply"}]}});
            }
            drop(base);
            let mut s = reopen(root.path(), profile);
            if selected != "apply" {
                let read = call(&mut s, json!({"kind":"read","documentId":"settings"}));
                let result = call(
                    &mut s,
                    json!({"kind":"plan","documentId":"settings","expectedRevision":read.value["data"]["document"]["revision"],"content":"refused"}),
                );
                assert_eq!(result.status, 422);
                assert_eq!(result.value["notApplied"], true);
            } else {
                let p = plan(&mut s);
                let id = p["workspacePlanId"].as_str().unwrap();
                s.engine.approve(id, false).unwrap();
                verify_host(&mut s, id);
                let result = call(
                    &mut s,
                    json!({"kind":"apply","documentId":"settings","planId":p["id"]}),
                );
                assert_eq!(result.status, 422);
                assert_eq!(result.value["notApplied"], true);
            }
        }
        let (_root, mut s) = fixture(true, true);
        let p = plan(&mut s);
        s.invoke("switchModes", &json!({"modes":["planning"]}), &d())
            .unwrap();
        assert_eq!(
            s.dashboard_documents_view().unwrap()["projectWrites"],
            false
        );
        assert_eq!(
            call(
                &mut s,
                json!({"kind":"inspect","documentId":"settings","planId":p["id"]})
            )
            .status,
            422
        );
    }

    #[test]
    fn source_line_ending_policy_publishes_the_exact_effective_proposal() {
        let (root, base) = fixture(true, true);
        let mut profile = serde_json::to_value(&base.host).unwrap();
        profile["sourceLineEndings"] = json!({"global":"lf"});
        drop(base);
        let mut s = reopen(root.path(), profile);
        let p = plan(&mut s);
        let content = p["content"].as_str().unwrap();
        assert!(content.starts_with('\u{feff}'));
        assert!(!content.contains('\r'));
        assert!(content.ends_with('\n'));
        let id = p["workspacePlanId"].as_str().unwrap();
        let native = crate::changes::load_checked_plan(&s.engine.store, id, &d()).unwrap();
        assert_eq!(
            native.files[0].after_hash.as_deref(),
            p["afterHash"].as_str()
        );
        assert_eq!(
            s.engine
                .store
                .read_blob(p["afterHash"].as_str().unwrap())
                .unwrap(),
            content.as_bytes()
        );
        assert_eq!(
            fs::read(root.path().join("settings-π.json")).unwrap(),
            "\u{feff}{\"value\":\"old 🦀\"}\r\n".as_bytes()
        );
    }

    #[test]
    fn secondary_project_uses_its_own_store_plan_approval_and_writer_root() {
        let temp = tempfile::tempdir().unwrap();
        let primary = temp.path().join("primary");
        let secondary = temp.path().join("secondary");
        for root in [&primary, &secondary] {
            fs::create_dir(root).unwrap();
            fs::write(root.join("settings-π.json"), "{\"before\":true}\r\n").unwrap();
        }
        let host:HostProfile=serde_json::from_value(json!({"projects":[{"name":"secondary","root":secondary}],"dashboard":{"management":{"documents":{"documents":[{"id":"settings","label":"Secondary settings","path":secondary.join("settings-π.json"),"scope":{"kind":"project","project":"secondary"},"writable":true}]}}}})).unwrap();
        let mut engine = Engine::open(&primary).unwrap();
        engine.allow_writes = true;
        let mut s = Session::open(engine, host, Some("secondary-documents"), &d()).unwrap();
        s.invoke("activate", &json!({"project":"secondary"}), &d())
            .unwrap();
        let p = plan(&mut s);
        let id = p["workspacePlanId"].as_str().unwrap();
        assert!(s.engine.store.get_record::<Value>("plan", id).is_err());
        s.active_engine_at(&d())
            .unwrap()
            .approve(id, false)
            .unwrap();
        verify_host(&mut s, id);
        let result = call(
            &mut s,
            json!({"kind":"apply","documentId":"settings","planId":p["id"]}),
        );
        assert_eq!(result.status, 200, "{}", result.value);
        assert_eq!(
            fs::read(primary.join("settings-π.json")).unwrap(),
            b"{\"before\":true}\r\n"
        );
        assert_eq!(
            fs::read(secondary.join("settings-π.json")).unwrap(),
            p["content"].as_str().unwrap().as_bytes()
        );
    }
}
