//! Published meaning composition uses owned local workers, never a real model.
use serde_json::Value;
use std::{collections::BTreeMap, fs, path::PathBuf};
use yoke::{
    Engine,
    capture::Deadline,
    meaning_build::{self, MeaningPolicy, MeaningSelection},
    meaning_view::{self, ClaimKind, MeaningLayer, MeaningResolution, ProviderAttestation},
    model::{Graph, Snapshot, hash, identity},
    providers::ProviderSpec,
};

struct Fixture {
    _dir: tempfile::TempDir,
    engine: Engine,
    snapshot: Snapshot,
    graph: Graph,
    provider: ProviderSpec,
    worker: PathBuf,
    marker: PathBuf,
}
fn fixture(mode: &str) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("workspace");
    fs::create_dir(&root).unwrap();
    fs::write(
        root.join("a.rs"),
        "// captured source\npub fn alpha() -> &'static str { \"café\" }\n",
    )
    .unwrap();
    fs::write(root.join("b.rs"), "pub fn beta() -> u32 { 2 }\n").unwrap();
    fs::write(root.join("history.rs"), "pub fn history() -> u32 { 0 }\n").unwrap();
    let worker = dir.path().join("worker.cjs");
    let marker = dir.path().join("calls.txt");
    fs::write(&worker, r#"
let raw='';process.stdin.on('data',b=>raw+=b);process.stdin.on('end',()=>{
 const fs=require('fs'),r=JSON.parse(raw),mode=process.argv[2],marker=process.argv[3];
 fs.appendFileSync(marker,r.requestId+'\n');
 const support=s=>({sourceId:s.anchor.sourceId,startByte:s.anchor.startByte,endByte:s.anchor.endByte});
 const all=r.payload.sources.map(support);
 let concepts=[{key:'one',label:'Storage policy',description:'Owned inferred fixture',aliases:['Shared alias'],support:all}];
 let relations=[],hyperedges=[];
 if(all.length>1){
  concepts.push({key:'two',label:'Read path',description:'Owned read fixture',aliases:['Shared alias'],support:[all[1]]});
  concepts.push({key:'three',label:'Write path',description:'Owned write fixture',aliases:[],support:[all[0]]});
  relations=[{source:'one',target:'two',relation:'uses',description:'Forward claim',support:all},{source:'one',target:'two',relation:'uses',description:'Parallel claim',support:all},{source:'two',target:'one',relation:'uses',description:'Reverse claim',support:all}];
  hyperedges=[{relation:'coordinates',participants:[{role:'output',concept:'three'},{role:'input',concept:'one'},{role:'input',concept:'two'}],description:'Ordered roles remain inferred',support:all}];
 }
 let coverage=r.payload.sources.map(s=>({sourceId:s.anchor.sourceId,outcome:'claims'}));
 if(mode==='no-claims'){concepts=[];relations=[];hyperedges=[];coverage=r.payload.sources.map(s=>({sourceId:s.anchor.sourceId,outcome:'no_claims'}));}
 process.stdout.write(JSON.stringify({schema:1,operation:r.operation,model:r.model,modelVersion:r.modelVersion,requestId:r.requestId,inputHash:r.inputHash,sourceBindings:r.payload.sources.map(s=>s.anchor),currency:r.currency,actualCostMicros:mode==='unknown'?null:7,usage:mode==='unknown'?null:{inputTokens:10,outputTokens:3,cachedInputTokens:2},data:{concepts,relations,hyperedges,coverage}}));
});
"#).unwrap();
    let mut engine = Engine::open(&root).unwrap();
    let (snapshot, graph, _, _) = engine.index(&Deadline::new(10000)).unwrap();
    let provider = ProviderSpec {
        trusted: true,
        command: PathBuf::from("node"),
        args: vec![
            worker.to_str().unwrap().into(),
            mode.into(),
            marker.to_str().unwrap().into(),
        ],
        model_id: "meaning-view-fixture".into(),
        model_version: "v1".into(),
        task_id: "meaning-view-fixture".into(),
        currency: "EUR".into(),
        max_cost_micros: 10,
        task_limit_micros: 200,
    };
    Fixture {
        _dir: dir,
        engine,
        snapshot,
        graph,
        provider,
        worker,
        marker,
    }
}
fn policy(f: &Fixture) -> MeaningPolicy {
    let mut p = MeaningPolicy::files(&f.snapshot, hash("owned meaning view extraction"));
    p.selection = MeaningSelection::Files(vec!["a.rs".into(), "b.rs".into()]);
    p.context_groups = vec![vec!["a.rs".into(), "b.rs".into()]];
    p
}
fn publish(f: &Fixture) -> Value {
    meaning_build::build(
        &f.engine.store,
        &f.snapshot,
        &f.graph,
        &f.provider,
        &policy(f),
        &Deadline::new(30000),
    )
    .unwrap()
}
fn calls(f: &Fixture) -> usize {
    fs::read_to_string(&f.marker)
        .map(|s| s.lines().count())
        .unwrap_or(0)
}
fn selected(f: &Fixture, id: &str) -> MeaningLayer {
    meaning_view::load_selected(
        &f.engine.store,
        &f.snapshot,
        &f.graph,
        &[id.into()],
        32_000_000,
        &Deadline::new(30000),
    )
    .expect("checked published meaning selection")
    .expect("explicit meaning layer")
}
fn attempt_rows(f: &Fixture) -> Vec<Value> {
    f.engine.store.records("native_model_attempt_v1").unwrap()
}

