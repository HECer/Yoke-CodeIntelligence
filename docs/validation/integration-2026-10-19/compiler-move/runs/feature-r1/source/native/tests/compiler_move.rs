//! Real TypeScript compiler moves over immutable captured source.
use serde_json::{Value, json};
use std::{fs, path::PathBuf};
use yoke::{Engine, capture::Deadline, projects::CompilerSpec};
fn d() -> Deadline {
    Deadline::new(30_000)
}
fn compiler() -> CompilerSpec {
    CompilerSpec {
        node: std::env::split_paths(&std::env::var_os("PATH").unwrap())
            .map(|p| p.join(if cfg!(windows) { "node.exe" } else { "node" }))
            .find(|p| p.is_file())
            .unwrap()
            .canonicalize()
            .unwrap(),
        typescript: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../node_modules/typescript/lib/typescript.js")
            .canonicalize()
            .unwrap(),
        trusted: true,
    }
}
fn fixture(source: &str, target: &str, consumer: &str) -> (tempfile::TempDir, Engine) {
    let root = tempfile::tempdir().unwrap();
    for (name, text) in [
        ("source.ts", source),
        ("target.ts", target),
        ("consumer.ts", consumer),
    ] {
        fs::write(root.path().join(name), text).unwrap();
    }
    fs::write(root.path().join("tsconfig.json"),r#"{"compilerOptions":{"strict":true,"target":"ES2022","module":"ESNext","moduleResolution":"Bundler","noEmit":true},"files":["source.ts","target.ts","consumer.ts"]}"#).unwrap();
    let engine = Engine::open(root.path()).unwrap();
    (root, engine)
}
fn args(name: &str) -> Value {
    json!({"relativePath":"source.ts","namePath":name,"targetRelativePath":"target.ts"})
}
fn planned(engine: &Engine, value: &Value, path: &str) -> String {
    let snapshot = engine
        .store
        .snapshot(value["plan"]["resultSnapshotId"].as_str().unwrap())
        .unwrap();
    yoke::query::source(&engine.store, &snapshot, path)
        .unwrap()
        .0
}
fn source_unchanged(root: &std::path::Path, source: &str, target: &str, consumer: &str) {
    for (name, text) in [
        ("source.ts", source),
        ("target.ts", target),
        ("consumer.ts", consumer),
    ] {
        assert_eq!(fs::read(root.join(name)).unwrap(), text.as_bytes());
    }
}
#[test]
fn actual_exported_function_move_repairs_split_consumer_imports_without_source_writes() {
    let source = "export function helper(n:number){return n+1;}\nexport const keep=3;\n";
    let target = "export const other=4;\n";
    let consumer = "import {helper,keep} from './source';\nexport const answer=helper(keep);\n";
    let (root, mut engine) = fixture(source, target, consumer);
    let result =
        yoke::compiler_move::prepare(&mut engine, &compiler(), &args("helper"), &d()).unwrap();
    assert!(!planned(&engine, &result, "source.ts").contains("function helper"));
    assert!(planned(&engine, &result, "target.ts").contains("function helper"));
    let after = planned(&engine, &result, "consumer.ts");
    assert!(after.contains("./target"), "{after}");
    assert!(after.contains("./source"), "{after}");
    assert_eq!(result["provider"], "host_selected_typescript_compiler");
    assert_eq!(result["complete"], false);
    assert_eq!(result["sourceExecuted"], false);
    source_unchanged(root.path(), source, target, consumer);
}
#[test]
fn actual_private_free_dependency_preserves_same_offset_compiler_insertion_order() {
    let source = "const delta=2;\nfunction helper(n:number){return n+delta;}\nexport const answer=helper(1);\n";
    let target = "export const other=4;\n";
    let consumer = "export {};\n";
    let (root, mut engine) = fixture(source, target, consumer);
    let result =
        yoke::compiler_move::prepare(&mut engine, &compiler(), &args("helper"), &d()).unwrap();
    let after = planned(&engine, &result, "source.ts");
    assert!(after.contains("export const delta"), "{after}");
    assert!(after.contains("./target"), "{after}");
    assert!(!after.contains("export import"), "{after}");
    assert!(planned(&engine, &result, "target.ts").contains("./source"));
    assert_eq!(
        result["bindingCheck"]["addedExports"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    source_unchanged(root.path(), source, target, consumer);
}
#[test]
fn actual_jsdoc_class_move_repairs_namespace_consumer_and_keeps_unrelated_member() {
    let source = "/** Helper docs. */\nexport class Helper {value=2;}\nexport const keep=3;\n";
    let target = "export const other=4;\n";
    let consumer =
        "import * as mod from './source';\nexport const answer=new mod.Helper().value+mod.keep;\n";
    let (root, mut engine) = fixture(source, target, consumer);
    let result =
        yoke::compiler_move::prepare(&mut engine, &compiler(), &args("Helper"), &d()).unwrap();
    let after = planned(&engine, &result, "target.ts");
    assert!(after.contains("/** Helper docs. */"), "{after}");
    let consumer_after = planned(&engine, &result, "consumer.ts");
    assert!(consumer_after.contains("./target"), "{consumer_after}");
    assert!(consumer_after.contains("mod.keep"), "{consumer_after}");
    source_unchanged(root.path(), source, target, consumer);
}
