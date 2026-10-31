//! Test-first public admission cases. No forged View fields or private claim producer.
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::Path,
    process::{Command, Output, Stdio},
};
use tempfile::TempDir;
use yoke::{
    Engine,
    capture::Deadline,
    graph_view::{self, BuildOptions, View},
    model::{hash, identity},
    project_session::{HostProfile, Session},
};

const SOURCE: &str = "// \u{3a9} e\u{301}\r\nfn \u{3b1}(){ stronger(); weaker(); }\r\nfn stronger(){ omega(); }\r\nfn weaker(){ omega(); }\r\nfn omega(){}\r\n";
const POLICY: &str = "weakest_declared_source_claims_v1";

struct Fixture {
    session: Session,
    base: View,
    root: TempDir,
    host: TempDir,
}

fn fixture() -> Fixture {
    assert_eq!(SOURCE.len(), 111);
    assert_eq!(
        hash(SOURCE.as_bytes()),
        "1cd640678892e21fb431a19d99b5bff3ce1b5c64b3e0828dd080169acacd4300"
    );
    let root = TempDir::new().unwrap();
    let host = TempDir::new().unwrap();
    fs::write(root.path().join("main.rs"), SOURCE).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let (snapshot, graph, _, _) = engine.index(&Deadline::new(30000)).unwrap();
    let base = graph_view::build(
        &engine.store,
        &snapshot,
        &graph,
        &BuildOptions {
            structural_kinds: vec![],
            ..BuildOptions::default()
        },
        &Deadline::new(30000),
    )
    .unwrap();
    assert_eq!(base.base_graph_hash, identity(&graph));
    let calls: Vec<_> = base
        .observations
        .iter()
        .filter(|edge| edge["relation"] == "calls")
        .collect();
    assert_eq!(
        calls.len(),
        4,
        "The real source has exactly four original calls"
    );
    let mut call_ranges = BTreeSet::new();
    let mut call_ids = BTreeSet::new();
    for edge in calls {
        assert!(call_ids.insert(edge["id"].as_str().unwrap()));
        let site_ids = edge["evidenceIds"].as_array().unwrap();
        assert_eq!(site_ids.len(), 1);
        let site = base
            .evidence
            .iter()
            .find(|site| site["id"] == site_ids[0])
            .unwrap();
        assert_eq!(site["path"], "main.rs");
        assert_eq!(site["contentHash"], hash(SOURCE.as_bytes()));
        let a = site["startByte"].as_u64().unwrap() as usize;
        let b = site["endByte"].as_u64().unwrap() as usize;
        assert!(["stronger()", "weaker()", "omega()"].contains(&&SOURCE[a..b]));
        assert!(call_ranges.insert((a, b)));
    }
    assert_eq!(
        call_ranges,
        BTreeSet::from([(20, 30), (32, 40), (60, 67), (85, 92)])
    );
    assert_eq!(call_ids.len(), 4);
    drop(call_ids);
    let session = Session::open(
        engine,
        HostProfile::default(),
        Some("source-claim-admission"),
        &Deadline::new(30000),
    )
    .unwrap();
    Fixture {
        session,
        base,
        root,
        host,
    }
}

fn node(f: &Fixture, name: &str) -> String {
    let rows: Vec<_> = f
        .base
        .nodes
        .iter()
        .filter(|n| n["kind"] == "function" && n["name"] == name)
        .collect();
    assert_eq!(rows.len(), 1);
    rows[0]["id"].as_str().unwrap().to_owned()
}

fn call(f: &Fixture, source: &str, target: &str) -> Value {
    let source = node(f, source);
    let target = node(f, target);
    let rows: Vec<_> = f
        .base
        .observations
        .iter()
        .filter(|edge| {
            edge["relation"] == "calls" && edge["source"] == source && edge["target"] == target
        })
        .collect();
    assert_eq!(rows.len(), 1);
    (*rows[0]).clone()
}

