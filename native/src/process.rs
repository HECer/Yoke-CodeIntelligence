use anyhow::{Context, Result, ensure};
use process_wrap::std::{StdChildWrapper, StdCommandWrap};
use std::{
    io::{Read, Write},
    ops::{Deref, DerefMut},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

#[cfg(windows)]
#[path = "process_windows_job.rs"]
mod windows_job;

#[cfg(test)]
#[path = "process_cleanup_tests.rs"]
mod cleanup_tests;

#[derive(Debug)]
pub(crate) struct CleanupUnproven(String);
impl CleanupUnproven {
    pub(crate) fn new(detail: impl Into<String>) -> Self {
        Self(detail.into())
    }
}
impl std::fmt::Display for CleanupUnproven {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Owned process cleanup could not be proven: {}", self.0)
    }
}
impl std::error::Error for CleanupUnproven {}
pub(crate) fn cleanup_unproven(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        let mut nested = Some(cause);
        while let Some(error) = nested {
            if error.is::<CleanupUnproven>() {
                return true;
            }
            // io::Error does not necessarily expose its boxed error itself in
            // source(). Fallible spawn hooks preserve that typed inner cause.
            nested = error
                .downcast_ref::<std::io::Error>()
                .and_then(std::io::Error::get_ref)
                .map(|inner| inner as &(dyn std::error::Error + 'static));
        }
        false
    })
}

#[derive(Clone, Copy, Debug)]
pub struct CaptureLimits {
    pub stdin_bytes: usize,
    pub stdout_bytes: usize,
    pub stderr_bytes: usize,
}

#[derive(Debug)]
pub struct CapturedOutput {
    pub exit_code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub stdin_complete: bool,
    pub output_complete: bool,
    pub timed_out: bool,
    pub output_exceeded: bool,
    pub io_error: bool,
}

