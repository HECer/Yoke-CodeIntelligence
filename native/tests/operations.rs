use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    process::{Command, Stdio},
};
use tempfile::TempDir;
use yoke::{Engine, capture::Deadline};
fn request(engine: &mut Engine, tool: &str, args: Value) -> Value {
    let result = engine.invoke(tool, args).unwrap();
    assert_ne!(result["status"], "error", "{result}");
    result
}

#[test]
fn real_grammars_extract_multilingual_symbols_and_diagnostics() {
    let dir = TempDir::new().unwrap();
    let fixtures = [
        ("test.ts", "export function greet() { return 1; }"),
        ("test.js", "function greet() { return 1; }"),
        ("test.py", "def greet():\n    return 1\n"),
        ("test.go", "package demo\nfunc greet() int { return 1 }"),
        ("test.c", "int greet(void) { return 1; }"),
        ("test.cpp", "int greet() { return 1; }"),
        ("Test.java", "class Test { int greet() { return 1; } }"),
        ("Test.cs", "class Test { int greet() { return 1; } }"),
        ("test.rb", "def greet\n  1\nend\n"),
        ("test.php", "<?php function greet() { return 1; }"),
        ("test.rs", "fn greet() -> u32 { 1 }"),
    ];
    for (path, text) in fixtures {
        fs::write(dir.path().join(path), text).unwrap();
    }
    let mut engine = Engine::open(dir.path()).unwrap();
    let (_, graph, _, _) = engine.index(&Deadline::new(120000)).unwrap();
    for (path, _) in fixtures {
        assert!(
            graph
                .entities
                .iter()
                .any(|e| e.path == path && e.name == "greet"),
            "{path}: {:?}",
            graph
                .entities
                .iter()
                .map(|e| (&e.path, &e.name))
                .collect::<Vec<_>>()
        );
    }
    assert!(graph.diagnostics.is_empty(), "{:?}", graph.diagnostics);
}

#[test]
fn additional_real_grammars_extract_symbols_without_running_source() {
    let dir = TempDir::new().unwrap();
    let fixtures = [
        ("test.swift", "func greet() -> Int { return 1 }"),
        ("Test.scala", "object Test { def greet(): Int = 1 }"),
        ("test.lua", "function greet() return 1 end"),
        ("test.sh", "greet() { return 0; }"),
        ("test.ex", "defmodule Test do\n  def greet(), do: 1\nend"),
        (
            "Test.sol",
            "pragma solidity ^0.8.0; contract Test { function greet() public pure returns (uint) { return 1; } }",
        ),
        ("test.ml", "let greet x = x + 1"),
        ("test.zig", "pub fn greet() u32 { return 1; }"),
        ("test.nix", "{ greet = x: x + 1; }"),
        ("test.R", "greet <- function(x) { x + 1 }"),
        ("Test.kt", "fun greet(): Int { return 1 }"),
        ("test.dart", "int greet() { return 1; }"),
        ("test.clj", "(defn greet [x] (+ x 1))"),
    ];
    for (path, text) in fixtures {
        fs::write(dir.path().join(path), text).unwrap();
    }
    let mut engine = Engine::open(dir.path()).unwrap();
    let (_, graph, _, _) = engine.index(&Deadline::new(120000)).unwrap();
    for (path, text) in fixtures {
        if !graph
            .entities
            .iter()
            .any(|e| e.path == path && e.name == "greet")
            && let Some(grammar) = yoke::analysis::grammar(yoke::capture::language(path))
        {
            let mut parser = tree_sitter::Parser::new();
            parser.set_language(&grammar).unwrap();
            eprintln!(
                "{path}: {}",
                parser.parse(text, None).unwrap().root_node().to_sexp()
            );
        }
    }
    for (path, _) in fixtures {
        assert!(
            graph
                .entities
                .iter()
                .any(|e| e.path == path && e.name == "greet"),
            "{path}: {:?}",
            graph
                .entities
                .iter()
                .map(|e| (&e.path, &e.name))
                .collect::<Vec<_>>()
        );
    }
    assert!(graph.diagnostics.is_empty(), "{:?}", graph.diagnostics);
}

