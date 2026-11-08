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
        resource_settings: None,
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

#[test]
fn current_public_backends_preserve_explicit_host_configuration_and_file_matching() {
    for (id, languages, suffixes) in [
        ("vbnet", &["vbnet", "csharp"][..], &["vb", "cs"][..]),
        (
            "typescript_tsgo",
            &["typescript", "javascript", "tsx", "jsx"][..],
            &[
                "ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs", "mtsx", "ctsx", "mjsx",
                "cjsx",
            ][..],
        ),
        ("graphql", &["graphql"][..], &["graphql", "gql"][..]),
    ] {
        let entry = server_catalog::get(id).expect("Current public backend missing");
        assert_eq!(entry.languages, languages);
        assert_eq!(entry.extensions, suffixes);
        for suffix in suffixes {
            let name = format!("owned-\u{1f4a1}/source.{}", suffix.to_ascii_uppercase());
            assert!(entry.supports_path(std::path::Path::new(&name)));
        }
        assert!(!entry.supports_path(std::path::Path::new("source.rs")));
        let mut host = registration();
        host.backend_id = id.into();
        host.open_languages.clear();
        host.language_ids.insert("vbnet".into(), "vb".into());
        let configured = host.configured().unwrap();
        assert_eq!(configured.server.language, languages[0]);
        assert_eq!(configured.configuration.open_languages, languages);
        assert_eq!(configured.configuration.language_ids["vbnet"], "vb");
        assert_eq!(configured.server.args, host.args);
        assert!(!configured.configuration.allow_source_execution);
        assert_eq!(configured.configuration.passive_profile, None);
    }
    assert_eq!(server_catalog::entries().len(), 80);
    assert_eq!(yoke::capture::language("schema.graphql"), "text");
    assert_eq!(yoke::capture::language("fragment.gql"), "text");
    let mut capture = yoke::capture::CaptureOptions::default();
    capture
        .language_overrides
        .insert("gql".into(), "graphql".into());
    assert_eq!(
        capture.validated().unwrap().language_for("fragment.gql"),
        "graphql"
    );
}

#[test]
fn new_backend_names_do_not_bypass_trust_command_path_or_capacity_gates() {
    for id in ["vbnet", "typescript_tsgo", "graphql"] {
        let mut host = registration();
        host.backend_id = id.into();
        host.trust = false;
        assert!(host.configured().is_err());
        host.trust = true;
        host.command = PathBuf::from("workspace-controlled.exe");
        assert!(host.configured().is_err());
        host.command = std::env::current_exe().unwrap();
        host.workspace_folders = vec!["../outside".into()];
        assert!(host.configured().is_err());
        host.workspace_folders.clear();
        host.args = vec!["x".repeat(200_000)];
        assert!(host.configured().is_err());
        assert!(server_catalog::get(&id.to_ascii_uppercase()).is_none());
    }
}