pub fn run_capture(
    command: Command,
    input: &[u8],
    deadline: &crate::capture::Deadline,
    limits: CaptureLimits,
) -> Result<CapturedOutput> {
    capture(command, input, deadline, limits, None)
}
/// Bound committed memory on Windows Jobs, or address space on Unix, before execution.
/// An unavailable OS limit fails spawn rather than running without the limit.
pub fn run_capture_with_memory_limit(
    command: Command,
    input: &[u8],
    deadline: &crate::capture::Deadline,
    limits: CaptureLimits,
    memory_bytes: usize,
) -> Result<CapturedOutput> {
    capture(command, input, deadline, limits, Some(memory_bytes))
}
fn capture(
    mut command: Command,
    input: &[u8],
    deadline: &crate::capture::Deadline,
    limits: CaptureLimits,
    memory_bytes: Option<usize>,
) -> Result<CapturedOutput> {
    deadline.check()?;
    ensure!(
        command.get_current_dir().is_some(),
        "Host process requires an explicit working directory"
    );
    ensure!(
        command.get_args().count() <= 200,
        "Host argument capacity exceeded"
    );
    ensure!(
        limits.stdin_bytes <= 16_000_000
            && (1..=32_000_000).contains(&limits.stdout_bytes)
            && (1..=8_000_000).contains(&limits.stderr_bytes),
        "Invalid host process byte capacities"
    );
    ensure!(
        input.len() <= limits.stdin_bytes,
        "Host process stdin capacity exceeded"
    );
    let explicit: Vec<_> = command
        .get_envs()
        .map(|(key, value)| (key.to_os_string(), value.map(|value| value.to_os_string())))
        .collect();
    ensure!(
        explicit.len() <= 64
            && explicit.iter().all(|(key, value)| key.len() <= 200
                && value.as_ref().is_none_or(|value| value.len() <= 64_000)),
        "Explicit host environment capacity exceeded"
    );
    command.env_clear();
    for name in [
        "PATH",
        "SystemRoot",
        "SystemDrive",
        "WINDIR",
        "USERPROFILE",
        "HOME",
        "TEMP",
        "TMP",
        "LANG",
        "RUSTUP_HOME",
        "CARGO_HOME",
    ] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    command
        .env("CI", "1")
        .env("DO_NOT_TRACK", "1")
        .env("NO_COLOR", "1");
    for (key, value) in explicit {
        if let Some(value) = value {
            command.env(key, value);
        } else {
            command.env_remove(key);
        }
    }
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = ManagedChild::spawn_limited(command, memory_bytes)
        .context("Host executable unavailable")?;
    let (Some(mut stdin), Some(stdout), Some(stderr)) = (
        child.stdin().take(),
        child.stdout().take(),
        child.stderr().take(),
    ) else {
        child.cleanup_checked(Duration::from_secs(2))?;
        anyhow::bail!("Host capture pipes unavailable");
    };
    let (done, completed) = mpsc::sync_channel::<(usize, std::io::Result<Vec<u8>>)>(3);
    let exceeded = Arc::new(AtomicBool::new(false));
    let input = input.to_vec();
    let writer_done = done.clone();
    thread::spawn(move || {
        let result = stdin
            .write_all(&input)
            .and_then(|()| stdin.flush())
            .map(|()| Vec::new());
        drop(stdin);
        let _ = writer_done.send((2, result));
    });
    for (stream, mut reader, limit) in [
        (
            0,
            Box::new(stdout) as Box<dyn Read + Send>,
            limits.stdout_bytes,
        ),
        (
            1,
            Box::new(stderr) as Box<dyn Read + Send>,
            limits.stderr_bytes,
        ),
    ] {
        let reader_done = done.clone();
        let reader_exceeded = Arc::clone(&exceeded);
        thread::spawn(move || {
            let result = (|| {
                let mut retained = Vec::new();
                let mut buffer = [0u8; 8192];
                loop {
                    let count = reader.read(&mut buffer)?;
                    if count == 0 {
                        return Ok(retained);
                    }
                    let keep = count.min(limit.saturating_sub(retained.len()));
                    retained.extend_from_slice(&buffer[..keep]);
                    if keep < count {
                        reader_exceeded.store(true, Ordering::SeqCst);
                    }
                }
            })();
            let _ = reader_done.send((stream, result));
        });
    }
    drop(done);
    let mut outputs: [Option<Vec<u8>>; 2] = [None, None];
    let mut stdin_done = false;
    let mut stdin_complete = false;
    let mut io_error = false;
    let mut exit = None;
    let mut timed_out = false;
    #[cfg(windows)]
    let mut group_cleaned = false;
    loop {
        while let Ok((stream, result)) = completed.try_recv() {
            if stream == 2 {
                stdin_done = true;
                stdin_complete = result.is_ok();
            }
            match result {
                Ok(bytes) if stream < 2 => outputs[stream] = Some(bytes),
                Err(_) => {
                    io_error = true;
                    if stream < 2 {
                        outputs[stream] = Some(Vec::new());
                    }
                }
                _ => {}
            }
        }
        if exit.is_none() {
            exit = match child.inner_mut().try_wait() {
                Ok(status) => status,
                Err(error) => {
                    child.cleanup_checked(Duration::from_secs(2))?;
                    return Err(error.into());
                }
            };
        }
        #[cfg(windows)]
        if exit.is_some() && !group_cleaned {
            // Descendants can keep inherited pipe handles open even after the
            // root exits. Terminate the owned job before waiting for pipe EOF;
            // otherwise a completed root can incorrectly consume the deadline.
            child.cleanup_checked(Duration::from_secs(2))?;
            group_cleaned = true;
        }
        if deadline.check().is_err() {
            timed_out = true;
            break;
        }
        if exceeded.load(Ordering::SeqCst) {
            break;
        }
        if stdin_done && outputs.iter().all(Option::is_some) && exit.is_some() {
            break;
        }
        thread::sleep(deadline.remaining().min(Duration::from_millis(5)));
    }
    let output_complete = stdin_done
        && outputs.iter().all(Option::is_some)
        && !io_error
        && !timed_out
        && !exceeded.load(Ordering::SeqCst);
    if !output_complete || exit.is_none() {
        exit = Some(child.cleanup_checked(Duration::from_secs(2))?);
        // Killing the owned group closes pipes. Collect ready bounded data;
        // never wait for arbitrary inherited descriptors after the deadline.
        while let Ok((stream, result)) = completed.try_recv() {
            if stream == 2 {
                stdin_done = true;
                stdin_complete = result.is_ok();
            }
            match result {
                Ok(bytes) if stream < 2 => outputs[stream] = Some(bytes),
                Err(_) => io_error = true,
                _ => {}
            }
        }
    }
    Ok(CapturedOutput {
        exit_code: exit.and_then(|status| status.code()),
        stdout: outputs[0].take().unwrap_or_default(),
        stderr: outputs[1].take().unwrap_or_default(),
        stdin_complete: stdin_done && stdin_complete,
        output_complete,
        timed_out,
        output_exceeded: exceeded.load(Ordering::SeqCst),
        io_error,
    })
}

