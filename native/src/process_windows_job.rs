//! Independently owned Windows job accounting, including fallible spawn hooks.
use super::CleanupUnproven;
use anyhow::Result;
use process_wrap::std::{StdChildWrapper, StdCommandWrap, StdCommandWrapper};
use std::{
    ffi::c_void,
    io,
    os::windows::io::{AsHandle, AsRawHandle, FromRawHandle, OwnedHandle},
    process::Child,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

type Handle = *mut c_void;
#[link(name = "kernel32")]
unsafe extern "system" {
    fn CreateJobObjectW(attributes: *mut c_void, name: *const u16) -> Handle;
    fn SetInformationJobObject(job: Handle, class: i32, info: *const c_void, size: u32) -> i32;
    fn QueryInformationJobObject(
        job: Handle,
        class: i32,
        info: *mut c_void,
        size: u32,
        returned: *mut u32,
    ) -> i32;
    fn AssignProcessToJobObject(job: Handle, process: Handle) -> i32;
    fn TerminateJobObject(job: Handle, code: u32) -> i32;
    fn TerminateProcess(process: Handle, code: u32) -> i32;
    fn WaitForSingleObject(handle: Handle, milliseconds: u32) -> u32;
    fn OpenProcess(access: u32, inherit: i32, id: u32) -> Handle;
}

#[repr(C)]
#[derive(Default)]
struct BasicLimits {
    process_time: i64,
    job_time: i64,
    flags: u32,
    minimum_working_set: usize,
    maximum_working_set: usize,
    active_process_limit: u32,
    affinity: usize,
    priority: u32,
    scheduling_class: u32,
}
#[repr(C)]
#[derive(Default)]
struct ExtendedLimits {
    basic: BasicLimits,
    io: [u64; 6],
    process_memory: usize,
    job_memory: usize,
    peak_process_memory: usize,
    peak_job_memory: usize,
}
#[repr(C)]
#[derive(Default)]
struct Accounting {
    total_user_time: i64,
    total_kernel_time: i64,
    period_user_time: i64,
    period_kernel_time: i64,
    page_faults: u32,
    total_processes: u32,
    active_processes: u32,
    terminated_processes: u32,
}

const MAX_MEMBERS: usize = 4096;
#[repr(C)]
struct ProcessIds {
    assigned: u32,
    listed: u32,
    ids: [usize; MAX_MEMBERS],
}

#[derive(Debug)]
pub(super) struct OwnedJob {
    job: OwnedHandle,
    root: OnceLock<OwnedHandle>,
    attached: AtomicBool,
}
impl OwnedJob {
    pub(super) fn new() -> io::Result<Arc<Self>> {
        // No name or inheritable security attributes: only this owner can close
        // the last job handle. Parent job resource limits remain in force.
        let handle = unsafe { CreateJobObjectW(std::ptr::null_mut(), std::ptr::null()) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: CreateJobObjectW returned a new, non-null, owned handle.
        let job = unsafe { OwnedHandle::from_raw_handle(handle) };
        let limits = ExtendedLimits {
            basic: BasicLimits {
                flags: 0x2000,
                ..Default::default()
            }, // KILL_ON_JOB_CLOSE
            ..Default::default()
        };
        if unsafe {
            SetInformationJobObject(
                job.as_raw_handle(),
                9,
                std::ptr::from_ref(&limits).cast(),
                std::mem::size_of::<ExtendedLimits>() as u32,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(Arc::new(Self {
            job,
            root: OnceLock::new(),
            attached: AtomicBool::new(false),
        }))
    }
    pub(super) fn attach(self: &Arc<Self>, wrapper: &mut StdCommandWrap) {
        wrapper.wrap(Attach(Arc::clone(self)));
    }
    pub(super) fn spawn_checked(
        &self,
        wrapper: &mut StdCommandWrap,
    ) -> Result<Box<dyn StdChildWrapper>> {
        anyhow::ensure!(
            self.root.get().is_none(),
            "Owned job cannot spawn a second root"
        );
        match wrapper.spawn() {
            Ok(child) => Ok(child),
            Err(error) => {
                if self.root.get().is_some() {
                    self.terminate_until(Instant::now() + Duration::from_secs(2))?;
                }
                Err(error.into())
            }
        }
    }
    fn root_exited(&self) -> io::Result<bool> {
        let root = self
            .root
            .get()
            .ok_or_else(|| io::Error::other("Owned root handle missing"))?;
        // SAFETY: root is an independently owned, live process handle.
        match unsafe { WaitForSingleObject(root.as_raw_handle(), 0) } {
            0 => Ok(true),
            258 => Ok(false),
            _ => Err(io::Error::last_os_error()),
        }
    }
    fn accounting(&self) -> io::Result<Accounting> {
        let mut info = Accounting::default();
        // SAFETY: info has the Win32 accounting layout and exact buffer size.
        if unsafe {
            QueryInformationJobObject(
                self.job.as_raw_handle(),
                1,
                std::ptr::from_mut(&mut info).cast(),
                std::mem::size_of::<Accounting>() as u32,
                std::ptr::null_mut(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(info)
    }
    fn empty(&self) -> io::Result<bool> {
        Ok(self.accounting()?.active_processes == 0)
    }
    fn member_list_refusal(
        &self,
        detail: &str,
        info: &ProcessIds,
        returned_bytes: u32,
        query_succeeded: bool,
        query_error: Option<i32>,
    ) -> CleanupUnproven {
        // These bounded numeric observations follow the list query. They are
        // not atomic with that query and never turn an incomplete list into
        // proof. Failed-query buffer fields are only initialized/partial data.
        let accounting_after_list = self
            .accounting()
            .map(|info| {
                (
                    info.total_processes,
                    info.active_processes,
                    info.terminated_processes,
                )
            })
            .map_err(|error| error.raw_os_error());
        let root_exited_after_list = self.root_exited().map_err(|error| error.raw_os_error());
        CleanupUnproven::new(format!(
            "{detail}; memberSnapshot {{ query_succeeded: {query_succeeded}, assigned: {}, listed: {}, capacity: {MAX_MEMBERS}, returned_bytes: {returned_bytes}, query_error: {query_error:?}, accounting_after_list: {accounting_after_list:?}, root_exited_after_list: {root_exited_after_list:?} }}",
            info.assigned, info.listed
        ))
    }
    fn member_handles(&self, end: Instant) -> Result<Vec<OwnedHandle>> {
        anyhow::ensure!(
            Instant::now() < end,
            CleanupUnproven::new("Owned member snapshot exceeded cleanup budget")
        );
        let mut info = ProcessIds {
            assigned: 0,
            listed: 0,
            ids: [0; MAX_MEMBERS],
        };
        let mut returned_bytes = 0;
        // The OS list includes nested child jobs. Fixed storage bounds both the
        // protocol and synchronization handles; an incomplete list is not proof.
        if unsafe {
            QueryInformationJobObject(
                self.job.as_raw_handle(),
                3,
                std::ptr::from_mut(&mut info).cast(),
                std::mem::size_of::<ProcessIds>() as u32,
                &mut returned_bytes,
            )
        } == 0
        {
            // GetLastError must be captured before the diagnostic queries.
            let query_error = io::Error::last_os_error().raw_os_error();
            return Err(self
                .member_list_refusal(
                    "Owned member list unavailable or over capacity",
                    &info,
                    returned_bytes,
                    false,
                    query_error,
                )
                .into());
        }
        anyhow::ensure!(
            info.assigned == info.listed && info.listed as usize <= MAX_MEMBERS,
            self.member_list_refusal(
                "Owned member list incomplete or over capacity",
                &info,
                returned_bytes,
                true,
                None,
            )
        );
        let mut handles = Vec::with_capacity(info.listed as usize);
        for id in &info.ids[..info.listed as usize] {
            anyhow::ensure!(
                Instant::now() < end,
                CleanupUnproven::new("Owned member snapshot exceeded cleanup budget")
            );
            let id = u32::try_from(*id)
                .ok()
                .filter(|id| *id != 0)
                .ok_or_else(|| CleanupUnproven::new("Owned member identity unavailable"))?;
            // Synchronization only: never terminate a process by a recycled PID.
            // PID reuse can conservatively extend a wait or refuse proof; it
            // cannot terminate an unrelated process. The job owns termination.
            let handle = unsafe { OpenProcess(0x0010_0000, 0, id) };
            if handle.is_null() {
                // With fixed valid access and a nonzero OS-supplied PID, this
                // error means the process no longer exists. Its object exited.
                if io::Error::last_os_error().raw_os_error() == Some(87) {
                    continue;
                }
                return Err(CleanupUnproven::new(
                    "Owned member synchronization handle unavailable",
                )
                .into());
            }
            // SAFETY: OpenProcess returned a new non-null owned handle.
            handles.push(unsafe { OwnedHandle::from_raw_handle(handle) });
        }
        Ok(handles)
    }
    pub(super) fn terminate_until(&self, end: Instant) -> Result<()> {
        // Retain member identities before asynchronous termination can remove
        // them from job accounting while their process objects remain unsignaled.
        let members = self.member_handles(end);
        // Always request termination, including when member admission refuses.
        let termination_requested = unsafe { TerminateJobObject(self.job.as_raw_handle(), 1) } != 0;
        if !self.attached.load(Ordering::SeqCst)
            && !self
                .root_exited()
                .map_err(|_| CleanupUnproven::new("Owned root state unavailable"))?
        {
            let root = self
                .root
                .get()
                .expect("root_exited checked the retained handle");
            if unsafe { TerminateProcess(root.as_raw_handle(), 1) } == 0
                && !self
                    .root_exited()
                    .map_err(|_| CleanupUnproven::new("Owned root state unavailable"))?
            {
                return Err(
                    CleanupUnproven::new("Unassigned owned root termination failed").into(),
                );
            }
        }
        let members = members?;
        loop {
            let remaining = end.saturating_duration_since(Instant::now());
            anyhow::ensure!(
                !remaining.is_zero(),
                CleanupUnproven::new("Owned group exit exceeded cleanup budget")
            );
            let root_exited = self
                .root_exited()
                .map_err(|_| CleanupUnproven::new("Owned root state unavailable"))?;
            let empty = self
                .empty()
                .map_err(|_| CleanupUnproven::new("Owned job accounting unavailable"))?;
            let mut members_exited = true;
            for member in &members {
                // Live independently owned process handles, zero-time polling.
                match unsafe { WaitForSingleObject(member.as_raw_handle(), 0) } {
                    0 => {}
                    258 => members_exited = false,
                    _ => {
                        return Err(
                            CleanupUnproven::new("Owned member exit state unavailable").into()
                        );
                    }
                }
            }
            if root_exited && empty && members_exited && Instant::now() < end {
                return Ok(());
            }
            if !termination_requested && !empty {
                return Err(CleanupUnproven::new("Owned job termination request failed").into());
            }
            std::thread::sleep(
                end.saturating_duration_since(Instant::now())
                    .min(Duration::from_millis(5)),
            );
        }
    }
}

#[derive(Debug)]
struct Attach(Arc<OwnedJob>);
impl StdCommandWrapper for Attach {
    fn post_spawn(&mut self, child: &mut Child, _: &StdCommandWrap) -> io::Result<()> {
        // JobObject's pre_spawn made this root suspended. Retain its handle
        // before fallible assignment or any later wrap_child can discard Child.
        let handle = match child.as_handle().try_clone_to_owned() {
            Ok(handle) => handle,
            Err(error) => {
                cleanup_unretained_root(child)?;
                return Err(error);
            }
        };
        self.0
            .root
            .set(handle)
            .map_err(|_| io::Error::other("Owned job reused for a second root"))?;
        if unsafe { AssignProcessToJobObject(self.0.job.as_raw_handle(), child.as_raw_handle()) }
            == 0
        {
            return Err(io::Error::last_os_error());
        }
        self.0.attached.store(true, Ordering::SeqCst);
        Ok(())
    }
}

fn cleanup_unretained_root(child: &mut Child) -> io::Result<()> {
    let end = Instant::now() + Duration::from_secs(2);
    child.kill().map_err(|_| {
        io::Error::other(CleanupUnproven::new(
            "Unretained owned root termination failed",
        ))
    })?;
    loop {
        let remaining = end.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(io::Error::other(CleanupUnproven::new(
                "Unretained owned root exit exceeded cleanup budget",
            )));
        }
        match child.try_wait() {
            Ok(Some(_)) => return Ok(()),
            Err(_) => {
                return Err(io::Error::other(CleanupUnproven::new(
                    "Unretained owned root state unavailable",
                )));
            }
            Ok(None) => std::thread::sleep(remaining.min(Duration::from_millis(5))),
        }
    }
}

#[cfg(test)]
#[path = "process_windows_job_tests.rs"]
mod history_spawn_tests;
