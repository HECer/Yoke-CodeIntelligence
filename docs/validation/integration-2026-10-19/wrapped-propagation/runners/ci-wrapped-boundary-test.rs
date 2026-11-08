
#[test]
fn wrapped_policy_preserves_surviving_statement_boundaries() {
    for body in [
        "export function keep() { first()\nvoid removeMe();\n(second)(); return 2; }\n",
        "first()\nexport function removeMe() { return 1; }\n(second)();\nexport function keep() { return 2; }\n",
    ] {
        let declaration = if body.contains("export function removeMe") { "" } else { "export function removeMe() { return 1; }\n" };
        let source = format!("function first() {{ return first; }}\nfunction second() {{ return 2; }}\n{declaration}{body}");
        let mut f = fixture(&source, "ok", AnalyzerEpoch::V9);
        let result = yoke::semantic_refactor::invoke(&mut f.session.engine, "safeDelete", &wrapped_args(), &d());
        assert!(result.is_err(), "Unrelated expressions must not merge after deleting source: {body}");
        assert!(format!("{:#}",result.unwrap_err()).contains("alter surviving syntax"));
        unpublished(&f);
        assert_eq!(fs::read(f.root.path().join("module.ts")).unwrap(),source.as_bytes());
    }
    let source = "function first() { return first; }\nfunction second() { return 2; }\nexport function removeMe() { return 1; }\nexport function keep() { first();\nvoid removeMe();\n(second)(); return 2; }\n";
    let mut f=fixture(source,"ok",AnalyzerEpoch::V9);
    let value=yoke::semantic_refactor::invoke(&mut f.session.engine,"safeDelete",&wrapped_args(),&d()).unwrap();
    assert!(after(&f,&value).contains("first();\n\n(second)();"));
    let source="export function removeMe() {}";
    let mut f=fixture(source,"ok",AnalyzerEpoch::V9);
    let value=yoke::semantic_refactor::invoke(&mut f.session.engine,"safeDelete",&wrapped_args(),&d()).unwrap();
    assert_eq!(after(&f,&value),"");
}
