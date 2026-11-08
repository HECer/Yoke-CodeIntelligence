use serde_json::{Value, json};
use std::{
    fs,
    sync::{Mutex, MutexGuard},
};
use tempfile::TempDir;
use yoke::{
    Engine,
    capture::Deadline,
    lsp::{ConfiguredServer, HostConfiguration, PassiveProfile, Server},
};

// This real stdio process is a labeled protocol fixture, not a semantic compiler.
const STDIO_FIXTURE: &str = r#"
const fs = require('node:fs');
const {fileURLToPath} = require('node:url');
const marker = process.argv[1];
let input = Buffer.alloc(0), documents = new Map();
function send(id, result) {
  const bytes = Buffer.from(JSON.stringify({jsonrpc:'2.0', id, result}));
  process.stdout.write('Content-Length: ' + bytes.length + '\r\n\r\n');
  process.stdout.write(bytes);
}
function handle(m) {
  if (m.method === 'initialize') {
    fs.writeFileSync(marker, 'started');
    send(m.id, {capabilities:{positionEncoding:'utf-16',definitionProvider:true,
      declarationProvider:true,referencesProvider:true,implementationProvider:true,
      hoverProvider:true,documentSymbolProvider:true}});
    return;
  }
  if (m.method === 'textDocument/didOpen') {
    documents.set(m.params.textDocument.uri, m.params.textDocument);
    return;
  }
  if (m.method === 'shutdown') { send(m.id, null); return; }
  if (m.method === 'exit') { process.exit(0); }
  if (m.id === undefined) return;
  const doc = documents.get(m.params.textDocument.uri);
  if (!doc || fs.readFileSync(fileURLToPath(doc.uri), 'utf8') !== doc.text || !doc.text.includes('old_target')) {
    throw new Error('Fixture did not receive the exact old immutable source');
  }
  if (doc.uri.endsWith('/main.rs') && m.method !== 'textDocument/documentSymbol' &&
      (m.params.position.line !== 2 || m.params.position.character !== 14)) {
    throw new Error('Fixture received the wrong requested UTF-16 source position');
  }
  const ranges = [...doc.text.matchAll(/old_target/g)].map(match => {
    const before = doc.text.slice(0, match.index), lines = before.split('\n');
    const start = {line:lines.length-1, character:lines.at(-1).length};
    return {start, end:{line:start.line, character:start.character+10}};
  });
  const location = range => ({uri:doc.uri, range});
  if (['textDocument/definition','textDocument/declaration','textDocument/implementation'].includes(m.method)) {
    send(m.id, location(ranges[0]));
  } else if (m.method === 'textDocument/references') {
    if (m.params.context.includeDeclaration !== true) throw new Error('Missing declaration context');
    send(m.id, ranges.map(location));
  } else if (m.method === 'textDocument/hover') {
    send(m.id, {contents:{kind:'markdown', value:'Labeled protocol fixture hover'}, range:ranges[0]});
  } else if (m.method === 'textDocument/documentSymbol') {
    send(m.id, [{name:'old_target',kind:12,range:ranges[0],selectionRange:ranges[0]}]);
  } else { throw new Error('Mutating or unrecognized fixture request: '+m.method); }
}
process.stdin.on('data', chunk => {
  input = Buffer.concat([input, chunk]);
  while (true) {
    const headerEnd = input.indexOf('\r\n\r\n');
    if (headerEnd < 0) return;
    const size = Number(/Content-Length: (\d+)/i.exec(input.subarray(0, headerEnd).toString())[1]);
    if (input.length < headerEnd+4+size) return;
    const message = JSON.parse(input.subarray(headerEnd+4, headerEnd+4+size));
    input = input.subarray(headerEnd+4+size);
    handle(message);
  }
});
"#;
const OLD: &str = "// 🌍\nfn old_target() {}\nfn caller() { old_target(); }\n";
static PROTOCOL_PROCESS: Mutex<()> = Mutex::new(());
fn serial_fixture() -> MutexGuard<'static, ()> {
    PROTOCOL_PROCESS.lock().unwrap_or_else(|e| e.into_inner())
}
fn fixture() -> (TempDir, TempDir, Engine, String) {
    let root = TempDir::new().unwrap();
    let provider = TempDir::new().unwrap();
    fs::write(root.path().join("main.rs"), OLD).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let (snapshot, _, _, _) = engine.index(&Deadline::new(20_000)).unwrap();
    (root, provider, engine, snapshot.id)
}
fn server(provider: &TempDir, language: &str) -> ConfiguredServer {
    ConfiguredServer {
        server: Server {
            command: "node".into(),
            args: vec![
                "-e".into(),
                STDIO_FIXTURE.into(),
                provider.path().join("started").display().to_string(),
            ],
            language: language.into(),
            trust: true,
        },
        configuration: HostConfiguration {
            passive_profile: Some(PassiveProfile::ContractFixture),
            ..Default::default()
        },
    }
}
fn call(engine: &mut Engine, args: Value) -> Value {
    let input = format!(
        "{}\n{}\n",
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"ci_find","arguments":args}})
    );
    let mut output = Vec::new();
    yoke::protocol::serve(engine, std::io::Cursor::new(input), &mut output).unwrap();
    let reply: Value =
        serde_json::from_str(String::from_utf8(output).unwrap().lines().nth(1).unwrap()).unwrap();
    serde_json::from_str(
        reply["result"]["content"][0]["text"]
            .as_str()
            .expect("MCP tool response"),
    )
    .unwrap()
}
fn request(snapshot: &str, operation: &str) -> Value {
    json!({"snapshotId":snapshot,"languageServer":{"operation":operation,"path":"main.rs","offset":41},"timeoutMs":20_000})
}

