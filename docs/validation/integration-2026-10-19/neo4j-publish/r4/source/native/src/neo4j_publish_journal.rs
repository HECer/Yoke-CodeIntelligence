//! Append-only approval/intent/confirmation journal. Torn or unknown states stop writes.
use super::{Operation, Plan};
use crate::{capture::Deadline, model::identity};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path},
    time::{SystemTime, UNIX_EPOCH},
};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Phase {
    Approval,
    Pending,
    Confirmed,
    Recovered,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Event {
    schema_version: u32,
    sequence: usize,
    previous_hash: Option<String>,
    hash: String,
    plan_id: String,
    phase: Phase,
    operation: Option<Operation>,
    response_hash: Option<String>,
    created_at_ms: u64,
    expires_at_ms: u64,
}
impl Event {
    fn identity(&self) -> String {
        let mut value = self.clone();
        value.hash.clear();
        identity(&value)
    }
}
pub(super) struct Journal {
    file: File,
    events: Vec<Event>,
    _directories: Vec<File>,
}
#[cfg(all(test, windows))]
#[path = "neo4j_publish_journal_tests.rs"]
mod tests;
pub(super) fn now() -> Result<u64> {
    u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| anyhow::anyhow!("neo4j_publish_clock_invalid"))?
            .as_millis(),
    )
    .map_err(|_| anyhow::anyhow!("neo4j_publish_clock_invalid"))
}
fn valid(ok: bool) -> Result<()> {
    ensure!(ok, "neo4j_publish_journal_invalid");
    Ok(())
}
fn io<T>(r: std::io::Result<T>) -> Result<T> {
    r.map_err(|_| anyhow::anyhow!("neo4j_publish_journal_unavailable"))
}
fn path_valid(path: &Path, plan: &Plan) -> Result<()> {
    valid(
        path.is_absolute()
            && path
                .to_str()
                .is_some_and(|s| s.len() <= 4096 && !s.chars().any(char::is_control)),
    )?;
    valid(
        path.file_name().and_then(|n| n.to_str())
            == Some(format!("neo4j-{}.jsonl", plan.id()).as_str()),
    )?;
    for c in path.components() {
        match c {
            Component::ParentDir | Component::CurDir => valid(false)?,
            Component::Normal(n) => crate::paths::normalized_syntax(
                n.to_str()
                    .ok_or_else(|| anyhow::anyhow!("neo4j_publish_journal_invalid"))?,
            )
            .map_err(|_| anyhow::anyhow!("neo4j_publish_journal_invalid"))?,
            #[cfg(windows)]
            Component::Prefix(p) => valid(matches!(
                p.kind(),
                std::path::Prefix::Disk(_) | std::path::Prefix::VerbatimDisk(_)
            ))?,
            _ => {}
        }
    }
    Ok(())
}
fn directories(path: &Path, d: &Deadline) -> Result<Vec<File>> {
    let mut held = Vec::new();
    for ancestor in path.ancestors() {
        d.check()?;
        let meta = io(fs::symlink_metadata(ancestor))?;
        valid(meta.is_dir() && !crate::paths::is_link(&meta))?;
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options
                .share_mode(3)
                .custom_flags(0x0200_0000 | 0x0020_0000);
        }
        let file = io(options.open(ancestor))?;
        let meta = io(file.metadata())?;
        valid(meta.is_dir() && !crate::paths::is_link(&meta))?;
        held.push(file);
    }
    Ok(held)
}
fn single_link(file: &File) -> Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        #[repr(C)]
        #[derive(Default)]
        struct Info {
            attributes: u32,
            created: [u32; 2],
            accessed: [u32; 2],
            written: [u32; 2],
            volume: u32,
            size_high: u32,
            size_low: u32,
            links: u32,
            index_high: u32,
            index_low: u32,
        }
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetFileInformationByHandle(handle: *mut std::ffi::c_void, info: *mut Info) -> i32;
        }
        let mut info = Info::default();
        // Live handle and the documented Windows C layout remain valid for this call.
        valid(
            unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } != 0
                && info.links == 1,
        )?;
    }
    #[cfg(not(windows))]
    {
        let _ = file;
        anyhow::bail!("neo4j_publish_guarded_journal_platform_unavailable");
    }
    #[cfg(windows)]
    Ok(())
}
impl Journal {
    pub(super) fn open(path: &Path, plan: &Plan, create: bool, d: &Deadline) -> Result<Self> {
        ensure!(
            cfg!(windows),
            "neo4j_publish_guarded_journal_platform_unavailable"
        );
        plan.validate(d)?;
        path_valid(path, plan)?;
        let held = directories(
            path.parent()
                .ok_or_else(|| anyhow::anyhow!("neo4j_publish_journal_invalid"))?,
            d,
        )?;
        let mut options = OpenOptions::new();
        options.read(true).append(true).create_new(create);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(3).custom_flags(0x0020_0000);
        }
        let mut file = io(options.open(path))?;
        fs2::FileExt::try_lock_exclusive(&file)
            .map_err(|_| anyhow::anyhow!("neo4j_publish_journal_busy"))?;
        let meta = io(file.metadata())?;
        valid(meta.is_file() && !crate::paths::is_link(&meta) && meta.len() <= 65536)?;
        single_link(&file)?;
        let mut bytes = Vec::new();
        io((&mut file).take(65537).read_to_end(&mut bytes))?;
        valid(bytes.len() <= 65536 && (bytes.is_empty() || bytes.last() == Some(&b'\n')))?;
        let mut events = Vec::new();
        let lines = bytes.strip_suffix(b"\n").unwrap_or(&bytes);
        for line in lines.split(|b| *b == b'\n') {
            if bytes.is_empty() {
                break;
            }
            d.check()?;
            valid(events.len() < 16)?;
            let event: Event = crate::json::parse(line, 8192)
                .and_then(crate::json::decode)
                .map_err(|_| anyhow::anyhow!("neo4j_publish_journal_invalid"))?;
            valid(
                event.schema_version == 1
                    && event.plan_id == plan.id()
                    && event.sequence == events.len()
                    && event.previous_hash == events.last().map(|e: &Event| e.hash.clone())
                    && event.hash == event.identity()
                    && event.created_at_ms > 0
                    && (!matches!(event.phase, Phase::Approval | Phase::Pending)
                        || event.created_at_ms < event.expires_at_ms),
            )?;
            if let Some(first) = events.first() {
                let first: &Event = first;
                valid(
                    event.expires_at_ms == first.expires_at_ms
                        && event.created_at_ms >= events.last().unwrap().created_at_ms,
                )?;
            } else {
                valid(
                    event.phase == Phase::Approval
                        && event.operation.is_none()
                        && event.response_hash.is_none()
                        && event.expires_at_ms - event.created_at_ms <= 3_600_000,
                )?;
            }
            events.push(event);
        }
        let journal = Self {
            file,
            events,
            _directories: held,
        };
        journal.validate_progress(plan)?;
        if !create {
            valid(!journal.events.is_empty())?;
        }
        d.check()?;
        Ok(journal)
    }
    fn validate_progress(&self, plan: &Plan) -> Result<()> {
        let ops = super::operations(plan.mode());
        let mut next = 0;
        let mut pending = None;
        for event in self.events.iter().skip(1) {
            match event.phase {
                Phase::Pending => {
                    valid(
                        pending.is_none()
                            && ops.get(next) == event.operation.as_ref()
                            && event.response_hash.is_none(),
                    )?;
                    pending = event.operation;
                }
                Phase::Confirmed | Phase::Recovered => {
                    valid(
                        pending.is_some()
                            && pending == event.operation
                            && event
                                .response_hash
                                .as_deref()
                                .is_some_and(crate::model::valid_id),
                    )?;
                    next += 1;
                    pending = None;
                }
                Phase::Approval => valid(false)?,
            }
        }
        Ok(())
    }
    fn append(
        &mut self,
        plan: &Plan,
        phase: Phase,
        operation: Option<Operation>,
        response_hash: Option<String>,
        expires: u64,
        d: &Deadline,
    ) -> Result<()> {
        d.check()?;
        let at = now()?;
        valid(
            self.events.len() < 16
                && (!matches!(phase, Phase::Approval | Phase::Pending) || at < expires)
                && self.events.last().is_none_or(|e| at >= e.created_at_ms),
        )?;
        let mut event = Event {
            schema_version: 1,
            sequence: self.events.len(),
            previous_hash: self.events.last().map(|e| e.hash.clone()),
            hash: String::new(),
            plan_id: plan.id().into(),
            phase,
            operation,
            response_hash,
            created_at_ms: at,
            expires_at_ms: expires,
        };
        event.hash = event.identity();
        let mut bytes = crate::json::encode_bounded(&event, 8192, d)?;
        bytes.push(b'\n');
        valid(
            io(self.file.metadata())?
                .len()
                .checked_add(bytes.len() as u64)
                .is_some_and(|n| n <= 65536),
        )?;
        single_link(&self.file)?;
        io(self.file.write_all(&bytes))?;
        io(self.file.sync_all())?;
        self.events.push(event);
        self.validate_progress(plan)?;
        Ok(())
    }
    pub(super) fn approve(&mut self, plan: &Plan, expires: u64, d: &Deadline) -> Result<()> {
        valid(self.events.is_empty())?;
        let at = now()?;
        valid(expires > at && expires - at <= 3_600_000)?;
        self.append(plan, Phase::Approval, None, None, expires, d)
    }
    pub(super) fn writable(&self) -> Result<()> {
        let first = self
            .events
            .first()
            .ok_or_else(|| anyhow::anyhow!("neo4j_publish_approval_required"))?;
        let at = now()?;
        ensure!(
            at >= self.events.last().unwrap().created_at_ms && at < first.expires_at_ms,
            "neo4j_publish_approval_expired_or_clock_regressed"
        );
        ensure!(
            self.pending().is_none(),
            "neo4j_publish_previous_result_unknown_recovery_required"
        );
        Ok(())
    }
    pub(super) fn pending(&self) -> Option<Operation> {
        self.events
            .last()
            .filter(|e| e.phase == Phase::Pending)
            .and_then(|e| e.operation)
    }
    pub(super) fn completed(&self) -> usize {
        self.events
            .iter()
            .filter(|e| matches!(e.phase, Phase::Confirmed | Phase::Recovered))
            .count()
    }
    pub(super) fn intent(&mut self, plan: &Plan, op: Operation, d: &Deadline) -> Result<()> {
        self.writable()?;
        valid(super::operations(plan.mode()).get(self.completed()) == Some(&op))?;
        self.append(
            plan,
            Phase::Pending,
            Some(op),
            None,
            self.events[0].expires_at_ms,
            d,
        )
    }
    pub(super) fn confirmed(
        &mut self,
        plan: &Plan,
        op: Operation,
        response: &Value,
        recovered: bool,
        d: &Deadline,
    ) -> Result<()> {
        valid(self.pending() == Some(op))?;
        // Confirmation is mandatory cleanup after the request stops. The caller
        // supplies a separate tiny local deadline, never extending network time.
        let expires = self.events[0].expires_at_ms;
        self.append(
            plan,
            if recovered {
                Phase::Recovered
            } else {
                Phase::Confirmed
            },
            Some(op),
            Some(identity(response)),
            expires,
            d,
        )
    }
    pub(super) fn summary(&self, plan: &Plan) -> Value {
        json!({"kind":"native_neo4j_journal_v1","planId":plan.id(),"eventCount":self.events.len(),"lastEventHash":self.events.last().map(|e|&e.hash),"completedOperations":self.completed(),"totalOperations":super::operations(plan.mode()).len(),"pendingOperation":self.pending(),"approvalExpiresAtMs":self.events.first().map(|e|e.expires_at_ms),"complete":false,"sourceAuthority":false})
    }
}
