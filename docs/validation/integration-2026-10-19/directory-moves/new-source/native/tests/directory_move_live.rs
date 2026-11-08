//! Explicit actual-provider gate. No protocol fixture or compiler fallback.
use serde_json::json;
use std::{fs, path::Path};
use yoke::{Engine, capture::Deadline, changes, lsp};

#[test]
#[ignore = "requires YOKE_RUST_ANALYZER and YOKE_RUSTC selecting actual installed executables"]
fn actual_rust_analyzer_folder_move_compiles_applies_and_recovers() {
    let analyzer =
        std::env::var("YOKE_RUST_ANALYZER").expect("Select the actual rust-analyzer executable");
    let compiler = std::env::var("YOKE_RUSTC").expect("Select the actual rustc executable");
    for command in [&analyzer, &compiler] {
        assert!(Path::new(command).is_absolute() && Path::new(command).is_file());
    }
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("src/tree/empty/nested")).unwrap();
    let library = "// owned actual-provider fixture: café 🦀\r\npub mod tree;\r\npub fn value() -> i32 { tree::answer() }\r\n";
    let module = "pub fn answer() -> i32 { 42 }\r\n";
    fs::write(root.path().join("Cargo.toml"), "[package]\nname = \"yoke_directory_live_fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n").unwrap();
    fs::write(root.path().join("src/lib.rs"), library).unwrap();
    fs::write(root.path().join("src/tree/mod.rs"), module).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    engine
        .register_configured_server(lsp::ConfiguredServer {
            server: lsp::Server {
                command: analyzer,
                args: vec![],
                language: "rust".into(),
                trust: true,
            },
            configuration: lsp::HostConfiguration {
                passive_profile: Some(lsp::PassiveProfile::RustAnalyzer),
                ..Default::default()
            },
        })
        .unwrap();
    let result = yoke::semantic_refactor::invoke(
        &mut engine,
        "move",
        &json!({"relativePath":"src/tree","targetRelativePath":"src/renamed"}),
        &Deadline::new(90_000),
    );
    let value =
        result.unwrap_or_else(|error| panic!("Actual rust-analyzer folder gate: {error:#}"));
    println!("actual-provider-plan={value}");
    let id = value["plan"]["id"].as_str().unwrap();
    assert_eq!(value["complete"], false);
    assert!(
        value["plan"]["files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|file| file["path"] == "src/lib.rs"),
        "Actual module reference repair was absent: {value}"
    );
    assert_eq!(
        fs::read(root.path().join("src/lib.rs")).unwrap(),
        library.as_bytes()
    );
    assert!(root.path().join("src/tree/empty/nested").is_dir());
    let verification = yoke::verify::execute(
        &engine,
        id,
        &compiler,
        &[
            "--crate-type=lib".into(),
            "--edition=2021".into(),
            "src/lib.rs".into(),
            "-o".into(),
            "owned-directory-live.rlib".into(),
        ],
        true,
        &Deadline::new(90_000),
    )
    .unwrap();
    println!("actual-rustc-verification={verification}");
    assert_eq!(verification["status"], "passed", "{verification}");
    assert_eq!(verification["sourceIntegrity"], true);
    changes::approve(&engine.store, id, false).unwrap();
    let receipt = changes::apply(
        &engine.store,
        root.path(),
        &json!({"planId":id,"idempotencyKey":"actual-rust-analyzer-folder","mode":"workspace"}),
        true,
        &Deadline::new(30_000),
    )
    .unwrap();
    assert!(!root.path().join("src/tree").exists());
    assert!(root.path().join("src/renamed/empty/nested").is_dir());
    assert_eq!(
        fs::read(root.path().join("src/renamed/mod.rs")).unwrap(),
        module.as_bytes()
    );
    assert_ne!(
        fs::read(root.path().join("src/lib.rs")).unwrap(),
        library.as_bytes()
    );
    changes::recover(&engine.store, root.path(), &receipt.id, true).unwrap();
    assert_eq!(
        fs::read(root.path().join("src/lib.rs")).unwrap(),
        library.as_bytes()
    );
    assert_eq!(
        fs::read(root.path().join("src/tree/mod.rs")).unwrap(),
        module.as_bytes()
    );
    assert!(root.path().join("src/tree/empty/nested").is_dir());
    assert!(!root.path().join("src/renamed").exists());
}