#[test]
fn empty_meaning_selection_stays_absent_and_never_resolves_a_worker() {
    let f = fixture("ok");
    fs::remove_file(&f.worker).unwrap();
    assert!(
        meaning_view::load_selected(
            &f.engine.store,
            &f.snapshot,
            &f.graph,
            &[],
            0,
            &Deadline::new(1000)
        )
        .unwrap()
        .is_none()
    );
    assert_eq!(calls(&f), 0);
    assert!(attempt_rows(&f).is_empty());
}

#[test]
fn published_selection_preserves_multifile_quotes_roles_parallel_claims_and_original_receipts() {
    let f = fixture("ok");
    let build = publish(&f);
    let before = attempt_rows(&f);
    fs::remove_file(&f.worker).unwrap();
    let layer = selected(&f, build["graphId"].as_str().unwrap());
    assert_eq!(layer.schema_version, 1);
    assert_eq!(layer.scope.workspace_id, f.snapshot.workspace_id);
    assert_eq!(layer.scope.snapshot_id, f.snapshot.id);
    assert_eq!(layer.scope.base_graph_hash, identity(&f.graph));
    assert_eq!(layer.scope.analyzer_epoch, "yoke-tree-sitter-8");
    assert_eq!(layer.selections.len(), 1);
    assert_eq!(layer.selections[0].job_id, build["jobId"].as_str().unwrap());
    assert_eq!(layer.packets.len(), 3);
    let packet = layer
        .packets
        .iter()
        .find(|p| p.source_bindings.len() == 2)
        .unwrap();
    let multi = packet
        .concepts
        .iter()
        .find(|c| c.label == "Storage policy")
        .unwrap();
    let quotes: Vec<_> = packet
        .evidence
        .iter()
        .filter(|e| multi.evidence_ids.contains(&e.id))
        .collect();
    assert_eq!(quotes.len(), 2);
    assert_ne!(quotes[0].path, quotes[1].path);
    for evidence in &packet.evidence {
        let (source, content_hash) =
            yoke::query::source(&f.engine.store, &f.snapshot, &evidence.path).unwrap();
        assert_eq!(evidence.snapshot_id, f.snapshot.id);
        assert_eq!(evidence.content_hash, content_hash);
        assert_eq!(
            evidence.text,
            source[evidence.start_byte..evidence.end_byte]
        );
        assert!(
            source.is_char_boundary(evidence.start_byte)
                && source.is_char_boundary(evidence.end_byte)
        );
    }
    assert_eq!(packet.relations.len(), 3);
    let forward = packet
        .relations
        .iter()
        .filter(|r| r.source == multi.id)
        .collect::<Vec<_>>();
    assert_eq!(forward.len(), 2);
    assert_ne!(forward[0].id, forward[1].id);
    assert!(packet.relations.iter().any(|r| r.target == multi.id));
    let hyper = &packet.hyperedges[0];
    assert_eq!(
        hyper
            .participants
            .iter()
            .map(|p| p.role.as_str())
            .collect::<Vec<_>>(),
        ["output", "input", "input"]
    );
    assert!(
        hyper
            .participants
            .iter()
            .all(|p| packet.concepts.iter().any(|c| c.id == p.concept))
    );
    assert!(
        packet
            .concepts
            .iter()
            .all(|c| c.resolution == MeaningResolution::InferredConcept
                && c.provider_attestation == ProviderAttestation::Unverified)
    );
    assert_eq!(packet.provenance.original_snapshot_id, f.snapshot.id);
    assert_eq!(packet.provenance.snapshot_id, f.snapshot.id);
    assert_eq!(packet.provenance.model, f.provider.model_id);
    assert!(!packet.provenance.rebound);
    assert_eq!(packet.provenance.provider_reported_cost_micros, Some(7));
    let wire = serde_json::to_value(multi).unwrap();
    assert!(wire.get("path").is_none() && wire.get("declarationAnchor").is_none());
    for link in &packet.citation_links {
        let file = f
            .graph
            .entities
            .iter()
            .find(|e| e.id == link.file_entity_id)
            .unwrap();
        let evidence = packet
            .evidence
            .iter()
            .find(|e| e.id == link.evidence_id)
            .unwrap();
        assert_eq!(file.kind, "file");
        assert_eq!(file.path, evidence.path);
        if link.claim_kind == ClaimKind::Concept {
            assert!(packet.concepts.iter().any(|c| c.id == link.claim));
        }
    }
    assert!(meaning_view::charged_input_bytes(&layer).unwrap() > layer.selections[0].total_bytes);
    assert_eq!(attempt_rows(&f), before);
    assert_eq!(calls(&f), 3);
}

