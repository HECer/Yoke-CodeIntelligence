//! Actual saved Git/source proofs; no live model is involved in classification.
use yoke as test_yoke;
#[path = "support/git_impact_fixture.rs"]
mod fixture;
use fixture::{Fixture, state};
use serde_json::{Value, json};
use std::process::Command;
use yoke::{
    capture::Deadline,
    git_workflows::{
        areas::{self, Options},
        impact_details,
    },
    model::identity,
};

fn d() -> Deadline {
    Deadline::new(30_000)
}
fn report(f: &Fixture, options: &Options) -> areas::AreaReport {
    areas::derive(&f.engine.store, f.id(), &[], options, &d()).unwrap()
}
#[test]
fn actual_saved_git_directory_areas_keep_exact_parent_quotes_and_every_parallel_occurrence() {
    let f = Fixture::moved();
    let before = state(&f.engine);
    let options = Options::default();
    let parent = impact_details::derive(&f.engine.store, f.id(), &options.details, &d()).unwrap();
    let result = report(&f, &options);
    let v = result.value();
    assert_eq!(v["impactDetails"], *parent.value());
    assert_eq!(v["parentResultId"], parent.value()["resultId"]);
    fixture::assert_originals_and_complete_quotes(&f, &v["impactDetails"]);
    assert_eq!(v["pathClassifications"].as_array().unwrap().len(), 2);
    for c in v["pathClassifications"].as_array().unwrap() {
        assert_eq!(c["directoryLabel"], "src/");
        assert_eq!(c["label"], "src/");
        assert_eq!(c["basis"], "directory_spelling_only");
        assert_eq!(c["semanticIdentityEstablished"], false);
        assert!(c["conceptCandidateOrdinals"].as_array().unwrap().is_empty());
    }
    for occurrence in v["occurrences"].as_array().unwrap() {
        let parent_pointer = occurrence["parentPointer"].as_str().unwrap();
        assert!(
            v["impactDetails"].pointer(parent_pointer).is_some(),
            "{parent_pointer}"
        );
        if let Some(q) = occurrence["quotePointer"].as_str() {
            let quote = v["impactDetails"].pointer(q).unwrap();
            assert!(quote["contentHash"].is_string(), "{q}");
            assert!(quote["path"].is_string());
        }
        let i = occurrence["classificationOrdinal"].as_u64().unwrap() as usize;
        assert!(
            v["pathClassifications"][i]["occurrenceOrdinals"]
                .as_array()
                .unwrap()
                .contains(&occurrence["ordinal"])
        );
    }
    let parallel = v["impactDetails"]["impactObservations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|o| o["original"]["relation"] == "calls")
        .count();
    assert_eq!(parallel, 4);
    assert_eq!(v["groups"].as_array().unwrap().len(), 2);
    assert_eq!(v["stateWritten"], false);
    assert_eq!(v["modelCalls"], 0);
    assert_eq!(v["gitSubprocesses"], 0);
    assert_eq!(v["complete"], false);
    let mut unsealed = v.clone();
    let id = unsealed
        .as_object_mut()
        .unwrap()
        .remove("resultId")
        .unwrap();
    assert_eq!(id, identity(&unsealed));
    assert_eq!(
        serde_json::from_slice::<Value>(result.canonical_bytes()).unwrap(),
        *v
    );
    assert_eq!(
        report(&f, &options).canonical_bytes(),
        result.canonical_bytes()
    );
    assert_eq!(state(&f.engine), before);
    assert_eq!(
        std::fs::read(f.dir.path().join(".git/index")).unwrap(),
        f.index_bytes
    );
}
#[test]
fn actual_cli_opens_existing_readonly_state_with_no_git_or_provider_on_path_and_exact_stdout_cap() {
    let f = Fixture::moved();
    let before = state(&f.engine);
    let expected = report(&f, &Options::default());
    let call = |max: usize, flags: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_yoke"))
            .args(flags)
            .arg("--root")
            .arg(f.dir.path())
            .args(["--max-bytes", &max.to_string(), "git", "areas", f.id()])
            .env("PATH", "")
            .output()
            .unwrap()
    };
    let out = call(expected.canonical_bytes().len() + 1, &[]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        &out.stdout[..out.stdout.len() - 1],
        expected.canonical_bytes()
    );
    assert_eq!(out.stdout.last(), Some(&b'\n'));
    let out = call(expected.canonical_bytes().len(), &[]);
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("git_areas_capacity_exceeded"));
    for flags in [
        vec!["--allow-writes"],
        vec!["--snapshot", f.id()],
        vec!["--trust-servers"],
        vec!["--analyzer-epoch", "9"],
    ] {
        let out = call(4_000_000, &flags);
        assert!(!out.status.success());
        assert!(out.stdout.is_empty());
    }
    assert_eq!(state(&f.engine), before);
}
#[test]
fn closed_options_zero_capacities_exact_wire_limits_and_missing_state_never_publish() {
    let f = Fixture::moved();
    let before = state(&f.engine);
    for field in [
        "maxPaths",
        "maxOccurrences",
        "maxSnapshotFiles",
        "maxMetadataBytes",
        "maxOutputBytes",
    ] {
        let mut v = serde_json::to_value(Options::default()).unwrap();
        v["limits"][field] = json!(0);
        let options: Options = serde_json::from_value(v).unwrap();
        let error = areas::derive(&f.engine.store, f.id(), &[], &options, &d()).unwrap_err();
        assert_eq!(error.to_string(), "git_areas_capacity_exceeded", "{field}");
        assert_eq!(state(&f.engine), before);
    }
    let mut exact = Options::default();
    exact.limits.max_output_bytes = report(&f, &exact).canonical_bytes().len();
    exact.limits.max_output_bytes = report(&f, &exact).canonical_bytes().len();
    let n = exact.limits.max_output_bytes;
    assert_eq!(report(&f, &exact).canonical_bytes().len(), n);
    assert_eq!(n.to_string().len(), (n - 1).to_string().len());
    exact.limits.max_output_bytes = n - 1;
    assert_eq!(
        areas::derive(&f.engine.store, f.id(), &[], &exact, &d())
            .unwrap_err()
            .to_string(),
        "git_areas_capacity_exceeded"
    );
    for raw in [
        br#"{"limits":{"allowWrites":true}}"#.as_slice(),
        br#"{"details":{},"details":{}}"#,
        b"{",
        &[255],
    ] {
        assert_eq!(
            Options::parse(raw).unwrap_err().to_string(),
            "git_areas_input_invalid"
        );
    }
    let missing = tempfile::tempdir().unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(missing.path())
        .args(["git", "areas", f.id()])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("store_unavailable"));
    assert!(!missing.path().join(".yoke").exists());
    assert_eq!(state(&f.engine), before);
}
#[test]
fn caller_transaction_sql_limits_original_cancel_and_expiry_are_preserved() {
    use rusqlite::limits::Limit;
    let f = Fixture::moved();
    let before = state(&f.engine);
    f.engine.store.db.execute_batch("BEGIN DEFERRED").unwrap();
    assert_eq!(
        areas::derive(&f.engine.store, f.id(), &[], &Options::default(), &d())
            .unwrap_err()
            .to_string(),
        "git_areas_input_invalid"
    );
    assert!(!f.engine.store.db.is_autocommit());
    f.engine.store.db.execute_batch("ROLLBACK").unwrap();
    let prior = f
        .engine
        .store
        .db
        .set_limit(Limit::SQLITE_LIMIT_LENGTH, 32)
        .unwrap();
    assert!(areas::derive(&f.engine.store, f.id(), &[], &Options::default(), &d()).is_err());
    assert_eq!(
        f.engine.store.db.limit(Limit::SQLITE_LIMIT_LENGTH).unwrap(),
        32
    );
    f.engine
        .store
        .db
        .set_limit(Limit::SQLITE_LIMIT_LENGTH, prior)
        .unwrap();
    assert!(
        areas::derive(
            &f.engine.store,
            f.id(),
            &[],
            &Options::default(),
            &Deadline::new(0)
        )
        .is_err()
    );
    let flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let cancel = Deadline::run_bounded(&d(), std::sync::Arc::clone(&flag), || Ok(d())).unwrap();
    flag.store(true, std::sync::atomic::Ordering::SeqCst);
    assert_eq!(
        areas::derive(&f.engine.store, f.id(), &[], &Options::default(), &cancel)
            .unwrap_err()
            .to_string(),
        "git_areas_cancelled"
    );
    assert_eq!(state(&f.engine), before);
}

