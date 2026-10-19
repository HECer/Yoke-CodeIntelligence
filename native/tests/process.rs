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
    // Query the actual kernel handle without requiring PowerShell's optional
    // CodeDOM compiler and reference-assembly setup on the host image.
    let source = root.path().join("console.rs");
    let probe = root.path().join("console.exe");
    std::fs::write(
        &source,
        r#"
#[link(name="kernel32")]
unsafe extern "system" { fn GetConsoleWindow() -> *mut std::ffi::c_void; }
fn main() { println!("{}",unsafe { GetConsoleWindow() as usize }); }
"#,
    )
    .unwrap();
    let mut compiler = Command::new("rustc");
    compiler
        .current_dir(root.path())
        .arg(&source)
        .args(["--edition=2024", "-Cdebuginfo=0", "-o"])
        .arg(&probe);
    #[cfg(target_env = "msvc")]
    {
        let linker = find_msvc_tools::find_tool(std::env::consts::ARCH, "link.exe")
            .expect("The owned native fixture requires its MSVC linker");
        for (name, value) in linker.env() {
            compiler.env(name, value);
        }
    }
    let build = run_capture(compiler, b"", &Deadline::new(30_000), limits()).unwrap();
    assert_eq!(
        build.exit_code,
        Some(0),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.output_complete && !build.timed_out && !build.output_exceeded && !build.io_error);
    let mut command = Command::new(probe);
    command.current_dir(root.path());
    let result = run_capture(command, b"", &Deadline::new(15_000), limits()).unwrap();
    assert_eq!(
        result.exit_code,
        Some(0),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(String::from_utf8(result.stdout).unwrap().trim(), "0");
    assert!(
        result.output_complete && !result.timed_out && !result.output_exceeded && !result.io_error
    );
}
