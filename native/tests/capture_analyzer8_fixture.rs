//! Run only before the epoch migration, against unchanged analyzer-8 production code.
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::Path};
use yoke::{Engine, capture::Deadline, graph_collections::Registry};

fn rows(engine: &Engine, table: &str) -> Vec<Value> {
    let sql = if table == "records" {
        "SELECT kind,id,value FROM records ORDER BY kind,id".to_owned()
    } else {
        format!(
            "SELECT {},value FROM {table} ORDER BY 1",
            if table == "cache" { "key" } else { "id" }
        )
    };
    engine.store.db.prepare(&sql).unwrap().query_map([], |row| {
        if table == "records" {
            Ok(json!({"kind":row.get::<_,String>(0)?,"id":row.get::<_,String>(1)?,"value":row.get::<_,String>(2)?}))
        } else {
            Ok(json!({"id":row.get::<_,String>(0)?,"value":row.get::<_,String>(1)?}))
        }
    }).unwrap().map(Result::unwrap).collect()
}

fn capture_case(base: &Path, name: &str, configured: bool) -> Value {
    let root = base.join(name);
    fs::create_dir_all(root.join("ignored")).unwrap();
    fs::create_dir_all(root.join("node_modules/pkg")).unwrap();
    for (path, text) in [
        (
            "main.rs",
            "// Unicode: λ 🦀\nfn target(value: i32) {}\nfn caller() { let f = target; target(1); }\n",
        ),
        (
            "main.ps1",
            "function Invoke-Target { param($Value) $Value }\nfunction Invoke-Caller { Invoke-Target 1 }\n",
        ),
        (
            "main.lisp",
            "(defun target (value) value)\n(defun caller () (target 1) #'target)\n",
        ),
        (
            "main.sv",
            "module demo; function int target(input int x); return x; endfunction initial target(1); endmodule\n",
        ),
        (
            "main.ts",
            "export function target(x: number): number { return x; }\nexport const caller = () => target(1);\n",
        ),
        (
            "main.vue",
            "<script lang=\"ts\">function target() {}\nfunction caller() { target(); }</script>\n",
        ),
        ("custom.fixture", "fn configured_target() {}\n"),
        (".gitignore", "ignored/hidden.rs\nnode_modules/\n"),
        ("ignored/hidden.rs", "fn hidden_target() {}\n"),
        ("ignored/visible.rs", "fn visible_target() {}\n"),
        (
            "node_modules/pkg/index.js",
            "export function dependency() {}\n",
        ),
    ] {
        fs::write(root.join(path), text).unwrap();
    }
    let deadline = Deadline::new(60000);
    let mut engine = Engine::open(&root).unwrap();
    if configured {
        engine.capture_options = serde_json::from_value(json!({
            "languageOverrides":{"fixture":"rust"},
            "includeIgnoredDirs":["ignored"],
            "readOnlyDependencyDirs":["node_modules"]
        }))
        .unwrap();
    }
    let (disk, graph, _, _) = engine.index(&deadline).unwrap();
    let file = disk.files.iter().find(|f| f.path == "main.rs").unwrap();
    yoke::changes::prepare(
        &mut engine.store,
        &disk,
        &graph,
        &json!({"edits":[{
            "kind":"replace","path":"main.rs","expectedHash":file.hash,
            "startByte":0,"endByte":0,"text":"// derived fixture\n"
        }]}),
        &deadline,
    )
    .unwrap();
    let session = yoke::editor::open(&mut engine, &deadline).unwrap();
    yoke::editor::update(&engine, &session.id, &json!({
        "path":"main.rs","version":1,"text":"fn editor_target() {}\nfn editor_caller() { editor_target(); }\n"
    }), &deadline).unwrap();
    yoke::editor::capture(&mut engine, &session.id, &deadline).unwrap();

    let registry_root = base.join(format!("{name}-archive"));
    fs::create_dir_all(&registry_root).unwrap();
    let registry = Registry::open(&registry_root).unwrap();
    let snapshot_rows = rows(&engine, "snapshots");
    for row in &snapshot_rows {
        let snapshot: yoke::model::Snapshot =
            serde_json::from_str(row["value"].as_str().unwrap()).unwrap();
        let graph = engine.store.graph(&snapshot.id).unwrap();
        let view = yoke::graph_view::build(
            &engine.store,
            &snapshot,
            &graph,
            &yoke::graph_view::BuildOptions::default(),
            &deadline,
        )
        .unwrap();
        registry
            .register_view(&snapshot.id, &engine.store, &view, &deadline)
            .unwrap();
    }
    let mut sources = BTreeMap::new();
    for row in &snapshot_rows {
        let snapshot: yoke::model::Snapshot =
            serde_json::from_str(row["value"].as_str().unwrap()).unwrap();
        for file in snapshot.files {
            sources.insert(
                file.hash.clone(),
                String::from_utf8(engine.store.read_blob(&file.hash).unwrap()).unwrap(),
            );
        }
    }
    drop(registry);
    let db = rusqlite::Connection::open(registry_root.join("yoke-collection.sqlite")).unwrap();
    let registrations: Vec<Value> = db.prepare("SELECT name,id,payload FROM registrations ORDER BY name").unwrap()
        .query_map([], |r| Ok(json!({"name":r.get::<_,String>(0)?,"id":r.get::<_,String>(1)?,"payload":r.get::<_,String>(2)?})))
        .unwrap().map(Result::unwrap).collect();
    json!({"name":name,"workspaceId":engine.store.workspace_id,"snapshots":snapshot_rows,
        "graphs":rows(&engine,"graphs"),"cache":rows(&engine,"cache"),"records":rows(&engine,"records"),
        "sources":sources,"registrations":registrations})
}

