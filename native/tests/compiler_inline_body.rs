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
    same_execution_mode(before, after, false);
}
fn same_execution_mode(before: &str, after: &str, await_result: bool) {
    let spec = compiler();
    let script = format!(
        "const ts=require({});const vm=require('node:vm');const {{isPromise}}=require('node:util').types;async function observe(s){{const js=ts.transpileModule(s,{{compilerOptions:{{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}}}).outputText;const c={{exports:{{}}}};vm.runInNewContext(js,c,{{timeout:1000}});let value=c.exports.observed;if({await_result}&&isPromise(value)){{let timer;try{{value=await Promise.race([value,new Promise((_,reject)=>{{timer=setTimeout(()=>reject(Error('owned async fixture timed out')),1000);}})]);}}finally{{clearTimeout(timer);}}}}return JSON.stringify(value);}}(async()=>{{if(await observe({})!==await observe({}))throw Error('owned body observations changed');}})().catch(e=>{{console.error(e);process.exitCode=1;}});",
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
fn assert_only_inert_type_vars(after: &str) {
    let spec = compiler();
    let script = format!(
        r#"const ts=require({});const sf=ts.createSourceFile('after.ts',{},ts.ScriptTarget.ES2022,true);function bound(n){{if(ts.isIdentifier(n))return[n.text];return n.elements.flatMap(e=>ts.isOmittedExpression(e)?[]:bound(e.name));}}function zero(n){{return ts.isVoidExpression(n)&&ts.isNumericLiteral(n.expression)&&n.expression.text==='0';}}function visit(n){{if(ts.isVariableDeclarationList(n)&&!(n.flags&ts.NodeFlags.BlockScoped)){{let inert=false;for(let p=n.parent;p;p=p.parent)if(ts.isIfStatement(p)&&ts.isBinaryExpression(p.expression)&&p.expression.operatorToken.kind===ts.SyntaxKind.ExclamationEqualsEqualsToken&&zero(p.expression.left)&&zero(p.expression.right)&&p.thenStatement.pos<=n.pos&&p.thenStatement.end>=n.end)inert=true;if(!inert||n.declarations.flatMap(d=>bound(d.name)).some(s=>!/^__ci_inline_[1-9][0-9]*$/.test(s)))throw Error('unhoisted original var or executable type witness');}}ts.forEachChild(n,visit);}}visit(sf);"#,
        serde_json::to_string(&spec.typescript).unwrap(),
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
fn actual_var_hoisting_preserves_parameter_environments_loops_patterns_and_catch_assignments() {
    for source in [
        "function helper(n:number):number{var x=n+1;return x;}const result=helper(2);export const observed=result;",
        "function helper(n=1,f=()=>n):number[]{var n=2;return [n,f()];}const result=helper();export const observed=result;",
        "function helper(n:number):number{var n:number;n++;return n;}const result=helper(2);export const observed=result;",
        "function helper():number{var x:number=1;var x:number;x++;return x;}const result=helper();export const observed=result;",
        "function helper():number[]{const fs:Array<()=>number>=[];for(var i=0;i<3;i++)fs.push(()=>i);return fs.map(f=>f());}const result=helper();export const observed=result;",
        "function helper():number[]{const fs:Array<()=>number>=[];for(var [n] of [[1],[2]])fs.push(()=>n);return fs.map(f=>f());}const result=helper();export const observed=result;",
        "function helper():string[]{const fs:Array<()=>string>=[];for(var n in {a:1,b:2})fs.push(()=>n);return fs.map(f=>f());}const result=helper();export const observed=result;",
        "function helper():unknown{var {x:n=3,...rest}:{x?:number,y?:number}={y:4};return [n,rest];}const result=helper();export const observed=result;",
        "function helper():unknown{try{throw 1;}catch(e){var e=2;}return e;}const result=helper();export const observed={result,empty:result===undefined};",
        "function helper():string[]{const fs:Array<()=>string>=[];for(var i=0;i<3;i++)fs.push(()=>i.toFixed());return fs.map(f=>f());}const result=helper();export const observed=result;",
        "function helper(n:number):number{type Count=number;var x:Count=n+1;return x;}const result=helper(2);export const observed=result;",
        "function helper(n:number):()=>number{var x=n;return ()=>++x;}const a=helper(1);const b=helper(10);export const observed=[a(),a(),b(),a(),b()];",
        "function helper(n:number):number{var \\u006e:number;n++;return n;}const result=helper(2);export const observed=result;",
    ] {
        let (root, mut engine) = fixture(source, "ts");
        let result =
            yoke::compiler_inline::prepare(&mut engine, &compiler(), &args(false, "ts"), &d())
                .unwrap_or_else(|e| panic!("{source}: {e:#}"));
        let after = planned(&engine, &result, "ts");
        same_execution(source, &after);
        assert_only_inert_type_vars(&after);
        assert_eq!(
            fs::read_to_string(root.path().join("main.ts")).unwrap(),
            source
        );
    }
}
#[test]
fn var_lowering_keeps_jsdoc_nested_scopes_comments_unicode_and_finally() {
    for (source, language) in [
        (
            "/** @param {number} n */\nfunction helper(n){var x=n+1;return x;}const result=helper(2);export const observed=result;",
            "js",
        ),
        (
            "function helper(){/** @type {Array<()=>number>} */ const fs=[];for(var i=0;i<3;i++)fs.push(()=>i);return fs.map(f=>f());}const result=helper();export const observed=result;",
            "js",
        ),
        (
            "function helper(n=1,read=()=>n){var n=2;return [n,read()];}const result=helper();export const observed=result;",
            "js",
        ),
        (
            "function helper():number{function inner(){var x=4;return x;}class Local{static {var x=7;}value(){var x=3;return x;}}var x=inner();return x+new Local().value();}const result=helper();export const observed=result;",
            "ts",
        ),
        (
            "// 🌍\r\nfunction helper(n:number):number{var /* café */ 値 /* lhs */ : number /* equals */ = /* rhs */ n+1, /* comma */ x:number; x=値;return x;}\r\nconst result=helper(2);export const observed=result;\r\n",
            "ts",
        ),
        (
            "function helper():unknown{var xs:number[]=[];for(var i=0;i<2;i++){try{return i;}finally{xs.push(i);continue;}}return [i,xs];}const result=helper();export const observed=result;",
            "ts",
        ),
        (
            "function helper():number{var {x:f=()=>{const {n:v}:{n:number}={n:4};return v;}}:{x?:()=>number}={};return f();}const result=helper();export const observed=result;",
            "ts",
        ),
        (
            "function helper():unknown{var {a:{x:n=3}}:{a:{x?:number}}={a:{}};return n;}const result=helper();export const observed=result;",
            "ts",
        ),
    ] {
        let (root, mut engine) = fixture(source, language);
        let result =
            yoke::compiler_inline::prepare(&mut engine, &compiler(), &args(false, language), &d())
                .unwrap_or_else(|e| panic!("{source}: {e:#}"));
        let after = planned(&engine, &result, language);
        same_execution(source, &after);
        for comment in [
            "/* café */",
            "/* lhs */",
            "/* equals */",
            "/* rhs */",
            "/* comma */",
        ] {
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
fn inferred_var_types_keep_object_array_tuple_union_function_and_nested_loop_closures() {
    for (source, language) in [
        (
            "function helper():number[]{const fs:Array<()=>number>=[];for(var row of [<const>[\"a\",1],<const>[\"bb\",2]])fs.push(()=>row[0].length);return fs.map(f=>f());}const result=helper();export const observed=result;",
            "ts",
        ),
        (
            "// 🌍\r\nfunction helper():()=>number{var {\\u0072ow}={row:{n:3}};return ()=>row.n;}\r\nconst result=helper();export const observed=result();\r\n",
            "ts",
        ),
        (
            "function helper():number[]{const fs:Array<()=>number>=[];for(var row of [{n:1},{n:2}])fs.push(()=>row.n);return fs.map(f=>f());}const result=helper();export const observed=result;",
            "ts",
        ),
        (
            "function helper():number[]{const fs:Array<()=>number>=[];for(var row of [[1],[2]])fs.push(()=>row[0]);return fs.map(f=>f());}const result=helper();export const observed=result;",
            "ts",
        ),
        (
            "function helper():number[]{const fs:Array<()=>number>=[];for(var row of [[\"a\",1] as const,[\"bb\",2] as const])fs.push(()=>row[0].length);return fs.map(f=>f());}const result=helper();export const observed=result;",
            "ts",
        ),
        (
            "function helper():number[]{const fs:Array<()=>number>=[];for(var fn of [(x:number)=>x+1,(x:number)=>x+2])fs.push(()=>fn(3));return fs.map(f=>f());}const result=helper();export const observed=result;",
            "ts",
        ),
        (
            "function helper():number[]{const fs:Array<()=>number>=[];for(var row of [{n:1},null])fs.push(()=>row?.n??0);return fs.map(f=>f());}const result=helper();export const observed=result;",
            "ts",
        ),
        (
            "function helper():number[]{const fs:Array<()=>number>=[];for(var [key,row] of new Map([[1,{n:1}],[2,{n:2}]]))fs.push(()=>row.n+key);return fs.map(f=>f());}const result=helper();export const observed=result;",
            "ts",
        ),
        (
            "function helper():()=>number{var read=()=>row.n;var row={n:3};return read;}const result=helper();export const observed=result();",
            "ts",
        ),
        (
            "function helper(n:number):number[]{const fs:Array<()=>number>=[];outer:for(var row of [{n:1},{n:2}]){if(row.n===n)continue outer;fs.push(()=>row.n);}return fs.map(f=>f());}const result=helper(1);export const observed=result;",
            "ts",
        ),
        (
            "const events:number[]=[];function rows(){events.push(1);return [{n:1},{n:2}];}function helper():number[]{const fs:Array<()=>number>=[];for(var row of rows())fs.push(()=>row.n);return fs.map(f=>f());}const result=helper();export const observed={result,events};",
            "ts",
        ),
        (
            "const events:number[]=[];function helper():()=>number{var read=()=>row.n;var row=(events.push(1),{n:3});return read;}const result=helper();export const observed={value:result(),events};",
            "ts",
        ),
        (
            "function helper():number[]{const fs:Array<()=>number>=[];for(var row={n:1};row.n<3;row.n++)fs.push(()=>row.n);return fs.map(f=>f());}const result=helper();export const observed=result;",
            "ts",
        ),
        (
            "function helper(flag:boolean):()=>number{if(flag)var row={n:1};else row={n:2};return ()=>row.n;}const a=helper(true);const b=helper(false);export const observed=[a(),b()];",
            "ts",
        ),
        (
            "function helper():number[]{const fs:Array<()=>number>=[];for(var a of [[{n:1}]])for(var b of a)fs.push(()=>b.n);return fs.map(f=>f());}const result=helper();export const observed=result;",
            "ts",
        ),
        (
            "function helper():()=>number{var a={n:1};var b={n:2};return ()=>a.n+b.n;}const result=helper();export const observed=result();",
            "ts",
        ),
        (
            "function helper(){/** @type {Array<()=>number>} */const fs=[];for(var row of [{n:1},{n:2}])fs.push(()=>row.n);return fs.map(f=>f()).join(',');}const result=helper();export const observed=result;",
            "js",
        ),
        (
            "function helper(){/** @type {Array<()=>number>} */const fs=[];for(var row of [[1],[2]])fs.push(()=>row[0]);return fs.map(f=>f()).join(',');}const result=helper();export const observed=result;",
            "js",
        ),
        (
            "function helper():()=>number{var {row}={row:{n:3}};return ()=>row.n;}const result=helper();export const observed=result();",
            "ts",
        ),
        (
            "function helper():()=>number{var {row={n:3}}:{row?:{n:number}}={};return ()=>row.n;}const result=helper();export const observed=result();",
            "ts",
        ),
        (
            "function helper():()=>number{var {__proto__}={[\"__proto__\"]:{n:3}};return ()=>__proto__.n;}const result=helper();export const observed=result();",
            "ts",
        ),
        (
            "function helper():()=>number{for(let i=1;i<3;i++)var row={n:i};return ()=>row.n;}const result=helper();export const observed=result();",
            "ts",
        ),
    ] {
        let (root, mut engine) = fixture(source, language);
        let r =
            yoke::compiler_inline::prepare(&mut engine, &compiler(), &args(false, language), &d())
                .unwrap_or_else(|e| panic!("{source}: {e:#}"));
        let after = planned(&engine, &r, language);
        assert!(after.contains("if (void 0 !== void 0)"), "{after}");
        assert!(after.contains("typeof __ci_inline_"), "{after}");
        same_execution(source, &after);
        assert_eq!(
            fs::read_to_string(root.path().join(format!("main.{language}"))).unwrap(),
            source
        );
    }
}
#[test]
fn inert_inference_preserves_enabled_compiler_diagnostics_and_initializer_counts() {
    for source in [
        "const events:number[]=[];function helper(){var row={n:3};events.push(row.n);}helper();export const observed=events;",
        "const events:number[]=[];function helper(){var {key,row}={key:1,row:{n:3}};events.push(row.n+key);}helper();export const observed=events;",
    ] {
        let (root, mut engine) = fixture(source, "ts");
        fs::write(root.path().join("tsconfig.json"),r#"{"compilerOptions":{"strict":true,"target":"ES2022","module":"ESNext","moduleResolution":"Bundler","noEmit":true,"allowUnreachableCode":false,"noUnusedLocals":true,"noUnusedParameters":true,"noFallthroughCasesInSwitch":true},"files":["main.ts"]}"#).unwrap();
        let r = yoke::compiler_inline::prepare(&mut engine, &compiler(), &args(false, "ts"), &d())
            .unwrap_or_else(|e| panic!("{source}: {e:#}"));
        let after = planned(&engine, &r, "ts");
        assert_only_inert_type_vars(&after);
        same_execution(source, &after);
        assert_eq!(
            fs::read_to_string(root.path().join("main.ts")).unwrap(),
            source
        );
    }
}
#[test]
fn strict_completion_preserves_returns_throws_cancellation_and_inferred_result_types() {
    for (language, keep, source) in [
        (
            "ts",
            false,
            "function helper():number{var row={n:3};return row.n;}const result=helper();export const observed=result;",
        ),
        (
            "ts",
            false,
            "function helper():number{throw new Error(\"owned\");}try{helper();}catch(e){}export const observed=1;",
        ),
        (
            "ts",
            false,
            "const opaque=(()=>{return 4;}) as unknown as ()=>never;function helper():number{return opaque();}const result=helper();export const observed=result;",
        ),
        (
            "ts",
            false,
            "function helper():number|undefined{exit:while(true){try{return 1;}finally{break exit;}}}const result=helper();export const observed=result;",
        ),
        (
            "ts",
            false,
            "function helper():number{try{return 1;}finally{return 2;}}const result=helper();export const observed=result;",
        ),
        (
            "ts",
            false,
            "const events:number[]=[];function helper(){var row={n:3};events.push(row.n);}helper();export const observed=events;",
        ),
        (
            "ts",
            false,
            "function helper(n:number){return {value:n};}const result=helper(3);export const observed=result.value;",
        ),
        (
            "ts",
            false,
            "function helper(){return ()=>3;}const result=helper();export const observed=result();",
        ),
        (
            "js",
            false,
            "/** @param {number} n */\nfunction helper(n){return {value:n};}const result=helper(3);export const observed=result.value;",
        ),
        (
            "js",
            false,
            "/** @returns {number|string} */\nfunction helper(){return 3;}const result=helper();export const observed=result;",
        ),
        (
            "ts",
            false,
            "function helper(){class Local{n=3;}return new Local();}const result=helper();export const observed=result.n;",
        ),
        (
            "ts",
            false,
            "function helper(){type Local={n:number};const row:Local={n:3};return row;}const result=helper();export const observed=result.n;",
        ),
        (
            "ts",
            false,
            "function stop():never{throw new Error('owned');}function helper():number{return stop();}const events:string[]=[];try{helper();}catch(e){events.push((e as Error).message);}export const observed=events;",
        ),
        (
            "ts",
            false,
            "function helper():never{throw new Error('owned');}const events:string[]=[];try{helper();}catch(e){events.push((e as Error).message);}export const observed=events;",
        ),
        (
            "ts",
            false,
            "const events:number[]=[];function helper():number|undefined{for(let n=0;n<2;n++){try{return n;}finally{events.push(n);continue;}}}const result=helper();export const observed={result,empty:result===undefined,events};",
        ),
        (
            "js",
            false,
            "/** first doc */\n/* between */\n/** @param {number} n @returns {{value:number}} */\nfunction helper(n){return {value:n};}const result=helper(3);export const observed=result.value;",
        ),
        (
            "js",
            true,
            "/** @param {number} n */\nexport function helper(n){return {value:n};}const result=helper(3);export const observed=result.value;",
        ),
        (
            "js",
            false,
            "/** @returns {number} */\nfunction helper(){throw new Error('owned');}const events=[];try{helper();}catch(e){if(e instanceof Error)events.push(e.message);else throw e;}export const observed=events;",
        ),
        (
            "ts",
            false,
            "const events:number[]=[];function helper(){function nested(){return 3;}events.push(nested());}helper();export const observed=events;",
        ),
        (
            "ts",
            false,
            "// 🌍\r\nfunction helper(){var 値={n:3};return /* café */ 値;}\r\nconst result=helper();export const observed=result.n;\r\n",
        ),
        (
            "js",
            false,
            "export const marker=1;/** @returns {string|number} */ function helper(){return {value:3};}const result=helper();export const observed={marker,result};",
        ),
        (
            "js",
            true,
            "export const marker=1;/** @returns {string|number} */ export function helper(){return {value:3};}const result=helper();export const observed={marker,result};",
        ),
        (
            "js",
            false,
            "export const marker=1;/* multiline\n */ /** @returns {string|number} */ function helper(){return {value:3};}const result=helper();export const observed={marker,result};",
        ),
        (
            "js",
            false,
            "export const marker=1;\u{2028}/** @returns {string|number} */ function helper(){return {value:3};}const result=helper();export const observed={marker,result};",
        ),
        (
            "js",
            false,
            "\u{feff}/** @returns {{n:number}} */ function helper(){return {n:3};}const result=helper();export const observed=result.n;",
        ),
        (
            "ts",
            false,
            "function helper(){return 3 as const;}const result=helper();export const observed=result+1;",
        ),
    ] {
        let (root, mut engine) = fixture(source, language);
        let config = json!({"compilerOptions":{"strict":true,"target":"ES2022","module":"ESNext","moduleResolution":"Bundler","noEmit":true,"allowUnreachableCode":false,"allowUnusedLabels":false,"noUnusedLocals":true,"noUnusedParameters":true,"noFallthroughCasesInSwitch":true,"allowJs":true,"checkJs":true},"files":[format!("main.{language}")]});
        fs::write(
            root.path().join("tsconfig.json"),
            serde_json::to_vec(&config).unwrap(),
        )
        .unwrap();
        let r =
            yoke::compiler_inline::prepare(&mut engine, &compiler(), &args(keep, language), &d())
                .unwrap_or_else(|e| panic!("{source}: {e:#}"));
        let after = planned(&engine, &r, language);
        assert!(after.contains("if (void 0 === void 0)"), "{after}");
        assert_eq!(r["sourceExecuted"], false);
        same_execution(source, &after);
        assert_eq!(
            fs::read_to_string(root.path().join(format!("main.{language}"))).unwrap(),
            source
        );
        assert_eq!(
            fs::read(root.path().join("tsconfig.json")).unwrap(),
            serde_json::to_vec(&config).unwrap()
        );
    }
}
#[test]
fn jsdoc_call_types_and_removed_comments_preserve_templates_await_yield_and_neighbors() {
    for (await_result, source) in [
        (
            false,
            "let input=3;\n/** @template T\n * @param {T} n\n * @returns {T} */\nfunction helper(n){return n;}const result=helper(input);export const observed=result+1;",
        ),
        (
            false,
            "let input={n:3};\n/** @template T\n * @param {T} n\n * @returns {T} */\nfunction helper(n){return n;}const result=helper(input);export const observed=result.n;",
        ),
        (
            false,
            "/** @template T\n * @param {T} n\n * @returns {T} */\nfunction helper(n){return n;}const result=helper(3);export const observed=result+1;",
        ),
        (
            false,
            "/** @type {string|null} */ let value=\"owned\";\n/** @template T\n * @param {T} n\n * @param {null} later\n * @returns {T} */\nfunction helper(n,later){void later;return n;}const result=helper(value,(value=null));export const observed=result.length;",
        ),
        (
            true,
            "/** @template T\n * @param {T} n\n * @returns {T} */\nfunction helper(n){return n;}export async function outer(){const result=helper(await Promise.resolve(3));return result+1;}export const observed=outer();",
        ),
        (
            false,
            "/** @template T\n * @param {T} n\n * @returns {T} */\nfunction helper(n){return n;}\n/** @returns {Generator<number,number,number>} */\nexport function* outer(){const result=helper(yield 3);return result+1;}const iterator=outer();export const observed=[iterator.next(),iterator.next(4)];",
        ),
        (
            false,
            "/** @returns {number} */\nfunction helper(){return 3;}export function other(){return {n:1};}const result=helper();export const observed=[result,other().n];",
        ),
        (
            false,
            "/** @typedef {{n:number}} Row */\nfunction helper(){return 3;}\n/** @type {Row} */ const other={n:4};const result=helper();export const observed=result+other.n;",
        ),
        (
            false,
            "/** @typedef {{n:number}} Row */\n/** @returns {Row} */\nfunction helper(){return {n:3};}\n/** @type {Row} */ const other={n:4};const result=helper();export const observed=result.n+other.n;",
        ),
        (
            false,
            "/** @template T\n * @param {T} n\n * @returns {T} */\nfunction helper(n){return n;}let result=helper(3);export const observed=result+1;",
        ),
        (
            false,
            "/** @template T\n * @param {T} n\n * @returns {T} */\nfunction helper(n){return n;}export function outer(){return helper(3);}export const observed=outer();",
        ),
        (
            false,
            "/** @template T\n * @param {T} n\n * @returns {T} */\nfunction helper(n){return n;}\n/** @returns {3} */\nexport function outer(){return helper(3);}export const observed=outer();",
        ),
    ] {
        let (root, mut engine) = fixture(source, "js");
        let p = root.path().join("tsconfig.json");
        let mut config: Value = serde_json::from_slice(&fs::read(&p).unwrap()).unwrap();
        for (key, value) in [
            ("allowUnreachableCode", false),
            ("allowUnusedLabels", false),
            ("noUnusedLocals", true),
            ("noUnusedParameters", true),
        ] {
            config["compilerOptions"][key] = json!(value);
        }
        fs::write(&p, serde_json::to_vec(&config).unwrap()).unwrap();
        let r = yoke::compiler_inline::prepare(&mut engine, &compiler(), &args(false, "js"), &d())
            .unwrap_or_else(|e| panic!("{source}: {e:#}"));
        let after = planned(&engine, &r, "js");
        assert!(!after.contains("function helper"), "{after}");
        assert_eq!(r["sourceExecuted"], false);
        same_execution_mode(source, &after, await_result);
        assert_eq!(
            fs::read_to_string(root.path().join("main.js")).unwrap(),
            source
        );
        assert_eq!(fs::read(&p).unwrap(), serde_json::to_vec(&config).unwrap());
    }
    let source = "/** @returns {import('./types').Row} */\nfunction helper(){return {n:3};}const result=helper();export const observed=result.n;";
    let (root, mut engine) = fixture(source, "js");
    let types = "export interface Row { n:number }\n";
    fs::write(root.path().join("types.ts"), types).unwrap();
    let r =
        yoke::compiler_inline::prepare(&mut engine, &compiler(), &args(false, "js"), &d()).unwrap();
    let after = planned(&engine, &r, "js");
    same_execution(source, &after);
    assert_eq!(
        fs::read_to_string(root.path().join("main.js")).unwrap(),
        source
    );
    assert_eq!(
        fs::read_to_string(root.path().join("types.ts")).unwrap(),
        types
    );
    let source = "/** @typedef {{n:number,left?:number}} Row */\nconst outside={n:3};\n/** @returns {Row} */\nfunction helper(){return outside;}export function outer(){\n/** @typedef {{n:number,right?:number}} Row */ const marker=0;void marker;\n/** @type {Row} */ const local={n:4};void local;const result=helper();return typeof result;}export const observed=outer();";
    let (root, mut engine) = fixture(source, "js");
    let error = yoke::compiler_inline::prepare(&mut engine, &compiler(), &args(false, "js"), &d())
        .unwrap_err();
    assert!(
        format!("{error:#}").contains("Body documentation free binding would change at consumer"),
        "{error:#}"
    );
    unpublished(&engine);
    assert_eq!(
        fs::read_to_string(root.path().join("main.js")).unwrap(),
        source
    );
}
#[test]
fn explicit_return_contexts_and_anonymous_return_names_retain_types_and_owned_execution() {
    for (language, keep, source) in [
        (
            "ts",
            false,
            "function helper(): {run:(n:number)=>number} {return {run:n=>n+1};}const result=helper();export const observed=result.run(3);",
        ),
        (
            "ts",
            false,
            "function helper(): (n:number)=>number {return n=>n+1;}const result=helper();export const observed=[result(3),result.name];",
        ),
        (
            "ts",
            false,
            "function helper(): {run(n:number):number} {return {run(n){return n+2;}};}const result=helper();export const observed=result.run(3);",
        ),
        (
            "ts",
            false,
            "function helper(): [(n:number)=>number,string] {return [n=>n+1,\"owned\"];}const result=helper();export const observed=[result[0](3),result[1]];",
        ),
        (
            "ts",
            false,
            "type Result={kind:\"owned\",run:(n:number)=>number};function helper():Result {return {kind:\"owned\",run:n=>n+1};}const result=helper();export const observed=result.run(3);",
        ),
        (
            "ts",
            false,
            "function helper(){return function(){};}const result=helper();export const observed=result.name;",
        ),
        (
            "ts",
            false,
            "function helper(){return ()=>3;}const result=helper();export const observed=[result.name,result()];",
        ),
        (
            "ts",
            false,
            "function helper(){return class{};}const result=helper();export const observed=result.name;",
        ),
        (
            "js",
            false,
            "function helper(){return function(){};}const result=helper();export const observed=result.name;",
        ),
        (
            "js",
            false,
            "function helper(){return ()=>3;}const result=helper();export const observed=[result.name,result()];",
        ),
        (
            "js",
            false,
            "function helper(){return class{};}const result=helper();export const observed=result.name;",
        ),
        (
            "ts",
            false,
            "function helper(): (n:number)=>number {try{return n=>n+1;}finally{return n=>n+2;}}const result=helper();export const observed=[result.name,result(3)];",
        ),
        (
            "ts",
            false,
            "function helper():()=>void{return ()=>3;}const result=helper();export const observed=[result.name,result()];",
        ),
        (
            "ts",
            false,
            "function helper():{value:number}|undefined {exit:while(true){try{return {value:3};}finally{break exit;}}}const result=helper();export const observed=result===undefined;",
        ),
        (
            "ts",
            false,
            "function helper():((n:number)=>number)|undefined {exit:while(true){try{return n=>n+1;}finally{break exit;}}}const result=helper();export const observed=result===undefined;",
        ),
        (
            "ts",
            false,
            "function helper(): (n:number)=>number {return function owned(n){return n+1;};}const result=helper();export const observed=[result.name,result(3)];",
        ),
        (
            "ts",
            false,
            "// 🌍\r\nfunction helper(): readonly [(n:number)=>number,\"é\"] {return [n=>n+1,\"é\"];}\r\nconst result=helper();export const observed=[result[0](3),result[1]];\r\n",
        ),
        (
            "ts",
            true,
            "export function helper(): {run:(n:number)=>number} {return {run:n=>n+1};}const result=helper();export const observed=result.run(3);",
        ),
        (
            "ts",
            false,
            "const result=helper();function helper(): {run:(n:number)=>number} {return {run:n=>n+1};}export const observed=result.run(3);",
        ),
        (
            "ts",
            false,
            "function helper(flag:boolean):(n:number)=>number {if(flag)return n=>n+1;return function(n){return n+2;};}const a=helper(true);const b=helper(false);export const observed=[a.name,b.name,a(3),b(3)];",
        ),
        (
            "ts",
            false,
            "const opaque=(()=>{return 4;}) as unknown as ()=>never;function helper():number{return opaque();}const result=helper();export const observed=result;",
        ),
        (
            "ts",
            false,
            "const opaque=(()=>{return 4;}) as unknown as ()=>never;function helper():never{return opaque();}const result=helper();export const observed=result;",
        ),
        (
            "ts",
            false,
            "const opaque=(()=>{return 4;}) as unknown as ()=>never;function helper():void{return opaque();}const result=helper();export const observed=result;",
        ),
        (
            "ts",
            false,
            "const opaque=(()=>{return 4;}) as unknown as ()=>never;function helper(){return opaque();}const result=helper();export const observed=result;",
        ),
        (
            "ts",
            false,
            "function helper(){return (((function(){})));}const result=helper();export const observed=result.name;",
        ),
        (
            "ts",
            false,
            "function helper(){return ((()=>3) as (()=>number));}const result=helper();export const observed=result.name;",
        ),
        (
            "ts",
            false,
            "function helper(){return <(()=>number)>(()=>3);}const result=helper();export const observed=result.name;",
        ),
        (
            "ts",
            false,
            "function helper(){return ((()=>3) satisfies (()=>number));}const result=helper();export const observed=result.name;",
        ),
        (
            "ts",
            false,
            "function helper(){return (function(){})!;}const result=helper();export const observed=result.name;",
        ),
        (
            "ts",
            false,
            "function helper(){return (<T>(n:T)=>n)<number>;}const result=helper();export const observed=result.name;",
        ),
        (
            "js",
            false,
            "function helper(){return /** @type {()=>number} */ (()=>3);}const result=helper();export const observed=[result.name,result()];",
        ),
    ] {
        let (root, mut engine) = fixture(source, language);
        let p = root.path().join("tsconfig.json");
        let mut config: Value = serde_json::from_slice(&fs::read(&p).unwrap()).unwrap();
        for option in ["allowUnreachableCode", "allowUnusedLabels"] {
            config["compilerOptions"][option] = json!(false);
        }
        for option in [
            "noUnusedLocals",
            "noUnusedParameters",
            "noFallthroughCasesInSwitch",
        ] {
            config["compilerOptions"][option] = json!(true);
        }
        fs::write(p, serde_json::to_vec(&config).unwrap()).unwrap();
        let r =
            yoke::compiler_inline::prepare(&mut engine, &compiler(), &args(keep, language), &d())
                .unwrap_or_else(|e| panic!("{source}: {e:#}"));
        let after = planned(&engine, &r, language);
        same_execution(source, &after);
        assert_eq!(r["sourceExecuted"], false);
        assert_eq!(r["verificationRequired"], true);
        assert_eq!(
            fs::read_to_string(root.path().join(format!("main.{language}"))).unwrap(),
            source
        );
    }
}
#[test]
fn body_free_references_keep_separate_type_value_and_namespace_bindings() {
    for source in [
        "type Result={run:(n:number)=>number};function helper(n:number):Result{return {run:value=>value+n};}export function outer(Result:number){const result=helper(2);return result.run(Result);}export const observed=outer(3);",
        "const BASE=2;function helper(n:number):number{return n+BASE;}export function outer(){type BASE={unused:true};const extra:BASE={unused:true};const result=helper(3);return [result,extra.unused];}export const observed=outer();",
        "namespace Types{export type Result={value:number};}function helper(n:number):Types.Result{return {value:n};}export function outer(Types:number){const result=helper(2);return result.value+Types;}export const observed=outer(3);",
        "const Witness:number=3;function helper(n:typeof Witness):typeof Witness{return n;}export function outer(){type Witness={unused:true};const extra:Witness={unused:true};const result=helper(3);return [result,extra.unused];}export const observed=outer();",
        "const BASE=2;function helper():{BASE:number}{return {BASE};}export function outer(){type BASE={unused:true};const extra:BASE={unused:true};const result=helper();return [result.BASE,extra.unused];}export const observed=outer();",
        "// 🧪\r\ntype Résultat={value:number};\r\nfunction helper(n:number):Résultat{return {value:n};}\r\nexport function outer(Résultat:number){const result=helper(2);return result.value+Résultat;}\r\nexport const observed=outer(3);\r\n",
    ] {
        for epoch in [AnalyzerEpoch::V8, AnalyzerEpoch::V9] {
            for keep in [false, true] {
                let (root, mut engine) = fixture(source, "ts");
                engine.capture_epoch = epoch;
                let r = yoke::compiler_inline::prepare(
                    &mut engine,
                    &compiler(),
                    &args(keep, "ts"),
                    &d(),
                )
                .unwrap_or_else(|e| panic!("{source}: {e:#}"));
                let after = planned(&engine, &r, "ts");
                assert_eq!(after.contains("function helper"), keep, "{after}");
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
}
#[test]
fn body_namespace_classification_still_refuses_actual_lexical_shadows() {
    for source in [
        "const BASE=2;function helper(n:number):number{return n+BASE;}export function outer(BASE:number){const result=helper(3);return result;}export const observed=outer(4);",
        "const Witness:number=3;function helper(n:typeof Witness):typeof Witness{return n;}export function outer(Witness:number){const value=helper(3);return value+Witness;}export const observed=outer(2);",
        "namespace Types{export type Result={value:number};}function helper(n:number):Types.Result{return {value:n};}namespace Consumer{namespace Types{export type Result={value:string};}export function outer(){const extra:Types.Result={value:\"x\"};const result=helper(2);return [result.value,extra.value];}}export const observed=Consumer.outer();",
        "const BASE=2;function helper():{BASE:number}{return {BASE};}export function outer(BASE:number){const result=helper();return result.BASE;}export const observed=outer(4);",
    ] {
        let (root, mut engine) = fixture(source, "ts");
        let error =
            yoke::compiler_inline::prepare(&mut engine, &compiler(), &args(false, "ts"), &d())
                .unwrap_err();
        assert!(
            format!("{error:#}").contains("Body free binding would change"),
            "{source}: {error:#}"
        );
        unpublished(&engine);
        assert_eq!(
            fs::read_to_string(root.path().join("main.ts")).unwrap(),
            source
        );
    }
}
#[test]
fn return_annotations_cannot_capture_consumer_type_shadows() {
    let source = "type Result={run:(n:number)=>number};function helper():Result{return {run:n=>n+1};}export function outer(){type Result={value:number};const unused:Result={value:3};const result=helper();return result.run(unused.value);}";
    let (root, mut engine) = fixture(source, "ts");
    let error = yoke::compiler_inline::prepare(&mut engine, &compiler(), &args(false, "ts"), &d())
        .unwrap_err();
    assert!(
        format!("{error:#}").contains("Body free binding would change"),
        "{error:#}"
    );
    unpublished(&engine);
    assert_eq!(
        fs::read_to_string(root.path().join("main.ts")).unwrap(),
        source
    );
}
#[test]
fn argument_capture_keeps_contextual_types_generic_literals_names_defaults_and_hostile_iterators() {
    for (language, source) in [
        (
            "ts",
            "function helper(fn:(n:number)=>number):number{return fn(3);}const result=helper(n=>n+1);export const observed=result;",
        ),
        (
            "ts",
            "function helper(fn:(n:number)=>number, later=(0)):number{return fn(3)+later;}const result=helper(function(n){return n+1;});export const observed=result;",
        ),
        (
            "ts",
            "function helper(value:{run:(n:number)=>number}):number{return value.run(3);}const result=helper({run:n=>n+1});export const observed=result;",
        ),
        (
            "ts",
            "function helper(values: [(n:number)=>number]):number{return values[0](3);}const result=helper([n=>n+1]);export const observed=result;",
        ),
        (
            "ts",
            "function helper(...values:Array<(n:number)=>number>):number{return values[0](3);}const result=helper(n=>n+1);export const observed=result;",
        ),
        (
            "js",
            "/** @param {(n:number)=>number} fn */\nfunction helper(fn){return fn(3);}const result=helper(n=>n+1);export const observed=result;",
        ),
        (
            "ts",
            "function helper(fn:()=>number):string{return fn.name;}const result=helper(()=>3);export const observed=result;",
        ),
        (
            "ts",
            "function helper(fn:()=>number):string{return fn.name;}const result=helper(function(){return 3;});export const observed=result;",
        ),
        (
            "ts",
            "function helper(fn:new()=>object):string{return fn.name;}const result=helper(class{});export const observed=result;",
        ),
        (
            "ts",
            "const opaque=(()=>{return 4;}) as unknown as ()=>never;function helper(n:number):number{return n;}const result=helper(opaque());export const observed=result;",
        ),
        (
            "js",
            "/** @template T\n * @param {T} n\n * @returns {T} */\nfunction helper(n){return n;}const result=helper({n:3});export const observed=result.n;",
        ),
        (
            "js",
            "/** @template T\n * @param {T} n\n * @returns {T} */\nfunction helper(n){return n;}const result=helper(()=>3);export const observed=[result.name,result()];",
        ),
        (
            "ts",
            "function helper(n:[number,string]):number{return n[0]+n[1].length;}const result=helper([3,\"own\"]);export const observed=result;",
        ),
        (
            "ts",
            "const events:string[]=[];const previous=Array.prototype[Symbol.iterator];Array.prototype[Symbol.iterator]=(function*(){events.push(\"iterator\");yield ()=>99;}) as unknown as typeof previous;function helper(fn:()=>number):number{return fn();}const result=helper(()=>3);export const observed={result,events};",
        ),
        (
            "ts",
            "const events:string[]=[];function helper(n=(events.push(\"default\"),3)):number{return n;}const result=helper((events.push(\"arg\"),undefined));export const observed={result,events};",
        ),
        (
            "js",
            "/** @template T\n * @param {T} n\n * @returns {T} */\nfunction helper(n){return n;}const result=helper(3);export const observed=result+1;",
        ),
        (
            "ts",
            "function helper(n:number):number{var x=n+1;return x;}const result=helper(2);export const observed=result;",
        ),
        (
            "ts",
            "function helper(n:number):number{var n:number;n++;return n;}const result=helper(2);export const observed=result;",
        ),
        (
            "js",
            "/** @param {...number} ns */\nfunction helper(...ns){return ns.join(\",\");}const result=helper(1,2);export const observed=result;",
        ),
        (
            "js",
            "/** @param {{run:(n:number)=>number}} options */\nfunction helper({run}){return run(3);}const result=helper({run:n=>n+1});export const observed=result;",
        ),
    ] {
        let (root, mut engine) = fixture(source, language);
        let path = root.path().join("tsconfig.json");
        let mut config: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        for option in ["allowUnreachableCode", "allowUnusedLabels"] {
            config["compilerOptions"][option] = json!(false);
        }
        for option in [
            "noUnusedLocals",
            "noUnusedParameters",
            "noFallthroughCasesInSwitch",
        ] {
            config["compilerOptions"][option] = json!(true);
        }
        fs::write(path, serde_json::to_vec(&config).unwrap()).unwrap();
        let r =
            yoke::compiler_inline::prepare(&mut engine, &compiler(), &args(false, language), &d())
                .unwrap_or_else(|e| panic!("{source}: {e:#}"));
        let after = planned(&engine, &r, language);
        same_execution(source, &after);
        assert_eq!(r["sourceExecuted"], false);
        assert_eq!(r["verificationRequired"], true);
        assert_eq!(
            fs::read_to_string(root.path().join(format!("main.{language}"))).unwrap(),
            source
        );
    }
}
#[test]
fn caller_documentation_keeps_its_declaration_context_typedef_identity_and_exact_bytes() {
    for (language, source) in [
        (
            "js",
            "/** @template T\n * @returns {T} */\nfunction make(){return /** @type {T} */ (/** @type {unknown} */ (3));}\n/** @template T\n * @param {T} n\n * @returns {T} */\nfunction helper(n){return n;}\n/** @type {3} */ const result=helper(make());export const observed=result+1;",
        ),
        (
            "js",
            "function helper(){return 3;}\n/** @type {number} */ const result=helper();export const observed=result;",
        ),
        (
            "js",
            "/** @template T\n * @param {T} n\n * @returns {T} */\nfunction helper(n){return n;}\n/** @type {3} */ export const result=helper(3);export const observed=result+1;",
        ),
        (
            "js",
            "/** @template T\n * @param {T} n\n * @returns {T} */\nfunction helper(n){return n;}\n/** @typedef {3} Result\n * @type {Result} */ const result=helper(3);export const observed=result+1;",
        ),
        (
            "js",
            "/** @template T\n * @param {T} n\n * @returns {T} */\nfunction helper(n){return n;}\n/** @typedef {3} Result */\n/** @type {Result} */ const result=helper(3);/** @type {Result} */ const other=3;export const observed=result+other;",
        ),
        (
            "js",
            "/** @typedef {string} Result */\n/** @template T\n * @param {T} n\n * @returns {T} */\nfunction helper(n){return n;}\nfunction caller(){\n/** @typedef {3} Result */\n/** @type {Result} */ const result=helper(3);return result;}export const observed=caller()+1;",
        ),
        (
            "js",
            "/** @template T\n * @param {T} n\n * @returns {T} */\nfunction helper(n){return n;}\n/** annotation */\n// retained\n/** @type {3} */ /* retained block */ const result=helper(3);export const observed=result+1;",
        ),
        (
            "js",
            "// \u{03c0}\u{1f30d}\r\n/** @template T\r\n * @returns {T} */\r\nfunction make(){return /** @type {T} */ (/** @type {unknown} */ (3));}\r\n/** @template T\r\n * @param {T} n\r\n * @returns {T} */\r\nfunction helper(n){return n;}\r\n/** @type {3} */ const result=helper(make());export const observed=result+1;",
        ),
        (
            "js",
            "/** @template T\n * @param {T} n\n * @returns {T} */\nfunction helper(n){return n;}\nconst before=3;/** @type {3} */ const result=helper(before);export const observed=result+1;",
        ),
        (
            "js",
            "\u{feff}/** @template T\n * @param {T} n\n * @returns {T} */\nfunction helper(n){return n;}\n/** @type {3} */ const result=helper(3);export const observed=result+1;",
        ),
        (
            "js",
            "/** @template T\n * @param {T} n\n * @returns {T} */\nfunction helper(n){return n;}\n/** @type {3} */ const first=helper(3);\n/** @type {4} */ const result=helper(4);export const observed=first+result;",
        ),
        (
            "ts",
            "function helper(n:number):number{return n;}\n/** @type {number} */ const result=helper(3);export const observed=result;",
        ),
        (
            "js",
            "function helper(){return 3;}/** @type {3} */ const result=helper();export const observed=result;",
        ),
    ] {
        let (root, mut engine) = fixture(source, language);
        let path = root.path().join("tsconfig.json");
        let mut config: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        for option in ["allowUnreachableCode", "allowUnusedLabels"] {
            config["compilerOptions"][option] = json!(false);
        }
        for option in ["noUnusedLocals", "noUnusedParameters"] {
            config["compilerOptions"][option] = json!(true);
        }
        fs::write(path, serde_json::to_vec(&config).unwrap()).unwrap();
        let r =
            yoke::compiler_inline::prepare(&mut engine, &compiler(), &args(false, language), &d())
                .unwrap_or_else(|e| panic!("{source}: {e:#}"));
        let after = planned(&engine, &r, language);
        same_execution(source, &after);
        assert_eq!(r["sourceExecuted"], false);
        assert_eq!(r["verificationRequired"], true);
        assert_eq!(
            after.matches("@typedef").count(),
            source.matches("@typedef").count()
        );
        assert_eq!(
            after.matches("@type {3}").count(),
            source.matches("@type {3}").count()
        );
        assert_eq!(
            fs::read_to_string(root.path().join(format!("main.{language}"))).unwrap(),
            source
        );
    }
}
#[test]
fn unsafe_or_unowned_body_forms_refuse_before_any_plan_or_derived_source() {
    for source in [
        "type Const={n:number};function helper():unknown{return {} as Const;}function caller(){type Const={m:string};return helper();}export const observed=caller();",
        "function helper():number{var f;function f(){return 4;}return f();}const result=helper();export const observed=result;",
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
fn wrapped_eval_in_the_body_or_caller_cannot_publish_a_plan() {
    for expression in [
        "(eval)('typeof helper')",
        "eval!('typeof helper')",
        "(eval as (s:string)=>any)('typeof helper')",
        "(<any>eval)('typeof helper')",
        "(eval satisfies Function)('typeof helper')",
        r"(\u0065val)('typeof helper')",
    ] {
        let source = format!(
            "function helper(n:number){{return {expression};}}export const observed=helper(2);"
        );
        let (root, mut engine) = fixture(&source, "ts");
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
    for expression in [
        "(eval)('helper = n => 99')",
        "(eval as any)('helper = n => 99')",
    ] {
        let source = format!(
            "function helper(n:number){{return n+1;}}{expression};export const observed=helper(2);"
        );
        let (root, mut engine) = fixture(&source, "ts");
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
    let source = "function helper(n:number){return n+1;}(eval<number>)('helper = n => 99');export const observed=helper(2);";
    let (root, mut engine) = fixture(source, "ts");
    let ambient = "declare function eval<T>(source:string):T;";
    fs::write(root.path().join("eval.d.ts"), ambient).unwrap();
    fs::write(root.path().join("tsconfig.json"),r#"{"compilerOptions":{"strict":true,"target":"ES2022","module":"ESNext","noEmit":true},"files":["main.ts","eval.d.ts"]}"#).unwrap();
    let error = yoke::compiler_inline::prepare(&mut engine, &compiler(), &args(false, "ts"), &d())
        .unwrap_err();
    assert!(
        format!("{error:#}").contains("Body context has dynamic eval authority"),
        "{error:#}"
    );
    unpublished(&engine);
    assert_eq!(
        fs::read_to_string(root.path().join("main.ts")).unwrap(),
        source
    );
    assert_eq!(
        fs::read_to_string(root.path().join("eval.d.ts")).unwrap(),
        ambient
    );
}
#[test]
fn body_cli_plans_require_independent_verification_approval_and_foreign_preserving_recovery() {
    for (source, keep, strict, language) in [
        (
            "function helper(n:number):boolean{return n<3;}const values:number[]=[];let n=0;do{values.push(n);if(++n>6)break;}while(helper(n));export const observed=values;\n",
            false,
            true,
            "ts",
        ),
        (
            "function helper(n:number):boolean{return n<3;}const values:number[]=[];for(let n=0;helper(n);n++){values.push(n);if(n>6)break;}export const observed=values;\n",
            false,
            true,
            "ts",
        ),
        (
            "function helper(n:number):boolean{return n<3;}let n=0;while(helper(n++)){if(n>6)break;}export const observed=n;\n",
            false,
            true,
            "ts",
        ),
        (
            "function helper(){return 3;}/** @type {3} */ const result=helper();export const observed=result;",
            false,
            true,
            "js",
        ),
        (
            "/** @template T\n * @returns {T} */\nfunction make(){return /** @type {T} */ (/** @type {unknown} */ (3));}\n/** @template T\n * @param {T} n\n * @returns {T} */\nfunction helper(n){return n;}\n/** @type {3} */ const result=helper(make());export const observed=result+1;",
            false,
            true,
            "js",
        ),
        (
            "function helper(fn:(n:number)=>number):number{return fn(3);}const result=helper(n=>n+1);export const observed=result;",
            false,
            true,
            "ts",
        ),
        (
            "/** @template T\n * @param {T} n\n * @returns {T} */\nfunction helper(n){return n;}const result=helper(()=>3);export const observed=[result.name,result()];",
            false,
            true,
            "js",
        ),
        (
            "const events:string[]=[];const previous=Array.prototype[Symbol.iterator];Array.prototype[Symbol.iterator]=(function*(){events.push(\"iterator\");yield ()=>99;}) as unknown as typeof previous;function helper(fn:()=>number):number{return fn();}const result=helper(()=>3);export const observed={result,events};",
            false,
            true,
            "ts",
        ),
        (
            "function helper(): {run:(n:number)=>number} {return {run:n=>n+1};}const result=helper();export const observed=result.run(3);",
            false,
            true,
            "ts",
        ),
        (
            "function helper(): (n:number)=>number {return n=>n+1;}const result=helper();export const observed=[result(3),result.name];",
            false,
            true,
            "ts",
        ),
        (
            "function helper(){return class{};}const result=helper();export const observed=result.name;",
            false,
            true,
            "js",
        ),
        (
            "let input={n:3};\n/** @template T\n * @param {T} n\n * @returns {T} */\nfunction helper(n){return n;}const result=helper(input);export const observed=result.n;",
            false,
            true,
            "js",
        ),
        (
            "/** @returns {number} */\nfunction helper(){return 3;}export function other(){return {n:1};}const result=helper();export const observed=[result,other().n];",
            false,
            true,
            "js",
        ),
        (
            "function helper(){var row={n:3};return row;}export const observed=helper();\n",
            false,
            true,
            "ts",
        ),
        (
            "function helper():number|undefined{exit:while(true){try{return 1;}finally{break exit;}}}export const observed=helper();\n",
            false,
            true,
            "ts",
        ),
        (
            "function helper():number[]{const fs:Array<()=>number>=[];for(var row of [{n:1},{n:2}])fs.push(()=>row.n);return fs.map(f=>f());}export const observed=helper();\n",
            false,
            false,
            "ts",
        ),
        (
            "function helper(n=2):number{try{return n+1;}finally{n++;}}export const observed=helper();\n",
            false,
            false,
            "ts",
        ),
        (
            "export function helper({n=2}:{n?:number}={}):number{return n+1;}export const observed=helper();\n",
            true,
            false,
            "ts",
        ),
        (
            "function helper(n=1,read=()=>n):number[]{var n=2;return [n,read()];}export const observed=helper();\n",
            false,
            false,
            "ts",
        ),
        (
            "function helper():number[]{const fs:Array<()=>number>=[];for(var i=0;i<3;i++)fs.push(()=>i);return fs.map(f=>f());}export const observed=helper();\n",
            false,
            false,
            "ts",
        ),
    ] {
        let (root, mut engine) = fixture(source, language);
        if strict {
            let p = root.path().join("tsconfig.json");
            let mut config: Value = serde_json::from_slice(&fs::read(&p).unwrap()).unwrap();
            config["compilerOptions"]["allowUnreachableCode"] = json!(false);
            config["compilerOptions"]["allowUnusedLabels"] = json!(false);
            config["compilerOptions"]["noUnusedLocals"] = json!(true);
            config["compilerOptions"]["noUnusedParameters"] = json!(true);
            fs::write(p, serde_json::to_vec(&config).unwrap()).unwrap();
        }
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
        fs::write(host.path().join("requests.json"),serde_json::to_vec(&json!([{"operation":"refactor.prepare","args":{"operation":"inline","arguments":args(keep,language)}}])).unwrap()).unwrap();
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
        let expected = fs::read(root.path().join(format!("main.{language}"))).unwrap();
        same_execution(source, std::str::from_utf8(&expected).unwrap());
        assert_ne!(expected, source.as_bytes());
        let receipt = applied["data"]["receipt"]["id"].as_str().unwrap();
        fs::write(
            root.path().join(format!("main.{language}")),
            "// foreign writer\n",
        )
        .unwrap();
        assert!(yoke::changes::recover(&engine.store, &engine.root, receipt, true).is_err());
        assert_eq!(
            fs::read_to_string(root.path().join(format!("main.{language}"))).unwrap(),
            "// foreign writer\n"
        );
        fs::write(root.path().join(format!("main.{language}")), expected).unwrap();
        yoke::changes::recover(&engine.store, &engine.root, receipt, true).unwrap();
        assert_eq!(
            fs::read_to_string(root.path().join(format!("main.{language}"))).unwrap(),
            source
        );
    }
}

#[test]
fn once_only_statement_heads_keep_control_effects_finally_and_unicode_bytes() {
    for source in [
        "const events:string[]=[];function helper(n:number){events.push('body:'+n);try{return n>0;}finally{events.push('finally');}}export function run(){outer:for(const n of [0,1,2])if((helper((events.push('arg:'+n),n)))){events.push('yes');continue outer;}else events.push('no');return events;}export const observed=run(); // café 🦀\n",
        "const events:string[]=[];function helper(n:number){events.push('body:'+n);return n*2;}function key(){events.push('case');return 2;}export function run(){outer:for(const n of [0,1,2])switch((helper((events.push('arg:'+n),n)))){case key():events.push('two');continue outer;case 0:events.push('zero');break;default:events.push('other');}return events;}export const observed=run();\n",
        "const events:string[]=[];function helper(n:number){try{events.push('body');return 'value:'+n;}finally{events.push('body-finally');}}export function run(){try{throw ((helper((events.push('arg'),3))));}catch(error){events.push(String(error));}finally{events.push('caller-finally');}return events;}export const observed=run();\n",
        "const events:string[]=[];function helper():never{events.push('body');throw 'inner';}export function run(){try{throw helper();}catch(error){events.push(String(error));}return events;}export const observed=run();\n",
    ] {
        for newline in ["\n", "\r\n"] {
            let source = source.replace('\n', newline);
            let (root, mut engine) = fixture(&source, "ts");
            let r =
                yoke::compiler_inline::prepare(&mut engine, &compiler(), &args(false, "ts"), &d())
                    .unwrap_or_else(|e| panic!("{source}: {e:#}"));
            let after = planned(&engine, &r, "ts");
            assert!(!after.contains("function helper"), "{after}");
            same_execution(&source, &after);
            assert_eq!(
                fs::read(root.path().join("main.ts")).unwrap(),
                source.as_bytes()
            );
        }
    }
}
#[test]
fn direct_if_heads_keep_explicit_and_inferred_predicate_flow() {
    for annotation in [":value is string", ""] {
        let source = format!(
            "function helper(value:unknown){annotation}{{return typeof value==='string';}}export function run(input:unknown){{if((helper(input)))return input.length;else return 0;}}export const observed=[run('owned'),run(3)];\n"
        );
        let (root, mut engine) = fixture(&source, "ts");
        let r = yoke::compiler_inline::prepare(&mut engine, &compiler(), &args(false, "ts"), &d())
            .unwrap_or_else(|e| panic!("{source}: {e:#}"));
        let after = planned(&engine, &r, "ts");
        same_execution(&source, &after);
        assert_eq!(
            fs::read(root.path().join("main.ts")).unwrap(),
            source.as_bytes()
        );
    }
}
#[test]
fn while_heads_repeat_arguments_body_effects_closures_finally_and_labelled_control() {
    for source in [
        "let n=3;function helper():number{return n--;}while(helper());export const observed=n;\n",
        "const events:string[]=[];function helper(n:number,...extra:number[]):boolean{try{events.push('call:'+n+':'+extra.length);return n<5;}finally{events.push('callee-finally');}}export function run(){let n=0;outer:while /* café 🦀 header */ (/* condition */((helper((events.push('arg:'+n),n++),(events.push('extra'),7))))/* tail */) /* between */ {try{events.push('body:'+n);if(n>6)break outer;if(n===2)continue outer;if(n===4)break outer;}finally{events.push('caller-finally');}}return {n,events};}export const observed=run();\n",
        "const closures:Array<()=>number>=[];const calls:number[]=[];function helper(n:number):boolean{calls.push(n);closures.push(()=>n);return n<3;}export function run(start:number){let n=start,bodies=0;while(helper(n++)){if(++bodies>6)break;}return [n,bodies];}const runs=[run(0),run(5)];export const observed={runs,calls,values:closures.map(read=>read())};\n",
        "const events:string[]=[];function helper(n:number):boolean{try{events.push('call:'+n);if(n===2)throw 'owned';return true;}finally{events.push('callee-finally');}}export function run(){let n=0;try{while(helper(n++)){events.push('body:'+n);if(n>6)break;}}catch(error){events.push(String(error));}finally{events.push('caller-finally');}return {n,events};}export const observed=run();\n",
        "function helper(n:number):boolean|undefined{exit:while(true){try{return n<3;}finally{if(n===2)break exit;}}}export function run(){let n=0,bodies=0;while(helper(n++)){if(++bodies>6)break;}return [n,bodies];}export const observed=run();\n",
    ] {
        for newline in ["\n", "\r\n"] {
            let source = source.replace('\n', newline);
            let (root, mut engine) = fixture(&source, "ts");
            let r =
                yoke::compiler_inline::prepare(&mut engine, &compiler(), &args(false, "ts"), &d())
                    .unwrap_or_else(|e| panic!("{source}: {e:#}"));
            let after = planned(&engine, &r, "ts");
            assert!(!after.contains("function helper"), "{after}");
            assert!(after.contains("while"), "{after}");
            assert_eq!(r["complete"], false);
            assert_eq!(r["sourceExecuted"], false);
            assert_eq!(r["sourceTreeWritten"], false);
            assert_eq!(r["verificationRequired"], true);
            same_execution(&source, &after);
            for comment in [
                "/* café 🦀 header */",
                "/* condition */",
                "/* tail */",
                "/* between */",
            ] {
                if source.contains(comment) {
                    assert_eq!(after.matches(comment).count(), 1, "{after}");
                }
            }
            if newline == "\r\n" {
                assert!(!after.replace("\r\n", "").contains('\n'), "{after}");
            }
            assert_eq!(
                fs::read(root.path().join("main.ts")).unwrap(),
                source.as_bytes()
            );
        }
    }
}
#[test]
fn while_heads_preserve_explicit_and_inferred_predicate_flow_inside_and_after_the_loop() {
    for annotation in [":value is string", ""] {
        let source = format!(
            "function helper(value:string|number){annotation}{{return typeof value==='string';}}export function run(value:string|number){{while((helper(value))){{value=value.length;}}const numberAfter=value;return numberAfter.toFixed(0);}}export const observed=[run('π🦀'),run(7)];\n"
        );
        let (root, mut engine) = fixture(&source, "ts");
        let r = yoke::compiler_inline::prepare(&mut engine, &compiler(), &args(false, "ts"), &d())
            .unwrap_or_else(|e| panic!("{source}: {e:#}"));
        let after = planned(&engine, &r, "ts");
        same_execution(&source, &after);
        assert_eq!(r["complete"], false);
        assert_eq!(
            fs::read(root.path().join("main.ts")).unwrap(),
            source.as_bytes()
        );
    }
}
#[test]
fn while_heads_keep_jsdoc_unbraced_else_ownership_and_the_original_label() {
    let source = "/** @param {number} n @returns {boolean} */\nfunction helper(n){return n<3;}\n/** @param {boolean} flag */\nexport function run(flag){let n=0,turns=0;const events=[];if(flag)outer:while /* header */ (/* condition */(helper(n++))/* tail */) /* between */ if(turns++>6||n>3)break outer;else if(n===1)continue outer;else events.push(n);else events.push(-1);return [n,events];}\nexport const observed=[run(false),run(true)];\n";
    let (root, mut engine) = fixture(source, "js");
    let r = yoke::compiler_inline::prepare(&mut engine, &compiler(), &args(false, "js"), &d())
        .unwrap_or_else(|e| panic!("{e:#}"));
    let after = planned(&engine, &r, "js");
    assert!(after.contains("outer:while /* header */ (true)"), "{after}");
    same_execution(source, &after);
    for comment in [
        "/* header */",
        "/* condition */",
        "/* tail */",
        "/* between */",
    ] {
        assert_eq!(after.matches(comment).count(), 1, "{after}");
    }
    assert_eq!(
        fs::read(root.path().join("main.js")).unwrap(),
        source.as_bytes()
    );
}
#[test]
fn for_heads_preserve_initializer_update_iteration_bindings_and_abrupt_completions() {
    for source in [
        "const events:string[]=[];function helper(n:number,...extra:number[]):boolean{try{events.push('call:'+n+':'+extra.length);return n<5;}finally{events.push('callee-finally');}}export function run(){let turns=0;outer:for /* café 🦀 header */ (let i=(events.push('init'),0);/* condition */(helper((events.push('arg:'+i),i),(events.push('extra'),7)))/* tail */;events.push('update:'+i),i++)/* between */{try{events.push('body:'+i);if(++turns>6)break outer;if(i===1)continue outer;if(i===3)break outer;}finally{events.push('caller-finally');}}return events;}export const observed=run();\n",
        "const closures:Array<()=>number>=[];const seen:number[]=[];function helper(read:()=>number):boolean{const n=read();seen.push(n);closures.push(read);return n<3;}const bodies:number[]=[];for(let i=0;helper(()=>i);i++){bodies.push(i);if(i>6)break;}for(var j=0;helper(()=>j);j++){bodies.push(j);if(j>6)break;}for(const box={value:0};helper(()=>box.value);box.value++){bodies.push(box.value);if(box.value>6)break;}export const observed={seen,bodies,values:closures.map(read=>read()),j};\n",
        "let calls=0,updates=0;function helper(n:number):number{calls++;return n;}export function run(start:number){let n=start;for(;helper(n);updates++,n--);return n;}export const observed=[run(0),run(2),calls,updates];\n",
        "const events:string[]=[];function helper(n:number):boolean{try{events.push('test:'+n);if(n===2)throw 'owned';return true;}finally{events.push('callee-finally');}}export function run(){try{for(let n=(events.push('init'),0);helper(n);events.push('update:'+n),n++){try{events.push('body:'+n);if(n>6)break;}finally{events.push('caller-finally');}}}catch(error){events.push(String(error));}return events;}export const observed=run();\n",
        "const events:string[]=[];function helper(n:number):boolean{events.push('test:'+n);return n<3;}export function run(){outer:for(let n=0;helper(n);events.push('update:'+n),n++){try{if(n>6)break outer;if(n===0)throw 'cancelled';events.push('body:'+n);}finally{events.push('finally:'+n);if(n===0)continue outer;if(n===2)break outer;}}return events;}export const observed=run();\n",
        "const events:string[]=[];function helper(n:number):boolean{events.push('test:'+n);return n<3;}export function run(){outer:for(let j=0;j<2;j++){for(let i=0;helper(i);events.push('inner-update:'+i),i++){events.push('body:'+j+':'+i);if(i>6)break;if(i===1)continue outer;}}return events;}export const observed=run();\n",
    ] {
        for newline in ["\n", "\r\n"] {
            let source = source.replace('\n', newline);
            let (root, mut engine) = fixture(&source, "ts");
            let r =
                yoke::compiler_inline::prepare(&mut engine, &compiler(), &args(false, "ts"), &d())
                    .unwrap_or_else(|e| panic!("{source}: {e:#}"));
            let after = planned(&engine, &r, "ts");
            assert!(!after.contains("function helper"), "{after}");
            assert_eq!(r["complete"], false);
            assert_eq!(r["sourceExecuted"], false);
            assert_eq!(r["sourceTreeWritten"], false);
            assert_eq!(r["verificationRequired"], true);
            same_execution(&source, &after);
            for comment in [
                "/* café 🦀 header */",
                "/* condition */",
                "/* tail */",
                "/* between */",
            ] {
                if source.contains(comment) {
                    assert_eq!(after.matches(comment).count(), 1, "{after}");
                }
            }
            if newline == "\r\n" {
                assert!(!after.replace("\r\n", "").contains('\n'), "{after}");
            }
            assert_eq!(
                fs::read(root.path().join("main.ts")).unwrap(),
                source.as_bytes()
            );
        }
    }
}
#[test]
fn for_heads_preserve_predicate_narrowing_in_the_body_update_and_after_exit() {
    for annotation in [":value is string", ""] {
        let source = format!(
            "function helper(value:string|number){annotation}{{return typeof value==='string';}}export function run(value:string|number){{let seen=0;for(;helper(value);value=value.length){{seen+=value.length;}}const numberAfter=value;return [seen,numberAfter.toFixed(0)];}}export const observed=[run('π🦀'),run(7)];\n"
        );
        let (root, mut engine) = fixture(&source, "ts");
        let r = yoke::compiler_inline::prepare(&mut engine, &compiler(), &args(false, "ts"), &d())
            .unwrap_or_else(|e| panic!("{source}: {e:#}"));
        let after = planned(&engine, &r, "ts");
        same_execution(&source, &after);
        assert_eq!(r["complete"], false);
        assert_eq!(
            fs::read(root.path().join("main.ts")).unwrap(),
            source.as_bytes()
        );
    }
}
#[test]
fn for_heads_keep_jsdoc_and_unbraced_else_ownership() {
    let source = "/** @param {number} n @returns {boolean} */\nfunction helper(n){return n<3;}\n/** @param {boolean} flag */\nexport function run(flag){const events=[];if(flag)outer:for(let n=0;(helper(n));n++)if(n>6)break outer;else if(n===1)continue outer;else events.push(n);else events.push(-1);return events;}export const observed=[run(false),run(true)];\n";
    let (root, mut engine) = fixture(source, "js");
    let r = yoke::compiler_inline::prepare(&mut engine, &compiler(), &args(false, "js"), &d())
        .unwrap_or_else(|e| panic!("{e:#}"));
    let after = planned(&engine, &r, "js");
    assert!(after.contains("outer:for(let n=0;true;n++)"), "{after}");
    same_execution(source, &after);
    assert_eq!(
        fs::read(root.path().join("main.js")).unwrap(),
        source.as_bytes()
    );
}
#[test]
fn do_heads_preserve_first_body_test_order_fresh_captures_and_abrupt_completions() {
    for source in [
        "let calls=0;function helper():boolean{calls++;return false;}let bodies=0;do{bodies++;}while(helper());export const observed={bodies,calls};\n",
        "const events:string[]=[];function helper(n:number,...extra:number[]):boolean{try{events.push('test:'+n+':'+extra.length);return n<4;}finally{events.push('callee-finally');}}export function run(){let n=0;outer:do /* café 🦀 before */ {try{events.push('body:'+n);if(++n>6)break outer;if(n===1)continue outer;if(n===3)break outer;}finally{events.push('caller-finally');}} /* after body */ while /* before test */ (/* condition */((helper((events.push('arg:'+n),n),(events.push('extra'),7))))/* tail */);return {n,events};}export const observed=run();\n",
        "const closures:Array<()=>number>=[];const tests:number[]=[];function helper(n:number):boolean{closures.push(()=>n);tests.push(n);return n<3;}export function run(start:number){let n=start,bodies=0;do{if(++bodies>6)break;n++;}while(helper(n));return [n,bodies];}const runs=[run(0),run(5)];export const observed={runs,tests,values:closures.map(read=>read())};\n",
        "const events:string[]=[];function helper():boolean{events.push('test');return false;}export function run(mode:number){try{do{try{events.push('body:'+mode);if(mode===0)break;if(mode===1)throw 'owned';if(mode===2)return 'returned';continue;}finally{events.push('body-finally');}}while(helper());}catch(error){events.push(String(error));}return 'done';}export const observed={runs:[run(0),run(1),run(2),run(3)],events};\n",
        "const events:string[]=[];function helper(n:number):boolean{try{events.push('test:'+n);if(n===2)throw 'owned';return true;}finally{events.push('callee-finally');}}export function run(){let n=0;try{do{events.push('body:'+n);if(++n>6)break;}while(helper(n));}catch(error){events.push(String(error));}finally{events.push('caller-finally');}return events;}export const observed=run();\n",
        "const events:string[]=[];function helper(n:number):boolean|undefined{exit:while(true){try{events.push('test:'+n);return n<3;}finally{if(n===2)break exit;}}}export function run(){let n=0;outer:for(let j=0;j<3;j++){do{try{events.push('body:'+j+':'+n);if(++n>6)break;if(j===1)continue outer;continue;}finally{events.push('body-finally');}}while(helper(n));}return {n,events};}export const observed=run();\n",
        "let n=3;const __ci_inline_1=7;function helper():number{return n--;}do;while((helper()));export const observed=[n,__ci_inline_1];\n",
    ] {
        for newline in ["\n", "\r\n"] {
            let source = source.replace('\n', newline);
            let (root, mut engine) = fixture(&source, "ts");
            let r =
                yoke::compiler_inline::prepare(&mut engine, &compiler(), &args(false, "ts"), &d())
                    .unwrap_or_else(|e| panic!("{source}: {e:#}"));
            let after = planned(&engine, &r, "ts");
            assert!(!after.contains("function helper"), "{after}");
            assert!(after.contains("for (let __ci_inline_"), "{after}");
            assert_eq!(r["complete"], false);
            assert_eq!(r["sourceExecuted"], false);
            assert_eq!(r["sourceTreeWritten"], false);
            assert_eq!(r["verificationRequired"], true);
            same_execution(&source, &after);
            for comment in [
                "/* café 🦀 before */",
                "/* after body */",
                "/* before test */",
                "/* condition */",
                "/* tail */",
            ] {
                if source.contains(comment) {
                    assert_eq!(after.matches(comment).count(), 1, "{after}");
                }
            }
            if newline == "\r\n" {
                assert!(!after.replace("\r\n", "").contains('\n'), "{after}");
            }
            assert_eq!(
                fs::read(root.path().join("main.ts")).unwrap(),
                source.as_bytes()
            );
        }
    }
}
#[test]
fn do_heads_preserve_predicate_condition_and_false_exit_flow() {
    for annotation in [":value is string", ""] {
        let source = format!(
            "function helper(value:string|number){annotation}{{return typeof value==='string';}}export function run(value:string|number){{let turns=0;const seen:string[]=[];do{{seen.push(typeof value);if(++turns===2||typeof value==='number')value=7;}}while(helper(value));const numberAfter=value;return [seen,numberAfter.toFixed(0)];}}export const observed=[run('π🦀'),run(7)];\n"
        );
        let (root, mut engine) = fixture(&source, "ts");
        let r = yoke::compiler_inline::prepare(&mut engine, &compiler(), &args(false, "ts"), &d())
            .unwrap_or_else(|e| panic!("{source}: {e:#}"));
        same_execution(&source, &planned(&engine, &r, "ts"));
        assert_eq!(r["complete"], false);
        assert_eq!(
            fs::read(root.path().join("main.ts")).unwrap(),
            source.as_bytes()
        );
    }
}
#[test]
fn do_heads_keep_jsdoc_comments_unbraced_else_and_automatic_semicolon_insertion() {
    let source = "/** @param {number} n @returns {boolean} */\nfunction helper(n){return n<3;}\n/** @param {boolean} flag */\nexport function run(flag){let n=0;const events=[];if(flag)outer:do /* before body */ if(++n>6)break outer;else if(n===1)continue outer;else events.push(n); /* after body */ while /* before test */ (/* condition */(helper(n))/* tail */)\nelse events.push(-1);return [n,events];}\nexport const observed=[run(false),run(true)];\n";
    for newline in ["\n", "\r\n"] {
        let source = source.replace('\n', newline);
        let (root, mut engine) = fixture(&source, "js");
        let r = yoke::compiler_inline::prepare(&mut engine, &compiler(), &args(false, "js"), &d())
            .unwrap_or_else(|e| panic!("{source}: {e:#}"));
        let after = planned(&engine, &r, "js");
        assert!(after.contains("outer:for (let __ci_inline_"), "{after}");
        same_execution(&source, &after);
        for comment in [
            "/* before body */",
            "/* after body */",
            "/* before test */",
            "/* condition */",
            "/* tail */",
        ] {
            assert_eq!(after.matches(comment).count(), 1, "{after}");
        }
        assert_eq!(
            fs::read(root.path().join("main.js")).unwrap(),
            source.as_bytes()
        );
    }
}
#[test]
fn statement_heads_keep_javascript_jsdoc_and_unbraced_else_ownership() {
    let source = "/** @param {number} n */\nfunction helper(n){return n>0;}\n/** @param {number} n */\nexport function run(n){if(n<0)return 'negative';else if(helper(n))return 'positive';else return 'zero';}\nexport const observed=[run(-1),run(0),run(2)];\n";
    let (root, mut engine) = fixture(source, "js");
    let r = yoke::compiler_inline::prepare(&mut engine, &compiler(), &args(false, "js"), &d())
        .unwrap_or_else(|e| panic!("{e:#}"));
    same_execution(source, &planned(&engine, &r, "js"));
    assert_eq!(
        fs::read(root.path().join("main.js")).unwrap(),
        source.as_bytes()
    );
}
#[test]
fn repeated_conditional_and_overlapping_statement_heads_refuse_without_publication() {
    for consumer in [
        "while(helper()&&true)break;",
        "do{}while(helper()&&true);",
        "do{if(helper())break;}while(helper());",
        "for(;helper()&&true;)break;",
        "if(true&&helper()){}",
        "switch(1+Number(helper())){}",
        "throw String(helper());",
        "if(helper()){if(helper())return true;}return false;",
        "while(helper()){if(helper())break;}",
        "for(;helper();){if(helper())break;}",
        "for(let value=helper();;){break;}",
        "for(;true;helper()){break;}",
    ] {
        let source =
            format!("function helper(){{return true;}}export function run(){{{consumer}}}\n");
        let (root, mut engine) = fixture(&source, "ts");
        assert!(
            yoke::compiler_inline::prepare(&mut engine, &compiler(), &args(false, "ts"), &d())
                .is_err(),
            "{source}"
        );
        unpublished(&engine);
        assert_eq!(
            fs::read(root.path().join("main.ts")).unwrap(),
            source.as_bytes()
        );
    }
}

#[test]
fn complete_identifier_assignments_keep_write_order_completion_and_branch_ownership() {
    for (language, source) in [
        (
            "ts",
            "const events:string[]=[];let target=1;function helper(n:number):number{events.push('body:'+n);target=10;return n+target;}target=helper((events.push('arg:'+target),target));export const observed={target,events};",
        ),
        (
            "ts",
            "const events:string[]=[];function helper(n:number):number{events.push('body');return n*2;}function run(flag:boolean){let target=1;if(flag)target=helper((events.push('arg'),3));else target=5;return target;}export const observed=[run(false),run(true),events];",
        ),
        (
            "ts",
            "const events:string[]=[];function helper(n:number):number{events.push('body');if(n)throw 'owned';return n;}let target=7;try{target=helper((events.push('arg'),3));}catch(error){events.push(String(error));}finally{events.push('target:'+target);}export const observed={target,events};",
        ),
        (
            "ts",
            "const events:string[]=[];function helper(n:number):number{try{events.push('try');return n;}finally{events.push('finally');return n+2;}}let target=0;target=helper((events.push('arg'),3));export const observed={target,events};",
        ),
        (
            "ts",
            "function helper():number|undefined{exit:while(true){try{return 1;}finally{break exit;}}}let target:number|undefined=7;target=helper();export const observed={target,isUndefined:target===undefined};",
        ),
        (
            "ts",
            "function helper():{run:(n:number)=>number}{return {run:n=>n+1};}let target:{run:(n:number)=>number}={run:n=>n};target=helper();export const observed=target.run(3);",
        ),
        (
            "ts",
            "function helper():never{throw 'owned';}let target=7;const events:string[]=[];try{target=helper();}catch(error){events.push(String(error));}export const observed={target,events};",
        ),
        (
            "ts",
            "function helper(n:number):number{return n+1;}let target=0;(((target = ((helper(2))))));export const observed=target;",
        ),
        (
            "ts",
            "// 🧪\r\nfunction helper(valeurÉ:number):number{return valeurÉ+1;}\r\nlet target=0;\r\ntarget /* avant */ = /* appel */ ((helper(/* café */2)));\r\nexport const observed=target;\r\n",
        ),
        (
            "js",
            "/** @param {number} n */\nfunction helper(n){return n+1;}\nlet target=0;target=helper(2);export const observed=target;\n",
        ),
    ] {
        for (epoch, keep) in [(AnalyzerEpoch::V8, false), (AnalyzerEpoch::V9, true)] {
            let (root, mut engine) = fixture(source, language);
            engine.capture_epoch = epoch;
            let r = yoke::compiler_inline::prepare(
                &mut engine,
                &compiler(),
                &args(keep, language),
                &d(),
            )
            .unwrap_or_else(|e| panic!("{source}: {e:#}"));
            let after = planned(&engine, &r, language);
            assert_eq!(after.contains("function helper"), keep, "{after}");
            assert_eq!(r["sourceExecuted"], false);
            assert_eq!(r["complete"], false);
            assert_eq!(r["verificationRequired"], true);
            same_execution(source, &after);
            for comment in ["/* avant */", "/* appel */", "/* café */"] {
                if source.contains(comment) {
                    assert!(after.contains(comment), "{after}");
                }
            }
            assert_eq!(
                fs::read(root.path().join(format!("main.{language}"))).unwrap(),
                source.as_bytes()
            );
        }
    }
}

#[test]
fn effectful_targets_and_compound_or_consumed_assignments_refuse_without_publication() {
    for consumer in [
        "const box={value:0};box.value=helper();",
        "const box=[0];function key(){return 0;}box[key()]=helper();",
        "const box={value:0};function base(){return box;}base().value=helper();",
        "let target=0;target+=helper();",
        "let target=0;target||=helper();",
        "let target=0,other=0;other=target=helper();",
        "let target=0;function run(){return target=helper();}run();",
        "let target=0;function use(n:number){return n;}use(target=helper());",
        "let target=0;for(target=helper();target<4;target++){}",
        "let target=0;((target=helper()),target++);",
        "let target=0;(target as number)=helper();",
    ] {
        let source = format!("function helper():number{{return 3;}}{consumer}export {{}};");
        let (root, mut engine) = fixture(&source, "ts");
        let error =
            yoke::compiler_inline::prepare(&mut engine, &compiler(), &args(false, "ts"), &d())
                .unwrap_err();
        assert!(
            format!("{error:#}").contains("Body expression context needs sequencing"),
            "{source}: {error:#}"
        );
        unpublished(&engine);
        assert_eq!(
            fs::read(root.path().join("main.ts")).unwrap(),
            source.as_bytes()
        );
    }
    let source = "function helper():number[]{return [3];}let target=0;[target]=helper();export {};";
    let (root, mut engine) = fixture(source, "ts");
    let error = yoke::compiler_inline::prepare(&mut engine, &compiler(), &args(false, "ts"), &d())
        .unwrap_err();
    assert!(format!("{error:#}").contains("Body expression context needs sequencing"));
    unpublished(&engine);
    assert_eq!(
        fs::read(root.path().join("main.ts")).unwrap(),
        source.as_bytes()
    );
}
