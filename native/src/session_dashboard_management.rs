//! Inert, bounded requests cross threads; the owning Session never does.
use crate::{capture::Deadline, model::valid_id};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU8, Ordering},
    mpsc::{self, Receiver, SyncSender},
};
use std::time::Duration;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct Config {
    pub session_controls: bool,
    pub memories: bool,
    pub operation_timeout_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language_servers: Option<LanguageGrant>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub documents: Option<crate::host_install::documents::Config>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_logs: Option<crate::host_install::logs::Config>,
    #[serde(skip_serializing_if = "is_false")]
    pub shutdown: bool,
}
fn is_false(value: &bool) -> bool {
    !value
}
impl Default for Config {
    fn default() -> Self {
        Self {
            session_controls: true,
            memories: true,
            operation_timeout_ms: 30_000,
            language_servers: None,
            documents: None,
            raw_logs: None,
            shutdown: false,
        }
    }
}
impl Config {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            (100..=60_000).contains(&self.operation_timeout_ms),
            "dashboard_management_timeout_capacity"
        );
        if let Some(grant) = &self.language_servers {
            grant.validate()?;
        }
        if let Some(documents) = &self.documents {
            documents.validate()?;
        }
        if let Some(logs) = &self.raw_logs {
            logs.validate()?;
        }
        Ok(())
    }
}

