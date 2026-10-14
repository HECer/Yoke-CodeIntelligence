// Explicit live gates: unavailable tools fail, and these tests never use protocol fixtures.
use serde_json::{Value, json};
use std::{fs, path::Path};
use tempfile::TempDir;
use yoke::{
    Engine,
    capture::Deadline,
    lsp::{self, Server},
};
#[test]
#[ignore = "requires YOKE_PYTHON_LSP pointing to an actual installed jedi-language-server executable"]
fn actual_python_server_cross_file_unicode_historical_navigation_and_rename() {
    let command = std::env::var("YOKE_PYTHON_LSP")
        .expect("Set YOKE_PYTHON_LSP to the real server executable");
    assert!(Path::new(&command).is_absolute() && Path::new(&command).is_file());
    let dir = TempDir::new().unwrap();
    let library = "def café(number: int) -> int:\n    return number + 1\n";
    let text = "# 🌍 historical Python\nfrom lib import café\nresult = café(4)\n";
    fs::write(dir.path().join("lib.py"), library).unwrap();
    fs::write(dir.path().join("main.py"), text).unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let (snapshot, graph, _, _) = engine.index(&Deadline::new(10_000)).unwrap();
    let current = "def disk_changed(): return 'live'\n";
    fs::write(dir.path().join("lib.py"), current).unwrap();
    let server = Server {
        command,
        args: vec![],
        language: "python".into(),
        trust: true,
    };
    for operation in ["definition", "references", "hover"] {
        let report = lsp::run_at(
            &mut engine,
            &server,
            &snapshot,
            &graph,
            lsp::SemanticRequest {
                path: "main.py",
                offset: text.rfind("café").unwrap(),
                operation,
                new_name: None,
            },
            &Deadline::new(60_000),
        )
        .unwrap();
        assert_eq!(report["snapshotId"], snapshot.id, "{report}");
        if operation == "definition" {
            assert!(
                report["locations"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|p| p["path"] == "lib.py" && p["text"] == "café"),
                "{report}"
            );
        }
        if operation == "references" {
            assert!(
                report["locations"].as_array().unwrap().len() >= 3,
                "{report}"
            );
        }
        if operation == "hover" {
            assert!(
                report["response"]["contents"]["value"]
                    .as_str()
                    .is_some_and(|t| t.contains("café(number: int) -> int")),
                "{report}"
            );
        }
    }
    let rename = lsp::run_at(
        &mut engine,
        &server,
        &snapshot,
        &graph,
        lsp::SemanticRequest {
            path: "lib.py",
            offset: 4,
            operation: "rename",
            new_name: Some("renamed"),
        },
        &Deadline::new(60_000),
    )
    .unwrap();
    assert_eq!(
        rename["plan"]["files"].as_array().unwrap().len(),
        2,
        "{rename}"
    );
    assert!(
        rename["plan"]["diff"]
            .as_str()
            .unwrap()
            .contains("from lib import renamed")
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("lib.py")).unwrap(),
        current
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("main.py")).unwrap(),
        text
    );
}

#[test]
#[ignore = "requires an actual installed rust-analyzer"]
fn actual_rust_analyzer_definition_references_and_rename() {
    let dir = TempDir::new().unwrap();
    fs::create_dir(dir.path().join("src")).unwrap();
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"yoke_live_fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    let text = "// 🌍\nfn helper() -> u32 { 7 }\nfn main() { let _ = helper(); }\n";
    fs::write(dir.path().join("src/main.rs"), text).unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let offset = text.rfind("helper").unwrap();
    let server = Server {
        command: "rust-analyzer".into(),
        args: vec![],
        language: "rust".into(),
        trust: true,
    };
    let definition = lsp::run(
        &mut engine,
        &server,
        "src/main.rs",
        offset,
        "definition",
        None,
        &Deadline::new(60000),
    )
    .unwrap();
    assert_eq!(definition["locations"][0]["text"], "helper", "{definition}");
    let references = lsp::run(
        &mut engine,
        &server,
        "src/main.rs",
        offset,
        "references",
        None,
        &Deadline::new(60000),
    )
    .unwrap();
    assert!(
        references["locations"].as_array().unwrap().len() >= 2,
        "{references}"
    );
    let rename = lsp::run(
        &mut engine,
        &server,
        "src/main.rs",
        offset,
        "rename",
        Some("renamed"),
        &Deadline::new(60000),
    )
    .unwrap();
    assert!(
        rename["plan"]["diff"]
            .as_str()
            .unwrap()
            .contains("fn renamed")
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("src/main.rs")).unwrap(),
        text
    );
}

