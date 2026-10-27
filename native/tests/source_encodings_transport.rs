//! Deferred real-process/transport acceptance. This file was authored source-only;
//! it must run serially under the parent's native execution/resource guards.
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    path::Path,
    process::Command,
    time::Duration,
};
use yoke::{
    Engine, capture::Deadline, changes, model::hash, project_session::HostProfile,
    source_encoding::Codec,
};

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());
fn d() -> Deadline {
    Deadline::new(30_000)
}
fn utf16(text: &str) -> Vec<u8> {
    let mut raw = vec![0xff, 0xfe];
    for u in text.encode_utf16() {
        raw.extend_from_slice(&u.to_le_bytes());
    }
    raw
}
fn run(root: &Path, args: &[&str], input: &[u8]) -> Vec<u8> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_yoke"));
    command.current_dir(root).args(args);
    let output = yoke::process::run_capture(
        command,
        input,
        &d(),
        yoke::process::CaptureLimits {
            stdin_bytes: 1_000_000,
            stdout_bytes: 4_000_000,
            stderr_bytes: 1_000_000,
        },
    )
    .unwrap();
    assert_eq!(
        output.exit_code,
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stdin_complete
            && output.output_complete
            && !output.timed_out
            && !output.output_exceeded
            && !output.io_error
    );
    output.stdout
}
fn fixture() -> (tempfile::TempDir, tempfile::TempDir, Vec<u8>) {
    let root = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    let raw = utf16("// é💡\r\nfn target() {}\r\n");
    fs::write(root.path().join("main.rs"), &raw).unwrap();
    fs::write(
        config.path().join("capture.json"),
        r#"{"sourceEncoding":{"kind":"strict_v1","codec":"utf16"}}"#,
    )
    .unwrap();
    (root, config, raw)
}
fn tool(response: &Value) -> Value {
    serde_json::from_str(
        response["result"]["content"][0]["text"]
            .as_str()
            .expect("MCP content"),
    )
    .unwrap()
}

#[test]
fn actual_cli_capture_config_and_yaml_projected_session_read_original_encoded_source() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (root, config, raw) = fixture();
    let source = root.path().to_str().unwrap();
    let capture = config.path().join("capture.json");
    let output = run(
        config.path(),
        &[
            "--root",
            source,
            "--capture-config",
            capture.to_str().unwrap(),
            "read",
            "main.rs",
        ],
        b"",
    );
    let result: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(result["status"], "ok", "{result}");
    assert_eq!(result["data"]["text"], "// é💡\r\nfn target() {}\r\n");
    assert_eq!(result["data"]["contentHash"], hash(&raw));
    let yaml = config.path().join("project.yml");
    fs::write(&yaml, "encoding: UTF-16\n").unwrap();
    let selection = config.path().join("selection.json");
    fs::write(&selection,json!({"hostProfile":{},"files":[{"path":yaml,"sha256":hash(b"encoding: UTF-16\n"),"target":{"kind":"project","project":"default"}}]}).to_string()).unwrap();
    let host = config.path().join("host.json");
    run(
        config.path(),
        &[
            "session-config",
            "export",
            "--selection",
            selection.to_str().unwrap(),
            "--output",
            host.to_str().unwrap(),
            "--allow-configuration-read",
            "--allow-policy-projection",
            "--allow-output",
        ],
        b"",
    );
    let requests = config.path().join("requests.json");
    fs::write(&requests,json!([{"operation":"configuration","args":{}},{"operation":"fs.read","args":{"path":"main.rs"}}]).to_string()).unwrap();
    // The direct session CLI selects its native source route, with no Python host.
    let output = run(
        config.path(),
        &[
            "--root",
            source,
            "session",
            "--host-profile",
            host.to_str().unwrap(),
            "--requests",
            requests.to_str().unwrap(),
        ],
        b"",
    );
    let result: Value = serde_json::from_slice(&output).unwrap();
    assert!(result.to_string().contains("target"), "{result}");
    assert_eq!(fs::read(root.path().join("main.rs")).unwrap(), raw);
}

