use serde_json::{Value, json};
use std::collections::BTreeMap;
use yoke::{
    capture::{CaptureOptions, Deadline},
    ledger::{self, State, Usage},
    model::{Snapshot, SourceFile, identity},
};

fn d() -> Deadline {
    Deadline::new(5_000)
}

fn accounting() -> (String, Value, String, Value) {
    let id = "a".repeat(64);
    let task = "Original human task label";
    let usage = json!({"inputTokens":17,"outputTokens":4,"cachedInputTokens":5});
    let attempt = json!({"schema":1,"id":id,"task":task,"currency":"EUR","provider":"original-provider",
        "state":"unknown","reservedMicros":23,"actualMicros":null,"usage":usage,
        "events":{"response":usage},"accounting":"provider_reported_unverified_or_unknown"});
    let row = json!({"schema":1,"task":task,"currency":"EUR","limitMicros":1000,
        "totals":{"actualMicros":9,"reservedMicros":23,"unknown":1,"encumberedMicros":32,"attempts":3,"reservationExceeded":false}});
    (id, attempt, identity(&task), row)
}

#[test]
fn archived_attempt_preserves_unknown_cost_original_currency_and_response_usage() {
    let (id, attempt, task_key, task) = accounting();
    let result = ledger::checked_archived_attempt(&id, &attempt, &task_key, &task, &d()).unwrap();
    assert_eq!(result.attempt_id, id);
    assert_eq!(result.task_id, "Original human task label");
    assert_eq!(result.provider, "original-provider");
    assert_eq!(result.currency, "EUR");
    assert_eq!(result.state, State::Unknown);
    assert_eq!(result.reserved_micros, 23);
    assert_eq!(result.actual_cost_micros, None);
    assert_eq!(
        result.reported_usage,
        Some(Usage {
            input_tokens: 17,
            output_tokens: 4,
            cached_input_tokens: 5
        })
    );
    assert_eq!(result.response_usage, result.reported_usage);
    assert_ne!(task_key, result.task_id);
}

#[test]
fn archived_attempt_keeps_response_usage_distinct_from_later_reconciled_usage() {
    let (id, mut attempt, task_key, task) = accounting();
    attempt["state"] = json!("settled");
    attempt["actualMicros"] = json!(7);
    let aggregate = json!({"inputTokens":20,"outputTokens":8,"cachedInputTokens":6});
    attempt["events"]["later-reconciliation"] = aggregate.clone();
    attempt["usage"] = aggregate;
    let result = ledger::checked_archived_attempt(&id, &attempt, &task_key, &task, &d()).unwrap();
    assert_eq!(result.state, State::Settled);
    assert_eq!(result.actual_cost_micros, Some(7));
    assert_eq!(result.reported_usage.unwrap().output_tokens, 8);
    assert_eq!(result.response_usage.unwrap().output_tokens, 4);
}

#[test]
fn archived_attempt_requires_exact_hashed_task_record_key_and_row_pairing() {
    let (id, attempt, task_key, task) = accounting();
    for wrong_key in ["Original human task label".to_owned(), "b".repeat(64)] {
        assert!(ledger::checked_archived_attempt(&id, &attempt, &wrong_key, &task, &d()).is_err());
    }
    let mut wrong = task.clone();
    wrong["task"] = json!("A different task");
    assert!(
        ledger::checked_archived_attempt(
            &id,
            &attempt,
            &identity(&"A different task"),
            &wrong,
            &d()
        )
        .is_err()
    );
    wrong = task.clone();
    wrong["currency"] = json!("USD");
    assert!(ledger::checked_archived_attempt(&id, &attempt, &task_key, &wrong, &d()).is_err());
    let mut wrong = attempt.clone();
    wrong["id"] = json!("b".repeat(64));
    assert!(ledger::checked_archived_attempt(&id, &wrong, &task_key, &task, &d()).is_err());
}

#[test]
fn archived_attempt_reuses_state_event_and_task_total_validation() {
    let (id, attempt, task_key, task) = accounting();
    for (field, value) in [
        ("state", json!("settled")),
        ("accounting", json!("verified_billing")),
        (
            "usage",
            json!({"inputTokens":17,"outputTokens":99,"cachedInputTokens":5}),
        ),
        ("reservedMicros", json!(u64::MAX)),
    ] {
        let mut wrong = attempt.clone();
        wrong[field] = value;
        assert!(ledger::checked_archived_attempt(&id, &wrong, &task_key, &task, &d()).is_err());
    }
    let mut wrong = task.clone();
    wrong["totals"]["encumberedMicros"] = json!(31);
    assert!(ledger::checked_archived_attempt(&id, &attempt, &task_key, &wrong, &d()).is_err());
    let mut wrong = attempt.clone();
    wrong["grantProvider"] = json!(true);
    assert!(ledger::checked_archived_attempt(&id, &wrong, &task_key, &task, &d()).is_err());
}