#[test]
#[ignore = "requires an actual installed rust-analyzer"]
fn actual_rust_analyzer_mcp_references_bind_immutable_snapshot() {
    let dir = TempDir::new().unwrap();
    fs::create_dir(dir.path().join("src")).unwrap();
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"yoke_mcp_live_fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    let text =
        "// 🌍\nfn historical_helper() -> u32 { 7 }\nfn main() { let _ = historical_helper(); }\n";
    fs::write(dir.path().join("src/main.rs"), text).unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let (snapshot, _, _, _) = engine.index(&Deadline::new(20_000)).unwrap();
    let hash = snapshot
        .files
        .iter()
        .find(|file| file.path == "src/main.rs")
        .unwrap()
        .hash
        .clone();
    let current = "fn main() { let disk_changed = 42; println!(\"{disk_changed}\"); }\n";
    fs::write(dir.path().join("src/main.rs"), current).unwrap();
    engine
        .register_server(Server {
            command: "rust-analyzer".into(),
            args: vec![],
            language: "rust".into(),
            trust: true,
        })
        .unwrap();
    let input = format!(
        "{}\n{}\n",
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"ci_find","arguments":{"snapshotId":snapshot.id,"languageServer":{"operation":"references","path":"src/main.rs","offset":text.rfind("historical_helper").unwrap()},"timeoutMs":60_000}}})
    );
    let mut output = Vec::new();
    yoke::protocol::serve(&mut engine, std::io::Cursor::new(input), &mut output).unwrap();
    let reply: Value =
        serde_json::from_str(String::from_utf8(output).unwrap().lines().nth(1).unwrap()).unwrap();
    let result: Value =
        serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(result["status"], "ok", "{result}");
    assert_eq!(result["snapshotId"], snapshot.id);
    assert_eq!(result["data"]["contentHash"], hash);
    let locations = result["data"]["locations"].as_array().unwrap();
    assert!(locations.len() >= 2, "{result}");
    for location in locations {
        assert_eq!(location["path"], "src/main.rs");
        assert_eq!(location["contentHash"], hash);
        assert_eq!(location["text"], "historical_helper");
        let start = location["startByte"].as_u64().unwrap() as usize;
        let end = location["endByte"].as_u64().unwrap() as usize;
        assert_eq!(&text[start..end], "historical_helper");
    }
    assert_eq!(result["metrics"]["languageServerAttempts"], 1);
    assert_eq!(result["metrics"]["localOnly"], false);
    assert_eq!(result["metrics"]["externalUsage"], "unknown");
    assert_eq!(
        fs::read_to_string(dir.path().join("src/main.rs")).unwrap(),
        current
    );
}

#[test]
#[ignore = "requires YOKE_TS_LSP pointing to the real typescript-language-server CLI"]
fn actual_typescript_server_cross_file_definition_and_rename() {
    let cli = std::env::var("YOKE_TS_LSP")
        .expect("YOKE_TS_LSP must identify an installed real typescript-language-server CLI");
    assert!(Path::new(&cli).is_file());
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("tsconfig.json"),
        "{\"compilerOptions\":{\"strict\":true},\"include\":[\"*.ts\"]}",
    )
    .unwrap();
    fs::write(
        dir.path().join("lib.ts"),
        "export function helper(): number { return 7; }\n",
    )
    .unwrap();
    let text = "import { helper } from './lib';\nexport const result = helper();\n";
    fs::write(dir.path().join("main.ts"), text).unwrap();
    let offset = text.rfind("helper").unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let server = Server {
        command: "node".into(),
        args: vec![cli, "--stdio".into()],
        language: "typescript".into(),
        trust: true,
    };
    let definition = lsp::run(
        &mut engine,
        &server,
        "main.ts",
        offset,
        "definition",
        None,
        &Deadline::new(60000),
    )
    .unwrap();
    assert_eq!(definition["locations"][0]["path"], "lib.ts", "{definition}");
    assert_eq!(definition["locations"][0]["text"], "helper");
    let rename = lsp::run(
        &mut engine,
        &server,
        "lib.ts",
        16,
        "rename",
        Some("renamed"),
        &Deadline::new(60000),
    )
    .unwrap();
    assert_eq!(
        rename["plan"]["files"].as_array().unwrap().len(),
        2,
        "{rename}"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("main.ts")).unwrap(),
        text
    );
}
