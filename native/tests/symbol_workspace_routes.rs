//! Workspace selection must reach direct LSP and staged refactoring entrypoints.
use serde_json::{Value, json};
use std::{fs, path::Path};
use yoke::{
    Engine,
    capture::Deadline,
    lsp,
    project_session::{HostProfile, Session},
};

fn d() -> Deadline {
    Deadline::new(30000)
}
fn fixture(parent: &Path, own_move: bool) -> (Session, std::path::PathBuf) {
    let root = parent.join("source");
    fs::create_dir(&root).unwrap();
    for dir in ["src", "other"] {
        fs::create_dir(root.join(dir)).unwrap();
    }
    fs::write(
        root.join("src/source.rs"),
        "// owned\npub fn helper() -> u8 { 7 }\n",
    )
    .unwrap();
    fs::write(
        root.join("src/destination.rs"),
        "pub mod bucket {\npub fn sentinel() {}\n}\n",
    )
    .unwrap();
    fs::write(root.join("other/main.rs"), "fn other() {}\n").unwrap();
    fs::write(
        root.join("project-config.txt"),
        "owned root configuration\n",
    )
    .unwrap();
    let log = parent.join("protocol.log");
    let mut e = Engine::open(&root).unwrap();
    e.allow_writes = true;
    let name = if own_move {
        "own-refactor-intent-lsp.mjs"
    } else {
        "symbol-workspace-lsp.mjs"
    };
    let mut args = vec![
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name)
            .display()
            .to_string(),
        log.display().to_string(),
    ];
    if own_move {
        args.push("valid".into());
    }
    e.register_configured_server(lsp::ConfiguredServer {
        server: lsp::Server {
            command: "node".into(),
            args,
            language: "rust".into(),
            trust: true,
        },
        configuration: lsp::HostConfiguration {
            passive_profile: Some(lsp::PassiveProfile::ContractFixture),
            refactor_intent_profile: own_move.then_some(lsp::RefactorIntentProfile::OwnV1),
            ..Default::default()
        },
    })
    .unwrap();
    let p: HostProfile =
        serde_json::from_value(json!({"symbolWorkspaceFolders":{"global":["src"]}})).unwrap();
    (Session::open(e, p, Some("routes"), &d()).unwrap(), log)
}
fn logged(log: &Path) -> Vec<Value> {
    fs::read_to_string(log)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}
#[test]
fn direct_configured_lsp_entrypoint_receives_effective_workspace_roots_and_profile() {
    let r = tempfile::tempdir().unwrap();
    let (mut s, log) = fixture(r.path(), false);
    let configured = lsp::ConfiguredServer {
        server: s.engine.servers["rust"].clone(),
        configuration: s.engine.server_configurations["rust"].clone(),
    };
    let (snapshot, graph, _, _) = s.engine.index(&d()).unwrap();
    let result = lsp::run_at_configured(
        &mut s.engine,
        &configured,
        &snapshot,
        &graph,
        lsp::SemanticRequest {
            path: "src/source.rs",
            offset: 16,
            operation: "hover",
            new_name: None,
        },
        &json!({}),
        &d(),
    );
    assert!(result.is_ok(), "{result:?}");
    let row = logged(&log).pop().unwrap();
    assert_eq!(row["folders"], json!(["src"]));
    assert_eq!(row["callbackVerified"], true);
}
#[test]
fn refactor_provider_registry_uses_selected_roots_through_its_direct_transport() {
    let r = tempfile::tempdir().unwrap();
    let (mut s, log) = fixture(r.path(), false);
    let result =
        yoke::semantic_refactor::invoke(&mut s.engine, "listInspections", &json!({}), &d());
    assert!(result.is_ok(), "{result:?}");
    let rows = logged(&log);
    assert!(!rows.is_empty());
    assert!(
        rows.iter()
            .all(|v| v["folders"] == json!(["src"]) && v["callbackVerified"] == true)
    );
}
#[test]
fn moving_a_selected_workspace_root_refuses_before_provider_and_plan_publication() {
    let r = tempfile::tempdir().unwrap();
    let (mut s, log) = fixture(r.path(), false);
    let result = yoke::semantic_refactor::invoke(
        &mut s.engine,
        "move",
        &json!({"relativePath":"src","targetRelativePath":"moved"}),
        &d(),
    );
    let error = result.unwrap_err().to_string();
    assert!(
        error.contains("relocate a host-registered workspace root"),
        "{error}"
    );
    assert!(!log.exists());
    assert!(s.engine.root.join("src/source.rs").exists());
    assert!(!s.engine.root.join("moved").exists());
    let plans: usize = s
        .engine
        .store
        .db
        .query_row("SELECT COUNT(*) FROM records WHERE kind='plan'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(plans, 0);
}
#[test]
fn owned_symbol_move_intent_receives_the_same_effective_workspace_selection() {
    let r = tempfile::tempdir().unwrap();
    let (mut s, log) = fixture(r.path(), true);
    let value = yoke::agent_repl::invoke_facade(&mut s,"jb.move",json!({"relative_path":"src/source.rs","name_path":"helper","target_relative_path":"src/destination.rs"}),None,&d()).unwrap();
    assert!(value["plan"].is_object());
    let rows = logged(&log);
    let initialize = rows.iter().find(|v| v["method"] == "initialize").unwrap();
    let folders = initialize["params"]["workspaceFolders"].as_array().unwrap();
    assert_eq!(folders.len(), 1);
    assert!(
        folders[0]["uri"].as_str().unwrap().ends_with("/src"),
        "{initialize}"
    );
    assert_eq!(
        initialize["params"]["capabilities"]["workspace"]["workspaceFolders"],
        true
    );
    assert_eq!(
        fs::read_to_string(s.engine.root.join("src/source.rs")).unwrap(),
        "// owned\npub fn helper() -> u8 { 7 }\n"
    );
}
