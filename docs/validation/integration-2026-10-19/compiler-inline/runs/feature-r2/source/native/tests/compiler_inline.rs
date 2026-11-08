//! Real installed TypeScript 5.8.3, not a scripted WorkspaceEdit provider.
use serde_json::{Value, json};
use std::{fs, path::PathBuf, process::Command};
use yoke::{
    Engine,
    analysis::AnalyzerEpoch,
    capture::Deadline,
    project_session::{HostProfile, Session},
    projects::CompilerSpec,
};
fn d() -> Deadline {
    Deadline::new(30_000)
}
fn compiler() -> CompilerSpec {
    let node = std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|p| p.join(if cfg!(windows) { "node.exe" } else { "node" }))
        .find(|p| p.is_file())
        .unwrap()
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
fn fixture(source: &str) -> (tempfile::TempDir, Engine) {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("main.ts"), source).unwrap();
    fs::write(root.path().join("tsconfig.json"),r#"{"compilerOptions":{"strict":true,"target":"ES2022","module":"CommonJS","noEmit":true},"files":["main.ts"]}"#).unwrap();
    let engine = Engine::open(root.path()).unwrap();
    (root, engine)
}
fn args(keep: bool) -> Value {
    json!({"relativePath":"main.ts","namePath":"helper","keepDefinition":keep})
}
fn planned(engine: &Engine, value: &Value) -> String {
    let snapshot = engine
        .store
        .snapshot(
            value["actions"][0]["plan"]["resultSnapshotId"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
    yoke::query::source(&engine.store, &snapshot, "main.ts")
        .unwrap()
        .0
}
fn unpublished(engine: &Engine) {
    assert_eq!(engine.store.db.query_row("SELECT COUNT(*) FROM records WHERE kind IN ('plan','approval','receipt','journal')",[],|r|r.get::<_,usize>(0)).unwrap(),0);
    assert_eq!(
        engine
            .store
            .db
            .query_row(
                "SELECT COUNT(*) FROM snapshots WHERE json_extract(value,'$.kind')='derived'",
                [],
                |r| r.get::<_, usize>(0)
            )
            .unwrap(),
        0
    );
}

#[test]
fn actual_compiler_inlines_a_middle_sibling_and_unicode_crlf_sources_in_both_epochs() {
    let source = "\u{feff}// café 🌍\r\nconst left=1, helper=(n:number)=>n+left, right=3;\r\nexport const result=helper(2)+helper(right);\r\n";
    for epoch in [AnalyzerEpoch::V8, AnalyzerEpoch::V9] {
        let (root, mut engine) = fixture(source);
        engine.capture_epoch = epoch;
        let value =
            yoke::compiler_inline::prepare(&mut engine, &compiler(), &args(false), &d()).unwrap();
        let after = planned(&engine, &value);
        assert!(
            after.starts_with("\u{feff}// café 🌍\r\nconst left=1, "),
            "{after}"
        );
        assert!(after.contains("right=3;\r\n"), "{after}");
        assert!(!after.contains("helper"), "{after}");
        assert_eq!(after.matches("n+left").count(), 2);
        assert_eq!(value["provider"], "host_selected_typescript_compiler");
        assert_eq!(value["commandsExecuted"], false);
        assert_eq!(value["complete"], false);
        assert_eq!(
            fs::read(root.path().join("main.ts")).unwrap(),
            source.as_bytes()
        );
    }
}

#[test]
fn actual_compiler_keep_definition_preserves_exact_bytes_with_earlier_and_later_consumers() {
    let source = "export function earlier(){return helper(2);}\nconst helper = (n: number) => n * 2;\nexport const later=helper(3);\n";
    let (root, mut engine) = fixture(source);
    let value =
        yoke::compiler_inline::prepare(&mut engine, &compiler(), &args(true), &d()).unwrap();
    let after = planned(&engine, &value);
    assert!(
        after.contains("const helper = (n: number) => n * 2;\n"),
        "{after}"
    );
    assert!(!after.contains("helper("));
    assert_eq!(
        after
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>()
            .matches("n*2")
            .count(),
        3
    );
    assert_eq!(
        fs::read_to_string(root.path().join("main.ts")).unwrap(),
        source
    );
}

#[test]
fn actual_unicode_names_keep_utf8_anchors_and_repository_plugins_inert() {
    let source = "// 🌍\nconst café = (n:number) => n+1;\nexport const answer=café(2);\n";
    let (root, mut engine) = fixture(source);
    let marker = root.path().join("project-code-executed");
    fs::write(
        root.path().join("plugin.cjs"),
        format!(
            "require('node:fs').writeFileSync({},'executed');throw Error('must never load');",
            serde_json::to_string(&marker).unwrap()
        ),
    )
    .unwrap();
    fs::write(root.path().join("tsconfig.json"),r#"{"compilerOptions":{"strict":true,"target":"ES2022","plugins":[{"name":"./plugin.cjs"}]},"files":["main.ts"]}"#).unwrap();
    let value = yoke::compiler_inline::prepare(
        &mut engine,
        &compiler(),
        &json!({"relativePath":"main.ts","namePath":"café"}),
        &d(),
    )
    .unwrap();
    let after = planned(&engine, &value);
    assert!(after.starts_with("// 🌍\n"));
    assert!(!after.contains("café"));
    assert!(!marker.exists());
}

#[test]
fn captured_readonly_dependencies_and_oversized_input_cannot_gain_inline_write_authority() {
    let (root, mut engine) = fixture("const helper=(n:number)=>n+1; export const x=helper(2);\n");
    fs::create_dir_all(root.path().join("vendor")).unwrap();
    fs::write(
        root.path().join("vendor/main.ts"),
        "const helper=(n:number)=>n+1; export const x=helper(2);\n",
    )
    .unwrap();
    engine.capture_options.read_only_dependency_dirs = vec!["vendor".into()];
    assert!(
        yoke::compiler_inline::prepare(
            &mut engine,
            &compiler(),
            &json!({"relativePath":"vendor/main.ts","namePath":"helper"}),
            &d()
        )
        .is_err()
    );
    unpublished(&engine);
    fs::write(
        root.path().join("main.ts"),
        format!(
            "const helper=(n:number)=>n+1; export const x=helper(2);\n//{}",
            "x".repeat(4_000_000)
        ),
    )
    .unwrap();
    assert!(yoke::compiler_inline::prepare(&mut engine, &compiler(), &args(false), &d()).is_err());
    unpublished(&engine);
}

#[test]
fn actual_compiler_handles_async_anonymous_functions_and_captured_import_bindings() {
    let source = "import { delta } from './dep';\nconst helper = async function(n:number){ return n + delta; };\nexport async function call(){return await helper(2);}\n";
    let (root, mut engine) = fixture(source);
    fs::write(root.path().join("dep.ts"), "export const delta=4;\n").unwrap();
    let value =
        yoke::compiler_inline::prepare(&mut engine, &compiler(), &args(false), &d()).unwrap();
    let after = planned(&engine, &value);
    assert!(after.starts_with("import { delta } from './dep';\n"));
    assert!(after.contains("async function"));
    assert!(!after.contains("helper"));
    assert!(value["sourceBindings"]["dep.ts"].is_string());
}

#[test]
fn compiler_inline_refuses_unsupported_and_unsafe_inputs_without_hidden_plans() {
    for source in [
        "export const helper=(n:number)=>n+1; export const x=helper(2);",
        "const helper=(n:number)=>n+1;",
        "const helper=(n:number):number=>n?helper(n-1):0; export const x=helper(2);",
        "let helper=(n:number)=>n+1; helper=(n:number)=>n+2; export const x=helper(2);",
        "const x=1; const helper=(n:number)=>n+x; export function caller(){const x=2; return helper(x);}",
        "const helper=()=>this; export const result=helper();",
        "const helper=(n:number)=>n+1; export const broken:number='wrong'; export const x=helper(2);",
        "function helper(n:number){return n+1;} export const x=helper(2);",
    ] {
        let (root, mut engine) = fixture(source);
        let result = yoke::compiler_inline::prepare(&mut engine, &compiler(), &args(false), &d());
        assert!(result.is_err(), "{source}: {result:?}");
        unpublished(&engine);
        assert_eq!(
            fs::read_to_string(root.path().join("main.ts")).unwrap(),
            source
        );
    }
}

#[test]
fn compiler_inline_refuses_cross_file_script_consumers_and_ambiguous_contexts() {
    for ambiguous in [false, true] {
        let (root, mut engine) = fixture("const helper=(n:number)=>n+1; const result=helper(2);\n");
        if ambiguous {
            fs::create_dir(root.path().join("other")).unwrap();
            fs::write(
                root.path().join("other/tsconfig.json"),
                r#"{"files":["../main.ts"]}"#,
            )
            .unwrap();
        } else {
            fs::write(root.path().join("consumer.ts"), "const other=helper(3);\n").unwrap();
            fs::write(
                root.path().join("tsconfig.json"),
                r#"{"compilerOptions":{"target":"ES2022"},"files":["main.ts","consumer.ts"]}"#,
            )
            .unwrap();
        }
        let error = yoke::compiler_inline::prepare(&mut engine, &compiler(), &args(false), &d())
            .unwrap_err();
        assert!(
            format!("{error:#}").contains(if ambiguous {
                "exactly one"
            } else {
                "outside the selected captured file"
            }),
            "{error:#}"
        );
        unpublished(&engine);
    }
}

#[test]
fn inline_authority_paths_stale_snapshot_deadline_and_budget_fail_before_publication() {
    let source = "const helper=(n:number)=>n+1; export const result=helper(2);\n";
    let (_root, mut engine) = fixture(source);
    let mut untrusted = compiler();
    untrusted.trusted = false;
    assert!(yoke::compiler_inline::prepare(&mut engine, &untrusted, &args(false), &d()).is_err());
    for input in [
        json!({"relativePath":"../main.ts","namePath":"helper"}),
        json!({"relativePath":".env","namePath":"helper"}),
        json!({"relativePath":"main.ts","namePath":"helper","keepDefinition":null}),
        json!({"relativePath":"main.ts","namePath":"helper","snapshotId":"0".repeat(64)}),
        json!({"relativePath":"main.ts","namePath":"helper","unknown":"x".repeat(64_000)}),
    ] {
        assert!(yoke::compiler_inline::prepare(&mut engine, &compiler(), &input, &d()).is_err());
    }
    assert!(
        yoke::compiler_inline::prepare(&mut engine, &compiler(), &args(false), &Deadline::new(0))
            .is_err()
    );
    unpublished(&engine);
}

#[test]
fn session_and_jb_facade_require_explicit_host_opt_in_and_edit_mode() {
    let source = "const helper=(n:number)=>n+1; export const result=helper(2);\n";
    for mode in ["editing", "planning"] {
        let (_root, engine) = fixture(source);
        let host = HostProfile {
            inline_compiler: Some(compiler()),
            modes: vec![mode.into()],
            ..Default::default()
        };
        let mut session = Session::open(engine, host, Some("inline-compiler"), &d()).unwrap();
        let result = yoke::agent_repl::invoke_facade(
            &mut session,
            "jb.inline_symbol",
            json!({"relative_path":"main.ts","name_path":"helper"}),
            None,
            &d(),
        );
        if mode == "editing" {
            assert_eq!(
                result.unwrap()["provider"],
                "host_selected_typescript_compiler"
            );
        } else {
            assert!(result.is_err());
            unpublished(&session.engine);
        }
    }
    assert!(
        serde_json::to_value(HostProfile::default())
            .unwrap()
            .get("inlineCompiler")
            .is_none()
    );
    let (_root, engine) = fixture(source);
    let mut session =
        Session::open(engine, HostProfile::default(), Some("old-host"), &d()).unwrap();
    assert!(
        session
            .invoke(
                "refactor.prepare",
                &json!({"operation":"inline","arguments":args(false)}),
                &d()
            )
            .is_err()
    );
    unpublished(&session.engine);
}

#[test]
fn compiler_registration_and_changed_binary_invalidate_session_before_rebinding() {
    let (root, engine) = fixture("const helper=(n:number)=>n+1; const x=helper(2);\n");
    let session = Session::open(engine, HostProfile::default(), Some("pinned-host"), &d()).unwrap();
    drop(session);
    assert!(
        Session::open(
            Engine::open(root.path()).unwrap(),
            HostProfile {
                inline_compiler: Some(compiler()),
                ..Default::default()
            },
            Some("pinned-host"),
            &d()
        )
        .is_err()
    );
    let tools = tempfile::tempdir().unwrap();
    let mut spec = compiler();
    let copy = tools.path().join("typescript.js");
    fs::copy(&spec.typescript, &copy).unwrap();
    spec.typescript = copy.clone();
    let host = HostProfile {
        inline_compiler: Some(spec),
        ..Default::default()
    };
    let session = Session::open(
        Engine::open(root.path()).unwrap(),
        host.clone(),
        Some("changed-tool"),
        &d(),
    )
    .unwrap();
    drop(session);
    use std::io::Write;
    fs::OpenOptions::new()
        .append(true)
        .open(copy)
        .unwrap()
        .write_all(b"\n// changed trusted compiler\n")
        .unwrap();
    assert!(
        Session::open(
            Engine::open(root.path()).unwrap(),
            host,
            Some("changed-tool"),
            &d()
        )
        .is_err()
    );
}

#[test]
fn actual_cli_compiler_plan_requires_independent_tsc_approval_apply_and_conditional_recovery() {
    let source = "const helper=(n:number)=>n+1; export const result=helper(2);\n";
    let (root, mut engine) = fixture(source);
    let spec = compiler();
    let host = tempfile::tempdir().unwrap();
    fs::write(
        host.path().join("host.json"),
        serde_json::to_vec(&HostProfile {
            inline_compiler: Some(spec.clone()),
            ..Default::default()
        })
        .unwrap(),
    )
    .unwrap();
    fs::write(host.path().join("requests.json"),serde_json::to_vec(&json!([{"operation":"refactor.prepare","args":{"operation":"inline","arguments":args(false)}}])).unwrap()).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(root.path())
        .arg("session")
        .args(["--id", "compiler-cli", "--host-profile"])
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
    let plan = &value["results"][0]["actions"][0]["plan"];
    let id = plan["id"].as_str().unwrap();
    engine.allow_writes = true;
    engine.approve(id, false).unwrap();
    let refused = engine
        .invoke(
            "ci_change_apply",
            json!({"planId":id,"mode":"workspace","idempotencyKey":"before-independent-verify"}),
        )
        .unwrap();
    assert_eq!(refused["status"], "error");
    let tsc = spec.typescript.parent().unwrap().join("tsc.js");
    let verify = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(root.path())
        .args(["verify", "--plan", id, "--trust-project", "--command"])
        .arg(&spec.node)
        .arg(format!("--arg={}", tsc.display()))
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
            json!({"planId":id,"mode":"workspace","idempotencyKey":"compiler-verified"}),
        )
        .unwrap();
    assert_eq!(applied["status"], "ok", "{applied}");
    let expected = fs::read(root.path().join("main.ts")).unwrap();
    assert_ne!(expected, source.as_bytes());
    let receipt = applied["data"]["receipt"]["id"].as_str().unwrap();
    fs::write(root.path().join("main.ts"), "// foreign writer\n").unwrap();
    assert!(yoke::changes::recover(&engine.store, &engine.root, receipt, true).is_err());
    assert_eq!(
        fs::read_to_string(root.path().join("main.ts")).unwrap(),
        "// foreign writer\n"
    );
    fs::write(root.path().join("main.ts"), expected).unwrap();
    yoke::changes::recover(&engine.store, &engine.root, receipt, true).unwrap();
    assert_eq!(
        fs::read_to_string(root.path().join("main.ts")).unwrap(),
        source
    );
}
