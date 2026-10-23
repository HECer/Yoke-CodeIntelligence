//! Actual compiler import attributes over immutable captures; no source executes.
use serde_json::{Value, json};
use std::{fs, path::PathBuf};
use yoke::{Engine, capture::Deadline, projects::CompilerSpec};
fn d() -> Deadline {
    Deadline::new(60_000)
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
const ATTR: &str = r#"with { /* original ? */ "flavor": "owned ??", mode: "static" }"#;
type Fixture = (tempfile::TempDir, Engine, Vec<(String, Vec<u8>)>);
fn fixture(extension: &str, binding: &str, expression: &str) -> Fixture {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("nested")).unwrap();
    let parameter = if matches!(extension, "ts" | "tsx") {
        "n:number"
    } else {
        "n"
    };
    let source = format!(
        "\u{feff}import {binding} from './dep' {ATTR};\r\n/** @param {{number}} n */\r\nexport function helper({parameter}){{return {expression};}}\r\nexport const keep=3;\r\n"
    );
    let files = vec![ (format!("source.{extension}"), source.into_bytes()),
        (format!("nested/target.{extension}"), b"const step=20;\r\nexport const other=step;\r\n".to_vec()),
        (format!("dep.{extension}"), b"export const delta=2; export default 4;\r\n".to_vec()),
        (format!("consumer.{extension}"), b"export {helper as renamed,keep} from './source';\r\n".to_vec()),
        ("tsconfig.json".into(), json!({"compilerOptions":{"strict":true,"target":"ES2022","module":"ESNext","moduleResolution":"Bundler","allowJs":true,"checkJs":true,"jsx":"preserve","noEmit":true},"files":[format!("source.{extension}"),format!("nested/target.{extension}"),format!("dep.{extension}"),format!("consumer.{extension}")]}).to_string().into_bytes()) ];
    for (name, text) in &files {
        fs::write(root.path().join(name), text).unwrap();
    }
    let engine = Engine::open(root.path()).unwrap();
    (root, engine, files)
}
fn args(extension: &str) -> Value {
    json!({"relativePath":format!("source.{extension}"),"namePath":"helper","targetRelativePath":format!("nested/target.{extension}")})
}
fn planned(engine: &Engine, result: &Value, extension: &str) -> String {
    let snapshot = engine
        .store
        .snapshot(result["plan"]["resultSnapshotId"].as_str().unwrap())
        .unwrap();
    yoke::query::source(
        &engine.store,
        &snapshot,
        &format!("nested/target.{extension}"),
    )
    .unwrap()
    .0
}
#[test]
fn named_default_namespace_attributes_relocate_with_exact_unicode_comment_bytes_in_all_four_languages()
 {
    for extension in ["ts", "tsx", "js", "jsx"] {
        for (binding, expression) in [
            ("{delta as step}", "n+step"),
            ("step", "n+step"),
            ("* as step", "n+step.delta"),
        ] {
            let (root, mut engine, files) = fixture(extension, binding, expression);
            let result =
                yoke::compiler_move::prepare(&mut engine, &compiler(), &args(extension), &d())
                    .unwrap();
            let after = planned(&engine, &result, extension);
            assert!(after.contains(ATTR), "{extension}: {after}");
            assert!(after.contains("../dep"), "{after}");
            assert!(
                result["sourceRepairs"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|r| r["kind"] == "importAttributes")
            );
            assert_eq!(result["complete"], false);
            assert_eq!(result["sourceExecuted"], false);
            for (name, text) in files {
                assert_eq!(fs::read(root.path().join(name)).unwrap(), text);
            }
        }
    }
}
#[test]
fn omitted_shorthand_import_retains_attributes_through_original_byte_repair() {
    let (root, mut engine, files) = fixture("ts", "{delta as step}", "{step,n}");
    let result = yoke::compiler_move::prepare(&mut engine, &compiler(), &args("ts"), &d()).unwrap();
    let after = planned(&engine, &result, "ts");
    assert!(after.contains(ATTR), "{after}");
    assert!(
        result["sourceRepairs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["kind"] == "missingImport"),
        "{result}"
    );
    for (name, text) in files {
        assert_eq!(fs::read(root.path().join(name)).unwrap(), text);
    }
}
#[test]
fn conflicting_original_attribute_requests_refuse_without_publishing_a_plan() {
    let (root, mut engine, mut files) = fixture("ts", "{delta as step}", "n+step");
    let target = "import {delta as original} from '../dep' with {flavor:'other'};\r\nexport const other=original;\r\n".to_string();
    fs::write(root.path().join("nested/target.ts"), &target).unwrap();
    files[1].1 = target.into_bytes();
    let error =
        yoke::compiler_move::prepare(&mut engine, &compiler(), &args("ts"), &d()).unwrap_err();
    assert!(format!("{error:#}").contains("attributes"), "{error:#}");
    assert_eq!(
        engine
            .store
            .db
            .query_row("SELECT COUNT(*) FROM records WHERE kind='plan'", [], |r| {
                r.get::<_, usize>(0)
            })
            .unwrap(),
        0
    );
    for (name, text) in files {
        assert_eq!(fs::read(root.path().join(name)).unwrap(), text);
    }
}
#[test]
fn attributed_move_keeps_path_and_deadline_refusals_without_plan_publication() {
    let (root, mut engine, files) = fixture("ts", "{delta as step}", "n+step");
    for path in ["../outside.ts", "nested/../target.ts", "C:/outside.ts"] {
        let mut bad = args("ts");
        bad["targetRelativePath"] = json!(path);
        assert!(yoke::compiler_move::prepare(&mut engine, &compiler(), &bad, &d()).is_err());
    }
    assert!(
        yoke::compiler_move::prepare(&mut engine, &compiler(), &args("ts"), &Deadline::new(0))
            .is_err()
    );
    assert_eq!(
        engine
            .store
            .db
            .query_row("SELECT COUNT(*) FROM records WHERE kind='plan'", [], |r| {
                r.get::<_, usize>(0)
            })
            .unwrap(),
        0
    );
    for (name, text) in files {
        assert_eq!(fs::read(root.path().join(name)).unwrap(), text);
    }
}

#[test]
fn actual_attributed_cli_move_requires_verified_approval_write_permission_and_conditional_recovery()
{
    use std::process::Command;
    let (root, mut engine, files) = fixture("ts", "{delta as step}", "{step,n}");
    let spec = compiler();
    let host = tempfile::tempdir().unwrap();
    let profile = host.path().join("host.json");
    let requests = host.path().join("requests.json");
    fs::write(
        &profile,
        json!({"moveCompiler":{"node":spec.node,"typescript":spec.typescript,"trusted":true}})
            .to_string(),
    )
    .unwrap();
    fs::write(&requests,json!([{"operation":"refactor.prepare","args":{"operation":"move","arguments":args("ts")}}]).to_string()).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(root.path())
        .args(["session", "--id", "owned-attributes", "--host-profile"])
        .arg(&profile)
        .arg("--requests")
        .arg(&requests)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    let result = &value["results"][0];
    assert!(planned(&engine, result, "ts").contains(ATTR));
    let id = result["plan"]["id"].as_str().unwrap();
    let apply = |engine: &mut Engine, key: &str| {
        engine
            .invoke(
                "ci_change_apply",
                json!({"planId":id,"mode":"workspace","idempotencyKey":key}),
            )
            .unwrap()
    };
    assert_eq!(apply(&mut engine, "without-permission")["status"], "error");
    engine.allow_writes = true;
    assert_eq!(apply(&mut engine, "without-approval")["status"], "error");
    engine.approve(id, false).unwrap();
    assert_eq!(
        apply(&mut engine, "without-verification")["status"],
        "error"
    );
    let verified = Command::new(env!("CARGO_BIN_EXE_yoke"))
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
        verified.status.success(),
        "{}",
        String::from_utf8_lossy(&verified.stderr)
    );
    for (name, text) in &files {
        assert_eq!(fs::read(root.path().join(name)).unwrap(), *text);
    }
    let original_source = fs::read(root.path().join("source.ts")).unwrap();
    let mut stale = original_source.clone();
    stale.extend_from_slice(b"// foreign source change\n");
    fs::write(root.path().join("source.ts"), &stale).unwrap();
    assert_eq!(apply(&mut engine, "stale-source")["status"], "error");
    assert_eq!(fs::read(root.path().join("source.ts")).unwrap(), stale);
    fs::write(root.path().join("source.ts"), original_source).unwrap();
    let applied = apply(&mut engine, "verified");
    assert_eq!(applied["status"], "ok", "{applied}");
    let target = fs::read(root.path().join("nested/target.ts")).unwrap();
    assert!(String::from_utf8_lossy(&target).contains(ATTR));
    let receipt = applied["data"]["receipt"]["id"].as_str().unwrap();
    fs::write(root.path().join("nested/target.ts"), b"// foreign writer\n").unwrap();
    assert!(yoke::changes::recover(&engine.store, &engine.root, receipt, true).is_err());
    assert_eq!(
        fs::read(root.path().join("nested/target.ts")).unwrap(),
        b"// foreign writer\n"
    );
    fs::write(root.path().join("nested/target.ts"), target).unwrap();
    yoke::changes::recover(&engine.store, &engine.root, receipt, true).unwrap();
    for (name, text) in files {
        assert_eq!(fs::read(root.path().join(name)).unwrap(), text);
    }
}