#[test]
fn actual_native_stdio_read_plan_and_unknown_codec_refusal_keep_raw_source_immutable() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (root, config, raw) = fixture();
    let source = root.path().to_str().unwrap();
    let capture = config.path().join("capture.json");
    let messages = [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"source-encoding-fixture","version":"1"}}}),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"ci_read","arguments":{"path":"main.rs"}}}),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"ci_change_prepare","arguments":{"edits":[{"kind":"create","path":"new.rs","text":"fn café() {}\r\n"}]}}}),
        json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"ci_read","arguments":{"path":"main.rs","sourceEncoding":"latin1"}}}),
    ];
    let input = messages
        .iter()
        .map(|m| format!("{m}\n"))
        .collect::<String>();
    let output = run(
        config.path(),
        &[
            "--root",
            source,
            "--capture-config",
            capture.to_str().unwrap(),
            "serve",
        ],
        input.as_bytes(),
    );
    let messages: Vec<Value> = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    let read = tool(messages.iter().find(|m| m["id"] == 2).unwrap());
    assert_eq!(read["status"], "ok", "{read}");
    assert_eq!(read["data"]["sourceView"]["rawContentHash"], hash(&raw));
    let prepared = tool(messages.iter().find(|m| m["id"] == 3).unwrap());
    assert_eq!(prepared["status"], "ok", "{prepared}");
    let p: changes::Plan = serde_json::from_value(prepared["data"]["plan"].clone()).unwrap();
    let e = Engine::open(root.path()).unwrap();
    assert_eq!(
        e.store
            .read_blob(p.files[0].after_hash.as_ref().unwrap())
            .unwrap(),
        utf16("fn café() {}\r\n")
    );
    run(
        config.path(),
        &[
            "--root",
            source,
            "--capture-config",
            capture.to_str().unwrap(),
            "approve",
            &p.id,
        ],
        b"",
    );
    let approval: changes::Approval = e.store.get_record("approval", &p.id).unwrap();
    assert_eq!(approval.plan_id, p.id);
    assert_eq!(
        tool(messages.iter().find(|m| m["id"] == 4).unwrap())["status"],
        "error"
    );
    assert!(!root.path().join("new.rs").exists());
    assert_eq!(fs::read(root.path().join("main.rs")).unwrap(), raw);
}

struct HttpReply {
    status: u16,
    session: Option<String>,
    body: Vec<u8>,
}
fn http(address: SocketAddr, session: Option<&str>, message: &Value) -> HttpReply {
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let body = serde_json::to_vec(message).unwrap();
    let session = session
        .map(|s| format!("Mcp-Session-Id: {s}\r\nMcp-Protocol-Version: 2025-11-25\r\n"))
        .unwrap_or_default();
    // Public, owned synthetic token; never read from host credentials.
    let header = format!(
        "POST /mcp HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nAccept: application/json, text/event-stream\r\nContent-Length: {}\r\n{session}\r\n",
        hex::encode([53u8; 32]),
        body.len()
    );
    stream.write_all(header.as_bytes()).unwrap();
    stream.write_all(&body).unwrap();
    let mut bytes = Vec::new();
    let mut buffer = [0; 8192];
    let deadline = d();
    loop {
        deadline.check().unwrap();
        let n = stream.read(&mut buffer).unwrap();
        assert!(n > 0);
        assert!(bytes.len() + n <= 4_000_000);
        bytes.extend_from_slice(&buffer[..n]);
        if let Some(end) = bytes.windows(4).position(|x| x == b"\r\n\r\n") {
            let h = std::str::from_utf8(&bytes[..end]).unwrap();
            let length: usize = h
                .lines()
                .find_map(|line| {
                    line.split_once(':')
                        .filter(|(key, _)| key.eq_ignore_ascii_case("content-length"))
                        .map(|(_, v)| v.trim().parse().unwrap())
                })
                .unwrap();
            assert!(length <= 4_000_000);
            if bytes.len() == end + 4 + length {
                let session = h.lines().find_map(|line| {
                    line.split_once(':')
                        .filter(|(key, _)| key.eq_ignore_ascii_case("mcp-session-id"))
                        .map(|(_, v)| v.trim().to_owned())
                });
                return HttpReply {
                    status: h.split_whitespace().nth(1).unwrap().parse().unwrap(),
                    session,
                    body: bytes[end + 4..].to_vec(),
                };
            }
            assert!(bytes.len() < end + 4 + length);
        }
    }
}