#[test]
fn additional_grammar_call_sites_have_exact_evidence_and_labeled_candidates() {
    let dir = TempDir::new().unwrap();
    let fixtures = [
        (
            "test.swift",
            "func greet() -> Int { return 1 }\nfunc use() -> Int { return greet() }",
        ),
        (
            "Test.scala",
            "object Test { def greet(): Int = 1; def use(): Int = greet() }",
        ),
        (
            "test.lua",
            "function greet() return 1 end\nfunction use() return greet() end",
        ),
        ("test.sh", "greet() { return 0; }\nuse() { greet; }"),
        (
            "test.ex",
            "defmodule Test do\n def greet(), do: 1\n def use(), do: greet()\nend",
        ),
        (
            "Test.sol",
            "pragma solidity ^0.8.0; contract Test { function greet() public pure returns (uint) { return 1; } function use() public pure returns (uint) { return greet(); } }",
        ),
        ("test.ml", "let greet x = x + 1\nlet use x = greet x"),
        (
            "test.zig",
            "pub fn greet() u32 { return 1; }\npub fn use() u32 { return greet(); }",
        ),
        ("test.nix", "rec { greet = x: x + 1; use = x: greet x; }"),
        (
            "test.R",
            "greet <- function(x) { x + 1 }\nuse <- function(x) { greet(x) }",
        ),
        (
            "Test.kt",
            "fun greet(): Int { return 1 }\nfun use(): Int { return greet() }",
        ),
        (
            "test.dart",
            "int greet() { return 1; }\nint use() { return greet(); }",
        ),
        (
            "test.clj",
            "(defn greet [x] (+ x 1))\n(defn use [x] (greet x))",
        ),
    ];
    for (path, text) in fixtures {
        fs::write(dir.path().join(path), text).unwrap();
    }
    let mut engine = Engine::open(dir.path()).unwrap();
    let (snapshot, graph, _, _) = engine.index(&Deadline::new(120000)).unwrap();
    let mut missing = Vec::new();
    for (path, text) in fixtures {
        let owner = graph
            .entities
            .iter()
            .find(|entity| entity.path == path && entity.name == "use")
            .unwrap();
        let calls: Vec<_> = graph
            .observations
            .iter()
            .filter(|edge| {
                edge.source == owner.id && edge.relation == "calls" && edge.target_label == "greet"
            })
            .collect();
        if calls.is_empty() {
            let mut parser = tree_sitter::Parser::new();
            parser
                .set_language(&yoke::analysis::grammar(yoke::capture::language(path)).unwrap())
                .unwrap();
            eprintln!(
                "{path}: {}",
                parser.parse(text, None).unwrap().root_node().to_sexp()
            );
            missing.push(path);
            continue;
        }
        for edge in calls {
            assert_ne!(edge.resolution, "exact_binding");
            for id in &edge.evidence_ids {
                let evidence = graph.evidence.iter().find(|e| &e.id == id).unwrap();
                assert_eq!(evidence.path, path);
                assert_eq!(
                    evidence.content_hash,
                    snapshot
                        .files
                        .iter()
                        .find(|file| file.path == path)
                        .unwrap()
                        .hash
                );
                assert!(text[evidence.start_byte..evidence.end_byte].contains("greet"));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "Missing structural calls in {missing:?}"
    );
    assert!(graph.diagnostics.is_empty(), "{:?}", graph.diagnostics);
}

#[test]
fn mcp_stdio_initialization_lists_exactly_eight_tools_and_rejects_host_authority() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("test.rs"), "fn example() {}\n").unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .args(["--root", dir.path().to_str().unwrap(), "serve"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let messages = [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"ci_status","arguments":{"allowWrites":true}}}),
        json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"ci_find","arguments":{"query":"example"}}}),
    ];
    let mut stdin = child.stdin.take().unwrap();
    for message in messages {
        writeln!(stdin, "{message}").unwrap();
    }
    drop(stdin);
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let values: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(values.len(), 4);
    assert_eq!(values[0]["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(values[1]["result"]["tools"].as_array().unwrap().len(), 8);
    assert_eq!(values[2]["result"]["isError"], true);
    let body: Value =
        serde_json::from_str(values[3]["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(body["data"]["items"][0]["name"], "example");
}

#[test]
fn binary_resource_reads_cannot_abort_the_next_mcp_request() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let blob = engine.store.artifact(&[0xff, 0, 0xfe]).unwrap();
    let messages = [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
        json!({"jsonrpc":"2.0","id":2,"method":"resources/read","params":{"uri":format!("ci://artifact/{blob}")}}),
        json!({"jsonrpc":"2.0","id":3,"method":"ping"}),
    ];
    let input = messages
        .iter()
        .map(|value| format!("{value}\n"))
        .collect::<String>();
    let mut output = Vec::new();
    yoke::protocol::serve(&mut engine, std::io::Cursor::new(input), &mut output).unwrap();
    let values = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(values.len(), 3);
    assert!(values[1].get("error").is_some());
    assert_eq!(values[2]["result"], json!({}));
}

#[test]
fn lsp_unicode_position_roundtrip_rejects_split_surrogates() {
    let text = "a🌍b\r\ncafé\n";
    for (byte, line, character) in [(0, 0, 0), (1, 0, 1), (5, 0, 3), (8, 1, 0), (13, 1, 4)] {
        let result = Command::new(env!("CARGO_BIN_EXE_yoke"))
            .args(["position", "--text", text, "--byte", &byte.to_string()])
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let position: Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(position, json!({"line":line,"character":character}));
    }
}

#[test]
fn cross_file_imports_are_recomputed_after_dependency_changes() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("consumer.ts"),
        "import { run } from './lib';\nfunction main() { run(); }\n",
    )
    .unwrap();
    fs::write(dir.path().join("lib.ts"), "export function run() {}\n").unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let (_, first, _, _) = engine.index(&Deadline::new(120000)).unwrap();
    let main = first.entities.iter().find(|e| e.name == "main").unwrap();
    let edge = first
        .observations
        .iter()
        .find(|e| e.source == main.id && e.relation == "calls")
        .unwrap();
    assert!(edge.target.is_some());
    fs::write(dir.path().join("lib.ts"), "export function other() {}\n").unwrap();
    let (_, second, parsed, reused) = engine.index(&Deadline::new(120000)).unwrap();
    assert_eq!((parsed, reused), (1, 1));
    assert!(
        second
            .observations
            .iter()
            .any(|e| e.relation == "calls" && e.target_label == "run" && e.target.is_none())
    );
}

#[test]
fn symbol_insertions_and_overlaps_use_immutable_exact_ranges() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("a.rs"), "fn example() {}\n").unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let found = request(&mut engine, "ci_find", json!({"query":"example"}));
    let entity = &found["data"]["items"][0];
    let plan = request(
        &mut engine,
        "ci_change_prepare",
        json!({"snapshotId":found["snapshotId"],"insertAfter":{"entityId":entity["id"],"text":"\nfn second() {}"}}),
    );
    let derived = request(
        &mut engine,
        "ci_read",
        json!({"snapshotId":plan["data"]["plan"]["resultSnapshotId"],"path":"a.rs"}),
    );
    assert!(
        derived["data"]["text"]
            .as_str()
            .unwrap()
            .contains("fn second")
    );
    let read = request(&mut engine, "ci_read", json!({"path":"a.rs"}));
    let edit = json!({"kind":"replace","path":"a.rs","expectedHash":read["data"]["contentHash"],"startByte":0,"endByte":3,"text":"bad"});
    let denied = engine
        .invoke("ci_change_prepare", json!({"edits":[edit.clone(),edit]}))
        .unwrap();
    assert_eq!(denied["status"], "error");
}
