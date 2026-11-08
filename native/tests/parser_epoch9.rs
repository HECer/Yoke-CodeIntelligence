use yoke::{
    analysis::{self, AnalyzerEpoch},
    capture::Deadline,
    model::{SourceFile, hash},
};

fn extract(language: &str, source: &str, epoch: AnalyzerEpoch) -> yoke::model::Extraction {
    analysis::extract_at(
        epoch,
        &SourceFile {
            path: format!("sample.{language}"),
            hash: hash(source),
            size: source.len(),
            language: language.into(),
            mode: 0o644,
        },
        source,
        &Deadline::new(20000),
    )
    .unwrap()
}

#[test]
fn powershell9_matches_command_case_without_binding_dynamic_command_variables() {
    let source = "function Invoke-Work { }\ninvoke-work\n& $InvokeWork\n";
    let result = extract("powershell", source, AnalyzerEpoch::V9);
    let static_call = result
        .resolution
        .uses
        .iter()
        .find(|u| u.label == "invoke-work")
        .unwrap();
    assert_eq!(static_call.name, "invoke-work");
    assert!(
        result
            .resolution
            .bindings
            .iter()
            .any(|b| b.name == "invoke-work")
    );
    let dynamic = result
        .resolution
        .uses
        .iter()
        .find(|u| u.label == "$InvokeWork")
        .unwrap();
    assert_ne!(dynamic.name, "InvokeWork");
}

#[test]
fn verilog9_retains_task_declaration_and_task_function_scope() {
    let source =
        "module m; task worker(input int x); $display(x); endtask initial worker(1); endmodule";
    let result = extract("verilog", source, AnalyzerEpoch::V9);
    let task = result
        .entities
        .iter()
        .find(|e| e.name == "worker" && e.kind == "function");
    assert!(task.is_some(), "task declaration must survive extraction");
    let task = task.unwrap();
    assert!(
        result
            .resolution
            .scopes
            .iter()
            .any(|s| s.start == task.start_byte && s.end == task.end_byte && s.function)
    );
}

#[test]
fn commonlisp9_extracts_local_functions_without_treating_binding_lists_as_calls() {
    let source = "(defun run (x) (labels ((recur (a) (recur a))) (recur x)))";
    let result = extract("commonlisp", source, AnalyzerEpoch::V9);
    assert!(
        result
            .entities
            .iter()
            .any(|e| e.kind == "function" && e.name == "recur")
    );
    assert_eq!(
        result
            .edges
            .iter()
            .filter(|e| e.relation == "calls" && e.target_label == "recur")
            .count(),
        2
    );
    assert!(
        !result
            .edges
            .iter()
            .any(|e| e.relation == "calls" && e.target_label == "a")
    );
}

fn graph(language: &str, source: &str) -> yoke::model::Graph {
    let extracted = extract(language, source, AnalyzerEpoch::V9);
    let snapshot = yoke::model::Snapshot {
        id: hash(source),
        workspace_id: "fixture".into(),
        kind: "fixture".into(),
        files: vec![SourceFile {
            path: format!("sample.{language}"),
            hash: hash(source),
            size: source.len(),
            language: language.into(),
            mode: 0o644,
        }],
        omissions: vec![],
        capture_consistency: "fixture".into(),
    };
    analysis::assemble_at(
        AnalyzerEpoch::V9,
        &snapshot,
        vec![extracted],
        &Deadline::new(20000),
    )
    .unwrap()
}

#[test]
fn commonlisp9_function_namespace_and_flet_labels_visibility_are_distinct() {
    let source = "(defun same (x) x) (defun run (same) (same 1) (flet ((local (x) (local x))) (local 2)) (labels ((recur (x) (recur x))) (recur 3))) (defun outside () (local 4))";
    let g = graph("commonlisp", source);
    let target_name = |label: &str, occurrence: usize| {
        let o = g
            .observations
            .iter()
            .filter(|o| o.relation == "calls" && o.target_label == label)
            .nth(occurrence)
            .unwrap();
        o.target.as_ref().map(|id| {
            g.entities
                .iter()
                .find(|e| &e.id == id)
                .unwrap()
                .name
                .as_str()
        })
    };
    assert_eq!(
        target_name("same", 0),
        Some("same"),
        "value parameter must not shadow a function"
    );
    assert_eq!(
        target_name("local", 0),
        None,
        "flet function unavailable in own definition"
    );
    assert_eq!(target_name("local", 1), Some("local"));
    assert_eq!(
        target_name("local", 2),
        None,
        "local function must not escape"
    );
    assert_eq!(
        target_name("recur", 0),
        Some("recur"),
        "labels permits recursion"
    );
    assert_eq!(target_name("recur", 1), Some("recur"));
}

#[test]
fn powershell9_function_drive_reference_is_distinct_from_value_and_literal_text() {
    let source = "function F { }\n${function:F}\n$F\n\"$F\"\n'$F'\n& $F\n";
    let g = graph("powershell", source);
    let refs: Vec<_> = g
        .observations
        .iter()
        .filter(|o| o.relation == "references")
        .collect();
    let function = refs
        .iter()
        .find(|o| o.target_label == "${function:F}")
        .expect("explicit function drive reference");
    assert!(function.target.is_some());
    assert_eq!(
        refs.iter().filter(|o| o.target_label == "$F").count(),
        2,
        "ordinary value and expandable string only"
    );
    assert!(
        refs.iter()
            .filter(|o| o.target_label == "$F")
            .all(|o| o.target.is_none())
    );
    assert!(
        g.observations
            .iter()
            .filter(|o| o.relation == "calls" && o.target_label == "$F")
            .all(|o| o.target.is_none())
    );
}

#[test]
fn commonlisp9_rejects_known_impossible_arity_but_keeps_optional_and_rest_candidates() {
    let g = graph(
        "commonlisp",
        "(defun exact (x y) x) (exact 1) (exact 1 2) (defun resty (x &optional y &rest z) x) (resty) (resty 1) (resty 1 2 3)",
    );
    let targets = |name: &str| {
        g.observations
            .iter()
            .filter(|o| o.relation == "calls" && o.target_label == name)
            .map(|o| o.target.is_some())
            .collect::<Vec<_>>()
    };
    assert_eq!(targets("exact"), vec![false, true]);
    assert_eq!(targets("resty"), vec![false, true, true]);
}

#[test]
fn commonlisp9_preserves_escaped_symbol_case() {
    let g = graph("commonlisp", "(defun |Case| () 1) (|case|) (|Case|)");
    let calls: Vec<_> = g
        .observations
        .iter()
        .filter(|o| o.relation == "calls")
        .collect();
    assert_eq!(calls.len(), 2);
    assert!(calls[0].target.is_none());
    assert!(calls[1].target.is_some());
}

#[test]
fn commonlisp9_local_function_designators_use_the_function_namespace() {
    let g = graph(
        "commonlisp",
        "(defun run () (labels ((Local () 1)) #'local (function LOCAL)))",
    );
    let refs: Vec<_> = g
        .observations
        .iter()
        .filter(|o| o.relation == "references")
        .collect();
    assert_eq!(refs.len(), 2);
    assert!(refs.iter().all(|o| {
        o.target
            .as_ref()
            .is_some_and(|id| g.entities.iter().any(|e| &e.id == id && e.name == "Local"))
    }));
}
