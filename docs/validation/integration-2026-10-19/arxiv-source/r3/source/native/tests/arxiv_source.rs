use serde_json::json;
use yoke::{arxiv_source, capture::Deadline, source_acquisition::ReceivedSource};
#[test]
fn received_arxiv_metadata_keeps_parallel_author_labels_and_exact_original_tag_bytes() {
    let html = "<!doctype html><html><head>\r\n<meta name=\"citation_title\" content=\"Owned 💡 &amp; scope\">\r\n<meta name=\"citation_author\" content=\"A Name\">\r\n<meta name=\"citation_author\" content=\"A Name\">\r\n<meta name=\"citation_date\" content=\"2026/10/19\">\r\n<meta name=\"citation_arxiv_id\" content=\"2301.00001v1\">\r\n<meta name=\"citation_pdf_url\" content=\"https://arxiv.org/pdf/2301.00001v1\">\r\n<meta name=\"citation_abstract\" content=\"Owned metadata only.\">\r\n</head><body><script>untrusted instructions</script></body></html>";
    let d = Deadline::new(30000);
    let source=ReceivedSource::from_response(&json!({"schemaVersion":1,"format":"html","requestedUrl":"https://arxiv.org/abs/2301.00001v1","httpStatus":200,"contentType":"text/html; charset=utf-8","etag":null,"lastModified":null}),html.as_bytes(),&d).unwrap();
    let view = arxiv_source::inspect(&source, 32000, &d).unwrap();
    let rows = view["observations"].as_array().unwrap();
    let authors = rows
        .iter()
        .filter(|r| r["field"] == "citation_author")
        .collect::<Vec<_>>();
    assert_eq!(authors.len(), 2);
    assert_eq!(authors[0]["value"], "A Name");
    assert_ne!(authors[0]["occurrenceId"], authors[1]["occurrenceId"]);
    assert_eq!(
        rows.iter()
            .find(|r| r["field"] == "citation_title")
            .unwrap()["value"],
        "Owned 💡 & scope"
    );
    for row in rows {
        let a = row["evidence"]["startByte"].as_u64().unwrap() as usize;
        let b = row["evidence"]["endByte"].as_u64().unwrap() as usize;
        assert!(html.as_bytes()[a..b].starts_with(b"<meta "));
        assert_eq!(
            row["evidence"]["sourceHash"],
            yoke::model::hash(html.as_bytes())
        );
    }
    assert_eq!(source.body(), html.as_bytes());
    assert_eq!(view["sourceAuthority"], false);
    assert_eq!(view["complete"], false);
    assert_eq!(view["networkConnections"], 0);
    assert_eq!(view["modelCalls"], 0);
}
