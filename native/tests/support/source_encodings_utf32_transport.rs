use super::*;

fn variants() -> [(Codec, bool, bool); 4] {
    [
        (Codec::Utf32, false, true),
        (Codec::Utf32, true, true),
        (Codec::Utf32Le, false, false),
        (Codec::Utf32Be, true, false),
    ]
}
fn raw32(text: &str, big: bool, bom: bool) -> Vec<u8> {
    let mut raw = if bom {
        if big {
            vec![0, 0, 0xfe, 0xff]
        } else {
            vec![0xff, 0xfe, 0, 0]
        }
    } else {
        Vec::new()
    };
    for ch in text.chars() {
        raw.extend_from_slice(&if big {
            (ch as u32).to_be_bytes()
        } else {
            (ch as u32).to_le_bytes()
        });
    }
    raw
}

#[test]
fn utf32_actual_stdio_keeps_raw_hashes_create_order_and_host_selection() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    for (codec, big, bom) in variants() {
        let root = tempfile::tempdir().unwrap();
        let config = tempfile::tempdir().unwrap();
        let text = "// é💡\r\nfn target() {}\r\n";
        let raw = raw32(text, big, bom);
        fs::write(root.path().join("main.rs"), &raw).unwrap();
        let capture = config.path().join("capture.json");
        fs::write(
            &capture,
            serde_json::to_vec(&json!({"sourceEncoding":{"kind":"strict_v1","codec":codec}}))
                .unwrap(),
        )
        .unwrap();
        let messages = [
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"utf32-fixture","version":"1"}}}),
            json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"ci_read","arguments":{"path":"main.rs"}}}),
            json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"ci_change_prepare","arguments":{"edits":[{"kind":"create","path":"new.rs","text":"// é💡\r\nfn fresh() {}\r\n"}]}}}),
            json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"ci_read","arguments":{"path":"main.rs","sourceEncoding":"utf8"}}}),
            json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"ci_read","arguments":{"path":"../main.rs"}}}),
        ];
        let input = messages
            .iter()
            .map(|m| format!("{m}\n"))
            .collect::<String>();
        let output = run(
            config.path(),
            &[
                "--root",
                root.path().to_str().unwrap(),
                "--capture-config",
                capture.to_str().unwrap(),
                "serve",
            ],
            input.as_bytes(),
        );
        let replies: Vec<Value> = String::from_utf8(output)
            .unwrap()
            .lines()
            .map(|s| serde_json::from_str(s).unwrap())
            .collect();
        let read = tool(replies.iter().find(|m| m["id"] == 2).unwrap());
        assert_eq!(read["status"], "ok", "{read}");
        assert_eq!(read["data"]["text"], text);
        assert_eq!(read["data"]["sourceView"]["rawContentHash"], hash(&raw));
        let prepared = tool(replies.iter().find(|m| m["id"] == 3).unwrap());
        assert_eq!(prepared["status"], "ok", "{prepared}");
        let plan: changes::Plan = serde_json::from_value(prepared["data"]["plan"].clone()).unwrap();
        assert_eq!(plan.source_encoding, codec.policy());
        let e = Engine::open(root.path()).unwrap();
        assert_eq!(
            e.store
                .read_blob(plan.files[0].after_hash.as_ref().unwrap())
                .unwrap(),
            raw32(
                "// é💡\r\nfn fresh() {}\r\n",
                codec == Codec::Utf32Be,
                codec == Codec::Utf32
            )
        );
        for id in [4, 5] {
            let reply = replies.iter().find(|m| m["id"] == id).unwrap();
            assert!(reply.get("error").is_some() || tool(reply)["status"] == "error");
        }
        assert_eq!(fs::read(root.path().join("main.rs")).unwrap(), raw);
        assert!(!root.path().join("new.rs").exists());
    }
}

#[test]
fn utf32_actual_http_uses_host_policy_and_refuses_request_codec() {
    use yoke::{
        mcp_http::{self, Config, Principal, Target},
        protocol::Profile,
    };
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    for (codec, big, bom) in variants() {
        let root = tempfile::tempdir().unwrap();
        let raw = raw32("// \u{e9}\u{1f4a1}\r\nfn target() {}\r\n", big, bom);
        fs::write(root.path().join("main.rs"), &raw).unwrap();
        let host: HostProfile =
            serde_json::from_value(json!({"sourceEncodings":{"global":codec}})).unwrap();
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
        assert_eq!(
            result["data"]["text"],
            "// \u{e9}\u{1f4a1}\r\nfn target() {}\r\n"
        );
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
}

#[test]
fn utf32_registered_lsp_maps_raw_anchors_and_refuses_rename() {
    use yoke::lsp::{
        self, ConfiguredServer, HostConfiguration, PassiveProfile, SemanticRequest, Server,
    };
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    for (codec, big, bom) in variants() {
        let (root, config, _) = fixture();
        let raw = raw32("// \u{e9}\u{1f4a1}\r\nfn target() {}\r\n", big, bom);
        fs::write(root.path().join("main.rs"), &raw).unwrap();
        let mut e = Engine::open(root.path()).unwrap();
        e.capture_options.source_encoding = codec.policy();
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
        assert_eq!(result["data"]["locations"][0]["endByte"], offset + 24);
        assert_eq!(result["data"]["locations"][0]["text"], "target");
        assert_eq!(result["data"]["locations"][0]["contentHash"], hash(&raw));
        assert_eq!(fs::read(root.path().join("main.rs")).unwrap(), raw);
    }
}

#[test]
fn utf32_executed_verification_writer_and_journal_preserve_order_and_prefix() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    for (codec, big, bom) in variants() {
        let root = tempfile::tempdir().unwrap();
        let raw = raw32("// \u{e9}\u{1f4a1}\r\nfn target() {}\r\n", big, bom);
        fs::write(root.path().join("main.rs"), &raw).unwrap();
        let mut e = Engine::open(root.path()).unwrap();
        e.capture_options.source_encoding = codec.policy();
        e.allow_writes = true;
        let (s, g, _, _) = e.index(&d()).unwrap();
        let v = yoke::query::source_view(&e.store, &s, "main.rs").unwrap();
        let start = v.to_raw(v.text().find("target").unwrap()).unwrap();
        let p=changes::prepare(&mut e.store,&s,&g,&json!({"edits":[{"kind":"replace","path":"main.rs","expectedHash":hash(&raw),"startByte":start,"endByte":start+24,"text":"café"}]}),&d()).unwrap();
        let expected = raw32("// \u{e9}\u{1f4a1}\r\nfn caf\u{e9}() {}\r\n", big, bom);
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
        assert_eq!(fs::read(root.path().join("main.rs")).unwrap(), raw);
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
            codec.policy()
        );
        assert_eq!(e.store.read_blob(&s.files[0].hash).unwrap(), raw);
    }
}