#[test]
fn unconfigured_semantic_navigation_fails_explicitly_without_execution() {
    let (root, provider, mut engine, snapshot) = fixture();
    let response = call(&mut engine, request(&snapshot, "definition"));
    assert_eq!(response["status"], "error");
    assert!(
        response["error"]["message"]
            .as_str()
            .unwrap()
            .contains("No host-authorized"),
        "{response}"
    );
    assert_eq!(response["metrics"]["localOnly"], true);
    assert_eq!(response["metrics"]["languageServerAttempts"], 0);
    assert_eq!(response["metrics"]["externalUsage"], "none");
    assert!(!provider.path().join("started").exists());
    assert_eq!(
        fs::read_to_string(root.path().join("main.rs")).unwrap(),
        OLD
    );
}

#[test]
fn registered_stdio_navigation_binds_old_requested_snapshot_after_disk_changes() {
    let _serial = serial_fixture();
    let (root, provider, mut engine, snapshot) = fixture();
    engine
        .register_configured_server(server(&provider, "rust"))
        .unwrap();
    let hash = engine.store.snapshot(&snapshot).unwrap().files[0]
        .hash
        .clone();
    let disk = "fn disk_changed_only() {}\n";
    fs::write(root.path().join("main.rs"), disk).unwrap();
    for operation in [
        "definition",
        "declaration",
        "references",
        "implementation",
        "hover",
        "symbols",
    ] {
        let response = call(&mut engine, request(&snapshot, operation));
        assert_eq!(response["status"], "ok", "{response}");
        assert_eq!(response["snapshotId"], snapshot);
        assert_eq!(response["data"]["snapshotId"], snapshot);
        assert_eq!(response["data"]["operation"], operation);
        assert_eq!(response["data"]["path"], "main.rs");
        assert_eq!(response["data"]["contentHash"], hash);
        assert_eq!(response["data"]["complete"], false);
        assert_eq!(response["metrics"]["localOnly"], false);
        assert_eq!(response["metrics"]["languageServerAttempts"], 1);
        assert_eq!(response["metrics"]["externalUsage"], "unknown");
        assert_eq!(response["metrics"]["modelCalls"], 0);
        if !matches!(operation, "hover" | "symbols") {
            let locations = response["data"]["locations"].as_array().unwrap();
            assert_eq!(
                locations.len(),
                if operation == "references" { 2 } else { 1 }
            );
            assert_eq!(locations[0]["path"], "main.rs");
            assert_eq!(locations[0]["contentHash"], hash);
            assert_eq!(locations[0]["startByte"], 11);
            assert_eq!(locations[0]["endByte"], 21);
            assert_eq!(locations[0]["text"], "old_target");
            if operation == "references" {
                assert_eq!(locations[1]["startByte"], 41);
                assert_eq!(locations[1]["endByte"], 51);
            }
        }
        assert_eq!(
            fs::read_to_string(root.path().join("main.rs")).unwrap(),
            disk
        );
    }
    assert!(provider.path().join("started").exists());
}

