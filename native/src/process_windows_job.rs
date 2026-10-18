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
    fn empty(&self) -> io::Result<bool> {
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
        Ok(info.active_processes == 0)
    }
    pub(super) fn terminate_until(&self, end: Instant) -> Result<()> {
        // Termination is asynchronous. Neither its success nor root exit alone
        // proves that the complete owned job has stopped.
        let termination_requested = unsafe { TerminateJobObject(self.job.as_raw_handle(), 1) } != 0;
        // Assignment can fail before the root enters this job. Its independently
        // held handle must still be terminated even if job termination refuses.
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
        loop {
            let remaining = end.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(
                    CleanupUnproven::new("Owned group exit exceeded cleanup budget").into(),
                );
            }
            let root_exited = self
                .root_exited()
                .map_err(|_| CleanupUnproven::new("Owned root state unavailable"))?;
            let empty = self
                .empty()
                .map_err(|_| CleanupUnproven::new("Owned job accounting unavailable"))?;
            if root_exited && empty && Instant::now() < end {
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
