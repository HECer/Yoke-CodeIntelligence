//! Owned protocol/byte contracts; actual compiler evidence is a separate gate.
use serde_json::{Value, json};
use std::{collections::BTreeSet, fs, path::Path};
use yoke::{
    Engine,
    analysis::AnalyzerEpoch,
    capture::Deadline,
    changes, lsp,
    project_session::{HostProfile, Session},
};
const SOURCE: &str = "// owned café 🦀\r\nfunction leaf() { return 1; }\r\nfunction dependency() { return leaf(); }\r\nexport function removeMe() { return dependency(); }\r\nexport function keep() { removeMe(); return 2; }\r\n";
fn d() -> Deadline {
    Deadline::new(30_000)
}
struct Fixture {
    session: Session,
    root: tempfile::TempDir,
    log: tempfile::TempDir,
}
fn fixture(source: &str, mode: &str, epoch: AnalyzerEpoch) -> Fixture {
    let root = tempfile::tempdir().unwrap();
    let log = tempfile::tempdir().unwrap();
    fs::write(root.path().join("module.ts"), source).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    engine.capture_epoch = epoch;
    engine
        .register_configured_server(lsp::ConfiguredServer {
            server: lsp::Server {
                command: "node".into(),
                args: vec![
                    Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("tests/fixtures/propagated-delete-lsp.mjs")
                        .display()
                        .to_string(),
                    log.path().join("protocol.log").display().to_string(),
                    mode.into(),
                    root.path().join("module.ts").display().to_string(),
                ],
                language: "typescript".into(),
                trust: true,
            },
            configuration: lsp::HostConfiguration {
                passive_profile: Some(lsp::PassiveProfile::ContractFixture),
                ..Default::default()
            },
        })
        .unwrap();
    Fixture {
        session: Session::open(
            engine,
            HostProfile::default(),
            Some("owned-propagation"),
            &d(),
        )
        .unwrap(),
        root,
        log,
    }
}
fn args() -> Value {
    json!({"relative_path":"module.ts","name_path":"removeMe","propagate":true})
}
fn invoke(f: &mut Fixture, args: Value) -> anyhow::Result<Value> {
    yoke::agent_repl::invoke_facade(&mut f.session, "jb.safe_delete", args, None, &d())
}
fn transcript(f: &Fixture) -> String {
    fs::read_to_string(f.log.path().join("protocol.log")).unwrap_or_default()
}
fn unpublished(f: &Fixture) {
    assert_eq!(f.session.engine.store.db.query_row("SELECT COUNT(*) FROM records WHERE kind IN ('plan','approval','receipt','journal')",[],|r|r.get::<_,usize>(0)).unwrap(),0);
    assert_eq!(
        f.session
            .engine
            .store
            .db
            .query_row(
                "SELECT COUNT(*) FROM snapshots WHERE json_extract(value,'$.kind')='derived'",
                [],
                |r| r.get::<_, usize>(0)
            )
            .unwrap(),
        0
    );
}
fn deleted(value: &Value) -> BTreeSet<&str> {
    value["propagation"]["deletedDeclarations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["name"].as_str().unwrap())
        .collect()
}
fn after(f: &Fixture, value: &Value) -> String {
    let snapshot = f
        .session
        .engine
        .store
        .snapshot(value["plan"]["resultSnapshotId"].as_str().unwrap())
        .unwrap();
    yoke::query::source(&f.session.engine.store, &snapshot, "module.ts")
        .unwrap()
        .0
}

fn wrapped_args() -> Value {
    json!({"relativePath":"module.ts","namePath":"removeMe","propagate":true,"propagationPolicy":"captured_es_function_propagation_v2"})
}

fn binding_args() -> Value {
    json!({"relativePath":"module.ts","namePath":"removeMe","propagate":true,"propagationPolicy":"captured_es_function_propagation_v3"})
}

#[test]
fn function_binding_policy_removes_function_values_with_exact_original_entities() {
    for epoch in [AnalyzerEpoch::V8, AnalyzerEpoch::V9] {
        for keyword in ["const", "let", "var"] {
            let source = format!(
                "\u{feff}// owned café 🦀\r\n{keyword} dependency = () => 1;\r\nexport {keyword} removeMe = function() {{ return dependency(); }};\r\nexport {keyword} keep = () => {{ void (removeMe()); return 2; }};\r\n"
            );
            let expected = format!(
                "\u{feff}// owned café 🦀\r\n\r\n\r\nexport {keyword} keep = () => {{  return 2; }};\r\n"
            );
            let mut f = fixture(&source, "duplicates", epoch);
            let (snapshot, graph, _, _) = f.session.engine.index(&d()).unwrap();
            let original = serde_json::to_value(&graph).unwrap();
            let value = yoke::semantic_refactor::invoke(
                &mut f.session.engine,
                "safeDelete",
                &binding_args(),
                &d(),
            )
            .unwrap();
            assert_eq!(
                value["propagation"]["policy"],
                "captured_es_function_propagation_v3"
            );
            assert_eq!(deleted(&value), BTreeSet::from(["removeMe", "dependency"]));
            assert_eq!(value["propagation"]["removedUsageCount"], 1);
            assert_eq!(after(&f, &value), expected);
            assert_eq!(
                serde_json::to_value(f.session.engine.store.graph(&snapshot.id).unwrap()).unwrap(),
                original
            );
            for declaration in value["propagation"]["deletedDeclarations"]
                .as_array()
                .unwrap()
            {
                let entity = graph
                    .entities
                    .iter()
                    .find(|entity| entity.id == declaration["entityId"].as_str().unwrap())
                    .unwrap();
                assert_eq!(declaration["entityStartByte"], entity.start_byte);
                assert_eq!(declaration["entityEndByte"], entity.end_byte);
                assert_eq!(entity.kind, "function");
                assert!(declaration["startByte"].as_u64().unwrap() < entity.start_byte as u64);
            }
            assert_eq!(value["complete"], false);
            assert_eq!(
                fs::read(f.root.path().join("module.ts")).unwrap(),
                source.as_bytes()
            );
        }
    }
}

#[test]
fn function_binding_policy_is_explicit_and_older_policies_keep_their_module_boundary() {
    let source = "const dependency = () => 1;\nexport const removeMe = () => dependency();\nexport function keep() { removeMe(); return 2; }\n";
    for policy in [
        None,
        Some("captured_es_function_propagation_v1"),
        Some("captured_es_function_propagation_v2"),
    ] {
        let mut f = fixture(source, "ok", AnalyzerEpoch::V9);
        let mut request = binding_args();
        if let Some(policy) = policy {
            request["propagationPolicy"] = json!(policy);
        } else {
            request.as_object_mut().unwrap().remove("propagationPolicy");
        }
        assert!(
            yoke::semantic_refactor::invoke(&mut f.session.engine, "safeDelete", &request, &d())
                .is_err()
        );
        unpublished(&f);
        assert!(transcript(&f).is_empty());
    }
    let mut f = fixture(SOURCE, "ok", AnalyzerEpoch::V9);
    let value =
        yoke::semantic_refactor::invoke(&mut f.session.engine, "safeDelete", &binding_args(), &d())
            .unwrap();
    assert_eq!(
        deleted(&value),
        BTreeSet::from(["leaf", "dependency", "removeMe"])
    );
    assert_eq!(
        after(&f, &value),
        "// owned café 🦀\r\n\r\n\r\n\r\nexport function keep() {  return 2; }\r\n"
    );
}

#[test]
fn function_binding_policy_refuses_multiple_pattern_ambient_and_non_function_bindings() {
    for declaration in [
        "const removeMe = () => 1, sibling = () => 2;",
        "let removeMe = function named() {};",
        "var removeMe = 1;",
        "const [removeMe] = [() => 1];",
        "const removeMe = (() => 1);",
        "declare const removeMe: () => number;",
        "using removeMe = () => 1;",
        "const removeMe = factory();",
        "const /* unclassified attachment */ removeMe = () => 1;",
    ] {
        let source = format!("{declaration}\nexport function keep() {{ return 2; }}\n");
        let mut f = fixture(&source, "ok", AnalyzerEpoch::V9);
        assert!(
            yoke::semantic_refactor::invoke(
                &mut f.session.engine,
                "safeDelete",
                &binding_args(),
                &d()
            )
            .is_err(),
            "{declaration}"
        );
        unpublished(&f);
        assert!(transcript(&f).is_empty());
    }
}

#[test]
fn function_binding_policy_cannot_force_away_surviving_value_or_assignment_consumers() {
    for usage in [
        "return removeMe();",
        "const value = removeMe; return value;",
        "other(removeMe()); return 2;",
        "new removeMe(); return 2;",
        "removeMe = () => 3; return 2;",
    ] {
        let source = format!(
            "export let removeMe = function() {{ return 1; }};\nexport function keep() {{ {usage} }}\n"
        );
        let mut f = fixture(&source, "ok", AnalyzerEpoch::V9);
        let mut request = binding_args();
        request["deleteEvenIfUsed"] = json!(true);
        assert!(
            yoke::semantic_refactor::invoke(&mut f.session.engine, "safeDelete", &request, &d())
                .is_err(),
            "{usage}"
        );
        unpublished(&f);
        assert!(!transcript(&f).is_empty());
    }
}

#[test]
fn function_binding_cycles_follow_removed_regions_and_keep_surviving_or_exported_dependencies() {
    for (export, surviving) in [(false, false), (false, true), (true, false)] {
        let source = format!(
            "{}const alpha = () => beta();\nconst beta = function() {{ return alpha(); }};\nexport const removeMe = () => alpha();\nexport function keep() {{ void removeMe(); {}return 2; }}\n",
            if export { "export " } else { "" },
            if surviving { "alpha(); " } else { "" }
        );
        let mut f = fixture(&source, "ok", AnalyzerEpoch::V9);
        let value = yoke::semantic_refactor::invoke(
            &mut f.session.engine,
            "safeDelete",
            &binding_args(),
            &d(),
        )
        .unwrap();
        let expected = if export || surviving {
            BTreeSet::from(["removeMe"])
        } else {
            BTreeSet::from(["alpha", "beta", "removeMe"])
        };
        assert_eq!(deleted(&value), expected);
        assert_eq!(value["complete"], false);
        assert_eq!(
            fs::read(f.root.path().join("module.ts")).unwrap(),
            source.as_bytes()
        );
    }
}

#[test]
fn function_binding_argument_dependencies_are_pruned_without_sweeping_unrelated_values() {
    let source = "const unrelated = () => 0;\nconst argument = () => 1;\nexport const removeMe = (value = 0) => value;\nexport function keep() { void removeMe(argument()); return 2; }\n";
    let mut f = fixture(source, "ok", AnalyzerEpoch::V9);
    let value =
        yoke::semantic_refactor::invoke(&mut f.session.engine, "safeDelete", &binding_args(), &d())
            .unwrap();
    assert_eq!(deleted(&value), BTreeSet::from(["argument", "removeMe"]));
    assert_eq!(
        after(&f, &value),
        "const unrelated = () => 0;\n\n\nexport function keep() {  return 2; }\n"
    );
}

#[test]
fn function_binding_policy_retains_exact_provider_and_original_source_requirements() {
    let source = "export const removeMe = () => 1;\nexport const keep = () => { void removeMe(); return 2; };\n";
    for mode in [
        "no-definition",
        "foreign-definition",
        "wrong-definition",
        "empty-references",
        "omit-self",
        "omit-use",
        "truncated",
        "malformed-references",
        "tamper-original",
    ] {
        let mut f = fixture(source, mode, AnalyzerEpoch::V9);
        assert!(
            yoke::semantic_refactor::invoke(
                &mut f.session.engine,
                "safeDelete",
                &binding_args(),
                &d()
            )
            .is_err(),
            "{mode}"
        );
        assert_eq!(f.session.engine.store.db.query_row("SELECT COUNT(*) FROM records WHERE kind IN ('plan','approval','receipt','journal')",[],|r|r.get::<_,usize>(0)).unwrap(),0);
        let expected = if mode == "tamper-original" {
            "// foreign edit\n"
        } else {
            source
        };
        assert_eq!(
            fs::read_to_string(f.root.path().join("module.ts")).unwrap(),
            expected
        );
    }
}

#[test]
fn function_binding_declaration_quota_admits_sixteen_and_refuses_seventeen_before_provider_start() {
    for declarations in [16, 17] {
        let mut source = (0..declarations - 1)
            .map(|i| format!("const private{i} = () => {i};\n"))
            .collect::<String>();
        source.push_str("export const removeMe = () => 1;\n");
        let mut f = fixture(&source, "ok", AnalyzerEpoch::V9);
        let result = yoke::semantic_refactor::invoke(
            &mut f.session.engine,
            "safeDelete",
            &binding_args(),
            &d(),
        );
        if declarations == 16 {
            let value = result.unwrap();
            assert_eq!(value["propagation"]["providerRequestCount"], 32);
            assert_eq!(deleted(&value), BTreeSet::from(["removeMe"]));
        } else {
            assert!(format!("{:#}", result.unwrap_err()).contains("propagated_delete_capacity"));
            unpublished(&f);
            assert!(transcript(&f).is_empty());
        }
    }
}

#[test]
fn function_binding_usage_quota_admits_128_and_refuses_129_without_clipping() {
    for count in [128, 129] {
        let source = format!(
            "export const removeMe = () => 1;\nexport const keep = () => {{ {}return 2; }};\n",
            "void removeMe();".repeat(count)
        );
        let mut f = fixture(&source, "ok", AnalyzerEpoch::V9);
        let result = yoke::semantic_refactor::invoke(
            &mut f.session.engine,
            "safeDelete",
            &binding_args(),
            &d(),
        );
        if count == 128 {
            assert_eq!(result.unwrap()["propagation"]["removedUsageCount"], 128);
        } else {
            assert!(format!("{:#}", result.unwrap_err()).contains("propagated_delete_capacity"));
            unpublished(&f);
        }
    }
}

#[test]
fn function_binding_removal_cannot_merge_surviving_statement_syntax() {
    for body in [
        "export function keep() { first()\nvoid removeMe();\n(second)(); return 2; }\n",
        "first()\nexport const removeMe = () => 1;\n(second)();\nexport function keep() { return 2; }\n",
    ] {
        let target = if body.contains("export const removeMe") {
            ""
        } else {
            "export const removeMe = () => 1;\n"
        };
        let source = format!("const first = () => first;\nconst second = () => 2;\n{target}{body}");
        let mut f = fixture(&source, "ok", AnalyzerEpoch::V9);
        let result = yoke::semantic_refactor::invoke(
            &mut f.session.engine,
            "safeDelete",
            &binding_args(),
            &d(),
        );
        assert!(format!("{:#}", result.unwrap_err()).contains("alter surviving syntax"));
        unpublished(&f);
    }
}

#[test]
fn wrapped_policy_removes_only_complete_parenthesized_await_and_void_statements() {
    for epoch in [AnalyzerEpoch::V8, AnalyzerEpoch::V9] {
        let source = "\u{feff}// owned café 🦀\r\nfunction dependency() { return 1; }\r\nexport function removeMe() { return dependency(); }\r\nexport async function keep() { /* before */ ((removeMe())); /* between */ await (removeMe()); void (await removeMe()); return 2; }\r\n";
        let mut f = fixture(source, "duplicates", epoch);
        let value = yoke::semantic_refactor::invoke(
            &mut f.session.engine,
            "safeDelete",
            &wrapped_args(),
            &d(),
        )
        .unwrap();
        assert_eq!(
            value["propagation"]["policy"],
            "captured_es_function_propagation_v2"
        );
        assert_eq!(value["propagation"]["removedUsageCount"], 3);
        assert_eq!(deleted(&value), BTreeSet::from(["removeMe", "dependency"]));
        assert_eq!(
            after(&f, &value),
            "\u{feff}// owned café 🦀\r\n\r\n\r\nexport async function keep() { /* before */  /* between */   return 2; }\r\n"
        );
        for usage in value["propagation"]["removedUsages"].as_array().unwrap() {
            let start = usage["startByte"].as_u64().unwrap() as usize;
            let end = usage["endByte"].as_u64().unwrap() as usize;
            assert!(
                [
                    "((removeMe()));",
                    "await (removeMe());",
                    "void (await removeMe());"
                ]
                .contains(&&source[start..end])
            );
        }
        assert_eq!(value["complete"], false);
        assert_eq!(value["remainingUsagesMayBreak"], true);
        assert_eq!(
            fs::read(f.root.path().join("module.ts")).unwrap(),
            source.as_bytes()
        );
    }
}

#[test]
fn wrapped_policy_is_opt_in_and_explicit_v1_keeps_the_original_effects() {
    let mut f = fixture(SOURCE, "ok", AnalyzerEpoch::V9);
    let original = invoke(&mut f, args()).unwrap();
    let explicit = yoke::semantic_refactor::invoke(&mut f.session.engine, "safeDelete",
        &json!({"relativePath":"module.ts","namePath":"removeMe","propagate":true,"propagationPolicy":"captured_es_function_propagation_v1"}), &d()).unwrap();
    for key in [
        "baseSnapshotId",
        "resultSnapshotId",
        "workspaceId",
        "files",
        "diff",
        "warnings",
    ] {
        assert_eq!(original["plan"][key], explicit["plan"][key], "{key}");
    }
    assert_eq!(
        original["plan"]
            .as_object()
            .unwrap()
            .keys()
            .collect::<Vec<_>>(),
        explicit["plan"]
            .as_object()
            .unwrap()
            .keys()
            .collect::<Vec<_>>()
    );
    assert_eq!(original["propagation"], explicit["propagation"]);
    for usage in ["((removeMe()));", "await removeMe();", "void removeMe();"] {
        let source = format!(
            "export function removeMe() {{ return 1; }}\nexport async function keep() {{ {usage} return 2; }}\n"
        );
        let mut f = fixture(&source, "ok", AnalyzerEpoch::V9);
        assert!(invoke(&mut f, args()).is_err(), "{usage}");
        unpublished(&f);
    }
}

#[test]
fn wrapped_policy_refuses_value_consumers_optional_calls_and_unowned_statement_slots() {
    for usage in [
        "return (await removeMe());",
        "const held = void removeMe();",
        "other(await removeMe());",
        "new removeMe();",
        "!removeMe();",
        "typeof removeMe();",
        "(removeMe(), other());",
        "removeMe?.();",
        "(removeMe)();",
        "removeMe<number>();",
        "removeMe()!;",
        "removeMe() as number;",
        "if (true) void removeMe();",
        "while (false) removeMe();",
        "for (;;) { break; } label: removeMe();",
        "do removeMe(); while (false);",
        "switch (1) { case 1: removeMe(); }",
        "void (/* unclassified attachment */ removeMe());",
    ] {
        let source = format!(
            "export function removeMe() {{ return 1; }}\nexport async function keep() {{ {usage} return 2; }}\n"
        );
        let mut f = fixture(&source, "ok", AnalyzerEpoch::V9);
        assert!(
            yoke::semantic_refactor::invoke(
                &mut f.session.engine,
                "safeDelete",
                &wrapped_args(),
                &d()
            )
            .is_err(),
            "{usage}"
        );
        unpublished(&f);
        assert_eq!(
            fs::read(f.root.path().join("module.ts")).unwrap(),
            source.as_bytes()
        );
    }
}

#[test]
fn wrapped_policy_rejects_invalid_native_selectors_before_starting_a_provider() {
    for policy in [
        Value::Null,
        json!(false),
        json!(7),
        json!({}),
        json!([]),
        json!("captured_es_function_propagation_v4"),
    ] {
        let mut f = fixture(SOURCE, "ok", AnalyzerEpoch::V9);
        let mut request = wrapped_args();
        request["propagationPolicy"] = policy;
        assert!(
            yoke::semantic_refactor::invoke(&mut f.session.engine, "safeDelete", &request, &d())
                .is_err()
        );
        unpublished(&f);
        assert!(transcript(&f).is_empty());
    }
    for field in ["propagate", "namePath"] {
        for value in [Value::Null, json!(false), json!("")] {
            let mut f = fixture(SOURCE, "ok", AnalyzerEpoch::V9);
            let mut request = wrapped_args();
            request[field] = value;
            assert!(
                yoke::semantic_refactor::invoke(
                    &mut f.session.engine,
                    "safeDelete",
                    &request,
                    &d()
                )
                .is_err()
            );
            unpublished(&f);
            assert!(transcript(&f).is_empty());
        }
    }
    let mut f = fixture(SOURCE, "ok", AnalyzerEpoch::V9);
    let mut request = args();
    request["propagation_policy"] = json!("captured_es_function_propagation_v2");
    assert!(invoke(&mut f, request).is_err());
    unpublished(&f);
    assert!(transcript(&f).is_empty());
}

#[test]
fn wrapped_policy_requires_the_same_complete_bound_provider_proofs() {
    let source = "export function removeMe() { return 1; }\nexport function keep() { void removeMe(); return 2; }\n";
    for mode in [
        "no-definition",
        "foreign-definition",
        "wrong-definition",
        "empty-references",
        "omit-self",
        "omit-use",
        "truncated",
        "malformed-references",
        "tamper-original",
    ] {
        let mut f = fixture(source, mode, AnalyzerEpoch::V9);
        assert!(
            yoke::semantic_refactor::invoke(
                &mut f.session.engine,
                "safeDelete",
                &wrapped_args(),
                &d()
            )
            .is_err(),
            "{mode}"
        );
        unpublished(&f);
        if mode == "tamper-original" {
            assert_eq!(
                fs::read_to_string(f.root.path().join("module.ts")).unwrap(),
                "// foreign edit\n"
            );
        }
    }
}

#[test]
fn wrapped_policy_bounds_wrapper_depth_without_clipping_the_whole_usage() {
    for depth in [32, 33] {
        let source = format!(
            "export function removeMe() {{ return 1; }}\nexport function keep() {{ {}removeMe(){}; return 2; }}\n",
            "(".repeat(depth),
            ")".repeat(depth)
        );
        let mut f = fixture(&source, "ok", AnalyzerEpoch::V9);
        let result = yoke::semantic_refactor::invoke(
            &mut f.session.engine,
            "safeDelete",
            &wrapped_args(),
            &d(),
        );
        if depth == 32 {
            let value = result.unwrap();
            assert_eq!(value["propagation"]["removedUsageCount"], 1);
            assert_eq!(
                after(&f, &value),
                "\nexport function keep() {  return 2; }\n"
            );
        } else {
            assert!(result.is_err());
            unpublished(&f);
        }
    }
}

#[test]
fn wrapped_policy_deduplicates_nested_calls_and_keeps_surviving_dependencies() {
    let source = "function leaf() { return 1; }\nfunction retained() { return 2; }\nexport function removeMe(value = 0) { return value + leaf(); }\nexport async function keep() { void (await removeMe(removeMe(retained()))); return retained(); }\n";
    let mut f = fixture(source, "duplicates", AnalyzerEpoch::V9);
    let value =
        yoke::semantic_refactor::invoke(&mut f.session.engine, "safeDelete", &wrapped_args(), &d())
            .unwrap();
    assert_eq!(value["propagation"]["removedUsageCount"], 1);
    assert_eq!(deleted(&value), BTreeSet::from(["removeMe", "leaf"]));
    assert_eq!(
        after(&f, &value),
        "\nfunction retained() { return 2; }\n\nexport async function keep() {  return retained(); }\n"
    );
}

#[test]
fn facade_propagation_preserves_both_epochs_force_flags_and_original_byte_anchors() {
    for epoch in [AnalyzerEpoch::V8, AnalyzerEpoch::V9] {
        for force in [false, true] {
            let mut f = fixture(SOURCE, "ok", epoch);
            let mut request = args();
            request["delete_even_if_used"] = json!(force);
            let value = invoke(&mut f, request).unwrap();
            assert_eq!(
                deleted(&value),
                BTreeSet::from(["leaf", "dependency", "removeMe"])
            );
            assert_eq!(value["forced"], force);
            assert_eq!(value["complete"], false);
            assert_eq!(value["remainingUsagesMayBreak"], true);
            assert_eq!(value["propagation"]["providerRequestCount"], 8);
            assert_eq!(value["propagation"]["removedUsageCount"], 1);
            assert_eq!(
                after(&f, &value),
                "// owned café 🦀\r\n\r\n\r\n\r\nexport function keep() {  return 2; }\r\n"
            );
            assert_eq!(
                fs::read_to_string(f.root.path().join("module.ts")).unwrap(),
                SOURCE
            );
            assert!(transcript(&f).contains("\"includeDeclaration\":true"));
        }
    }
}

#[test]
fn private_cycles_are_pruned_together_and_surviving_consumers_keep_the_cycle() {
    for surviving in [false, true] {
        let source = format!(
            "function alpha() {{ return beta(); }}\nfunction beta() {{ return alpha(); }}\nexport function removeMe() {{ return alpha(); }}\nexport function keep() {{ removeMe(); {}return 2; }}\n",
            if surviving { "beta(); " } else { "" }
        );
        let mut f = fixture(&source, "ok", AnalyzerEpoch::V9);
        let value = invoke(&mut f, args()).unwrap();
        assert_eq!(
            deleted(&value),
            if surviving {
                BTreeSet::from(["removeMe"])
            } else {
                BTreeSet::from(["alpha", "beta", "removeMe"])
            }
        );
        assert_eq!(value["propagation"]["removedUsageCount"], 1);
    }
}

#[test]
fn reported_non_reference_tokens_cannot_seed_deletion_of_an_unrelated_unused_function() {
    for body in [
        "/* unrelated */ return 1;",
        "return 'unrelated';",
        "const unrelated = () => 1; return unrelated();",
        "const local = unrelated => unrelated(); return 1;",
    ] {
        // The owned fixture intentionally reports raw matching tokens, including
        // these non-reference positions. Such proof must be refused, not used
        // to classify an unrelated declaration as newly unused.
        let source = format!(
            "function unrelated() {{ return 7; }}\nexport function removeMe() {{ {body} }}\nexport function keep() {{ removeMe(); return 2; }}\n"
        );
        let mut f = fixture(&source, "raw-token-references", AnalyzerEpoch::V9);
        let result = invoke(&mut f, args());
        assert!(
            result.is_err(),
            "invalid provider token admitted: {result:?}"
        );
        unpublished(&f);
    }
}

#[test]
fn bom_and_utf16_provider_positions_preserve_multibyte_bytes_before_the_removed_call() {
    let source = format!(
        "\u{feff}{}",
        SOURCE.replace(
            "keep() { removeMe();",
            "keep() { const marker = '🦀'; removeMe();"
        )
    );
    let mut f = fixture(&source, "ok", AnalyzerEpoch::V9);
    let value = invoke(&mut f, args()).unwrap();
    let result = after(&f, &value);
    assert!(result.starts_with('\u{feff}'));
    assert!(result.contains("const marker = '🦀';  return 2;"));
    let mut f = fixture(
        &format!("{SOURCE}function escaped\\u004eame() {{ return 1; }}\n"),
        "ok",
        AnalyzerEpoch::V9,
    );
    assert!(invoke(&mut f, args()).is_err());
    unpublished(&f);
    assert!(transcript(&f).is_empty());
}

#[test]
fn stalled_provider_cannot_publish_after_the_original_deadline() {
    let mut f = fixture(SOURCE, "stall", AnalyzerEpoch::V9);
    assert!(
        yoke::agent_repl::invoke_facade(
            &mut f.session,
            "jb.safe_delete",
            args(),
            None,
            &Deadline::new(750)
        )
        .is_err()
    );
    unpublished(&f);
    assert_eq!(
        fs::read_to_string(f.root.path().join("module.ts")).unwrap(),
        SOURCE
    );
}

#[test]
fn argument_dependencies_become_unused_without_sweeping_unrelated_unused_functions() {
    let source = "function argumentOnly() { return 1; }\nfunction unrelated() { return 3; }\nexport function removeMe(value: number) { return value; }\nexport function keep() { removeMe(argumentOnly()); return 2; }\n";
    let mut f = fixture(source, "ok", AnalyzerEpoch::V9);
    let value = invoke(&mut f, args()).unwrap();
    assert_eq!(
        deleted(&value),
        BTreeSet::from(["argumentOnly", "removeMe"])
    );
    assert!(after(&f, &value).contains("function unrelated()"));
}

#[test]
fn exported_and_captured_external_dependencies_keep_their_own_dependencies() {
    for case in ["export", "external-literal", "surviving-reference"] {
        let source = match case {
            "export" => SOURCE.replace("function dependency()", "export function dependency()"),
            "surviving-reference" => {
                format!("{SOURCE}export function anotherConsumer() {{ return dependency(); }}\n")
            }
            _ => SOURCE.into(),
        };
        let mut f = fixture(&source, "ok", AnalyzerEpoch::V9);
        if case == "external-literal" {
            fs::write(f.root.path().join("notes.txt"), "dependency").unwrap();
        }
        let value = invoke(&mut f, args()).unwrap();
        assert_eq!(deleted(&value), BTreeSet::from(["removeMe"]));
        assert_eq!(
            value["propagation"]["retainedCandidates"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
    }
}

#[test]
fn duplicate_provider_observations_and_nested_target_calls_emit_one_physical_statement_edit() {
    for mode in ["ok", "duplicates"] {
        let source = "export function removeMe(...values: unknown[]) { return 1; }\nexport function keep() { removeMe(removeMe()); return 2; }\n";
        let mut f = fixture(source, mode, AnalyzerEpoch::V9);
        let value = invoke(&mut f, args()).unwrap();
        assert_eq!(value["propagation"]["removedUsageCount"], 1);
        assert_eq!(
            after(&f, &value),
            "\nexport function keep() {  return 2; }\n"
        );
    }
}

#[test]
fn value_return_constructor_and_other_expression_consumers_refuse_without_a_plan() {
    for usage in [
        "return removeMe();",
        "const held = removeMe; return held;",
        "other(removeMe()); return 2;",
        "new removeMe(); return 2;",
        "void removeMe(); return 2;",
    ] {
        let source = format!(
            "export function removeMe() {{ return 1; }}\nexport function keep() {{ {usage} }}\n"
        );
        let mut f = fixture(&source, "ok", AnalyzerEpoch::V9);
        assert!(invoke(&mut f, args()).is_err(), "{usage}");
        unpublished(&f);
    }
}

#[test]
fn missing_empty_omitted_truncated_foreign_and_malformed_proofs_do_not_publish() {
    for mode in [
        "no-definition",
        "foreign-definition",
        "wrong-definition",
        "empty-references",
        "omit-self",
        "omit-use",
        "truncated",
        "proof-overflow",
        "malformed-references",
        "other-file-reference",
    ] {
        let mut f = fixture(SOURCE, mode, AnalyzerEpoch::V9);
        if mode == "other-file-reference" {
            fs::write(f.root.path().join("other.ts"), "removeMe();\n").unwrap();
        }
        assert!(invoke(&mut f, args()).is_err(), "{mode}");
        unpublished(&f);
        assert_eq!(
            fs::read_to_string(f.root.path().join("module.ts")).unwrap(),
            SOURCE
        );
    }
}

#[test]
fn unreported_identifier_and_literal_target_candidates_cannot_be_disguised_as_no_uses() {
    for (suffix, mode) in [
        (
            "export function other() { const held = removeMe; return held; }\n",
            "omit-use",
        ),
        ("export function other() { return 'removeMe'; }\n", "ok"),
    ] {
        let mut f = fixture(&format!("{SOURCE}{suffix}"), mode, AnalyzerEpoch::V9);
        assert!(invoke(&mut f, args()).is_err());
        unpublished(&f);
    }
    let mut f = fixture(SOURCE, "ok", AnalyzerEpoch::V9);
    fs::write(f.root.path().join("notes.txt"), "removeMe").unwrap();
    assert!(invoke(&mut f, args()).is_err());
    unpublished(&f);
}

#[test]
fn source_changes_during_provider_work_are_preserved_and_refused_before_publication() {
    let mut f = fixture(SOURCE, "tamper-original", AnalyzerEpoch::V9);
    assert!(invoke(&mut f, args()).is_err());
    unpublished(&f);
    assert_eq!(
        fs::read_to_string(f.root.path().join("module.ts")).unwrap(),
        "// foreign edit\n"
    );
}

#[test]
fn missing_edit_or_provider_authority_and_invalid_selectors_refuse_before_provider_start() {
    for case in [
        "planning",
        "readonly",
        "missing",
        "untrusted",
        "no-profile",
        "traversal",
        "case-alias",
        "empty",
        "file-selection",
    ] {
        let mut f = fixture(SOURCE, "ok", AnalyzerEpoch::V9);
        let mut request = args();
        match case {
            "planning" => {
                f.session
                    .invoke("switchModes", &json!({"modes":["planning"]}), &d())
                    .unwrap();
            }
            "readonly" => {
                f.session.engine.capture_options.read_only_dependency_dirs = vec![".".into()]
            }
            "missing" => f.session.engine.servers.clear(),
            "untrusted" => {
                f.session
                    .engine
                    .servers
                    .get_mut("typescript")
                    .unwrap()
                    .trust = false
            }
            "no-profile" => f.session.engine.server_configurations.clear(),
            "traversal" => request["relative_path"] = json!("../module.ts"),
            "case-alias" => request["relative_path"] = json!("MODULE.ts"),
            "empty" => request["name_path"] = json!(""),
            _ => request["name_path"] = Value::Null,
        }
        assert!(invoke(&mut f, request).is_err(), "{case}");
        unpublished(&f);
        assert!(transcript(&f).is_empty(), "{case}");
    }
}

#[test]
fn unsupported_module_shapes_refuse_before_provider_start() {
    for suffix in [
        "export const variable = 1;\n",
        "import value from './outside';\n",
        "export class Other {}\n",
        "export interface Other {}\n",
        "export default function() {}\n",
        "/* malformed",
    ] {
        let mut f = fixture(&format!("{SOURCE}{suffix}"), "ok", AnalyzerEpoch::V9);
        assert!(invoke(&mut f, args()).is_err(), "{suffix}");
        unpublished(&f);
        assert!(transcript(&f).is_empty());
    }
}

#[test]
fn declaration_quota_admits_sixteen_and_refuses_seventeen_before_provider_start() {
    for count in [16, 17] {
        let source = format!(
            "{}export function removeMe() {{ return 1; }}\nexport function keep() {{ removeMe(); return 2; }}\n",
            (0..count - 2)
                .map(|i| format!("function filler{i}() {{ return {i}; }}\n"))
                .collect::<String>()
        );
        let mut f = fixture(&source, "ok", AnalyzerEpoch::V9);
        let value = invoke(&mut f, args());
        if count == 16 {
            assert_eq!(value.unwrap()["propagation"]["providerRequestCount"], 32);
        } else {
            assert!(value.is_err());
            unpublished(&f);
            assert!(transcript(&f).is_empty());
        }
    }
}

#[test]
fn usage_statement_quota_admits128_and_refuses129_without_publication() {
    for count in [128, 129] {
        let source = format!(
            "export function removeMe() {{ return 1; }}\nexport function keep() {{ {}return 2; }}\n",
            "removeMe();".repeat(count)
        );
        let mut f = fixture(&source, "ok", AnalyzerEpoch::V9);
        let value = invoke(&mut f, args());
        if count == 128 {
            assert_eq!(value.unwrap()["propagation"]["removedUsageCount"], 128);
        } else {
            assert!(value.is_err());
            unpublished(&f);
        }
    }
}

#[test]
fn selected_module_byte_quota_is_exact_and_checked_before_provider_start() {
    for size in [256_000, 256_001] {
        let source = format!("//{}\n{SOURCE}", "x".repeat(size - SOURCE.len() - 3));
        assert_eq!(source.len(), size);
        let mut f = fixture(&source, "no-definition", AnalyzerEpoch::V9);
        let error = invoke(&mut f, args()).unwrap_err();
        unpublished(&f);
        assert_eq!(
            transcript(&f).is_empty(),
            size == 256_001,
            "{size}: {error:#}"
        );
    }
}

#[test]
fn captured_file_quota_admits2000_and_refuses2001_before_provider_start() {
    for count in [2000, 2001] {
        let mut f = fixture(SOURCE, "no-definition", AnalyzerEpoch::V9);
        for i in 1..count {
            fs::write(f.root.path().join(format!("entry_{i:04}.txt")), b"").unwrap();
        }
        let error = invoke(&mut f, args()).unwrap_err();
        unpublished(&f);
        assert_eq!(
            transcript(&f).is_empty(),
            count == 2001,
            "{count}: {error:#}"
        );
    }
}

#[test]
fn captured_byte_quota_counts_physical_duplicate_cas_members() {
    for size in [16_000_000usize, 16_000_001] {
        let mut f = fixture(SOURCE, "no-definition", AnalyzerEpoch::V9);
        let mut left = size - SOURCE.len();
        for i in 0..4 {
            let bytes = left.min(4_000_000);
            fs::write(
                f.root.path().join(format!("payload_{i}.txt")),
                vec![b'x'; bytes],
            )
            .unwrap();
            left -= bytes;
        }
        assert_eq!(left, 0);
        let error = invoke(&mut f, args()).unwrap_err();
        unpublished(&f);
        assert_eq!(
            transcript(&f).is_empty(),
            size == 16_000_001,
            "{size}: {error:#}"
        );
    }
}

#[test]
fn actual_owned_byte_verification_permission_approval_stale_and_recovery_gates_remain() {
    let mut f = fixture(SOURCE, "ok", AnalyzerEpoch::V9);
    let value = invoke(&mut f, args()).unwrap();
    let id = value["plan"]["id"].as_str().unwrap();
    let expected = after(&f, &value);
    let apply = json!({"planId":id,"idempotencyKey":"owned-propagated-delete","mode":"workspace"});
    assert!(changes::apply(&f.session.engine.store, f.root.path(), &apply, true, &d()).is_err());
    changes::approve(&f.session.engine.store, id, false).unwrap();
    assert!(changes::apply(&f.session.engine.store, f.root.path(), &apply, true, &d()).is_err());
    let command = format!(
        "const f=require('node:fs');if(f.readFileSync('module.ts','utf8')!=={})process.exit(7)",
        serde_json::to_string(&expected).unwrap()
    );
    let verification = yoke::verify::execute(
        &f.session.engine,
        id,
        "node",
        &["-e".into(), command],
        true,
        &d(),
    )
    .unwrap();
    assert_eq!(verification["status"], "passed");
    assert!(changes::apply(&f.session.engine.store, f.root.path(), &apply, false, &d()).is_err());
    fs::write(f.root.path().join("module.ts"), "// foreign edit\n").unwrap();
    assert!(changes::apply(&f.session.engine.store, f.root.path(), &apply, true, &d()).is_err());
    assert_eq!(
        fs::read_to_string(f.root.path().join("module.ts")).unwrap(),
        "// foreign edit\n"
    );
    fs::write(f.root.path().join("module.ts"), SOURCE).unwrap();
    let receipt =
        changes::apply(&f.session.engine.store, f.root.path(), &apply, true, &d()).unwrap();
    assert_eq!(
        fs::read_to_string(f.root.path().join("module.ts")).unwrap(),
        expected
    );
    changes::recover(&f.session.engine.store, f.root.path(), &receipt.id, true).unwrap();
    assert_eq!(
        fs::read_to_string(f.root.path().join("module.ts")).unwrap(),
        SOURCE
    );
}

#[test]
fn wrapped_policy_preserves_surviving_statement_boundaries() {
    for body in [
        "export function keep() { first()\nvoid removeMe();\n(second)(); return 2; }\n",
        "first()\nexport function removeMe() { return 1; }\n(second)();\nexport function keep() { return 2; }\n",
    ] {
        let declaration = if body.contains("export function removeMe") {
            ""
        } else {
            "export function removeMe() { return 1; }\n"
        };
        let source = format!(
            "function first() {{ return first; }}\nfunction second() {{ return 2; }}\n{declaration}{body}"
        );
        let mut f = fixture(&source, "ok", AnalyzerEpoch::V9);
        let result = yoke::semantic_refactor::invoke(
            &mut f.session.engine,
            "safeDelete",
            &wrapped_args(),
            &d(),
        );
        assert!(
            result.is_err(),
            "Unrelated expressions must not merge after deleting source: {body}"
        );
        assert!(format!("{:#}", result.unwrap_err()).contains("alter surviving syntax"));
        unpublished(&f);
        assert_eq!(
            fs::read(f.root.path().join("module.ts")).unwrap(),
            source.as_bytes()
        );
    }
    let source = "function first() { return first; }\nfunction second() { return 2; }\nexport function removeMe() { return 1; }\nexport function keep() { first();\nvoid removeMe();\n(second)(); return 2; }\n";
    let mut f = fixture(source, "ok", AnalyzerEpoch::V9);
    let value =
        yoke::semantic_refactor::invoke(&mut f.session.engine, "safeDelete", &wrapped_args(), &d())
            .unwrap();
    assert!(after(&f, &value).contains("first();\n\n(second)();"));
    let source = "export function removeMe() {}";
    let mut f = fixture(source, "ok", AnalyzerEpoch::V9);
    let value =
        yoke::semantic_refactor::invoke(&mut f.session.engine, "safeDelete", &wrapped_args(), &d())
            .unwrap();
    assert_eq!(after(&f, &value), "");
}

#[test]
fn wrapped_policy_bounds_combined_surviving_ast_work_before_publishing() {
    for statements in [20_000, 40_000] {
        let source = format!(
            "export function removeMe() {{}}\nexport function keep() {{ void removeMe(); {}return 2; }}\n",
            "0;".repeat(statements)
        );
        let mut f = fixture(&source, "ok", AnalyzerEpoch::V9);
        let result = yoke::semantic_refactor::invoke(
            &mut f.session.engine,
            "safeDelete",
            &wrapped_args(),
            &d(),
        );
        if statements == 20_000 {
            assert_eq!(result.unwrap()["propagation"]["removedUsageCount"], 1);
        } else {
            assert!(format!("{:#}", result.unwrap_err()).contains("propagated_delete_capacity"));
            unpublished(&f);
        }
        assert_eq!(
            fs::read(f.root.path().join("module.ts")).unwrap(),
            source.as_bytes()
        );
    }
}
