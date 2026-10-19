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

#[test]
fn runtime_environment_filters_secrets_and_preserves_host_overrides() {
    const CHILD: &str = "YOKE_PROCESS_ENV_CHILD_20261019";
    let root = TempDir::new().unwrap();
    if std::env::var(CHILD).as_deref() != Ok("owned") {
        let mut child = Command::new(std::env::current_exe().unwrap());
        child
            .current_dir(root.path())
            .args([
                "--exact",
                "runtime_environment_filters_secrets_and_preserves_host_overrides",
                "--nocapture",
            ])
            .env(CHILD, "owned")
            .env("OPENAI_API_KEY", "owned-not-a-credential")
            .env("YOKE_UNAPPROVED_HOST_VALUE", "owned-private-value");
        // Seed the isolated test host explicitly. The inner capture must inherit
        // this runtime value while filtering the synthetic secret values.
        #[cfg(windows)]
        child.env("SystemDrive", std::env::var_os("SystemDrive").unwrap());
        let result = run_capture(child, b"", &Deadline::new(20_000), limits()).unwrap();
        assert_eq!(
            result.exit_code,
            Some(0),
            "{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(
            result.output_complete
                && !result.timed_out
                && !result.output_exceeded
                && !result.io_error
        );
        return;
    }
    assert_eq!(
        std::env::var("OPENAI_API_KEY").unwrap(),
        "owned-not-a-credential"
    );
    let program = "process.stdout.write(JSON.stringify({drive:process.env.SystemDrive??null,secret:'OPENAI_API_KEY' in process.env,unapproved:'YOKE_UNAPPROVED_HOST_VALUE' in process.env,explicit:process.env.YOKE_EXPLICIT_HOST_VALUE??null}));";
    let result = run_capture(node(&root, program), b"", &Deadline::new(10_000), limits()).unwrap();
    assert_eq!(result.exit_code, Some(0));
    assert!(
        result.output_complete && !result.timed_out && !result.output_exceeded && !result.io_error
    );
    let value: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(value["secret"], false);
    assert_eq!(value["unapproved"], false);
    #[cfg(windows)]
    assert_eq!(value["drive"], std::env::var("SystemDrive").unwrap());
    let mut override_command = node(&root, program);
    override_command
        .env_remove("SystemDrive")
        .env("YOKE_EXPLICIT_HOST_VALUE", "owned-explicit");
    let result = run_capture(override_command, b"", &Deadline::new(10_000), limits()).unwrap();
    assert_eq!(result.exit_code, Some(0));
    assert!(
        result.output_complete && !result.timed_out && !result.output_exceeded && !result.io_error
    );
    let value: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert!(value["drive"].is_null());
    assert_eq!(value["explicit"], "owned-explicit");
    assert_eq!(value["secret"], false);
    assert_eq!(value["unapproved"], false);
}

#[cfg(windows)]
#[test]
fn windows_runtime_environment_supports_managed_known_folders() {
    let root = TempDir::new().unwrap();
    let mut command = Command::new(
        std::path::PathBuf::from(std::env::var_os("SystemRoot").unwrap())
            .join("System32/WindowsPowerShell/v1.0/powershell.exe"),
    );
    command.current_dir(root.path()).args(["-NoProfile", "-NonInteractive", "-Command", r#"$ErrorActionPreference='Stop'; if([string]::IsNullOrEmpty($env:SystemDrive)){throw 'Host system drive missing'}; $folder=[Environment]::GetFolderPath([Environment+SpecialFolder]::CommonApplicationData,[Environment+SpecialFolderOption]::DoNotVerify); if(-not [IO.Path]::IsPathRooted($folder)){throw 'Managed known folder is not absolute'}; Write-Output 'owned-windows-runtime'"#]);
    let result = run_capture(command, b"", &Deadline::new(20_000), limits()).unwrap();
    assert_eq!(
        result.exit_code,
        Some(0),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        String::from_utf8(result.stdout).unwrap().trim(),
        "owned-windows-runtime"
    );
    assert!(
        result.output_complete && !result.timed_out && !result.output_exceeded && !result.io_error
    );
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
