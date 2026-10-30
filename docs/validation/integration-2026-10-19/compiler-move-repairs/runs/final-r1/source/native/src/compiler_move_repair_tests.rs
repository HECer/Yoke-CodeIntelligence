use super::*;
use crate::{Engine, projects::CompilerSpec};
use serde_json::{Value, json};
fn deadline() -> Deadline {
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
        typescript: std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../node_modules/typescript/lib/typescript.js")
            .canonicalize()
            .unwrap(),
        trusted: true,
    }
}
#[test]
fn original_byte_composition_matches_independent_sequential_utf8_edits() {
    let d = deadline();
    let before = Contents::from([("a.ts".into(), "\u{feff}a🦀bc\néz".into())]);
    for seed in 0..128u64 {
        let mut state = seed + 1;
        let mut next = || {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            (state >> 32) as usize
        };
        let mut tables = BTreeMap::new();
        let mut expected = before.clone();
        for _ in 0..8 {
            let text = &expected["a.ts"];
            let mut boundaries: Vec<_> = text.char_indices().map(|(i, _)| i).collect();
            boundaries.push(text.len());
            let a = next() % boundaries.len();
            let b = next() % boundaries.len();
            let (s, e) = (boundaries[a.min(b)], boundaries[a.max(b)]);
            let inserted = ["", "x", "🌍", "ab", ";\n"][next() % 5];
            let edit = TextEdit {
                path: "a.ts".into(),
                start_byte: s,
                end_byte: e,
                text: inserted.into(),
            };
            expected = super::super::apply(std::slice::from_ref(&edit), &expected, &d).unwrap();
            apply_pieces(&mut tables, &[edit], &before, &d).unwrap();
            assert_eq!(materialize(&tables, &before, &d).unwrap(), expected);
            let composed = compose(&tables, &before, &d).unwrap();
            assert_eq!(
                super::super::apply(&composed, &before, &d).unwrap(),
                expected
            );
        }
    }
    let mut pieces = vec![Piece::Inserted("🦀".into())];
    assert!(split(&mut pieces, 1, &d).is_err());
    assert!(split(&mut pieces, 5, &d).is_err());
    assert!(split(&mut pieces, 0, &Deadline::new(0)).is_err());
}

