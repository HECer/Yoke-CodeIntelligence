use super::*;
#[test]
fn finite_pattern_policy_keeps_directory_depth_case_and_unicode_distinct() {
    for (pattern, path, expected) in [
        ("*", "src/a.rs", true),
        ("*.rs", "src/a.rs", true),
        ("*.rs", "src/a.ts", false),
        ("docs/*", "docs/a.md", true),
        ("docs/*", "docs/deep/a.md", false),
        ("/docs/", "docs/deep/a.md", true),
        ("/docs/", "other/docs/a.md", false),
        ("apps/", "other/apps/a.rs", true),
        ("apps/", "apps", false),
        ("/apps/github", "apps/github/a.rs", true),
        ("/apps/github", "other/apps/github/a.rs", false),
        ("**/logs", "logs/a.txt", true),
        ("**/logs", "deep/logs/a.txt", true),
        ("a/**/b.rs", "a/b.rs", true),
        ("a/**/b.rs", "a/deep/b.rs", true),
        ("/SRC/*", "src/a.rs", false),
        ("/src/?.rs", "src/β.rs", true),
        ("/src/?.rs", "src/e\u{301}.rs", false),
    ] {
        assert_eq!(
            matcher(pattern).unwrap().unwrap().is_match(path),
            expected,
            "{pattern} {path}"
        );
    }
}
#[test]
fn unsupported_patterns_are_explicit_and_do_not_become_empty_complete_matches() {
    for p in [
        "",
        "/",
        "../x",
        "a/../b",
        "//root/x",
        "C:/secret",
        "!a.rs",
        "[a-z]",
        "a\\ b",
        "***",
        "a**b",
        "{a,b}",
        "\u{feff}*.rs",
        "a\0b",
    ] {
        assert!(matcher(p).unwrap().is_none(), "{p:?}");
    }
}
#[test]
fn parser_preserves_comment_crlf_byte_offsets_and_repeated_owner_tokens() {
    let source = "# β 💡\r\n/src/β.rs @same @same # comment\r\n/src/empty.rs\r\n";
    let mut used = Used::default();
    let rules = parse_rules(
        source,
        &mut used,
        &Limits::default(),
        &Deadline::new(10_000),
    )
    .unwrap();
    assert_eq!(rules.len(), 2);
    assert_eq!(used.owner_tokens, 2);
    assert_eq!(rules[0].record["ownerLabels"], json!(["@same", "@same"]));
    assert_eq!(rules[1].record["ownerLabels"], json!([]));
    for rule in rules {
        let b = rule.record["startByte"].as_u64().unwrap() as usize;
        let e = rule.record["endByte"].as_u64().unwrap() as usize;
        assert_eq!(&source[b..e], rule.record["quote"].as_str().unwrap());
        for token in rows(&rule.record, "ownerTokens").unwrap() {
            let b = token["startByte"].as_u64().unwrap() as usize;
            let e = token["endByte"].as_u64().unwrap() as usize;
            assert_eq!(&source[b..e], token["label"].as_str().unwrap());
        }
    }
}
