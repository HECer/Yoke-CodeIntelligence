//! Actual compiler observations plus independent native body-frame admission.
use serde_json::{Value, json};
use std::{fs, path::PathBuf, process::Command};
use yoke::project_session::HostProfile;
use yoke::{Engine, analysis::AnalyzerEpoch, capture::Deadline, projects::CompilerSpec};
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
fn fixture(source: &str, language: &str) -> (tempfile::TempDir, Engine) {
    let root = tempfile::tempdir().unwrap();
    let file = format!("main.{language}");
    fs::write(root.path().join(&file), source).unwrap();
    fs::write(root.path().join("tsconfig.json"),serde_json::to_vec(&json!({"compilerOptions":{"strict":true,"target":"ES2022","module":"ESNext","moduleResolution":"Bundler","allowJs":true,"checkJs":true,"noEmit":true},"files":[file]})).unwrap()).unwrap();
    let engine = Engine::open(root.path()).unwrap();
    (root, engine)
}
fn args(keep: bool, language: &str) -> Value {
    json!({"relativePath":format!("main.{language}"),"namePath":"helper","keepDefinition":keep})
}
fn planned(engine: &Engine, r: &Value, language: &str) -> String {
    let snapshot = engine
        .store
        .snapshot(
            r["actions"][0]["plan"]["resultSnapshotId"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
    yoke::query::source(&engine.store, &snapshot, &format!("main.{language}"))
        .unwrap()
        .0
}
fn unpublished(engine: &Engine) {
    assert_eq!(engine.store.db.query_row("SELECT COUNT(*) FROM records WHERE kind IN ('plan','approval','receipt','journal','native_compiler_report_v1')",[],|r|r.get::<_,usize>(0)).unwrap(),0);
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
fn same_execution(before: &str, after: &str) {
    let spec = compiler();
    let script = format!(
        "const ts=require({});const vm=require('node:vm');function observe(s){{const js=ts.transpileModule(s,{{compilerOptions:{{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}}}).outputText;const c={{exports:{{}}}};vm.runInNewContext(js,c,{{timeout:1000}});return JSON.stringify(c.exports.observed);}}if(observe({})!==observe({}))throw Error('owned body observations changed');",
        serde_json::to_string(&spec.typescript).unwrap(),
        serde_json::to_string(before).unwrap(),
        serde_json::to_string(after).unwrap()
    );
    let output = Command::new(spec.node)
        .args(["-e", &script])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{after}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn actual_classic_body_expansion_removes_the_call_and_retains_the_original_definition_only_when_requested()
 {
    let source = "function helper(n:number):number {const value=n+1;return value;}\nexport const observed=helper(2);\n";
    for epoch in [AnalyzerEpoch::V8, AnalyzerEpoch::V9] {
        for keep in [false, true] {
            let (root, mut engine) = fixture(source, "ts");
            engine.capture_epoch = epoch;
            let r =
                yoke::compiler_inline::prepare(&mut engine, &compiler(), &args(keep, "ts"), &d())
                    .unwrap();
            let after = planned(&engine, &r, "ts");
            assert_eq!(after.contains("function helper"), keep, "{after}");
            assert!(!after.contains("helper(2)"), "{after}");
            assert!(after.contains("break __ci_inline_"));
            assert_eq!(r["actions"][0]["kind"], "native.inline.function.body");
            assert_eq!(r["sourceExecuted"], false);
            assert_eq!(r["complete"], false);
            assert_eq!(r["verificationRequired"], true);
            same_execution(source, &after);
            assert_eq!(
                fs::read_to_string(root.path().join("main.ts")).unwrap(),
                source
            );
        }
    }
}
#[test]
fn actual_body_frames_preserve_argument_order_local_scope_closures_and_finally_control() {
    for source in [
        "const events:number[]=[];let n=9;function arg(x:number){events.push(x);return x;}function helper(n:number,m:number):number{events.push(n,m);n++;return n+m;}const result=helper(arg(n),arg(2));export const observed={result,events,n};",
        "const events:string[]=[];function helper(n:number):number{try{events.push('try');return n+1;}finally{events.push('finally');return n+2;}}const result=helper(3);export const observed={result,events};",
        "let n=1;function helper(x:number):number{try{return n;}finally{n=7;}}const result=helper(3);export const observed={result,n};",
        "function helper():number|undefined{exit:while(true){try{return 1;}finally{break exit;}}}const result=helper();export const observed={result,isUndefined:result===undefined};",
        "function helper():number|undefined{for(let n=0;n<2;n++){try{return n;}finally{continue;}}}const result=helper();export const observed={result,isUndefined:result===undefined};",
        "function helper(n:number):()=>number{const inner=()=>{return n+1;};return inner;}const result=helper(3);export const observed=result();",
        "function helper(n:number):number{let sum=0;for(let i=0;i<n;i++){if(i===2)return sum;sum+=i;}return sum;}const a=helper(2);const b=helper(4);export const observed=[a,b];",
        "const events:string[]=[];function helper():number{events.push('called');return 7;}function outer(flag:boolean):number{if(flag)return helper(/* empty */);return 2;}export const observed=[outer(false),outer(true),events];",
        "const events:number[]=[];function helper(n:number):number{return /* before */ events.push(n),n+2 /* after */;}const result=helper(/* café */3,/* trailing */);export const observed={result,events};",
        "function helper(n:number):{value:number}{return {value:n};}const result=helper(3);export const observed=result.value;",
        "function helper(n:number){return {value:n};}const result=helper(3);export const observed=result.value;",
    ] {
        let (root, mut engine) = fixture(source, "ts");
        let r = yoke::compiler_inline::prepare(&mut engine, &compiler(), &args(false, "ts"), &d())
            .unwrap_or_else(|e| panic!("{source}: {e:#}"));
        let after = planned(&engine, &r, "ts");
        same_execution(source, &after);
        for comment in [
            "/* empty */",
            "/* before */",
            "/* after */",
            "/* café */",
            "/* trailing */",
        ] {
            if source.contains(comment) {
                assert!(after.contains(comment), "{after}");
            }
        }
        assert_eq!(
            fs::read_to_string(root.path().join("main.ts")).unwrap(),
            source
        );
    }
}
#[test]
fn actual_jsdoc_body_and_retained_export_keep_their_captured_bindings() {
    for (source, language, keep) in [
        (
            "/** @param {number} n */\nfunction helper(n){return n+1;}\nexport const observed=helper(2);\n",
            "js",
            false,
        ),
        (
            "export function helper(n:number):number{return n+1;}\nexport const observed=helper(2);\n",
            "ts",
            true,
        ),
    ] {
        let (root, mut engine) = fixture(source, language);
        let r =
            yoke::compiler_inline::prepare(&mut engine, &compiler(), &args(keep, language), &d())
                .unwrap_or_else(|e| panic!("{source}: {e:#}"));
        let after = planned(&engine, &r, language);
        same_execution(source, &after);
        assert_eq!(
            fs::read_to_string(root.path().join(format!("main.{language}"))).unwrap(),
            source
        );
    }
}
#[test]
fn parameter_initialization_preserves_defaults_rest_patterns_optional_types_and_inferred_names() {
    for (source, language) in [
        (
            "const events:string[]=[];function arg(){events.push('arg');return undefined;}function helper(n=(events.push('default'),3),m=(events.push('later'),n+1)){return [n,m];}const result=helper(arg(),(events.push('second'),undefined));export const observed={result,events};",
            "ts",
        ),
        (
            "function helper(n=1,m=n+2){return [n,m];}const a=helper();const b=helper(0);export const observed=[a,b];",
            "ts",
        ),
        (
            "const events:string[]=[];function helper(n:number,...rest:number[]){rest.push(n);return rest;}const result=helper((events.push('first'),2),(events.push('second'),3),(events.push('third'),4));export const observed={result,events};",
            "ts",
        ),
        (
            "function helper(...rest:number[]){return rest.length;}const result=helper();export const observed=result;",
            "ts",
        ),
        (
            "const events:string[]=[];const input={get x(){events.push('get');return undefined;}};function helper({x=(events.push('pattern'),3)}: {x?:number}={}){return x;}const a=helper(input);const b=helper();export const observed={a,b,events};",
            "ts",
        ),
        (
            "function helper([n=3,...rest]:number[]=[4,5,6]){return [n,rest];}const a=helper();const b=helper([undefined as unknown as number,7]);export const observed=[a,b];",
            "ts",
        ),
        (
            "function helper(n?:number):number{return n??4;}const a=helper();const b=helper(2);export const observed=[a,b];",
            "ts",
        ),
        (
            "const x=3;function helper(n=()=>x){let x=9;return [n(),x];}const result=helper();export const observed=result;",
            "ts",
        ),
        (
            "function helper(fn=()=>1,cl=class {}){return [fn.name,cl.name,fn()];}const result=helper();export const observed=result;",
            "ts",
        ),
        (
            "function helper(fn:(x:number)=>number=x=>x+1){return [fn.name,fn(3)];}const result=helper();export const observed=result;",
            "ts",
        ),
        (
            "function helper(__proto__=function(){return 3;},\\u0066n=()=>2){return [__proto__.name,fn.name,__proto__(),fn()];}const result=helper();export const observed=result;",
            "ts",
        ),
        (
            "/** @param {number} [n] @param {...number} rest */\nfunction helper(n /* lhs */ = /* rhs */ 3,...rest /* tail */){return [n,rest];}const result=helper(undefined,2,4);export const observed=result;",
            "js",
        ),
    ] {
        let (root, mut engine) = fixture(source, language);
        let r =
            yoke::compiler_inline::prepare(&mut engine, &compiler(), &args(false, language), &d())
                .unwrap_or_else(|e| panic!("{source}: {e:#}"));
        let after = planned(&engine, &r, language);
        same_execution(source, &after);
        for comment in ["/* lhs */", "/* rhs */", "/* tail */"] {
            if source.contains(comment) {
                assert!(after.contains(comment), "{after}");
            }
        }
        assert_eq!(
            fs::read_to_string(root.path().join(format!("main.{language}"))).unwrap(),
            source
        );
    }
}
#[test]
fn unsafe_or_unowned_body_forms_refuse_before_any_plan_or_derived_source() {
    for source in [
        "function helper(n:number){var x=n+1;return x;} export const observed=helper(2);",
        "function helper(n:number){return arguments[0];} export const observed=helper(2);",
        "function helper(n:number):unknown{return this;} export const observed=helper(2);",
        "async function helper(n:number){return n+1;} export const observed=helper(2);",
        "function helper<T>(n:T){return n;} export const observed=helper(2);",
        "const x=1;function helper(n:number){return n+x;}export function caller(){const x=2;return helper(x);}export const observed=caller();",
        "function helper(n:number){return n+1;}export const observed=1+helper(2);",
        "function helper(n:number){return n+1;}const a=helper(2),b=3;export const observed=[a,b];",
        "function helper(n:number){return n?helper(n-1):0;}export const observed=helper(2);",
        "function helper(n:number){return n+1;}helper=(n:number)=>n+2;export const observed=helper(2);",
    ] {
        let (root, mut engine) = fixture(source, "ts");
        assert!(
            yoke::compiler_inline::prepare(&mut engine, &compiler(), &args(false, "ts"), &d())
                .is_err(),
            "{source}"
        );
        unpublished(&engine);
        assert_eq!(
            fs::read_to_string(root.path().join("main.ts")).unwrap(),
            source
        );
    }
}

#[test]
fn body_cli_plans_require_independent_verification_approval_and_foreign_preserving_recovery() {
    for (source, keep) in [
        (
            "function helper(n=2):number{try{return n+1;}finally{n++;}}export const observed=helper();\n",
            false,
        ),
        (
            "export function helper({n=2}:{n?:number}={}):number{return n+1;}export const observed=helper();\n",
            true,
        ),
    ] {
        let (root, mut engine) = fixture(source, "ts");
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
        fs::write(host.path().join("requests.json"),serde_json::to_vec(&json!([{"operation":"refactor.prepare","args":{"operation":"inline","arguments":args(keep,"ts")}}])).unwrap()).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_yoke"))
            .arg("--root")
            .arg(root.path())
            .arg("session")
            .args(["--id", "body-compiler-cli", "--host-profile"])
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
        let report = &value["results"][0];
        assert_eq!(report["actions"][0]["kind"], "native.inline.function.body");
        let id = report["actions"][0]["plan"]["id"].as_str().unwrap();
        engine.allow_writes = true;
        assert_eq!(engine.invoke("ci_change_apply",json!({"planId":id,"mode":"workspace","idempotencyKey":"body-before-verification"})).unwrap()["status"],"error");
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
        assert_eq!(
            engine
                .invoke(
                    "ci_change_apply",
                    json!({"planId":id,"mode":"workspace","idempotencyKey":"body-before-approval"})
                )
                .unwrap()["status"],
            "error"
        );
        engine.approve(id, false).unwrap();
        let applied = engine
            .invoke(
                "ci_change_apply",
                json!({"planId":id,"mode":"workspace","idempotencyKey":"body-approved-verified"}),
            )
            .unwrap();
        assert_eq!(applied["status"], "ok", "{applied}");
        let expected = fs::read(root.path().join("main.ts")).unwrap();
        same_execution(source, std::str::from_utf8(&expected).unwrap());
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
}
