use regex::Regex;
use yoke::{capture::Deadline, replacement};
#[test]
fn capture_templates_preserve_numeric_named_unicode_escaped_and_missing_references() {
    let re = Regex::new(r"(?P<word>abc)(?P<世界>🌍)").unwrap();
    let captures = re.captures("abc🌍").unwrap();
    let value = replacement::expand(
        &captures,
        "${1}x:$word:${世界}:$$:$missing:$1a:${unfinished",
        100,
        &Deadline::new(1000),
    )
    .unwrap();
    assert_eq!(value, "abcx:abc:🌍:$:::${unfinished");
}
#[test]
fn multiplicative_templates_are_rejected_with_bounded_memory_and_shared_deadline() {
    let source = "x".repeat(32768);
    let re = Regex::new(r"(?s).+").unwrap();
    let captures = re.captures(&source).unwrap();
    let error = replacement::expand(&captures, &"$0".repeat(500000), 64000, &Deadline::new(1000))
        .unwrap_err();
    assert!(error.to_string().contains("budget"), "{error}");
    let value =
        replacement::expand(&captures, &"$$".repeat(30000), 64000, &Deadline::new(1000)).unwrap();
    assert_eq!(value, "$".repeat(30000));
    assert!(replacement::expand(&captures, "$0", 64000, &Deadline::new(0)).is_err());
}
