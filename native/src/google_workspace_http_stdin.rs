//! Private pipe input only. No terminal prompt, environment lookup or secret output.
use super::{HostAccessToken, MAX_TOKEN};
use crate::capture::Deadline;
use anyhow::{Result, ensure};
use zeroize::Zeroizing;

fn append(bytes: &mut Zeroizing<Vec<u8>>, chunk: &[u8]) -> Result<bool> {
    ensure!(
        bytes
            .len()
            .checked_add(chunk.len())
            .is_some_and(|n| n <= MAX_TOKEN + 2),
        "google_workspace_private_token_capacity_exceeded"
    );
    if let Some(index) = chunk.iter().position(|b| *b == b'\n') {
        ensure!(
            index + 1 == chunk.len(),
            "google_workspace_private_token_invalid"
        );
        bytes.extend_from_slice(chunk);
        Ok(true)
    } else {
        bytes.extend_from_slice(chunk);
        Ok(false)
    }
}

#[cfg(windows)]
pub(super) mod windows {
    use super::*;
    use std::ffi::c_void;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetStdHandle(kind: u32) -> *mut c_void;
        fn GetFileType(handle: *mut c_void) -> u32;
        fn PeekNamedPipe(
            handle: *mut c_void,
            buffer: *mut c_void,
            size: u32,
            read: *mut u32,
            available: *mut u32,
            left: *mut u32,
        ) -> i32;
        fn ReadFile(
            handle: *mut c_void,
            buffer: *mut c_void,
            size: u32,
            read: *mut u32,
            overlapped: *mut c_void,
        ) -> i32;
        fn GetLastError() -> u32;
    }
    pub(super) fn stdin(deadline: &Deadline) -> Result<HostAccessToken> {
        let handle = unsafe { GetStdHandle((-10i32) as u32) };
        read_pipe(handle as isize, deadline)
    }
    pub(super) fn read_pipe(raw: isize, deadline: &Deadline) -> Result<HostAccessToken> {
        let handle = raw as *mut c_void;
        ensure!(
            unsafe { GetFileType(handle) } == 3,
            "google_workspace_private_token_pipe_required"
        );
        let mut bytes = Zeroizing::new(Vec::new());
        let mut chunk = Zeroizing::new([0u8; MAX_TOKEN + 3]);
        loop {
            deadline.check()?;
            let mut available = 0;
            if unsafe {
                PeekNamedPipe(
                    handle,
                    std::ptr::null_mut(),
                    0,
                    std::ptr::null_mut(),
                    &mut available,
                    std::ptr::null_mut(),
                )
            } == 0
            {
                let error = unsafe { GetLastError() };
                ensure!(
                    matches!(error, 109 | 232),
                    "google_workspace_private_token_pipe_failed"
                );
                break;
            }
            if available == 0 {
                std::thread::sleep(std::time::Duration::from_millis(10));
                continue;
            }
            let size = (available as usize)
                .min(chunk.len())
                .min(MAX_TOKEN + 3 - bytes.len());
            ensure!(size > 0, "google_workspace_private_token_capacity_exceeded");
            let mut count = 0;
            ensure!(
                unsafe {
                    ReadFile(
                        handle,
                        chunk.as_mut_ptr().cast(),
                        size as u32,
                        &mut count,
                        std::ptr::null_mut(),
                    )
                } != 0,
                "google_workspace_private_token_pipe_failed"
            );
            if count == 0 {
                break;
            }
            if append(&mut bytes, &chunk[..count as usize])? {
                break;
            }
        }
        deadline.check()?;
        HostAccessToken::from_host_input(&bytes)
    }
}