/// One-way self-limit for a dedicated owned worker, before reading its input.
/// On Unix the hard address-space ceiling cannot subsequently be raised.
pub struct CurrentMemoryLimit {
    #[cfg(windows)]
    _job: std::os::windows::io::OwnedHandle,
}
pub fn limit_current_memory(memory_bytes: usize) -> Result<CurrentMemoryLimit> {
    ensure!(
        (32 * 1024 * 1024..=2 * 1024 * 1024 * 1024).contains(&memory_bytes),
        "Invalid owned memory limit"
    );
    #[cfg(windows)]
    {
        Ok(CurrentMemoryLimit {
            _job: windows_job::limit_current_memory(memory_bytes)?,
        })
    }
    #[cfg(unix)]
    {
        let limit = libc::rlimit {
            rlim_cur: memory_bytes as libc::rlim_t,
            rlim_max: memory_bytes as libc::rlim_t,
        };
        ensure!(
            unsafe { libc::setrlimit(libc::RLIMIT_AS, &limit) } == 0,
            "Owned memory limit unavailable"
        );
        Ok(CurrentMemoryLimit {})
    }
    #[cfg(not(any(windows, unix)))]
    {
        anyhow::bail!("Owned memory limit unavailable on this platform")
    }
}

/// Observe an actual current OS memory ceiling. Missing enforcement is not success.
pub fn current_memory_limit() -> Result<Option<usize>> {
    #[cfg(windows)]
    {
        Ok(windows_job::current_memory_limit()?)
    }
    #[cfg(unix)]
    {
        let mut limit = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        // SAFETY: limit is a writable rlimit; getrlimit has no child effects.
        ensure!(
            unsafe { libc::getrlimit(libc::RLIMIT_AS, &mut limit) } == 0,
            "Owned memory limit observation unavailable"
        );
        let ceiling = limit.rlim_cur.min(limit.rlim_max);
        Ok((ceiling != libc::RLIM_INFINITY)
            .then(|| usize::try_from(ceiling))
            .transpose()?)
    }
    #[cfg(not(any(windows, unix)))]
    {
        anyhow::bail!("Owned memory limit observation unavailable on this platform")
    }
}

