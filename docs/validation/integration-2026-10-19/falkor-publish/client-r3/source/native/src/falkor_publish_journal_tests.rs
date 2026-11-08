use super::*;
use serde_json::json;
fn plan() -> Plan {
    let target=serde_json::from_value(json!({"baseUrl":"redis://127.0.0.1:6379/","address":"127.0.0.1:6379","namespace":"owned \u{1f4a1}","user":"default","useSystemRoots":false})).unwrap();
    Plan::bootstrap(target, &Deadline::new(1000)).unwrap()
}
#[test]
fn durable_intent_blocks_resend_until_positive_read_recovery_is_recorded() {
    let plan = plan();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(format!("falkor-{}.jsonl", plan.id()));
    let d = Deadline::new(10000);
    let mut journal = Journal::open(&path, &plan, true, &d).unwrap();
    journal.approve(&plan, now().unwrap() + 60000, &d).unwrap();
    journal.intent(&plan, Operation::Initialize, &d).unwrap();
    drop(journal);
    let mut journal = Journal::open(&path, &plan, false, &d).unwrap();
    assert!(journal.writable().is_err());
    assert_eq!(journal.pending(), Some(Operation::Initialize));
    assert!(journal.intent(&plan, Operation::Initialize, &d).is_err());
    journal
        .confirmed(
            &plan,
            Operation::Initialize,
            &json!({"positiveRead":true}),
            true,
            &d,
        )
        .unwrap();
    assert!(journal.writable().is_ok());
    assert_eq!(journal.completed(), 1);
    for op in super::super::operations(plan.mode()).into_iter().skip(1) {
        journal.intent(&plan, op, &d).unwrap();
        journal
            .confirmed(&plan, op, &json!({"confirmed":true}), false, &d)
            .unwrap();
    }
    assert_eq!(journal.completed(), 9);
    assert!(journal.intent(&plan, Operation::Initialize, &d).is_err());
    drop(journal);
    assert_eq!(
        Journal::open(&path, &plan, false, &d).unwrap().completed(),
        9
    );
}
#[test]
fn exclusive_guarded_file_refuses_hardlinks_aliases_truncation_and_changed_chains() {
    let plan = plan();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(format!("falkor-{}.jsonl", plan.id()));
    let d = Deadline::new(10000);
    let mut journal = Journal::open(&path, &plan, true, &d).unwrap();
    journal.approve(&plan, now().unwrap() + 60000, &d).unwrap();
    assert!(Journal::open(&path, &plan, false, &d).is_err());
    drop(journal);
    let original = fs::read(&path).unwrap();
    let alias = dir.path().join("alias");
    fs::hard_link(&path, &alias).unwrap();
    assert!(Journal::open(&path, &plan, false, &d).is_err());
    fs::remove_file(alias).unwrap();
    fs::write(&path, &original[..original.len() - 1]).unwrap();
    assert!(Journal::open(&path, &plan, false, &d).is_err());
    let mut changed = original.clone();
    let i = changed.iter().position(|b| *b == b'1').unwrap();
    changed[i] = b'2';
    fs::write(&path, changed).unwrap();
    assert!(Journal::open(&path, &plan, false, &d).is_err());
    fs::write(&path, &original).unwrap();
    assert!(Journal::open(&path, &plan, false, &d).is_ok());
    for bad in [
        dir.path().join("other.jsonl"),
        dir.path()
            .join("..")
            .join(format!("falkor-{}.jsonl", plan.id())),
        dir.path()
            .join(format!("falkor-{}.jsonl:stream", plan.id())),
    ] {
        assert!(Journal::open(&bad, &plan, true, &d).is_err());
    }
}
#[test]
fn expired_or_regressed_approval_cannot_send_but_positive_recovery_can_be_recorded() {
    let plan = plan();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(format!("falkor-{}.jsonl", plan.id()));
    let d = Deadline::new(10000);
    let mut journal = Journal::open(&path, &plan, true, &d).unwrap();
    journal.approve(&plan, now().unwrap() + 60000, &d).unwrap();
    journal.intent(&plan, Operation::Initialize, &d).unwrap();
    drop(journal);
    let mut events = fs::read_to_string(&path)
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str::<Event>(s).unwrap())
        .collect::<Vec<_>>();
    let past = now().unwrap() - 60000;
    let mut previous = None;
    for (i, event) in events.iter_mut().enumerate() {
        event.created_at_ms = past + i as u64;
        event.expires_at_ms = past + 500;
        event.previous_hash = previous;
        event.hash = event.identity();
        previous = Some(event.hash.clone());
    }
    let bytes = events
        .iter()
        .map(|e| serde_json::to_string(e).unwrap() + "\n")
        .collect::<String>();
    fs::write(&path, bytes).unwrap();
    let mut journal = Journal::open(&path, &plan, false, &d).unwrap();
    assert!(journal.writable().is_err());
    journal
        .confirmed(
            &plan,
            Operation::Initialize,
            &json!({"positiveRead":true}),
            true,
            &d,
        )
        .unwrap();
    assert_eq!(journal.completed(), 1);
    assert!(journal.writable().is_err());
    drop(journal);
    assert_eq!(
        Journal::open(&path, &plan, false, &d).unwrap().completed(),
        1
    );
}
