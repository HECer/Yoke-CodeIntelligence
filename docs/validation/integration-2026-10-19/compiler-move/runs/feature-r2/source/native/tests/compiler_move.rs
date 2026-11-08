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

#[test]
fn actual_declaration_matrix_keeps_original_graph_identity_and_explicit_scalar_origin() {
    for (declaration, name, consumer, scalar) in [
        (
            "export function helper(n:number){return n+1;}",
            "helper",
            "import {helper} from './source'; export const answer=helper(1);",
            false,
        ),
        (
            "export interface Helper {value:number}",
            "Helper",
            "import type {Helper} from './source'; export const answer:Helper={value:1};",
            false,
        ),
        (
            "export type Helper={value:number};",
            "Helper",
            "import type {Helper} from './source'; export const answer:Helper={value:1};",
            false,
        ),
        (
            "export enum Helper {One=1,Two=2}",
            "Helper",
            "import {Helper} from './source'; export const answer=Helper.Two;",
            false,
        ),
        (
            "export namespace Helper {export const value=2;}",
            "Helper",
            "import {Helper} from './source'; export const answer=Helper.value;",
            false,
        ),
        (
            "export const helper=(n:number)=>n+1;",
            "helper",
            "import {helper} from './source'; export const answer=helper(1);",
            false,
        ),
        (
            "export const helper=7;",
            "helper",
            "import {helper} from './source'; export const answer=helper+1;",
            true,
        ),
    ] {
        for epoch in [
            yoke::analysis::AnalyzerEpoch::V8,
            yoke::analysis::AnalyzerEpoch::V9,
        ] {
            let source = format!("{declaration}\nexport const keep=3;\n");
            let target = "export const other=4;\n";
            let (root, mut engine) = fixture(&source, target, consumer);
            engine.capture_epoch = epoch;
            let (_, graph, _, _) = engine.index(&d()).unwrap();
            let graph_id = yoke::model::identity(&graph);
            let result = yoke::compiler_move::prepare(&mut engine, &compiler(), &args(name), &d())
                .unwrap_or_else(|error| panic!("{declaration}: {error:#}"));
            assert_eq!(
                result["intent"]["entityId"].is_null(),
                scalar,
                "{declaration}"
            );
            assert_eq!(
                result["intent"]["sourceSelectionOrigin"],
                if scalar {
                    "independent_captured_variable_declarator_v1"
                } else {
                    "captured_graph_and_exact_ast"
                }
            );
            assert_eq!(
                yoke::model::identity(&engine.store.graph(&graph.snapshot_id).unwrap()),
                graph_id
            );
            assert!(planned(&engine, &result, "target.ts").contains(name));
            source_unchanged(root.path(), &source, target, consumer);
        }
    }
}

#[test]
fn actual_language_matrix_keeps_bom_unicode_crlf_and_repository_plugins_inert() {
    for extension in ["ts", "tsx", "js", "jsx"] {
        let declaration = if matches!(extension, "ts" | "tsx") {
            "export function café(n:number){return n+1;}"
        } else {
            "export function café(n){return n+1;}"
        };
        let source = format!("\u{feff}// 🌍\r\n{declaration}\r\nexport const keep=3;\r\n");
        let target = "\u{feff}export const other=4;\r\n";
        let consumer = "import {café,keep} from './source';\r\nexport const answer=café(keep);\r\n";
        let (root, engine) = fixture(&source, target, consumer);
        drop(engine);
        for file in ["source", "target", "consumer"] {
            fs::rename(
                root.path().join(format!("{file}.ts")),
                root.path().join(format!("{file}.{extension}")),
            )
            .unwrap();
        }
        let marker = root.path().join("project-code-executed");
        fs::write(
            root.path().join("plugin.cjs"),
            format!(
                "require('node:fs').writeFileSync({},'executed');throw Error('must never run');",
                serde_json::to_string(&marker).unwrap()
            ),
        )
        .unwrap();
        fs::write(root.path().join("tsconfig.json"),serde_json::to_vec(&json!({"compilerOptions":{"strict":false,"allowJs":true,"checkJs":true,"jsx":"preserve","target":"ES2022","module":"ESNext","moduleResolution":"Bundler","noEmit":true,"plugins":[{"name":"./plugin.cjs"}]},"files":[format!("source.{extension}"),format!("target.{extension}"),format!("consumer.{extension}")]})).unwrap()).unwrap();
        let mut engine = Engine::open(root.path()).unwrap();
        let result=yoke::compiler_move::prepare(&mut engine,&compiler(),&json!({"relativePath":format!("source.{extension}"),"namePath":"café","targetRelativePath":format!("target.{extension}")}),&d()).unwrap_or_else(|error|panic!("{extension}: {error:#}"));
        for (file, original) in [
            ("source", source.as_str()),
            ("target", target),
            ("consumer", consumer),
        ] {
            assert_eq!(
                fs::read(root.path().join(format!("{file}.{extension}"))).unwrap(),
                original.as_bytes()
            );
            assert!(
                planned(&engine, &result, &format!("{file}.{extension}")).starts_with(
                    if original.starts_with('\u{feff}') {
                        "\u{feff}"
                    } else {
                        "import"
                    }
                )
            );
        }
        assert!(!marker.exists());
    }
}

