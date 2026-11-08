use serde_json::json;
use std::fs;
use yoke::{
    Engine, agent_repl,
    capture::Deadline,
    project_session::{HostProfile, Session},
};
fn documentation(source: &str) -> serde_json::Value {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("main.py"), source).unwrap();
    let deadline = Deadline::new(20000);
    let mut session = Session::open(
        Engine::open(root.path()).unwrap(),
        HostProfile::default(),
        Some("documentation-options"),
        &deadline,
    )
    .unwrap();
    agent_repl::invoke_facade(
        &mut session,
        "jb.get_symbols_overview",
        json!({"relative_path":"main.py","include_file_documentation":true}),
        None,
        &deadline,
    )
    .unwrap()["fileDocumentation"]
        .clone()
}
#[test]
fn executable_fstrings_and_byte_literals_are_not_python_module_docstrings() {
    for source in [
        "f\"\"\"computed {execute()}\"\"\"\ndef actual(): pass\n",
        "b\"\"\"binary data\"\"\"\ndef actual(): pass\n",
    ] {
        assert!(
            documentation(source).is_null(),
            "Non-docstring expression mislabeled as file documentation: {source}"
        );
    }
}
#[test]
fn adjacent_literal_strings_form_one_exact_python_module_docstring_span() {
    let source = "\"first \" \"second\"\ndef actual(): pass\n";
    let value = documentation(source);
    assert_eq!(value["text"], "\"first \" \"second\"");
    assert_eq!(value["contentHash"], yoke::model::hash(source));
    assert_eq!(value["startByte"], 0);
    assert_eq!(value["endByte"], 17);
}
