use super::*;
fn d() -> Deadline {
    Deadline::new(30_000)
}
fn spec() -> CompilerSpec {
    CompilerSpec {
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
    }
}
#[test]
fn raw_compiler_edit_order_bom_utf8_overlap_and_capacity_are_independently_admitted() {
    let before = Contents::from([("a.ts".into(), "\u{feff}const café=1;\n".into())]);
    let raw = json!([{"path":"a.ts","startByte":0,"endByte":0,"text":"import {x} from './b';\n","ordinal":0},{"path":"a.ts","startByte":0,"endByte":0,"text":"export ","ordinal":1}]);
    let decode = |v: Value| crate::json::decode::<Vec<RawEdit>>(v).unwrap();
    let edits = normalize(&decode(raw.clone()), &before, &d()).unwrap();
    assert_eq!(edits[0].start_byte, 3);
    assert_eq!(edits[0].end_byte, 3);
    assert!(
        apply(&edits, &before, &d()).unwrap()["a.ts"]
            .starts_with("\u{feff}import {x} from './b';\nexport const")
    );
    for (pointer, value) in [
        ("/1/ordinal", json!(0)),
        ("/1/startByte", json!(1)),
        ("/0/endByte", json!(3)),
        ("/0/path", json!("../a.ts")),
        ("/0/text", json!("\0")),
    ] {
        let mut bad = raw.clone();
        *bad.pointer_mut(pointer).unwrap() = value;
        assert!(normalize(&decode(bad), &before, &d()).is_err(), "{pointer}");
    }
    let mut huge = raw.clone();
    huge[0]["text"] = json!("x".repeat(4_000_001));
    assert!(normalize(&decode(huge), &before, &d()).is_err());
    let mut remove = raw;
    remove[0]["endByte"] = json!(before["a.ts"].len());
    remove[0]["text"] = json!("");
    remove.as_array_mut().unwrap().pop();
    let after = apply(
        &normalize(&decode(remove), &before, &d()).unwrap(),
        &before,
        &d(),
    )
    .unwrap();
    assert_eq!(after["a.ts"], "\u{feff}");
}
#[test]
fn native_syntax_correspondence_keeps_operators_precedence_comments_and_for_separators() {
    let signature = |text: &str| {
        let tree = ast::parse("typescript", text, &d()).unwrap();
        let span = Span {
            path: "a.ts".into(),
            start_byte: 0,
            end_byte: 0,
            text: String::new(),
        };
        ast::project(
            &tree,
            "a.ts",
            text,
            &BTreeMap::new(),
            &span,
            "a.ts",
            None,
            None,
            &d(),
        )
        .unwrap()
        .events
        .into_iter()
        .map(|event| event.signature)
        .collect::<Vec<_>>()
    };
    for (a, b, equal) in [
        (
            "function f(){return 1+2}",
            "function f() { return 1 + 2; }",
            true,
        ),
        (
            "function f(){return (1+2)*3}",
            "function f(){return 1+2*3}",
            false,
        ),
        (
            "function f(){return 1+2}",
            "function f(){return 1-2}",
            false,
        ),
        (
            "function f(){for(;x;y){}}",
            "function f(){for(x;;y){}}",
            false,
        ),
        (
            "/** owned */ function f(){}",
            "/** changed */ function f(){}",
            false,
        ),
    ] {
        assert_eq!(signature(a) == signature(b), equal, "{a} / {b}");
    }
}
#[test]
fn coherent_rehashed_compiler_effect_cannot_change_bodies_exports_or_omit_import_names() {
    let root = tempfile::tempdir().unwrap();
    for (path, text) in [
        (
            "source.ts",
            "export function helper(n:number){return n+1;}\nconst hidden=5;\nexport const keep=3;\n",
        ),
        ("target.ts", "export const other=4;\n"),
        (
            "consumer.ts",
            "import {helper,keep} from './source';\nexport const answer=helper(keep);\n",
        ),
        (
            "tsconfig.json",
            r#"{"compilerOptions":{"strict":true,"target":"ES2022","module":"ESNext","moduleResolution":"Bundler","noEmit":true},"files":["source.ts","target.ts","consumer.ts"]}"#,
        ),
    ] {
        std::fs::write(root.path().join(path), text).unwrap();
    }
    let mut engine = Engine::open(root.path()).unwrap();
    let (snapshot, graph, _, _) = engine.index(&d()).unwrap();
    let symbol = Declaration::from_graph(
        graph
            .entities
            .iter()
            .find(|entity| entity.path == "source.ts" && entity.name == "helper")
            .unwrap()
            .clone(),
    );
    let before: Contents = snapshot
        .files
        .iter()
        .map(|file| {
            (
                file.path.clone(),
                crate::query::source(&engine.store, &snapshot, &file.path)
                    .unwrap()
                    .0,
            )
        })
        .collect();
    let tree = ast::parse("typescript", &before["source.ts"], &d()).unwrap();
    let selected = ast::selection(
        &tree,
        &symbol,
        &before["source.ts"],
        crate::capture::analyzer_epoch_for_snapshot_with_deadline(&engine.store, &snapshot, &d())
            .unwrap(),
        &d(),
    )
    .unwrap();
    let spec = spec();
    let report=crate::projects::execute(&engine.store,&snapshot,&spec,"move",Some(json!({"path":"source.ts","destination":"target.ts","name":"helper","startByte":selected.statement.start_byte(),"endByte":selected.statement.end_byte()})),&d()).unwrap();
    let effect: Effect = crate::json::decode(report["move"].clone()).unwrap();
    let after = apply(
        &normalize(&effect.raw_edits, &before, &d()).unwrap(),
        &before,
        &d(),
    )
    .unwrap();
    let check = |value: Value, after: &Contents| {
        let effect: Effect = crate::json::decode(value).unwrap();
        bindings::check(
            &engine,
            &spec,
            &snapshot,
            &symbol,
            "target.ts",
            &before,
            after,
            &effect,
            &report,
            &d(),
        )
    };
    check(report["move"].clone(), &after).unwrap();
    for (path, a, b) in [
        ("target.ts", "n+1", "n-1"),
        ("source.ts", "keep=3", "keep=9"),
    ] {
        let mut output = after.clone();
        assert!(output[path].contains(a));
        *output.get_mut(path).unwrap() = output[path].replace(a, b);
        let mut bad = report["move"].clone();
        bad["resultHashes"][path] = json!(crate::model::hash(&output[path]));
        assert!(check(bad, &output).is_err(), "{path}");
    }
    let mut bad = report["move"].clone();
    let module = bad["after"]["modules"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|module| module["path"] == "consumer.ts")
        .unwrap();
    module["imports"][0]["names"] = json!([]);
    assert!(check(bad, &after).is_err());
    let mut bad = report["move"].clone();
    let start = after["source.ts"].find("hidden").unwrap();
    let module = bad["after"]["modules"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|module| module["path"] == "source.ts")
        .unwrap();
    module["exports"].as_array_mut().unwrap().push(json!({"name":"hidden","resolution":"resolved","targets":[{"kind":"captured","span":{"path":"source.ts","startByte":start,"endByte":start+6,"text":"hidden"}}]}));
    assert!(check(bad, &after).is_err());
    for key in ["command", "authority", "resourceOperations"] {
        let mut bad = report["move"].clone();
        bad[key] = json!(true);
        assert!(crate::json::decode::<Effect>(bad).is_err());
    }
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
