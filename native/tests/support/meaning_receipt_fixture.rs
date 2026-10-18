//! Shared Meaning receipt fixtures with literal cost, usage and namespace variants.
//! Owned local Node workers run only during explicit fixture construction.
//! Published and portable archive inputs use the original checked source proofs.
#![allow(dead_code)] // Shared test targets intentionally use different fixture members.
use serde_json::{Value, json};
use std::{fs, path::PathBuf};
use tempfile::TempDir;
use yoke::{
    Engine,
    analysis::AnalyzerEpoch,
    capture::Deadline,
    graph_view::{self, BuildOptions, View},
    meaning_build::{self, MeaningPolicy, MeaningSelection},
    model::{Graph, Snapshot, hash, identity},
    providers::ProviderSpec,
};

pub const SOURCE: &str =
    "// __H11_SOURCE_ONLY_9c6d__ caf\u{e9}\r\npub fn claim() -> &'static str { \"\u{fc}ber\" }\r\n";
pub const HISTORY_OLD: &str = "// __H11_HISTORY_OLD_7a2e__\npub fn history(){ }\n";
pub const HISTORY_NEW: &str = "// __H11_HISTORY_NEW_3f09__\npub fn history(){ let value = 1; }\n";
#[allow(dead_code)] // Used by the separate future comparison target sharing this fixture.
pub const SENTINELS: &[&str] = &[
    "__H11_SOURCE_ONLY_9c6d__",
    "__H11_HISTORY_OLD_7a2e__",
    "__H11_HISTORY_NEW_3f09__",
    "__H11_DESCRIPTION_ONLY_812a__",
    "__H11_ALIAS_ONLY_67af__",
];
pub fn d() -> Deadline {
    Deadline::new(30_000)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Variant {
    Known,
    Zero,
    Unknown,
    CostOnly,
    UsageOnly,
    NoClaims,
}
impl Variant {
    pub fn label(self) -> &'static str {
        match self {
            Self::Known => "known",
            Self::Zero => "zero",
            Self::Unknown => "unknown",
            Self::CostOnly => "cost_only",
            Self::UsageOnly => "usage_only",
            Self::NoClaims => "no_claims",
        }
    }
    pub fn version(self) -> &'static str {
        match self {
            Self::Known => "h11-v1",
            Self::Zero => "h11-v2",
            Self::Unknown => "h11-v3",
            Self::CostOnly => "h11-v4",
            Self::UsageOnly => "h11-v5",
            Self::NoClaims => "h11-v6",
        }
    }
    pub fn currency(self) -> &'static str {
        match self {
            Self::Known | Self::CostOnly | Self::UsageOnly => "EUR",
            Self::Zero | Self::Unknown | Self::NoClaims => "USD",
        }
    }
    pub fn cost(self) -> Value {
        match self {
            Self::Known => json!(7),
            Self::Zero => json!(0),
            Self::CostOnly => json!(5),
            Self::Unknown | Self::UsageOnly | Self::NoClaims => Value::Null,
        }
    }
    pub fn usage(self) -> Value {
        match self {
            Self::Known => json!({"inputTokens":101,"outputTokens":13,"cachedInputTokens":17}),
            Self::Zero => json!({"inputTokens":0,"outputTokens":0,"cachedInputTokens":0}),
            Self::UsageOnly => json!({"inputTokens":11,"outputTokens":3,"cachedInputTokens":0}),
            Self::Unknown | Self::CostOnly | Self::NoClaims => Value::Null,
        }
    }
    pub fn claim_count(self) -> usize {
        usize::from(self != Self::NoClaims)
    }
}
pub const VARIANTS: [Variant; 6] = [
    Variant::Known,
    Variant::Zero,
    Variant::Unknown,
    Variant::CostOnly,
    Variant::UsageOnly,
    Variant::NoClaims,
];
pub struct Case {
    pub variant: Variant,
    pub provider: ProviderSpec,
    pub original: View,
    pub rebound: View,
}
pub struct Fixture {
    pub dir: TempDir,
    pub engine: Engine,
    pub original_snapshot: Snapshot,
    pub rebound_snapshot: Snapshot,
    pub cases: Vec<Case>,
    pub marker: PathBuf,
}
impl Fixture {
    pub fn case(&self, variant: Variant) -> &Case {
        self.cases.iter().find(|c| c.variant == variant).unwrap()
    }
}
fn policy(snapshot: &Snapshot, variant: Variant) -> MeaningPolicy {
    let mut policy = MeaningPolicy::files(snapshot, hash("r3-h11-owned-contract"));
    policy.selection = MeaningSelection::Files(vec!["claim.rs".into()]);
    policy.generation = format!("r3-h11-generation-{}", variant.label());
    policy
}
fn published_view(
    engine: &Engine,
    snapshot: &Snapshot,
    graph: &Graph,
    provider: &ProviderSpec,
    variant: Variant,
    calls: usize,
) -> View {
    let result = meaning_build::build(
        &engine.store,
        snapshot,
        graph,
        provider,
        &policy(snapshot, variant),
        &d(),
    )
    .unwrap();
    assert_eq!(
        result["status"], "ready",
        "Genuine host-selected provider response must publish first"
    );
    assert_eq!(result["modelCallsThisRun"], calls);
    let view = graph_view::build(
        &engine.store,
        snapshot,
        graph,
        &BuildOptions {
            structural_kinds: vec![],
            meaning_graph_ids: vec![result["graphId"].as_str().unwrap().into()],
            ..Default::default()
        },
        &d(),
    )
    .unwrap();
    let layer = view.meaning.as_ref().unwrap();
    assert_eq!(layer.packets.len(), 1);
    let packet = &layer.packets[0];
    assert_eq!(
        packet
            .provenance
            .provider_reported_cost_micros
            .map(Value::from)
            .unwrap_or(Value::Null),
        variant.cost()
    );
    assert_eq!(
        serde_json::to_value(&packet.provenance.provider_reported_usage).unwrap(),
        variant.usage()
    );
    assert_eq!(packet.provenance.currency, variant.currency());
    assert_eq!(packet.provenance.model_version, variant.version());
    assert_eq!(packet.concepts.len(), variant.claim_count());
    assert_eq!(
        packet.source_bindings.len(),
        1,
        "No-claims coverage still has original source binding"
    );
    assert_eq!(packet.coverage.len(), 1);
    assert_eq!(
        packet.coverage[0].outcome,
        if variant == Variant::NoClaims {
            "no_claims"
        } else {
            "claims"
        }
    );
    view
}
pub fn fixture() -> Fixture {
    let dir = TempDir::new().unwrap();
    let root = dir.path().join("workspace");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("claim.rs"), SOURCE).unwrap();
    fs::write(root.join("history.rs"), HISTORY_OLD).unwrap();
    let worker = dir.path().join("h11-receipt-worker.cjs");
    let marker = dir.path().join("h11-owned-calls.txt");
    fs::write(&worker, r#"
let raw='';process.stdin.on('data',b=>raw+=b);process.stdin.on('end',()=>{
 const r=JSON.parse(raw),fs=require('fs'),variant=process.argv[3];
 fs.appendFileSync(process.argv[2],JSON.stringify({variant,request:r})+'\n');
 const reports={known:[7,{inputTokens:101,outputTokens:13,cachedInputTokens:17}],zero:[0,{inputTokens:0,outputTokens:0,cachedInputTokens:0}],unknown:[null,null],cost_only:[5,null],usage_only:[null,{inputTokens:11,outputTokens:3,cachedInputTokens:0}],no_claims:[null,null]};
 if(!Object.hasOwn(reports,variant))throw Error('Unknown explicit owned receipt fixture');
 const all=r.payload.sources.map(s=>({sourceId:s.anchor.sourceId,startByte:s.anchor.startByte,endByte:s.anchor.endByte}));
 const concepts=variant==='no_claims'?[]:[{key:'claim',label:'Same receipt fixture concept',description:'__H11_DESCRIPTION_ONLY_812a__',aliases:['__H11_ALIAS_ONLY_67af__'],support:all}];
 process.stdout.write(JSON.stringify({schema:1,operation:r.operation,model:r.model,modelVersion:r.modelVersion,
 requestId:r.requestId,inputHash:r.inputHash,sourceBindings:r.payload.sources.map(s=>s.anchor),currency:r.currency,
 actualCostMicros:reports[variant][0],usage:reports[variant][1],
 data:{concepts,relations:[],hyperedges:[],coverage:r.payload.sources.map(s=>({sourceId:s.anchor.sourceId,outcome:variant==='no_claims'?'no_claims':'claims'}))}}));
});
"#).unwrap();
    let mut engine = Engine::open(&root).unwrap();
    engine.capture_epoch = AnalyzerEpoch::V9;
    let (original_snapshot, original_graph, _, _) = engine.index(&d()).unwrap();
    let mut originals = Vec::new();
    for variant in VARIANTS {
        let provider = ProviderSpec {
            trusted: true,
            command: PathBuf::from("node"),
            args: vec![
                worker.to_str().unwrap().into(),
                marker.to_str().unwrap().into(),
                variant.label().into(),
            ],
            model_id: format!("r3-h11-owned-{}", variant.label()),
            model_version: variant.version().into(),
            task_id: format!("r3-h11-task-{}", variant.label()),
            currency: variant.currency().into(),
            max_cost_micros: 10,
            task_limit_micros: 200,
        };
        let original = published_view(
            &engine,
            &original_snapshot,
            &original_graph,
            &provider,
            variant,
            1,
        );
        originals.push((variant, provider, original));
    }
    // Change ONLY an uncited captured file. Exact submitted units/provider policy
    // stay equal, so each new published graph must reuse its original receipt.
    fs::write(root.join("history.rs"), HISTORY_NEW).unwrap();
    let (rebound_snapshot, rebound_graph, _, _) = engine.index(&d()).unwrap();
    let mut cases = Vec::new();
    for (variant, provider, original) in originals {
        let rebound = published_view(
            &engine,
            &rebound_snapshot,
            &rebound_graph,
            &provider,
            variant,
            0,
        );
        let before = &original.meaning.as_ref().unwrap().packets[0].provenance;
        let after = &rebound.meaning.as_ref().unwrap().packets[0].provenance;
        assert!(!before.rebound);
        assert!(after.rebound);
        assert_eq!(after.original_snapshot_id, original_snapshot.id);
        assert_eq!(after.snapshot_id, rebound_snapshot.id);
        assert_eq!(after.original_job_id, before.original_job_id);
        assert_eq!(after.response_artifact_id, before.response_artifact_id);
        assert_eq!(after.attempt_id, before.attempt_id);
        assert_eq!(after.request_id, before.request_id);
        assert_eq!(
            after.provider_reported_cost_micros,
            before.provider_reported_cost_micros
        );
        assert_eq!(
            after.provider_reported_usage,
            before.provider_reported_usage
        );
        assert_eq!(after.currency, before.currency);
        assert_eq!(after.model_version, before.model_version);
        let original_layer = original.meaning.as_ref().unwrap();
        let rebound_layer = rebound.meaning.as_ref().unwrap();
        assert_eq!(
            original_layer.scope.capture_policy_hash,
            rebound_layer.scope.capture_policy_hash
        );
        assert_eq!(
            original_layer.packets[0].input_hash,
            rebound_layer.packets[0].input_hash
        );
        assert_eq!(
            original_layer.packets[0].unit_set_hash,
            rebound_layer.packets[0].unit_set_hash
        );
        assert_eq!(
            original_layer.packets[0].contract_hash,
            rebound_layer.packets[0].contract_hash
        );
        assert_ne!(
            original_layer.selections[0].job_id,
            rebound_layer.selections[0].job_id
        );
        assert_ne!(
            original_layer.selections[0].graph_id,
            rebound_layer.selections[0].graph_id
        );
        if variant != Variant::NoClaims {
            assert_ne!(
                original_layer.packets[0].concepts[0].id,
                rebound_layer.packets[0].concepts[0].id
            );
            assert_eq!(
                original_layer.packets[0].concepts[0].claim_id,
                rebound_layer.packets[0].concepts[0].claim_id
            );
        }
        cases.push(Case {
            variant,
            provider,
            original,
            rebound,
        });
    }
    assert_eq!(fs::read_to_string(&marker).unwrap().lines().count(), 6);
    fs::remove_file(&worker).unwrap();
    Fixture {
        dir,
        engine,
        original_snapshot,
        rebound_snapshot,
        cases,
        marker,
    }
}

