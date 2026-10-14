//! Labeled real Node stdio protocol fixtures; these are not neural models.
use serde_json::Value;
use std::{fs, path::PathBuf};
use tempfile::TempDir;
use yoke::{
    Engine,
    capture::Deadline,
    model::{Graph, Snapshot},
    providers::{self, ProviderSpec},
};

fn fixture() -> (TempDir, Engine, Snapshot, Graph, ProviderSpec) {
    let dir = TempDir::new().unwrap();
    let workspace = dir.path().join("workspace");
    fs::create_dir(&workspace).unwrap();
    fs::write(
        workspace.join("billing.rs"),
        "// Gebühren 🌍\npub fn invoice() -> u32 { 17 }\n",
    )
    .unwrap();
    let script = dir.path().join("labeled-provider-fixture.cjs");
    fs::write(&script, r#"
// Explicit protocol fixture, not a neural provider.
let input = ''; process.stdin.on('data', b => input += b);
process.stdin.on('end', () => {
  const r = JSON.parse(input), sources = r.payload.sources || [];
  if (r.schema !== 1 || r.model !== 'fixture' || r.modelVersion !== 'v1') throw Error('bad request identity');
  if (r.currency!=='EUR' || r.costLimitMicros!==10 || r.taskId!=='summary') throw Error('host budget missing');
  if (sources.some(s => !s.text.includes('pub fn invoice()'))) throw Error('mutable source or wrong source payload');
  if (sources.some(s => s.anchor.endByte-s.anchor.startByte!==Buffer.byteLength(s.text))) throw Error('source byte anchors');
  let data;
  if (r.operation==='summarize') data={statements:[{text:'The source defines invoice.',evidenceEntityIds:[sources[0].anchor.sourceId]}]};
  else if (r.operation==='embed') data={items:sources.map(s=>({sourceId:s.anchor.sourceId,vector:[1,0]}))};
  else if (r.operation==='embed_query') {
    if(require('crypto').createHash('sha256').update(r.payload.query).digest('hex')!==r.payload.queryHash) throw Error('query hash');
    data={items:[{sourceId:'query',vector:[1,0]}]};
  } else if (r.operation==='rerank') data={items:sources.map(s=>({entityId:s.anchor.sourceId,score:0.75}))};
  else if (r.operation==='describe_media') {
    if(require('crypto').createHash('sha256').update(Buffer.from(r.payload.mediaBase64,'base64')).digest('hex')!==r.payload.contentHash) throw Error('media hash');
    data={assetId:r.payload.assetId,contentHash:r.payload.contentHash,description:'A labeled fixture icon.'};
  } else throw Error('unsupported fixture operation');
  const reply = {schema:1, operation:r.operation, model:r.model, modelVersion:r.modelVersion,
    requestId:r.requestId,inputHash:r.inputHash,sourceBindings:r.payload.mediaAnchor?[r.payload.mediaAnchor]:sources.map(s=>s.anchor),currency:'EUR',actualCostMicros:7,
    usage:{inputTokens:10,outputTokens:3,cachedInputTokens:2},data};
  const mode=process.argv[2] || 'ok';
  if (mode==='timeout') return setTimeout(()=>process.stdout.write(JSON.stringify(reply)),10000);
  if (mode==='flood') return process.stdout.write('x'.repeat(33*1024*1024));
  if (mode==='stderr-flood') process.stderr.write('x'.repeat(270000));
  if (mode==='malformed') return process.stdout.write('{broken');
  if (mode==='wrong-binding') reply.sourceBindings[0].contentHash='0'.repeat(64);
  if (mode==='wrong-model') reply.modelVersion='other';
  if (mode==='wrong-input-hash') reply.inputHash='0'.repeat(64);
  if (mode==='wrong-currency') reply.currency='USD';
  if (mode==='bad-usage') reply.usage.cachedInputTokens=11;
  if (mode==='authority') reply.allowWrites=true;
  if (mode==='bad-evidence') reply.data.statements[0].evidenceEntityIds=['0'.repeat(64)];
  if (mode==='bad-vector') reply.data.items[0].vector=[0,0];
  if (mode==='bad-query-dimension') reply.data.items[0].vector=[1,0,0];
  if (mode==='bad-candidate') reply.data.items[0].entityId='0'.repeat(64);
  if (mode==='empty-summary') reply.data.statements=[];
  if (mode==='unknown') delete reply.actualCostMicros;
  if (mode==='overrun') reply.actualCostMicros=60;
  if (mode==='nonzero') process.exitCode=2;
  if (mode==='nonzero-overrun') {reply.actualCostMicros=60;process.exitCode=2;}
  if (mode==='declared-error') {reply.error='Labeled fixture failure';delete reply.data;}
  if (mode==='duplicate-header') return process.stdout.write(JSON.stringify(reply).replace(/}$/,' ,"schema":1}'));
  process.stdout.write(JSON.stringify(reply));
});
"#).unwrap();
    let mut engine = Engine::open(&workspace).unwrap();
    let (snapshot, graph, _, _) = engine.index(&Deadline::new(10_000)).unwrap();
    let provider = ProviderSpec {
        trusted: true,
        command: PathBuf::from("node"),
        args: vec![script.to_str().unwrap().into()],
        model_id: "fixture".into(),
        model_version: "v1".into(),
        task_id: "summary".into(),
        currency: "EUR".into(),
        max_cost_micros: 10,
        task_limit_micros: 100,
    };
    (dir, engine, snapshot, graph, provider)
}

