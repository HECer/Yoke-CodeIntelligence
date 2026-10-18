use yoke::{
    analysis,
    capture::Deadline,
    model::{Extraction, Graph, Snapshot, SourceFile, hash},
};

fn extraction(language: &str, source: &str) -> Extraction {
    let file = SourceFile {
        path: "sample.source".into(),
        language: language.into(),
        hash: hash(source),
        size: source.len(),
        mode: 0o644,
    };
    analysis::extract(&file, source, &Deadline::new(10000)).unwrap()
}
fn graph(language: &str, source: &str) -> Graph {
    let file = SourceFile {
        path: "sample.source".into(),
        language: language.into(),
        hash: hash(source),
        size: source.len(),
        mode: 0o644,
    };
    let snapshot = Snapshot {
        id: "a".repeat(64),
        workspace_id: "b".repeat(64),
        kind: "test".into(),
        files: vec![file],
        omissions: vec![],
        capture_consistency: "immutable_test_source".into(),
    };
    analysis::assemble(
        &snapshot,
        vec![extraction(language, source)],
        &Deadline::new(10000),
    )
    .unwrap()
}

#[test]
fn new_language_procedure_nodes_have_actual_function_scopes() {
    for (language, source) in [
        ("powershell", "function Local { param($x); $x }\n"),
        ("erlang", "local(X) -> X.\n"),
        ("commonlisp", "(defun local (x) x)\n"),
        (
            "fortran",
            "subroutine local(x)\ninteger :: x\nend subroutine local\n",
        ),
    ] {
        let result = extraction(language, source);
        assert!(
            !result
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.severity == "error"),
            "Invalid {language} fixture"
        );
        let entity = result
            .entities
            .iter()
            .find(|entity| entity.kind == "function")
            .unwrap();
        let name = entity.name_start_byte.unwrap();
        assert!(
            result
                .resolution
                .scopes
                .iter()
                .any(|scope| scope.function && scope.start <= name && name < scope.end),
            "{language} declaration has no function scope: {:?}",
            result.resolution.scopes
        );
    }
}

#[test]
fn powershell_nested_functions_do_not_leak_into_a_sibling_function_scope() {
    let source = "function First { function Local { 1 }; Local }\nfunction Second { Local }\n";
    let result = graph("powershell", source);
    let first = result
        .entities
        .iter()
        .find(|entity| entity.name == "First")
        .unwrap();
    let second = result
        .entities
        .iter()
        .find(|entity| entity.name == "Second")
        .unwrap();
    let local = result
        .entities
        .iter()
        .find(|entity| entity.name == "Local")
        .unwrap();
    assert!(
        result
            .observations
            .iter()
            .any(|edge| edge.relation == "calls"
                && edge.source == first.id
                && edge.target.as_ref() == Some(&local.id))
    );
    assert!(
        result
            .observations
            .iter()
            .any(|edge| edge.relation == "calls"
                && edge.source == second.id
                && edge.target.is_none()),
        "Nested Local incorrectly leaked into Second"
    );
}

#[test]
fn commonlisp_function_invocations_keep_the_exact_original_call_span() {
    let source = "; 🌍\n(defun target () 1)\n(defun caller () (target))\n";
    let result = extraction("commonlisp", source);
    let call = result
        .edges
        .iter()
        .find(|edge| edge.relation == "calls" && edge.target_label == "target")
        .expect("CommonLisp call candidate");
    assert_eq!(&source[call.start_byte..call.end_byte], "(target)");
    assert!(
        !result
            .edges
            .iter()
            .any(|edge| edge.relation == "calls" && edge.target_label == "defun")
    );
}

