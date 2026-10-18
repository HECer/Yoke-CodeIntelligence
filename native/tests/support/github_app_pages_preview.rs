//! Reuses only our owned completed-worker fixture; no external service or key.
use super::*;
use yoke::github_app::pages;

fn request(result: &Value) -> pages::PreviewRequest {
    pages::PreviewRequest {
        schema_version: 1,
        completion_id: result["completionId"].as_str().unwrap().into(),
        expected_queue_revision: 2,
        public_base_url: "https://reports.example.invalid".into(),
    }
}
fn options(f: &Fixture) -> pages::Options {
    pages::Options {
        worker: f.options.clone(),
        ..Default::default()
    }
}
fn records(f: &Fixture) -> Vec<(String, String, String)> {
    f.engine
        .store
        .db
        .prepare("SELECT kind,id,value FROM records ORDER BY kind,id")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap()
}
fn preview(
    f: &Fixture,
    request: &pages::PreviewRequest,
    o: &pages::Options,
) -> anyhow::Result<Value> {
    pages::preview(&f.engine.store, &f.host, request, o, &f.d)
}

#[test]
fn preview_replays_exact_source_into_an_inert_graph_without_persistence_or_http() {
    let f = Fixture::new();
    let completion = completed(&f);
    let before = records(&f);
    let blobs = f.blobs();
    let value = preview(&f, &request(&completion), &options(&f)).unwrap();
    assert_eq!(
        value,
        preview(&f, &request(&completion), &options(&f)).unwrap()
    );
    assert_eq!(value["completionId"], completion["completionId"]);
    assert_eq!(value["headSnapshotId"], f.snapshot);
    assert_eq!(value["repositoryId"], 9);
    assert_eq!(value["number"], 7);
    assert_eq!(value["networkRequests"], 0);
    assert_eq!(value["modelCalls"], 0);
    for flag in [
        "persisted",
        "published",
        "pageWriteAuthority",
        "publicationAuthority",
        "sourceWritten",
        "currentHeadVerified",
        "complete",
    ] {
        assert_eq!(value[flag], false, "{flag}");
    }
    let nodes = value["graph"]["head"]["nodes"].as_array().unwrap();
    let target = nodes.iter().find(|n| n["name"] == "target").unwrap();
    let caller = nodes.iter().find(|n| n["name"] == "caller").unwrap();
    assert!(
        target["roles"]
            .as_array()
            .unwrap()
            .contains(&json!("changed"))
    );
    assert!(
        caller["roles"]
            .as_array()
            .unwrap()
            .contains(&json!("incoming"))
    );
    let edges = value["graph"]["head"]["observations"].as_array().unwrap();
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0]["source"], caller["id"]);
    assert_eq!(edges[0]["target"], target["id"]);
    assert_eq!(edges[0]["relation"], "calls");
    assert_eq!(target["path"], PATH);
    assert!(yoke::model::valid_id(value["pageId"].as_str().unwrap()));
    assert!(yoke::model::valid_id(value["proposalId"].as_str().unwrap()));
    let html = value["html"].as_str().unwrap();
    assert_eq!(value["htmlHash"], yoke::model::hash(html.as_bytes()));
    assert!(html.starts_with("<!doctype html>"));
    assert!(html.contains("Structural candidates"));
    assert!(!html.contains(SOURCE));
    assert!(!value["graph"].to_string().contains("\"signature\""));
    assert!(!value.to_string().contains(TOKEN));
    assert!(
        value["url"]
            .as_str()
            .unwrap()
            .starts_with("https://reports.example.invalid/p/")
    );
    assert_eq!(records(&f), before);
    assert_eq!(f.blobs(), blobs);
    assert_eq!(
        std::fs::read(f.root.path().join(PATH)).unwrap(),
        SOURCE.as_bytes()
    );
}

#[test]
fn preview_cannot_promote_a_claim_stale_selection_or_a_reinstalled_old_job() {
    let mut f = Fixture::new();
    let completion = completed(&f);
    let mut req = request(&completion);
    req.expected_queue_revision = 1;
    assert!(preview(&f, &req, &options(&f)).is_err());
    req = request(&completion);
    req.completion_id = f.claim.evidence()["claimId"].as_str().unwrap().into();
    assert!(preview(&f, &req, &options(&f)).is_err());
    f.host = host_for(43);
    fail_marker(
        preview(&f, &request(&completion), &options(&f)),
        "github_app_worker_grant_denied",
    );
}

#[test]
fn preview_rechecks_captured_bytes_and_preserves_a_refused_caller_transaction() {
    let f = Fixture::new();
    let completion = completed(&f);
    let req = request(&completion);
    f.engine
        .store
        .db
        .execute_batch("CREATE TEMP TABLE page_tx(v INTEGER); BEGIN; INSERT INTO page_tx VALUES(1)")
        .unwrap();
    fail_marker(
        preview(&f, &req, &options(&f)),
        "github_app_pages_caller_transaction_refused",
    );
    assert!(!f.engine.store.db.is_autocommit());
    assert_eq!(
        f.engine
            .store
            .db
            .query_row("SELECT count(*) FROM page_tx", [], |r| r.get::<_, usize>(0))
            .unwrap(),
        1
    );
    f.engine.store.db.execute_batch("ROLLBACK").unwrap();
    std::fs::remove_file(
        f.engine
            .store
            .dir
            .join("blobs")
            .join(yoke::model::hash(SOURCE.as_bytes())),
    )
    .unwrap();
    fail_marker(
        preview(&f, &req, &options(&f)),
        "github_app_worker_source_unavailable",
    );
}

