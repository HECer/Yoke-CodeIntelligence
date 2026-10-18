use std::process::Command;

#[test]
fn optional_http_python_requires_independent_host_authority_before_credentials_or_state() {
    for (profile, shell, flags, expected) in [
        ("serena", false, vec![], "requires --trust-python"),
        (
            "native",
            false,
            vec!["--trust-python"],
            "only in the Serena profile",
        ),
        (
            "serena",
            true,
            vec!["--trust-python"],
            "requires --trust-shell",
        ),
    ] {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("host.json");
        let mut runtime = serde_json::json!({"python":root.path().join("never-spawn.exe")});
        if shell {
            runtime["shell"] = serde_json::json!({"command":root.path().join("never-shell.exe")});
        }
        std::fs::write(
            &config,
            serde_json::json!({"principals":[{
                "id":"fixture", "tokenCredentialName":"not-accessed", "profile":profile,
                "agentRepl":runtime
            }]})
            .to_string(),
        )
        .unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_yoke"))
            .arg("--root")
            .arg(root.path())
            .args([
                "serve-http",
                "--trust-service",
                "--duration-ms",
                "1",
                "--config",
            ])
            .arg(&config)
            .args(flags)
            .output()
            .unwrap();
        assert!(!output.status.success());
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(error.contains(expected), "Expected {expected}: {error}");
        assert!(!root.path().join(".yoke").exists());
    }
}