#[test]
fn archived_attempt_byte_budget_and_deadline_fail_without_replaying_live_state() {
    let (id, attempt, task_key, task) = accounting();
    let mut large = attempt.clone();
    let events = (0..256)
        .map(|index| {
            (
                format!("{index:03}{}", "\\".repeat(125)),
                attempt["usage"].clone(),
            )
        })
        .collect::<serde_json::Map<String, Value>>();
    large["events"] = Value::Object(events);
    assert!(serde_json::to_vec(&large).unwrap().len() > 64_000);
    assert!(ledger::checked_archived_attempt(&id, &large, &task_key, &task, &d()).is_err());
    let mut large = task.clone();
    large["untrustedPadding"] = json!("x".repeat(64_001));
    assert!(ledger::checked_archived_attempt(&id, &attempt, &task_key, &large, &d()).is_err());
    assert!(
        ledger::checked_archived_attempt(&id, &attempt, &task_key, &task, &Deadline::new(0))
            .is_err()
    );
}

fn native(epoch: &str, configured: bool) -> (Snapshot, Value, CaptureOptions) {
    let options = if configured {
        CaptureOptions {
            only_dirs: vec!["target".into()],
            read_only_dependency_dirs: vec!["target".into()],
            language_overrides: BTreeMap::from([("custom".into(), "rust".into())]),
            ..Default::default()
        }
    } else {
        CaptureOptions::default()
    };
    let mut snapshot = Snapshot {
        id: String::new(),
        workspace_id: "c".repeat(64),
        kind: "disk".into(),
        files: vec![SourceFile {
            path: if configured {
                "target/main.custom"
            } else {
                "src/main.rs"
            }
            .into(),
            hash: "d".repeat(64),
            size: 14,
            language: "rust".into(),
            mode: 0o100644,
        }],
        omissions: vec![],
        capture_consistency: "stable-double-scan".into(),
    };
    snapshot.id = if configured {
        identity(&(
            &snapshot.workspace_id,
            "disk",
            epoch,
            &snapshot.files,
            &snapshot.omissions,
            identity(&options),
        ))
    } else {
        identity(&(
            &snapshot.workspace_id,
            "disk",
            epoch,
            &snapshot.files,
            &snapshot.omissions,
        ))
    };
    let record = configured.then(|| json!({"snapshotId":snapshot.id,"workspaceId":snapshot.workspace_id,"analyzer":epoch,"optionsHash":identity(&options),"options":options}));
    let proof = json!({"schemaVersion":1,"nodes":[{"snapshot":snapshot,"proof":{"kind":"native","options":record}}]});
    (snapshot, proof, options)
}

fn derived(base: &Snapshot, proof: &Value, options: &CaptureOptions) -> (Snapshot, Value) {
    let mut snapshot = base.clone();
    snapshot.kind = "derived".into();
    // Source-scope inheritance does not itself attest an extractor generation.
    snapshot.id = identity(&(
        "authored scope-only derived source",
        &base.id,
        &snapshot.files,
    ));
    let record = json!({"snapshotId":snapshot.id,"workspaceId":snapshot.workspace_id,"originSnapshotId":base.id,
        "originSnapshotHash":identity(base),"derivedSnapshotHash":identity(&snapshot),"optionsHash":identity(options)});
    let mut nodes = vec![json!({"snapshot":snapshot,"proof":{"kind":"derived","record":record}})];
    nodes.extend(proof["nodes"].as_array().unwrap().iter().cloned());
    (snapshot, json!({"schemaVersion":1,"nodes":nodes}))
}

