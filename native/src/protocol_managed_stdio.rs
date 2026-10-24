//! Opt-in stdio driver: the non-Send Session stays on its owning thread, even
//! while protocol input is idle. No detached reader or process is created.
use super::*;
use crate::{capture::Deadline, project_session::Session};
use std::time::Duration;

const FRAME_LIMIT: usize = 4_000_000;
const CHUNK: usize = 65_536;

trait Input {
    /// None is idle; Some(0) is EOF. Never waits for a complete protocol line.
    fn available(&mut self, buffer: &mut [u8]) -> Result<Option<usize>>;
}

#[derive(Default)]
struct Frame {
    bytes: Vec<u8>,
    oversized: bool,
}
impl Frame {
    fn append(&mut self, bytes: &[u8]) {
        if self.oversized || self.bytes.len().saturating_add(bytes.len()) > FRAME_LIMIT {
            self.oversized = true;
            self.bytes.clear();
        } else {
            self.bytes.extend_from_slice(bytes);
        }
    }
    fn finish(&self) -> Result<()> {
        ensure!(
            self.bytes.is_empty() && !self.oversized,
            "MCP messages must end with newline"
        );
        Ok(())
    }
}

/// Only for an explicitly management-enabled, Session-backed host. The caller
/// retains its runtime and must perform its normal checked runtime shutdown.
/// Input is exclusively owned process stdin, never a borrowed buffered reader
/// whose prefetched bytes would bypass OS readiness. Windows requires a pipe or
/// regular file; Unix uses descriptor readiness without reconfiguring the input.
pub fn serve_managed_stdio(
    session: &mut Session,
    runtime: Option<&mut crate::agent_repl::AgentRepl>,
    profile: Profile,
    output: impl Write,
) -> Result<()> {
    let mut input = Stdin::open()?;
    run(session, runtime, profile, &mut input, output)
}

fn run(
    session: &mut Session,
    mut runtime: Option<&mut crate::agent_repl::AgentRepl>,
    profile: Profile,
    input: &mut impl Input,
    mut output: impl Write,
) -> Result<()> {
    ensure!(
        profile != Profile::Native,
        "Management requires a Session-backed profile"
    );
    ensure!(
        runtime.is_none() || profile == Profile::Serena,
        "Persistent runtime requires its host-selected Serena profile"
    );
    session.enable_dashboard_management(&Deadline::new(120_000))?;
    session.bind_dashboard_shutdown(
        crate::session_dashboard::management::ShutdownScope::StdioConnection,
    )?;
    let result = (|| -> Result<()> {
        // Bind before the first idle poll, not only after the first tool call.
        session.select_prompt_profile(match profile {
            Profile::Serena => crate::session_prompt::Profile::Serena,
            Profile::Graphify => crate::session_prompt::Profile::Graphify,
            Profile::Trail => crate::session_prompt::Profile::Trail,
            Profile::Graft => crate::session_prompt::Profile::Graft,
            _ => crate::session_prompt::Profile::Native,
        });
        let mut initialized = false;
        let mut frame = Frame::default();
        let mut chunk = [0_u8; CHUNK];
        'connection: loop {
            if session.dashboard_shutdown_pending() {
                if session.dashboard_shutdown_ready() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
                continue;
            }
            let available = input.available(&mut chunk)?;
            if available == Some(0) {
                frame.finish()?;
                break;
            }
            if session.poll_dashboard_management(&Deadline::new(120_000))? && initialized {
                serde_json::to_writer(
                    &mut output,
                    &json!({"jsonrpc":"2.0","method":"notifications/tools/list_changed"}),
                )?;
                output.write_all(b"\n")?;
                output.flush()?;
            }
            if session.dashboard_shutdown_pending() {
                continue;
            }
            let Some(count) = available else {
                std::thread::sleep(Duration::from_millis(10));
                continue;
            };
            ensure!(count <= chunk.len(), "Invalid host input length");
            let mut rest = &chunk[..count];
            while !rest.is_empty() {
                let end = rest.iter().position(|b| *b == b'\n').map(|i| i + 1);
                let length = end.unwrap_or(rest.len());
                frame.append(&rest[..length]);
                rest = &rest[length..];
                if end.is_none() {
                    continue;
                }
                if frame.oversized {
                    serde_json::to_writer(
                        &mut output,
                        &error(Value::Null, -32700, "Input message budget exceeded"),
                    )?;
                    output.write_all(b"\n")?;
                    output.flush()?;
                } else {
                    if let Some(runtime) = runtime.as_ref() {
                        ensure!(
                            runtime.is_alive(),
                            "Agent REPL state was lost; open a new connection"
                        );
                    }
                    let mut backend = match profile {
                        Profile::Session => Backend::Session(session, None),
                        Profile::Serena => Backend::Serena(session, runtime.as_deref_mut()),
                        Profile::Graphify => Backend::Graphify(session),
                        Profile::Trail => Backend::Trail(session, "trail"),
                        Profile::Graft => Backend::Trail(session, "graft"),
                        Profile::Native => unreachable!("checked before admission"),
                    };
                    // Reuse the complete existing framing/dispatch/error and
                    // notification behavior, retaining initialization state.
                    serve_backend_state(
                        &mut backend,
                        std::io::Cursor::new(&frame.bytes),
                        &mut output,
                        &mut initialized,
                    )?;
                }
                frame.bytes.clear();
                frame.oversized = false;
                // Many pipelined short frames must not starve the dashboard.
                if session.poll_dashboard_management(&Deadline::new(120_000))? && initialized {
                    serde_json::to_writer(
                        &mut output,
                        &json!({"jsonrpc":"2.0","method":"notifications/tools/list_changed"}),
                    )?;
                    output.write_all(b"\n")?;
                    output.flush()?;
                }
                if session.dashboard_shutdown_pending() {
                    continue 'connection;
                }
            }
        }
        Ok(())
    })();
    session.close_dashboard_management();
    result
}

