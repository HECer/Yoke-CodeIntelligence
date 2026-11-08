//! Actual owned model-protocol process builds; receiver reads execute no model.
use super::*;
use receiver::interpretation as semantic;
use yoke::{
    meaning_build::{self, MeaningPolicy, MeaningSelection},
    meaning_view,
    providers::ProviderSpec,
};

struct Fixture {
    received: Received,
    root: tempfile::TempDir,
    engine: Engine,
    snapshot: String,
    host: tempfile::TempDir,
    graph: String,
    marker: std::path::PathBuf,
}
impl Fixture {
    fn new(mode: &str, selection: &str) -> Self {
        let mut sender = Source::new();
        let text = if selection == "nested" {
            "pub mod outer { pub fn café()->usize { 123456789 } }\r\n"
        } else {
            "pub fn café(value:usize)->usize { value+123456789 }\r\npub fn second()->usize { 2 }\r\nfn private_only()->usize { 73 }\r\n"
        };
        std::fs::write(sender.root.path().join("main.rs"), text).unwrap();
        sender.request.snapshot_id = sender.engine.index(&Deadline::new(20000)).unwrap().0.id;
        let received = Received::from_source(sender, |_| {});
        let (root, engine, snapshot) = received.local();
        let host = tempfile::tempdir().unwrap();
        let worker = host.path().join("owned-worker.cjs");
        let marker = host.path().join("calls.txt");
        std::fs::write(&worker, r#"
let raw='';process.stdin.on('data',b=>raw+=b);process.stdin.on('end',()=>{
 const fs=require('fs'),r=JSON.parse(raw),mode=process.argv[2],marker=process.argv[3];
 fs.appendFileSync(marker,r.requestId+'\n');
 const support=s=>({sourceId:s.anchor.sourceId,startByte:s.anchor.startByte,endByte:mode==='first-byte'?s.anchor.startByte+1:s.anchor.endByte});
 const all=r.payload.sources.map(support);
 let concepts=r.payload.sources.map((s,i)=>({key:'c'+i,label:'Increment policy',description:'Owned inference '+i,aliases:['Shared alias'],support:[support(s)]}));
 let relations=[],hyperedges=[];
 if(all.length>1){
  relations=[{source:'c0',target:'c1',relation:'uses',description:'Forward inference',support:all},{source:'c0',target:'c1',relation:'uses',description:'Parallel inference',support:all},{source:'c1',target:'c0',relation:'uses',description:'Reverse inference',support:all},{source:'c0',target:'c0',relation:'uses',description:'Self inference',support:[all[0]]}];
  hyperedges=[{relation:'coordinates',participants:[{role:'input',concept:'c1'},{role:'output',concept:'c1'},{role:'input',concept:'c0'}],description:'Repeated role and concept with distinct role/concept pairs',support:all}];
 }
 let coverage=r.payload.sources.map(s=>({sourceId:s.anchor.sourceId,outcome:'claims'}));
 if(mode==='no-claims'){concepts=[];relations=[];hyperedges=[];coverage=r.payload.sources.map(s=>({sourceId:s.anchor.sourceId,outcome:'no_claims'}));}
 process.stdout.write(JSON.stringify({schema:1,operation:r.operation,model:r.model,modelVersion:r.modelVersion,requestId:r.requestId,inputHash:r.inputHash,sourceBindings:r.payload.sources.map(s=>s.anchor),currency:r.currency,actualCostMicros:7,usage:{inputTokens:10,outputTokens:3,cachedInputTokens:2},data:{concepts,relations,hyperedges,coverage}}));
});
"#).unwrap();
        let snap = engine.store.snapshot(&snapshot).unwrap();
        let canonical = engine.store.graph(&snapshot).unwrap();
        let mut policy =
            MeaningPolicy::files(&snap, hash("owned receiver interpretation contract"));
        if selection != "file" {
            let names: &[&str] = match selection {
                "private" => &["private_only"],
                "nested" => &["café"],
                _ => &["café", "second"],
            };
            let ids = canonical
                .entities
                .iter()
                .filter(|e| e.kind == "function" && names.contains(&e.name.as_str()))
                .map(|e| e.id.clone())
                .collect::<Vec<_>>();
            assert_eq!(ids.len(), names.len());
            if ids.len() > 1 {
                policy.context_groups = vec![ids.clone()];
            }
            policy.selection = MeaningSelection::Bodies(ids);
        }
        let provider = ProviderSpec {
            trusted: true,
            command: "node".into(),
            args: vec![
                worker.to_str().unwrap().into(),
                mode.into(),
                marker.to_str().unwrap().into(),
            ],
            model_id: "owned-receiver-meaning-fixture".into(),
            model_version: "v1".into(),
            task_id: "receiver-meaning-fixture".into(),
            currency: "EUR".into(),
            max_cost_micros: 10,
            task_limit_micros: 100,
        };
        let built = meaning_build::build(
            &engine.store,
            &snap,
            &canonical,
            &provider,
            &policy,
            &Deadline::new(30000),
        )
        .unwrap();
        Self {
            received,
            root,
            engine,
            snapshot,
            host,
            graph: built["graphId"].as_str().unwrap().into(),
            marker,
        }
    }
    fn request(&self) -> semantic::Request {
        semantic::Request {
            schema_version: 1,
            verification: self.received.request(&self.snapshot),
            meaning_graph_ids: vec![self.graph.clone()],
        }
    }
    fn calls(&self) -> usize {
        std::fs::read_to_string(&self.marker)
            .unwrap()
            .lines()
            .count()
    }
    fn records(&self) -> usize {
        self.engine
            .store
            .db
            .query_row("SELECT count(*) FROM records", [], |r| r.get(0))
            .unwrap()
    }
}
fn interpretation_options() -> semantic::Options {
    semantic::Options {
        allow_source_interpretation: true,
        ..Default::default()
    }
}
fn evaluate(
    f: &Fixture,
    r: &semantic::Request,
    o: &semantic::Options,
) -> anyhow::Result<semantic::CheckedInterpretation> {
    let d = Deadline::new(30000);
    let intake = Store::open_existing_read_only(f.received.root.path(), &d).unwrap();
    let verify = options();
    semantic::interpret(
        &verify::Context {
            receiver_store: &intake,
            receiver_profile: &f.received.profile,
            principal: "sender",
            source_store: &f.engine.store,
            source_profile: &f.received.sender.app,
            source_options: &f.received.sender.options,
            options: &verify,
        },
        r,
        o,
        &d,
    )
}
fn output(f: &Fixture) -> Value {
    evaluate(f, &f.request(), &interpretation_options())
        .unwrap()
        .export(16_000_000, &Deadline::new(30000))
        .unwrap()
}

#[test]
fn original_semantic_layer_parallel_directions_roles_and_receipts_are_preserved_without_provider_calls()
 {
    let f = Fixture::new("ok", "public");
    assert_eq!(f.calls(), 3);
    let before = f.records();
    let latest = f.engine.store.latest().unwrap();
    let received = count(f.received.root.path());
    let snapshot = f.engine.store.snapshot(&f.snapshot).unwrap();
    let graph = f.engine.store.graph(&f.snapshot).unwrap();
    let layer = meaning_view::load_selected(
        &f.engine.store,
        &snapshot,
        &graph,
        &[f.graph.clone()],
        32_000_000,
        &Deadline::new(30000),
    )
    .unwrap()
    .unwrap();
    std::fs::remove_file(f.host.path().join("owned-worker.cjs")).unwrap();
    let v = output(&f);
    assert_eq!(v["meaningLayer"], serde_json::to_value(&layer).unwrap());
    assert_eq!(v["unitBindings"].as_array().unwrap().len(), 4);
    assert_eq!(f.calls(), 3);
    assert_eq!(f.records(), before);
    assert_eq!(f.engine.store.latest().unwrap(), latest);
    assert_eq!(count(f.received.root.path()), received);
    assert!(v.to_string().contains("value+123456789"));
    assert!(!v.to_string().contains("private_only"));
    assert_eq!(v["providerCallsThisRead"], 0);
    assert_eq!(v["networkRequests"], 0);
    for key in [
        "recordsWritten",
        "sourceWritten",
        "semanticIdentityEstablished",
        "observedExecution",
        "sourceAuthority",
        "complete",
    ] {
        assert_eq!(v[key], false, "{key}");
    }
    assert_eq!(v["providerAttestation"], "unverified");
    let packet = layer
        .packets
        .iter()
        .find(|p| p.relations.len() == 4)
        .unwrap();
    let forward = packet
        .relations
        .iter()
        .find(|r| r.description == "Forward inference")
        .unwrap();
    let parallel = packet
        .relations
        .iter()
        .find(|r| r.description == "Parallel inference")
        .unwrap();
    let reverse = packet
        .relations
        .iter()
        .find(|r| r.description == "Reverse inference")
        .unwrap();
    assert_ne!(forward.id, parallel.id);
    assert_eq!(
        (&forward.source, &forward.target),
        (&reverse.target, &reverse.source)
    );
    assert_eq!(
        packet.hyperedges[0].participants[0].concept,
        packet.hyperedges[0].participants[1].concept
    );
    assert_eq!(
        packet.hyperedges[0].participants[0].role,
        packet.hyperedges[0].participants[2].role
    );
    assert_eq!(packet.hyperedges[0].participants[1].role, "output");
}

#[test]
fn every_source_unit_must_fit_a_verified_declaration_even_if_its_quotes_or_coverage_are_smaller() {
    for (mode, selection) in [
        ("ok", "private"),
        ("first-byte", "file"),
        ("no-claims", "private"),
    ] {
        let f = Fixture::new(mode, selection);
        let before = f.records();
        let calls = f.calls();
        let error = evaluate(&f, &f.request(), &interpretation_options())
            .err()
            .unwrap();
        assert_eq!(
            error.to_string(),
            "github_app_digest_interpretation_source_mismatch",
            "{mode}/{selection}"
        );
        assert_eq!(f.records(), before);
        assert_eq!(f.calls(), calls);
    }
}

#[test]
fn overlapping_module_and_function_correspondence_preserves_every_received_ordinal() {
    let f = Fixture::new("ok", "nested");
    let v = output(&f);
    let links = v["unitBindings"][0]["declarationCorrespondence"]
        .as_array()
        .unwrap();
    assert_eq!(links.len(), 2);
    assert_ne!(links[0]["verificationRowId"], links[1]["verificationRowId"]);
    assert_ne!(links[0]["receivedOrdinal"], links[1]["receivedOrdinal"]);
    assert!(
        links
            .iter()
            .all(|l| l["semanticIdentityEstablished"] == false)
    );
    assert_eq!(
        v["meaningLayer"]["packets"][0]["concepts"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn successful_no_claims_processing_keeps_explicit_source_bound_coverage() {
    let f = Fixture::new("no-claims", "public");
    let v = output(&f);
    assert_eq!(v["sourceQuotesPresent"], false);
    assert_eq!(v["unitBindings"].as_array().unwrap().len(), 4);
    for p in v["meaningLayer"]["packets"].as_array().unwrap() {
        assert!(p["concepts"].as_array().unwrap().is_empty());
        assert!(
            p["coverage"]
                .as_array()
                .unwrap()
                .iter()
                .all(|c| c["outcome"] == "no_claims")
        );
    }
    assert_eq!(v["complete"], false);
}

#[test]
fn changing_only_unadvertised_bytes_cannot_rebind_an_old_meaning_graph_to_a_new_snapshot() {
    let mut f = Fixture::new("ok", "public");
    let original = std::fs::read_to_string(f.root.path().join("main.rs")).unwrap();
    std::fs::write(
        f.root.path().join("main.rs"),
        original.replace("{ 73 }", "{ 74 }"),
    )
    .unwrap();
    let old = f.snapshot.clone();
    f.snapshot = f.engine.index(&Deadline::new(20000)).unwrap().0.id;
    assert_ne!(f.snapshot, old);
    // The advertised projection still matches. The independently selected old
    // semantic graph may not be silently rebound to this new source observation.
    run(&f.received, &f.engine.store, &f.snapshot, &options()).unwrap();
    assert_eq!(
        evaluate(&f, &f.request(), &interpretation_options())
            .err()
            .unwrap()
            .to_string(),
        "github_app_digest_interpretation_parent_unavailable"
    );
    assert_eq!(f.calls(), 3);
}

#[test]
fn interpretation_permissions_closed_requests_and_revoked_grants_precede_parent_access() {
    let mut f = Fixture::new("ok", "public");
    let good = serde_json::to_vec(&f.request()).unwrap();
    assert!(semantic::Request::parse(&good).is_ok());
    let mut unknown = serde_json::to_value(f.request()).unwrap();
    unknown["allowSourceInterpretation"] = json!(true);
    assert!(semantic::Request::parse(unknown.to_string().as_bytes()).is_err());
    let duplicate = format!(
        "{{\"schemaVersion\":1,{}",
        String::from_utf8(good).unwrap().strip_prefix('{').unwrap()
    );
    assert!(semantic::Request::parse(duplicate.as_bytes()).is_err());
    let mut r = f.request();
    r.meaning_graph_ids = vec![f.graph.clone(), f.graph.clone()];
    assert!(r.validate().is_err());
    r.meaning_graph_ids.clear();
    assert!(r.validate().is_err());
    r = f.request();
    r.meaning_graph_ids = vec!["a".repeat(64)];
    assert_eq!(
        evaluate(&f, &r, &semantic::Options::default())
            .err()
            .unwrap()
            .to_string(),
        "github_app_digest_interpretation_permission_required"
    );
    f.received.profile.grants[0].allow_query = false;
    assert_eq!(
        evaluate(&f, &r, &interpretation_options())
            .err()
            .unwrap()
            .to_string(),
        "github_app_digest_verification_permission_required"
    );
    assert_eq!(f.calls(), 3);
}

#[test]
fn independent_input_mapping_and_output_limits_refuse_complete_results_without_writes() {
    let f = Fixture::new("ok", "public");
    let before = f.records();
    for mode in 0..5 {
        let mut o = interpretation_options();
        match mode {
            0 => o.max_input_bytes = 1,
            1 => o.max_unit_bindings = 1,
            2 => o.max_mapping_checks = 1,
            3 => o.max_declaration_matches = 1,
            _ => o.max_output_bytes = 1024,
        }
        assert_eq!(
            evaluate(&f, &f.request(), &o).err().unwrap().to_string(),
            "github_app_digest_interpretation_capacity_exceeded",
            "{mode}"
        );
        assert_eq!(f.records(), before);
        assert_eq!(f.calls(), 3);
    }
}

#[test]
fn corrupted_semantic_publication_cannot_promote_its_embedded_source_or_error_text() {
    let f = Fixture::new("ok", "public");
    let raw: String = f
        .engine
        .store
        .db
        .query_row(
            "SELECT value FROM records WHERE kind=?1 AND id=?2",
            rusqlite::params![meaning_build::GRAPH_KIND, f.graph],
            |r| r.get(0),
        )
        .unwrap();
    let mut registration: Value = serde_json::from_str(&raw).unwrap();
    registration["artifactId"] = json!("github_app_digest_interpretation_capacity_exceeded");
    f.engine
        .store
        .db
        .execute(
            "UPDATE records SET value=?1 WHERE kind=?2 AND id=?3",
            rusqlite::params![registration.to_string(), meaning_build::GRAPH_KIND, f.graph],
        )
        .unwrap();
    assert_eq!(
        evaluate(&f, &f.request(), &interpretation_options())
            .err()
            .unwrap()
            .to_string(),
        "github_app_digest_interpretation_parent_unavailable"
    );
    assert_eq!(f.calls(), 3);
}

#[test]
fn frozen_interpretation_exports_after_both_stores_are_gone_with_deadline_and_output_checks() {
    let f = Fixture::new("ok", "public");
    let checked = evaluate(&f, &f.request(), &interpretation_options()).unwrap();
    let first = checked.export(16_000_000, &Deadline::new(20000)).unwrap();
    let Fixture {
        root,
        engine,
        received,
        ..
    } = f;
    let local = root.path().join(".yoke");
    let intake = received.root.path().join(".yoke");
    drop(engine);
    std::fs::rename(&local, root.path().join("offline-local-state")).unwrap();
    std::fs::rename(&intake, received.root.path().join("offline-receiver-state")).unwrap();
    // Both selected state locations are unavailable. Explicitly close the owned
    // Engine before moving its Store, including on Windows with open-file locks.
    assert!(!local.exists());
    assert!(!intake.exists());
    assert_eq!(
        checked.export(16_000_000, &Deadline::new(20000)).unwrap(),
        first
    );
    assert!(checked.export(1024, &Deadline::new(20000)).is_err());
    assert_eq!(
        checked
            .export(16_000_000, &Deadline::new(0))
            .unwrap_err()
            .to_string(),
        "Request deadline exceeded"
    );
}

#[test]
fn actual_interpret_cli_requires_three_host_permissions_and_replays_existing_stores_without_a_provider()
 {
    let f = Fixture::new("ok", "public");
    for (name, value) in [
        (
            "receiver.json",
            serde_json::to_value(&f.received.profile).unwrap(),
        ),
        ("source.json", app_value()),
        ("request.json", serde_json::to_value(f.request()).unwrap()),
    ] {
        std::fs::write(
            f.host.path().join(name),
            serde_json::to_vec(&value).unwrap(),
        )
        .unwrap();
    }
    let command = || {
        let mut c = std::process::Command::new(env!("CARGO_BIN_EXE_yoke"));
        c.arg("--root")
            .arg(f.received.root.path())
            .args([
                "--max-bytes",
                "16000000",
                "digest-inbox",
                "interpret",
                "--profile",
            ])
            .arg(f.host.path().join("receiver.json"))
            .args(["--principal", "sender", "--request"])
            .arg(f.host.path().join("request.json"))
            .arg("--source-root")
            .arg(f.root.path())
            .arg("--source-profile")
            .arg(f.host.path().join("source.json"));
        c
    };
    let before = f.records();
    for flags in [
        vec![],
        vec!["--allow-query", "--allow-source-verification"],
        vec!["--allow-query", "--allow-source-interpretation"],
        vec![
            "--allow-source-verification",
            "--allow-source-interpretation",
        ],
    ] {
        let out = command().args(flags).output().unwrap();
        assert!(!out.status.success());
        assert!(out.stdout.is_empty());
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("interpretation_permission_required")
        );
    }
    std::fs::remove_file(f.host.path().join("owned-worker.cjs")).unwrap();
    let out = command()
        .args([
            "--allow-query",
            "--allow-source-verification",
            "--allow-source-interpretation",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["providerCallsThisRead"], 0);
    assert_eq!(v["meaningLayer"]["packets"].as_array().unwrap().len(), 3);
    assert_eq!(f.calls(), 3);
    assert_eq!(f.records(), before);
}

#[test]
fn cli_permissions_and_meaning_budget_precede_missing_host_files_and_state() {
    let root = tempfile::tempdir().unwrap();
    let command = || {
        let mut c = std::process::Command::new(env!("CARGO_BIN_EXE_yoke"));
        c.arg("--root")
            .arg(root.path())
            .args(["digest-inbox", "interpret", "--profile"])
            .arg(root.path().join("missing.json"))
            .args(["--principal", "sender", "--request"])
            .arg(root.path().join("missing-request.json"))
            .arg("--source-root")
            .arg(root.path())
            .arg("--source-profile")
            .arg(root.path().join("missing-app.json"));
        c
    };
    let output = command()
        .args(["--allow-query", "--allow-source-verification"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("interpretation_permission_required"));
    let output = command()
        .args([
            "--allow-query",
            "--allow-source-verification",
            "--allow-source-interpretation",
            "--max-meaning-input-bytes",
            "0",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("interpretation_capacity_exceeded"));
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}