#[test]
fn objc_message_calls_are_distinct_from_plain_identifier_references() {
    let source = "// 🌍\n@interface Sample\n- (void)target;\n- (void)caller;\n@end\n@implementation Sample\n- (void)target {}\n- (void)caller { [self target]; }\n@end\n";
    let result = extraction("objc", source);
    let call = result
        .edges
        .iter()
        .find(|edge| edge.relation == "calls" && edge.target_label == "target")
        .expect("Objective-C message candidate");
    assert_eq!(&source[call.start_byte..call.end_byte], "[self target]");
    assert!(
        !result
            .edges
            .iter()
            .any(|edge| edge.relation == "references" && edge.target_label == "target")
    );
}

#[test]
fn vb_invocations_and_address_of_references_keep_distinct_relations() {
    let source = "Module Test\n Function target() As Integer\n Return 1\n End Function\n Sub caller()\n Dim f = AddressOf target\n target()\n End Sub\nEnd Module\n";
    let result = extraction("vbnet", source);
    let call = result
        .edges
        .iter()
        .find(|edge| edge.relation == "calls" && edge.target_label == "target")
        .expect("VB invocation candidate");
    assert_eq!(&source[call.start_byte..call.end_byte], "target()");
    let address = source.find("AddressOf target").unwrap() + "AddressOf ".len();
    assert!(result.edges.iter().any(|edge| edge.relation == "references"
        && edge.start_byte == address
        && edge.end_byte == address + "target".len()));
}

#[test]
fn quoted_lisp_lists_and_lambda_lists_do_not_become_function_calls() {
    let source =
        "(defun target () 1)\n(defun caller (parameter) '(target) (quote (target)) (target))\n";
    let result = extraction("commonlisp", source);
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == "error")
    );
    let calls: Vec<_> = result
        .edges
        .iter()
        .filter(|edge| edge.relation == "calls")
        .collect();
    assert_eq!(
        calls.len(),
        1,
        "Quoted data or a lambda list produced calls: {calls:?}"
    );
    assert_eq!(calls[0].target_label, "target");
    assert_eq!(calls[0].start_byte, source.rfind("(target)").unwrap());
}

#[test]
fn explicit_erlang_fun_references_point_to_the_named_function_without_becoming_calls() {
    let source = "target() -> ok.\ncaller() -> fun target/0.\n";
    let result = extraction("erlang", source);
    let at = source.find("fun target").unwrap() + 4;
    assert!(
        result.edges.iter().any(|edge| edge.relation == "references"
            && edge.target_label == "target"
            && edge.start_byte == at
            && edge.end_byte == at + 6),
        "Explicit Erlang function value reference was omitted"
    );
    assert!(
        !result
            .edges
            .iter()
            .any(|edge| edge.relation == "calls" && edge.target_label == "target")
    );
}

#[test]
fn explicit_lisp_function_designators_are_references_and_quoted_data_stays_literal() {
    let source = "(defun target () 1)\n(defun caller () #'target (function target) 'target)\n";
    let result = extraction("commonlisp", source);
    let refs: Vec<_> = result
        .edges
        .iter()
        .filter(|edge| edge.relation == "references" && edge.target_label == "target")
        .collect();
    assert_eq!(
        refs.len(),
        2,
        "Function designators must remain distinct from a quoted atom: {refs:?}"
    );
    for reference in refs {
        assert_eq!(&source[reference.start_byte..reference.end_byte], "target");
    }
    assert!(!result.edges.iter().any(|edge| edge.relation == "calls"));
}

#[test]
fn vb_local_declarations_block_an_unrelated_same_named_module_method() {
    let source = "Module Test\n Function target() As Integer\n Return 1\n End Function\n Sub caller()\n Dim target = Nothing\n target()\n End Sub\nEnd Module\n";
    let result = graph("vbnet", source);
    let caller = result
        .entities
        .iter()
        .find(|entity| entity.name == "caller")
        .unwrap();
    assert!(
        result
            .observations
            .iter()
            .any(|edge| edge.relation == "calls"
                && edge.source == caller.id
                && edge.target.is_none()),
        "Local VB variable falsely resolved to a module method"
    );
}

