use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};
use tempfile::TempDir;
use yoke::{
    Engine,
    capture::Deadline,
    graph_extensions::{self, AnalysisOptions, VIEW_REPORT_KIND},
    graph_view::{self, BuildOptions},
    model::hash,
};

const A: &str = "import { b } from './b';\r\nimport { b as other } from './b';\r\nexport const a = () => b() + other();\r\n";
const B: &str = "export { a } from './a';\r\nexport const b = () => 2;\r\n";

fn write_fixture(root: &Path) {
    for (name, content) in [
        ("a.ts", A),
        ("b.ts", B),
        (
            "c.ts",
            "import { type D } from './d';\nexport type C = D;\n",
        ),
        (
            "d.ts",
            "export { type C } from './c';\nexport type D = string;\n",
        ),
        (
            "e.ts",
            "export async function go() { return import('./f'); }\n",
        ),
        ("f.ts", "export { go } from './e';\n"),
        (
            "g.ts",
            "import type { H } from './h';\nexport type G = H;\n",
        ),
        (
            "h.ts",
            "export type { G } from './g';\nexport type H = string;\n",
        ),
        ("unresolved.ts", "import './missing';\n"),
        ("branch.ts", "import './amb';\n"),
        ("amb.ts", "export * from './branch';\n"),
        ("amb.tsx", "export * from './branch';\n"),
        ("x.py", "import y\n"),
        ("y.py", "from x import target\n"),
    ] {
        fs::write(root.join(name), content).unwrap();
    }
}

#[test]
fn native_analysis_publishes_exact_import_sites_and_historic_view_replays() {
    let root = TempDir::new().unwrap();
    let workspace = root.path().join("workspace");
    fs::create_dir(&workspace).unwrap();
    write_fixture(&workspace);
    let mut engine = Engine::open(&workspace).unwrap();
    let deadline = Deadline::new(60_000);
    let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
    let legacy = graph_extensions::analyze(
        &engine.store,
        &snapshot,
        &graph,
        &AnalysisOptions::default(),
        &deadline,
    )
    .unwrap();
    assert!(legacy.get("importCycles").is_none());

    let options_file = root.path().join("options.json");
    fs::write(&options_file, br#"{"importCycles":{}}"#).unwrap();
    let changed = "export const changed = 17;\n";
    fs::write(workspace.join("a.ts"), changed).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(&workspace)
        .args([
            "--snapshot",
            &snapshot.id,
            "--max-bytes",
            "4000000",
            "graph",
            "analyze",
            "--options",
        ])
        .arg(options_file)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["producer"], "yoke-import-cycles-report-1");
    let cycles = &report["importCycles"];
    assert_eq!(
        cycles["cycles"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["paths"].clone())
            .collect::<Vec<_>>(),
        vec![json!(["a.ts", "b.ts"]), json!(["x.py", "y.py"])]
    );
    assert_eq!(cycles["coverage"]["excludedTypeOnlyStatements"], 4);
    assert!(
        cycles["coverage"]["unresolvedOrAmbiguousModules"]
            .as_u64()
            .unwrap()
            >= 2
    );
    assert_eq!(cycles["truncated"], false);

    let forward = cycles["links"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["from"] == "a.ts" && row["to"] == "b.ts")
        .unwrap();
    assert_eq!(forward["sites"].as_array().unwrap().len(), 2);
    for site in forward["sites"].as_array().unwrap() {
        assert_eq!(site["contentHash"], hash(A));
        assert!(!site["observationIds"].as_array().unwrap().is_empty());
        assert!(
            A[site["startByte"].as_u64().unwrap() as usize
                ..site["endByte"].as_u64().unwrap() as usize]
                .contains("from './b'")
        );
    }
    let backward = cycles["links"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["from"] == "b.ts")
        .unwrap();
    assert_eq!(backward["sites"][0]["kind"], "static_reexport");
    assert_eq!(backward["sites"][0]["contentHash"], hash(B));
    let md = engine
        .store
        .read_artifact_limited(report["markdownArtifactId"].as_str().unwrap(), 1_000_000)
        .unwrap();
    assert!(
        String::from_utf8(md)
            .unwrap()
            .contains("a.ts → b.ts → a.ts")
    );

    let options: AnalysisOptions = serde_json::from_value(json!({"importCycles":{}})).unwrap();
    let view = graph_view::build(
        &engine.store,
        &snapshot,
        &graph,
        &BuildOptions::default(),
        &deadline,
    )
    .unwrap();
    let view_report =
        graph_extensions::analyze_view(&engine.store, &view, &options, &deadline).unwrap();
    assert_eq!(&view_report["importCycles"], cycles);
    let id = view_report["artifactId"].as_str().unwrap();
    let stored: Value = engine.store.get_record(VIEW_REPORT_KIND, id).unwrap();
    let checked =
        graph_extensions::validate_view_report_record(&engine.store, id, &stored, &deadline)
            .unwrap();
    assert_eq!(checked.snapshot_id, snapshot.id);
    let again = graph_extensions::analyze(
        &engine.store,
        &snapshot,
        &graph,
        &AnalysisOptions::default(),
        &deadline,
    )
    .unwrap();
    assert_eq!(legacy["artifactId"], again["artifactId"]);
    assert_eq!(fs::read_to_string(workspace.join("a.ts")).unwrap(), changed);
}

#[test]
fn distinguishes_output_and_search_limits_and_preserves_rotation() {
    let root = TempDir::new().unwrap();
    write_fixture(root.path());
    let mut engine = Engine::open(root.path()).unwrap();
    let deadline = Deadline::new(60_000);
    let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
    let analyze = |limits| {
        let selected = serde_json::from_value(json!({"importCycles":limits})).unwrap();
        graph_extensions::analyze(&engine.store,&snapshot,&graph,&selected,&deadline).unwrap()["importCycles"].clone()
    };
    let cut = analyze(json!({"limit":1}));
    assert_eq!(cut["cycles"][0]["paths"], json!(["a.ts", "b.ts"]));
    assert_eq!(cut["resultLimitReached"], true);
    assert_eq!(cut["searchBudgetReached"], false);
    let exhausted = analyze(json!({"maxSearchSteps":1}));
    assert_eq!(exhausted["searchBudgetReached"], true);
    assert_eq!(exhausted["cycles"], json!([]));
    assert_eq!(exhausted["resultLimitReached"], false);
    assert_eq!(exhausted["truncated"], true);
    let short = analyze(json!({"maxLength":1}));
    assert_eq!(short["cycles"], json!([]));
    assert_eq!(short["truncated"], false);
}