#[test]
fn preview_admits_exact_html_and_stdout_limits_and_refuses_graph_truncation() {
    let f = Fixture::new();
    let completion = completed(&f);
    let req = request(&completion);
    let mut o = options(&f);
    let value = preview(&f, &req, &o).unwrap();
    o.max_html_bytes = value["html"].as_str().unwrap().len();
    o.max_output_bytes = serde_json::to_vec(&value).unwrap().len() + 1;
    assert_eq!(preview(&f, &req, &o).unwrap(), value);
    o.max_html_bytes -= 1;
    fail_marker(preview(&f, &req, &o), "github_app_pages_capacity_exceeded");
    o = options(&f);
    o.max_output_bytes = serde_json::to_vec(&value).unwrap().len();
    fail_marker(preview(&f, &req, &o), "github_app_pages_capacity_exceeded");
    o = options(&f);
    o.max_nodes = 1;
    fail_marker(preview(&f, &req, &o), "github_app_pages_capacity_exceeded");
    o = options(&f);
    o.max_observations = 0;
    fail_marker(preview(&f, &req, &o), "github_app_pages_capacity_exceeded");
}

#[test]
fn request_rejects_duplicates_unknown_fields_and_non_origin_public_urls() {
    let valid = json!({"schemaVersion":1,"completionId":"a".repeat(64),"expectedQueueRevision":2,"publicBaseUrl":"https://reports.example.invalid"});
    assert!(pages::PreviewRequest::parse(&serde_json::to_vec(&valid).unwrap()).is_ok());
    for url in [
        "http://reports.example.invalid",
        "https://user:pass@reports.example.invalid",
        "https://reports.example.invalid/path",
        "https://reports.example.invalid?x=1",
        "https://reports.example.invalid#token",
        "file:///tmp/page",
        "https://reports.example.invalid/",
    ] {
        let mut bad = valid.clone();
        bad["publicBaseUrl"] = json!(url);
        fail_marker(
            pages::PreviewRequest::parse(&serde_json::to_vec(&bad).unwrap()),
            "github_app_pages_input_invalid",
        );
    }
    let duplicate = format!(
        "{{\"schemaVersion\":1,\"completionId\":\"{}\",\"expectedQueueRevision\":2,\"expectedQueueRevision\":2,\"publicBaseUrl\":\"https://reports.example.invalid\"}}",
        "a".repeat(64)
    );
    fail_marker(
        pages::PreviewRequest::parse(duplicate.as_bytes()),
        "github_app_pages_input_invalid",
    );
    let mut unknown = valid;
    unknown["html"] = json!("<h1>caller content cannot grant source authority</h1>");
    fail_marker(
        pages::PreviewRequest::parse(&serde_json::to_vec(&unknown).unwrap()),
        "github_app_pages_input_invalid",
    );
}

#[test]
#[ignore = "Exports one owned native HTML fixture for the explicitly selected browser oracle"]
fn preview_exports_owned_native_html_for_browser_oracle() {
    let f = Fixture::new();
    let completion = completed(&f);
    let before = records(&f);
    let blobs = || {
        f.blobs()
            .into_iter()
            .map(|name| {
                let digest = yoke::model::hash(
                    std::fs::read(f.engine.store.dir.join("blobs").join(&name)).unwrap(),
                );
                (name, digest)
            })
            .collect::<Vec<_>>()
    };
    let before_blobs = blobs();
    let value = preview(&f, &request(&completion), &options(&f)).unwrap();
    let html = value["html"].as_str().unwrap().as_bytes();
    assert_eq!(records(&f), before);
    assert_eq!(blobs(), before_blobs);
    assert_eq!(
        std::fs::read(f.root.path().join(PATH)).unwrap(),
        SOURCE.as_bytes()
    );
    let metadata = json!({"oracle":"native_pages_preview_html_v1","fixtureOnly":true,
        "htmlSha256":yoke::model::hash(html),"completionId":value["completionId"],
        "headSnapshotId":value["headSnapshotId"],"reviewHash":value["reviewHash"],
        "sourceSha256":yoke::model::hash(SOURCE.as_bytes()),"recordsUnchanged":true,
        "casFilesHashed":before_blobs.len(),"sourceBytesUnchanged":true,
        "nodes":value["graph"]["head"]["nodes"],
        "observations":value["graph"]["head"]["observations"],
        "networkRequestsDuringPreview":value["networkRequests"],"modelCalls":value["modelCalls"],
        "productionRouteValidated":false});
    for (variable, bytes) in [
        ("YOKE_PAGES_BROWSER_EXPORT", html.to_vec()),
        (
            "YOKE_PAGES_BROWSER_METADATA",
            serde_json::to_vec_pretty(&metadata).unwrap(),
        ),
    ] {
        let path = std::env::var_os(variable).expect("explicit browser fixture output path");
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .unwrap();
        file.write_all(&bytes).unwrap();
        file.sync_all().unwrap();
    }
    println!(
        "{}",
        json!({"oracle":"native_pages_preview_html_v1","fixtureOnly":true,
        "htmlSha256":metadata["htmlSha256"],"recordsUnchanged":true,"casFilesHashed":before_blobs.len(),
        "sourceBytesUnchanged":true,"networkRequestsDuringPreview":0,"modelCalls":0,
        "productionRouteValidated":false})
    );
}