#[test]
fn actual_http_session_uses_host_codec_and_cannot_select_a_request_codec() {
    use yoke::{
        mcp_http::{self, Config, Principal, Target},
        protocol::Profile,
    };
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (root, _config, raw) = fixture();
    let host: HostProfile =
        serde_json::from_value(json!({"sourceEncodings":{"global":"utf16"}})).unwrap();
    let principal = Principal::new(
        "owned-source-encoding",
        zeroize::Zeroizing::new(vec![53; 32]),
        Target::new(root.path(), Profile::Native, host),
    )
    .unwrap();
    let server = mcp_http::serve(Config::default(), vec![principal]).unwrap();
    let addr = server.local_addr();
    let reply = http(
        addr,
        None,
        &json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"encoding-transport-fixture","version":"1"}}}),
    );
    assert_eq!(reply.status, 200);
    let session = reply.session.unwrap();
    assert_eq!(
        http(
            addr,
            Some(&session),
            &json!({"jsonrpc":"2.0","method":"notifications/initialized"})
        )
        .status,
        202
    );
    let reply = http(
        addr,
        Some(&session),
        &json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"ci_read","arguments":{"path":"main.rs"}}}),
    );
    assert_eq!(reply.status, 200);
    let result = tool(&serde_json::from_slice(&reply.body).unwrap());
    assert_eq!(result["status"], "ok", "{result}");
    assert_eq!(result["data"]["sourceView"]["rawContentHash"], hash(&raw));
    let reply = http(
        addr,
        Some(&session),
        &json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"ci_read","arguments":{"path":"main.rs","sourceEncoding":{"kind":"strict_v1","codec":"latin1"}}}}),
    );
    assert_eq!(
        tool(&serde_json::from_slice(&reply.body).unwrap())["status"],
        "error"
    );
    server.shutdown().unwrap();
    assert_eq!(fs::read(root.path().join("main.rs")).unwrap(), raw);
}

// Labeled protocol fixture: it checks actual UTF-8 files/didOpen/UTF-16 positions.
// It is not a language compiler or backend-runtime certification.
const LSP_FIXTURE: &str = r#"
const fs=require('fs'),{fileURLToPath}=require('url');let b=Buffer.alloc(0),doc;
const marker=process.argv[1];
function send(id,r){let x=Buffer.from(JSON.stringify({jsonrpc:'2.0',id,result:r}));process.stdout.write('Content-Length: '+x.length+'\r\n\r\n');process.stdout.write(x)}
function handle(m){if(m.method==='initialize'){fs.writeFileSync(marker,'started');send(m.id,{capabilities:{positionEncoding:'utf-16',definitionProvider:true,renameProvider:true}})}
else if(m.method==='textDocument/didOpen'){doc=m.params.textDocument;if(fs.readFileSync(fileURLToPath(doc.uri),'utf8')!==doc.text)throw Error('private copy not exact UTF8 view')}
else if(m.method==='textDocument/definition'){if(m.params.position.line!==1||m.params.position.character!==3||!doc.text.startsWith('// é💡\r\n'))throw Error('wrong source position');send(m.id,{uri:doc.uri,range:{start:{line:1,character:3},end:{line:1,character:9}}})}
else if(m.method==='shutdown')send(m.id,null);else if(m.method==='exit')process.exit(0);else if(m.id!==undefined)throw Error('unexpected request')}
process.stdin.on('data',x=>{b=Buffer.concat([b,x]);for(;;){let i=b.indexOf('\r\n\r\n');if(i<0)return;let n=Number(/Content-Length: (\d+)/i.exec(b.subarray(0,i).toString())[1]);if(b.length<i+4+n)return;let m=JSON.parse(b.subarray(i+4,i+4+n));b=b.subarray(i+4+n);handle(m)}})
"#;