#[test]
#[ignore = "one-time authentic analyzer-8 capture; requires YOKE_CAPTURE_ANALYZER8_DIR"]
fn capture_authentic_analyzer8_before_migration() {
    assert_eq!(yoke::model::ANALYZER, "yoke-tree-sitter-8");
    let base =
        std::env::var_os("YOKE_CAPTURE_ANALYZER8_DIR").expect("explicit output directory required");
    let base = Path::new(&base);
    fs::create_dir_all(base).unwrap();
    let bundle = json!({"schema":1,"analyzer":yoke::model::ANALYZER,
        "cases":[capture_case(base,"default",false),capture_case(base,"configured",true)]});
    let path = base.join("authentic-analyzer8.json");
    fs::write(&path, serde_json::to_vec(&bundle).unwrap()).unwrap();
    println!(
        "authentic analyzer-8 fixture: {} sha256={}",
        path.display(),
        yoke::model::identity(&bundle)
    );
}

#[test]
#[ignore = "authentic V1 receiver-8 archive capture; requires YOKE_CAPTURE_ANALYZER8_DIR"]
fn capture_authentic_federation_receiver8_before_receipt_migration() {
    use ed25519_dalek::SigningKey;
    use yoke::federation::{self, ExportScope, ImportGrant, Scope};
    assert_eq!(yoke::model::ANALYZER, "yoke-tree-sitter-8");
    let base = std::env::var_os("YOKE_CAPTURE_ANALYZER8_DIR").unwrap();
    let base = Path::new(&base);
    let sender_root = base.join("federation-sender8");
    let receiver_root = base.join("federation-receiver8");
    fs::create_dir_all(&sender_root).unwrap();
    fs::create_dir_all(&receiver_root).unwrap();
    fs::write(
        sender_root.join("main.rs"),
        "fn target() {}\nfn caller() { let f = target; target(); }\n",
    )
    .unwrap();
    let mut sender = Engine::open(&sender_root).unwrap();
    let receiver = Engine::open(&receiver_root).unwrap();
    let deadline = Deadline::new(20000);
    let (snapshot, _, _, _) = sender.index(&deadline).unwrap();
    let secret = [97u8; 32];
    let scope = Scope {
        tenant_id: "epoch-test".into(),
        project_id: "fixture".into(),
        sender_id: "sender".into(),
        key_id: "fixture-key".into(),
    };
    let grant = ImportGrant {
        scope: scope.clone(),
        public_key: hex::encode(SigningKey::from_bytes(&secret).verifying_key().to_bytes()),
        sender_workspace_id: snapshot.workspace_id.clone(),
        exclude: vec![],
    };
    let envelope = federation::export_snapshot(
        &sender.store,
        &snapshot,
        &ExportScope {
            scope,
            ttl_ms: 604800000,
        },
        &secret,
        &deadline,
    )
    .unwrap();
    let imported = federation::import_snapshot(&receiver, envelope, &grant, &deadline).unwrap();
    let id = imported["remoteId"].as_str().unwrap();
    let grants = BTreeMap::from([("source".to_string(), grant.clone())]);
    let prepared = yoke::federation_archive::FederationArchiveAccess::new(&receiver, &grants)
        .capture("source", id, &deadline)
        .unwrap();
    let sources: BTreeMap<_, _> = prepared
        .sources()
        .iter()
        .map(|(path, bytes)| (path.clone(), String::from_utf8(bytes.clone()).unwrap()))
        .collect();
    let data = json!({"schema":1,"analyzer":yoke::model::ANALYZER,"receiverWorkspaceId":receiver.store.workspace_id,
        "grant":grant,"snapshot":prepared.snapshot(),"graphRaw":serde_json::to_string(prepared.graph()).unwrap(),
        "bindingRaw":serde_json::to_string(prepared.binding()).unwrap(),
        "provenanceRaw":serde_json::to_string(&prepared.binding().provenance(&deadline).unwrap()).unwrap(),
        "records":rows(&receiver,"records"),"sources":sources});
    let path = base.join("authentic-federation-receiver8.json");
    fs::write(&path, serde_json::to_vec(&data).unwrap()).unwrap();
    println!(
        "authentic receiver-8 V1 fixture: {} sha256={}",
        path.display(),
        yoke::model::identity(&data)
    );
}