/// Independent host authority to select already trusted registrations. Dormant
/// registration alone never grants dashboard activation of a backend.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct LanguageGrant {
    pub backend_ids: Vec<String>,
    pub legacy_languages: Vec<String>,
}
fn distinct(values: &[String], maximum: usize) -> Result<()> {
    ensure!(values.len() <= maximum, "dashboard_language_capacity");
    let mut seen = std::collections::BTreeSet::new();
    for value in values {
        ensure!(
            !value.is_empty() && value.len() <= 100 && seen.insert(value),
            "dashboard_language_duplicate_or_invalid"
        );
    }
    Ok(())
}
impl LanguageGrant {
    pub(crate) fn validate(&self) -> Result<()> {
        distinct(&self.backend_ids, 32)?;
        distinct(&self.legacy_languages, 100)?;
        if !self.backend_ids.is_empty() {
            crate::server_selection::validate_ids(&self.backend_ids)?;
        }
        Ok(())
    }
    pub(crate) fn permits(&self, selection: &LanguageSelection) -> Result<()> {
        selection.validate()?;
        let (values, allowed) = match selection {
            LanguageSelection::Backends { ids } => (ids, &self.backend_ids),
            LanguageSelection::Legacy { languages } => (languages, &self.legacy_languages),
        };
        ensure!(
            values.iter().all(|id| allowed.contains(id)),
            "dashboard_language_not_granted"
        );
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum LanguageSelection {
    Backends { ids: Vec<String> },
    Legacy { languages: Vec<String> },
}
impl LanguageSelection {
    pub(crate) fn validate(&self) -> Result<()> {
        match self {
            Self::Backends { ids } => crate::server_selection::validate_ids(ids),
            Self::Legacy { languages } => distinct(languages, 100),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Request {
    pub id: String,
    pub session_state_sha256: String,
    pub project: String,
    pub action: Action,
}
#[derive(Debug, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub(crate) enum Action {
    Shutdown {},
    ReadLog {
        log_id: String,
        #[serde(default)]
        max_bytes: Option<usize>,
    },
    Document {
        request: DocumentRequest,
    },
    ListMemories {
        #[serde(default)]
        topic: String,
    },
    ReadMemory {
        name: String,
    },
    CreateMemory {
        name: String,
        content: String,
    },
    WriteMemory {
        name: String,
        content: String,
        expected_revision: String,
    },
    DeleteMemory {
        name: String,
        expected_revision: String,
    },
    RenameMemory {
        old_name: String,
        new_name: String,
        expected_revision: String,
    },
    SwitchModes {
        modes: Vec<String>,
    },
    ResetModes {},
    ActivateProject {
        project: String,
    },
    SelectLanguageServers {
        selection: LanguageSelection,
    },
    ResetLanguageServers {},
}
#[derive(Debug, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub(crate) enum DocumentRequest {
    Read {
        document_id: String,
    },
    Plan {
        document_id: String,
        expected_revision: String,
        content: String,
    },
    Apply {
        document_id: String,
        plan_id: String,
    },
    Inspect {
        document_id: String,
        plan_id: String,
    },
    Pending {
        document_id: String,
        #[serde(default)]
        cursor: Option<String>,
    },
    Recover {
        document_id: String,
        journal_id: String,
        expected_event: String,
        direction: String,
    },
}
impl DocumentRequest {
    pub(crate) fn document_id(&self) -> &str {
        match self {
            Self::Read { document_id }
            | Self::Plan { document_id, .. }
            | Self::Apply { document_id, .. }
            | Self::Inspect { document_id, .. }
            | Self::Pending { document_id, .. }
            | Self::Recover { document_id, .. } => document_id,
        }
    }
    fn validate(&self) -> Result<()> {
        crate::project_session::label(self.document_id())?;
        match self {
            Self::Plan {
                expected_revision,
                content,
                ..
            } => ensure!(
                valid_id(expected_revision) && content.len() <= 64000,
                "dashboard_document_proposal_capacity_or_revision"
            ),
            Self::Apply { plan_id, .. } | Self::Inspect { plan_id, .. } => {
                ensure!(valid_id(plan_id), "dashboard_document_plan_id_invalid")
            }
            Self::Pending { cursor, .. } => {
                ensure!(
                    cursor.as_deref().is_none_or(valid_id),
                    "dashboard_document_cursor_invalid"
                )
            }
            Self::Recover {
                journal_id,
                expected_event,
                direction,
                ..
            } => ensure!(
                valid_id(journal_id)
                    && valid_id(expected_event)
                    && matches!(direction.as_str(), "resume" | "rollback"),
                "dashboard_document_recovery_invalid"
            ),
            _ => {}
        }
        Ok(())
    }
}
impl Request {
    pub(crate) fn validate(&self) -> Result<()> {
        ensure!(
            valid_id(&self.id) && valid_id(&self.session_state_sha256),
            "dashboard_request_identity_invalid"
        );
        crate::project_session::label(&self.project)?;
        let project_name = |name: &str| -> Result<()> {
            let (scope, _) = crate::notes::prompt_selection(name)?;
            ensure!(
                scope == crate::notes::Scope::Project,
                "dashboard_project_memory_scope_required"
            );
            Ok(())
        };
        match &self.action {
            Action::ReadLog { log_id, max_bytes } => {
                crate::project_session::label(log_id)?;
                ensure!(
                    max_bytes.is_none_or(|n| (1..=64000).contains(&n)),
                    "dashboard_log_window_capacity"
                );
            }
            Action::Document { request } => request.validate()?,
            Action::ReadMemory { name } | Action::DeleteMemory { name, .. } => project_name(name)?,
            Action::CreateMemory { name, content } | Action::WriteMemory { name, content, .. } => {
                project_name(name)?;
                crate::notes::content(content)?;
            }
            Action::RenameMemory {
                old_name, new_name, ..
            } => {
                project_name(old_name)?;
                project_name(new_name)?;
            }
            Action::ActivateProject { project } => crate::project_session::label(project)?,
            Action::SelectLanguageServers { selection } => selection.validate()?,
            Action::ListMemories { topic } if !topic.is_empty() => {
                crate::paths::normalized(topic)?;
            }
            _ => {}
        }
        match &self.action {
            Action::WriteMemory {
                expected_revision, ..
            }
            | Action::DeleteMemory {
                expected_revision, ..
            }
            | Action::RenameMemory {
                expected_revision, ..
            } => ensure!(
                valid_id(expected_revision),
                "dashboard_note_revision_invalid"
            ),
            Action::SwitchModes { modes } => ensure!(
                modes.len() <= 64 && modes.iter().all(|m| m.len() <= 100),
                "dashboard_mode_capacity"
            ),
            _ => {}
        }
        Ok(())
    }
}

#[derive(Debug)]
pub(crate) struct Reply {
    pub status: u16,
    pub value: Value,
    pub completion: Option<ResponseCompletion>,
}
impl Reply {
    pub(crate) fn fault(
        status: u16,
        code: &'static str,
        id: Option<&str>,
        not_applied: bool,
    ) -> Self {
        Self {
            status,
            value: json!({"schema":"native_dashboard_management_v1","id":id,"error":code,"notApplied":not_applied}),
            completion: None,
        }
    }
}

/// Installed only by a trusted owning transport driver, never from JSON.
#[derive(Clone, Copy, Debug)]
pub(crate) enum ShutdownScope {
    StdioConnection,
    HttpSession,
}
impl ShutdownScope {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::StdioConnection => "owning_stdio_mcp_connection",
            Self::HttpSession => "owning_http_mcp_session",
        }
    }
}
#[derive(Clone, Debug)]
pub(crate) struct ShutdownSignal {
    stage: Arc<AtomicU8>,
    pub scope: ShutdownScope,
}
impl ShutdownSignal {
    pub(crate) fn new(scope: ShutdownScope) -> Self {
        Self {
            stage: Arc::new(AtomicU8::new(0)),
            scope,
        }
    }
    pub(crate) fn requested(&self) -> bool {
        self.stage.load(Ordering::Acquire) != 0
    }
    pub(crate) fn ready(&self) -> bool {
        self.stage.load(Ordering::Acquire) >= 2
    }
    pub(crate) fn request(&self, deadline: &Deadline) -> Result<ResponseCompletion> {
        deadline.check()?;
        ensure!(
            self.stage
                .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
                .is_ok(),
            "Dashboard shutdown already requested"
        );
        Ok(ResponseCompletion {
            stage: self.stage.clone(),
            attempted: false,
        })
    }
}
/// Sending a Reply is not an HTTP response attempt. The listener retains this
/// until its bounded write returns; abandoned delivery separately releases the
/// owner so a failed client cannot keep an accepted shutdown pending forever.
#[derive(Debug)]
pub(crate) struct ResponseCompletion {
    stage: Arc<AtomicU8>,
    attempted: bool,
}
impl ResponseCompletion {
    pub(crate) fn finish(mut self) {
        self.attempted = true;
        let _ = self
            .stage
            .compare_exchange(1, 2, Ordering::AcqRel, Ordering::Acquire);
    }
}
impl Drop for ResponseCompletion {
    fn drop(&mut self) {
        if !self.attempted {
            let _ = self
                .stage
                .compare_exchange(1, 3, Ordering::AcqRel, Ordering::Acquire);
        }
    }
}
#[derive(Clone)]
pub(crate) struct Client {
    sender: SyncSender<Pending>,
}
pub(crate) struct Mailbox {
    receiver: Receiver<Pending>,
    pub client: Client,
}
pub(crate) struct Pending {
    pub request: Request,
    pub deadline: Deadline,
    pub cancel: Arc<AtomicBool>,
    pub access_closed: Arc<AtomicBool>,
    pub transport_closed: Option<Arc<AtomicBool>>,
    state: Arc<AtomicU8>,
    reply: SyncSender<Reply>,
}
impl Mailbox {
    pub(crate) fn new() -> Self {
        let (sender, receiver) = mpsc::sync_channel(8);
        Self {
            receiver,
            client: Client { sender },
        }
    }
    pub(crate) fn next(&self) -> Option<Pending> {
        self.receiver.try_recv().ok()
    }
}
impl Pending {
    pub(crate) fn begin(&self) -> bool {
        !self.access_closed.load(Ordering::Acquire)
            && !self
                .transport_closed
                .as_ref()
                .is_some_and(|f| f.load(Ordering::Acquire))
            && self.deadline.check().is_ok()
            && self
                .state
                .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
    }
    pub(crate) fn finish(self, reply: Reply) {
        self.state.store(3, Ordering::Release);
        let _ = self.reply.send(reply);
    }
}
impl Client {
    pub(crate) fn call(
        &self,
        request: Request,
        deadline: Deadline,
        access_closed: Arc<AtomicBool>,
        transport_closed: Option<Arc<AtomicBool>>,
    ) -> Reply {
        let id = request.id.clone();
        if request.validate().is_err() {
            return Reply::fault(400, "invalid_request", Some(&id), true);
        }
        let (reply, receiver) = mpsc::sync_channel(1);
        let state = Arc::new(AtomicU8::new(0));
        let cancel = Arc::new(AtomicBool::new(false));
        let pending = Pending {
            request,
            deadline: deadline.clone(),
            cancel: cancel.clone(),
            access_closed: access_closed.clone(),
            transport_closed: transport_closed.clone(),
            state: state.clone(),
            reply,
        };
        if self.sender.try_send(pending).is_err() {
            return Reply::fault(503, "host_busy", Some(&id), true);
        }
        loop {
            let remaining = deadline
                .remaining()
                .saturating_sub(Duration::from_millis(20));
            if remaining.is_zero()
                || access_closed.load(Ordering::Acquire)
                || transport_closed
                    .as_ref()
                    .is_some_and(|f| f.load(Ordering::Acquire))
            {
                let not_applied = state
                    .compare_exchange(0, 2, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok();
                cancel.store(true, Ordering::Release);
                return Reply::fault(
                    408,
                    if not_applied {
                        "expired_before_execution"
                    } else {
                        "outcome_unconfirmed_refresh_before_retry"
                    },
                    Some(&id),
                    not_applied,
                );
            }
            match receiver.recv_timeout(remaining.min(Duration::from_millis(10))) {
                Ok(reply) => return reply,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    let not_applied = state
                        .compare_exchange(0, 2, Ordering::AcqRel, Ordering::Acquire)
                        .is_ok();
                    cancel.store(true, Ordering::Release);
                    return Reply::fault(503, "host_unavailable", Some(&id), not_applied);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shutdown_signal_waits_for_response_attempt_and_cannot_target_another_owner() {
        let signal = ShutdownSignal::new(ShutdownScope::StdioConnection);
        let other = ShutdownSignal::new(ShutdownScope::HttpSession);
        assert!(signal.request(&Deadline::new(0)).is_err());
        assert!(!signal.requested() && !signal.ready());
        let completion = signal.request(&Deadline::new(1000)).unwrap();
        assert!(signal.requested() && !signal.ready());
        assert!(signal.request(&Deadline::new(1000)).is_err());
        assert!(!other.requested() && !other.ready());
        completion.finish();
        assert!(signal.ready());
        assert_eq!(signal.stage.load(Ordering::Acquire), 2);
        assert!(!other.ready());
    }
    #[test]
    fn abandoned_shutdown_delivery_unblocks_only_its_already_accepted_owner() {
        let signal = ShutdownSignal::new(ShutdownScope::HttpSession);
        let completion = signal.request(&Deadline::new(1000)).unwrap();
        assert!(!signal.ready());
        drop(completion);
        assert!(signal.ready());
        assert_eq!(signal.stage.load(Ordering::Acquire), 3);
        assert_eq!(signal.scope.name(), "owning_http_mcp_session");
    }
    #[test]
    fn absent_and_false_shutdown_grants_preserve_existing_serialization() {
        let absent: Config = serde_json::from_value(json!({})).unwrap();
        let disabled: Config = serde_json::from_value(json!({"shutdown":false})).unwrap();
        assert_eq!(absent, disabled);
        assert!(
            serde_json::to_value(absent)
                .unwrap()
                .get("shutdown")
                .is_none()
        );
        assert!(serde_json::from_value::<Config>(json!({"shutdown":null})).is_err());
        assert!(serde_json::from_value::<Config>(json!({"shutdown":{"scope":"service"}})).is_err());
    }
    #[test]
    fn raw_log_request_rejects_paths_commands_offsets_and_excessive_budgets() {
        let base = json!({"id":"a".repeat(64),"sessionStateSha256":"b".repeat(64),"project":"default","action":{"kind":"readLog","logId":"session-log","maxBytes":8}});
        crate::json::decode::<Request>(base.clone())
            .unwrap()
            .validate()
            .unwrap();
        for field in ["path", "command", "offset", "service", "grant"] {
            let mut bad = base.clone();
            bad["action"][field] = json!("untrusted");
            assert!(crate::json::decode::<Request>(bad).is_err());
        }
        for value in [json!(0), json!(64001)] {
            let mut bad = base.clone();
            bad["action"]["maxBytes"] = value;
            assert!(
                crate::json::decode::<Request>(bad)
                    .unwrap()
                    .validate()
                    .is_err()
            );
        }
        let mut bad = base;
        bad["action"]["logId"] = json!("../outside");
        assert!(
            crate::json::decode::<Request>(bad)
                .unwrap()
                .validate()
                .is_err()
        );
    }
    #[test]
    fn argumentless_management_actions_reject_all_unknown_authority_fields() {
        for kind in ["resetModes", "resetLanguageServers", "shutdown"] {
            let base = json!({"kind":kind});
            serde_json::from_value::<Action>(base.clone()).unwrap();
            for field in ["command", "scope", "project", "service"] {
                let mut action = base.clone();
                action[field] = json!("untrusted");
                assert!(
                    serde_json::from_value::<Action>(action).is_err(),
                    "{kind}: {field}"
                );
            }
        }
    }
    #[test]
    fn document_actions_reject_unknown_authority_and_invalid_byte_or_revision_requests() {
        let base = json!({"id":"a".repeat(64),"sessionStateSha256":"b".repeat(64),"project":"default","action":{"kind":"document","request":{"kind":"plan","documentId":"settings","expectedRevision":"c".repeat(64),"content":"inert"}}});
        let valid: Request = crate::json::decode(base.clone()).unwrap();
        valid.validate().unwrap();
        for key in ["path", "root", "approval", "command", "allowWrites"] {
            let mut value = base.clone();
            value["action"]["request"][key] = json!(true);
            assert!(crate::json::decode::<Request>(value).is_err());
        }
        for (key, value) in [
            ("content", json!("x".repeat(64001))),
            ("expectedRevision", json!("stale")),
            ("documentId", json!("../outside")),
        ] {
            let mut bad = base.clone();
            bad["action"]["request"][key] = value;
            assert!(
                crate::json::decode::<Request>(bad)
                    .unwrap()
                    .validate()
                    .is_err()
            );
        }
    }
    fn request() -> Request {
        crate::json::decode(json!({"id":"a".repeat(64),"sessionStateSha256":"b".repeat(64),"project":"default","action":{"kind":"createMemory","name":"owned","content":"original"}})).unwrap()
    }
    #[test]
    fn expired_queued_action_cannot_begin_after_caller_timeout() {
        let queue = Mailbox::new();
        let client = queue.client.clone();
        let thread = std::thread::spawn(move || {
            client.call(
                request(),
                Deadline::new(50),
                Arc::new(AtomicBool::new(false)),
                None,
            )
        });
        let reply = thread.join().unwrap();
        assert_eq!(reply.status, 408);
        assert_eq!(reply.value["notApplied"], true);
        let pending = queue.next().unwrap();
        assert!(!pending.begin());
    }
    #[test]
    fn running_action_timeout_is_reported_as_unknown_and_cancels_its_deadline() {
        let queue = Mailbox::new();
        let client = queue.client.clone();
        let thread = std::thread::spawn(move || {
            client.call(
                request(),
                Deadline::new(200),
                Arc::new(AtomicBool::new(false)),
                None,
            )
        });
        let until = std::time::Instant::now() + Duration::from_secs(2);
        let pending = loop {
            if let Some(pending) = queue.next() {
                break pending;
            }
            assert!(std::time::Instant::now() < until);
            std::thread::yield_now();
        };
        assert!(pending.begin());
        let reply = thread.join().unwrap();
        assert_eq!(reply.value["notApplied"], false);
        assert!(pending.cancel.load(Ordering::Acquire));
    }
    #[test]
    fn management_envelope_and_actions_reject_unknown_authority_and_wrong_revisions() {
        let base = json!({"id":"a".repeat(64),"sessionStateSha256":"b".repeat(64),"project":"default","action":{"kind":"writeMemory","name":"owned","content":"body","expectedRevision":"c".repeat(64)}});
        let valid: Request = crate::json::decode(base.clone()).unwrap();
        valid.validate().unwrap();
        for pointer in ["/root", "/action/scope", "/action/command"] {
            let mut value = base.clone();
            if let Some(key) = pointer.strip_prefix("/action/") {
                value["action"][key] = json!("global");
            } else {
                value["root"] = json!("C:/outside");
            }
            assert!(crate::json::decode::<Request>(value).is_err());
        }
        let mut bad = base;
        bad["action"]["expectedRevision"] = json!("bad");
        assert!(
            crate::json::decode::<Request>(bad)
                .unwrap()
                .validate()
                .is_err()
        );
    }
}