#[test]
fn mutating_authority_mixed_and_malformed_requests_never_start_a_provider() {
    let (root, provider, mut engine, snapshot) = fixture();
    engine
        .register_configured_server(server(&provider, "rust"))
        .unwrap();
    let valid = request(&snapshot, "definition");
    let mut invalid = Vec::new();
    for field in [
        "command",
        "args",
        "arg",
        "trust",
        "trustServer",
        "language",
        "newName",
        "edits",
        "write",
        "applyEdit",
    ] {
        let mut args = valid.clone();
        args["languageServer"][field] = json!(true);
        invalid.push(args);
    }
    for operation in [
        "rename",
        "workspace/executeCommand",
        "executeCommand",
        "workspace/applyEdit",
    ] {
        let mut args = valid.clone();
        args["languageServer"]["operation"] = json!(operation);
        invalid.push(args);
    }
    for (field, value) in [
        ("query", json!("old_target")),
        ("kind", json!("semantic")),
        ("model", json!("fixture")),
        ("vector", json!([1.0])),
        ("path", json!("main.rs")),
        ("offset", json!(0)),
        ("regex", json!(false)),
        ("limit", json!(1)),
        ("textSearch", json!(true)),
    ] {
        let mut args = valid.clone();
        args[field] = value;
        invalid.push(args);
    }
    for field in ["operation", "path", "offset"] {
        let mut args = valid.clone();
        args["languageServer"]
            .as_object_mut()
            .unwrap()
            .remove(field);
        invalid.push(args);
    }
    for value in [Value::Null, json!(true), json!([]), json!("definition")] {
        let mut args = valid.clone();
        args["languageServer"] = value;
        invalid.push(args);
    }
    for value in [json!(-1), json!(1.5), json!(4_000_001), json!("41")] {
        let mut args = valid.clone();
        args["languageServer"]["offset"] = value;
        invalid.push(args);
    }
    for args in invalid {
        let response = call(&mut engine, args);
        assert_eq!(response["status"], "error", "{response}");
        assert_eq!(response["metrics"]["localOnly"], true);
        assert_eq!(response["metrics"]["languageServerAttempts"], 0);
    }
    assert!(!provider.path().join("started").exists());
    assert_eq!(
        fs::read_to_string(root.path().join("main.rs")).unwrap(),
        OLD
    );
}

#[test]
fn captured_path_language_and_utf8_offset_are_checked_before_execution() {
    let (root, provider, mut engine, snapshot) = fixture();
    engine
        .register_configured_server(server(&provider, "rust"))
        .unwrap();
    for path in ["missing.rs", "../main.rs", ".git/config", "main\\rs"] {
        let mut args = request(&snapshot, "definition");
        args["languageServer"]["path"] = json!(path);
        let response = call(&mut engine, args);
        assert_eq!(response["status"], "error", "{response}");
        assert_eq!(response["metrics"]["languageServerAttempts"], 0);
    }
    for offset in [5, 100] {
        let mut args = request(&snapshot, "definition");
        args["languageServer"]["offset"] = json!(offset);
        let response = call(&mut engine, args);
        assert_eq!(response["status"], "error", "{response}");
        assert_eq!(response["metrics"]["languageServerAttempts"], 0);
    }
    fs::write(root.path().join("notes.ts"), "export const value = 1;\n").unwrap();
    let (new, _, _, _) = engine.index(&Deadline::new(20_000)).unwrap();
    let response = call(
        &mut engine,
        json!({"snapshotId":new.id,"languageServer":{"operation":"definition","path":"notes.ts","offset":0}}),
    );
    assert_eq!(response["status"], "error");
    assert!(
        response["error"]["message"]
            .as_str()
            .unwrap()
            .contains("No host-authorized")
    );
    assert!(!provider.path().join("started").exists());
}

