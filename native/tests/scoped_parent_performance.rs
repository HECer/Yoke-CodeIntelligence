use yoke::{
    analysis,
    capture::Deadline,
    model::{SourceFile, hash},
};

#[test]
fn wide_comment_siblings_preserve_source_facts_within_the_request_deadline() {
    let text = format!(
        "{}\nfn target() {{}}\nfn caller() {{ target(); }}\n",
        "// source padding for elapsed analysis\n".repeat(12_000)
    );
    let file = SourceFile {
        path: "wide.rs".into(),
        hash: hash(&text),
        size: text.len(),
        language: "rust".into(),
        mode: 0o644,
    };
    // A single small function after ordinary comments must remain usable within
    // a bounded request; no wall-clock assertion or machine-speed ratio is used.
    let extracted = analysis::extract(&file, &text, &Deadline::new(5_000)).unwrap();
    let functions: Vec<_> = extracted
        .entities
        .iter()
        .filter(|entity| entity.kind == "function")
        .collect();
    assert_eq!(
        functions
            .iter()
            .map(|entity| entity.name.as_str())
            .collect::<Vec<_>>(),
        ["target", "caller"]
    );
    for entity in functions {
        assert_eq!(
            &text[entity.name_start_byte.unwrap()..entity.name_end_byte.unwrap()],
            entity.name
        );
    }
    let calls: Vec<_> = extracted
        .resolution
        .uses
        .iter()
        .filter(|usage| usage.relation == "calls")
        .collect();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "target");
    assert!(text[calls[0].start..calls[0].end].contains("target()"));
    assert_eq!(
        extracted
            .resolution
            .scopes
            .iter()
            .filter(|scope| scope.function)
            .count(),
        2
    );
}