/// Raw attributes are inserted verbatim so a large integer never passes through f64.
fn document(f: &Fixture, attributes: &BTreeMap<String, String>) -> Vec<u8> {
    let mut entries = Vec::new();
    for edge in &f.base.observations {
        let id = edge["id"].as_str().unwrap();
        let attrs = attributes
            .get(id)
            .map(String::as_str)
            .unwrap_or(r#"{"confidence":"EXTRACTED"}"#);
        entries.push(format!(
            r#"{{"observationId":{},"source":{},"target":{},"evidenceIds":{},"attributes":{attrs}}}"#,
            edge["id"], edge["source"], edge["target"], edge["evidenceIds"],
        ));
    }
    format!(
        "{{\"schemaVersion\":1,\"snapshotId\":{},\"workspaceId\":{},\"baseGraphHash\":{},\"claims\":[{}]}}\r\n",
        json!(f.base.snapshot_id),json!(f.base.workspace_id),json!(f.base.base_graph_hash),entries.join(","),
    ).into_bytes()
}

fn cli(f: &Fixture, words: &[&str], input: Option<&Path>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_yoke"));
    command
        .arg("--root")
        .arg(f.root.path())
        .args(["--snapshot", &f.base.snapshot_id, "--timeout", "30000"])
        .args(words);
    if let Some(path) = input {
        command.arg("--input").arg(path);
    }
    let output = command.output().unwrap();
    assert!(output.stdout.len() <= 256_000 && output.stderr.len() <= 64_000);
    output
}

fn import(f: &Fixture, raw: &[u8]) -> (String, std::path::PathBuf) {
    let path = f.host.path().join("selected-claims.json");
    fs::write(&path, raw).unwrap();
    let before = f.session.engine.store.latest().unwrap();
    let output = cli(f, &["graph", "import-source-claims"], Some(&path));
    assert!(
        output.status.success(),
        "The public host source-claim importer must admit this exact snapshot-bound document before numeric policy assertions can run; exit={}; stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stderr),
    );
    let result = yoke::json::parse(&output.stdout, 256_000).unwrap();
    let id = result["artifactId"].as_str().unwrap().to_owned();
    assert!(yoke::model::valid_id(&id));
    assert_eq!(result["sourceDocumentHash"], hash(raw));
    assert_eq!(result["snapshotId"], f.base.snapshot_id);
    assert_eq!(result["workspaceId"], f.base.workspace_id);
    assert_eq!(result["baseGraphHash"], f.base.base_graph_hash);
    assert_eq!(
        f.session.engine.store.latest().unwrap(),
        before,
        "Admission must not capture a new snapshot"
    );
    assert_eq!(fs::read(&path).unwrap(), raw);
    let payload = f
        .session
        .engine
        .store
        .read_artifact_limited(&id, 2_000_000)
        .unwrap();
    let retained = yoke::json::parse(&payload, 2_000_000).unwrap();
    assert_eq!(retained["sourceDocumentHash"], hash(raw));
    assert_eq!(
        retained["sourceDocument"].as_str().unwrap().as_bytes(),
        raw,
        "The original host document, including numeric lexemes and CRLF, must survive publication byte-exact"
    );
    (id, path)
}

fn selected(f: &Fixture, artifact: &str) -> View {
    let options = f.host.path().join("selected-view.json");
    fs::write(
        &options,
        serde_json::to_vec(&json!({
            "structuralKinds":[],
            "sourceClaimOverlayIds":[artifact],
        }))
        .unwrap(),
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(f.root.path())
        .args([
            "--snapshot",
            &f.base.snapshot_id,
            "--timeout",
            "30000",
            "graph",
            "build",
            "--options",
        ])
        .arg(&options)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "Host selection of an admitted source-claim overlay must publish a checked View: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report = yoke::json::parse(&output.stdout, 256_000).unwrap();
    let view = graph_view::load(
        &f.session.engine.store,
        report["viewId"].as_str().unwrap(),
        &Deadline::new(30000),
    )
    .unwrap();
    assert_eq!(view.snapshot_id, f.base.snapshot_id);
    assert_eq!(view.base_graph_hash, f.base.base_graph_hash);
    assert_eq!(view.nodes, f.base.nodes);
    assert_eq!(
        view.observations, f.base.observations,
        "Imported declarations do not rewrite canonical observations or directions"
    );
    assert_eq!(view.evidence, f.base.evidence);
    assert_eq!(
        view.overlays
            .iter()
            .filter(|overlay| overlay["artifactId"] == artifact)
            .count(),
        1
    );
    view
}

fn arguments(f: &Fixture, view: &View) -> Value {
    json!({"graphViewId":view.id,"source":node(f,"\u{3b1}"),"target":node(f,"omega"),"max_hops":2,"path_policy":POLICY})
}

fn query(f: &mut Fixture, view: &View) -> Value {
    let args = arguments(f, view);
    let result = yoke::graphify_adapter::invoke(
        &mut f.session,
        "shortest_path",
        &args,
        &Deadline::new(30000),
    )
    .unwrap();
    assert_eq!(result["found"], true);
    assert_eq!(result["pathPolicy"], POLICY);
    assert_eq!(result["viewId"], view.id);
    assert_eq!(result["snapshotId"], f.base.snapshot_id);
    assert_eq!(result["hops"], 2);
    assert_eq!(result["complete"], false);
    result
}

fn nested_claims(f: &Fixture) -> BTreeMap<String, String> {
    BTreeMap::from([
        (call(f,"\u{3b1}","stronger")["id"].as_str().unwrap().to_owned(),r#"{"confidence":"INFERRED","confidence_score":0.5,"source_note":"stronger original declaration"}"#.into()),
        (call(f,"\u{3b1}","weaker")["id"].as_str().unwrap().to_owned(),r#"{"confidence":"INFERRED","confidence_score":0.9,"also_seen":[{"confidence":"INFERRED","confidence_score":0.2,"source_note":"contradictory retained copy \u03c0"},false,{"confidence":"INFERRED","confidence_score":true}]}"#.into()),
    ])
}

fn assert_weaker_route(f: &Fixture, result: &Value, branch: &str) {
    assert_eq!(result["pathSourceCategory"], "inferred");
    assert_eq!(
        result["pathObservationIds"],
        json!([
            call(f, "\u{3b1}", branch)["id"],
            call(f, branch, "omega")["id"]
        ])
    );
    let hops = result["pathHops"].as_array().unwrap();
    assert_eq!(hops.len(), 2);
    for (hop, (from, to)) in hops.iter().zip([("\u{3b1}", branch), (branch, "omega")]) {
        let original = call(f, from, to);
        assert_eq!(hop["traversalSourceId"], original["source"]);
        assert_eq!(hop["traversalTargetId"], original["target"]);
        assert_eq!(hop["selectedObservationId"], original["id"]);
        assert_eq!(hop["observationIds"], json!([original["id"]]));
        assert_eq!(hop["observations"], json!([original]));
        let sites: Vec<_> = f
            .base
            .evidence
            .iter()
            .filter(|site| {
                original["evidenceIds"]
                    .as_array()
                    .unwrap()
                    .contains(&site["id"])
            })
            .cloned()
            .collect();
        assert_eq!(sites.len(), 1);
        assert_eq!(
            hop["evidence"],
            json!(sites),
            "A hop must return its complete original source evidence in View order"
        );
    }
}

#[test]
fn public_host_admission_retains_numeric_copies_and_reloads_immutable_original_sources() {
    let mut f = fixture();
    let original = serde_json::to_vec(&f.base).unwrap();
    let raw = document(&f, &nested_claims(&f));
    let (artifact, host_input) = import(&f, &raw);
    let repeated = import(&f, &raw).0;
    assert_eq!(
        repeated, artifact,
        "Identical document admission is content-addressed"
    );
    let view = selected(&f, &artifact);
    let result = query(&mut f, &view);
    assert_weaker_route(&f, &result, "weaker");
    let mut control_attributes = nested_claims(&f);
    let weak_id = call(&f, "\u{3b1}", "weaker")["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let original_attributes = control_attributes[&weak_id].clone();
    assert_eq!(
        original_attributes
            .matches("\"confidence_score\":0.2")
            .count(),
        1
    );
    control_attributes.insert(
        weak_id,
        original_attributes.replacen("\"confidence_score\":0.2", "\"confidence_score\":0.95", 1),
    );
    let control_raw = document(&f, &control_attributes);
    let (control_artifact, _) = import(&f, &control_raw);
    assert_ne!(control_artifact, artifact);
    let control_view = selected(&f, &control_artifact);
    let control_result = query(&mut f, &control_view);
    assert_weaker_route(&f, &control_result, "stronger");
    assert_eq!(
        query(&mut f, &view),
        result,
        "Changing only the retained copy reverses the control route, without overwriting the original selected declarations"
    );
    let saved_view = serde_json::to_vec(&view).unwrap();
    let saved_graph =
        serde_json::to_vec(&f.session.engine.store.graph(&f.base.snapshot_id).unwrap()).unwrap();
    fs::write(
        &host_input,
        b"changed after admission; this is not new authority",
    )
    .unwrap();
    fs::write(f.root.path().join("main.rs"), "fn later_source(){}\r\n").unwrap();
    let (new_snapshot, _, _, _) = f.session.engine.index(&Deadline::new(30000)).unwrap();
    assert_ne!(new_snapshot.id, f.base.snapshot_id);
    assert_eq!(
        query(&mut f, &view),
        result,
        "An explicit old View must replay the retained document and source, without reading the changed host input"
    );
    assert_eq!(
        serde_json::to_vec(
            &graph_view::load(&f.session.engine.store, &view.id, &Deadline::new(30000)).unwrap()
        )
        .unwrap(),
        saved_view
    );
    assert_eq!(
        serde_json::to_vec(
            &graph_view::load(&f.session.engine.store, &f.base.id, &Deadline::new(30000)).unwrap()
        )
        .unwrap(),
        original
    );
    assert_eq!(
        serde_json::to_vec(&f.session.engine.store.graph(&f.base.snapshot_id).unwrap()).unwrap(),
        saved_graph
    );
    assert!(f.session.engine.servers.is_empty());
}

#[test]
fn admitted_integer_claims_above_u64_keep_their_exact_order_and_original_lexemes() {
    let mut f = fixture();
    let mut branches = ["stronger", "weaker"];
    branches.sort_by_key(|name| node(&f, name));
    let high = call(&f, "\u{3b1}", branches[0]);
    let low = call(&f, "\u{3b1}", branches[1]);
    let attrs = BTreeMap::from([
        (
            high["id"].as_str().unwrap().to_owned(),
            r#"{"confidence":"INFERRED","confidence_score":18446744073709551617}"#.to_owned(),
        ),
        (
            low["id"].as_str().unwrap().to_owned(),
            r#"{"confidence":"INFERRED","confidence_score":18446744073709551616}"#.to_owned(),
        ),
    ]);
    let raw = document(&f, &attrs);
    let (artifact, _) = import(&f, &raw);
    let view = selected(&f, &artifact);
    let result = query(&mut f, &view);
    assert_weaker_route(&f, &result, branches[1]);
    let swapped = BTreeMap::from([
        (
            high["id"].as_str().unwrap().to_owned(),
            r#"{"confidence":"INFERRED","confidence_score":18446744073709551616}"#.to_owned(),
        ),
        (
            low["id"].as_str().unwrap().to_owned(),
            r#"{"confidence":"INFERRED","confidence_score":18446744073709551617}"#.to_owned(),
        ),
    ]);
    let (swapped_artifact, _) = import(&f, &document(&f, &swapped));
    let swapped_view = selected(&f, &swapped_artifact);
    assert_eq!(swapped_view.nodes, view.nodes);
    assert_eq!(swapped_view.observations, view.observations);
    let swapped_result = query(&mut f, &swapped_view);
    assert_weaker_route(&f, &swapped_result, branches[0]);
}

#[test]
fn real_graphify_mcp_selects_admitted_claims_without_a_host_input_read_or_import_tool() {
    let f = fixture();
    let raw = document(&f, &nested_claims(&f));
    let (artifact, host_input) = import(&f, &raw);
    let view = selected(&f, &artifact);
    fs::remove_file(&host_input).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(f.root.path())
        .args([
            "--timeout",
            "30000",
            "serve-session",
            "--id",
            "admitted-claims-mcp",
            "--profile",
            "graphify",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let mut input = child.stdin.take().unwrap();
        for request in [
            json!({"jsonrpc":"2.0","id":1,"method":"initialize"}),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"shortest_path","arguments":arguments(&f,&view)}}),
            json!({"jsonrpc":"2.0","id":3,"method":"tools/list"}),
        ] {
            writeln!(input, "{request}").unwrap();
        }
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.len() <= 1_000_000 && output.stderr.len() <= 64_000);
    let rows: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| yoke::json::parse(line.as_bytes(), 256_000).unwrap())
        .collect();
    let row = rows.iter().find(|row| row["id"] == 2).unwrap();
    assert_eq!(row["result"]["isError"], false, "{}", row);
    let result = yoke::json::parse(
        row["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .as_bytes(),
        256_000,
    )
    .unwrap();
    assert_eq!(result["status"], "ok");
    assert_weaker_route(&f, &result["data"], "weaker");
    let listed = rows.iter().find(|row| row["id"] == 3).unwrap()["result"]["tools"]
        .as_array()
        .unwrap();
    assert_eq!(listed.len(), 10);
    let names: BTreeSet<_> = listed
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        BTreeSet::from([
            "query_graph",
            "get_node",
            "get_neighbors",
            "get_community",
            "god_nodes",
            "graph_stats",
            "shortest_path",
            "list_prs",
            "get_pr_impact",
            "triage_prs"
        ])
    );
    assert!(!host_input.exists());
    assert_eq!(
        fs::read(f.root.path().join("main.rs")).unwrap(),
        SOURCE.as_bytes()
    );
}

#[derive(Debug, PartialEq, Eq)]
struct PublishedState {
    snapshots: Vec<(String, String)>,
    graphs: Vec<(String, String)>,
    registrations: Vec<(String, String, String)>,
    blobs: BTreeMap<String, String>,
}

fn published_state(f: &Fixture) -> PublishedState {
    let pairs = |table: &str| {
        assert!(["snapshots", "graphs"].contains(&table));
        f.session
            .engine
            .store
            .db
            .prepare(&format!("SELECT id,value FROM {table} ORDER BY id"))
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<Vec<(String, String)>>>()
            .unwrap()
    };
    let registrations = f
        .session
        .engine
        .store
        .db
        .prepare("SELECT kind,id,value FROM records ORDER BY kind,id")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .unwrap()
        .collect::<rusqlite::Result<Vec<(String, String, String)>>>()
        .unwrap();
    let mut blobs = BTreeMap::new();
    for entry in fs::read_dir(f.session.engine.store.dir.join("blobs")).unwrap() {
        let entry = entry.unwrap();
        assert!(entry.file_type().unwrap().is_file());
        let name = entry.file_name().into_string().unwrap();
        assert!(yoke::model::valid_id(&name));
        let bytes = fs::read(entry.path()).unwrap();
        assert!(bytes.len() <= 2_000_000 && blobs.len() < 1000);
        assert_eq!(hash(&bytes), name);
        assert!(blobs.insert(name, hash(&bytes)).is_none());
    }
    PublishedState {
        snapshots: pairs("snapshots"),
        graphs: pairs("graphs"),
        registrations,
        blobs,
    }
}

#[test]
fn admitted_claim_scope_failures_refuse_without_registration_capture_or_source_change() {
    let f = fixture();
    let raw = document(&f, &nested_claims(&f));
    import(&f, &raw);
    let baseline = published_state(&f);
    let latest = f.session.engine.store.latest().unwrap();
    let doc: Value = yoke::json::parse(&raw, 1_000_000).unwrap();
    let mut invalid = Vec::new();
    for field in ["snapshotId", "workspaceId", "baseGraphHash"] {
        let mut bad = doc.clone();
        bad[field] = json!("f".repeat(64));
        invalid.push(serde_json::to_vec(&bad).unwrap());
    }
    for field in ["observationId", "source", "target"] {
        let mut bad = doc.clone();
        bad["claims"][0][field] = json!("e".repeat(64));
        invalid.push(serde_json::to_vec(&bad).unwrap());
    }
    let first_call = call(&f, "\u{3b1}", "stronger");
    let other_call = call(&f, "weaker", "omega");
    let claim_index = doc["claims"]
        .as_array()
        .unwrap()
        .iter()
        .position(|claim| claim["observationId"] == first_call["id"])
        .unwrap();
    for field in ["observationId", "source", "target", "evidenceIds"] {
        let mut bad = doc.clone();
        let value = if field == "observationId" {
            other_call["id"].clone()
        } else {
            other_call[field].clone()
        };
        assert_ne!(bad["claims"][claim_index][field], value);
        bad["claims"][claim_index][field] = value;
        invalid.push(serde_json::to_vec(&bad).unwrap());
    }
    let mut bad = doc.clone();
    bad["claims"][0]["evidenceIds"] = json!(["d".repeat(64)]);
    invalid.push(serde_json::to_vec(&bad).unwrap());
    let mut bad = doc.clone();
    bad["claims"][0]["attributes"] = json!(["not an edge attribute dictionary"]);
    invalid.push(serde_json::to_vec(&bad).unwrap());
    let mut bad = doc.clone();
    bad["unexpectedHostAuthority"] = json!({"execute":"never"});
    invalid.push(serde_json::to_vec(&bad).unwrap());
    let duplicate =
        String::from_utf8(raw.clone())
            .unwrap()
            .replacen("{", "{\"schemaVersion\":1,", 1);
    invalid.push(duplicate.into_bytes());
    for (ordinal, bytes) in invalid.into_iter().enumerate() {
        let path = f.host.path().join(format!("invalid-{ordinal}.json"));
        fs::write(&path, &bytes).unwrap();
        let output = cli(&f, &["graph", "import-source-claims"], Some(&path));
        assert!(
            !output.status.success(),
            "Malformed or foreign scope case{ordinal} was admitted"
        );
        assert!(
            output.stdout.is_empty(),
            "Refusal must not return a partial successful receipt"
        );
        assert_eq!(
            published_state(&f),
            baseline,
            "Refused scope case{ordinal} changed existing rows or published a registered/unregistered CAS blob"
        );
        assert_eq!(f.session.engine.store.latest().unwrap(), latest);
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(
            fs::read(f.root.path().join("main.rs")).unwrap(),
            SOURCE.as_bytes()
        );
    }
}