#[test]
fn ci_find_requires_one_search_mode_and_keeps_eight_advertised_tools() {
    assert!(yoke::validate("ci_find", &json!({})).is_err());
    assert!(yoke::validate("ci_find", &json!({"query":"old_target"})).is_ok());
    assert!(
        yoke::validate(
            "ci_find",
            &json!({"kind":"semantic","model":"fixture","vector":[1.0]})
        )
        .is_ok()
    );
    assert!(
        yoke::validate(
            "ci_find",
            &json!({"languageServer":{"operation":"symbols","path":"main.rs","offset":0}})
        )
        .is_ok()
    );
    let tools = yoke::protocol::tools();
    assert_eq!(tools.as_array().unwrap().len(), 8);
    let find = tools
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "ci_find")
        .unwrap();
    assert_eq!(find["annotations"]["readOnlyHint"], true);
    assert_eq!(find["annotations"]["openWorldHint"], true);
    assert_eq!(
        find["inputSchema"]["properties"]["languageServer"]["additionalProperties"],
        false
    );
}

#[test]
fn tsx_navigation_can_use_an_explicitly_registered_typescript_provider() {
    let _serial = serial_fixture();
    let (root, provider, mut engine, _) = fixture();
    fs::remove_file(root.path().join("main.rs")).unwrap();
    let source = "// 🌍\nfunction old_target() { return <b/>; }\nold_target();\n";
    fs::write(root.path().join("main.tsx"), source).unwrap();
    let (snapshot, _, _, _) = engine.index(&Deadline::new(20_000)).unwrap();
    engine
        .register_configured_server(server(&provider, "typescript"))
        .unwrap();
    let response = call(
        &mut engine,
        json!({"snapshotId":snapshot.id,"languageServer":{"operation":"definition","path":"main.tsx","offset":0},"timeoutMs":20_000}),
    );
    assert_eq!(response["status"], "ok", "{response}");
    assert_eq!(response["data"]["locations"][0]["text"], "old_target");
    assert_eq!(
        fs::read_to_string(root.path().join("main.tsx")).unwrap(),
        source
    );
}

#[test]
fn explicit_vector_search_without_a_query_keeps_its_existing_local_mode() {
    let (_root, _provider, mut engine, id) = fixture();
    let snapshot = engine.store.snapshot(&id).unwrap();
    let graph = engine.store.graph(&id).unwrap();
    let entity = graph
        .entities
        .iter()
        .find(|entity| entity.name == "old_target")
        .unwrap();
    yoke::knowledge::import_vectors(&engine.store, &snapshot, &graph,
        json!({"snapshotId":id,"model":"fixture-mcp-vector","items":[{"entityId":entity.id,"contentHash":snapshot.files[0].hash,"vector":[1.0,0.0]}]}),
        &Deadline::new(20_000)).unwrap();
    let result = call(
        &mut engine,
        json!({"snapshotId":id,"kind":"semantic","model":"fixture-mcp-vector","vector":[1.0,0.0]}),
    );
    assert_eq!(result["status"], "ok", "{result}");
    assert_eq!(result["data"]["items"][0]["entity"]["id"], entity.id);
    assert_eq!(result["metrics"]["localOnly"], true);
    assert_eq!(result["metrics"]["languageServerAttempts"], 0);
    assert_eq!(result["metrics"]["externalUsage"], "none");
}
