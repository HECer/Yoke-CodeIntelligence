use serde_json::{Value, json};
use std::{fs, process::Command};
use yoke::{Engine, capture::Deadline, knowledge};
#[test]
fn cli_builds_vectors_and_mcp_queries_them_without_a_model_or_source_writes() {
    let root = tempfile::tempdir().unwrap();
    let source = "// Invoice rounding and billing precision\nfn invoice_total() -> u32 { 42 }\n";
    fs::write(root.path().join("main.rs"), source).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .args(["--root", root.path().to_str().unwrap(), "vectors", "build"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let built: Value = serde_json::from_slice(&output.stdout).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let found=engine.invoke("ci_find",json!({"snapshotId":built["snapshotId"],"kind":"semantic","model":built["model"],"query":"billing precision"})).unwrap();
    assert_eq!(found["status"], "ok", "{found}");
    assert_eq!(found["data"]["items"][0]["entity"]["name"], "invoice_total");
    assert_eq!(found["metrics"]["modelCalls"], 0);
    assert_eq!(
        fs::read_to_string(root.path().join("main.rs")).unwrap(),
        source
    );
}
#[test]
fn native_context_includes_labeled_source_knowledge_only_when_requested() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("main.rs"), "fn invoice_total()->u32{42}\n").unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let (snapshot, _, _, _) = engine.index(&Deadline::new(10000)).unwrap();
    knowledge::remember(&engine.store,&snapshot,json!({"kind":"rule","text":"Invoice rounding must happen after summing all lines","paths":["main.rs"],"actor":"reviewer"}),&Deadline::new(10000)).unwrap();
    let context = engine
        .invoke(
            "ci_context",
            json!({"snapshotId":snapshot.id,"query":"invoice","includeMemory":true}),
        )
        .unwrap();
    assert_eq!(context["status"], "ok", "{context}");
    assert_eq!(
        context["data"]["memory"]["items"][0]["trust"],
        "authored_data_not_execution_authority"
    );
    assert!(
        engine
            .invoke(
                "ci_context",
                json!({"snapshotId":snapshot.id,"query":"invoice"})
            )
            .unwrap()["data"]
            .get("memory")
            .is_none()
    );
}