#[cfg(windows)]
struct Stdin {
    handle: *mut std::ffi::c_void,
    kind: u32,
}
#[cfg(windows)]
mod win {
    use std::ffi::c_void;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        pub(super) fn GetStdHandle(which: u32) -> *mut c_void;
        pub(super) fn GetFileType(handle: *mut c_void) -> u32;
        pub(super) fn PeekNamedPipe(
            handle: *mut c_void,
            buffer: *mut c_void,
            size: u32,
            read: *mut u32,
            available: *mut u32,
            remaining: *mut u32,
        ) -> i32;
        pub(super) fn ReadFile(
            handle: *mut c_void,
            buffer: *mut c_void,
            size: u32,
            read: *mut u32,
            overlapped: *mut c_void,
        ) -> i32;
    }
}
#[cfg(windows)]
impl Stdin {
    fn open() -> Result<Self> {
        // Borrow the process standard handle; never close or reconfigure it.
        let handle = unsafe { win::GetStdHandle((-10_i32) as u32) };
        ensure!(
            !handle.is_null() && handle as isize != -1,
            "Stdin handle unavailable"
        );
        let kind = unsafe { win::GetFileType(handle) };
        ensure!(
            matches!(kind, 1 | 3),
            "Managed MCP stdio requires a pipe or regular file"
        );
        Ok(Self { handle, kind })
    }
}
#[cfg(windows)]
impl Input for Stdin {
    fn available(&mut self, buffer: &mut [u8]) -> Result<Option<usize>> {
        let mut size = buffer.len().min(CHUNK) as u32;
        if self.kind == 3 {
            let mut available = 0;
            if unsafe {
                win::PeekNamedPipe(
                    self.handle,
                    std::ptr::null_mut(),
                    0,
                    std::ptr::null_mut(),
                    &mut available,
                    std::ptr::null_mut(),
                )
            } == 0
            {
                let error = std::io::Error::last_os_error();
                if matches!(error.raw_os_error(), Some(109 | 233)) {
                    return Ok(Some(0));
                }
                return Err(error.into());
            }
            if available == 0 {
                return Ok(None);
            }
            size = size.min(available);
        }
        let mut read = 0;
        if unsafe {
            win::ReadFile(
                self.handle,
                buffer.as_mut_ptr().cast(),
                size,
                &mut read,
                std::ptr::null_mut(),
            )
        } == 0
        {
            let error = std::io::Error::last_os_error();
            if matches!(error.raw_os_error(), Some(109 | 233 | 38)) {
                return Ok(Some(0));
            }
            return Err(error.into());
        }
        Ok(Some(read as usize))
    }
}

#[cfg(unix)]
struct Stdin;
#[cfg(unix)]
impl Stdin {
    fn open() -> Result<Self> {
        Ok(Self)
    }
}
#[cfg(unix)]
impl Input for Stdin {
    fn available(&mut self, buffer: &mut [u8]) -> Result<Option<usize>> {
        let mut descriptor = libc::pollfd {
            fd: libc::STDIN_FILENO,
            events: libc::POLLIN,
            revents: 0,
        };
        let ready = unsafe { libc::poll(&mut descriptor, 1, 0) };
        if ready < 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::Interrupted {
                return Ok(None);
            }
            return Err(error.into());
        }
        if ready == 0 {
            return Ok(None);
        }
        ensure!(
            descriptor.revents & libc::POLLNVAL == 0,
            "Stdin unavailable"
        );
        let read = unsafe {
            libc::read(
                libc::STDIN_FILENO,
                buffer.as_mut_ptr().cast(),
                buffer.len().min(CHUNK),
            )
        };
        if read < 0 {
            let error = std::io::Error::last_os_error();
            if matches!(
                error.kind(),
                std::io::ErrorKind::Interrupted | std::io::ErrorKind::WouldBlock
            ) {
                return Ok(None);
            }
            return Err(error.into());
        }
        Ok(Some(read as usize))
    }
}

