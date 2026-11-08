use serde_json::json;
use yoke::{
    Engine,
    capture::Deadline,
    github::{self, HostReadProfile},
};

#[test]
#[ignore = "Explicit anonymous public GitHub HTTPS read; requires YOKE_GITHUB_PUBLIC_READ=1"]
fn actual_public_github_https_preserves_scope_and_shared_request_budget() {
    assert_eq!(std::env::var("YOKE_GITHUB_PUBLIC_READ").as_deref(), Ok("1"));
    let root = tempfile::tempdir().unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let profile = HostReadProfile {
        allow_network: true,
        allowed_repos: vec!["rust-lang/rust".into()],
        default_repo: Some("rust-lang/rust".into()),
        max_pages: 1,
        max_items: 1,
        max_requests: 3,
        max_response_bytes: 8 * 1024 * 1024,
        aggregate_response_bytes: 16 * 1024 * 1024,
        ..Default::default()
    };
    let observed = github::query(
        &profile,
        &mut engine,
        "list_prs",
        &json!({}),
        &Deadline::new(60000),
    )
    .unwrap();
    assert_eq!(observed["repo"], "rust-lang/rust");
    assert_eq!(observed["transport"], "host_authorized_github_https");
    assert!(observed["budget"]["requests"].as_u64().unwrap() <= 3);
    assert!(observed["items"].as_array().unwrap().len() <= 1);
    assert_eq!(observed["complete"], false);
    assert!(
        observed["receipts"]
            .as_array()
            .unwrap()
            .iter()
            .all(|receipt| {
                receipt["url"]
                    .as_str()
                    .unwrap()
                    .starts_with("https://api.github.com/repos/rust-lang/rust")
                    && receipt["bodyRetained"] == false
            })
    );
    assert!(
        root.path()
            .read_dir()
            .unwrap()
            .all(|entry| entry.unwrap().file_name() == ".yoke")
    );
}
