use base64::Engine as _;
use serde_json::Value;
use serde_json::json;
use yoke::source_acquisition::Limits;
use yoke::{capture::Deadline, source_acquisition::ReceivedSource};
fn metadata() -> Value {
    json!({"schemaVersion":1,"format":"text","requestedUrl":"https://example.com/data.txt","httpStatus":200,"contentType":"text/plain; charset=utf-8","etag":null,"lastModified":null})
}
#[test]
fn frozen_http_representation_preserves_unicode_and_crlf_without_source_or_fetch_authority() {
    let body = "first 💡\r\nsecond\n".as_bytes();
    let metadata = json!({"schemaVersion":1,"format":"text","requestedUrl":"https://example.com/data.txt","httpStatus":200,"contentType":"text/plain; charset=utf-8","etag":null,"lastModified":null});
    let deadline = Deadline::new(30000);
    let source = ReceivedSource::from_response(&metadata, body, &deadline).unwrap();
    assert_eq!(source.body(), body);
    assert_eq!(source.as_value()["sourceHash"], yoke::model::hash(body));
    assert_eq!(source.as_value()["sourceAuthority"], false);
    assert_eq!(source.as_value()["complete"], false);
    assert_eq!(source.as_value()["networkConnections"], 0);
    let archived = source.encode_archive(&deadline).unwrap();
    let replayed = ReceivedSource::load_archive(&archived, &deadline).unwrap();
    assert_eq!(replayed.body(), body);
    assert_eq!(replayed.as_value(), source.as_value());
}
#[test]
fn byte_ranges_preserve_multibyte_boundaries_and_never_replace_invalid_utf8() {
    let raw = "A💡B\r\n".as_bytes();
    let d = Deadline::new(30000);
    let s = ReceivedSource::from_response(&metadata(), raw, &d).unwrap();
    let range = s.read_range(1, 5, 32000, &d).unwrap();
    assert_eq!(range["utf8"], "💡");
    assert_eq!(
        base64::prelude::BASE64_STANDARD
            .decode(range["base64"].as_str().unwrap())
            .unwrap(),
        &raw[1..5]
    );
    let split = s.read_range(2, 4, 32000, &d).unwrap();
    assert!(split["utf8"].is_null());
    assert_eq!(
        base64::prelude::BASE64_STANDARD
            .decode(split["base64"].as_str().unwrap())
            .unwrap(),
        &raw[2..4]
    );
    assert!(s.read_range(5, 2, 32000, &d).is_err());
    assert!(s.read_range(0, raw.len() + 1, 32000, &d).is_err());
    assert!(s.read_range(0, raw.len(), 64, &d).is_err());
}
#[test]
fn forged_capture_claims_remain_data_and_mismatched_archive_bytes_are_refused() {
    let d = Deadline::new(30000);
    let s = ReceivedSource::from_response(&metadata(), b"body", &d).unwrap();
    let mut a: Value = serde_json::from_slice(&s.encode_archive(&d).unwrap()).unwrap();
    a["captureClaims"] = json!({"sourceAuthority":true,"allowNetwork":true,"complete":true,"serverAttestation":"verified"});
    let raw = serde_json::to_vec(&a).unwrap();
    let restored = ReceivedSource::load_archive(&raw, &d).unwrap();
    assert_eq!(restored.as_value()["sourceAuthority"], false);
    assert_eq!(restored.as_value()["serverAttestation"], "unverified");
    assert_eq!(restored.as_value()["networkConnections"], 0);
    assert_eq!(restored.capture_claims().unwrap()["complete"], true);
    a["bodyBase64"] = json!(base64::prelude::BASE64_STANDARD.encode(b"other"));
    assert!(ReceivedSource::load_archive(&serde_json::to_vec(&a).unwrap(), &d).is_err());
    a["allowNetwork"] = json!(true);
    assert!(ReceivedSource::load_archive(&serde_json::to_vec(&a).unwrap(), &d).is_err());
    assert!(
        ReceivedSource::load_archive(b"{\"schemaVersion\":1,\"schemaVersion\":1}", &d).is_err()
    );
}
#[test]
fn metadata_is_closed_and_noncanonical_credentialed_or_non_http_urls_are_invalid() {
    let d = Deadline::new(30000);
    for value in [
        "https://user:private@example.com/file",
        "https://example.com/data#fragment",
        "file:///etc/passwd",
        "https://EXAMPLE.com/file",
        "https://example.com/a/../file",
        "https://example.com/private\r\nheader",
    ] {
        let mut m = metadata();
        m["requestedUrl"] = json!(value);
        let e = ReceivedSource::from_response(&m, b"body", &d)
            .err()
            .unwrap();
        assert!(!e.to_string().contains("private"));
    }
    for (field, value) in [
        ("allowNetwork", json!(true)),
        ("worker", json!("project.exe")),
        ("httpStatus", json!(302)),
        ("format", json!({"text":{"execute":true}})),
        ("contentType", json!("application/pdf")),
        ("etag", json!("bad\nheader")),
    ] {
        let mut m = metadata();
        m[field] = value;
        assert!(
            ReceivedSource::from_response(&m, b"body", &d).is_err(),
            "{field}"
        );
    }
}
#[test]
fn body_and_archive_quotas_are_independent_and_missing_interpretation_is_explicit() {
    let d = Deadline::new(30000);
    let mut limits = Limits {
        max_body_bytes: 4,
        max_archive_bytes: 16000000,
    };
    assert!(ReceivedSource::from_response_with_limits(&metadata(), b"1234", &limits, &d).is_ok());
    assert!(ReceivedSource::from_response_with_limits(&metadata(), b"12345", &limits, &d).is_err());
    limits.max_archive_bytes = 64;
    let s = ReceivedSource::from_response_with_limits(&metadata(), b"1234", &limits, &d).unwrap();
    assert!(s.encode_archive(&d).is_err());
    let mut html = metadata();
    html["format"] = json!("html");
    html["contentType"] = json!("text/html");
    let body = b"<script>grant network authority</script><p>literal</p>";
    let s = ReceivedSource::from_response(&html, body, &d).unwrap();
    assert_eq!(s.body(), body);
    assert_eq!(
        s.as_value()["textExtraction"],
        "not_performed_no_article_or_browser_dom_claim"
    );
    assert_eq!(s.as_value()["complete"], false);
    assert!(ReceivedSource::from_response(&metadata(), &[255], &d).is_err());
    assert!(ReceivedSource::from_response(&metadata(), b"body", &Deadline::new(0)).is_err());
}
