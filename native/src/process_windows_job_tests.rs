//! Held process handles observe cleanup across actual fallible spawn hooks.
use super::*;
use std::{
    os::windows::process::CommandExt,
    process::{Command, Stdio},
};

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCurrentProcess() -> Handle;
    fn DuplicateHandle(
        source_process: Handle,
        source: Handle,
        target_process: Handle,
        target: *mut Handle,
        access: u32,
        inherit: i32,
        options: u32,
    ) -> i32;
}

#[derive(Debug)]
struct Observe(Arc<OnceLock<OwnedHandle>>);
impl StdCommandWrapper for Observe {
    fn post_spawn(&mut self, child: &mut Child, _: &StdCommandWrap) -> io::Result<()> {
        self.0
            .set(child.as_handle().try_clone_to_owned().unwrap())
            .unwrap();
        Ok(())
    }
}
#[derive(Debug)]
struct RefuseWrapping;
impl StdCommandWrapper for RefuseWrapping {
    fn wrap_child(
        &mut self,
        _: Box<dyn StdChildWrapper>,
        _: &StdCommandWrap,
    ) -> io::Result<Box<dyn StdChildWrapper>> {
        Err(io::Error::other("Owned fixture wrapping refusal"))
    }
}
fn command(root: &std::path::Path) -> Command {
    let mut command = Command::new("node");
    command
        .current_dir(root)
        .args(["-e", "setInterval(()=>{},1000)"])
        .creation_flags(0x0800_0000)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}
fn wrapper(
    root: &std::path::Path,
    job: &Arc<OwnedJob>,
    observer: &Arc<OnceLock<OwnedHandle>>,
) -> StdCommandWrap {
    let mut wrapper = StdCommandWrap::from(command(root));
    let mut flags = process_wrap::std::CreationFlags(Default::default());
    flags.0.0 = 0x0800_0000;
    wrapper.wrap(flags).wrap(Observe(Arc::clone(observer)));
    job.attach(&mut wrapper);
    wrapper.wrap(process_wrap::std::JobObject);
    wrapper
}
fn assert_exited(handle: &OwnedHandle) {
    assert_eq!(
        unsafe { WaitForSingleObject(handle.as_raw_handle(), 0) },
        0,
        "The independently held owned-process handle must be signaled before return"
    );
}

#[test]
fn assignment_failure_reaps_the_unassigned_suspended_root_and_its_owned_job() {
    struct Blocker(Child);
    impl Drop for Blocker {
        fn drop(&mut self) {
            let _ = self.0.kill();
        }
    }
    let root = tempfile::tempdir().unwrap();
    let job = OwnedJob::new().unwrap();
    let blocker = Blocker(command(root.path()).spawn().unwrap());
    let blocker_handle = blocker.0.as_handle().try_clone_to_owned().unwrap();
    assert_ne!(
        unsafe { AssignProcessToJobObject(job.job.as_raw_handle(), blocker.0.as_raw_handle()) },
        0
    );
    let limits = ExtendedLimits {
        basic: BasicLimits {
            flags: 0x2000 | 0x8,
            active_process_limit: 1,
            ..Default::default()
        },
        ..Default::default()
    };
    assert_ne!(
        unsafe {
            SetInformationJobObject(
                job.job.as_raw_handle(),
                9,
                std::ptr::from_ref(&limits).cast(),
                std::mem::size_of::<ExtendedLimits>() as u32,
            )
        },
        0
    );
    let observed = Arc::new(OnceLock::new());
    let mut wrapper = wrapper(root.path(), &job, &observed);
    let started = Instant::now();
    let result = job.spawn_checked(&mut wrapper);
    assert!(
        result.is_err(),
        "The active-process limit must refuse the second root"
    );
    assert!(
        !super::super::cleanup_unproven(&result.err().unwrap()),
        "A proven cleanup must preserve the original assignment refusal"
    );
    assert_exited(observed.get().unwrap());
    assert_exited(&blocker_handle);
    assert!(job.empty().unwrap());
    assert!(started.elapsed() < Duration::from_secs(3));
}

#[test]
fn a_later_wrapper_failure_proves_root_exit_before_returning_the_spawn_error() {
    let root = tempfile::tempdir().unwrap();
    let job = OwnedJob::new().unwrap();
    let observed = Arc::new(OnceLock::new());
    let mut wrapper = wrapper(root.path(), &job, &observed);
    wrapper.wrap(RefuseWrapping);
    let started = Instant::now();
    let result = job.spawn_checked(&mut wrapper);
    let error = result
        .err()
        .expect("The owned wrapping fixture must refuse");
    assert!(error.to_string().contains("Owned fixture wrapping refusal"));
    assert!(!super::super::cleanup_unproven(&error));
    assert_exited(observed.get().unwrap());
    assert!(job.empty().unwrap());
    assert!(started.elapsed() < Duration::from_secs(3));
}

#[test]
fn assignment_and_job_termination_refusals_still_reap_the_unassigned_root() {
    let root = tempfile::tempdir().unwrap();
    let original = OwnedJob::new().unwrap();
    let mut limited = std::ptr::null_mut();
    // This real handle permits accounting only. Assignment and job termination
    // must both be refused by Windows; root ownership remains independent.
    assert_ne!(
        unsafe {
            DuplicateHandle(
                GetCurrentProcess(),
                original.job.as_raw_handle(),
                GetCurrentProcess(),
                &mut limited,
                0x4,
                0,
                0,
            )
        },
        0
    );
    let job = Arc::new(OwnedJob {
        job: unsafe { OwnedHandle::from_raw_handle(limited) },
        root: OnceLock::new(),
        attached: AtomicBool::new(false),
    });
    drop(original);
    let observed = Arc::new(OnceLock::new());
    let mut wrapper = wrapper(root.path(), &job, &observed);
    let started = Instant::now();
    let error = job
        .spawn_checked(&mut wrapper)
        .err()
        .expect("Windows must refuse assignment through the query-only handle");
    assert_exited(observed.get().unwrap());
    assert!(job.empty().unwrap());
    assert!(
        !super::super::cleanup_unproven(&error),
        "The original assignment refusal must follow independently proven root cleanup"
    );
    assert!(started.elapsed() < Duration::from_secs(3));
}

#[test]
fn zero_cleanup_budget_requests_termination_and_refuses_exit_proof() {
    let root = tempfile::tempdir().unwrap();
    let child = super::super::ManagedChild::spawn(command(root.path())).unwrap();
    let observed = child.inner().as_handle().try_clone_to_owned().unwrap();
    let started = Instant::now();
    let error = child.terminate_checked(Duration::ZERO).unwrap_err();
    assert!(super::super::cleanup_unproven(&error));
    assert!(started.elapsed() < Duration::from_secs(1));
    assert_eq!(
        unsafe { WaitForSingleObject(observed.as_raw_handle(), 2000) },
        0
    );
}
