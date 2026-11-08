use serde_json::json;
use std::fs;
use tempfile::TempDir;
use yoke::Engine;

fn fixture() -> (TempDir, Engine) {
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("invoice.rs"), "pub fn calculateInvoiceTotal() -> u32 { 42 }\npub fn checkout() -> u32 { calculateInvoiceTotal() }\n").unwrap();
    fs::write(
        root.path().join("guide.md"),
        "# Billing\n\nWe preserve rounding precision until the invoice is finalized.\n",
    )
    .unwrap();
    let engine = Engine::open(root.path()).unwrap();
    (root, engine)
}

#[test]
fn identifier_components_and_document_body_are_retrievable_from_the_snapshot() {
    let (root, mut engine) = fixture();
    let found = engine
        .invoke("ci_find", json!({"query":"invoice","kind":"symbol"}))
        .unwrap();
    assert_eq!(
        found["data"]["items"][0]["name"], "calculateInvoiceTotal",
        "{found}"
    );
    fs::write(root.path().join("guide.md"), "# Completely changed\n").unwrap();
    let docs = engine.invoke("ci_find", json!({"snapshotId":found["snapshotId"],"query":"rounding precision","kind":"document"})).unwrap();
    assert_eq!(docs["status"], "ok", "{docs}");
    assert_eq!(docs["data"]["items"][0]["path"], "guide.md");
    assert!(
        docs["data"]["items"][0]["text"]
            .as_str()
            .unwrap()
            .contains("rounding precision")
    );
    let context = engine
        .invoke(
            "ci_context",
            json!({"snapshotId":found["snapshotId"],"query":"rounding precision"}),
        )
        .unwrap();
    assert!(
        context["data"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["path"] == "guide.md"),
        "{context}"
    );
}

#[test]
fn document_fts_normalization_keeps_exact_diacritic_and_punctuation_evidence() {
    let root = TempDir::new().unwrap();
    let text = "# Café\nA café handles résumés and request_id values.\n";
    fs::write(root.path().join("guide.md"), text).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    for term in ["cafe", "resume", "request_id", "id"] {
        let result = engine
            .invoke("ci_find", json!({"kind":"document","query":term}))
            .unwrap();
        assert_eq!(result["status"], "ok", "{result}");
        let item = &result["data"]["items"][0];
        assert_eq!(
            item["text"].as_str().unwrap(),
            &text[item["startByte"].as_u64().unwrap() as usize
                ..item["endByte"].as_u64().unwrap() as usize]
        );
    }
    assert_eq!(
        engine
            .invoke("ci_context", json!({"query":"cafe"}))
            .unwrap()["status"],
        "ok"
    );
}

#[test]
fn common_document_terms_cannot_amplify_unbounded_highlight_work() {
    let root = TempDir::new().unwrap();
    let text = "documentation ".repeat(7100);
    for index in 0..4 {
        fs::write(root.path().join(format!("doc{index}.md")), &text).unwrap();
    }
    let mut engine = Engine::open(root.path()).unwrap();
    let (snapshot, _, _, _) = engine.index(&yoke::capture::Deadline::new(10000)).unwrap();
    let start = std::time::Instant::now();
    let result=engine.invoke("ci_find",json!({"snapshotId":snapshot.id,"kind":"document","query":"documentation","limit":1,"timeoutMs":10})).unwrap();
    assert!(
        start.elapsed() < std::time::Duration::from_secs(1),
        "Document query outlived deadline: {:?}; {}",
        start.elapsed(),
        result["status"]
    );
}

#[test]
fn context_plan_find_references_read_select_preserves_exact_old_source() {
    let (root, mut engine) = fixture();
    let indexed = engine.invoke("ci_status", json!({"mode":"index"})).unwrap();
    fs::write(
        root.path().join("invoice.rs"),
        "fn deleted_old_symbols() {}\n",
    )
    .unwrap();
    let result = engine.invoke("ci_context", json!({"snapshotId":indexed["snapshotId"],"plan":[
        {"op":"find","query":"calculateInvoiceTotal","limit":5},
        {"op":"filter","field":"name","includes":"calculateInvoiceTotal"},
        {"op":"references"}, {"op":"filter","field":"name","includes":"checkout"},
        {"op":"read"}, {"op":"select","fields":["path","contentHash","startByte","endByte","text"]},
        {"op":"limit","count":1}
    ]})).unwrap();
    assert_eq!(result["status"], "ok", "{result}");
    assert_eq!(result["data"]["steps"], 7);
    let item = &result["data"]["items"][0];
    assert!(
        item["text"].as_str().unwrap().contains("fn checkout()"),
        "{result}"
    );
    assert_eq!(item["startByte"], 45);
    assert_eq!(
        fs::read_to_string(root.path().join("invoice.rs")).unwrap(),
        "fn deleted_old_symbols() {}\n"
    );
}

#[test]
fn context_plan_rejects_execution_unknown_fields_and_over_budget_steps() {
    let (_root, mut engine) = fixture();
    for plan in [
        json!([{"op":"shell","command":"echo unsafe"}]),
        json!([{"op":"find","query":"invoice","command":"unsafe"}]),
        json!([{"op":"select","fields":["__proto__"]}]),
        json!([{"op":"limit","count":1001}]),
        json!(vec![json!({ "op":"read" }); 17]),
    ] {
        let result = engine.invoke("ci_context", json!({"plan":plan})).unwrap();
        assert_eq!(result["status"], "error", "{result}");
    }
}