#[test]
fn selection_order_is_deterministic_shared_dependencies_are_charged_once_and_models_stay_distinct()
{
    let mut f = fixture("ok");
    let first = publish(&f);
    f.provider.model_id = "another-meaning-fixture".into();
    f.provider.task_id = "another-meaning-task".into();
    let second = publish(&f);
    let a = first["graphId"].as_str().unwrap().to_owned();
    let b = second["graphId"].as_str().unwrap().to_owned();
    fs::remove_file(&f.worker).unwrap();
    let load = |ids: &[String]| {
        meaning_view::load_selected(
            &f.engine.store,
            &f.snapshot,
            &f.graph,
            ids,
            32_000_000,
            &Deadline::new(30000),
        )
        .unwrap()
        .unwrap()
    };
    let ab = load(&[a.clone(), b.clone()]);
    let ba = load(&[b.clone(), a.clone()]);
    assert_eq!(ab, ba);
    assert_eq!(ab.selections.len(), 2);
    let one = load(&[a]);
    let two = load(&[b]);
    assert!(
        meaning_view::charged_input_bytes(&ab).unwrap()
            < meaning_view::charged_input_bytes(&one).unwrap()
                + meaning_view::charged_input_bytes(&two).unwrap()
    );
    let ids = ab
        .packets
        .iter()
        .flat_map(|p| p.concepts.iter().map(|c| &c.id))
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        ids.len(),
        ab.packets.iter().map(|p| p.concepts.len()).sum::<usize>()
    );
    assert_eq!(calls(&f), 6);
}

