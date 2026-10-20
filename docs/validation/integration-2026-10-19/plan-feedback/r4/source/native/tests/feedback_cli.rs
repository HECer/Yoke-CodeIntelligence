//! Real immutable sources, original transcript bytes and independently expected votes.
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    process::{Command, Output},
};
use yoke::{
    Engine,
    capture::Deadline,
    graph_view,
    knowledge::{
        self,
        feedback::{self, Draft, Options, RankInput},
    },
    model::{Snapshot, hash},
};

#[path = "support/meaning_comparison_fixture.rs"]
mod historical_support;

const SOURCE: &str = "// Invoice policy café 🌍\r\npub fn invoice() -> u32 { 17 }\r\n";
struct Fixture {
    dir: tempfile::TempDir,
    engine: Engine,
    snapshot: Snapshot,
    pin: Value,
    knowledge_id: String,
}
fn pin(view: &graph_view::View) -> Value {
    json!({"workspaceId":view.workspace_id,"snapshotId":view.snapshot_id,"viewId":view.id,
        "viewArtifactId":view.artifact_id,"baseGraphHash":view.base_graph_hash})
}
fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("source");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("billing.rs"), SOURCE).unwrap();
    fs::write(root.join("routing.rs"), "pub fn route() {}\n").unwrap();
    let mut engine = Engine::open(&root).unwrap();
    let d = Deadline::new(60000);
    let (snapshot, graph, _, _) = engine.index(&d).unwrap();
    let view =
        graph_view::build(&engine.store, &snapshot, &graph, &Default::default(), &d).unwrap();
    let memory = knowledge::remember(
        &engine.store,
        &snapshot,
        json!({"kind":"hypothesis","text":"Invoice uses integer units.",
        "paths":["billing.rs"],"actor":"fixture-author"}),
        &d,
    )
    .unwrap();
    Fixture {
        dir,
        engine,
        snapshot,
        pin: pin(&view),
        knowledge_id: memory["id"].as_str().unwrap().to_owned(),
    }
}
fn draft(
    f: &Fixture,
    actor: &str,
    session: &str,
    sequence: u32,
    time: u64,
    outcome: &str,
) -> Value {
    let quote = "// Invoice policy café 🌍";
    json!({"schemaVersion":1,"origin":f.pin,"evaluated":f.pin,"knowledgeId":f.knowledge_id,
        "actor":actor,"session":session,"sequence":sequence,"occurredAtMs":time,"outcome":outcome,
        "rationale":"Explicit authored fixture outcome.","sourceEvidence":[{"path":"billing.rs","contentHash":hash(SOURCE),
        "startByte":0,"endByte":quote.len(),"quote":quote}],"transcriptEvidence":[]})
}
fn record(f: &Fixture, value: Value) -> Value {
    feedback::record(
        &f.engine.store,
        &serde_json::from_value::<Draft>(value).unwrap(),
        &Options::default(),
        &Deadline::new(60000),
    )
    .unwrap()
}
fn query(f: &Fixture, now: u64) -> Value {
    json!({"schemaVersion":1,"origin":f.pin,"current":f.pin,"knowledgeIds":[f.knowledge_id],
        "asOfMs":now,"decayWindowDays":1,"limit":20})
}
fn rank(f: &Fixture, input: Value) -> Value {
    feedback::rank(
        &f.engine.store,
        &serde_json::from_value::<RankInput>(input).unwrap(),
        &Options::default(),
        &Deadline::new(60000),
    )
    .unwrap()
}
type StoreState = (Vec<(String, String, String)>, BTreeMap<String, String>);
fn state(engine: &Engine) -> StoreState {
    let mut statement = engine
        .store
        .db
        .prepare("SELECT kind,id,value FROM records ORDER BY kind,id")
        .unwrap();
    let rows = statement
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    let blobs = fs::read_dir(engine.store.dir.join("blobs"))
        .unwrap()
        .map(|entry| {
            let path = entry.unwrap().path();
            (
                path.file_name().unwrap().to_str().unwrap().to_owned(),
                hash(fs::read(&path).unwrap()),
            )
        })
        .collect();
    (rows, blobs)
}
fn host(f: &Fixture, name: &str, value: &Value) -> std::path::PathBuf {
    let path = f.dir.path().join(name);
    fs::write(&path, serde_json::to_vec(value).unwrap()).unwrap();
    path
}
fn command(root: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_yoke"));
    command.args([
        "--root",
        root.to_str().unwrap(),
        "--timeout",
        "60000",
        "--max-bytes",
        "4000000",
    ]);
    command
}
fn success(output: Output) -> Value {
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.ends_with(b"\n"));
    serde_json::from_slice(&output.stdout).unwrap()
}
#[test]
fn feedback_cli_records_one_source_bound_event_then_ranks_without_source_changes() {
    let f = fixture();
    let input = host(
        &f,
        "event.json",
        &draft(&f, "Ada", "s1", 1, 1000, "success"),
    );
    let created = success(
        command(&f.engine.root)
            .args(["--allow-writes", "feedback", "record", "--input"])
            .arg(input)
            .output()
            .unwrap(),
    );
    assert_eq!(created["inserted"], true);
    assert_eq!(created["executionVerified"], false);
    assert_eq!(created["modelCalls"], 0);
    assert_eq!(
        created["sourceProofs"][0]["origin"]["viewId"],
        f.pin["viewId"]
    );
    let input = host(&f, "rank.json", &query(&f, 1000));
    let before = state(&f.engine);
    let ranked = success(
        command(&f.engine.root)
            .args(["feedback", "rank", "--input"])
            .arg(input)
            .output()
            .unwrap(),
    );
    assert_eq!(ranked["items"][0]["effectiveScore"], 1000);
    assert_eq!(ranked["items"][0]["votingSessions"], 1);
    assert_eq!(ranked["independentCorroborationEstablished"], false);
    assert_eq!(ranked["sourceWritten"], false);
    assert_eq!(state(&f.engine), before);
    assert_eq!(
        fs::read(f.engine.root.join("billing.rs")).unwrap(),
        SOURCE.as_bytes()
    );
}
#[test]
fn identical_replay_is_idempotent_and_a_conflicting_sequence_cannot_rewrite_history() {
    let f = fixture();
    let first = draft(&f, "Ada", "s1", 1, 1000, "success");
    let saved = record(&f, first.clone());
    let before = state(&f.engine);
    let again = record(&f, first.clone());
    assert_eq!(again["id"], saved["id"]);
    assert_eq!(again["inserted"], false);
    assert_eq!(state(&f.engine), before);
    let mut conflict = first;
    conflict["outcome"] = json!("failure");
    assert_eq!(
        feedback::record(
            &f.engine.store,
            &serde_json::from_value(conflict).unwrap(),
            &Options::default(),
            &Deadline::new(60000)
        )
        .unwrap_err()
        .to_string(),
        "feedback_sequence_conflict"
    );
    assert_eq!(state(&f.engine), before);
    record(&f, draft(&f, "Ada", "s1", 2, 2000, "failure"));
    let result = rank(&f, query(&f, 2000));
    assert_eq!(result["items"][0]["effectiveScore"], -1000);
    assert_eq!(result["items"][0]["votingSessions"], 1);
    assert_eq!(result["items"][0]["events"].as_array().unwrap().len(), 2);
}
#[test]
fn integer_decay_future_exclusion_and_label_corroboration_have_an_independent_numeric_oracle() {
    let f = fixture();
    let day = 86_400_000;
    record(&f, draft(&f, "Ada", "a", 1, 1000 + day, "success"));
    record(&f, draft(&f, "Ben", "b", 1, 1000, "contradiction"));
    record(&f, draft(&f, "Cora", "c", 1, 1001 + day, "corroboration"));
    let result = rank(&f, query(&f, 1000 + day));
    // A fresh +1000 and a one-window-old -500 are the only eligible votes.
    assert_eq!(result["items"][0]["effectiveScore"], 500);
    assert_eq!(result["items"][0]["votingSessions"], 2);
    let events = result["items"][0]["events"].as_array().unwrap();
    let future = events
        .iter()
        .find(|value| value["event"]["draft"]["actor"] == "Cora")
        .unwrap();
    assert_eq!(future["disposition"], "future_dated");
    assert_eq!(future["contribution"], 0);
    assert_eq!(result["independentCorroborationEstablished"], false);
    assert_eq!(
        rank(&f, query(&f, u64::MAX))["items"][0]["effectiveScore"],
        0
    );
    assert_eq!(rank(&f, query(&f, 999))["items"][0]["votingSessions"], 0);
}
#[test]
fn sequence_timestamps_are_monotone_even_when_events_arrive_out_of_order() {
    let f = fixture();
    record(&f, draft(&f, "Ada", "a", 2, 2000, "success"));
    record(&f, draft(&f, "Ada", "a", 1, 1000, "failure"));
    let before = state(&f.engine);
    assert!(
        feedback::record(
            &f.engine.store,
            &serde_json::from_value(draft(&f, "Ada", "a", 3, 1999, "success")).unwrap(),
            &Options::default(),
            &Deadline::new(60000)
        )
        .is_err()
    );
    assert_eq!(state(&f.engine), before);
    assert_eq!(
        rank(&f, query(&f, 2000))["items"][0]["effectiveScore"],
        1000
    );
}
#[test]
fn every_source_view_pin_and_original_quote_remain_binding_without_publication() {
    let f = fixture();
    let before = state(&f.engine);
    for side in ["origin", "evaluated"] {
        for field in [
            "workspaceId",
            "snapshotId",
            "viewId",
            "viewArtifactId",
            "baseGraphHash",
        ] {
            let mut input = draft(&f, "Ada", "s1", 1, 1000, "success");
            input[side][field] = json!(hash(format!("wrong {side}/{field}")));
            assert!(
                feedback::record(
                    &f.engine.store,
                    &serde_json::from_value(input).unwrap(),
                    &Options::default(),
                    &Deadline::new(60000)
                )
                .is_err()
            );
            assert_eq!(state(&f.engine), before);
        }
    }
    for field in ["contentHash", "quote", "startByte", "endByte"] {
        let mut input = draft(&f, "Ada", "s1", 1, 1000, "success");
        input["sourceEvidence"][0][field] = match field {
            "contentHash" => json!(hash(b"wrong source")),
            "quote" => json!("invented quote"),
            _ => json!(1),
        };
        assert!(
            feedback::record(
                &f.engine.store,
                &serde_json::from_value(input).unwrap(),
                &Options::default(),
                &Deadline::new(60000)
            )
            .is_err()
        );
        assert_eq!(state(&f.engine), before);
    }
}
#[test]
fn a_resealed_event_record_cannot_invent_an_original_source_quote() {
    let f = fixture();
    let created = record(&f, draft(&f, "Ada", "s1", 1, 1000, "success"));
    let id = created["id"].as_str().unwrap();
    let mut raw: Value = f.engine.store.get_record(feedback::EVENT_KIND, id).unwrap();
    let text = raw["draft"]["sourceEvidence"][0]["quote"]
        .as_str()
        .unwrap()
        .to_owned();
    raw["draft"]["sourceEvidence"][0]["quote"] = json!(format!("!{}", &text[1..]));
    let typed: Draft = serde_json::from_value(raw["draft"].clone()).unwrap();
    let new_id =
        yoke::model::identity(&(feedback::EVENT_KIND, &f.engine.store.workspace_id, &typed));
    raw["id"] = json!(new_id);
    f.engine
        .store
        .db
        .execute(
            "DELETE FROM records WHERE kind=?1 AND id=?2",
            rusqlite::params![feedback::EVENT_KIND, id],
        )
        .unwrap();
    f.engine
        .store
        .db
        .execute(
            "INSERT INTO records VALUES(?1,?2,?3)",
            rusqlite::params![
                feedback::EVENT_KIND,
                new_id,
                serde_json::to_string(&raw).unwrap()
            ],
        )
        .unwrap();
    let before = state(&f.engine);
    assert!(
        feedback::rank(
            &f.engine.store,
            &serde_json::from_value(query(&f, 1000)).unwrap(),
            &Options::default(),
            &Deadline::new(60000)
        )
        .is_err()
    );
    assert_eq!(state(&f.engine), before);
}
#[test]
fn historical_ranking_uses_checked_snapshots_after_live_sources_change_or_disappear() {
    let mut f = fixture();
    record(&f, draft(&f, "Ada", "s1", 1, 1000, "success"));
    fs::write(f.engine.root.join("billing.rs"), "pub fn changed() {}\n").unwrap();
    let d = Deadline::new(60000);
    let (snapshot, graph, _, _) = f.engine.index(&d).unwrap();
    let view =
        graph_view::build(&f.engine.store, &snapshot, &graph, &Default::default(), &d).unwrap();
    let mut input = query(&f, 1000);
    input["current"] = pin(&view);
    let changed = rank(&f, input);
    assert_eq!(changed["items"][0]["anchors"][0]["state"], "changed");
    assert_eq!(changed["items"][0]["effectiveScore"], 0);
    fs::remove_file(f.engine.root.join("billing.rs")).unwrap();
    fs::remove_file(f.engine.root.join("routing.rs")).unwrap();
    let before = state(&f.engine);
    assert_eq!(
        rank(&f, query(&f, 1000))["items"][0]["effectiveScore"],
        1000
    );
    assert_eq!(state(&f.engine), before);
}
#[test]
fn an_observed_authored_successor_makes_the_old_knowledge_ineligible_without_claiming_truth() {
    let f = fixture();
    record(&f, draft(&f, "Ada", "s1", 1, 1000, "success"));
    let successor = knowledge::remember(
        &f.engine.store,
        &f.snapshot,
        json!({"kind":"correction","text":"Explicit authored correction.","paths":["billing.rs"],
        "actor":"fixture-author","supersedes":f.knowledge_id}),
        &Deadline::new(60000),
    )
    .unwrap();
    let result = rank(&f, query(&f, 1000));
    assert_eq!(result["items"][0]["authoredSupersessionObserved"], true);
    assert_eq!(
        result["items"][0]["authoredSupersessionIds"],
        json!([successor["id"]])
    );
    assert_eq!(result["items"][0]["eligible"], false);
    assert_eq!(result["items"][0]["rawScore"], 1000);
    assert_eq!(result["items"][0]["effectiveScore"], 0);
    assert_eq!(result["executionVerified"], false);
}

