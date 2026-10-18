use serde_json::json;
use std::{collections::BTreeMap, path::PathBuf};
use yoke::server_catalog::{self, HostRegistration};

fn registration() -> HostRegistration {
    HostRegistration {
        backend_id: "rust".into(),
        command: std::env::current_exe().unwrap(),
        tcp_endpoint: None,
        args: vec!["--stdio".into()],
        trust: true,
        allow_source_execution: false,
        passive_profile: None,
        initialization_options: json!({"cargo":{"buildScripts":{"enable":false}}}),
        settings: json!({}),
        workspace_folders: vec![".".into()],
        language_ids: BTreeMap::new(),
        open_languages: vec!["rust".into()],
    }
}
#[test]
fn host_registration_resolves_language_metadata_without_starting_any_process() {
    let configured = registration().configured().unwrap();
    assert_eq!(configured.server.language, "rust");
    assert_eq!(configured.server.args, ["--stdio"]);
    assert_eq!(configured.configuration.open_languages, ["rust"]);
    assert_eq!(
        server_catalog::get("typescript").unwrap().extensions,
        [
            "ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs", "mtsx", "ctsx", "mjsx", "cjsx"
        ]
    );
    assert!(
        server_catalog::get("svelte")
            .unwrap()
            .companions
            .contains(&"typescript")
    );
    assert!(server_catalog::get("imaginary-language-server").is_none());
}
#[test]
fn registration_never_derives_execution_authority_from_a_backend_name() {
    let mut profile = registration();
    profile.trust = false;
    assert!(profile.configured().is_err());
    profile.trust = true;
    profile.command = PathBuf::from("project-controlled.exe");
    assert!(profile.configured().is_err());
    profile = registration();
    profile.workspace_folders = vec!["../outside".into()];
    assert!(profile.configured().is_err());
    profile = registration();
    profile.args = vec!["x".repeat(200_000)];
    assert!(profile.configured().is_err());
}