pub struct ManagedChild {
    inner: Box<dyn StdChildWrapper>,
    drop_wait: bool,
    #[cfg(windows)]
    job: Arc<windows_job::OwnedJob>,
}
impl ManagedChild {
    pub fn spawn(command: Command) -> Result<Self> {
        Self::spawn_limited(command, None)
    }
    fn spawn_limited(command: Command, memory_bytes: Option<usize>) -> Result<Self> {
        #[cfg(unix)]
        let mut command = command;
        if let Some(bytes) = memory_bytes {
            ensure!(
                (32 * 1024 * 1024..=2 * 1024 * 1024 * 1024).contains(&bytes),
                "Invalid owned memory limit"
            );
            #[cfg(unix)]
            {
                use std::os::unix::process::CommandExt;
                // Only an async-signal-safe system call is made between fork and exec.
                unsafe {
                    command.pre_exec(move || {
                        let limit = libc::rlimit {
                            rlim_cur: bytes as libc::rlim_t,
                            rlim_max: bytes as libc::rlim_t,
                        };
                        if libc::setrlimit(libc::RLIMIT_AS, &limit) != 0 {
                            return Err(std::io::Error::last_os_error());
                        }
                        Ok(())
                    });
                }
            }
            #[cfg(not(any(unix, windows)))]
            anyhow::bail!("Owned memory limit unavailable on this platform");
        }
        let mut wrapper = StdCommandWrap::from(command);
        #[cfg(windows)]
        let job = {
            let job = windows_job::OwnedJob::with_memory_limit(memory_bytes)?;
            // JobObject replaces Command's creation flags. Preserve this flag
            // through the wrapper that it explicitly consults before spawn.
            let mut flags = process_wrap::std::CreationFlags(Default::default());
            flags.0.0 = 0x0800_0000; // CREATE_NO_WINDOW
            wrapper.wrap(flags);
            job.attach(&mut wrapper);
            wrapper.wrap(process_wrap::std::JobObject);
            job
        };
        #[cfg(unix)]
        wrapper.wrap(process_wrap::std::ProcessGroup::leader());
        Ok(Self {
            #[cfg(windows)]
            inner: job.spawn_checked(&mut wrapper)?,
            #[cfg(not(windows))]
            inner: wrapper.spawn()?,
            drop_wait: true,
            #[cfg(windows)]
            job,
        })
    }
    /// Consume an owned process and prove its exit after requesting wrapper
    /// termination. Cleanup uses a separate bounded host budget even when the
    /// request that owned the process has already been cancelled.
    pub fn terminate_checked(mut self, budget: Duration) -> Result<std::process::ExitStatus> {
        self.cleanup_checked(budget)
    }
    fn cleanup_checked(&mut self, budget: Duration) -> Result<std::process::ExitStatus> {
        let end = std::time::Instant::now() + budget.min(Duration::from_secs(2));
        // A consumed cleanup budget must never be followed by another full wait
        // from Drop. Windows kill-on-close remains a termination backup.
        self.drop_wait = false;
        #[cfg(windows)]
        {
            self.job.terminate_until(end)?;
            ensure!(
                !budget.is_zero() && budget <= Duration::from_secs(2),
                CleanupUnproven::new("Invalid owned process cleanup budget")
            );
            self.inner
                .inner_mut()
                .try_wait()
                .map_err(|_| CleanupUnproven::new("Owned process exit status unavailable"))?
                .ok_or_else(|| CleanupUnproven::new("Owned process exit status unproven").into())
        }
        #[cfg(not(windows))]
        {
            if let Err(error) = self.inner.start_kill() {
                // A naturally exited/reaped leader can already have no group.
                // ESRCH alone is not an exit receipt: the raw owned child must
                // still report an actual status below. Other signal failures
                // never become successful cleanup.
                #[cfg(unix)]
                let absent = error.raw_os_error() == Some(libc::ESRCH);
                #[cfg(not(unix))]
                let absent = false;
                ensure!(
                    absent,
                    CleanupUnproven::new(format!(
                        "Owned process termination request failed (os error {:?})",
                        error.raw_os_error()
                    ))
                );
            }
            ensure!(
                !budget.is_zero() && budget <= Duration::from_secs(2),
                CleanupUnproven::new("Invalid owned process cleanup budget")
            );
            loop {
                if let Some(status) =
                    self.inner.inner_mut().try_wait().map_err(|_| {
                        CleanupUnproven::new("Owned process exit status unavailable")
                    })?
                {
                    return Ok(status);
                }
                let remaining = end.saturating_duration_since(std::time::Instant::now());
                ensure!(
                    !remaining.is_zero(),
                    CleanupUnproven::new(
                        "Owned process exit could not be proven within cleanup budget"
                    )
                );
                thread::sleep(remaining.min(Duration::from_millis(10)));
            }
        }
    }
}
impl Deref for ManagedChild {
    type Target = dyn StdChildWrapper;
    fn deref(&self) -> &Self::Target {
        &*self.inner
    }
}
impl DerefMut for ManagedChild {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut *self.inner
    }
}
impl Drop for ManagedChild {
    fn drop(&mut self) {
        if self.drop_wait {
            let _ = self.cleanup_checked(Duration::from_secs(2));
        }
    }
}
