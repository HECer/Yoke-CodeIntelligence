#![allow(dead_code)]
//! Shared literal fixture owned by B; no provider or package-manager execution.
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs};
use yoke::{
    Engine,
    analysis::AnalyzerEpoch,
    capture::Deadline,
    model::{Graph, Snapshot, hash},
};
pub const MARKERS: &[(&str, &str)] = &[
    (r#"package.json"#, r#"{"workspaces":["packages/*"]}"#),
    (r#"packages/large/package.json"#, r#"{"name":"large"}"#),
    (r#"packages/small/package.json"#, r#"{"name":"small"}"#),
];
pub const SOURCES: &[(&str, &str)] = &[
    (
        r#"packages/large/one.rs"#,
        r#"fn big_first(){ payment(); payment(); payment(); }
"#,
    ),
    (
        r#"packages/large/two.rs"#,
        r#"fn big_second(){ payment(); payment(); }
"#,
    ),
    (
        r#"packages/large/n03.rs"#,
        r#"fn noise_03(){ payment(); }
"#,
    ),
    (
        r#"packages/large/n04.rs"#,
        r#"fn noise_04(){ payment(); }
"#,
    ),
    (
        r#"packages/large/n05.rs"#,
        r#"fn noise_05(){ payment(); }
"#,
    ),
    (
        r#"packages/large/n06.rs"#,
        r#"fn noise_06(){ payment(); }
"#,
    ),
    (
        r#"packages/large/n07.rs"#,
        r#"fn noise_07(){ payment(); }
"#,
    ),
    (
        r#"packages/large/n08.rs"#,
        r#"fn noise_08(){ payment(); }
"#,
    ),
    (
        r#"packages/large/n09.rs"#,
        r#"fn noise_09(){ payment(); }
"#,
    ),
    (
        r#"packages/large/n10.rs"#,
        r#"fn noise_10(){ payment(); }
"#,
    ),
    (
        r#"packages/large/n11.rs"#,
        r#"fn noise_11(){ payment(); }
"#,
    ),
    (
        r#"packages/large/n12.rs"#,
        r#"fn noise_12(){ payment(); }
"#,
    ),
    (
        r#"packages/large/n13.rs"#,
        r#"fn noise_13(){ payment(); }
"#,
    ),
    (
        r#"packages/large/n14.rs"#,
        r#"fn noise_14(){ payment(); }
"#,
    ),
    (
        r#"packages/large/n15.rs"#,
        r#"fn noise_15(){ payment(); }
"#,
    ),
    (
        r#"packages/large/n16.rs"#,
        r#"fn noise_16(){ payment(); }
"#,
    ),
    (
        r#"packages/large/n17.rs"#,
        r#"fn noise_17(){ payment(); }
"#,
    ),
    (
        r#"packages/large/n18.rs"#,
        r#"fn noise_18(){ payment(); }
"#,
    ),
    (
        r#"packages/large/n19.rs"#,
        r#"fn noise_19(){ payment(); }
"#,
    ),
    (
        r#"packages/large/n20.rs"#,
        r#"fn noise_20(){ payment(); }
"#,
    ),
    (
        r#"packages/large/n21.rs"#,
        r#"fn noise_21(){ payment(); }
"#,
    ),
    (
        r#"packages/large/n22.rs"#,
        r#"fn noise_22(){ payment(); }
"#,
    ),
    (
        r#"packages/large/n23.rs"#,
        r#"fn noise_23(){ payment(); }
"#,
    ),
    (
        r#"packages/large/n24.rs"#,
        r#"fn noise_24(){ payment(); }
"#,
    ),
    (
        r#"packages/large/n25.rs"#,
        r#"fn noise_25(){ payment(); }
"#,
    ),
    (
        r#"packages/large/n26.rs"#,
        r#"fn noise_26(){ payment(); }
"#,
    ),
    (
        r#"packages/large/n27.rs"#,
        r#"fn noise_27(){ payment(); }
"#,
    ),
    (
        r#"packages/large/n28.rs"#,
        r#"fn noise_28(){ payment(); }
"#,
    ),
    (
        r#"packages/large/n29.rs"#,
        r#"fn noise_29(){ payment(); }
"#,
    ),
    (
        r#"packages/large/n30.rs"#,
        r#"fn noise_30(){ payment(); }
"#,
    ),
    (
        r#"packages/small/one.rs"#,
        r#"fn small_only(){ payment(); /* padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding */ }
"#,
    ),
];
pub struct Fixture {
    pub directory: tempfile::TempDir,
    pub engine: Engine,
    pub snapshot: Snapshot,
    pub graph: Graph,
    pub texts: BTreeMap<String, String>,
}
pub fn captured(texts: BTreeMap<String, String>, fixed_workspace: bool) -> Fixture {
    let directory = tempfile::tempdir().unwrap();
    for (path, text) in &texts {
        let target = directory.path().join(path);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(target, text).unwrap();
    }
    let mut engine = Engine::open(directory.path()).unwrap();
    engine.capture_epoch = AnalyzerEpoch::V9;
    if fixed_workspace {
        let id = hash("owned-monorepo-retrieval-legacy-oracle-v1");
        engine
            .store
            .db
            .execute("UPDATE meta SET value=?1 WHERE key='workspace'", [&id])
            .unwrap();
        engine.store.workspace_id = id;
    }
    let (snapshot, graph, _, _) = engine.index(&Deadline::new(120000)).unwrap();
    for (path, text) in &texts {
        let file = snapshot
            .files
            .iter()
            .find(|f| f.path == *path)
            .unwrap_or_else(|| panic!("Fixture path was not captured: {path}"));
        assert_eq!(file.size, text.len());
        assert_eq!(file.hash, hash(text));
    }
    Fixture {
        directory,
        engine,
        snapshot,
        graph,
        texts,
    }
}
pub fn literal31() -> Fixture {
    assert_eq!(SOURCES.len(), 31);
    captured(
        MARKERS
            .iter()
            .chain(SOURCES)
            .map(|(p, s)| (p.to_string(), s.to_string()))
            .collect(),
        true,
    )
}
pub fn query() -> Value {
    json!({"query":"payment","limit":2,"perFileLimit":200,"graphWeight":0,"relativePath":"packages","pathGlob":"**/*.rs","includeBody":true})
}
pub fn state(engine: &Engine) -> (i64, Vec<String>) {
    let changes = engine
        .store
        .db
        .query_row("SELECT total_changes()", [], |r| r.get(0))
        .unwrap();
    let mut blobs = fs::read_dir(engine.store.dir.join("blobs"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    blobs.sort();
    (changes, blobs)
}
pub fn padded_json(json: &str, bytes: usize) -> String {
    assert!(json.len() <= bytes);
    let mut out = json.to_string();
    out.extend(std::iter::repeat_n(' ', bytes - json.len()));
    out
}
pub fn eight_package_markers() -> BTreeMap<String, String> {
    let mut texts = BTreeMap::from([(
        "package.json".into(),
        padded_json("{\"workspaces\":[\"*\"]}", 800000),
    )]);
    for s in ["a", "b", "c", "d", "e", "f", "g", "h"] {
        texts.insert(
            format!("{s}/package.json"),
            padded_json(&format!("{{\"name\":\"{s}\"}}"), 400000),
        );
    }
    assert_eq!(texts.values().map(String::len).sum::<usize>(), 4_000_000);
    texts
}
pub fn function_body(name: &str, bytes: usize) -> String {
    let prefix = format!("fn {name}(){{ payment(); /*");
    let suffix = "*/ }";
    assert!(prefix.len() + suffix.len() <= bytes);
    format!(
        "{prefix}{}{suffix}",
        " ".repeat(bytes - prefix.len() - suffix.len())
    )
}
pub fn nested_body(parent: &str, child: &str, bytes: usize) -> String {
    let head = format!("fn {parent}(){{ payment(); /*");
    let mut out = format!("{head}{}*/", " ".repeat(100 - head.len() - 2));
    assert_eq!(out.len(), 100);
    let nested = function_body(child, 1_000_000);
    out.push_str(&nested);
    out.push_str("/*");
    out.push_str(&" ".repeat(bytes - out.len() - 4));
    out.push_str("*/ }");
    assert_eq!(out.len(), bytes);
    out
}
