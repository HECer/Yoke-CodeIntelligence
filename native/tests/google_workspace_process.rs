//! Execute one compiled, owned client fixture; never authenticates or opens a socket.
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
use yoke::{
    capture::Deadline,
    google_workspace::{self as google, CaptureSpec, ClientSpec, Limits, Permissions},
    process::{CaptureLimits, run_capture},
};

const CLIENT: &str = r###"
use std::{env,fs,io::Write};
fn main() {
    let config=std::path::PathBuf::from(env::var_os("GOOGLE_WORKSPACE_CLI_CONFIG_DIR").unwrap());
    let panic_config=config.clone();
    std::panic::set_hook(Box::new(move |info| { let _=fs::write(panic_config.join("owned-panic.txt"),info.to_string()); }));
    let args=env::args().skip(1).collect::<Vec<_>>();
    let mut marker=fs::OpenOptions::new().append(true).create(true).open(config.join("calls")).unwrap();
    writeln!(marker,"{}",args.join("|")).unwrap();
    assert_eq!(fs::read(".env").unwrap(), b"");
    for key in ["GOOGLE_WORKSPACE_CLI_SANITIZE_TEMPLATE","GOOGLE_WORKSPACE_CLI_TOKEN","HTTP_PROXY","YOKE_UNSELECTED"] {
        assert!(env::var_os(key).is_none(), "unselected environment leaked");
    }
    let adc=std::path::PathBuf::from(env::var_os("GOOGLE_APPLICATION_CREDENTIALS").unwrap());
    assert_eq!(adc.file_name().unwrap(),"adc-not-selected.json");
    assert_eq!(fs::canonicalize(adc.parent().unwrap()).unwrap(),fs::canonicalize(env::current_dir().unwrap()).unwrap());
    assert!(!adc.exists());
    if args==["--version"] { println!("gws 0.22.5"); return; }
    assert_eq!(&args[..4], &["drive","files","export","--params"]);
    assert_eq!(&args[5..], &["--output","export.bin"]);
    assert_eq!(args[4], r#"{"fileId":"owned_123","mimeType":"text/markdown"}"#);
    let mode=fs::read_to_string(config.join("mode")).unwrap();
    match mode.as_str() {
        "ok"=>fs::write("export.bin","A💡B\r\n").unwrap(),
        "oversize"=>fs::write("export.bin",[b'a';33]).unwrap(),
        "missing"=>(),
        "failure"=>{ eprintln!("private_owned_diagnostic"); std::process::exit(7); },
        "stderr"=>{ eprint!("{}","x".repeat(40000)); fs::write("export.bin","body").unwrap(); },
        "timeout"=>std::thread::sleep(std::time::Duration::from_secs(10)),
        "changed"=>{ fs::write("export.bin","owned export").unwrap();
            let path=fs::read_to_string(config.join("shortcut-path")).unwrap();
            fs::write(path,b"changed by the owned trusted fixture").unwrap(); },
        _=>panic!("bad fixture mode")
    }
}
"###;
fn build_client(root: &Path) -> PathBuf {
    let source = root.join("owned_client.rs");
    let program = root.join(if cfg!(windows) {
        "owned_client.exe"
    } else {
        "owned_client"
    });
    fs::write(&source, CLIENT).unwrap();
    let mut compiler = Command::new("rustc");
    compiler
        .current_dir(root)
        .arg(&source)
        .args(["--edition=2024", "-Cdebuginfo=0", "-o"])
        .arg(&program);
    #[cfg(target_env = "msvc")]
    {
        let linker = find_msvc_tools::find_tool(std::env::consts::ARCH, "link.exe").unwrap();
        for (key, value) in linker.env() {
            compiler.env(key, value);
        }
    }
    let output = run_capture(
        compiler,
        b"",
        &Deadline::new(60000),
        CaptureLimits {
            stdin_bytes: 0,
            stdout_bytes: 32768,
            stderr_bytes: 32768,
        },
    )
    .unwrap();
    assert_eq!(
        output.exit_code,
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.output_complete && !output.timed_out && !output.io_error);
    program
}
fn permissions() -> Permissions {
    Permissions {
        allow_network: true,
        allow_exporter_execution: true,
        allow_client_state: true,
    }
}
#[test]
fn real_owned_client_contract_success_refusals_and_bounded_cleanup() {
    let owned = tempfile::tempdir().unwrap();
    let root = owned.path().join("source");
    let config = owned.path().join("client-state");
    fs::create_dir(&root).unwrap();
    fs::create_dir(&config).unwrap();
    // A source .env must never be used as client configuration.
    fs::write(root.join(".env"), b"YOKE_UNSELECTED=source-data").unwrap();
    let program = build_client(owned.path());
    let bytes = br#"{"doc_id":"owned_123","allowNetwork":true}"#;
    let mut spec = CaptureSpec {
        shortcut_path: "notes.gdoc".into(),
        expected_shortcut_sha256: yoke::model::hash(bytes),
        expected_file_id: "owned_123".into(),
        client: ClientSpec {
            executable: program.clone(),
            executable_sha256: yoke::model::hash(fs::read(&program).unwrap()),
            version: "0.22.5".into(),
            configuration_directory: config.clone(),
        },
    };
    let limits = Limits {
        max_body_bytes: 32,
        max_archive_bytes: 4096,
    };
    fs::write(config.join("mode"), b"ok").unwrap();
    let captured = google::capture(
        &root,
        bytes,
        &spec,
        &permissions(),
        &limits,
        &Deadline::new(30000),
    )
    .unwrap_or_else(|error| {
        panic!(
            "capture failed: {error}; owned fixture diagnostic: {}",
            fs::read_to_string(config.join("owned-panic.txt")).unwrap_or_default()
        )
    });
    assert_eq!(
        captured.read(1, 5, &Deadline::new(30000)).unwrap()["utf8"],
        "💡"
    );
    let summary = captured.summary();
    assert_eq!(summary["googleIdentityVerified"], false);
    assert_eq!(summary["exporterClaimVerified"], false);
    assert_eq!(summary["modelCalls"], 0);
    assert_eq!(
        fs::read_to_string(config.join("calls"))
            .unwrap()
            .lines()
            .count(),
        2
    );
    let before = fs::read(config.join("calls")).unwrap();
    for permissions in [
        Permissions::default(),
        Permissions {
            allow_network: false,
            ..permissions()
        },
        Permissions {
            allow_exporter_execution: false,
            ..permissions()
        },
        Permissions {
            allow_client_state: false,
            ..permissions()
        },
    ] {
        assert!(
            google::capture(
                &root,
                bytes,
                &spec,
                &permissions,
                &limits,
                &Deadline::new(30000)
            )
            .is_err()
        );
    }
    spec.expected_shortcut_sha256 = "f".repeat(64);
    assert!(
        google::capture(
            &root,
            bytes,
            &spec,
            &permissions(),
            &limits,
            &Deadline::new(30000)
        )
        .is_err()
    );
    spec.expected_shortcut_sha256 = yoke::model::hash(bytes);
    let original = spec.client.executable_sha256.clone();
    spec.client.executable_sha256 = "a".repeat(64);
    assert!(
        google::capture(
            &root,
            bytes,
            &spec,
            &permissions(),
            &limits,
            &Deadline::new(30000)
        )
        .is_err()
    );
    spec.client.executable_sha256 = original;
    let cramped = Limits {
        max_body_bytes: 32,
        max_archive_bytes: 32,
    };
    assert!(
        google::capture(
            &root,
            bytes,
            &spec,
            &permissions(),
            &cramped,
            &Deadline::new(30000)
        )
        .is_err()
    );
    spec.client.configuration_directory = root.clone();
    assert!(
        google::capture(
            &root,
            bytes,
            &spec,
            &permissions(),
            &limits,
            &Deadline::new(30000)
        )
        .is_err()
    );
    spec.client.configuration_directory = config.clone();
    let inside = root.join(program.file_name().unwrap());
    fs::copy(&program, &inside).unwrap();
    spec.client.executable = inside;
    assert!(
        google::capture(
            &root,
            bytes,
            &spec,
            &permissions(),
            &limits,
            &Deadline::new(30000)
        )
        .is_err()
    );
    spec.client.executable = program.clone();
    let key_bytes =
        serde_json::to_vec(&json!({"doc_id":"owned_123","resourceKey":"owned_key"})).unwrap();
    spec.expected_shortcut_sha256 = yoke::model::hash(&key_bytes);
    let error = google::capture(
        &root,
        &key_bytes,
        &spec,
        &permissions(),
        &limits,
        &Deadline::new(30000),
    )
    .err()
    .unwrap();
    assert!(
        error
            .to_string()
            .contains("resource_key_transport_unavailable")
    );
    assert!(!error.to_string().contains("owned_key"));
    assert_eq!(fs::read(config.join("calls")).unwrap(), before);
    spec.expected_shortcut_sha256 = yoke::model::hash(bytes);
    for mode in ["oversize", "missing", "failure", "stderr", "timeout"] {
        fs::write(config.join("mode"), mode).unwrap();
        let timeout = if mode == "timeout" { 1500 } else { 30000 };
        let error = google::capture(
            &root,
            bytes,
            &spec,
            &permissions(),
            &limits,
            &Deadline::new(timeout),
        )
        .err()
        .unwrap();
        assert!(!error.to_string().contains("private_owned_diagnostic"));
    }
    assert!(!root.join(".yoke").exists());
    assert!(!root.join("export.bin").exists());
    assert_eq!(
        fs::read(root.join(".env")).unwrap(),
        b"YOKE_UNSELECTED=source-data"
    );

    // Real standalone CLI receives hostile ambient settings; the selected client
    // must still observe none of them. No credential or actual model is used.
    fs::write(root.join("notes.gdoc"), bytes).unwrap();
    fs::write(config.join("mode"), b"ok").unwrap();
    let selection = owned.path().join("capture.json");
    fs::write(&selection, serde_json::to_vec(&json!({"shortcutPath":spec.shortcut_path,
        "expectedShortcutSha256":spec.expected_shortcut_sha256,"expectedFileId":spec.expected_file_id,
        "client":{"executable":program,"executableSha256":spec.client.executable_sha256,
        "version":"0.22.5","configurationDirectory":config}})).unwrap()).unwrap();
    let archive = owned.path().join("captured.json");
    let cli = || {
        let mut command = Command::new(env!("CARGO_BIN_EXE_yoke"));
        command
            .current_dir(&root)
            .args(["source-acquisition", "google-capture", "--config"])
            .arg(&selection)
            .arg("--source-root")
            .arg(&root)
            .arg("--output")
            .arg(&archive)
            .args([
                "--allow-shortcut-read",
                "--allow-network",
                "--allow-exporter-execution",
                "--allow-client-state",
                "--allow-output",
            ])
            .env("GOOGLE_WORKSPACE_CLI_TOKEN", "owned_fake_token")
            .env(
                "GOOGLE_WORKSPACE_CLI_SANITIZE_TEMPLATE",
                "owned_fake_template",
            )
            .env(
                "GOOGLE_APPLICATION_CREDENTIALS",
                "owned_missing_credentials.json",
            )
            .env("HTTP_PROXY", "http://127.0.0.1:1")
            .env("YOKE_UNSELECTED", "owned_fake_setting");
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        command.output().unwrap()
    };
    let output = cli();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let saved = fs::read(&archive).unwrap();
    assert_eq!(value["archiveHash"], yoke::model::hash(&saved));
    assert_eq!(value["exporterInvocations"], 2);
    assert_eq!(value["exporterHashMatchedAtCapture"], true);
    assert_eq!(value["shortcutMatchedBeforeAndAfter"], true);
    let replay =
        google::ReceivedExport::from_archive(&saved, &Limits::default(), &Deadline::new(30000))
            .unwrap();
    assert_eq!(
        replay.read(1, 5, &Deadline::new(30000)).unwrap()["utf8"],
        "💡"
    );
    let calls = fs::read(config.join("calls")).unwrap();
    assert!(!cli().status.success());
    assert_eq!(fs::read(config.join("calls")).unwrap(), calls);
    assert_eq!(fs::read(&archive).unwrap(), saved);
    fs::remove_file(&archive).unwrap();
    fs::write(config.join("mode"), b"changed").unwrap();
    fs::write(
        config.join("shortcut-path"),
        root.join("notes.gdoc").to_str().unwrap(),
    )
    .unwrap();
    let changed = cli();
    assert!(!changed.status.success());
    assert!(
        String::from_utf8_lossy(&changed.stderr)
            .contains("google_workspace_shortcut_changed_after_export")
    );
    assert!(!archive.exists());
}
