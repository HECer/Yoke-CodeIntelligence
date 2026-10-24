use std::{fs, path::PathBuf};
use yoke::{
    Engine,
    capture::Deadline,
    projects::{self, CompilerSpec},
};
#[test]
fn changed_trusted_standard_library_changes_context_identity_for_unchanged_source() {
    let (_dir, engine, snapshot) = fixture(&[
        ("tsconfig.json", r#"{"files":["one.ts"]}"#),
        ("one.ts", "export const sample: YokeAuditPrimitive = 3;"),
    ]);
    let library = tempfile::tempdir().unwrap();
    let mut spec = compiler();
    for entry in fs::read_dir(spec.typescript.parent().unwrap()).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        let name = name.to_str().unwrap();
        if name == "typescript.js" || (name.starts_with("lib") && name.ends_with(".d.ts")) {
            fs::copy(entry.path(), library.path().join(name)).unwrap();
        }
    }
    spec.typescript = library.path().join("typescript.js");
    let es5 = library.path().join("lib.es5.d.ts");
    let original = fs::read_to_string(&es5).unwrap();
    fs::write(
        &es5,
        format!("{original}\ntype YokeAuditPrimitive = number;\n"),
    )
    .unwrap();
    let first = projects::analyze(&engine.store, &snapshot, &spec, &Deadline::new(20_000)).unwrap();
    fs::write(
        &es5,
        format!("{original}\ntype YokeAuditPrimitive = string;\n"),
    )
    .unwrap();
    let second =
        projects::analyze(&engine.store, &snapshot, &spec, &Deadline::new(20_000)).unwrap();
    assert_eq!(first["status"], "passed");
    assert_eq!(second["status"], "failed");
    assert_ne!(
        first["contexts"][0]["contextId"], second["contexts"][0]["contextId"],
        "A compiler context must bind the standard library bytes it used"
    );
    assert_eq!(first["snapshotId"], second["snapshotId"]);
}
fn compiler() -> CompilerSpec {
    let binary = if cfg!(windows) { "node.exe" } else { "node" };
    let node = std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|p| p.join(binary))
        .find(|p| p.is_file())
        .expect("Actual Node must be installed for compiler integration tests")
        .canonicalize()
        .unwrap();
    CompilerSpec {
        node,
        typescript: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../node_modules/typescript/lib/typescript.js")
            .canonicalize()
            .unwrap(),
        trusted: true,
    }
}
fn fixture(entries: &[(&str, &str)]) -> (tempfile::TempDir, Engine, yoke::model::Snapshot) {
    let dir = tempfile::tempdir().unwrap();
    for (path, text) in entries {
        let p = dir.path().join(path);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }
    let mut engine = Engine::open(dir.path()).unwrap();
    let (snapshot, _, _, _) = engine.index(&Deadline::new(10_000)).unwrap();
    (dir, engine, snapshot)
}
#[test]
fn configured_compiler_uses_historical_sources_extends_and_exact_unicode_bindings() {
    let (dir, engine, snapshot) = fixture(&[
        (
            "base.json",
            r#"{"compilerOptions":{"strict":true,"target":"ES2022","module":"CommonJS"}}"#,
        ),
        (
            "tsconfig.json",
            r#"{"extends":"./base.json","include":["src/**/*.ts"],"exclude":["src/excluded.ts"]}"#,
        ),
        ("src/value.ts", "export const café = 7;\n"),
        (
            "src/main.ts",
            "import { café } from './value';\nconst emoji = '🌍';\nexport const result = café;\n",
        ),
        ("src/excluded.ts", "export const failure: number = 'bad';\n"),
    ]);
    fs::write(
        dir.path().join("src/value.ts"),
        "export const changed = 'live';",
    )
    .unwrap();
    let report = projects::analyze(
        &engine.store,
        &snapshot,
        &compiler(),
        &Deadline::new(20_000),
    )
    .unwrap();
    assert_eq!(report["snapshotId"], snapshot.id);
    assert_eq!(report["sourceExecuted"], false);
    assert_eq!(report["contexts"][0]["config"], "tsconfig.json");
    assert_eq!(report["contexts"][0]["status"], "passed", "{report}");
    assert!(
        !report["contexts"][0]["files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f == "src/excluded.ts")
    );
    let bindings = report["contexts"][0]["bindings"].as_array().unwrap();
    let hit = bindings
        .iter()
        .find(|b| {
            b["path"] == "src/main.ts"
                && b["text"] == "café"
                && b["startByte"].as_u64().unwrap() > 60
        })
        .unwrap();
    assert!(
        hit["definitions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["path"] == "src/value.ts" && d["text"] == "café")
    );
    assert!(report["artifactId"].is_string());
}
#[test]
fn overlapping_projects_keep_distinct_compiler_contexts_and_report_binding_conflicts() {
    let (_dir, engine, snapshot) = fixture(&[
        (
            "a/tsconfig.json",
            r#"{"compilerOptions":{"baseUrl":"..","paths":{"choice":["a/value.ts"]}},"files":["../shared.ts"]}"#,
        ),
        (
            "b/tsconfig.json",
            r#"{"compilerOptions":{"baseUrl":"..","paths":{"choice":["b/value.ts"]}},"files":["../shared.ts"]}"#,
        ),
        ("a/value.ts", "export const choice = 1;"),
        ("b/value.ts", "export const choice = 'two';"),
        (
            "shared.ts",
            "import { choice } from 'choice'; export const selected = choice;",
        ),
    ]);
    let report = projects::analyze(
        &engine.store,
        &snapshot,
        &compiler(),
        &Deadline::new(20_000),
    )
    .unwrap();
    let contexts = report["contexts"].as_array().unwrap();
    assert_eq!(contexts.len(), 2);
    assert_ne!(contexts[0]["contextId"], contexts[1]["contextId"]);
    assert!(
        !report["bindingConflicts"].as_array().unwrap().is_empty(),
        "{report}"
    );
}
#[test]
fn solution_build_consumes_referenced_declarations_without_source_tree_writes() {
    let (dir, engine, snapshot) = fixture(&[
        (
            "tsconfig.json",
            r#"{"files":[],"references":[{"path":"./app"}]}"#,
        ),
        (
            "lib/tsconfig.json",
            r#"{"compilerOptions":{"composite":true,"outDir":"../generated/lib","rootDir":".","target":"ES2022"},"files":["index.ts"]}"#,
        ),
        (
            "lib/index.ts",
            "export function value(): number { return 7; }",
        ),
        (
            "app/tsconfig.json",
            r#"{"compilerOptions":{"composite":true,"outDir":"../generated/app","rootDir":".","target":"ES2022","module":"CommonJS"},"files":["index.ts"],"references":[{"path":"../lib"}]}"#,
        ),
        (
            "app/index.ts",
            "import { value } from '../lib'; export const answer: number = value();",
        ),
    ]);
    let report = projects::build(
        &engine.store,
        &snapshot,
        &compiler(),
        &Deadline::new(30_000),
    )
    .unwrap();
    assert_eq!(report["status"], "passed", "{report}");
    assert_eq!(report["sourceTreeWritten"], false);
    assert!(
        report["outputs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|o| o["path"] == "generated/lib/index.d.ts")
    );
    assert!(!dir.path().join("generated").exists());
    assert!(!dir.path().join("lib/tsconfig.tsbuildinfo").exists());
    assert!(report["artifactId"].is_string());
    let analyzed = projects::analyze(
        &engine.store,
        &snapshot,
        &compiler(),
        &Deadline::new(30_000),
    )
    .unwrap();
    assert_eq!(analyzed["status"], "passed", "{analyzed}");
    let app = analyzed["contexts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["config"] == "app/tsconfig.json")
        .unwrap();
    assert!(
        app["bindings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|b| b["path"] == "app/index.ts"
                && b["text"] == "value"
                && b["definitions"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|d| d["path"] == "lib/index.ts")),
        "{analyzed}"
    );
}
#[test]
fn missing_references_cycles_and_empty_solution_never_become_successful_builds() {
    let (_dir, engine, snapshot) = fixture(&[(
        "tsconfig.json",
        r#"{"files":[],"references":[{"path":"./missing"}]}"#,
    )]);
    let report = projects::build(
        &engine.store,
        &snapshot,
        &compiler(),
        &Deadline::new(20_000),
    )
    .unwrap();
    assert_ne!(report["status"], "passed");
    assert!(!report["diagnostics"].as_array().unwrap().is_empty());
    let (_dir, engine, snapshot) = fixture(&[("one.ts", "export const one = 1;")]);
    let report = projects::build(
        &engine.store,
        &snapshot,
        &compiler(),
        &Deadline::new(20_000),
    )
    .unwrap();
    assert_eq!(report["status"], "incomplete");
    let (_dir, engine, snapshot) = fixture(&[
        (
            "tsconfig.json",
            r#"{"files":[],"references":[{"path":"./b"}]}"#,
        ),
        (
            "b/tsconfig.json",
            r#"{"files":[],"references":[{"path":".."}]}"#,
        ),
    ]);
    let report = projects::build(
        &engine.store,
        &snapshot,
        &compiler(),
        &Deadline::new(20_000),
    )
    .unwrap();
    assert_ne!(report["status"], "passed");
}
#[test]
fn compiler_output_outside_snapshot_and_overwriting_inputs_are_denied() {
    let (dir, engine, snapshot) = fixture(&[
        (
            "tsconfig.json",
            r#"{"compilerOptions":{"outDir":"../escape"},"files":["one.ts"]}"#,
        ),
        ("one.ts", "export const one = 1;"),
    ]);
    let result = projects::build(
        &engine.store,
        &snapshot,
        &compiler(),
        &Deadline::new(20_000),
    );
    if let Ok(report) = result {
        assert_eq!(report["status"], "failed", "{report}");
        assert!(!report["diagnostics"].as_array().unwrap().is_empty());
        assert!(report["outputs"].as_array().unwrap().is_empty());
    }
    assert!(!dir.path().parent().unwrap().join("escape/one.js").exists());
    let (_dir, engine, snapshot) = fixture(&[
        (
            "tsconfig.json",
            r#"{"compilerOptions":{"allowJs":true},"files":["one.js"]}"#,
        ),
        ("one.js", "export const one = 1;"),
    ]);
    let report = projects::build(
        &engine.store,
        &snapshot,
        &compiler(),
        &Deadline::new(20_000),
    )
    .unwrap();
    assert_ne!(report["status"], "passed");
}
#[test]
fn compiler_requires_host_trust_and_captured_config_cannot_enable_plugins() {
    let (dir, engine, snapshot) = fixture(&[
        (
            "tsconfig.json",
            r#"{"compilerOptions":{"plugins":[{"name":"./plugin.cjs"}]},"files":["one.ts"]}"#,
        ),
        ("one.ts", "export const one = 1;"),
        (
            "plugin.cjs",
            "require('node:fs').writeFileSync('executed','bad');",
        ),
    ]);
    let mut spec = compiler();
    spec.trusted = false;
    assert!(projects::analyze(&engine.store, &snapshot, &spec, &Deadline::new(20_000)).is_err());
    spec.trusted = true;
    let report =
        projects::analyze(&engine.store, &snapshot, &spec, &Deadline::new(20_000)).unwrap();
    assert!(!report["limitations"].as_array().unwrap().is_empty());
    assert!(!dir.path().join("executed").exists());
}