#[test]
fn published_rebind_retains_original_snapshot_request_and_accounting_lineage() {
    let mut f = fixture("ok");
    let first = publish(&f);
    let original = f.snapshot.id.clone();
    let before = attempt_rows(&f);
    fs::write(
        f.engine.root.join("history.rs"),
        "pub fn history() -> u32 { 9 }\n",
    )
    .unwrap();
    let (snapshot, graph, _, _) = f.engine.index(&Deadline::new(10000)).unwrap();
    f.snapshot = snapshot;
    f.graph = graph;
    let rebound = publish(&f);
    assert_eq!(rebound["modelCallsThisRun"], 0);
    fs::remove_file(&f.worker).unwrap();
    let layer = selected(&f, rebound["graphId"].as_str().unwrap());
    assert!(layer.packets.iter().all(|p| p.provenance.rebound
        && p.provenance.snapshot_id == f.snapshot.id
        && p.provenance.original_snapshot_id == original
        && p.provenance.original_job_id == first["jobId"].as_str().unwrap()));
    let roots = meaning_view::validate_selected(
        &f.engine.store,
        &f.snapshot,
        &f.graph,
        &layer,
        32_000_000,
        &Deadline::new(30000),
    )
    .unwrap();
    assert!(roots.snapshots.contains(&original) && roots.snapshots.contains(&f.snapshot.id));
    assert!(
        roots
            .records
            .iter()
            .any(|(kind, _)| kind == "native_model_attempt_v1")
    );
    assert!(
        roots
            .records
            .iter()
            .any(|(kind, _)| kind == "native_model_task_v1")
    );
    assert!(
        meaning_view::load_selected(
            &f.engine.store,
            &f.snapshot,
            &f.graph,
            &[first["graphId"].as_str().unwrap().into()],
            32_000_000,
            &Deadline::new(30000)
        )
        .is_err()
    );
    assert_eq!(before, attempt_rows(&f));
    assert_eq!(calls(&f), 3);
}

#[test]
fn explicit_no_claims_keeps_source_bindings_and_complete_packet_coverage() {
    let f = fixture("no-claims");
    let built = publish(&f);
    let layer = selected(&f, built["graphId"].as_str().unwrap());
    for p in &layer.packets {
        assert!(
            p.concepts.is_empty()
                && p.relations.is_empty()
                && p.hyperedges.is_empty()
                && p.evidence.is_empty()
                && p.citation_links.is_empty()
        );
        assert_eq!(p.source_bindings.len(), p.coverage.len());
        assert!(p.coverage.iter().all(|c| c.outcome == "no_claims"
            && p.source_bindings.iter().any(|b| b.source_id == c.source_id)));
    }
}

#[test]
fn validation_replays_quotes_roles_provenance_scope_and_real_closure_budget() {
    let f = fixture("ok");
    let built = publish(&f);
    let id = built["graphId"].as_str().unwrap().to_owned();
    let layer = selected(&f, &id);
    let charged = meaning_view::charged_input_bytes(&layer).unwrap();
    assert!(
        meaning_view::load_selected(
            &f.engine.store,
            &f.snapshot,
            &f.graph,
            std::slice::from_ref(&id),
            charged - 1,
            &Deadline::new(30000)
        )
        .is_err()
    );
    assert!(
        meaning_view::load_selected(
            &f.engine.store,
            &f.snapshot,
            &f.graph,
            &[id.clone(), id],
            32_000_000,
            &Deadline::new(30000)
        )
        .is_err()
    );
    for mode in 0..6 {
        let mut changed = layer.clone();
        match mode {
            0 => changed.packets[0].evidence[0].text.push('!'),
            1 => changed
                .packets
                .iter_mut()
                .find(|p| !p.hyperedges.is_empty())
                .unwrap()
                .hyperedges[0]
                .participants
                .swap(0, 1),
            2 => changed.packets[0].provenance.request_id = "0".repeat(64),
            3 => changed.scope.analyzer_epoch = "yoke-tree-sitter-9".into(),
            4 => changed.scope.capture_policy_hash = "0".repeat(64),
            _ => changed.input_bytes = 0,
        }
        assert!(
            meaning_view::validate_selected(
                &f.engine.store,
                &f.snapshot,
                &f.graph,
                &changed,
                32_000_000,
                &Deadline::new(30000)
            )
            .is_err(),
            "mode {mode}"
        );
    }
    let foreign = fixture("ok");
    assert!(
        meaning_view::validate_selected(
            &foreign.engine.store,
            &foreign.snapshot,
            &foreign.graph,
            &layer,
            32_000_000,
            &Deadline::new(30000)
        )
        .is_err()
    );
    assert_eq!(calls(&f), 3);
}