#[test]
fn actual_registered_lsp_reads_private_utf8_copy_maps_raw_anchors_and_refuses_encoded_rename_before_spawn()
 {
    use yoke::lsp::{
        self, ConfiguredServer, HostConfiguration, PassiveProfile, SemanticRequest, Server,
    };
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (root, config, raw) = fixture();
    let mut e = Engine::open(root.path()).unwrap();
    e.capture_options.source_encoding = Codec::Utf16.policy();
    let (s, g, _, _) = e.index(&d()).unwrap();
    let marker = config.path().join("started");
    let server = Server {
        command: "node".into(),
        args: vec![
            "-e".into(),
            LSP_FIXTURE.into(),
            marker.display().to_string(),
        ],
        language: "rust".into(),
        trust: true,
    };
    let registered = ConfiguredServer {
        server: server.clone(),
        configuration: HostConfiguration {
            passive_profile: Some(PassiveProfile::ContractFixture),
            ..Default::default()
        },
    };
    let view = yoke::query::source_view(&e.store, &s, "main.rs").unwrap();
    let offset = view.to_raw(view.text().find("target").unwrap()).unwrap();
    let bad = lsp::run_at_configured(
        &mut e,
        &registered,
        &s,
        &g,
        SemanticRequest {
            path: "main.rs",
            offset,
            operation: "rename",
            new_name: Some("renamed"),
        },
        &json!({}),
        &d(),
    );
    assert!(
        bad.unwrap_err()
            .to_string()
            .contains("source_encoding_capability_unavailable")
    );
    assert!(!marker.exists());
    e.register_configured_server(registered).unwrap();
    let result=e.invoke("ci_find",json!({"snapshotId":s.id,"languageServer":{"operation":"definition","path":"main.rs","offset":offset}})).unwrap();
    assert_eq!(result["status"], "ok", "{result}");
    assert!(marker.exists());
    assert_eq!(result["data"]["locations"][0]["startByte"], offset);
    assert_eq!(result["data"]["locations"][0]["text"], "target");
    assert_eq!(result["data"]["locations"][0]["contentHash"], hash(&raw));
    assert_eq!(fs::read(root.path().join("main.rs")).unwrap(), raw);
}