fn unpublished(engine: &Engine) {
    assert_eq!(engine.store.db.query_row("SELECT COUNT(*) FROM records WHERE kind IN ('plan','approval','receipt','journal')",[],|r|r.get::<_,usize>(0)).unwrap(),0);
}
#[test]
fn invalid_paths_stale_selection_trust_context_and_directives_publish_no_plan() {
    let source = "export function helper(n:number){return n+1;}\nexport const keep=3;\n";
    for bad in [
        json!({"relativePath":"../source.ts","namePath":"helper","targetRelativePath":"target.ts"}),
        json!({"relativePath":"source.ts","namePath":"helper","targetRelativePath":".env"}),
        json!({"relativePath":"source.ts","namePath":"helper","targetRelativePath":"absent.ts"}),
        json!({"relativePath":"source.ts","namePath":"helper","targetRelativePath":"source.ts"}),
        json!({"relativePath":"source.ts","namePath":"helper","targetRelativePath":"target.ts","snapshotId":"a".repeat(64)}),
        json!({"relativePath":"source.ts","namePath":"helper","targetRelativePath":"target.ts","targetParentNamePath":"Container"}),
        json!({"relativePath":"source.ts","namePath":"helper","targetRelativePath":"target.ts","command":"execute"}),
    ] {
        let (root, mut engine) = fixture(source, "export {};\n", "export {};\n");
        assert!(
            yoke::compiler_move::prepare(&mut engine, &compiler(), &bad, &d()).is_err(),
            "{bad}"
        );
        unpublished(&engine);
        assert_eq!(
            fs::read_to_string(root.path().join("source.ts")).unwrap(),
            source
        );
    }
    for (source, target) in [
        (
            "'use client';\nexport function helper(){}\n",
            "export {};\n",
        ),
        (source, "export function helper(){}\n"),
        (
            "export function helper(){return missing;}\n",
            "export {};\n",
        ),
    ] {
        let (_root, mut engine) = fixture(source, target, "export {};\n");
        assert!(
            yoke::compiler_move::prepare(&mut engine, &compiler(), &args("helper"), &d()).is_err()
        );
        unpublished(&engine);
    }
    let (_root, mut engine) = fixture(source, "export {};\n", "export {};\n");
    let mut spec = compiler();
    spec.trusted = false;
    assert!(yoke::compiler_move::prepare(&mut engine, &spec, &args("helper"), &d()).is_err());
    unpublished(&engine);
    let deadline = Deadline::new(0);
    assert!(
        yoke::compiler_move::prepare(&mut engine, &compiler(), &args("helper"), &deadline).is_err()
    );
    unpublished(&engine);
}