#[test]
fn archived_replay_uses_frozen_original_and_rebound_sources_after_live_source_and_worker_removal() {
    let mut f = fixture("ok");
    publish(&f);
    let original = f.snapshot.id.clone();
    fs::write(
        f.engine.root.join("history.rs"),
        "pub fn history() -> u32 { 9 }\n",
    )
    .unwrap();
    let (snapshot, graph, _, _) = f.engine.index(&Deadline::new(10000)).unwrap();
    f.snapshot = snapshot;
    f.graph = graph;
    let built = publish(&f);
    let layer = selected(&f, built["graphId"].as_str().unwrap());
    let archive = meaning_view::archive_selected(
        &f.engine.store,
        &f.snapshot,
        &f.graph,
        &layer,
        32_000_000,
        64_000_000,
        &Deadline::new(30000),
    )
    .unwrap();
    let scopes = archive.source_scopes().cloned().collect::<Vec<_>>();
    assert!(scopes.iter().any(|s| s.id == original));
    assert!(scopes.iter().any(|s| s.id == f.snapshot.id));
    let mut frozen = BTreeMap::new();
    for scope in &scopes {
        for file in &scope.files {
            let bytes = f
                .engine
                .store
                .read_blob_exact(&file.hash, file.size, 4_000_000)
                .unwrap();
            frozen.insert((scope.id.clone(), file.path.clone()), bytes);
        }
    }
    assert_ne!(
        frozen[&(original.clone(), "history.rs".into())],
        frozen[&(f.snapshot.id.clone(), "history.rs".into())]
    );
    let wire = serde_json::to_vec(&archive).unwrap();
    let archive = serde_json::from_slice(&wire).unwrap();
    let snapshot = f.snapshot.clone();
    let graph = f.graph.clone();
    let source_root = f.engine.root.clone();
    assert!(source_root.is_absolute());
    assert!(source_root.starts_with(f._dir.path().canonicalize().unwrap()));
    assert_eq!(source_root.file_name().unwrap(), "workspace");
    fs::remove_file(&f.worker).unwrap();
    drop(f.engine);
    fs::remove_dir_all(&source_root).unwrap();
    let mut read = |scope: &Snapshot, file: &yoke::model::SourceFile, _: &Deadline| {
        frozen
            .get(&(scope.id.clone(), file.path.clone()))
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("missing frozen source pair"))
    };
    let replay = meaning_view::replay_archive(
        &snapshot,
        &graph,
        &archive,
        &mut read,
        32_000_000,
        64_000_000,
        &Deadline::new(30000),
    )
    .unwrap();
    assert_eq!(replay, layer);
    frozen.remove(&(original, "history.rs".into()));
    let mut missing = |scope: &Snapshot, file: &yoke::model::SourceFile, _: &Deadline| {
        frozen
            .get(&(scope.id.clone(), file.path.clone()))
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("missing frozen source pair"))
    };
    assert!(
        meaning_view::replay_archive(
            &snapshot,
            &graph,
            &archive,
            &mut missing,
            32_000_000,
            64_000_000,
            &Deadline::new(30000)
        )
        .is_err()
    );
}

#[test]
fn archived_unknown_reports_remain_unknown_without_fresh_accounting_or_provider_attestation() {
    let f = fixture("unknown");
    let built = publish(&f);
    let before = attempt_rows(&f);
    let layer = selected(&f, built["graphId"].as_str().unwrap());
    assert!(
        layer
            .packets
            .iter()
            .all(|p| p.provenance.provider_reported_cost_micros.is_none()
                && p.provenance.provider_reported_usage.is_none())
    );
    let archive = meaning_view::archive_selected(
        &f.engine.store,
        &f.snapshot,
        &f.graph,
        &layer,
        32_000_000,
        64_000_000,
        &Deadline::new(30000),
    )
    .unwrap();
    let mut frozen = BTreeMap::new();
    for scope in archive.source_scopes() {
        for file in &scope.files {
            frozen.insert(
                (scope.id.clone(), file.path.clone()),
                f.engine
                    .store
                    .read_blob_exact(&file.hash, file.size, 4_000_000)
                    .unwrap(),
            );
        }
    }
    fs::remove_file(&f.worker).unwrap();
    let mut read = |scope: &Snapshot, file: &yoke::model::SourceFile, _: &Deadline| {
        frozen
            .get(&(scope.id.clone(), file.path.clone()))
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("missing frozen source pair"))
    };
    let replay = meaning_view::replay_archive(
        &f.snapshot,
        &f.graph,
        &archive,
        &mut read,
        32_000_000,
        64_000_000,
        &Deadline::new(30000),
    )
    .unwrap();
    assert_eq!(replay, layer);
    assert_eq!(attempt_rows(&f), before);
    assert_eq!(calls(&f), 3);
}