#[test]
fn actual_executed_raw_byte_verification_approval_and_writer_journal_preserve_selected_codec() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (root, _config, raw) = fixture();
    let mut e = Engine::open(root.path()).unwrap();
    e.capture_options.source_encoding = Codec::Utf16.policy();
    e.allow_writes = true;
    let (s, g, _, _) = e.index(&d()).unwrap();
    let v = yoke::query::source_view(&e.store, &s, "main.rs").unwrap();
    let start = v.to_raw(v.text().find("target").unwrap()).unwrap();
    let p=changes::prepare(&mut e.store,&s,&g,&json!({"edits":[{"kind":"replace","path":"main.rs","expectedHash":hash(&raw),"startByte":start,"endByte":start+12,"text":"café"}]}),&d()).unwrap();
    let expected = utf16("// é💡\r\nfn café() {}\r\n");
    let verification = |expected: &[u8]| {
        #[cfg(windows)]
        {
            let exe = std::path::PathBuf::from(std::env::var_os("SystemRoot").unwrap())
                .join("System32/WindowsPowerShell/v1.0/powershell.exe");
            let script = format!(
                "$b=[IO.File]::ReadAllBytes('main.rs'); $h=[Security.Cryptography.SHA256]::Create(); if ([BitConverter]::ToString($h.ComputeHash($b)).Replace('-','').ToLowerInvariant() -ne '{}') {{ exit 19 }}",
                hash(expected)
            );
            (
                exe,
                vec![
                    "-NoProfile".into(),
                    "-NonInteractive".into(),
                    "-Command".into(),
                    script,
                ],
            )
        }
        #[cfg(not(windows))]
        {
            (
                std::path::PathBuf::from("/bin/sh"),
                vec![
                    "-c".into(),
                    format!(
                        "test \"$(od -An -tx1 main.rs | tr -d ' \\n')\" = '{}'",
                        hex::encode(expected)
                    ),
                ],
            )
        }
    };
    changes::approve(&e.store, &p.id, false).unwrap();
    let (command, args) = verification(&raw);
    let wrong =
        yoke::verify::execute(&e, &p.id, command.to_str().unwrap(), &args, true, &d()).unwrap();
    assert_eq!(wrong["status"], "failed");
    assert!(
        changes::apply(
            &e.store,
            root.path(),
            &json!({"planId":p.id,"idempotencyKey":"unverified","mode":"workspace"}),
            true,
            &d()
        )
        .is_err()
    );
    let (command, args) = verification(&expected);
    let checked =
        yoke::verify::execute(&e, &p.id, command.to_str().unwrap(), &args, true, &d()).unwrap();
    assert_eq!(checked["status"], "passed", "{checked}");
    let receipt = changes::apply(
        &e.store,
        root.path(),
        &json!({"planId":p.id,"idempotencyKey":"verified","mode":"workspace"}),
        true,
        &d(),
    )
    .unwrap();
    assert_eq!(receipt.status, "applied");
    assert_eq!(fs::read(root.path().join("main.rs")).unwrap(), expected);
    let journal: Value = e.store.get_record("journal", &receipt.id).unwrap();
    assert_eq!(journal["planId"], p.id);
    assert_eq!(
        changes::load_checked_plan(&e.store, &p.id, &d())
            .unwrap()
            .source_encoding,
        Codec::Utf16.policy()
    );
    assert_eq!(e.store.read_blob(&s.files[0].hash).unwrap(), raw);
}

#[test]
fn utf8_sig_registered_lsp_maps_raw_anchors_and_refuses_rename() {
    use yoke::lsp::{
        self, ConfiguredServer, HostConfiguration, PassiveProfile, SemanticRequest, Server,
    };
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    for bom in [false, true] {
        let (root, config, _) = fixture();
        let mut raw = if bom {
            vec![0xef, 0xbb, 0xbf]
        } else {
            Vec::new()
        };
        raw.extend_from_slice("// \u{e9}\u{1f4a1}\r\nfn target() {}\r\n".as_bytes());
        fs::write(root.path().join("main.rs"), &raw).unwrap();
        let mut e = Engine::open(root.path()).unwrap();
        e.capture_options.source_encoding = Codec::Utf8Sig.policy();
        let (s, g, _, _) = e.index(&d()).unwrap();
        let marker = config.path().join("started");
        let server = Server {
            command: "node".into(),
            args: vec![
                "-e".into(),
                LSP_FIXTURE.into(),
                marker.display().to_string(),
            ],
            language: "rust".into(),
            trust: true,
        };
        let registered = ConfiguredServer {
            server: server.clone(),
            configuration: HostConfiguration {
                passive_profile: Some(PassiveProfile::ContractFixture),
                ..Default::default()
            },
        };
        let view = yoke::query::source_view(&e.store, &s, "main.rs").unwrap();
        let offset = view.to_raw(view.text().find("target").unwrap()).unwrap();
        let bad = lsp::run_at_configured(
            &mut e,
            &registered,
            &s,
            &g,
            SemanticRequest {
                path: "main.rs",
                offset,
                operation: "rename",
                new_name: Some("renamed"),
            },
            &json!({}),
            &d(),
        );
        assert!(
            bad.unwrap_err()
                .to_string()
                .contains("source_encoding_capability_unavailable")
        );
        assert!(!marker.exists());
        e.register_configured_server(registered).unwrap();
        let result=e.invoke("ci_find",json!({"snapshotId":s.id,"languageServer":{"operation":"definition","path":"main.rs","offset":offset}})).unwrap();
        assert_eq!(result["status"], "ok", "{result}");
        assert!(marker.exists());
        assert_eq!(result["data"]["locations"][0]["startByte"], offset);
        assert_eq!(result["data"]["locations"][0]["text"], "target");
        assert_eq!(result["data"]["locations"][0]["contentHash"], hash(&raw));
        assert_eq!(fs::read(root.path().join("main.rs")).unwrap(), raw);
    }
}

