//! Real immutable compiler witness, followed by independent forged-effect refusals.
use super::*;

#[test]
fn body_effects_cannot_forge_source_bindings_origins_types_paths_or_output() {
    let root = tempfile::tempdir().unwrap();
    let source = "// 🌍\n/** @returns {()=>{length:number}} */\nfunction helper(n:number) {const values=[n];return ()=>({length:values.length});}\nexport const answer=helper(2);\n";
    fs::write(root.path().join("main.ts"), source).unwrap();
    fs::write(root.path().join("tsconfig.json"), r#"{"compilerOptions":{"strict":true,"target":"ES2022","module":"ESNext","noEmit":true},"files":["main.ts"]}"#).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let d = Deadline::new(60_000);
    let (snapshot, graph, _, _) = engine.index(&d).unwrap();
    let symbol =
        crate::agent_repl::source_symbol(&engine.store, &snapshot, &graph, "helper", "main.ts", &d)
            .unwrap();
    let tree = parse("typescript", source, &d).unwrap();
    let declaration = classic(&tree, &symbol, source, &d).unwrap().unwrap();
    let spec = CompilerSpec {
        node: std::env::split_paths(&std::env::var_os("PATH").unwrap())
            .map(|p| p.join(if cfg!(windows) { "node.exe" } else { "node" }))
            .find(|p| p.is_file())
            .unwrap()
            .canonicalize()
            .unwrap(),
        typescript: std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../node_modules/typescript/lib/typescript.js")
            .canonicalize()
            .unwrap(),
        trusted: true,
    };
    let intent = json!({"path":symbol.path,"name":symbol.name,"startByte":symbol.start_byte,"endByte":symbol.end_byte,"nameStartByte":symbol.name_start_byte,"nameEndByte":symbol.name_end_byte,"keepDefinition":false});
    let report = crate::projects::execute(
        &engine.store,
        &snapshot,
        &spec,
        "inlineBody",
        Some(intent),
        &d,
    )
    .unwrap();
    let baseline = report["bodyInline"].clone();
    let admit = |value: Value, deadline: &Deadline| -> Result<String> {
        let effect: Effect = crate::json::decode(value)?;
        Ok(check(
            &engine,
            &spec,
            &snapshot,
            &symbol,
            source,
            &tree,
            declaration,
            false,
            &effect,
            &report,
            deadline,
        )?
        .1)
    };
    let result = admit(baseline.clone(), &d).unwrap();
    for (field, value) in [
        ("literalBox", json!(true)),
        ("docPieces", json!([])),
        (
            "docPieces",
            json!([{"startByte":0,"endByte":source.len(),"tag":true}]),
        ),
    ] {
        let mut bad = baseline.clone();
        bad["frames"][0][field] = value;
        assert!(admit(bad, &d).is_err(), "{field}");
    }
    assert!(!result.contains("helper(2)"));
    let mut missing_types = baseline.clone();
    missing_types["before"]["types"] = json!([]);
    missing_types["after"]["types"] = json!([]);
    assert!(admit(missing_types, &d).is_err());
    for (pointer, value) in [
        ("/schema", json!(2)),
        ("/path", json!("../main.ts")),
        ("/keepDefinition", json!(true)),
        ("/actionKind", json!("native.move")),
        ("/contextId", json!("wrong")),
        ("/resultHash", json!("b".repeat(64))),
        ("/frames/0/statementStart", json!(0)),
        ("/frames/0/callStart", json!(3)),
        ("/frames/0/names/result", json!("answer")),
        (
            "/frames/0/names/exit",
            baseline["frames"][0]["names"]["result"].clone(),
        ),
        ("/frames/0/resultType", json!("number; hidden()")),
        ("/frames/0/names/argumentCapture", json!([])),
        ("/frames/0/names/argumentCapture/0", json!("answer")),
        (
            "/frames/0/names/argumentCapture/1",
            baseline["frames"][0]["names"]["argumentCapture"][0].clone(),
        ),
        (
            "/frames/0/argumentDocs",
            json!([{"piece":null,"optional":false}]),
        ),
        (
            "/frames/0/argumentTemplates",
            json!([{"startByte":0,"endByte":4,"tag":true}]),
        ),
        ("/frames/0/names/returnType", json!([])),
        ("/frames/0/names/returnType", json!(["__ci_inline_8"])),
        ("/frames/0/names/returnType/0", json!("answer")),
        (
            "/frames/0/names/returnType/1",
            baseline["frames"][0]["names"]["returnType"][0].clone(),
        ),
        ("/references/0/path", json!("/outside.ts")),
        ("/references/0/text", json!("answer")),
        ("/edits/0/text", json!("// hidden deletion")),
        ("/edits/1/path", json!("other.ts")),
        ("/before/bindings/0/span/startByte", json!(3)),
        ("/after/bindings/0/span/path", json!("../main.ts")),
        ("/after/types/0/type", json!("x".repeat(16_001))),
        ("/after/types/0/span/path", json!("../main.ts")),
    ] {
        let mut bad = baseline.clone();
        *bad.pointer_mut(pointer).unwrap() = value;
        assert!(admit(bad, &d).is_err(), "{pointer}");
    }
    for pointer in [
        "/frames",
        "/references",
        "/before/bindings",
        "/after/bindings",
        "/before/types",
        "/after/types",
    ] {
        let mut bad = baseline.clone();
        let rows = bad.pointer_mut(pointer).unwrap().as_array_mut().unwrap();
        rows.push(rows[0].clone());
        assert!(admit(bad, &d).is_err(), "duplicate {pointer}");
    }
    // Coherent changed replacement and result hash still cannot alter source-derived frames.
    let mut bad = baseline.clone();
    let changed = bad["edits"][1]["text"]
        .as_str()
        .unwrap()
        .replace("values.length", "values.length+1");
    bad["edits"][1]["text"] = json!(changed);
    bad["resultHash"] = json!(crate::model::hash(
        result.replace("values.length", "values.length+1")
    ));
    assert!(admit(bad, &d).is_err());
    for (from, to) in [
        ("(void 0,", "("),
        ("if (void 0 === void 0)", "if (void 0 !== void 0)"),
        ("if (void 0 !== void 0)", "if (void 0 === void 0)"),
        (" as unknown as (typeof ", " as unknown as (never & typeof "),
    ] {
        let mut bad = baseline.clone();
        let changed = bad["edits"][1]["text"].as_str().unwrap().replace(from, to);
        assert_ne!(changed, bad["edits"][1]["text"].as_str().unwrap());
        bad["edits"][1]["text"] = json!(changed);
        bad["resultHash"] = json!(crate::model::hash(result.replace(from, to)));
        assert!(admit(bad, &d).is_err(), "{from}");
    }
    let from = format!(
        "{}({})",
        baseline["frames"][0]["names"]["returnType"][0]
            .as_str()
            .unwrap(),
        baseline["frames"][0]["names"]["arguments"][0]
            .as_str()
            .unwrap()
    );
    let to = format!(
        "{}(void 0)",
        baseline["frames"][0]["names"]["returnType"][0]
            .as_str()
            .unwrap()
    );
    let mut bad = baseline.clone();
    let changed = bad["edits"][1]["text"]
        .as_str()
        .unwrap()
        .replace(&from, &to);
    assert_ne!(changed, bad["edits"][1]["text"].as_str().unwrap());
    bad["edits"][1]["text"] = json!(changed);
    bad["resultHash"] = json!(crate::model::hash(result.replace(&from, &to)));
    assert!(admit(bad, &d).is_err());
    let mut bad = baseline.clone();
    assert_eq!(bad["edits"][0]["text"], ";");
    bad["edits"][0]["text"] = json!("");
    let mut changed = source.to_owned();
    for edit in bad["edits"].as_array().unwrap().iter().rev() {
        changed.replace_range(
            edit["startByte"].as_u64().unwrap() as usize
                ..edit["endByte"].as_u64().unwrap() as usize,
            edit["text"].as_str().unwrap(),
        );
    }
    bad["resultHash"] = json!(crate::model::hash(changed));
    assert!(admit(bad, &d).is_err());
    // Omission from both witnesses cannot hide the loss of an original identifier.
    let mut bad = baseline.clone();
    for phase in ["before", "after"] {
        bad[phase]["bindings"]
            .as_array_mut()
            .unwrap()
            .retain(|b| b["span"]["text"] != "values");
    }
    assert!(admit(bad, &d).is_err());
    let mut library_forged = false;
    let mut bad = baseline.clone();
    for b in bad["before"]["bindings"].as_array_mut().unwrap() {
        for t in b["targets"].as_array_mut().unwrap() {
            if t["kind"] == "library" {
                t["hash"] = json!("0".repeat(64));
                library_forged = true;
            }
        }
    }
    assert!(library_forged);
    assert!(admit(bad, &d).is_err());
    for key in ["command", "authority", "unknown"] {
        let mut bad = baseline.clone();
        bad[key] = json!(true);
        assert!(admit(bad, &d).is_err());
    }
    assert!(admit(baseline, &Deadline::new(0)).is_err());
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
    assert_eq!(
        fs::read_to_string(root.path().join("main.ts")).unwrap(),
        source
    );
}

#[test]
fn var_witnesses_cannot_forge_parameter_copies_catch_targets_types_or_property_roles() {
    let root = tempfile::tempdir().unwrap();
    let source = "// 🌍\nfunction helper(n=1,read=()=>n):unknown{var n=2;var {x:v=3,...rest}:{x?:number,y?:number}={y:4};try{throw 1;}catch(e){var e=2;}const fs:Array<()=>number>=[];for(var i=0;i<2;i++)fs.push(()=>i);return [n,read(),v,rest,e,fs.map(f=>f())];}export const answer=helper();";
    fs::write(root.path().join("main.ts"), source).unwrap();
    fs::write(root.path().join("tsconfig.json"), r#"{"compilerOptions":{"strict":true,"target":"ES2022","module":"ESNext","noEmit":true},"files":["main.ts"]}"#).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let d = Deadline::new(60_000);
    let (snapshot, graph, _, _) = engine.index(&d).unwrap();
    let symbol =
        crate::agent_repl::source_symbol(&engine.store, &snapshot, &graph, "helper", "main.ts", &d)
            .unwrap();
    let tree = parse("typescript", source, &d).unwrap();
    let declaration = classic(&tree, &symbol, source, &d).unwrap().unwrap();
    let spec = CompilerSpec {
        node: std::env::split_paths(&std::env::var_os("PATH").unwrap())
            .map(|p| p.join(if cfg!(windows) { "node.exe" } else { "node" }))
            .find(|p| p.is_file())
            .unwrap()
            .canonicalize()
            .unwrap(),
        typescript: std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../node_modules/typescript/lib/typescript.js")
            .canonicalize()
            .unwrap(),
        trusted: true,
    };
    let report = crate::projects::execute(&engine.store, &snapshot, &spec, "inlineBody", Some(json!({"path":symbol.path,"name":symbol.name,"startByte":symbol.start_byte,"endByte":symbol.end_byte,"nameStartByte":symbol.name_start_byte,"nameEndByte":symbol.name_end_byte,"keepDefinition":false})), &d).unwrap();
    let baseline = report["bodyInline"].clone();
    let admit = |value: Value| {
        let effect: Effect = crate::json::decode(value)?;
        check(
            &engine,
            &spec,
            &snapshot,
            &symbol,
            source,
            &tree,
            declaration,
            false,
            &effect,
            &report,
            &d,
        )
    };
    let (_, result, proof) = admit(baseline.clone()).unwrap();
    assert!(proof["varAssignmentPropertiesChecked"].as_u64().unwrap() > 0);
    assert_eq!(proof["functionVarGroups"].as_array().unwrap().len(), 5);
    for (pointer, value) in [
        ("/varAssignments", json!([])),
        ("/varTypes", json!([])),
        ("/varAssignments/0/span/path", json!("../main.ts")),
        ("/varAssignments/0/span/startByte", json!(4)),
        ("/varAssignments/0/resolution", json!("unbound")),
        ("/varTypes/0/span/text", json!("read")),
        ("/varTypes/0/type", json!("string")),
        ("/varTypes/0/type", json!("x".repeat(16_001))),
        ("/frames/0/varTypes/0", json!("number; hidden()")),
        ("/frames/0/names/varParameters", json!([])),
        ("/frames/0/names/varParameters/0", json!("n")),
        ("/frames/0/varSeedGroups", json!([])),
        ("/frames/0/varSeedGroups", json!([2, 2])),
        ("/frames/0/varSeedGroups", json!([0])),
        ("/frames/0/names/varSeeds", json!([])),
        ("/frames/0/names/varSeeds/0", json!("n")),
        ("/varTypes/2/type", json!("number; hidden()")),
    ] {
        let mut bad = baseline.clone();
        *bad.pointer_mut(pointer).unwrap() = value;
        assert!(admit(bad).is_err(), "{pointer}");
    }
    for pointer in ["/varAssignments", "/varTypes"] {
        let mut bad = baseline.clone();
        let rows = bad.pointer_mut(pointer).unwrap().as_array_mut().unwrap();
        rows.push(rows[0].clone());
        assert!(admit(bad).is_err(), "duplicate {pointer}");
    }
    let mut bad = baseline.clone();
    let own = bad["varAssignments"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["span"]["text"] == "n")
        .unwrap()["targets"]
        .clone();
    bad["varAssignments"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|b| b["span"]["text"] == "e")
        .unwrap()["targets"] = own;
    assert!(admit(bad).is_err());
    let mut bad = baseline.clone();
    let other_property = bad["after"]["bindings"]
        .as_array()
        .unwrap()
        .iter()
        .rev()
        .find(|b| b["span"]["text"] == "x")
        .unwrap()["targets"]
        .clone();
    let assignment_property = result.find("( {x:").unwrap() + 3;
    bad["after"]["bindings"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|b| b["span"]["text"] == "x" && b["span"]["startByte"] == assignment_property)
        .unwrap()["targets"] = other_property;
    let error = admit(bad)
        .err()
        .expect("foreign property target was admitted");
    assert!(
        format!("{error:#}").contains("assignment property"),
        "{error:#}"
    );
    let mut bad = baseline.clone();
    bad["before"]["types"]
        .as_array_mut()
        .unwrap()
        .retain(|t| t["span"]["text"] != "rest");
    assert!(admit(bad).is_err(), "omitted destructured type witness");
    let mut bad = baseline.clone();
    let changed = bad["edits"][1]["text"]
        .as_str()
        .unwrap()
        .replace("void 0 !== void 0", "void 0 === void 0");
    bad["edits"][1]["text"] = json!(changed);
    bad["resultHash"] = json!(crate::model::hash(
        result.replace("void 0 !== void 0", "void 0 === void 0")
    ));
    assert!(admit(bad).is_err(), "executable type witness");
    let from = ": (unknown) | undefined";
    let to = ": any";
    let mut bad = baseline.clone();
    let changed = bad["edits"][1]["text"].as_str().unwrap().replace(from, to);
    assert_ne!(changed, bad["edits"][1]["text"].as_str().unwrap());
    bad["edits"][1]["text"] = json!(changed);
    bad["resultHash"] = json!(crate::model::hash(result.replace(from, to)));
    assert!(admit(bad).is_err(), "forged return storage context");

    let mut bad = baseline.clone();
    let changed = bad["edits"][1]["text"]
        .as_str()
        .unwrap()
        .replace("let i:", "const i:");
    bad["edits"][1]["text"] = json!(changed);
    bad["resultHash"] = json!(crate::model::hash(result.replace("let i:", "const i:")));
    assert!(admit(bad).is_err());
    assert_eq!(
        fs::read_to_string(root.path().join("main.ts")).unwrap(),
        source
    );
    assert_eq!(engine.store.db.query_row("SELECT COUNT(*) FROM records WHERE kind IN ('plan','approval','receipt','journal','native_compiler_report_v1')",[],|r|r.get::<_,usize>(0)).unwrap(),0);
}
