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
}
impl Default for Config {
    fn default() -> Self {
        Self {
            session_controls: true,
            memories: true,
            operation_timeout_ms: 30_000,
        }
    }
}
impl Config {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            (100..=60_000).contains(&self.operation_timeout_ms),
            "dashboard_management_timeout_capacity"
        );
        Ok(())
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
    ResetModes,
    ActivateProject {
        project: String,
    },
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