#[test]
fn fortran_parameter_bindings_do_not_escape_to_another_procedure() {
    let source = "module sample\ncontains\nsubroutine target\nend subroutine target\nsubroutine local(target)\ninteger :: target\nend subroutine local\nsubroutine caller\ncall target\nend subroutine caller\nend module sample\n";
    let result = graph("fortran", source);
    let caller = result
        .entities
        .iter()
        .find(|entity| entity.name == "caller")
        .unwrap();
    let target = result
        .entities
        .iter()
        .find(|entity| entity.name == "target")
        .unwrap();
    assert!(
        result
            .observations
            .iter()
            .any(|edge| edge.relation == "calls"
                && edge.source == caller.id
                && edge.target.as_ref() == Some(&target.id)),
        "A sibling procedure parameter suppressed the module function candidate"
    );
}

#[test]
fn literal_lisp_vectors_do_not_emit_executable_calls_or_function_references() {
    let source =
        "(defun target () 1)\n(defun caller () #((target) #'target (function target)) (target))\n";
    let result = extraction("commonlisp", source);
    assert!(
        !result.diagnostics.iter().any(|d| d.severity == "error"),
        "Invalid literal vector fixture"
    );
    let calls: Vec<_> = result
        .edges
        .iter()
        .filter(|e| e.relation == "calls")
        .collect();
    assert_eq!(
        calls.len(),
        1,
        "Literal vector data was treated as executable: {calls:?}"
    );
    assert_eq!(calls[0].start_byte, source.rfind("(target)").unwrap());
    assert!(
        !result
            .edges
            .iter()
            .any(|e| e.relation == "references" && e.target_label == "target")
    );
}
#[test]
fn objc_untyped_foreign_receiver_does_not_resolve_to_callers_own_method() {
    let source = "@interface Sample\n- (void)target;\n- (void)caller:(id)other;\n@end\n@implementation Sample\n- (void)target {}\n- (void)caller:(id)other { [other target]; }\n@end\n";
    let result = graph("objc", source);
    let caller = result
        .entities
        .iter()
        .filter(|e| e.name == "caller")
        .max_by_key(|e| e.start_byte)
        .unwrap();
    assert!(
        result
            .observations
            .iter()
            .any(|e| e.relation == "calls" && e.source == caller.id && e.target.is_none()),
        "Dynamic foreign receiver falsely bound to caller's method: {:?} {:?}",
        result.entities,
        result.observations
    );
}

#[test]
fn lisp_lexical_declaration_headers_are_not_calls_but_bodies_and_initializers_are() {
    for source in [
        "(defun target () 1) (defun caller () (flet ((local (x) (target))) (local 1)))",
        "(defun target () 1) (defun caller () (let ((x (target))) (target)))",
    ] {
        let result = extraction("commonlisp", source);
        assert!(!result.diagnostics.iter().any(|d| d.severity == "error"));
        let calls: Vec<_> = result
            .edges
            .iter()
            .filter(|e| e.relation == "calls")
            .collect();
        assert_eq!(
            calls.len(),
            2,
            "Declaration headers produced fake calls: {calls:?}"
        );
        assert!(!calls.iter().any(|e| e.target_label == "x"));
        assert!(calls.iter().any(|e| e.target_label == "target"));
        if source.contains("flet") {
            assert_eq!(
                calls.iter().filter(|e| e.target_label == "local").count(),
                1
            );
        }
    }
}

#[test]
fn lisp_class_superclasses_and_slot_headers_are_not_calls_but_initforms_are() {
    let source = "(defun target () 1) (defclass C (Parent) ((slot :initform (target))))";
    let result = extraction("commonlisp", source);
    assert!(!result.diagnostics.iter().any(|d| d.severity == "error"));
    let calls: Vec<_> = result
        .edges
        .iter()
        .filter(|e| e.relation == "calls")
        .collect();
    assert_eq!(
        calls.len(),
        1,
        "Class declaration headers became executable calls: {calls:?}"
    );
    assert_eq!(calls[0].target_label, "target");
    assert_eq!(&source[calls[0].start_byte..calls[0].end_byte], "(target)");
}
