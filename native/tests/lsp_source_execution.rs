use serde_json::json;
use std::{fs, path::PathBuf};
use yoke::{
    Engine,
    capture::Deadline,
    lsp::{self, ConfiguredServer, HostConfiguration, SemanticRequest, Server},
};
#[test]
fn passive_rust_omits_selectors_only_from_provider_copy_and_reports_original_hashes() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join(".cargo")).unwrap();
    let selector = "[build]\nrustc = \"unapproved-source-command.exe\"\n";
    fs::write(root.path().join(".cargo/config.toml"), selector).unwrap();
    fs::write(
        root.path().join("rust-toolchain.toml"),
        "[toolchain]\nchannel = \"unapproved-source-toolchain\"\n",
    )
    .unwrap();
    fs::write(root.path().join("main.rs"), "fn actual() {}\n").unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let deadline = Deadline::new(20000);
    let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
    let server = ConfiguredServer {
        server: Server {
            command: "node".into(),
            args: vec![
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/fixtures/passive-rust-lsp.mjs")
                    .display()
                    .to_string(),
            ],
            language: "rust".into(),
            trust: true,
        },
        configuration: HostConfiguration {
            passive_profile: Some(lsp::PassiveProfile::ContractFixture),
            ..Default::default()
        },
    };
    let value = lsp::run_at_configured(
        &mut engine,
        &server,
        &snapshot,
        &graph,
        SemanticRequest {
            path: "main.rs",
            offset: 0,
            operation: "capabilities",
            new_name: None,
        },
        &json!({}),
        &deadline,
    )
    .unwrap();
    assert_eq!(
        value["providerSourceOmissions"].as_array().unwrap().len(),
        2,
        "{value}"
    );
    assert!(
        value["providerSourceOmissions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["path"] == ".cargo/config.toml"
                && item["contentHash"] == yoke::model::hash(selector))
    );
    assert_eq!(
        fs::read_to_string(root.path().join(".cargo/config.toml")).unwrap(),
        selector
    );
    assert_eq!(snapshot.files.len(), 3);
}

#[test]
fn passive_host_redirect_settings_are_refused_before_a_provider_is_started() {
    let root = tempfile::tempdir().unwrap();
    let log = tempfile::NamedTempFile::new().unwrap();
    fs::write(root.path().join("main.rs"), "fn actual() {}\n").unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let deadline = Deadline::new(20000);
    let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
    let server = ConfiguredServer {
        server: Server {
            command: "node".into(),
            args: vec![
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/fixtures/lifecycle-lsp.mjs")
                    .display()
                    .to_string(),
                log.path().display().to_string(),
                "healthy".into(),
            ],
            language: "rust".into(),
            trust: true,
        },
        configuration: HostConfiguration {
            passive_profile: Some(lsp::PassiveProfile::ContractFixture),
            initialization_options: json!({"linkedProjects":["C:/original/source/Cargo.toml"]}),
            ..Default::default()
        },
    };
    assert!(
        lsp::run_at_configured(
            &mut engine,
            &server,
            &snapshot,
            &graph,
            SemanticRequest {
                path: "main.rs",
                offset: 0,
                operation: "capabilities",
                new_name: None
            },
            &json!({}),
            &deadline
        )
        .is_err(),
        "original source redirect silently accepted"
    );
    assert!(
        fs::read_to_string(log.path()).unwrap().is_empty(),
        "provider started before source redirect admission"
    );
}