#[cfg(unix)]
mod unix {
    use super::*;
    use std::os::fd::AsRawFd;
    struct Flags {
        fd: i32,
        original: i32,
    }
    impl Drop for Flags {
        fn drop(&mut self) {
            unsafe {
                libc::fcntl(self.fd, libc::F_SETFL, self.original);
            }
        }
    }
    pub(super) fn stdin(deadline: &Deadline) -> Result<HostAccessToken> {
        let stdin = std::io::stdin();
        let fd = stdin.as_raw_fd();
        ensure!(
            unsafe { libc::isatty(fd) } == 0,
            "google_workspace_private_token_pipe_required"
        );
        let original = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        ensure!(
            original >= 0
                && unsafe { libc::fcntl(fd, libc::F_SETFL, original | libc::O_NONBLOCK) } == 0,
            "google_workspace_private_token_pipe_failed"
        );
        let _flags = Flags { fd, original };
        let mut bytes = Zeroizing::new(Vec::new());
        let mut chunk = Zeroizing::new([0u8; MAX_TOKEN + 3]);
        loop {
            deadline.check()?;
            let mut event = libc::pollfd {
                fd,
                events: libc::POLLIN | libc::POLLHUP | libc::POLLERR,
                revents: 0,
            };
            let status = unsafe { libc::poll(&mut event, 1, 10) };
            if status < 0 {
                if std::io::Error::last_os_error().raw_os_error() == Some(libc::EINTR) {
                    continue;
                }
                anyhow::bail!("google_workspace_private_token_pipe_failed");
            }
            if status == 0 {
                continue;
            }
            let size = chunk.len().min(MAX_TOKEN + 3 - bytes.len());
            ensure!(size > 0, "google_workspace_private_token_capacity_exceeded");
            let count = unsafe { libc::read(fd, chunk.as_mut_ptr().cast(), size) };
            if count < 0 {
                let error = std::io::Error::last_os_error().raw_os_error();
                if matches!(error, Some(libc::EINTR) | Some(libc::EAGAIN)) {
                    continue;
                }
                anyhow::bail!("google_workspace_private_token_pipe_failed");
            }
            if count == 0 {
                break;
            }
            if append(&mut bytes, &chunk[..count as usize])? {
                break;
            }
        }
        deadline.check()?;
        HostAccessToken::from_host_input(&bytes)
    }
}

/// One bounded private token line. The caller must own this process's stdin.
pub fn read_private_token_stdin(deadline: &Deadline) -> Result<HostAccessToken> {
    deadline.check()?;
    #[cfg(windows)]
    {
        windows::stdin(deadline)
    }
    #[cfg(unix)]
    {
        unix::stdin(deadline)
    }
    #[cfg(not(any(windows, unix)))]
    {
        anyhow::bail!("google_workspace_private_token_pipe_platform_unavailable")
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::{
        ffi::c_void,
        time::{Duration, Instant},
    };
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreatePipe(
            read: *mut *mut c_void,
            write: *mut *mut c_void,
            attributes: *mut c_void,
            size: u32,
        ) -> i32;
        fn WriteFile(
            handle: *mut c_void,
            buffer: *const c_void,
            size: u32,
            written: *mut u32,
            overlapped: *mut c_void,
        ) -> i32;
        fn CloseHandle(handle: *mut c_void) -> i32;
    }
    struct Handle(*mut c_void);
    impl Drop for Handle {
        fn drop(&mut self) {
            assert_ne!(unsafe { CloseHandle(self.0) }, 0);
        }
    }
    fn pipe() -> (Handle, Handle) {
        let mut read = std::ptr::null_mut();
        let mut write = std::ptr::null_mut();
        assert_ne!(
            unsafe { CreatePipe(&mut read, &mut write, std::ptr::null_mut(), 8192) },
            0
        );
        (Handle(read), Handle(write))
    }
    fn write(handle: &Handle, bytes: &[u8]) {
        let mut written = 0;
        assert_ne!(
            unsafe {
                WriteFile(
                    handle.0,
                    bytes.as_ptr().cast(),
                    bytes.len() as u32,
                    &mut written,
                    std::ptr::null_mut(),
                )
            },
            0
        );
        assert_eq!(written as usize, bytes.len());
    }
    #[test]
    fn owned_windows_pipe_reads_one_private_crlf_line_without_waiting_for_eof() {
        let (read, writer) = pipe();
        write(&writer, b"owned_private_pipe_token\r\n");
        let token = windows::read_pipe(read.0 as isize, &Deadline::new(1000)).unwrap();
        assert_eq!(token.0.as_str(), "owned_private_pipe_token");
    }
    #[test]
    fn owned_windows_pipe_eof_capacity_and_invalid_lines_are_explicit() {
        let (read, writer) = pipe();
        write(&writer, b"owned_token");
        drop(writer);
        assert_eq!(
            windows::read_pipe(read.0 as isize, &Deadline::new(1000))
                .unwrap()
                .0
                .as_str(),
            "owned_token"
        );
        for bytes in [
            Vec::new(),
            vec![b'a'; MAX_TOKEN + 3],
            b"owned\nextra".to_vec(),
            b"owned\r".to_vec(),
        ] {
            let (read, writer) = pipe();
            if !bytes.is_empty() {
                write(&writer, &bytes);
            }
            drop(writer);
            assert!(windows::read_pipe(read.0 as isize, &Deadline::new(1000)).is_err());
        }
    }
    #[test]
    fn owned_windows_pipe_with_open_writer_obeys_the_deadline() {
        let (read, _writer) = pipe();
        let start = Instant::now();
        assert!(windows::read_pipe(read.0 as isize, &Deadline::new(40)).is_err());
        assert!(start.elapsed() < Duration::from_secs(1));
    }
}
