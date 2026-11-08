//! Real immutable compiler witness, followed by independent forged-effect refusals.
use super::*;

#[test]
fn body_effects_cannot_forge_source_bindings_origins_types_paths_or_output() {
    let root = tempfile::tempdir().unwrap();
    let source = "// 🌍\nfunction helper(n:number):number {const values=[n];return values.length;}\nexport const answer=helper(2);\n";
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