#[test]
fn unknown_passive_backend_contract_is_unavailable_before_process_start() {
    let root = tempfile::tempdir().unwrap();
    let log = tempfile::NamedTempFile::new().unwrap();
    fs::write(root.path().join("main.rs"), "fn actual() {}\n").unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let deadline = Deadline::new(20000);
    let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
    let server = ConfiguredServer {
        server: Server {
            command: "node".into(),
            args: vec![
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/fixtures/lifecycle-lsp.mjs")
                    .display()
                    .to_string(),
                log.path().display().to_string(),
                "healthy".into(),
            ],
            language: "rust".into(),
            trust: true,
        },
        configuration: HostConfiguration::default(),
    };
    assert!(
        lsp::run_at_configured(
            &mut engine,
            &server,
            &snapshot,
            &graph,
            SemanticRequest {
                path: "main.rs",
                offset: 0,
                operation: "capabilities",
                new_name: None
            },
            &json!({}),
            &deadline
        )
        .is_err(),
        "backend launch trust implicitly certified source passivity"
    );
    assert!(fs::read_to_string(log.path()).unwrap().is_empty());
}
#[test]
fn passive_rust_manifest_and_host_tool_redirects_are_refused_before_provider_start() {
    for (manifest, initialization) in [
        (
            "[package]\nname='redirect'\nversion='0.1.0'\nworkspace='C:/uncaptured-workspace'\n",
            json!({}),
        ),
        (
            "[workspace]\nmembers=['../uncaptured-workspace']\n",
            json!({}),
        ),
        (
            "[package]\nname='redirect'\nversion='0.1.0'\n[dependencies]\nother={path='../uncaptured-workspace'}\n",
            json!({}),
        ),
        (
            "[package]\nname='redirect'\nversion='0.1.0'\n",
            json!({"cargo":{"extraArgs":["--config","build.rustc='unapproved-source-command.exe'"]}}),
        ),
    ] {
        let root = tempfile::tempdir().unwrap();
        let log = tempfile::NamedTempFile::new().unwrap();
        fs::write(root.path().join("Cargo.toml"), manifest).unwrap();
        fs::write(root.path().join("main.rs"), "fn actual() {}\n").unwrap();
        let mut engine = Engine::open(root.path()).unwrap();
        let deadline = Deadline::new(20000);
        let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
        let server = ConfiguredServer {
            server: Server {
                command: "node".into(),
                args: vec![
                    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                        .join("tests/fixtures/lifecycle-lsp.mjs")
                        .display()
                        .to_string(),
                    log.path().display().to_string(),
                    "healthy".into(),
                ],
                language: "rust".into(),
                trust: true,
            },
            configuration: HostConfiguration {
                passive_profile: Some(lsp::PassiveProfile::ContractFixture),
                initialization_options: initialization,
                ..Default::default()
            },
        };
        let result = lsp::run_at_configured(
            &mut engine,
            &server,
            &snapshot,
            &graph,
            SemanticRequest {
                path: "main.rs",
                offset: 0,
                operation: "capabilities",
                new_name: None,
            },
            &json!({}),
            &deadline,
        );
        assert!(
            result.is_err(),
            "Unproven passive source/tool redirect accepted: {manifest}: {result:?}"
        );
        assert!(
            fs::read_to_string(log.path()).unwrap().is_empty(),
            "Provider started before redirect admission"
        );
    }
}
#[test]
fn passive_rust_keeps_proven_nested_workspace_and_dependency_paths_available() {
    let root = tempfile::tempdir().unwrap();
    let log = tempfile::NamedTempFile::new().unwrap();
    fs::create_dir_all(root.path().join("crates/helper/src")).unwrap();
    fs::write(
        root.path().join("Cargo.toml"),
        "[workspace]\nmembers=['crates/*']\n",
    )
    .unwrap();
    fs::write(root.path().join("crates/helper/Cargo.toml"),"[package]\nname='helper'\nversion='0.1.0'\nworkspace='../..'\n[lib]\npath='src/lib.rs'\n[package.metadata]\npath='C:/opaque-data-not-a-cargo-source-selector'\n").unwrap();
    fs::write(
        root.path().join("crates/helper/src/lib.rs"),
        "pub fn actual() {}\n",
    )
    .unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let deadline = Deadline::new(20000);
    let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
    let server = ConfiguredServer {
        server: Server {
            command: "node".into(),
            args: vec![
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/fixtures/lifecycle-lsp.mjs")
                    .display()
                    .to_string(),
                log.path().display().to_string(),
                "healthy".into(),
            ],
            language: "rust".into(),
            trust: true,
        },
        configuration: HostConfiguration {
            passive_profile: Some(lsp::PassiveProfile::ContractFixture),
            ..Default::default()
        },
    };
    let value = lsp::run_at_configured(
        &mut engine,
        &server,
        &snapshot,
        &graph,
        SemanticRequest {
            path: "crates/helper/src/lib.rs",
            offset: 0,
            operation: "capabilities",
            new_name: None,
        },
        &json!({}),
        &deadline,
    )
    .unwrap();
    assert_eq!(value["sourceExecutionAuthorized"], false);
    assert_eq!(value["providerSourceOmissions"], json!([]));
    assert!(
        fs::read_to_string(log.path())
            .unwrap()
            .contains("initialize")
    );
}

