use serde_json::json;
use yoke::{arxiv_source, capture::Deadline, source_acquisition::ReceivedSource};
fn source(html: &[u8]) -> ReceivedSource {
    ReceivedSource::from_response(&json!({"schemaVersion":1,"format":"html","requestedUrl":"https://arxiv.org/abs/2301.00001","httpStatus":200,"contentType":"text/html; charset=utf-8","etag":null,"lastModified":null}),html,&Deadline::new(30000)).unwrap()
}
#[test]
fn comment_crossing_head_scan_budget_returns_error_without_panicking() {
    let mut html = b"<html><head>".to_vec();
    html.resize(999_998, b' ');
    html.extend_from_slice(b"<!-- beyond finite scan --> </head>");
    assert!(arxiv_source::inspect(&source(&html), 32000, &Deadline::new(30000)).is_err());
}
#[test]
fn inert_and_raw_content_cannot_create_metadata_occurrences() {
    let fake_head =
        source(b"<script><head><meta name=citation_title content=forged></head></script>");
    assert!(arxiv_source::inspect(&fake_head, 32000, &Deadline::new(30000)).is_err());
    let html = br#"<!doctype html><html><head><!-- <meta name=citation_author content=comment> --><script>"<meta name=citation_author content=script>"</script><style><meta name=citation_author content=style></style><title><meta name=citation_author content=title></title><template><meta name=citation_author content=template></template><noscript><meta name=citation_author content=noscript></noscript><META NAME='citation_title' CONTENT='Owned > &#x1f4a1; &amp; quote'><meta name=citation_arxiv_id content=2301.00001></head><body><meta name=citation_author content=body></body></html>"#;
    let v = arxiv_source::inspect(&source(html), 32000, &Deadline::new(30000)).unwrap();
    assert_eq!(v["observations"].as_array().unwrap().len(), 2);
    assert_eq!(v["observations"][0]["value"], "Owned > \u{1f4a1} & quote");
    assert_eq!(
        v["idBinding"],
        "received_base_id_candidates_match_url_label"
    );
}
#[test]
fn malformed_ambiguous_or_over_budget_metadata_is_refused() {
    for head in [
        "<meta name=citation_title NAME=citation_author content=x>",
        "<meta name=citation_title content=x content=y>",
        "<meta name=citation_title content='&unknown;'>",
        "<template><meta name=citation_title content=x></head>",
        "<meta name=citation_title content='unterminated>",
    ] {
        let html = format!("<html><head>{head}</head></html>");
        assert!(
            arxiv_source::inspect(&source(html.as_bytes()), 32000, &Deadline::new(30000)).is_err(),
            "{head}"
        );
    }
    let html = format!(
        "<head>{}</head>",
        "<meta name=citation_author content=x>".repeat(1025)
    );
    assert!(
        arxiv_source::inspect(&source(html.as_bytes()), 16_000_000, &Deadline::new(30000)).is_err()
    );
    let owned = source(b"<head><meta name=citation_title content=owned></head>");
    assert!(arxiv_source::inspect(&owned, 8, &Deadline::new(30000)).is_err());
    assert!(arxiv_source::inspect(&owned, 32000, &Deadline::new(0)).is_err());
}
#[test]
fn selector_labels_and_version_claims_remain_separate_from_received_metadata() {
    for id in [
        "2301.00001",
        "2301.00001v12",
        "hep-th/9901001",
        "math.GT/0301001v1",
    ] {
        arxiv_source::validate_id(id).unwrap();
    }
    for id in [
        "https://arxiv.org/abs/2301.00001",
        "../2301.00001",
        "2300.00001",
        "2313.00001",
        "2301.00001v0",
        "2301.00001?x=1",
        "2301.00001#x",
        "2301.00001\n",
        "2301.00001\u{200b}",
    ] {
        assert!(arxiv_source::validate_id(id).is_err(), "{id:?}");
    }
    let v=arxiv_source::inspect(&source(b"<head><meta name=citation_arxiv_id content=2301.00002v2><meta name=citation_title content=x></head>"),32000,&Deadline::new(30000)).unwrap();
    assert_eq!(v["idBinding"], "mismatch");
    assert_eq!(
        v["versionBinding"],
        "not_established_from_base_id_or_url_label_alone"
    );
    let v = arxiv_source::inspect(
        &source(b"<head><meta name=citation_title content=x></head>"),
        32000,
        &Deadline::new(30000),
    )
    .unwrap();
    assert_eq!(v["idBinding"], "missing");
    assert!(v["fieldCounts"]["citation_author"].is_null());
}
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