// Losing model version identity, skipping embedding import or binding search to mutable files must break this.
#[test]
fn embeddings_and_query_search_use_versioned_source_bound_collections() {
    let (_dir, engine, snapshot, graph, provider) = fixture();
    let deadline = Deadline::new(10_000);
    let generated =
        providers::generate_embeddings(&engine.store, &snapshot, &graph, &provider, &deadline)
            .unwrap();
    assert_eq!(generated["model"], "fixture@v1");
    assert_eq!(generated["count"], 1);
    assert_eq!(generated["dimension"], 2);
    fs::write(engine.root.join("billing.rs"), "pub fn changed() {}\n").unwrap();
    let result = providers::semantic_search(
        &engine.store,
        &snapshot,
        &graph,
        "invoice",
        5,
        &provider,
        &deadline,
    )
    .unwrap();
    assert_eq!(result["items"][0]["entity"]["id"], ids(&graph)[0]);
    assert_eq!(result["trust"], "ranking_not_evidence");
    assert_eq!(result["model"], "fixture@v1");
}

// Accepting an unknown candidate or a zero-vector success must break this.
#[test]
fn reranking_and_vectors_reject_invalid_exact_candidates() {
    let (_dir, engine, snapshot, graph, provider) = fixture();
    let deadline = Deadline::new(10_000);
    let result = providers::rerank(
        &engine.store,
        &snapshot,
        &graph,
        "invoice",
        &ids(&graph),
        &provider,
        &deadline,
    )
    .unwrap();
    assert_eq!(result["items"][0]["entityId"], ids(&graph)[0]);
    assert_eq!(result["items"][0]["score"], 0.75);
    assert_eq!(result["trust"], "ranking_not_evidence");
    for mode in ["bad-vector", "bad-candidate"] {
        let (_dir, engine, snapshot, graph, mut provider) = fixture();
        provider.args.push(mode.into());
        let deadline = Deadline::new(10_000);
        let result = if mode == "bad-vector" {
            providers::generate_embeddings(&engine.store, &snapshot, &graph, &provider, &deadline)
        } else {
            providers::rerank(
                &engine.store,
                &snapshot,
                &graph,
                "invoice",
                &ids(&graph),
                &provider,
                &deadline,
            )
        };
        assert!(result.is_err(), "{mode}");
        assert_eq!(
            yoke::ledger::Ledger::new(&engine.store, &deadline)
                .total("summary")
                .unwrap()
                .encumbered_micros,
            10
        );
    }
}

// Sending an unchecked binary hash or treating a media description as a verified fact must break this.
#[test]
fn media_descriptions_bind_captured_bytes_without_source_writes() {
    let (_dir, engine, _snapshot, _graph, provider) = fixture();
    // Actual complete 1x1 PNG container with valid chunk CRCs.
    let png = hex::decode("89504e470d0a1a0a0000000d49484452000000010000000108060000001f15c4890000000b49444154789c6360000200000500017a5eab3f0000000049454e44ae426082").unwrap();
    fs::write(engine.root.join("fixture.png"), &png).unwrap();
    let result =
        providers::describe_media(&engine, "fixture.png", &provider, &Deadline::new(10_000))
            .unwrap();
    assert_eq!(result["description"], "A labeled fixture icon.");
    assert_eq!(result["contentHash"], yoke::model::hash(&png));
    assert_eq!(result["truth"], "model_inference_not_verified_fact");
    assert_eq!(fs::read(engine.root.join("fixture.png")).unwrap(), png);
}
fn ids(graph: &Graph) -> Vec<String> {
    graph
        .entities
        .iter()
        .filter(|e| e.name == "invoice")
        .map(|e| e.id.clone())
        .collect()
}

