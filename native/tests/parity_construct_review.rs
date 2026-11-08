use yoke::{
    analysis,
    capture::Deadline,
    model::{SourceFile, hash},
};
fn extract(source: &str) -> yoke::model::Extraction {
    analysis::extract(
        &SourceFile {
            path: "sample.lisp".into(),
            language: "commonlisp".into(),
            hash: hash(source),
            size: source.len(),
            mode: 0o644,
        },
        source,
        &Deadline::new(10000),
    )
    .unwrap()
}
#[test]
fn lisp_let_binding_pairs_are_not_invocations_of_same_named_functions() {
    let source = "(defun target () 1)\n(defun caller () (let ((target 2)) target))\n";
    let extraction = extract(source);
    assert!(
        !extraction.diagnostics.iter().any(|d| d.severity == "error"),
        "invalid review fixture"
    );
    assert!(
        !extraction
            .edges
            .iter()
            .any(|edge| edge.relation == "calls" && edge.target_label == "target"),
        "Lisp binding pair was falsely emitted as an executable call: {:?}",
        extraction.edges
    );
}
#[test]
fn quoted_lisp_declarations_do_not_create_executable_declaration_entities() {
    let source = "(defun caller () '(defun imaginary () 1) '(defclass Phantom () ()))\n";
    let extraction = extract(source);
    assert!(
        !extraction.diagnostics.iter().any(|d| d.severity == "error"),
        "invalid review fixture"
    );
    assert!(
        !extraction
            .entities
            .iter()
            .any(|entity| entity.name == "imaginary" || entity.name == "Phantom"),
        "Quoted Lisp data produced source declaration claims: {:?}",
        extraction.entities
    );
}
