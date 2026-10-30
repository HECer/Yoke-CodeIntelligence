//! Explicit owned compiler/AST checks. Project modules are never executed.
use serde_json::json;
use std::{fs, path::Path};
use yoke::{Engine, capture::Deadline, changes, query};

fn checked_cycle(path: &str, source: &str, expected: &str, command: &str, args: &[String]) {
    assert!(Path::new(command).is_absolute() && Path::new(command).is_file());
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join(path), source).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let (snapshot, graph, _, _) = engine.index(&Deadline::new(30_000)).unwrap();
    let selected = graph
        .entities
        .iter()
        .find(|e| e.name == "delete_me")
        .unwrap();
    let original = serde_json::to_value(selected).unwrap();
    let plan = changes::prepare(
        &mut engine.store,
        &snapshot,
        &graph,
        &json!({"deleteSymbol":{"entityId":selected.id}}),
        &Deadline::new(30_000),
    )
    .unwrap();
    println!(
        "actual-{path}-attachment-plan={}",
        serde_json::to_value(&plan).unwrap()
    );
    assert_eq!(fs::read(root.path().join(path)).unwrap(), source.as_bytes());
    let verification = yoke::verify::execute(
        &engine,
        &plan.id,
        command,
        args,
        true,
        &Deadline::new(60_000),
    )
    .unwrap();
    println!("actual-{path}-attachment-verification={verification}");
    assert_eq!(verification["status"], "passed", "{verification}");
    let derived = engine.store.snapshot(&plan.result_snapshot_id).unwrap();
    assert_eq!(
        query::source(&engine.store, &derived, path).unwrap().0,
        expected
    );
    assert_eq!(serde_json::to_value(selected).unwrap(), original);
    changes::approve(&engine.store, &plan.id, false).unwrap();
    let receipt = changes::apply(
        &engine.store,
        root.path(),
        &json!({"planId":plan.id,"idempotencyKey":"actual-attachment-delete","mode":"workspace"}),
        true,
        &Deadline::new(30_000),
    )
    .unwrap();
    assert_eq!(
        fs::read(root.path().join(path)).unwrap(),
        expected.as_bytes()
    );
    changes::recover(&engine.store, root.path(), &receipt.id, true).unwrap();
    assert_eq!(fs::read(root.path().join(path)).unwrap(), source.as_bytes());
    assert_eq!(
        query::source(&engine.store, &snapshot, path).unwrap().0,
        source
    );
}

#[test]
#[ignore = "requires YOKE_RUSTC selecting the actual installed Rust compiler"]
fn actual_rust_compiler_rejects_an_attribute_left_on_a_neighbor_after_deletion() {
    let compiler = std::env::var("YOKE_RUSTC").expect("Select the actual installed Rust compiler");
    checked_cycle(
        "module.rs",
        "// owned café 🦀\r\n#[inline]\r\npub fn delete_me() {}\r\npub const RETAINED: u32 = 2;\r\n",
        "// owned café 🦀\r\n\r\npub const RETAINED: u32 = 2;\r\n",
        &compiler,
        &[
            "--edition=2021".into(),
            "--crate-type=lib".into(),
            "--emit=metadata".into(),
            "module.rs".into(),
        ],
    );
}

#[test]
#[ignore = "requires YOKE_PYTHON selecting the actual installed Python compiler/stdlib AST"]
fn actual_python_ast_rejects_a_decorator_reattached_to_a_neighbor_after_deletion() {
    let compiler =
        std::env::var("YOKE_PYTHON").expect("Select the actual installed Python compiler");
    let check = "import ast,pathlib; text=pathlib.Path('module.py').read_text(encoding='utf-8'); compile(text,'module.py','exec',dont_inherit=True); tree=ast.parse(text); functions=[n for n in tree.body if isinstance(n,ast.FunctionDef)]; assert [n.name for n in functions]==['identity','retained']; assert functions[-1].decorator_list==[], 'Deletion reattached a decorator to the retained function'";
    checked_cycle(
        "module.py",
        "# owned café 🦀\r\ndef identity(function):\r\n    return function\r\n\r\n@identity\r\ndef delete_me():\r\n    return 1\r\n\r\ndef retained():\r\n    return 2\r\n",
        "# owned café 🦀\r\ndef identity(function):\r\n    return function\r\n\r\n\r\n\r\ndef retained():\r\n    return 2\r\n",
        &compiler,
        &["-I".into(), "-c".into(), check.into()],
    );
}