// Reading the live tree or elevating summary text to a verified fact must break this.
#[test]
fn summaries_use_immutable_utf8_source_and_remain_inferences() {
    let (dir, engine, snapshot, graph, provider) = fixture();
    fs::write(engine.root.join("billing.rs"), "pub fn hostile() {}\n").unwrap();
    let result = providers::summarize_sources(
        &engine.store,
        &snapshot,
        &graph,
        &ids(&graph),
        &provider,
        &Deadline::new(10_000),
    )
    .unwrap();
    assert_eq!(result["truth"], "model_inference_not_verified_fact");
    assert_eq!(
        result["statements"][0]["text"],
        "The source defines invoice."
    );
    assert_eq!(result["sourceBindings"][0]["path"], "billing.rs");
    assert_eq!(result["sourceBindings"][0]["startByte"], 18);
    assert_eq!(
        fs::read_to_string(engine.root.join("billing.rs")).unwrap(),
        "pub fn hostile() {}\n"
    );
    let stored: Value = serde_json::from_slice(
        &engine
            .store
            .read_artifact(result["artifactId"].as_str().unwrap())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(stored["truth"], "model_inference_not_verified_fact");
    assert_eq!(
        result["accounting"],
        "provider_reported_unverified_or_unknown"
    );
    drop(dir);
}

// Trust derived from repository/model text, accepted authority keys, or accepted bad bindings must break this.
#[test]
fn host_trust_and_exact_reply_binding_fail_closed() {
    use yoke::ledger::Ledger;
    for mode in [
        "wrong-binding",
        "wrong-model",
        "authority",
        "bad-evidence",
        "malformed",
        "empty-summary",
        "wrong-input-hash",
        "wrong-currency",
        "bad-usage",
        "duplicate-header",
    ] {
        let (_dir, engine, snapshot, graph, mut provider) = fixture();
        provider.args.push(mode.into());
        let deadline = Deadline::new(10_000);
        assert!(
            providers::summarize_sources(
                &engine.store,
                &snapshot,
                &graph,
                &ids(&graph),
                &provider,
                &deadline
            )
            .is_err(),
            "{mode}"
        );
        let total = Ledger::new(&engine.store, &deadline)
            .total("summary")
            .unwrap();
        assert_eq!(total.encumbered_micros, 10, "{mode}");
        assert_eq!(total.actual_micros, 0, "{mode}");
        assert_eq!(total.unknown, 1, "{mode}");
    }
    let (_dir, engine, snapshot, graph, mut provider) = fixture();
    provider.trusted = false;
    let deadline = Deadline::new(10_000);
    assert!(
        providers::summarize_sources(
            &engine.store,
            &snapshot,
            &graph,
            &ids(&graph),
            &provider,
            &deadline
        )
        .is_err()
    );
    assert_eq!(
        Ledger::new(&engine.store, &deadline)
            .total("summary")
            .unwrap()
            .attempts,
        0
    );
}

// Dropping a well-bound bill because exit status or provider error says failure must break this.
#[test]
fn complete_bound_failed_replies_still_account_for_reported_costs() {
    use yoke::ledger::Ledger;
    for mode in ["nonzero", "declared-error", "nonzero-overrun"] {
        let (_dir, engine, snapshot, graph, mut provider) = fixture();
        provider.args.push(mode.into());
        let deadline = Deadline::new(10_000);
        assert!(
            providers::summarize_sources(
                &engine.store,
                &snapshot,
                &graph,
                &ids(&graph),
                &provider,
                &deadline
            )
            .is_err()
        );
        let ledger = Ledger::new(&engine.store, &deadline);
        let total = ledger.total("summary").unwrap();
        assert_eq!(
            total.actual_micros,
            if mode == "nonzero-overrun" { 60 } else { 7 },
            "{mode}"
        );
        assert_eq!(total.reserved_micros, 0, "{mode}");
        if mode == "nonzero-overrun" {
            assert!(
                ledger
                    .reserve("summary", 1, 100, "EUR", "fixture@v1")
                    .is_err()
            );
        }
    }
}

// Allowing a missing corpus, invalid source scope or credentials in host arguments to launch must break this.
#[test]
fn invalid_requests_and_missing_capabilities_fail_before_model_attempts() {
    let (_dir, engine, snapshot, graph, provider) = fixture();
    let deadline = Deadline::new(10_000);
    assert!(
        providers::semantic_search(
            &engine.store,
            &snapshot,
            &graph,
            "invoice",
            5,
            &provider,
            &deadline
        )
        .is_err()
    );
    let id = ids(&graph)[0].clone();
    for selection in [vec![], vec![id.clone(), id.clone()], vec!["0".repeat(64)]] {
        assert!(
            providers::summarize_sources(
                &engine.store,
                &snapshot,
                &graph,
                &selection,
                &provider,
                &deadline
            )
            .is_err()
        );
    }
    for field in ["model", "args", "currency"] {
        let mut rejected = provider.clone();
        match field {
            "model" => rejected.model_version.clear(),
            "args" => rejected
                .args
                .push("--api-key=synthetic-fixture-credential".into()),
            _ => rejected.currency = "eur".into(),
        }
        assert!(
            providers::summarize_sources(
                &engine.store,
                &snapshot,
                &graph,
                std::slice::from_ref(&id),
                &rejected,
                &deadline
            )
            .is_err()
        );
    }
    assert_eq!(
        yoke::ledger::Ledger::new(&engine.store, &deadline)
            .total("summary")
            .unwrap()
            .attempts,
        0
    );
}

// Freeing unknown billing from a successful reply or admitting more calls after a known overrun must break this.
#[test]
fn successful_unknown_billing_and_known_overrun_keep_budget_conservative() {
    let (_dir, engine, snapshot, graph, mut provider) = fixture();
    let deadline = Deadline::new(10_000);
    provider.args.push("unknown".into());
    let first = providers::summarize_sources(
        &engine.store,
        &snapshot,
        &graph,
        &ids(&graph),
        &provider,
        &deadline,
    )
    .unwrap();
    assert_eq!(first["providerReportedCostMicros"], Value::Null);
    provider.args.pop();
    provider.args.push("overrun".into());
    let second = providers::summarize_sources(
        &engine.store,
        &snapshot,
        &graph,
        &ids(&graph),
        &provider,
        &deadline,
    )
    .unwrap();
    assert_eq!(second["providerReportedCostMicros"], 60);
    assert!(
        providers::summarize_sources(
            &engine.store,
            &snapshot,
            &graph,
            &ids(&graph),
            &provider,
            &deadline
        )
        .is_err()
    );
    let total = yoke::ledger::Ledger::new(&engine.store, &deadline)
        .total("summary")
        .unwrap();
    assert_eq!(total.actual_micros, 60);
    assert_eq!(total.reserved_micros, 10);
    assert_eq!(total.encumbered_micros, 70);
    assert_eq!(total.attempts, 2);
    assert!(total.reservation_exceeded);
}

// Splitting a multi-byte character or hiding omitted source bytes at the excerpt boundary must break this.
#[test]
fn corpus_excerpts_stop_at_exact_utf8_boundaries_and_report_omissions() {
    let (_dir, mut engine, _snapshot, _graph, provider) = fixture();
    let source = format!("pub fn invoice() {{\n//{}🌍\n}}\n", "x".repeat(4_978));
    fs::write(engine.root.join("billing.rs"), source).unwrap();
    let deadline = Deadline::new(10_000);
    let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
    let result =
        providers::generate_embeddings(&engine.store, &snapshot, &graph, &provider, &deadline)
            .unwrap();
    assert_eq!(result["sourceBindings"][0]["startByte"], 0);
    assert_eq!(result["sourceBindings"][0]["endByte"], 4_999);
    assert_eq!(result["sourceOmissions"][0]["omittedBytes"], 6);
}

// Accepting a query vector incompatible with the indexed model as a valid billed reply must break this.
#[test]
fn query_vector_dimension_must_match_indexed_corpus_before_settlement() {
    let (_dir, engine, snapshot, graph, mut provider) = fixture();
    let deadline = Deadline::new(10_000);
    providers::generate_embeddings(&engine.store, &snapshot, &graph, &provider, &deadline).unwrap();
    provider.args.push("bad-query-dimension".into());
    assert!(
        providers::semantic_search(
            &engine.store,
            &snapshot,
            &graph,
            "invoice",
            5,
            &provider,
            &deadline
        )
        .is_err()
    );
    let total = yoke::ledger::Ledger::new(&engine.store, &deadline)
        .total("summary")
        .unwrap();
    assert_eq!(total.actual_micros, 7);
    assert_eq!(total.reserved_micros, 10);
    assert_eq!(total.unknown, 1);
}

// Treating unavailable/failed/truncated/timed-out provider work as zero cost must break this.
#[test]
fn unavailable_timeout_and_output_cap_keep_unknown_reservations() {
    use yoke::ledger::Ledger;
    for mode in ["unavailable", "timeout", "flood", "stderr-flood"] {
        let (_dir, engine, snapshot, graph, mut provider) = fixture();
        if mode == "unavailable" {
            provider.command = PathBuf::from("yoke-intentionally-unavailable-provider");
        } else {
            provider.args.push(mode.into());
        }
        let started = std::time::Instant::now();
        let deadline = Deadline::new(if mode == "timeout" { 500 } else { 10000 });
        assert!(
            providers::summarize_sources(
                &engine.store,
                &snapshot,
                &graph,
                &ids(&graph),
                &provider,
                &deadline
            )
            .is_err(),
            "{mode}"
        );
        assert!(
            started.elapsed() < std::time::Duration::from_secs(8),
            "{mode}"
        );
        let accounting_deadline = Deadline::new(10000);
        let total = Ledger::new(&engine.store, &accounting_deadline)
            .total("summary")
            .unwrap();
        assert_eq!(total.encumbered_micros, 10, "{mode}");
        assert_eq!(total.unknown, 1, "{mode}");
    }
}