#[test]
fn coherent_repaired_effects_still_require_original_bindings_ownership_and_property_keys() {
    let root = tempfile::tempdir().unwrap();
    for (name, text) in [
        (
            "source.ts",
            "import {delta as step} from './dep';\nexport function helper(n:number){return {step,n:n+step};}\nexport const keep=3;\n",
        ),
        ("target.ts", "const step=20;\nexport const other=step;\n"),
        (
            "consumer.ts",
            "export {helper as renamed,keep} from './source';\n",
        ),
        ("dep.ts", "export const delta=2;\n"),
        (
            "tsconfig.json",
            r#"{"compilerOptions":{"strict":true,"target":"ES2022","module":"ESNext","moduleResolution":"Bundler","noEmit":true},"files":["source.ts","target.ts","consumer.ts"]}"#,
        ),
    ] {
        std::fs::write(root.path().join(name), text).unwrap();
    }
    let mut engine = Engine::open(root.path()).unwrap();
    let (snapshot, graph, _, _) = engine.index(&deadline()).unwrap();
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
    let tree = ast::parse("typescript", &before["source.ts"], &deadline()).unwrap();
    let selected = ast::named_statement(&tree, &before["source.ts"], "helper", &deadline())
        .unwrap()
        .unwrap();
    let spec = compiler();
    let report=crate::projects::execute(&engine.store,&snapshot,&spec,"move",Some(json!({"path":"source.ts","destination":"target.ts","name":"helper","startByte":selected.statement.start_byte(),"endByte":selected.statement.end_byte()})),&deadline()).unwrap();
    assert_eq!(report["move"]["repairs"].as_array().unwrap().len(), 2);
    let check = |value: Value| -> Result<Contents> {
        let mut effect: Effect = crate::json::decode(value)?;
        let original = super::super::normalize(&effect.raw_edits, &before, &deadline())?;
        let (edits, after) = admit(
            &before,
            &original,
            &effect,
            &snapshot,
            &symbol,
            "target.ts",
            &deadline(),
        )?;
        effect.edits = edits;
        effect.result_hashes = effect
            .edits
            .iter()
            .map(|e| (e.path.clone(), crate::model::hash(&after[&e.path])))
            .collect();
        super::super::bindings::check(
            &engine,
            &spec,
            &snapshot,
            &symbol,
            "target.ts",
            &before,
            &after,
            &effect,
            &report,
            &deadline(),
        )?;
        Ok(after)
    };
    let after = check(report["move"].clone()).unwrap();
    let mut exhausted = report["move"].clone();
    let raw = exhausted["rawEdits"].as_array_mut().unwrap();
    let count = 2000 - raw.len();
    let path = raw[0]["path"].clone();
    let mut prefix: Vec<Value> = (0..count)
        .map(|ordinal| json!({"path":path,"startByte":0,"endByte":0,"text":"","ordinal":ordinal}))
        .collect();
    for edit in raw.iter_mut().filter(|e| e["path"] == path) {
        edit["ordinal"] = json!(edit["ordinal"].as_u64().unwrap() + count as u64);
    }
    prefix.append(raw);
    *raw = prefix;
    assert!(
        check(exhausted).is_err(),
        "repair operations must share the original compiler operation budget"
    );

    for (pointer, value) in [
        ("/repairs/0/selected", json!([1])),
        ("/repairs/0/moduleText", json!("'./dep'")),
        (
            "/repairs/0/current/text",
            json!("export {keep} from './source';"),
        ),
        ("/repairs/1/importName/path", json!("source.ts")),
        ("/repairs/1/newName", json!("step")),
        ("/repairs/1/newName", json!("__ci_move_0001")),
        ("/repairs/1/originalReferences/0/path", json!("target.ts")),
        ("/repairs/1/currentReferences/0/startByte", json!(0)),
        ("/composition", json!("unchecked")),
    ] {
        let mut bad = report["move"].clone();
        *bad.pointer_mut(pointer).unwrap() = value;
        assert!(check(bad).is_err(), "{pointer}");
    }
    let mut bad = report["move"].clone();
    bad["repairs"][1]["currentReferences"][1] = bad["repairs"][1]["currentReferences"][0].clone();
    assert!(check(bad).is_err());
    for field in ["command", "authority", "writePermission"] {
        let mut bad = report["move"].clone();
        bad["repairs"][0][field] = json!(true);
        assert!(crate::json::decode::<Effect>(bad).is_err());
    }
    // Even coherently changed source and key-span reports cannot rename the
    // property represented by the original shorthand's shared source anchor.
    let mut effect: Effect = crate::json::decode(report["move"].clone()).unwrap();
    let mut changed = after.clone();
    let start = changed["target.ts"].find("step:").unwrap();
    changed
        .get_mut("target.ts")
        .unwrap()
        .replace_range(start..start + 4, "stap");
    for binding in &mut effect.after.bindings {
        if binding.span.path == "target.ts" && binding.span.start_byte == start {
            binding.span.text = "stap".into();
        }
        for target in &mut binding.targets {
            if let Target::Captured { span } = target
                && span.path == "target.ts"
                && span.start_byte == start
            {
                span.text = "stap".into();
            }
        }
    }
    effect.result_hashes.insert(
        "target.ts".into(),
        crate::model::hash(&changed["target.ts"]),
    );
    assert!(
        super::super::bindings::check(
            &engine,
            &spec,
            &snapshot,
            &symbol,
            "target.ts",
            &before,
            &changed,
            &effect,
            &report,
            &deadline()
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
