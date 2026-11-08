use serde_json::{Value, json};
use std::{fs, path::Path};
use yoke::{
    Engine, agent_repl,
    capture::Deadline,
    project_session::{HostProfile, Session},
};

fn session(root: &Path) -> Session {
    Session::open(
        Engine::open(root).unwrap(),
        HostProfile::default(),
        Some("pinned-symbol-contracts"),
        &Deadline::new(30000),
    )
    .unwrap()
}
fn invoke(session: &mut Session, method: &str, args: Value) -> anyhow::Result<Value> {
    agent_repl::invoke_facade(session, method, args, None, &Deadline::new(30000))
}
#[test]
fn max_matches_is_a_permitted_match_threshold_and_unlimited_does_not_mean_native_default_thirty() {
    let root = tempfile::tempdir().unwrap();
    for index in 0..45 {
        fs::write(
            root.path().join(format!("source{index}.rs")),
            "fn shared() {}\n",
        )
        .unwrap();
    }
    let mut session = session(root.path());
    let error = invoke(
        &mut session,
        "lsp.find_symbol",
        json!({"name_path_pattern":"shared","max_matches":1}),
    )
    .expect_err("Exceeded match threshold must fail rather than silently truncate");
    assert!(error.to_string().contains("max_matches"), "{error:#}");
    let all = invoke(
        &mut session,
        "lsp.find_symbol",
        json!({"name_path_pattern":"shared","max_matches":-1}),
    )
    .unwrap();
    assert_eq!(all["items"].as_array().unwrap().len(), 45);
    assert_eq!(all["truncated"], false);
    assert!(
        invoke(
            &mut session,
            "lsp.find_symbol",
            json!({"name_path_pattern":"shared","max_matches":-2})
        )
        .is_err()
    );
}
#[test]
fn overview_depth_defaults_are_language_specific_and_explicit_depth_limits_descendants() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("main.py"),"class Top:\n    def member(self):\n        def nested():\n            pass\n        return nested()\n").unwrap();
    fs::write(
        root.path().join("Top.java"),
        "class Top { void member() {} }\n",
    )
    .unwrap();
    let mut session = session(root.path());
    for method in ["lsp.get_symbols_overview", "jb.get_symbols_overview"] {
        let top = invoke(&mut session, method, json!({"relative_path":"main.py"})).unwrap();
        assert_eq!(
            top["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|item| item["name"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["Top"]
        );
        let children = invoke(
            &mut session,
            method,
            json!({"relative_path":"main.py","depth":1}),
        )
        .unwrap();
        assert_eq!(children["items"].as_array().unwrap().len(), 2, "{children}");
        let java = invoke(&mut session, method, json!({"relative_path":"Top.java"})).unwrap();
        assert_eq!(java["items"].as_array().unwrap().len(), 2, "{java}");
    }
}
#[test]
fn full_bodies_suppress_descendant_depth_and_hover_provider_queries() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("main.py"),
        "class Top:\n    def member(self):\n        return 1\n",
    )
    .unwrap();
    let mut session = session(root.path());
    for method in ["lsp.find_symbol", "jb.find_symbol"] {
        let value=invoke(&mut session,method,json!({"name_path_pattern":"Top","relative_path":"main.py","depth":99,"include_body":true,"include_info":true})).unwrap();
        assert_eq!(
            value["items"].as_array().unwrap().len(),
            1,
            "Bodies must suppress descendant expansion: {value}"
        );
        assert!(
            value["items"][0]["body"]
                .as_str()
                .unwrap()
                .contains("def member")
        );
        assert!(value["items"][0].get("info").is_none());
    }
}
#[test]
fn requested_hover_is_attached_to_root_matches_and_never_descendants() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("main.py"),
        "class Top:\n    def member(self):\n        return 1\n",
    )
    .unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    engine
        .register_configured_server(yoke::lsp::ConfiguredServer {
            server: yoke::lsp::Server {
                command: "node".into(),
                args: vec![
                    Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("tests/fixtures/advanced-lsp.mjs")
                        .display()
                        .to_string(),
                ],
                language: "python".into(),
                trust: true,
            },
            configuration: yoke::lsp::HostConfiguration {
                passive_profile: Some(yoke::lsp::PassiveProfile::ContractFixture),
                ..Default::default()
            },
        })
        .unwrap();
    let mut session = Session::open(
        engine,
        HostProfile::default(),
        Some("root-only-hover"),
        &Deadline::new(30000),
    )
    .unwrap();
    let value = invoke(
        &mut session,
        "lsp.find_symbol",
        json!({"name_path_pattern":"Top","relative_path":"main.py","depth":1,"include_info":true}),
    )
    .unwrap();
    let items = value["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert!(
        items
            .iter()
            .find(|item| item["name"] == "Top")
            .unwrap()
            .get("info")
            .is_some()
    );
    assert!(
        items
            .iter()
            .find(|item| item["name"] == "member")
            .unwrap()
            .get("info")
            .is_none(),
        "Child hover contradicts the public facade contract: {value}"
    );
}

