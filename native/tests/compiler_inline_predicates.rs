//! Actual TypeScript predicate/assertion observations and original runtime effects.
use serde_json::{Value, json};
use std::{fs, path::PathBuf, process::Command};
use yoke::{Engine, capture::Deadline, projects::CompilerSpec};
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
fn prepare(source: &str) -> (tempfile::TempDir, Engine, anyhow::Result<Value>) {
    prepare_options(source, false, yoke::analysis::AnalyzerEpoch::V8)
}
fn prepare_options(
    source: &str,
    keep: bool,
    epoch: yoke::analysis::AnalyzerEpoch,
) -> (tempfile::TempDir, Engine, anyhow::Result<Value>) {
    prepare_language(source, "ts", keep, epoch)
}
fn prepare_language(
    source: &str,
    language: &str,
    keep: bool,
    epoch: yoke::analysis::AnalyzerEpoch,
) -> (tempfile::TempDir, Engine, anyhow::Result<Value>) {
    let root = tempfile::tempdir().unwrap();
    let file = format!("main.{language}");
    fs::write(root.path().join(&file), source).unwrap();
    fs::write(root.path().join("tsconfig.json"),serde_json::to_vec(&json!({"compilerOptions":{"strict":true,"target":"ES2022","module":"ESNext","moduleResolution":"Bundler","noEmit":true,"allowJs":language=="js","checkJs":language=="js","noUnusedLocals":true,"noUnusedParameters":true,"allowUnreachableCode":false,"allowUnusedLabels":false,"exactOptionalPropertyTypes":true,"noUncheckedIndexedAccess":true},"files":[file]})).unwrap()).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    engine.capture_epoch = epoch;
    let result = yoke::compiler_inline::prepare(
        &mut engine,
        &compiler(),
        &json!({"relativePath":file,"namePath":"helper","keepDefinition":keep}),
        &Deadline::new(30000),
    );
    assert_eq!(fs::read_to_string(root.path().join(&file)).unwrap(), source);
    (root, engine, result)
}
#[test]
fn predicate_frames_keep_both_capture_epochs_and_explicit_definition_choice() {
    for epoch in [
        yoke::analysis::AnalyzerEpoch::V8,
        yoke::analysis::AnalyzerEpoch::V9,
    ] {
        for keep in [false, true] {
            let source = format!(
                "{}function helper(value:unknown):value is string{{return typeof value==='string';}}\nexport function run(input:unknown){{const ok=helper(input);return ok?input.length:0;}}\nexport const observed=run('owned');\n",
                if keep { "export " } else { "" }
            );
            let (_root, engine, r) = prepare_options(&source, keep, epoch);
            let r = r.unwrap();
            let after = planned(&engine, &r);
            assert_eq!(after.contains("function helper"), keep);
            assert_eq!(r["verificationRequired"], true);
            same_execution(&source, &after);
        }
    }
}
#[test]
fn actual_boolean_control_retains_normal_body_admission() {
    let (_root, _engine, r) = prepare(
        "function helper(value:unknown):boolean{return typeof value==='string';}\nexport const observed=helper('owned');\n",
    );
    assert!(r.is_ok(), "{r:?}");
}
#[test]
fn actual_predicate_and_assertion_frames_preserve_surviving_caller_flow() {
    let sources = [
        "function helper(value:unknown):value is string{return typeof value==='string';}\nexport function run(input:unknown){const ok=helper(input);return ok?input.length:0;}\nexport const observed=[run('owned'),run(7)];\n",
        "function helper(value:unknown):asserts value is string{if(typeof value!=='string')throw Error('owned refusal');}\nexport function run(input:unknown){helper(input);return input.length;}\nexport const observed=run('owned');\n",
    ];
    for source in sources {
        let (_root, engine, r) = prepare(source);
        let r = r.unwrap();
        assert_eq!(r["sourceExecuted"], false);
        assert_eq!(r["complete"], false);
        assert!(r["actions"][0]["plan"]["resultSnapshotId"].is_string());
        assert!(engine.store.db.is_autocommit());
        let after = planned(&engine, &r);
        assert!(!after.contains("helper("), "{after}");
        same_execution(source, &after);
    }
}
fn planned(engine: &Engine, report: &Value) -> String {
    planned_language(engine, report, "ts")
}
fn planned_language(engine: &Engine, report: &Value, language: &str) -> String {
    let snapshot = engine
        .store
        .snapshot(
            report["actions"][0]["plan"]["resultSnapshotId"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
    yoke::query::source(&engine.store, &snapshot, &format!("main.{language}"))
        .unwrap()
        .0
}
#[test]
fn dense_rest_argument_types_preserve_optional_values_in_typescript_and_javascript() {
    for (language, source) in [
        (
            "ts",
            "const events:string[]=[];function mark():number{events.push('arg');return 1;}\nfunction helper(...values:(number|undefined)[]):number{return values.filter(value=>value!==undefined).length;}\nconst result=helper(mark(),undefined);export const observed=[result,events];\n",
        ),
        (
            "js",
            "/** @type {string[]} */ const events=[];function mark(){events.push('arg');return 1;}\n/** @param {...(number|undefined)} values @returns {number} */\nfunction helper(...values){return values.filter(value=>value!==undefined).length;}\nconst result=helper(mark(),undefined);export const observed=[result,events];\n",
        ),
        (
            "ts",
            "const events:string[]=[];function mark():undefined{events.push('arg');return undefined;}\nfunction helper(value:unknown=(events.push('default'),'owned'),...tail:(string|undefined)[]):value is string{events.push(tail[0]===undefined?'undefined':'present');return typeof value==='string';}\nconst result=helper(mark(),undefined);export const observed=[result,events];\n",
        ),
    ] {
        let (_root, engine, r) =
            prepare_language(source, language, false, yoke::analysis::AnalyzerEpoch::V8);
        let r = r.unwrap();
        let after = planned_language(&engine, &r, language);
        same_execution(source, &after);
        assert!(after.contains("typeof __ci_inline_"), "{after}");
    }
}
#[test]
fn inferred_nonflow_legacy_context_does_not_gain_an_ambient_repeatable_reference() {
    let source = "declare const input:unknown;function helper(value:unknown){return typeof value==='string';}\nconst ok=helper(input);export const observed=ok;\n";
    let (_root, engine, r) = prepare(source);
    let r = r.unwrap();
    let after = planned(&engine, &r);
    assert_eq!(after.matches("= (input)").count(), 1, "{after}");
    assert!(
        !after.contains(": typeof __ci_inline_"),
        "Legacy boolean recipe unexpectedly changed: {after}"
    );
    let flowing = source.replace(
        "export const observed=ok;",
        "export const observed=ok?input.length:0;",
    );
    let (_root, engine, r) = prepare(&flowing);
    assert!(r.is_err());
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
}
fn same_execution(before: &str, after: &str) {
    let spec = compiler();
    let script = format!(
        "const ts=require({});const vm=require('node:vm');function read(s){{const code=ts.transpileModule(s,{{compilerOptions:{{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}}}).outputText;const context={{exports:{{}}}};vm.runInNewContext(code,context,{{timeout:1000}});return JSON.stringify(context.exports.observed);}}if(read({})!==read({}))throw Error('owned predicate runtime observations differ');",
        serde_json::to_string(&spec.typescript).unwrap(),
        serde_json::to_string(before).unwrap(),
        serde_json::to_string(after).unwrap()
    );
    let out = Command::new(spec.node)
        .args(["-e", &script])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}\n{after}",
        String::from_utf8_lossy(&out.stderr)
    );
}
#[test]
fn actual_predicate_second_parameter_and_dependent_source_signature_retain_flow() {
    for source in [
        "function helper(note:string,value:unknown):value is string{return note==='owned'&&typeof value==='string';}\nexport function run(input:unknown){const ok=helper('owned',input);return ok?input.length:0;}\nexport const observed=[run('abc'),run(7)];\n",
        "function helper(value:unknown,example:string):value is typeof example{return typeof value===typeof example;}\nexport function run(input:unknown){const ok=helper(input,'owned');return ok?input.length:0;}\nexport const observed=[run('abc'),run(7)];\n",
        "function helper(value:unknown){return typeof value==='string';}\nexport function run(input:unknown){const ok=helper(input);return ok?input.length:0;}\nexport const observed=[run('abc'),run(7)];\n",
        "function helper(value:unknown):value is string{return typeof value==='string';}\nexport function run(input:unknown){return helper(input);}\nexport const observed=[run('abc'),run(7)];\n",
    ] {
        let (_root, engine, r) = prepare(source);
        let r = r.unwrap();
        same_execution(source, &planned(&engine, &r));
    }
}
#[test]
fn actual_getter_calls_defaults_rest_and_argument_side_effects_execute_once() {
    let source = "const events:string[]=[];function mark(tag:string){events.push(tag);return tag;}\nconst box={get value(){events.push('getter');return {n:3};}};\nfunction helper(note:string,value:unknown,...tail:string[]):value is {n:number}{events.push(note,tail[0]||'empty');return typeof value==='object'&&value!==null;}\nconst ok=helper(mark('first'),box.value,mark('last'));export const observed=[ok,events];\n";
    let (_root, engine, r) = prepare(source);
    let r = r.unwrap();
    let after = planned(&engine, &r);
    same_execution(source, &after);
    assert_eq!(after.matches("box.value").count(), 1, "{after}");
}
#[test]
fn actual_truthiness_assertion_destructured_lexical_target_and_unicode_crlf() {
    for source in [
        "function helper(value:unknown):asserts value{if(!value)throw Error('owned false');}\nexport function run(input:unknown){helper(input);return input.toString();}\nexport const observed=run('abc');\n",
        "function helper(value:unknown):value is string{return typeof value==='string';}\nexport function run({input}:{input:unknown}){const ok=helper(input);return ok?input.length:0;}\nexport const observed=run({input:'abc'});\n",
        "// owned \u{03c0}\r\nfunction helper(val\\u0075e:unknown):value is string{return typeof value==='string';}\r\nexport function run(\u{03c0}:unknown){const ok=helper((\u{03c0}));return ok?\u{03c0}.length:0;}\r\nexport const observed=run('owned \u{1f30d}');\r\n",
    ] {
        let (_root, engine, r) = prepare(source);
        let r = r.unwrap();
        same_execution(source, &planned(&engine, &r));
    }
}
#[test]
fn ambient_exported_and_effectful_property_flow_cannot_gain_a_repeatable_read() {
    for source in [
        "declare let input:unknown;function helper(value:unknown):value is string{return typeof value==='string';}\nconst ok=helper(input);export const observed=ok;\n",
        "export let input:unknown='owned';function helper(value:unknown):value is string{return typeof value==='string';}\nconst ok=helper(input);export const observed=ok;\n",
        "function helper(value:unknown):asserts value is string{if(typeof value!=='string')throw Error('owned refusal');}\nexport function run(box:{value:unknown}){helper(box.value);return box.value.length;}\n",
    ] {
        let (_root, engine, r) = prepare(source);
        assert!(r.is_err(), "unsafe predicate context accepted: {r:?}");
        assert_eq!(engine.store.db.query_row("SELECT COUNT(*) FROM records WHERE kind IN ('plan','approval','receipt','journal','native_compiler_report_v1')",[],|r|r.get::<_,usize>(0)).unwrap(),0);
    }
}

#[test]
fn indirect_exports_do_not_gain_replay_reads_and_local_parameter_shadowing_stays_valid() {
    let source = "export const input:unknown=0;function helper(value:unknown):value is string{return typeof value==='string';}\nexport function run(input:unknown){const ok=helper(input);return ok?input.length:0;}\nexport const observed=run('owned');\n";
    let (_root, engine, r) = prepare(source);
    let r = r.unwrap();
    same_execution(source, &planned(&engine, &r));
    for export in ["export {input};", "export {input as publicInput};"] {
        let source = format!(
            "let input:unknown='owned';{export}\nfunction helper(value:unknown):value is string{{return typeof value==='string';}}\nconst ok=helper(input);export const observed=ok;\n"
        );
        let (_root, engine, r) = prepare(&source);
        assert!(
            r.is_err(),
            "Indirectly exported binding gained a repeatable predicate reference"
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
        let inferred = source.replace(":value is string", "");
        let (_root, engine, r) = prepare(&inferred);
        let r = r.unwrap();
        let after = planned(&engine, &r);
        assert!(
            !after.contains(": typeof __ci_inline_"),
            "Inferred nonflow context lost its legacy single-read recipe: {after}"
        );
    }
}
