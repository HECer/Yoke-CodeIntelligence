use serde_json::json;
use yoke::{capture::Deadline, source_acquisition::ReceivedSource};
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