#[test]
fn excluded_root_kinds_cannot_seed_descendants_beyond_a_zero_root_match_threshold() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("main.py"),
        "class Top:\n    def member(self):\n        return 1\n",
    )
    .unwrap();
    let mut session = session(root.path());
    let value = invoke(
        &mut session,
        "lsp.find_symbol",
        json!({"name_path_pattern":"Top","depth":1,"include_kinds":[6],"max_matches":0}),
    )
    .unwrap();
    assert_eq!(value["rootMatchCount"], 0);
    assert!(
        value["items"].as_array().unwrap().is_empty(),
        "Excluded root bypassed the permitted-root threshold through its descendants: {value}"
    );
}

#[test]
fn overloaded_symbol_paths_have_stable_zero_based_indices_and_resolve_back_to_the_same_source() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("Top.java"), "class Top { int convert(int value) { return value; } String convert(String value) { return value; } }\n").unwrap();
    let mut session = session(root.path());
    for method in ["lsp.find_symbol", "jb.find_symbol"] {
        let all = invoke(
            &mut session,
            method,
            json!({"name_path_pattern":"Top/convert","relative_path":"Top.java"}),
        )
        .unwrap();
        assert_eq!(
            all["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|item| item["namePath"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["Top/convert[0]", "Top/convert[1]"]
        );
        let second = invoke(&mut session, method, json!({"name_path_pattern":"Top/convert[1]","relative_path":"Top.java","max_matches":1,"include_body":true})).unwrap();
        assert_eq!(second["items"].as_array().unwrap().len(), 1);
        assert_eq!(second["items"][0]["id"], all["items"][1]["id"]);
        assert!(
            second["items"][0]["body"]
                .as_str()
                .unwrap()
                .starts_with("String convert")
        );
    }
    let references = invoke(
        &mut session,
        "lsp.find_referencing_symbols",
        json!({"name_path":"Top/convert[1]","relative_path":"Top.java"}),
    )
    .unwrap();
    assert!(
        references["referencingSymbols"]
            .as_array()
            .unwrap()
            .is_empty(),
        "Indexed path must be accepted by symbol-taking methods: {references}"
    );
}

#[test]
fn substring_matching_expands_only_the_last_name_segment_and_absolute_paths_stay_rooted() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("main.py"),
        "class Container:\n    def computeMethod(self):\n        return 1\n",
    )
    .unwrap();
    let mut session = session(root.path());
    for (method, patterns) in [
        (
            "lsp.find_symbol",
            ["tainer/compute", "Container/compute", "/compute"],
        ),
        (
            "jb.find_symbol",
            ["tainer/*compute*", "Container/*compute*", "/*compute*"],
        ),
    ] {
        let options = |pattern| {
            let mut args = json!({"name_path_pattern":pattern});
            if method == "lsp.find_symbol" {
                args["substring_matching"] = json!(true);
            }
            args
        };
        let wrong = invoke(&mut session, method, options(patterns[0])).unwrap();
        assert!(
            wrong["items"].as_array().unwrap().is_empty(),
            "Prefix components must stay exact: {wrong}"
        );
        let correct = invoke(&mut session, method, options(patterns[1])).unwrap();
        assert_eq!(correct["items"].as_array().unwrap().len(), 1);
        let rooted = invoke(&mut session, method, options(patterns[2])).unwrap();
        assert!(rooted["items"].as_array().unwrap().is_empty());
    }
}

#[test]
fn jb_wildcard_only_queries_require_the_overview_facade() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("main.rs"), "fn main() {}\n").unwrap();
    let mut session = session(root.path());
    for pattern in ["*", "/*"] {
        assert!(
            invoke(
                &mut session,
                "jb.find_symbol",
                json!({"name_path_pattern":pattern})
            )
            .is_err(),
            "Wildcard-only matches must use overview: {pattern}"
        );
    }
}
