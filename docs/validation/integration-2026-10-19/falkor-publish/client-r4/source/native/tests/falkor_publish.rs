use serde_json::{Value, json};
use yoke::{
    Engine,
    capture::Deadline,
    falkor_publish::{self as publish, Operation, Plan, Target},
};
fn target() -> Target {
    serde_json::from_value(json!({"baseUrl":"redis://127.0.0.1:6379/","address":"127.0.0.1:6379","namespace":"owned \u{1f4a1}","user":"default","useSystemRoots":false})).unwrap()
}
fn fixture() -> (
    tempfile::TempDir,
    Engine,
    yoke::model::Snapshot,
    yoke::model::Graph,
) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("owned.rs"),
        "// owned \u{1f4a1}\r\nfn alpha(){beta();beta();missing();}\r\nfn beta(){alpha();}\r\n",
    )
    .unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let (s, g, _, _) = engine.index(&Deadline::new(30000)).unwrap();
    (dir, engine, s, g)
}
#[test]
fn offline_plans_keep_exact_saved_graph_rows_and_grant_no_host_authority() {
    let (_dir, engine, s, g) = fixture();
    let d = Deadline::new(30000);
    let plan = Plan::prepare(
        &engine.store,
        &s,
        &g,
        target(),
        "absent",
        &Default::default(),
        &d,
    )
    .unwrap();
    let payload = plan.payload().unwrap();
    assert_eq!(payload["entities"], json!(g.entities));
    assert_eq!(payload["observations"], json!(g.observations));
    assert_eq!(payload["evidence"], json!(g.evidence));
    assert_eq!(payload["sourceFiles"], json!(s.files));
    let decoded = Plan::decode(&plan.encode(&d).unwrap(), &d).unwrap();
    assert_eq!(decoded.id(), plan.id());
    assert_eq!(decoded.revision(), plan.revision());
    assert_eq!(decoded.inspect(&d).unwrap()["hostAuthorityGranted"], false);
    let command = publish::command(&plan, Operation::Publish, &d).unwrap();
    assert_eq!(command[0], b"GRAPH.QUERY");
    assert_eq!(command[1], plan.graph_key().as_bytes());
    let sql = std::str::from_utf8(&command[2]).unwrap();
    assert!(
        sql.starts_with("CYPHER ")
            && sql.contains("CALL { WITH scope MATCH")
            && !sql.contains("COUNT { MATCH")
    );
    assert!(!plan.graph_key().contains('\u{1f4a1}'));
}
#[test]
fn bootstrap_and_publication_operations_remain_separate_closed_programs() {
    let d = Deadline::new(30000);
    let bootstrap = Plan::bootstrap(target(), &d).unwrap();
    let ops = publish::operations(bootstrap.mode());
    assert_eq!(ops.len(), 9);
    assert_eq!(ops[0], Operation::Initialize);
    assert!(publish::command(&bootstrap, Operation::Publish, &d).is_err());
    let command = publish::command(&bootstrap, Operation::ObservationConstraint, &d).unwrap();
    assert_eq!(
        command
            .iter()
            .map(|s| std::str::from_utf8(s).unwrap())
            .collect::<Vec<_>>(),
        vec![
            "GRAPH.CONSTRAINT",
            "CREATE",
            bootstrap.graph_key(),
            "UNIQUE",
            "RELATIONSHIP",
            "YOKE_NATIVE_OBSERVATION",
            "PROPERTIES",
            "1",
            "key"
        ]
    );
}
#[test]
fn target_binding_rejects_remote_plaintext_paths_credentials_unknown_grants_and_wrong_ips() {
    for url in [
        "redis://example.org:6379/",
        "redis://localhost:6379/",
        "redis://user:password@127.0.0.1:6379/",
        "redis://127.0.0.1:6379/1",
        "redis://127.0.0.1:6379/?token=x",
        "redis://127.0.0.1:6379/#x",
    ] {
        let mut t = target();
        t.base_url = url.into();
        assert!(t.validate().is_err());
    }
    let mut t = target();
    t.address = "127.0.0.2:6379".parse().unwrap();
    assert!(t.validate().is_err());
    let mut bad = serde_json::to_value(target()).unwrap();
    bad["allowWrite"] = json!(true);
    assert!(serde_json::from_value::<Target>(bad).is_err());
    let mut other = target();
    other.user = "publisher".into();
    assert_eq!(other.graph_key().unwrap(), target().graph_key().unwrap());
    assert_ne!(
        other.credential_binding().unwrap(),
        target().credential_binding().unwrap()
    );
    other.namespace.push('x');
    assert_ne!(other.graph_key().unwrap(), target().graph_key().unwrap());
    let mut tls = target();
    tls.base_url = "rediss://localhost:6379/".into();
    assert!(tls.validate().is_err());
}
#[test]
fn changed_source_or_tampered_preconditions_cannot_reuse_a_plan_identity() {
    let (_dir, engine, s, g) = fixture();
    let d = Deadline::new(30000);
    let plan = Plan::prepare(
        &engine.store,
        &s,
        &g,
        target(),
        "absent",
        &Default::default(),
        &d,
    )
    .unwrap();
    let value: Value = serde_json::from_slice(&plan.encode(&d).unwrap()).unwrap();
    for (key, bad) in [
        ("expectedRevision", json!("wrong")),
        ("programHash", json!("0".repeat(64))),
        ("graphKey", json!("foreign")),
        ("hostAuthorityGranted", json!(true)),
        ("allowNetwork", json!(true)),
        ("mode", json!("bootstrap")),
    ] {
        let mut altered = value.clone();
        altered[key] = bad;
        assert!(Plan::decode(&serde_json::to_vec(&altered).unwrap(), &d).is_err());
    }
    let mut changed = g.clone();
    changed.observations[0].resolution = "observed_execution".into();
    assert!(
        Plan::prepare(
            &engine.store,
            &s,
            &changed,
            target(),
            "absent",
            &Default::default(),
            &d
        )
        .is_err()
    );
    let bootstrap = Plan::bootstrap(target(), &d).unwrap();
    let mut missing: Value = serde_json::from_slice(&bootstrap.encode(&d).unwrap()).unwrap();
    missing.as_object_mut().unwrap().remove("payload");
    assert!(Plan::decode(&serde_json::to_vec(&missing).unwrap(), &d).is_err());
}