#[test]
fn resealed_successor_metadata_cannot_disable_knowledge_without_its_original_source_proof() {
    fn seal(value: &Value) -> String {
        let names = [
            "id",
            "snapshotId",
            "workspaceId",
            "kind",
            "text",
            "anchors",
            "actor",
            "createdAtMs",
            "createdAtNonce",
            "supersedes",
        ];
        let fields = names
            .iter()
            .map(|name| {
                let encoded = if *name == "anchors" {
                    let anchors = value["anchors"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|anchor| {
                            format!(
                                "{{\"path\":{},\"contentHash\":{}}}",
                                serde_json::to_string(&anchor["path"]).unwrap(),
                                serde_json::to_string(&anchor["contentHash"]).unwrap()
                            )
                        })
                        .collect::<Vec<_>>();
                    format!("[{}]", anchors.join(","))
                } else {
                    serde_json::to_string(&value[name]).unwrap()
                };
                format!("{}:{}", serde_json::to_string(name).unwrap(), encoded)
            })
            .collect::<Vec<_>>();
        hash(format!("{{{}}}", fields.join(",")))
    }
    for field in ["snapshot", "anchor"] {
        let f = fixture();
        record(&f, draft(&f, "Ada", "s1", 1, 1000, "success"));
        let successor = knowledge::remember(
            &f.engine.store,
            &f.snapshot,
            json!({"kind":"correction","text":"Authored successor.","paths":["billing.rs"],
                "actor":"Ada","supersedes":f.knowledge_id}),
            &Deadline::new(60000),
        )
        .unwrap();
        let id = successor["id"].as_str().unwrap();
        assert_eq!(rank(&f, query(&f, 1000))["items"][0]["effectiveScore"], 0);
        let mut raw: Value = f
            .engine
            .store
            .get_record("native_knowledge_v1", id)
            .unwrap();
        raw["id"] = json!("");
        assert_eq!(seal(&raw), id);
        if field == "snapshot" {
            raw["snapshotId"] = json!(hash("nonexistent successor snapshot"));
        } else {
            raw["anchors"][0]["contentHash"] = json!(hash("forged successor source"));
        }
        let resealed = seal(&raw);
        raw["id"] = json!(resealed);
        f.engine
            .store
            .db
            .execute(
                "UPDATE records SET id=?1,value=?2 WHERE kind='native_knowledge_v1' AND id=?3",
                rusqlite::params![resealed, serde_json::to_string(&raw).unwrap(), id],
            )
            .unwrap();
        let before = state(&f.engine);
        assert_eq!(
            feedback::rank(
                &f.engine.store,
                &serde_json::from_value(query(&f, 1000)).unwrap(),
                &Options::default(),
                &Deadline::new(60000)
            )
            .unwrap_err()
            .to_string(),
            "feedback_binding_invalid"
        );
        assert_eq!(state(&f.engine), before);
    }
}