pub fn requests(fixture: &Fixture) -> Vec<Value> {
    fs::read_to_string(&fixture.marker)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

pub fn assert_original_inputs(fixture: &Fixture, case: &Case) {
    let requests = requests(fixture);
    let requests = requests
        .iter()
        .filter(|row| row["variant"] == case.variant.label())
        .collect::<Vec<_>>();
    assert_eq!(
        requests.len(),
        1,
        "Exactly one actual owned request and no rebound provider invocation"
    );
    let request = &requests[0]["request"];
    for view in [&case.original, &case.rebound] {
        let layer = view.meaning.as_ref().unwrap();
        let packet = &layer.packets[0];
        let provenance = &packet.provenance;
        let job: Value = fixture
            .engine
            .store
            .get_record(meaning_build::JOB_KIND, &layer.selections[0].job_id)
            .unwrap();
        let origin: Value = fixture
            .engine
            .store
            .get_record(meaning_build::JOB_KIND, &provenance.original_job_id)
            .unwrap();
        let response_bytes = fixture
            .engine
            .store
            .read_artifact_limited(&provenance.response_artifact_id, 4_000_000)
            .unwrap();
        let response: Value = serde_json::from_slice(&response_bytes).unwrap();
        assert_eq!(hash(&response_bytes), provenance.response_artifact_id);
        assert_eq!(origin["jobId"], provenance.original_job_id);
        assert_eq!(origin["snapshotId"], fixture.original_snapshot.id);
        assert_eq!(origin["capturePolicyHash"], layer.scope.capture_policy_hash);
        assert_eq!(job["capturePolicyHash"], origin["capturePolicyHash"]);
        assert_eq!(job["provider"], origin["provider"]);
        assert_eq!(job["policy"], origin["policy"]);
        assert_eq!(response["jobId"], provenance.original_job_id);
        assert_eq!(response["attemptId"], provenance.attempt_id);
        assert_eq!(response["requestId"], provenance.request_id);
        assert_eq!(response["inputHash"], packet.input_hash);
        assert_eq!(
            response["sourceBindings"],
            serde_json::to_value(&packet.source_bindings).unwrap()
        );
        assert_eq!(response["actualCostMicros"], case.variant.cost());
        assert_eq!(response["usage"], case.variant.usage());
        assert_eq!(response["currency"], case.variant.currency());
        assert_eq!(response["modelVersion"], case.variant.version());
        assert_eq!(response["model"], case.provider.model_id);
        assert_eq!(request["requestId"], provenance.request_id);
        assert_eq!(request["inputHash"], packet.input_hash);
        assert_eq!(request["inputHash"], identity(&request["payload"]));
        assert_eq!(request["modelVersion"], case.variant.version());
        assert_eq!(request["currency"], case.variant.currency());
        assert_eq!(request["payload"], origin["packets"][0]["input"]);
    }
}

pub fn add_later_host_accounting(fixture: &Fixture) {
    let deadline = d();
    let ledger = yoke::ledger::Ledger::new(&fixture.engine.store, &deadline);
    for variant in [Variant::Unknown, Variant::NoClaims] {
        let case = fixture.case(variant);
        let attempt = &case.original.meaning.as_ref().unwrap().packets[0]
            .provenance
            .attempt_id;
        ledger.settle(attempt, Some(8)).unwrap();
        ledger
            .usage(
                attempt,
                "later-host-report",
                yoke::ledger::Usage {
                    input_tokens: 12_345,
                    output_tokens: 678,
                    cached_input_tokens: 12,
                },
            )
            .unwrap();
        let status = ledger.attempt_status(attempt).unwrap();
        assert_eq!(status.actual_cost_micros, Some(8));
        assert_eq!(status.reported_usage.as_ref().unwrap().input_tokens, 12_345);
        assert!(
            status.response_usage.is_none(),
            "Later host reports are not the immutable response event"
        );
        assert_original_inputs(fixture, case);
        let replay = graph_view::load(&fixture.engine.store, &case.rebound.id, &d()).unwrap();
        assert!(
            replay.meaning.as_ref().unwrap().packets[0]
                .provenance
                .provider_reported_cost_micros
                .is_none()
        );
        assert!(
            replay.meaning.as_ref().unwrap().packets[0]
                .provenance
                .provider_reported_usage
                .is_none()
        );
    }
}

pub fn remove_store_and_workspace(f: Fixture) -> TempDir {
    let root = f.engine.root.clone();
    assert!(root.is_absolute());
    assert!(root.starts_with(f.dir.path().canonicalize().unwrap()));
    assert_eq!(root.file_name().unwrap(), "workspace");
    drop(f.engine);
    fs::remove_dir_all(&root).unwrap();
    assert!(!root.exists());
    assert!(!f.dir.path().join("h11-receipt-worker.cjs").exists());
    f.dir
}