#[test]
#[ignore = "requires real installed rust-analyzer and an independently built harmless rustc probe"]
fn actual_rust_source_selected_rustc_requires_explicit_host_execution_authority() {
    let probe =
        std::env::var("YOKE_RUSTC_PROBE").expect("Set independently compiled probe executable");
    let marker = std::env::var("YOKE_RUSTC_PROBE_LOG").expect("Set independent probe marker log");
    assert!(PathBuf::from(&probe).is_absolute() && PathBuf::from(&marker).is_absolute());
    fs::write(&marker, "").unwrap();
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join(".cargo")).unwrap();
    fs::write(root.path().join("main.rs"), "fn main() {}\n").unwrap();
    fs::write(root.path().join("Cargo.toml"),"[package]\nname=\"source_authority_gate\"\nversion=\"0.1.0\"\nedition=\"2021\"\n[[bin]]\nname=\"source_authority_gate\"\npath=\"main.rs\"\n").unwrap();
    let selector = format!(
        "[build]\nrustc = {}\n",
        serde_json::to_string(&probe.replace('\\', "/")).unwrap()
    );
    fs::write(root.path().join(".cargo/config.toml"), &selector).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let deadline = Deadline::new(120000);
    let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
    let mut server = ConfiguredServer {
        server: Server {
            command: "rust-analyzer".into(),
            args: vec![],
            language: "rust".into(),
            trust: true,
        },
        configuration: HostConfiguration {
            passive_profile: Some(lsp::PassiveProfile::RustAnalyzer),
            ..Default::default()
        },
    };
    let request = || SemanticRequest {
        path: "main.rs",
        offset: 0,
        operation: "capabilities",
        new_name: None,
    };
    let passive = lsp::run_at_configured(
        &mut engine,
        &server,
        &snapshot,
        &graph,
        request(),
        &json!({}),
        &deadline,
    )
    .unwrap();
    assert!(
        fs::read_to_string(&marker).unwrap().is_empty(),
        "source-configured compiler executed without host authority"
    );
    assert_eq!(passive["sourceExecutionAuthorized"], false);
    assert_eq!(
        passive["providerSourceOmissions"][0]["contentHash"],
        yoke::model::hash(&selector)
    );
    server.configuration.allow_source_execution = true;
    let authorized = lsp::run_at_configured(
        &mut engine,
        &server,
        &snapshot,
        &graph,
        request(),
        &json!({}),
        &deadline,
    )
    .unwrap();
    assert!(
        !fs::read_to_string(&marker).unwrap().is_empty(),
        "explicit authorized original source configuration was not used"
    );
    assert_eq!(authorized["sourceExecutionAuthorized"], true);
    assert_eq!(authorized["providerSourceOmissions"], json!([]));
    assert_ne!(
        authorized["serverProfileHash"],
        passive["serverProfileHash"]
    );
    assert_eq!(
        fs::read_to_string(root.path().join(".cargo/config.toml")).unwrap(),
        selector
    );
    assert!(lsp::validate_query_options(&json!({"allowSourceExecution":true})).is_err());
}

#[test]
#[ignore = "requires YOKE_PYTHON_LSP identifying the actual installed Jedi language-server"]
fn actual_jedi_source_auto_import_module_does_not_execute_without_host_authority() {
    let command = std::env::var("YOKE_PYTHON_LSP").expect("Set actual Jedi language-server path");
    let root = tempfile::tempdir().unwrap();
    let marker = tempfile::NamedTempFile::new().unwrap();
    let source = "import gi\nvalue = gi.VALUE\n";
    fs::write(root.path().join("main.py"), source).unwrap();
    fs::write(
        root.path().join("gi.py"),
        format!(
            "from pathlib import Path\nPath({}).write_text('executed')\nVALUE = 42\n",
            serde_json::to_string(&marker.path().display().to_string()).unwrap()
        ),
    )
    .unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let deadline = Deadline::new(120000);
    let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
    let server = ConfiguredServer {
        server: Server {
            command,
            args: vec![],
            language: "python".into(),
            trust: true,
        },
        configuration: HostConfiguration {
            passive_profile: Some(lsp::PassiveProfile::JediLanguageServer),
            ..Default::default()
        },
    };
    lsp::run_at_configured(
        &mut engine,
        &server,
        &snapshot,
        &graph,
        SemanticRequest {
            path: "main.py",
            offset: source.find("VALUE").unwrap(),
            operation: "typeDefinition",
            new_name: None,
        },
        &json!({}),
        &deadline,
    )
    .unwrap();
    assert!(
        fs::read_to_string(marker.path()).unwrap().is_empty(),
        "Jedi auto-import executed captured source without host permission"
    );
}