fn reseal_graph_payload(archive: &mut Value, layer: &MeaningLayer, alter: impl FnOnce(&mut Value)) {
    let original = &layer.selections[0].artifact_id;
    let artifact = archive["proof"]["artifacts"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|a| a["artifactId"].as_str() == Some(original))
        .unwrap();
    let mut graph: Value = serde_json::from_str(artifact["payload"].as_str().unwrap()).unwrap();
    alter(&mut graph);
    let payload = serde_json::to_string(&graph).unwrap();
    let id = hash(payload.as_bytes());
    artifact["artifactId"] = Value::String(id.clone());
    artifact["totalBytes"] = Value::from(payload.len());
    artifact["payload"] = Value::String(payload.clone());
    let rows = archive["proof"]["rows"].as_array_mut().unwrap();
    for row in rows {
        if row["key"]["kind"] == meaning_build::GRAPH_KIND
            && row["key"]["id"] == layer.selections[0].graph_id
        {
            let mut record: Value = serde_json::from_str(row["payload"].as_str().unwrap()).unwrap();
            record["artifactId"] = Value::String(id.clone());
            record["totalBytes"] = Value::from(payload.len());
            row["payload"] = Value::String(serde_json::to_string(&record).unwrap());
        }
        if row["key"]["kind"] == "artifact" && row["key"]["id"] == *original {
            row["key"]["id"] = Value::String(id.clone());
            row["payload"] = Value::String(format!(
                "{{\"artifactId\":\"{}\",\"totalBytes\":{}}}",
                id,
                payload.len()
            ));
        }
    }
}