// Literal local Node protocol worker, used only during explicit fixture setup.
// Offline area calls below run after its removal; this is not model-quality proof.
struct MeaningFixture {
    f: Fixture,
    id: String,
    marker: std::path::PathBuf,
    _worker_owner: tempfile::TempDir,
}
fn meaning_fixture() -> MeaningFixture {
    use yoke::{
        meaning_build::{self, MeaningPolicy, MeaningSelection},
        model::hash,
        providers::ProviderSpec,
    };
    let f = Fixture::new(
        &[
            ("src/a.rs", "// café 🦀\r\npub fn a()->u32 { 1 }\r\n"),
            ("lib/b.rs", "pub fn b()->u32 { 3 }\r\n"),
        ],
        &[
            ("src/a.rs", "// café 🦀\r\npub fn a()->u32 { 2 }\r\n"),
            ("lib/b.rs", "pub fn b()->u32 { 4 }\r\n"),
        ],
    );
    let owner = tempfile::tempdir().unwrap();
    let worker = owner.path().join("area-worker.cjs");
    let marker = owner.path().join("area-calls.txt");
    std::fs::write(&worker, r#"
let raw='';process.stdin.on('data',x=>raw+=x);process.stdin.on('end',()=>{
 const fs=require('fs'),r=JSON.parse(raw);
 fs.appendFileSync(process.argv[2],r.requestId+'\n');
 const all=r.payload.sources.map(s=>({sourceId:s.anchor.sourceId,startByte:s.anchor.startByte,endByte:s.anchor.endByte}));
 const concepts=[],relations=[],hyperedges=[];
 if(all.length===2){
  const first=all[r.payload.sources.findIndex(s=>s.anchor.path==='src/a.rs')];
  const second=all[r.payload.sources.findIndex(s=>s.anchor.path==='lib/b.rs')];
  const add=(key,label,support)=>concepts.push({key,label,description:'Labeled area protocol fixture',aliases:[],support});
  add('all','Broad <area>',all);add('first','Same label',[first]);
  add('twin','Same label',[first]);add('second','Second area',[second]);
  for(const [source,target,description] of [['first','second','parallel1'],['first','second','parallel2'],['second','first','reverse']])
   relations.push({source,target,relation:'uses',description,support:all});
  hyperedges.push({relation:'coordinates',participants:[{role:'input',concept:'first'},{role:'input',concept:'second'},{role:'context',concept:'first'},{role:'output',concept:'second'}],description:'Ordered fixture roles',support:all});
 }
 process.stdout.write(JSON.stringify({schema:1,operation:r.operation,model:r.model,modelVersion:r.modelVersion,
 requestId:r.requestId,inputHash:r.inputHash,sourceBindings:r.payload.sources.map(s=>s.anchor),currency:r.currency,
 actualCostMicros:0,usage:null,data:{concepts,relations,hyperedges,
 coverage:r.payload.sources.map(s=>({sourceId:s.anchor.sourceId,outcome:all.length===2?'claims':'no_claims'}))}}));
});
"#).unwrap();
    let snapshot = f
        .engine
        .store
        .snapshot(f.report["headSnapshotId"].as_str().unwrap())
        .unwrap();
    let graph = f.engine.store.graph(&snapshot.id).unwrap();
    let provider = ProviderSpec {
        trusted: true,
        command: "node".into(),
        args: vec![
            worker.to_str().unwrap().into(),
            marker.to_str().unwrap().into(),
        ],
        model_id: "owned-area-fixture".into(),
        model_version: "v1".into(),
        task_id: "owned-area-fixture-task".into(),
        currency: "EUR".into(),
        max_cost_micros: 10,
        task_limit_micros: 100,
    };
    let mut policy = MeaningPolicy::files(&snapshot, hash("owned-area-contract"));
    policy.selection = MeaningSelection::Files(vec!["src/a.rs".into(), "lib/b.rs".into()]);
    policy.context_groups = vec![vec!["src/a.rs".into(), "lib/b.rs".into()]];
    policy.provider_input_files = vec![worker.clone()];
    let saved =
        meaning_build::build(&f.engine.store, &snapshot, &graph, &provider, &policy, &d()).unwrap();
    assert_eq!(saved["status"], "ready");
    assert_eq!(saved["modelCallsThisRun"], 3);
    assert_eq!(std::fs::read_to_string(&marker).unwrap().lines().count(), 3);
    std::fs::remove_file(&worker).unwrap();
    MeaningFixture {
        f,
        id: saved["graphId"].as_str().unwrap().into(),
        marker,
        _worker_owner: owner,
    }
}
#[test]
fn replayed_meaning_file_sets_rank_specific_claims_preserve_equal_label_ties_and_stay_offline() {
    use yoke::meaning_build;
    let f = meaning_fixture();
    let original = meaning_build::load(&f.f.engine.store, &f.id, &d()).unwrap();
    let calls = std::fs::read(&f.marker).unwrap();
    // Current working bytes disappear. Saved exact UTF-8/CRLF proofs stay usable.
    for name in ["src/a.rs", "lib/b.rs"] {
        std::fs::remove_file(f.f.dir.path().join(name)).unwrap();
    }
    let before = state(&f.f.engine);
    let result = areas::derive(
        &f.f.engine.store,
        f.f.id(),
        std::slice::from_ref(&f.id),
        &Options::default(),
        &d(),
    )
    .unwrap();
    let v = result.value();
    let concepts = v["concepts"].as_array().unwrap();
    assert_eq!(concepts.len(), 4);
    assert_eq!(v["consumed"]["memberships"], 5);
    let classes = v["pathClassifications"].as_array().unwrap();
    let a = classes
        .iter()
        .find(|c| c["side"] == "head" && c["path"] == "src/a.rs")
        .unwrap();
    let b = classes
        .iter()
        .find(|c| c["side"] == "head" && c["path"] == "lib/b.rs")
        .unwrap();
    assert_eq!(a["conceptCandidateOrdinals"].as_array().unwrap().len(), 3);
    assert_eq!(a["preferredConceptOrdinals"].as_array().unwrap().len(), 2);
    assert_eq!(a["preferredAmbiguous"], true);
    assert_eq!(a["label"], Value::Null);
    let preferred = a["preferredConceptOrdinals"].as_array().unwrap();
    let left = &concepts[preferred[0].as_u64().unwrap() as usize];
    let right = &concepts[preferred[1].as_u64().unwrap() as usize];
    assert_eq!(left["original"]["label"], "Same label");
    assert_eq!(right["original"]["label"], "Same label");
    assert_ne!(left["reference"], right["reference"]);
    assert_eq!(b["preferredAmbiguous"], false);
    assert_eq!(b["label"], "Second area");
    for c in classes.iter().filter(|c| c["side"] == "base") {
        assert_eq!(c["basis"], "directory_spelling_only");
        assert!(c["conceptCandidateOrdinals"].as_array().unwrap().is_empty());
    }
    assert!(
        concepts
            .iter()
            .flat_map(|c| c["supportEvidence"].as_array().unwrap())
            .any(|e| e["text"]
                .as_str()
                .is_some_and(|t| t.contains("café 🦀\r\n")))
    );
    assert_eq!(v["modelCalls"], 0);
    assert_eq!(v["gitSubprocesses"], 0);
    let out = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(f.f.dir.path())
        .args(["git", "areas", f.f.id(), "--meaning", &f.id])
        .env("PATH", "")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        &out.stdout[..out.stdout.len() - 1],
        result.canonical_bytes()
    );
    assert_eq!(
        meaning_build::load(&f.f.engine.store, &f.id, &d()).unwrap(),
        original
    );
    assert_eq!(std::fs::read(&f.marker).unwrap(), calls);
    assert_eq!(state(&f.f.engine), before);
}
#[test]
fn optional_meaning_quotas_and_foreign_or_corrupt_registrations_refuse_without_new_calls_or_writes()
{
    use yoke::meaning_build;
    let f = meaning_fixture();
    let before = state(&f.f.engine);
    let calls = std::fs::read(&f.marker).unwrap();
    for field in [
        "maxConcepts",
        "maxEvidenceRows",
        "maxSupportReferences",
        "maxMemberships",
        "maxAssignments",
        "maxMeaningBytes",
    ] {
        let mut v = serde_json::to_value(Options::default()).unwrap();
        v["limits"][field] = json!(0);
        let options: Options = serde_json::from_value(v).unwrap();
        let error = areas::derive(
            &f.f.engine.store,
            f.f.id(),
            std::slice::from_ref(&f.id),
            &options,
            &d(),
        )
        .unwrap_err();
        assert_eq!(error.to_string(), "git_areas_capacity_exceeded", "{field}");
        assert_eq!(state(&f.f.engine), before);
        assert_eq!(std::fs::read(&f.marker).unwrap(), calls);
    }
    let raw: String =
        f.f.engine
            .store
            .db
            .query_row(
                "SELECT value FROM records WHERE kind=?1 AND id=?2",
                rusqlite::params![meaning_build::GRAPH_KIND, f.id],
                |r| r.get(0),
            )
            .unwrap();
    for field in ["workspaceId", "snapshotId", "artifactId", "jobId"] {
        let mut forged: Value = serde_json::from_str(&raw).unwrap();
        forged[field] = json!("7".repeat(64));
        f.f.engine
            .store
            .db
            .execute(
                "UPDATE records SET value=?3 WHERE kind=?1 AND id=?2",
                rusqlite::params![
                    meaning_build::GRAPH_KIND,
                    f.id,
                    serde_json::to_string(&forged).unwrap()
                ],
            )
            .unwrap();
        let corrupt = state(&f.f.engine);
        assert_eq!(
            areas::derive(
                &f.f.engine.store,
                f.f.id(),
                std::slice::from_ref(&f.id),
                &Options::default(),
                &d()
            )
            .unwrap_err()
            .to_string(),
            "git_areas_source_unavailable",
            "{field}"
        );
        assert_eq!(state(&f.f.engine), corrupt);
        f.f.engine
            .store
            .db
            .execute(
                "UPDATE records SET value=?3 WHERE kind=?1 AND id=?2",
                rusqlite::params![meaning_build::GRAPH_KIND, f.id, raw],
            )
            .unwrap();
    }
    assert_eq!(state(&f.f.engine), before);
    assert_eq!(std::fs::read(&f.marker).unwrap(), calls);
}