#[test]
fn utf8_sig_executed_verification_and_writer_preserve_signature() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (root, _config, _) = fixture();
    let mut raw = vec![0xef, 0xbb, 0xbf];
    raw.extend_from_slice("// \u{e9}\u{1f4a1}\r\nfn target() {}\r\n".as_bytes());
    fs::write(root.path().join("main.rs"), &raw).unwrap();
    let mut e = Engine::open(root.path()).unwrap();
    e.capture_options.source_encoding = Codec::Utf8Sig.policy();
    e.allow_writes = true;
    let (s, g, _, _) = e.index(&d()).unwrap();
    let v = yoke::query::source_view(&e.store, &s, "main.rs").unwrap();
    let start = v.to_raw(v.text().find("target").unwrap()).unwrap();
    let p=changes::prepare(&mut e.store,&s,&g,&json!({"edits":[{"kind":"replace","path":"main.rs","expectedHash":hash(&raw),"startByte":start,"endByte":start+6,"text":"café"}]}),&d()).unwrap();
    let mut expected = vec![0xef, 0xbb, 0xbf];
    expected.extend_from_slice("// \u{e9}\u{1f4a1}\r\nfn caf\u{e9}() {}\r\n".as_bytes());
    let verification = |expected: &[u8]| {
        #[cfg(windows)]
        {
            let exe = std::path::PathBuf::from(std::env::var_os("SystemRoot").unwrap())
                .join("System32/WindowsPowerShell/v1.0/powershell.exe");
            let script = format!(
                "$b=[IO.File]::ReadAllBytes('main.rs'); $h=[Security.Cryptography.SHA256]::Create(); if ([BitConverter]::ToString($h.ComputeHash($b)).Replace('-','').ToLowerInvariant() -ne '{}') {{ exit 19 }}",
                hash(expected)
            );
            (
                exe,
                vec![
                    "-NoProfile".into(),
                    "-NonInteractive".into(),
                    "-Command".into(),
                    script,
                ],
            )
        }
        #[cfg(not(windows))]
        {
            (
                std::path::PathBuf::from("/bin/sh"),
                vec![
                    "-c".into(),
                    format!(
                        "test \"$(od -An -tx1 main.rs | tr -d ' \\n')\" = '{}'",
                        hex::encode(expected)
                    ),
                ],
            )
        }
    };
    changes::approve(&e.store, &p.id, false).unwrap();
    let (command, args) = verification(&raw);
    let wrong =
        yoke::verify::execute(&e, &p.id, command.to_str().unwrap(), &args, true, &d()).unwrap();
    assert_eq!(wrong["status"], "failed");
    assert!(
        changes::apply(
            &e.store,
            root.path(),
            &json!({"planId":p.id,"idempotencyKey":"unverified","mode":"workspace"}),
            true,
            &d()
        )
        .is_err()
    );
    let (command, args) = verification(&expected);
    let checked =
        yoke::verify::execute(&e, &p.id, command.to_str().unwrap(), &args, true, &d()).unwrap();
    assert_eq!(checked["status"], "passed", "{checked}");
    let receipt = changes::apply(
        &e.store,
        root.path(),
        &json!({"planId":p.id,"idempotencyKey":"verified","mode":"workspace"}),
        true,
        &d(),
    )
    .unwrap();
    assert_eq!(receipt.status, "applied");
    assert_eq!(fs::read(root.path().join("main.rs")).unwrap(), expected);
    let journal: Value = e.store.get_record("journal", &receipt.id).unwrap();
    assert_eq!(journal["planId"], p.id);
    assert_eq!(
        changes::load_checked_plan(&e.store, &p.id, &d())
            .unwrap()
            .source_encoding,
        Codec::Utf8Sig.policy()
    );
    assert_eq!(e.store.read_blob(&s.files[0].hash).unwrap(), raw);
}