#[test]
fn move_profile_is_optional_host_bound_and_routed_through_the_existing_facade() {
    use yoke::project_session::{HostProfile, Session};
    let source = "export function helper(n:number){return n+1;}\nexport const keep=3;\n";
    assert!(
        serde_json::to_value(HostProfile::default())
            .unwrap()
            .get("moveCompiler")
            .is_none()
    );
    let (root, engine) = fixture(source, "export {};\n", "export {};\n");
    let session =
        Session::open(engine, HostProfile::default(), Some("profile-bind"), &d()).unwrap();
    drop(session);
    assert!(
        Session::open(
            Engine::open(root.path()).unwrap(),
            HostProfile {
                move_compiler: Some(compiler()),
                ..Default::default()
            },
            Some("profile-bind"),
            &d()
        )
        .is_err()
    );
    for mode in [None, Some("planning"), Some("readonly")] {
        let (_root, engine) = fixture(source, "export {};\n", "export {};\n");
        let mut session = Session::open(
            engine,
            HostProfile {
                move_compiler: Some(compiler()),
                ..Default::default()
            },
            Some("facade-move"),
            &d(),
        )
        .unwrap();
        if let Some(mode) = mode {
            session
                .invoke("mode.switch", &json!({"mode":mode}), &d())
                .unwrap();
        }
        let result = yoke::agent_repl::run(
            &mut session,
            "jb.move({relative_path:'source.ts',name_path:'helper',target_relative_path:'target.ts'})",
            &d(),
        );
        if mode.is_none() {
            assert!(result.is_ok(), "{result:?}");
        } else {
            assert!(result.is_err());
            unpublished(&session.engine);
        }
    }
}

#[test]
fn actual_cli_move_requires_independent_tsc_approval_and_conditional_recovery() {
    use std::process::Command;
    use yoke::project_session::HostProfile;
    let source = "const delta=2;\nfunction helper(n:number){return n+delta;}\nexport const answer=helper(1);\n";
    let target = "export const other=4;\n";
    let consumer = "export {};\n";
    let (root, mut engine) = fixture(source, target, consumer);
    let spec = compiler();
    let host = tempfile::tempdir().unwrap();
    fs::write(
        host.path().join("host.json"),
        serde_json::to_vec(&HostProfile {
            move_compiler: Some(spec.clone()),
            ..Default::default()
        })
        .unwrap(),
    )
    .unwrap();
    fs::write(host.path().join("requests.json"),serde_json::to_vec(&json!([{"operation":"refactor.prepare","args":{"operation":"move","arguments":args("helper")}}])).unwrap()).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(root.path())
        .args(["session", "--id", "move-cli", "--host-profile"])
        .arg(host.path().join("host.json"))
        .arg("--requests")
        .arg(host.path().join("requests.json"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    let id = value["results"][0]["plan"]["id"].as_str().unwrap();
    engine.allow_writes = true;
    engine.approve(id, false).unwrap();
    assert_eq!(
        engine
            .invoke(
                "ci_change_apply",
                json!({"planId":id,"mode":"workspace","idempotencyKey":"unverified"})
            )
            .unwrap()["status"],
        "error"
    );
    let verify = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(root.path())
        .args(["verify", "--plan", id, "--trust-project", "--command"])
        .arg(&spec.node)
        .arg(format!(
            "--arg={}",
            spec.typescript.parent().unwrap().join("tsc.js").display()
        ))
        .arg("--arg=--noEmit")
        .output()
        .unwrap();
    assert!(
        verify.status.success(),
        "{}",
        String::from_utf8_lossy(&verify.stderr)
    );
    let applied = engine
        .invoke(
            "ci_change_apply",
            json!({"planId":id,"mode":"workspace","idempotencyKey":"verified"}),
        )
        .unwrap();
    assert_eq!(applied["status"], "ok", "{applied}");
    let expected = fs::read(root.path().join("source.ts")).unwrap();
    let receipt = applied["data"]["receipt"]["id"].as_str().unwrap();
    fs::write(root.path().join("source.ts"), "// foreign writer\n").unwrap();
    assert!(yoke::changes::recover(&engine.store, &engine.root, receipt, true).is_err());
    assert_eq!(
        fs::read_to_string(root.path().join("source.ts")).unwrap(),
        "// foreign writer\n"
    );
    fs::write(root.path().join("source.ts"), expected).unwrap();
    yoke::changes::recover(&engine.store, &engine.root, receipt, true).unwrap();
    source_unchanged(root.path(), source, target, consumer);
}