#[test]
fn archived_proof_rederives_claims_and_rejects_missing_receipts_cycles_and_policy_drift() {
    let mut f = fixture("ok");
    publish(&f);
    fs::write(
        f.engine.root.join("history.rs"),
        "pub fn history() -> u32 { 9 }\n",
    )
    .unwrap();
    let (snapshot, graph, _, _) = f.engine.index(&Deadline::new(10000)).unwrap();
    f.snapshot = snapshot;
    f.graph = graph;
    let built = publish(&f);
    let layer = selected(&f, built["graphId"].as_str().unwrap());
    let archive = meaning_view::archive_selected(
        &f.engine.store,
        &f.snapshot,
        &f.graph,
        &layer,
        32_000_000,
        64_000_000,
        &Deadline::new(30000),
    )
    .unwrap();
    let mut frozen = BTreeMap::new();
    for scope in archive.source_scopes() {
        for file in &scope.files {
            frozen.insert(
                (scope.id.clone(), file.path.clone()),
                f.engine
                    .store
                    .read_blob_exact(&file.hash, file.size, 4_000_000)
                    .unwrap(),
            );
        }
    }
    let wire = serde_json::to_value(&archive).unwrap();
    fs::remove_file(&f.worker).unwrap();
    for (archive_cap, source_cap) in [(1, 64_000_000), (32_000_000, 1)] {
        let mut forbidden = |_: &Snapshot, _: &yoke::model::SourceFile, _: &Deadline| {
            panic!("Archive byte admission must precede any source callback")
        };
        assert!(
            meaning_view::replay_archive(
                &f.snapshot,
                &f.graph,
                &archive,
                &mut forbidden,
                archive_cap,
                source_cap,
                &Deadline::new(30000),
            )
            .is_err()
        );
    }
    for mode in 0..13 {
        let mut changed = wire.clone();
        match mode {
            0..=4 => {
                let kind = match mode {
                    0 => "native_model_attempt_v1",
                    1 => "native_model_task_v1",
                    2 => meaning_build::JOB_KIND,
                    3 => meaning_build::CACHE_KIND,
                    _ => "artifact",
                };
                let rows = changed["proof"]["rows"].as_array_mut().unwrap();
                let position = rows.iter().position(|r| r["key"]["kind"] == kind).unwrap();
                rows.remove(position);
            }
            5 => {
                let artifacts = changed["proof"]["artifacts"].as_array_mut().unwrap();
                let receipt = &layer.packets[0].provenance.response_artifact_id;
                let position = artifacts
                    .iter()
                    .position(|a| a["artifactId"] == *receipt)
                    .unwrap();
                artifacts.remove(position);
            }
            6 => reseal_graph_payload(&mut changed, &layer, |g| {
                g["packets"][0]["meaning"]["evidence"][0]["text"] =
                    Value::String("invented quotation".into());
            }),
            7 => reseal_graph_payload(&mut changed, &layer, |g| {
                let packet = g["packets"]
                    .as_array_mut()
                    .unwrap()
                    .iter_mut()
                    .find(|p| !p["meaning"]["hyperedges"].as_array().unwrap().is_empty())
                    .unwrap();
                packet["meaning"]["hyperedges"][0]["participants"]
                    .as_array_mut()
                    .unwrap()
                    .swap(0, 1);
            }),
            8..=10 => {
                let job_id = &layer.selections[0].job_id;
                let row = changed["proof"]["rows"]
                    .as_array_mut()
                    .unwrap()
                    .iter_mut()
                    .find(|r| {
                        r["key"]["kind"] == meaning_build::JOB_KIND && r["key"]["id"] == *job_id
                    })
                    .unwrap();
                let mut job: Value =
                    serde_json::from_str(row["payload"].as_str().unwrap()).unwrap();
                match mode {
                    8 => job["analyzerEpoch"] = Value::String("yoke-tree-sitter-9".into()),
                    9 => job["capturePolicyHash"] = Value::String("0".repeat(64)),
                    _ => job["packets"][0]["originJobId"] = Value::String(job_id.clone()),
                }
                row["payload"] = Value::String(serde_json::to_string(&job).unwrap());
            }
            11 => {
                let rows = changed["proof"]["rows"].as_array_mut().unwrap();
                let row = rows[0].clone();
                *rows = vec![row; 4097];
            }
            _ => changed["proof"]["sourceManifests"][0]["id"] = Value::String("0".repeat(64)),
        }
        let changed = serde_json::from_value(changed).unwrap();
        let mut read = |scope: &Snapshot, file: &yoke::model::SourceFile, _: &Deadline| {
            frozen
                .get(&(scope.id.clone(), file.path.clone()))
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("missing frozen source pair"))
        };
        assert!(
            meaning_view::replay_archive(
                &f.snapshot,
                &f.graph,
                &changed,
                &mut read,
                32_000_000,
                64_000_000,
                &Deadline::new(30000),
            )
            .is_err(),
            "archive mutation {mode} was accepted"
        );
    }
    assert_eq!(calls(&f), 3);
}

