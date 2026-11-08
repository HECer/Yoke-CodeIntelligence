//! Checked quotations qualify host-submitted observations, never semantic truth.
use serde_json::{Value, json};
use std::{fs, io::Read, process::Command};
use yoke::{
    capture::Deadline,
    graph_view,
    mixed_view::{self, MeaningInputRequest, MixedMeaningClaimRequest, Query, SourcePin},
    model::{hash, identity},
};
#[allow(dead_code)]
#[path = "support/mixed_pdf_fixture.rs"]
mod support;
use support::*;

fn loaded(f: &Fixture, d: &Deadline) -> mixed_view::CheckedMixedView {
    mixed_view::load_with_pdf(
        &f.engine.store,
        &f.selection,
        &worker(),
        &Default::default(),
        d,
    )
    .unwrap()
}
fn request(f: &Fixture, view: &mixed_view::CheckedMixedView, d: &Deadline) -> MeaningInputRequest {
    let found = view
        .query(
            &serde_json::from_value::<Query>(
                json!({"operation":"find","query":"storage","ignoreCase":true}),
            )
            .unwrap(),
            4_000_000,
            d,
        )
        .unwrap();
    let mut units: Vec<_> = found["items"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .map(|(n, item)| json!({"label":format!("u{n}"),"source":item["source"]}))
        .collect();
    units.push(json!({"label":"unicode","source":{"kind":"sourceUtf8","viewId":f.selection.source.view_id,"snapshotId":f.selection.source.snapshot_id,
        "path":"billing.rs","contentHash":hash(SOURCE),"startByte":0,"endByte":SOURCE.len()}}));
    let container = fs::read(f.dir.path().join("office.docx")).unwrap();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(container)).unwrap();
    let mut xml = String::new();
    zip.by_name("word/document.xml")
        .unwrap()
        .read_to_string(&mut xml)
        .unwrap();
    units.push(json!({"label":"xml","source":{"kind":"documentPartXml","documentGraphId":f.selection.documents[0].document_graph_id,
        "part":"word/document.xml","contentHash":hash(&xml),"startByte":0,"endByte":xml.len()}}));
    serde_json::from_value(json!({"schemaVersion":1,"units":units,"contextGroups":[{"label":"joint","units":["u2","u0","u1","u2"]}]})).unwrap()
}
fn span(input: &Value, n: usize, length: usize) -> Value {
    let unit = &input["units"][n];
    let start = unit["item"]["source"]["startByte"].as_u64().unwrap();
    json!({"sourceId":unit["id"],"startByte":start,"endByte":start+length as u64})
}
fn claims(input: &Value) -> MixedMeaningClaimRequest {
    let a = span(input, 0, 1);
    let b = span(input, 1, 1);
    let p = span(input, 2, 1);
    let unicode = SOURCE.find('\u{1f30d}').unwrap();
    let u = json!({"sourceId":input["units"][4]["id"],"startByte":unicode,"endByte":unicode+4});
    let xml = input["units"][5]["item"]["text"]
        .as_str()
        .unwrap()
        .find("storage")
        .unwrap();
    let x = json!({"sourceId":input["units"][5]["id"],"startByte":xml,"endByte":xml+7});
    let edge = json!({"source":"a","target":"b","relation":"relates","description":"Host interpretation","support":[a,b,p,x]});
    serde_json::from_value(json!({"schemaVersion":1,"inputHash":input["inputHash"],"fragment":{
        "concepts":[{"key":"a","label":"Storage","description":"Host observation A","aliases":["same display"],"support":[a,u]},
                    {"key":"b","label":"Storage","description":"Host observation B","aliases":["same display"],"support":[b]}],
        "relations":[edge,edge,{"source":"b","target":"a","relation":"reverse","description":"Reverse observation","support":[p]},
            {"source":"a","target":"a","relation":"self","description":"Self observation","support":[a]}],
        "hyperedges":[{"relation":"joint","participants":[{"role":"input","concept":"a"},{"role":"output","concept":"b"},{"role":"aux","concept":"a"}],"description":"Ordered roles","support":[a,b,p]}],
        "coverage":input["units"].as_array().unwrap().iter().enumerate().map(|(n,u)|json!({"sourceId":u["id"],"outcome":if n==3 {"no_claims"}else{"claims"}})).collect::<Vec<_>>()}})).unwrap()
}
#[test]
fn mixed_claims_preserve_parallel_directions_role_slots_and_exact_frozen_quote_spaces() {
    let f = fixture();
    let d = Deadline::new(60000);
    let view = loaded(&f, &d);
    let request = request(&f, &view, &d);
    let prepared = view.prepare_meaning_inputs(&request, &d).unwrap();
    let input = prepared.export(4_000_000, &d).unwrap();
    let claims = claims(&input);
    for path in ["billing.rs", "office.docx", "notes.pdf"] {
        fs::remove_file(f.dir.path().join(path)).unwrap();
    }
    let before = state(&f.engine);
    let checked = prepared.check_meaning_claims(&claims, &d).unwrap();
    let result = checked.export(4_000_000, &d).unwrap();
    assert_eq!(result["kind"], "mixed_meaning_claims_v1");
    assert_eq!(result["inputs"], input);
    assert_eq!(
        result["originalFragment"],
        serde_json::to_value(&claims.fragment).unwrap()
    );
    assert_eq!(
        result["originalFragment"]["relations"][0],
        result["originalFragment"]["relations"][1]
    );
    assert_ne!(
        result["originalFragment"]["concepts"][0]["key"],
        result["originalFragment"]["concepts"][1]["key"]
    );
    assert_eq!(
        result["interpretationOrigin"],
        "host_submitted_unverified_interpretation"
    );
    assert_eq!(result["providerAttestation"], "not_established");
    assert_eq!(result["canonicalSourceAuthority"], false);
    for (key, value) in [
        ("complete", json!(false)),
        ("modelCalls", json!(0)),
        ("sourceWritten", json!(false)),
    ] {
        assert_eq!(result[key], value);
    }
    let mut spaces = std::collections::BTreeSet::new();
    for quote in result["supportQuotes"].as_array().unwrap() {
        let source = &quote["item"]["source"];
        spaces.insert(source["kind"].as_str().unwrap());
        let q = serde_json::from_value(json!({"operation":"read","source":source})).unwrap();
        let exact = view.query(&q, 4_000_000, &d).unwrap();
        assert_eq!(quote["item"], exact["item"]);
        assert_eq!(source["startByte"], quote["support"]["startByte"]);
        assert_eq!(source["endByte"], quote["support"]["endByte"]);
    }
    assert_eq!(spaces.len(), 4);
    assert!(
        result["supportQuotes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|q| q["item"]["text"] == "\u{1f30d}")
    );
    assert_eq!(checked.export(4_000_000, &d).unwrap(), result);
    let mut changed = claims.clone();
    changed.fragment.relations.swap(0, 2);
    assert_ne!(
        prepared
            .check_meaning_claims(&changed, &d)
            .unwrap()
            .export(4_000_000, &d)
            .unwrap()["id"],
        result["id"]
    );
    assert_eq!(state(&f.engine), before);
}
#[test]
fn mixed_claims_refuse_foreign_supports_coverage_endpoints_roles_and_split_utf8() {
    let f = fixture();
    let d = Deadline::new(60000);
    let view = loaded(&f, &d);
    let request = request(&f, &view, &d);
    let prepared = view.prepare_meaning_inputs(&request, &d).unwrap();
    let input = prepared.export(4_000_000, &d).unwrap();
    let valid = serde_json::to_value(claims(&input)).unwrap();
    let before = state(&f.engine);
    let mut invalid = Vec::new();
    let mut c = valid.clone();
    c["inputHash"] = json!("0".repeat(64));
    invalid.push(c);
    let mut c = valid.clone();
    c["fragment"]["concepts"][0]["support"][0]["sourceId"] = json!("0".repeat(64));
    invalid.push(c);
    let mut c = valid.clone();
    c["fragment"]["concepts"][0]["support"][0]["startByte"] = json!(0);
    invalid.push(c);
    let mut c = valid.clone();
    c["fragment"]["relations"][0]["target"] = json!("missing");
    invalid.push(c);
    let mut c = valid.clone();
    c["fragment"]["coverage"][0]["outcome"] = json!("no_claims");
    invalid.push(c);
    let mut c = valid.clone();
    c["fragment"]["coverage"].as_array_mut().unwrap().pop();
    invalid.push(c);
    let mut c = valid.clone();
    c["fragment"]["hyperedges"][0]["participants"][2] =
        c["fragment"]["hyperedges"][0]["participants"][0].clone();
    invalid.push(c);
    let mut c = valid.clone();
    c["fragment"]["concepts"][1]["key"] = json!("a");
    invalid.push(c);
    for c in invalid {
        let c = serde_json::from_value(c).unwrap();
        refused(
            prepared.check_meaning_claims(&c, &d),
            "mixed_meaning_claim_invalid",
        );
    }
    let mut split: MixedMeaningClaimRequest = serde_json::from_value(valid).unwrap();
    split.fragment.concepts[0].support[1].start_byte += 1;
    refused(
        prepared.check_meaning_claims(&split, &d),
        "mixed_view_input_invalid",
    );
    assert_eq!(state(&f.engine), before);
}
#[test]
fn mixed_claims_empty_coverage_is_explicit_and_exact_output_deadlines_remain_bounded() {
    let f = fixture();
    let d = Deadline::new(60000);
    let view = loaded(&f, &d);
    let request = request(&f, &view, &d);
    let prepared = view.prepare_meaning_inputs(&request, &d).unwrap();
    let input = prepared.export(4_000_000, &d).unwrap();
    let claims=serde_json::from_value(json!({"schemaVersion":1,"inputHash":input["inputHash"],"fragment":{"concepts":[],"relations":[],"hyperedges":[],
        "coverage":input["units"].as_array().unwrap().iter().map(|u|json!({"sourceId":u["id"],"outcome":"no_claims"})).collect::<Vec<_>>()}})).unwrap();
    let before = state(&f.engine);
    let checked = prepared.check_meaning_claims(&claims, &d).unwrap();
    let result = checked.export(4_000_000, &d).unwrap();
    assert_eq!(result["supportQuotes"], json!([]));
    let size = serde_json::to_vec(&result).unwrap().len();
    assert_eq!(checked.export(size, &d).unwrap(), result);
    refused(checked.export(size - 1, &d), "mixed_view_capacity_exceeded");
    let expired = Deadline::new(0);
    refused(
        prepared.check_meaning_claims(&claims, &expired),
        "mixed_view_deadline_exceeded",
    );
    refused(
        checked.export(4_000_000, &expired),
        "mixed_view_deadline_exceeded",
    );
    assert_eq!(state(&f.engine), before);
}
#[test]
fn mixed_claims_quotation_capacity_counts_distinct_unit_observations_without_deduplication() {
    let mut f = fixture();
    let d = Deadline::new(60000);
    let source = format!("//{}\r\npub fn billing() {{}}\r\n", "x".repeat(16000));
    fs::write(f.dir.path().join("billing.rs"), &source).unwrap();
    let (snapshot, graph) = f.engine.select(&json!({}), &d).unwrap();
    let view =
        graph_view::build(&f.engine.store, &snapshot, &graph, &Default::default(), &d).unwrap();
    f.selection.source = SourcePin {
        workspace_id: snapshot.workspace_id,
        snapshot_id: snapshot.id,
        view_id: view.id,
        view_artifact_id: view.artifact_id,
        base_graph_hash: identity(&graph),
    };
    let request:MeaningInputRequest=serde_json::from_value(json!({"schemaVersion":1,"units":(0..9).map(|n|json!({"label":format!("u{n}"),"source":{
        "kind":"sourceUtf8","viewId":f.selection.source.view_id,"snapshotId":f.selection.source.snapshot_id,"path":"billing.rs","contentHash":hash(&source),"startByte":0,"endByte":16000}})).collect::<Vec<_>>()})).unwrap();
    let view = loaded(&f, &d);
    let prepared = view.prepare_meaning_inputs(&request, &d).unwrap();
    let input = prepared.export(4_000_000, &d).unwrap();
    let mut claims:MixedMeaningClaimRequest=serde_json::from_value(json!({"schemaVersion":1,"inputHash":input["inputHash"],"fragment":{
        "concepts":[{"key":"a","label":"Host","description":"Host range observation","aliases":[],"support":(0..8).map(|n|span(&input,n,16000)).collect::<Vec<_>>()}],
        "relations":[],"hyperedges":[],"coverage":input["units"].as_array().unwrap().iter().enumerate().map(|(n,u)|json!({"sourceId":u["id"],"outcome":if n==8 {"no_claims"}else{"claims"}})).collect::<Vec<_>>()}})).unwrap();
    let before = state(&f.engine);
    let result = prepared
        .check_meaning_claims(&claims, &d)
        .unwrap()
        .export(4_000_000, &d)
        .unwrap();
    assert_eq!(result["supportQuotes"].as_array().unwrap().len(), 8);
    claims.fragment.relations.push(serde_json::from_value(json!({"source":"a","target":"a","relation":"extra","description":"Extra observation","support":[span(&input,8,1)]})).unwrap());
    claims.fragment.coverage[8].outcome = "claims".into();
    refused(
        prepared.check_meaning_claims(&claims, &d),
        "mixed_view_capacity_exceeded",
    );
    assert_eq!(state(&f.engine), before);
}
#[test]
fn mixed_claims_cli_requires_checked_inputs_and_closed_data_before_store_initialization() {
    let f = fixture();
    let d = Deadline::new(60000);
    let view = loaded(&f, &d);
    let request = request(&f, &view, &d);
    let input = view
        .prepare_meaning_inputs(&request, &d)
        .unwrap()
        .export(4_000_000, &d)
        .unwrap();
    let claims = claims(&input);
    let selection = f.dir.path().join("selection.json");
    let file = f.dir.path().join("host-claims.json");
    fs::write(&selection, serde_json::to_vec(&f.selection).unwrap()).unwrap();
    fs::write(&file, serde_json::to_vec(&claims).unwrap()).unwrap();
    let run = |root: &std::path::Path, with_inputs: bool| {
        let mut c = Command::new(env!("CARGO_BIN_EXE_yoke"));
        c.arg("--root")
            .arg(root)
            .args([
                "--timeout",
                "60000",
                "--max-bytes",
                "4000000",
                "mixed-view",
                "--selection",
            ])
            .arg(&selection)
            .arg("--native-pdf-text");
        if with_inputs {
            c.arg("--meaning-inputs")
                .arg(serde_json::to_string(&request).unwrap());
        }
        c.arg("--meaning-claims").arg(&file).output().unwrap()
    };
    let before = state(&f.engine);
    let result = run(&f.engine.root, true);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let result: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(result["inputs"], input);
    let denied = run(&f.engine.root, false);
    assert!(!denied.status.success());
    assert!(denied.stdout.is_empty());
    let empty = tempfile::tempdir().unwrap();
    for raw in [
        "{\"schemaVersion\":1,\"schemaVersion\":1}".into(),
        {
            let mut c = serde_json::to_value(&claims).unwrap();
            c["providerAttestation"] = json!("verified");
            c.to_string()
        },
        {
            let mut c = serde_json::to_value(&claims).unwrap();
            c["fragment"]["concepts"][0]["sourceAuthority"] = json!(true);
            c.to_string()
        },
    ] {
        fs::write(&file, raw).unwrap();
        let denied = run(empty.path(), true);
        assert!(!denied.status.success());
        assert!(denied.stdout.is_empty());
        assert!(String::from_utf8_lossy(&denied.stderr).contains("mixed_meaning_claim_invalid"));
    }
    assert!(!empty.path().join(".yoke").exists());
    assert_eq!(state(&f.engine), before);
}
