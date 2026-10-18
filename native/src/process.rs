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
    error.downcast_ref::<CleanupUnproven>().is_some()
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
    mut command: Command,
    input: &[u8],
    deadline: &crate::capture::Deadline,
    limits: CaptureLimits,
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
    let mut child = ManagedChild::spawn(command).context("Host executable unavailable")?;
    let (done, completed) = mpsc::sync_channel::<(usize, std::io::Result<Vec<u8>>)>(3);
    let exceeded = Arc::new(AtomicBool::new(false));
    let mut stdin = child.stdin().take().context("Host stdin unavailable")?;
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
            Box::new(child.stdout().take().context("Host stdout unavailable")?)
                as Box<dyn Read + Send>,
            limits.stdout_bytes,
        ),
        (
            1,
            Box::new(child.stderr().take().context("Host stderr unavailable")?)
                as Box<dyn Read + Send>,
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
            exit = child.inner_mut().try_wait()?;
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
        child
            .start_kill()
            .context("Unable to terminate host process group")?;
        if exit.is_none() {
            exit = Some(child.inner_mut().wait()?);
        }
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

pub struct ManagedChild {
    inner: Box<dyn StdChildWrapper>,
    drop_wait: bool,
}
impl ManagedChild {
    pub fn spawn(command: Command) -> Result<Self> {
        let mut wrapper = StdCommandWrap::from(command);
        #[cfg(windows)]
        {
            // JobObject replaces Command's creation flags. Preserve this flag
            // through the wrapper that it explicitly consults before spawn.
            let mut flags = process_wrap::std::CreationFlags(Default::default());
            flags.0.0 = 0x0800_0000; // CREATE_NO_WINDOW
            wrapper.wrap(flags);
            wrapper.wrap(process_wrap::std::JobObject);
        }
        #[cfg(unix)]
        wrapper.wrap(process_wrap::std::ProcessGroup::leader());
        Ok(Self {
            inner: wrapper.spawn()?,
            drop_wait: true,
        })
    }
    /// Consume an owned process and prove its exit after requesting wrapper
    /// termination. Cleanup uses a separate bounded host budget even when the
    /// request that owned the process has already been cancelled.
    pub fn terminate_checked(mut self, budget: Duration) -> Result<std::process::ExitStatus> {
        self.drop_wait = false;
        self.inner
            .start_kill()
            .context("Owned process termination request failed")?;
        ensure!(
            !budget.is_zero() && budget <= Duration::from_secs(2),
            "Invalid owned process cleanup budget"
        );
        let end = std::time::Instant::now() + budget;
        loop {
            if let Some(status) = self.inner.inner_mut().try_wait()? {
                return Ok(status);
            }
            let remaining = end.saturating_duration_since(std::time::Instant::now());
            ensure!(
                !remaining.is_zero(),
                "Owned process exit could not be proven within cleanup budget"
            );
            thread::sleep(remaining.min(Duration::from_millis(10)));
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
        if self.inner.start_kill().is_ok() && self.drop_wait {
            let _ = self.inner.inner_mut().wait();
        }
    }
}
