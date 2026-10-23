use super::super::*;
fn d() -> Deadline {
    Deadline::new(60_000)
}
#[test]
fn literal_attribute_budgets_deadlines_and_closed_repair_schema_refuse() {
    for suffix in [
        format!("with {{a: '{}' }}", "x".repeat(65_536)),
        format!(
            "with {{{}}}",
            (0..257)
                .map(|i| format!("a{i}:'x'"))
                .collect::<Vec<_>>()
                .join(",")
        ),
        "with {a: run()}".into(),
        "with {...other}".into(),
        "with {[key]:'x'}".into(),
    ] {
        let text = format!("import {{x}} from './dep' {suffix};");
        let tree = ast::parse("typescript", &text, &d()).unwrap();
        assert!(
            super::suffix(tree.root_node().named_child(0).unwrap(), &text, &d()).is_err(),
            "{suffix}"
        );
    }
    let text = "import {x} from './dep' with {a:'x'};";
    let tree = ast::parse("typescript", text, &d()).unwrap();
    assert!(
        super::suffix(
            tree.root_node().named_child(0).unwrap(),
            text,
            &Deadline::new(0)
        )
        .is_err()
    );
    let span = json!({"path":"source.ts","startByte":0,"endByte":0,"text":""});
    let mut repair = json!({"kind":"importAttributes","originalImport":span,"currentImport":span});
    assert!(crate::json::decode::<repairs::Repair>(repair.clone()).is_ok());
    repair["authority"] = json!(true);
    assert!(crate::json::decode::<repairs::Repair>(repair).is_err());
}
#[test]
fn coherent_rehashed_attribute_changes_and_omissions_fail_native_binding_admission() {
    let root = tempfile::tempdir().unwrap();
    for (name, text) in [
        (
            "source.ts",
            "import {delta as step} from './dep' with {flavor: 'owned'};\nexport function helper(n:number){return n+step;}\nexport const keep=3;\n",
        ),
        ("target.ts", "const step=20; export const other=step;\n"),
        ("dep.ts", "export const delta=2;\n"),
        (
            "tsconfig.json",
            r#"{"compilerOptions":{"strict":true,"target":"ES2022","module":"ESNext","moduleResolution":"Bundler","noEmit":true},"files":["source.ts","target.ts","dep.ts"]}"#,
        ),
    ] {
        std::fs::write(root.path().join(name), text).unwrap();
    }
    let mut engine = Engine::open(root.path()).unwrap();
    let (snapshot, graph, _, _) = engine.index(&d()).unwrap();
    let symbol = Declaration::from_graph(
        graph
            .entities
            .iter()
            .find(|e| e.path == "source.ts" && e.name == "helper")
            .unwrap()
            .clone(),
    );
    let before: Contents = snapshot
        .files
        .iter()
        .map(|f| {
            (
                f.path.clone(),
                crate::query::source(&engine.store, &snapshot, &f.path)
                    .unwrap()
                    .0,
            )
        })
        .collect();
    let tree = ast::parse("typescript", &before["source.ts"], &d()).unwrap();
    let selected = ast::named_statement(&tree, &before["source.ts"], "helper", &d())
        .unwrap()
        .unwrap();
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
    let report=crate::projects::execute(&engine.store,&snapshot,&spec,"move",Some(json!({"path":"source.ts","destination":"target.ts","name":"helper","startByte":selected.statement.start_byte(),"endByte":selected.statement.end_byte()})),&d()).unwrap();
    let effect: Effect = crate::json::decode(report["move"].clone()).unwrap();
    let after = apply(&effect.edits, &before, &d()).unwrap();
    bindings::check(
        &engine,
        &spec,
        &snapshot,
        &symbol,
        "target.ts",
        &before,
        &after,
        &effect,
        &report,
        &d(),
    )
    .unwrap();
    assert!(after["target.ts"].contains("with {flavor: 'owned'}"));
    for replacement in [
        "with {flavor: 'alien'}".to_string(),
        " ".repeat("with {flavor: 'owned'}".len()),
    ] {
        let mut forged = after.clone();
        *forged.get_mut("target.ts").unwrap() =
            after["target.ts"].replace("with {flavor: 'owned'}", &replacement);
        let mut value = report["move"].clone();
        value["resultHashes"]["target.ts"] = json!(crate::model::hash(&forged["target.ts"]));
        for module in value["after"]["modules"].as_array_mut().unwrap() {
            if module["path"] != "target.ts" {
                continue;
            }
            for import in module["imports"].as_array_mut().unwrap() {
                let start = import["span"]["startByte"].as_u64().unwrap() as usize;
                let end = import["span"]["endByte"].as_u64().unwrap() as usize;
                import["span"]["text"] = json!(&forged["target.ts"][start..end]);
            }
        }
        if replacement.trim().is_empty() {
            // The erased clause has no AST identifiers. Keep the forged compiler
            // observations coherent, so this exercises attribute admission rather
            // than the independent stale-span refusal.
            let start = after["target.ts"].find("with {flavor: 'owned'}").unwrap();
            let end = start + "with {flavor: 'owned'}".len();
            value["after"]["bindings"]
                .as_array_mut()
                .unwrap()
                .retain(|binding| {
                    let span = &binding["span"];
                    span["path"] != "target.ts"
                        || span["startByte"].as_u64().unwrap() < start as u64
                        || span["endByte"].as_u64().unwrap() > end as u64
                });
        }
        let bad: Effect = crate::json::decode(value).unwrap();
        let error = bindings::check(
            &engine,
            &spec,
            &snapshot,
            &symbol,
            "target.ts",
            &before,
            &forged,
            &bad,
            &report,
            &d(),
        )
        .unwrap_err();
        assert!(format!("{error:#}").contains("attributes"), "{error:#}");
    }
    let mut bad = report["move"].clone();
    let repair = bad["repairs"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|r| r["kind"] == "importAttributes")
        .unwrap();
    repair["originalImport"]["path"] = json!("../foreign.ts");
    let effect: Effect = crate::json::decode(bad).unwrap();
    let original = normalize(&effect.raw_edits, &before, &d()).unwrap();
    assert!(
        repairs::admit(
            &before,
            &original,
            &effect,
            &snapshot,
            &symbol,
            "target.ts",
            &d()
        )
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
}