#[test]
fn an_authored_successor_requires_its_snapshot_in_the_explicit_checked_selection() {
    let mut f = fixture();
    record(&f, draft(&f, "Ada", "s1", 1, 1000, "success"));
    let d = Deadline::new(60000);
    fs::write(
        f.engine.root.join("routing.rs"),
        "pub fn route_changed() {}\n",
    )
    .unwrap();
    let (snapshot, graph, _, _) = f.engine.index(&d).unwrap();
    let current =
        graph_view::build(&f.engine.store, &snapshot, &graph, &Default::default(), &d).unwrap();
    let successor = knowledge::remember(
        &f.engine.store,
        &snapshot,
        json!({"kind":"correction","text":"New snapshot correction.","paths":["billing.rs"],
            "actor":"Ada","supersedes":f.knowledge_id}),
        &d,
    )
    .unwrap();
    let before = state(&f.engine);
    assert_eq!(
        feedback::rank(
            &f.engine.store,
            &serde_json::from_value(query(&f, 1000)).unwrap(),
            &Options::default(),
            &d
        )
        .unwrap_err()
        .to_string(),
        "feedback_binding_invalid"
    );
    assert_eq!(state(&f.engine), before);
    let mut input = query(&f, 1000);
    input["current"] = pin(&current);
    let result = rank(&f, input);
    assert_eq!(result["items"][0]["stale"], false);
    assert_eq!(
        result["items"][0]["authoredSupersessionIds"],
        json!([successor["id"]])
    );
    assert_eq!(result["items"][0]["rawScore"], 1000);
    assert_eq!(result["items"][0]["effectiveScore"], 0);
    assert_eq!(state(&f.engine), before);
}
#[test]
fn explicit_transcript_import_preserves_utf8_crlf_and_mining_never_creates_events() {
    let f = fixture();
    let text = "User: café 🌍\r\nUser: tests passed for invoice.\r\n";
    let d = Deadline::new(60000);
    fs::write(
        f.dir.path().join("unselected-private-transcript.txt"),
        "tests failed; confirmed",
    )
    .unwrap();
    let imported = feedback::import_transcript(
        &f.engine.store,
        text.as_bytes(),
        "Selected fixture",
        "Ada",
        &Options::default(),
        &d,
    )
    .unwrap();
    let id = imported["id"].as_str().unwrap();
    assert_eq!(imported["sourceHash"], hash(text));
    assert_eq!(imported["ownershipAuthenticated"], false);
    assert_eq!(
        f.engine
            .store
            .read_blob_exact(&hash(text), text.len(), 1_000_000)
            .unwrap(),
        text.as_bytes()
    );
    let before = state(&f.engine);
    let mined = feedback::mine(&f.engine.store, id, &Options::default(), &d).unwrap();
    let candidates = mined["candidates"].as_array().unwrap();
    assert_eq!(candidates.len(), 1);
    let citation = &candidates[0]["source"];
    let start = citation["startByte"].as_u64().unwrap() as usize;
    let end = citation["endByte"].as_u64().unwrap() as usize;
    assert_eq!(
        &text.as_bytes()[start..end],
        citation["quote"].as_str().unwrap().as_bytes()
    );
    assert_eq!(mined["eventsCreated"], 0);
    assert_eq!(mined["semanticUnderstandingEstablished"], false);
    assert_eq!(state(&f.engine), before);
    let mut event = draft(&f, "Ada", "transcript", 1, 1000, "success");
    event["sourceEvidence"] = json!([]);
    event["transcriptEvidence"] = json!([{"transcriptId":id,"contentHash":hash(text),"startByte":start,"endByte":end,"quote":&text[start..end]}]);
    record(&f, event);
    assert_eq!(
        rank(&f, query(&f, 1000))["items"][0]["effectiveScore"],
        1000
    );
}
#[test]
fn mining_and_transcript_quote_tampering_fail_without_changing_any_state() {
    let f = fixture();
    let text = "café 🌍 tests failed\r\n";
    let d = Deadline::new(60000);
    let imported = feedback::import_transcript(
        &f.engine.store,
        text.as_bytes(),
        "Selected fixture",
        "Ada",
        &Options::default(),
        &d,
    )
    .unwrap();
    let id = imported["id"].as_str().unwrap();
    let before = state(&f.engine);
    assert!(
        feedback::mine(
            &f.engine.store,
            id,
            &Options {
                max_candidates: 0,
                ..Default::default()
            },
            &d
        )
        .is_err()
    );
    assert_eq!(state(&f.engine), before);
    let mut event = draft(&f, "Ada", "transcript", 1, 1000, "failure");
    event["sourceEvidence"] = json!([]);
    event["transcriptEvidence"] = json!([{"transcriptId":id,"contentHash":hash(text),"startByte":2,"endByte":4,"quote":"af"}]);
    assert!(
        feedback::record(
            &f.engine.store,
            &serde_json::from_value(event).unwrap(),
            &Options::default(),
            &d
        )
        .is_err()
    );
    assert_eq!(state(&f.engine), before);
    fs::write(
        f.engine.store.dir.join("blobs").join(hash(text)),
        "corrupt bytes",
    )
    .unwrap();
    let corrupted = state(&f.engine);
    assert!(feedback::mine(&f.engine.store, id, &Options::default(), &d).is_err());
    assert_eq!(state(&f.engine), corrupted);
}
#[test]
fn output_admission_happens_before_transcript_sql_or_cas_publication() {
    let f = fixture();
    let d = Deadline::new(60000);
    let first = "tests passed";
    let result = feedback::import_transcript(
        &f.engine.store,
        first.as_bytes(),
        "a",
        "Ada",
        &Options::default(),
        &d,
    )
    .unwrap();
    let exact = serde_json::to_vec(&result).unwrap().len();
    let before = state(&f.engine);
    assert!(
        feedback::import_transcript(
            &f.engine.store,
            b"tests failed",
            "b",
            "Ada",
            &Options {
                max_output_bytes: exact - 1,
                ..Default::default()
            },
            &d
        )
        .is_err()
    );
    assert_eq!(state(&f.engine), before);
    assert!(
        !f.engine
            .store
            .dir
            .join("blobs")
            .join(hash(b"tests failed"))
            .exists()
    );
    assert!(
        feedback::import_transcript(
            &f.engine.store,
            b"tests failed",
            "b",
            "Ada",
            &Options {
                max_output_bytes: exact,
                ..Default::default()
            },
            &d
        )
        .is_ok()
    );
}
#[test]
fn zero_resource_capacities_and_expired_or_cancelled_requests_publish_nothing() {
    use std::sync::{Arc, atomic::AtomicBool};
    let f = fixture();
    let input: Draft = serde_json::from_value(draft(&f, "Ada", "s1", 1, 1000, "success")).unwrap();
    let before = state(&f.engine);
    for options in [
        Options {
            max_events: 0,
            ..Default::default()
        },
        Options {
            max_pairs: 0,
            ..Default::default()
        },
        Options {
            max_input_bytes: 0,
            ..Default::default()
        },
        Options {
            max_source_bytes: 0,
            ..Default::default()
        },
        Options {
            max_output_bytes: 0,
            ..Default::default()
        },
    ] {
        assert!(
            feedback::record(&f.engine.store, &input, &options, &Deadline::new(60000)).is_err()
        );
        assert_eq!(state(&f.engine), before);
    }
    assert!(
        feedback::record(
            &f.engine.store,
            &input,
            &Options::default(),
            &Deadline::new(0)
        )
        .is_err()
    );
    let cancellation = Arc::new(AtomicBool::new(false));
    assert!(
        Deadline::run_bounded(&Deadline::new(60000), Arc::clone(&cancellation), || {
            let inherited = Deadline::new(60000);
            cancellation.store(true, std::sync::atomic::Ordering::SeqCst);
            assert_eq!(
                feedback::record(&f.engine.store, &input, &Options::default(), &inherited)
                    .unwrap_err()
                    .to_string(),
                "Request cancelled by host"
            );
            Ok(())
        })
        .is_err()
    );
    assert_eq!(state(&f.engine), before);
}
#[test]
fn read_transactions_and_sqlite_limits_survive_both_success_and_refusal() {
    use rusqlite::limits::Limit;
    let f = fixture();
    record(&f, draft(&f, "Ada", "s1", 1, 1000, "success"));
    let before = state(&f.engine);
    let old = f
        .engine
        .store
        .db
        .set_limit(Limit::SQLITE_LIMIT_LENGTH, 1_000_000)
        .unwrap();
    f.engine.store.db.execute_batch("BEGIN").unwrap();
    assert_eq!(
        rank(&f, query(&f, 1000))["items"][0]["effectiveScore"],
        1000
    );
    assert!(!f.engine.store.db.is_autocommit());
    assert_eq!(
        f.engine.store.db.limit(Limit::SQLITE_LIMIT_LENGTH).unwrap(),
        1_000_000
    );
    assert_eq!(
        feedback::record(
            &f.engine.store,
            &serde_json::from_value(draft(&f, "Ada", "s1", 2, 2000, "failure")).unwrap(),
            &Options::default(),
            &Deadline::new(60000)
        )
        .unwrap_err()
        .to_string(),
        "feedback_transaction_unavailable"
    );
    assert!(!f.engine.store.db.is_autocommit());
    f.engine.store.db.execute_batch("ROLLBACK").unwrap();
    f.engine
        .store
        .db
        .set_limit(Limit::SQLITE_LIMIT_LENGTH, old)
        .unwrap();
    assert_eq!(state(&f.engine), before);
}
#[test]
fn permission_and_strict_host_input_admission_precede_missing_workspace_creation() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("missing");
    let output = command(&root)
        .args(["feedback", "record", "--input"])
        .arg(dir.path().join("absent.json"))
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("feedback_write_permission_required"));
    assert!(output.stdout.is_empty());
    assert!(!root.exists());
    let input = dir.path().join("invalid.json");
    fs::write(&input, b"{\"schemaVersion\":1,\"schemaVersion\":2}").unwrap();
    let output = command(&root)
        .args(["--allow-writes", "feedback", "record", "--input"])
        .arg(input)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("feedback_input_invalid"));
    assert!(output.stdout.is_empty());
    assert!(!root.exists());
}
#[test]
fn retention_recognizes_transcript_roots_and_refuses_a_corrupt_feedback_registration() {
    let f = fixture();
    let d = Deadline::new(60000);
    record(&f, draft(&f, "Ada", "s1", 1, 1000, "success"));
    let imported = feedback::import_transcript(
        &f.engine.store,
        b"tests passed",
        "selected",
        "Ada",
        &Options::default(),
        &d,
    )
    .unwrap();
    let plan = yoke::retention::plan(&f.engine.store, &Default::default(), &d).unwrap();
    assert_eq!(plan["unknownDependencyKinds"], json!([]));
    let orphan = f.engine.store.blob(b"orphan feedback bytes").unwrap();
    let moved = yoke::maintenance::collect(
        &f.engine.store,
        &yoke::maintenance::Policy {
            apply: true,
            minimum_age_ms: 0,
            purge_quarantine: false,
        },
        &d,
    )
    .unwrap();
    assert!(moved["quarantinedBlobs"].as_u64().unwrap() >= 1);
    assert!(
        f.engine
            .store
            .dir
            .join("blobs")
            .join(imported["sourceHash"].as_str().unwrap())
            .is_file()
    );
    let purge = yoke::retention::purge_plan(&f.engine.store, 0, &d).unwrap();
    assert!(
        purge["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["blobId"] == orphan)
    );
    assert!(
        !purge["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["blobId"] == imported["sourceHash"])
    );
    let id = imported["id"].as_str().unwrap();
    let mut header: Value = f
        .engine
        .store
        .get_record(feedback::TRANSCRIPT_KIND, id)
        .unwrap();
    header["label"] = json!("forged label");
    f.engine
        .store
        .db
        .execute(
            "UPDATE records SET value=?1 WHERE kind=?2 AND id=?3",
            rusqlite::params![
                serde_json::to_string(&header).unwrap(),
                feedback::TRANSCRIPT_KIND,
                id
            ],
        )
        .unwrap();
    let before = state(&f.engine);
    assert!(yoke::retention::plan(&f.engine.store, &Default::default(), &d).is_err());
    assert_eq!(state(&f.engine), before);
}

#[test]
fn historical_meaning_receipts_remain_proofs_without_becoming_new_quote_authority() {
    let f = historical_support::fixture(yoke::analysis::AnalyzerEpoch::V9);
    let d = Deadline::new(60000);
    let memory = knowledge::remember(
        &f.engine.store,
        &f.snapshot,
        json!({"kind":"hypothesis","text":"History uses the captured policy.",
            "paths":["history.rs"],"actor":"fixture-author"}),
        &d,
    )
    .unwrap();
    let selected = pin(&f.view);
    let make = |text: &str| {
        json!({"schemaVersion":1,"origin":selected,"evaluated":selected,
        "knowledgeId":memory["id"],"actor":"Ada","session":"history","sequence":1,"occurredAtMs":1000,
        "outcome":"success","rationale":"Selected original UTF-8 source.","transcriptEvidence":[],
        "sourceEvidence":[{"path":"history.rs","contentHash":hash(text),"startByte":0,"endByte":text.len(),"quote":text}]})
    };
    for name in ["a.rs", "b.rs", "history.rs", "GUIDE.md"] {
        fs::remove_file(f.engine.root.join(name)).unwrap();
    }
    let calls = fs::read(&f.marker).unwrap();
    let before = state(&f.engine);
    assert!(
        feedback::record(
            &f.engine.store,
            &serde_json::from_value(make(historical_support::HISTORY_OLD)).unwrap(),
            &Options::default(),
            &d
        )
        .is_err()
    );
    assert_eq!(state(&f.engine), before);
    let result = feedback::record(
        &f.engine.store,
        &serde_json::from_value(make(historical_support::HISTORY_NEW)).unwrap(),
        &Options::default(),
        &d,
    )
    .unwrap();
    let proof = &result["sourceProofs"][0]["evaluated"];
    let layer = f.view.meaning.as_ref().unwrap();
    assert_eq!(proof["overlays"], json!(f.view.overlays));
    assert_eq!(proof["viewComplete"], f.view.complete);
    assert_eq!(proof["viewLimitations"], json!(f.view.limitations));
    assert_eq!(proof["meaningScope"], json!(layer.scope));
    assert_eq!(proof["meaningSelections"], json!(layer.selections));
    assert_eq!(
        proof["meaningPackets"].as_array().unwrap().len(),
        layer.packets.len()
    );
    for (actual, packet) in proof["meaningPackets"]
        .as_array()
        .unwrap()
        .iter()
        .zip(&layer.packets)
    {
        let expected = serde_json::to_value(packet).unwrap();
        for key in [
            "selectionIndex",
            "index",
            "inputHash",
            "unitSetHash",
            "contractHash",
            "sourceBindings",
            "provenance",
            "coverage",
        ] {
            assert_eq!(actual[key], expected[key]);
        }
    }
    let scopes = proof["sourceScopes"].as_array().unwrap();
    assert!(scopes.iter().any(
        |scope| scope["snapshotId"] == f.original.id && scope["role"] == "receipt_replay_only"
    ));
    assert!(
        scopes.iter().any(
            |scope| scope["snapshotId"] == f.snapshot.id && scope["role"] == "primary_evidence"
        )
    );
    let query: RankInput = serde_json::from_value(
        json!({"schemaVersion":1,"origin":selected,"current":selected,
        "knowledgeIds":[memory["id"]],"asOfMs":1000,"decayWindowDays":1,"limit":1}),
    )
    .unwrap();
    assert_eq!(
        feedback::rank(&f.engine.store, &query, &Options::default(), &d).unwrap()["items"][0]["effectiveScore"],
        1000
    );
    assert_eq!(fs::read(&f.marker).unwrap(), calls);
    // Even a replay-only historical source remains part of the complete proof closure.
    fs::write(
        f.engine
            .store
            .dir
            .join("blobs")
            .join(hash(historical_support::HISTORY_OLD)),
        b"corrupt original source",
    )
    .unwrap();
    let before = state(&f.engine);
    assert!(feedback::rank(&f.engine.store, &query, &Options::default(), &d).is_err());
    assert_eq!(state(&f.engine), before);
    assert_eq!(fs::read(&f.marker).unwrap(), calls);
}

#[test]
fn exact_rank_input_source_pair_and_output_limits_admit_boundary_then_refuse_below() {
    let f = fixture();
    record(&f, draft(&f, "Ada", "s1", 1, 1000, "success"));
    let query: RankInput = serde_json::from_value(query(&f, 1000)).unwrap();
    let d = Deadline::new(60000);
    let baseline = feedback::rank(&f.engine.store, &query, &Options::default(), &d).unwrap();
    let exact = Options {
        max_input_bytes: baseline["inputBytes"].as_u64().unwrap() as usize,
        max_source_bytes: baseline["sourceBytes"].as_u64().unwrap() as usize,
        max_pairs: 1,
        max_output_bytes: serde_json::to_vec(&baseline).unwrap().len(),
        ..Default::default()
    };
    let before = state(&f.engine);
    assert_eq!(
        feedback::rank(&f.engine.store, &query, &exact, &d).unwrap(),
        baseline
    );
    for options in [
        Options {
            max_input_bytes: exact.max_input_bytes - 1,
            ..exact.clone()
        },
        Options {
            max_source_bytes: exact.max_source_bytes - 1,
            ..exact.clone()
        },
        Options {
            max_pairs: 0,
            ..exact.clone()
        },
        Options {
            max_output_bytes: exact.max_output_bytes - 1,
            ..exact.clone()
        },
    ] {
        assert_eq!(
            feedback::rank(&f.engine.store, &query, &options, &d)
                .unwrap_err()
                .to_string(),
            "feedback_capacity_exceeded"
        );
        assert_eq!(state(&f.engine), before);
    }
}

#[test]
fn feedback_host_flags_and_protected_inputs_refuse_before_missing_store_access() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("absent");
    let missing = dir.path().join("missing.json");
    for flags in [
        vec!["--snapshot", "invalid"],
        vec!["--analyzer-epoch", "9"],
        vec!["--trust-servers"],
    ] {
        let output = command(&root)
            .args(flags)
            .args(["feedback", "rank", "--input"])
            .arg(&missing)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(!root.exists());
    }
    let protected = dir.path().join(".env");
    fs::write(&protected, b"tests passed").unwrap();
    let output = command(&root)
        .args(["--allow-writes", "feedback", "import-transcript", "--input"])
        .arg(protected)
        .args(["--label", "selected", "--author", "Ada"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("feedback_input_invalid"));
    assert!(!root.exists());
}

#[cfg(windows)]
#[test]
fn windows_host_aliases_cannot_read_protected_files_or_alternate_streams() {
    let f = fixture();
    let before = state(&f.engine);
    fs::write(f.dir.path().join("secret.pem"), b"tests passed").unwrap();
    fs::create_dir(f.dir.path().join(".ssh")).unwrap();
    fs::write(f.dir.path().join(".ssh/config"), b"tests passed").unwrap();
    let regular = f.dir.path().join("selected.txt");
    fs::write(&regular, b"tests passed").unwrap();
    fs::write(f.dir.path().join("selected.txt:feedback"), b"tests failed").unwrap();
    let verbatim = format!(r"\\?\{}", regular.display());
    let linked = f.dir.path().join("linked.txt");
    std::os::windows::fs::symlink_file(&regular, &linked).unwrap();
    let actual = f.dir.path().join("selected-directory");
    fs::create_dir(&actual).unwrap();
    fs::write(actual.join("session.txt"), b"tests passed").unwrap();
    let directory_link = f.dir.path().join("linked-directory");
    std::os::windows::fs::symlink_dir(&actual, &directory_link).unwrap();
    for input in [
        f.dir.path().join("secret.pem."),
        f.dir.path().join(".ssh./config"),
        f.dir.path().join("selected.txt:feedback"),
        std::path::PathBuf::from(verbatim),
        linked,
        directory_link.join("session.txt"),
    ] {
        let output = command(&f.engine.root)
            .args(["--allow-writes", "feedback", "import-transcript", "--input"])
            .arg(input)
            .args(["--label", "selected", "--author", "Ada"])
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("feedback_input_invalid"));
        assert_eq!(state(&f.engine), before);
    }
}
