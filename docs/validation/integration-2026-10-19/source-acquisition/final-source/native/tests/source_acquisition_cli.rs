use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Command, Output},
    thread,
};
use yoke::{capture::Deadline, source_acquisition::ReceivedSource};
fn invoke(args: &[&str], root: &Path) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_yoke"));
    command.args(args).current_dir(root);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    command.output().unwrap()
}
fn artifact(root: &Path, body: &[u8], format: &str, mime: &str) -> PathBuf {
    let source=ReceivedSource::from_response(&json!({"schemaVersion":1,"format":format,"requestedUrl":"https://example.com/fixture","httpStatus":200,"contentType":mime,"etag":null,"lastModified":null}),body,&Deadline::new(30000)).unwrap();
    let path = root.join("source.json");
    fs::write(&path, source.encode_archive(&Deadline::new(30000)).unwrap()).unwrap();
    path
}
#[test]
fn frozen_ranges_replay_without_a_workspace_and_source_flags_are_refused() {
    let root = tempfile::tempdir().unwrap();
    let input = artifact(
        root.path(),
        "A💡B\r\n".as_bytes(),
        "text",
        "text/plain; charset=utf-8",
    );
    let before = fs::read(&input).unwrap();
    let out = invoke(
        &[
            "source-acquisition",
            "read",
            "--input",
            input.to_str().unwrap(),
            "--start-byte",
            "1",
            "--end-byte",
            "5",
        ],
        root.path(),
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["utf8"], "💡");
    assert_eq!(value["networkConnections"], 0);
    assert_eq!(fs::read(&input).unwrap(), before);
    assert!(!root.path().join(".yoke").exists());
    let out = invoke(
        &[
            "--root",
            root.path().to_str().unwrap(),
            "source-acquisition",
            "read",
            "--input",
            input.to_str().unwrap(),
        ],
        root.path(),
    );
    assert!(!out.status.success());
    assert!(!root.path().join(".yoke").exists());
    let out = invoke(
        &["source-acquisition", "read", "--input", "relative.json"],
        root.path(),
    );
    assert!(!out.status.success());
}
#[test]
fn host_permissions_and_existing_output_precede_config_or_network_effects() {
    let root = tempfile::tempdir().unwrap();
    let config = root.path().join("config.json");
    let output = root.path().join("existing.json");
    fs::write(&output, b"foreign bytes").unwrap();
    let out = invoke(
        &[
            "source-acquisition",
            "capture",
            "--config",
            config.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
        ],
        root.path(),
    );
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr)
            .contains("source_acquisition_host_permission_required")
    );
    fs::write(&config,serde_json::to_vec(&json!({"url":"http://127.0.0.1:9/file","address":"127.0.0.1:9","format":"text","useSystemRoots":false})).unwrap()).unwrap();
    let out = invoke(
        &[
            "source-acquisition",
            "capture",
            "--config",
            config.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
            "--allow-network",
            "--allow-output",
            "--allow-loopback-http",
        ],
        root.path(),
    );
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("fresh file"));
    assert_eq!(fs::read(&output).unwrap(), b"foreign bytes");
    assert!(!root.path().join(".yoke").exists());
}
#[test]
fn real_cli_capture_is_create_only_and_ignores_ambient_proxy_settings() {
    let root = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let worker = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut got = Vec::new();
        let mut buf = [0u8; 1024];
        while !got.windows(4).any(|b| b == b"\r\n\r\n") {
            let n = socket.read(&mut buf).unwrap();
            assert!(n > 0);
            got.extend_from_slice(&buf[..n]);
        }
        socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 4\r\nConnection: close\r\n\r\nbody").unwrap();
        got
    });
    let config = root.path().join("config.json");
    let output = root.path().join("captured.json");
    fs::write(&config,serde_json::to_vec(&json!({"url":format!("http://{address}/fixture"),"address":address,"format":"text","useSystemRoots":false})).unwrap()).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_yoke"));
    command
        .args([
            "source-acquisition",
            "capture",
            "--config",
            config.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
            "--allow-network",
            "--allow-output",
            "--allow-loopback-http",
        ])
        .env("HTTP_PROXY", "http://127.0.0.1:1")
        .env("HTTPS_PROXY", "http://127.0.0.1:1")
        .env("ALL_PROXY", "http://127.0.0.1:1")
        .env("NO_PROXY", "")
        .current_dir(root.path());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let out = command.output().unwrap();
    let request = worker.join().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&request).starts_with("GET /fixture HTTP/1.1"));
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();
    let bytes = fs::read(&output).unwrap();
    assert_eq!(value["archiveHash"], yoke::model::hash(&bytes));
    let source = ReceivedSource::load_archive(&bytes, &Deadline::new(30000)).unwrap();
    assert_eq!(source.body(), b"body");
    assert!(!root.path().join(".yoke").exists());
}
fn pdf() -> Vec<u8> {
    let content = b"BT /F1 12 Tf 10 100 Td (HTTP fixture Gr\xfc\xdfe) Tj ET";
    let objects=[b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>".to_vec(),b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".to_vec(),[format!("<< /Length {} >>\nstream\n",content.len()).as_bytes(),content,b"\nendstream"].concat()];
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = vec![0usize];
    for (i, object) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend(object);
        out.extend(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len()).as_bytes());
    for offset in offsets.iter().skip(1) {
        out.extend(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            offsets.len()
        )
        .as_bytes(),
    );
    out
}
#[test]
fn explicit_pdf_text_uses_the_native_bounded_worker_without_store_or_original_path() {
    let root = tempfile::tempdir().unwrap();
    let body = pdf();
    let input = artifact(root.path(), &body, "pdf", "application/pdf");
    let before = fs::read(&input).unwrap();
    let out = invoke(
        &[
            "--max-bytes",
            "1000000",
            "source-acquisition",
            "read",
            "--input",
            input.to_str().unwrap(),
            "--pdf-text",
        ],
        root.path(),
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["sourceHash"], yoke::model::hash(&body));
    assert!(
        value["pdf"]["segments"][0]["text"]
            .as_str()
            .unwrap()
            .contains("HTTP fixture Grüße")
    );
    assert_eq!(value["networkConnections"], 0);
    assert_eq!(value["sourceAuthority"], false);
    assert_eq!(value["complete"], false);
    assert_eq!(fs::read(&input).unwrap(), before);
    assert!(!root.path().join(".yoke").exists());
}
