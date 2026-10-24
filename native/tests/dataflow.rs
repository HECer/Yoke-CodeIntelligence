use std::fs;
use yoke::{
    Engine,
    capture::Deadline,
    dataflow::{self, Policy},
};
fn report(text: &str, policy: Policy) -> serde_json::Value {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("input.ts"), text).unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let deadline = Deadline::new(10_000);
    let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
    dataflow::analyze(&engine.store, &snapshot, &graph, policy, &deadline).unwrap()
}
#[test]
fn scope_shadowing_overwrite_and_branch_join_preserve_candidate_anchors() {
    let text = "function run(req:any, flag:boolean) {\n let value = req.query.q;\n { let value = 'safe'; eval(value); }\n eval(value);\n value = 'safe'; eval(value);\n if (flag) value = req.body.q; else value = 'safe';\n eval('🌍' + value);\n}";
    let result = report(text, Policy::default());
    let findings = result["findings"].as_array().unwrap();
    assert_eq!(
        findings.len(),
        2,
        "shadowed and overwritten values must not remain tainted"
    );
    let spans: Vec<_> = findings
        .iter()
        .map(|v| {
            &text
                [v["startByte"].as_u64().unwrap() as usize..v["endByte"].as_u64().unwrap() as usize]
        })
        .collect();
    assert_eq!(spans, vec!["eval(value)", "eval('🌍' + value)"]);
    assert_eq!(findings[1]["startLine"], 7);
    assert_eq!(result["exhaustive"], false);
    assert!(result["artifactId"].is_string());
}
#[test]
fn sanitizer_and_unknown_calls_are_explicit_host_assumptions() {
    let policy = Policy {
        sanitizers: vec!["clean".into()],
        ..Policy::default()
    };
    let result = report(
        "let x=req.query.q; eval(clean(x)); eval(mystery(x));",
        policy,
    );
    assert_eq!(result["findings"].as_array().unwrap().len(), 1);
    assert!(result["unknownCalls"].as_u64().unwrap() > 0);
    assert_eq!(result["findings"][0]["resolution"], "heuristic_dataflow");
}
#[test]
fn loop_fixpoint_bound_and_unsupported_constructs_are_reported() {
    let policy = Policy {
        max_iterations: 1,
        ..Policy::default()
    };
    let result = report(
        "function run(req:any){ let x=''; while (req.query.ok) { x=req.body.q; } eval(x); let {a}=req.query; eval(a); }",
        policy,
    );
    assert_eq!(result["boundedOut"], true);
    assert_eq!(result["status"], "incomplete");
    assert!(!result["unsupported"].as_array().unwrap().is_empty());
    assert!(!result["findings"].as_array().unwrap().is_empty());
}
#[test]
fn node_budget_and_foreign_graph_are_rejected_or_disclosed() {
    let policy = Policy {
        max_nodes: 1,
        ..Policy::default()
    };
    assert_eq!(
        report("let x=req.query.q; eval(x);", policy)["boundedOut"],
        true
    );
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("x.py"), "print('not analyzed')").unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let d = Deadline::new(10_000);
    let (s, mut g, _, _) = engine.index(&d).unwrap();
    let result = dataflow::analyze(&engine.store, &s, &g, Policy::default(), &d).unwrap();
    assert_eq!(result["unsupportedLanguages"][0], "python");
    g.snapshot_id = "foreign".into();
    assert!(dataflow::analyze(&engine.store, &s, &g, Policy::default(), &d).is_err());
}
#[test]
fn lexical_declaration_hoisting_and_function_parameters_do_not_leak_outer_taint() {
    let result = report(
        "let x=req.query.q; function a(x:string) { eval(x); } { eval(x); let x='safe'; } function b(){ var x='safe'; eval(x); } eval(x);",
        Policy::default(),
    );
    assert_eq!(result["findings"].as_array().unwrap().len(), 1);
}
#[test]
fn record_budget_is_visible_and_never_returns_a_complete_empty_result() {
    let p = Policy {
        max_records: 1,
        ..Policy::default()
    };
    let r = report("eval(req.query.a); eval(req.body.b);", p);
    assert_eq!(r["boundedOut"], true);
    assert_eq!(r["status"], "incomplete");
    assert_eq!(r["findings"].as_array().unwrap().len(), 1);
}
#[test]
fn var_redeclaration_without_initializer_does_not_kill_existing_flow() {
    let result = report(
        "function run(req:any) { var x=req.query.q; { var x; } eval(x); }",
        Policy::default(),
    );
    assert_eq!(result["findings"].as_array().unwrap().len(), 1);
}