#[cfg(not(any(unix, windows)))]
struct Stdin;
#[cfg(not(any(unix, windows)))]
impl Stdin {
    fn open() -> Result<Self> {
        anyhow::bail!("Managed stdio unavailable on this platform")
    }
}
#[cfg(not(any(unix, windows)))]
impl Input for Stdin {
    fn available(&mut self, _: &mut [u8]) -> Result<Option<usize>> {
        anyhow::bail!("Managed stdio unavailable on this platform")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project_session::HostProfile;
    use std::collections::VecDeque;

    struct Script(VecDeque<Option<Vec<u8>>>);
    impl Input for Script {
        fn available(&mut self, buffer: &mut [u8]) -> Result<Option<usize>> {
            let Some(step) = self.0.pop_front() else {
                return Ok(Some(0));
            };
            let Some(bytes) = step else {
                return Ok(None);
            };
            ensure!(bytes.len() <= buffer.len(), "Owned fixture chunk limit");
            buffer[..bytes.len()].copy_from_slice(&bytes);
            Ok(Some(bytes.len()))
        }
    }
    fn fixture() -> (tempfile::TempDir, Session) {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("main.rs"), b"fn original() {}\r\n").unwrap();
        let host: HostProfile =
            serde_json::from_value(json!({"dashboard":{"management":{}},"steering":{}})).unwrap();
        let session = Session::open(
            Engine::open(root.path()).unwrap(),
            host,
            Some("stdio-owned"),
            &Deadline::new(20_000),
        )
        .unwrap();
        (root, session)
    }
    #[test]
    fn fragmented_unicode_idle_and_pipeline_keep_existing_protocol_responses() {
        let (root, mut session) = fixture();
        let bytes = concat!(
            "{\"jsonrpc\":\"2.0\",\"id\":\"π🦀\",\"method\":\"initialize\"}\n",
            "{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\r\n",
            "[1,2]\n",
            "{broken\n",
            "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\"}\n"
        )
        .as_bytes();
        let mut expected = Vec::new();
        serve_serena(&mut session, std::io::Cursor::new(bytes), &mut expected).unwrap();
        let mut input = Script(
            bytes
                .iter()
                .flat_map(|byte| [Some(vec![*byte]), None])
                .collect(),
        );
        let mut actual = Vec::new();
        run(&mut session, None, Profile::Serena, &mut input, &mut actual).unwrap();
        assert_eq!(actual, expected);
        assert_eq!(
            std::fs::read(root.path().join("main.rs")).unwrap(),
            b"fn original() {}\r\n"
        );
        assert!(
            session
                .invoke("openDashboard", &json!({}), &Deadline::new(1000))
                .is_err(),
            "Driver closure must revoke its mailbox"
        );
    }
    #[test]
    fn oversized_input_drains_bounded_storage_and_resynchronizes() {
        let (_root, mut session) = fixture();
        let mut steps = VecDeque::new();
        for _ in 0..64 {
            steps.push_back(Some(vec![b'x'; CHUNK]));
        }
        steps.push_back(Some(
            b"\n{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n".to_vec(),
        ));
        let mut actual = Vec::new();
        run(
            &mut session,
            None,
            Profile::Serena,
            &mut Script(steps),
            &mut actual,
        )
        .unwrap();
        let replies = actual
            .split(|b| *b == b'\n')
            .filter(|b| !b.is_empty())
            .map(|b| serde_json::from_slice::<Value>(b).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(replies.len(), 2);
        assert_eq!(replies[0]["error"]["code"], -32700);
        assert_eq!(replies[1]["id"], 1);
        assert_eq!(replies[1]["result"], json!({}));
        let mut frame = Frame::default();
        for _ in 0..128 {
            frame.append(&[b'x'; CHUNK]);
        }
        assert!(frame.oversized);
        assert!(frame.bytes.is_empty());
        assert!(frame.bytes.capacity() <= FRAME_LIMIT * 2);
    }
    #[test]
    fn eof_partial_message_is_refused_and_driver_is_closed() {
        let (_root, mut session) = fixture();
        let mut input = Script(VecDeque::from([Some(b"{\"jsonrpc\":\"2.0\"}".to_vec())]));
        let mut output = Vec::new();
        let error = run(&mut session, None, Profile::Serena, &mut input, &mut output).unwrap_err();
        assert!(error.to_string().contains("end with newline"));
        assert!(output.is_empty());
        assert!(
            session
                .invoke("openDashboard", &json!({}), &Deadline::new(1000))
                .is_err()
        );
    }
}