#[test]
fn archived_capture_recovers_exact_v8_and_v9_default_and_configured_options() {
    for epoch in ["yoke-tree-sitter-8", "yoke-tree-sitter-9"] {
        for configured in [false, true] {
            let (snapshot, scope, options) = native(epoch, configured);
            assert_eq!(
                yoke::capture::archived_capture_options(&snapshot, &scope, &d()).unwrap(),
                options
            );
            if !configured {
                assert_eq!(
                    yoke::capture::archived_capture_options(&snapshot, &Value::Null, &d()).unwrap(),
                    CaptureOptions::default()
                );
            } else {
                assert!(
                    yoke::capture::archived_capture_options(&snapshot, &Value::Null, &d()).is_err()
                );
            }
        }
    }
}

#[test]
fn archived_capture_scope_only_derived_lineage_returns_acquisition_policy_without_an_epoch_upgrade()
{
    let (base, scope, options) = native("yoke-tree-sitter-8", true);
    let (snapshot, proof) = derived(&base, &scope, &options);
    assert_eq!(
        yoke::capture::archived_capture_options(&snapshot, &proof, &d()).unwrap(),
        options
    );
    let mut wrong = proof.clone();
    wrong["nodes"][0]["proof"]["record"]["optionsHash"] = json!("0".repeat(64));
    assert!(yoke::capture::archived_capture_options(&snapshot, &wrong, &d()).is_err());
    let mut missing = proof.clone();
    missing["nodes"].as_array_mut().unwrap().pop();
    assert!(yoke::capture::archived_capture_options(&snapshot, &missing, &d()).is_err());
}

#[test]
fn archived_capture_overlay_uses_its_exact_base_scope_and_checked_revision() {
    let (base, scope, options) = native("yoke-tree-sitter-8", true);
    let mut snapshot = base.clone();
    snapshot.kind = "overlay".into();
    snapshot.capture_consistency = "overlay-from-stable-disk-and-immutable-buffers".into();
    let session = "e".repeat(64);
    let versions = BTreeMap::<String, u32>::new();
    snapshot.id = identity(&(
        "yoke-tree-sitter-9",
        &base.id,
        &session,
        1u64,
        &versions,
        &snapshot.files,
    ));
    let node = json!({"snapshot":snapshot,"proof":{"kind":"overlay","record":{"snapshotId":snapshot.id,"baseSnapshotId":base.id,"sessionId":session,"revision":1,"versions":versions}}});
    let proof = json!({"schemaVersion":1,"nodes":[node,scope["nodes"][0]]});
    assert_eq!(
        yoke::capture::archived_capture_options(&snapshot, &proof, &d()).unwrap(),
        options
    );
    let mut wrong = proof.clone();
    wrong["nodes"][0]["proof"]["record"]["revision"] = json!(2);
    assert!(yoke::capture::archived_capture_options(&snapshot, &wrong, &d()).is_err());
}

#[test]
fn archived_capture_rejects_resealed_protected_paths_unknown_policy_and_noncanonical_envelopes() {
    let (snapshot, proof, _) = native("yoke-tree-sitter-9", false);
    let mut wrong = proof.clone();
    wrong["futureAuthority"] = json!(true);
    assert!(yoke::capture::archived_capture_options(&snapshot, &wrong, &d()).is_err());
    let mut altered = snapshot.clone();
    altered.files[0].path = ".env".into();
    altered.id = identity(&(
        &altered.workspace_id,
        "disk",
        "yoke-tree-sitter-9",
        &altered.files,
        &altered.omissions,
    ));
    let protected = json!({"schemaVersion":1,"nodes":[{"snapshot":altered,"proof":{"kind":"native","options":null}}]});
    assert!(yoke::capture::archived_capture_options(&altered, &protected, &d()).is_err());
    let (unknown, scope, _) = native("yoke-tree-sitter-99", true);
    assert!(yoke::capture::archived_capture_options(&unknown, &scope, &d()).is_err());
}

#[test]
fn archived_capture_deadline_and_lineage_capacity_are_checked_before_replay() {
    let (snapshot, proof, _) = native("yoke-tree-sitter-9", false);
    assert!(yoke::capture::archived_capture_options(&snapshot, &proof, &Deadline::new(0)).is_err());
    let mut too_many = proof.clone();
    too_many["nodes"] = json!(vec![proof["nodes"][0].clone(); 17]);
    assert!(yoke::capture::archived_capture_options(&snapshot, &too_many, &d()).is_err());
    let mut cyclic = proof.clone();
    cyclic["nodes"] = json!(vec![proof["nodes"][0].clone(); 2]);
    assert!(yoke::capture::archived_capture_options(&snapshot, &cyclic, &d()).is_err());
}