#[test]
fn public_http_cli_requires_host_trust_before_opening_source_state() {
    let root = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(root.path())
        .args([
            "serve-http",
            "--config",
            "not-read.json",
            "--duration-ms",
            "1",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("requires --trust-service"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!root.path().join(".yoke").exists());
}

#[test]
fn public_http_host_configuration_rejects_inline_tokens_before_credential_access() {
    let root = tempfile::tempdir().unwrap();
    let config = root.path().join("host.json");
    std::fs::write(&config,r#"{"principals":[{"id":"fixture","tokenCredentialName":"not-accessed","token":"sentinel-private-host-value"}]}"#).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(root.path())
        .args([
            "serve-http",
            "--trust-service",
            "--duration-ms",
            "1",
            "--config",
        ])
        .arg(&config)
        .output()
        .unwrap();
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("Invalid host input schema"), "{error}");
    assert!(!error.contains("sentinel-private-host-value"));
    assert!(!root.path().join(".yoke").exists());
}

#[cfg(windows)]
#[test]
#[ignore = "Actual Windows Credential Manager and own bounded loopback MCP CLI service"]
fn actual_public_http_cli_resolves_credential_reference_and_authenticates() {
    use std::io::{BufRead, Read, Write};
    let mut nonce = [0u8; 16];
    getrandom::fill(&mut nonce).unwrap();
    let name = format!("mcp-cli-test-{}", hex::encode(nonce));
    struct Created(String);
    impl Drop for Created {
        fn drop(&mut self) {
            #[link(name = "Advapi32")]
            unsafe extern "system" {
                fn CredDeleteW(target: *const u16, kind: u32, flags: u32) -> i32;
            }
            assert!(
                self.0
                    .starts_with("yoke-code-intelligence/service/token/mcp-cli-test-")
            );
            let wide: Vec<_> = self.0.encode_utf16().chain(Some(0)).collect();
            unsafe {
                CredDeleteW(wide.as_ptr(), 1, 0);
            }
        }
    }
    yoke::host_credentials::generate_token(&name).unwrap();
    let _created = Created(format!("yoke-code-intelligence/service/token/{name}"));
    let token = yoke::host_credentials::read_token(&name).unwrap();
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("main.rs"), "fn main() {}\n").unwrap();
    let config = root.path().join("host.json");
    let python = Command::new("python")
        .args(["-I", "-c", "import sys;print(sys.executable)"])
        .output()
        .unwrap();
    assert!(python.status.success());
    let python = String::from_utf8(python.stdout).unwrap();
    std::fs::write(&config,serde_json::json!({"principals":[{"id":"fixture","tokenCredentialName":name,"profile":"serena","agentRepl":{"python":python.trim()}}]}).to_string()).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(root.path())
        .args([
            "serve-http",
            "--trust-service",
            "--trust-python",
            "--duration-ms",
            "4000",
            "--config",
        ])
        .arg(&config)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdout = std::io::BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    stdout.read_line(&mut line).unwrap();
    let ready: serde_json::Value =
        serde_json::from_str(&line).expect("CLI must emit bounded listener metadata");
    assert_eq!(ready["status"], "listening");
    let address = ready["address"].as_str().unwrap();
    let request=serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","clientInfo":{"name":"cli-fixture","version":"1"},"capabilities":{}}}).to_string();
    let mut socket = std::net::TcpStream::connect(address).unwrap();
    socket
        .set_read_timeout(Some(std::time::Duration::from_secs(3)))
        .unwrap();
    let bearer = zeroize::Zeroizing::new(hex::encode(token.as_slice()));
    write!(socket,"POST /mcp HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {}\r\nAccept: application/json, text/event-stream\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{request}",bearer.as_str(),request.len()).unwrap();
    let mut response = String::new();
    if let Err(error) = socket.read_to_string(&mut response) {
        let status = response.lines().next().unwrap_or("no response status");
        panic!(
            "HTTP EOF failed: {error}; received {} bytes, status={status}",
            response.len()
        );
    }
    assert!(
        response.starts_with("HTTP/1.1 200 "),
        "Authenticated CLI request failed"
    );
    assert!(!response.contains(bearer.as_str()));
    let body = response.split_once("\r\n\r\n").unwrap().1;
    let value: serde_json::Value = serde_json::from_str(body).unwrap();
    assert_eq!(value["result"]["protocolVersion"], "2025-11-25");
    let sid = response
        .lines()
        .find_map(|line| {
            line.split_once(':')
                .filter(|(name, _)| name.eq_ignore_ascii_case("mcp-session-id"))
                .map(|(_, value)| value.trim().to_owned())
        })
        .unwrap();
    let mut request_id = 1;
    let mut tool = |name: &str, arguments: serde_json::Value| -> serde_json::Value {
        request_id += 1;
        let request = serde_json::json!({"jsonrpc":"2.0","id":request_id,"method":"tools/call","params":{"name":name,"arguments":arguments}}).to_string();
        let mut socket = std::net::TcpStream::connect(address).unwrap();
        socket
            .set_read_timeout(Some(std::time::Duration::from_secs(3)))
            .unwrap();
        write!(socket,"POST /mcp HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {}\r\nMcp-Session-Id: {sid}\r\nMcp-Protocol-Version: 2025-11-25\r\nAccept: application/json, text/event-stream\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{request}",bearer.as_str(),request.len()).unwrap();
        let mut response = String::new();
        socket.read_to_string(&mut response).unwrap();
        assert!(
            response.starts_with("HTTP/1.1 200 "),
            "CLI runtime request failed"
        );
        assert!(!response.contains(bearer.as_str()));
        let value: serde_json::Value =
            serde_json::from_str(response.split_once("\r\n\r\n").unwrap().1).unwrap();
        assert_eq!(value["result"]["isError"], false);
        serde_json::from_str(value["result"]["content"][0]["text"].as_str().unwrap()).unwrap()
    };
    let current = tool("get_current_config", serde_json::json!({}));
    let label = current["data"]["sessionId"].as_str().unwrap();
    assert_eq!(
        tool(
            "serena_repl",
            serde_json::json!({"session_id":label,"code":"counter=41\ncounter"})
        )["data"]["result"],
        41
    );
    assert_eq!(
        tool(
            "serena_repl",
            serde_json::json!({"session_id":label,"code":"counter+1"})
        )["data"]["result"],
        42
    );
    stdout.read_to_string(&mut line).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "CLI bounded service did not close cleanly"
    );
    assert!(!line.contains(bearer.as_str()));
    assert!(!String::from_utf8_lossy(&output.stderr).contains(bearer.as_str()));
}