#[test]
fn later_task_accounting_cannot_change_a_published_layer_or_upgrade_unknown_receipts() {
    let f = fixture("unknown");
    let built = publish(&f);
    let graph_id = built["graphId"].as_str().unwrap();
    let layer = selected(&f, graph_id);
    let before = attempt_rows(&f);
    let d = Deadline::new(30000);
    let ledger = yoke::ledger::Ledger::new(&f.engine.store, &d);
    let unrelated = ledger
        .reserve(
            &f.provider.task_id,
            90,
            f.provider.task_limit_micros,
            &f.provider.currency,
            &f.provider.model_key().unwrap(),
        )
        .unwrap();
    ledger.mark_sent(&unrelated).unwrap();
    ledger.settle(&unrelated, Some(90)).unwrap();
    let original = &layer.packets[0].provenance.attempt_id;
    ledger.settle(original, Some(8)).unwrap();
    ledger
        .usage(
            original,
            "later-host-reconciliation",
            yoke::ledger::Usage {
                input_tokens: 12345,
                output_tokens: 678,
                cached_input_tokens: 12,
            },
        )
        .unwrap();
    assert_ne!(attempt_rows(&f), before);
    fs::remove_file(&f.worker).unwrap();
    let reread = selected(&f, graph_id);
    assert_eq!(
        reread, layer,
        "Mutable task/attempt row size entered the immutable meaning layer"
    );
    meaning_view::validate_selected(
        &f.engine.store,
        &f.snapshot,
        &f.graph,
        &layer,
        32_000_000,
        &d,
    )
    .unwrap();
    let archive = meaning_view::archive_selected(
        &f.engine.store,
        &f.snapshot,
        &f.graph,
        &layer,
        32_000_000,
        64_000_000,
        &d,
    )
    .unwrap();
    let mut read = |_: &Snapshot, file: &yoke::model::SourceFile, _: &Deadline| {
        f.engine
            .store
            .read_blob_exact(&file.hash, file.size, 4_000_000)
    };
    let replay = meaning_view::replay_archive(
        &f.snapshot,
        &f.graph,
        &archive,
        &mut read,
        32_000_000,
        64_000_000,
        &d,
    )
    .unwrap();
    assert_eq!(replay, layer);
    assert!(
        replay
            .packets
            .iter()
            .all(|p| p.provenance.provider_reported_cost_micros.is_none()
                && p.provenance.provider_reported_usage.is_none())
    );
    assert_eq!(calls(&f), 3);
}

#[test]
fn archived_snapshot_rows_reject_unrecognized_manifest_properties() {
    let mut f = fixture("ok");
    fs::write(f.engine.root.join("opaque.bin"), [0, 255, 0]).unwrap();
    let (snapshot, graph, _, _) = f.engine.index(&Deadline::new(10000)).unwrap();
    f.snapshot = snapshot;
    f.graph = graph;
    assert!(!f.snapshot.omissions.is_empty());
    let built = publish(&f);
    let layer = selected(&f, built["graphId"].as_str().unwrap());
    let archive = meaning_view::archive_selected(
        &f.engine.store,
        &f.snapshot,
        &f.graph,
        &layer,
        32_000_000,
        64_000_000,
        &Deadline::new(30000),
    )
    .unwrap();
    let mut frozen = BTreeMap::new();
    for scope in archive.source_scopes() {
        for file in &scope.files {
            frozen.insert(
                (scope.id.clone(), file.path.clone()),
                f.engine
                    .store
                    .read_blob_exact(&file.hash, file.size, 4_000_000)
                    .unwrap(),
            );
        }
    }
    fs::remove_file(&f.worker).unwrap();
    let wire = serde_json::to_value(&archive).unwrap();
    let mut accepted = Vec::new();
    for mode in 0..3 {
        let mut changed = wire.clone();
        let row = changed["proof"]["rows"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|r| r["key"]["table"] == "snapshots" && r["key"]["id"] == f.snapshot.id)
            .unwrap();
        let mut manifest: Value = serde_json::from_str(row["payload"].as_str().unwrap()).unwrap();
        match mode {
            0 => manifest["unrecognized"] = Value::Bool(true),
            1 => manifest["files"][0]["unrecognized"] = Value::Bool(true),
            _ => manifest["omissions"][0]["unrecognized"] = Value::Bool(true),
        }
        row["payload"] = Value::String(serde_json::to_string(&manifest).unwrap());
        let changed = serde_json::from_value(changed).unwrap();
        let mut read = |scope: &Snapshot, file: &yoke::model::SourceFile, _: &Deadline| {
            frozen
                .get(&(scope.id.clone(), file.path.clone()))
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("missing frozen source pair"))
        };
        if meaning_view::replay_archive(
            &f.snapshot,
            &f.graph,
            &changed,
            &mut read,
            32_000_000,
            64_000_000,
            &Deadline::new(30000),
        )
        .is_ok()
        {
            accepted.push(mode);
        }
    }
    assert!(
        accepted.is_empty(),
        "Unknown manifest properties were ignored in schema locations {accepted:?}"
    );
    assert_eq!(calls(&f), 3);
}
