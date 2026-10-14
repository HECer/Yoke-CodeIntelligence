use std::{process::Command, time::Instant};
use tempfile::TempDir;
use yoke::{
    capture::Deadline,
    process::{CaptureLimits, run_capture},
};

fn limits() -> CaptureLimits {
    CaptureLimits {
        stdin_bytes: 16_000_000,
        stdout_bytes: 32_000_000,
        stderr_bytes: 1_000_000,
    }
}

fn node(root: &TempDir, program: &str) -> Command {
    // This host-selected program is a labeled real stdio protocol fixture.
    let mut command = Command::new("node");
    command.args(["-e", program]).current_dir(root.path());
    command
}

#[test]
fn capture_transfers_exact_unicode_bytes_and_independent_streams() {
    let root = TempDir::new().unwrap();
    let input = "source 🌍 Grüße\r\n".as_bytes();
    let result = run_capture(
        node(
            &root,
            "process.stdin.pipe(process.stdout); process.stderr.write('diagnostic');",
        ),
        input,
        &Deadline::new(10_000),
        limits(),
    )
    .unwrap();
    assert_eq!(result.stdout, input);
    assert_eq!(result.stderr, b"diagnostic");
    assert_eq!(result.exit_code, Some(0));
    assert!(result.stdin_complete && result.output_complete);
    assert!(!result.timed_out && !result.output_exceeded && !result.io_error);
}

#[test]
fn output_overflow_is_a_failure_with_bounded_retained_bytes() {
    let root = TempDir::new().unwrap();
    let result = run_capture(
        node(
            &root,
            "process.stdout.write(Buffer.alloc(1000000, 65)); setInterval(()=>{},1000);",
        ),
        b"",
        &Deadline::new(10_000),
        CaptureLimits {
            stdout_bytes: 1234,
            ..limits()
        },
    )
    .unwrap();
    assert!(result.output_exceeded);
    assert!(result.stdout.len() <= 1234 && !result.output_complete);
}

#[test]
fn descendant_held_stdio_cannot_extend_the_shared_deadline() {
    let root = TempDir::new().unwrap();
    let began = Instant::now();
    let result = run_capture(node(&root, "require('node:child_process').spawn(process.execPath,['-e','setInterval(()=>{},1000)'],{stdio:['ignore',process.stdout,process.stderr],windowsHide:true});"), b"", &Deadline::new(1500), limits()).unwrap();
    assert!(result.timed_out && !result.output_complete);
    assert!(began.elapsed().as_millis() < 3000, "{result:?}");
}

#[test]
fn source_selected_defaults_and_oversized_stdin_are_rejected_before_spawn() {
    let root = TempDir::new().unwrap();
    let marker = root.path().join("started");
    let mut command = node(
        &root,
        "require('node:fs').writeFileSync(process.argv[1], 'started');",
    );
    command.arg(&marker);
    assert!(
        run_capture(
            command,
            b"large",
            &Deadline::new(10_000),
            CaptureLimits {
                stdin_bytes: 1,
                ..limits()
            }
        )
        .is_err()
    );
    assert!(!marker.exists());
    let mut command = Command::new("node");
    command.args(["-e", "process.exit(0)"]);
    assert!(run_capture(command, b"", &Deadline::new(10_000), limits()).is_err());
}

#[cfg(windows)]
#[test]
fn windows_host_fixture_does_not_allocate_a_console_window() {
    let root = TempDir::new().unwrap();
    let mut command = Command::new("powershell.exe");
    command.current_dir(root.path()).args(["-NoProfile", "-NonInteractive", "-Command", r#"Add-Type -TypeDefinition 'using System; using System.Runtime.InteropServices; public class OwnedConsoleFixture { [DllImport("kernel32.dll")] public static extern IntPtr GetConsoleWindow(); }'; [OwnedConsoleFixture]::GetConsoleWindow().ToInt64()"#]);
    let result = run_capture(command, b"", &Deadline::new(15_000), limits()).unwrap();
    assert_eq!(result.exit_code, Some(0));
    assert_eq!(String::from_utf8(result.stdout).unwrap().trim(), "0");
}
