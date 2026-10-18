use serde_json::json;
use std::{fs, path::Path};
use yoke::{
    Engine,
    capture::{CaptureOptions, Deadline},
    lsp::Server,
};

#[test]
fn captured_readonly_dependency_uris_are_readable_but_cannot_be_workspace_edit_targets() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("target")).unwrap();
    let source = "// 🌍\nfn helper() {}\nfn main() { helper(); }\n";
    fs::write(root.path().join("target/generated.rs"), source).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    engine.capture_options =
        serde_json::from_value::<CaptureOptions>(json!({"readOnlyDependencyDirs":["target"]}))
            .unwrap();
    let server = Server {
        command: "node".into(),
        args: vec![
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/advanced-lsp.mjs")
                .display()
                .to_string(),
        ],
        language: "rust".into(),
        trust: true,
    };
    let result = lsp::run(
        &mut engine,
        &server,
        "target/generated.rs",
        44,
        "typeDefinition",
        None,
        &Deadline::new(20000),
    )
    .unwrap();
    assert_eq!(result["locations"][0]["path"], "target/generated.rs");
    assert_eq!(
        result["locations"][0]["contentHash"],
        yoke::model::hash(source)
    );
    let edit = lsp::run(
        &mut engine,
        &server,
        "target/generated.rs",
        44,
        "codeActions",
        None,
        &Deadline::new(20000),
    );
    assert!(
        edit.is_err(),
        "readonly dependency edits must be refused: {edit:?}"
    );
    let count: i64 = engine
        .store
        .db
        .query_row(
            "SELECT count(*) FROM records WHERE kind='plan'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
    assert_eq!(
        fs::read_to_string(root.path().join("target/generated.rs")).unwrap(),
        source
    );
}

#[path = "support/lsp_fixture.rs"]
mod lsp;
