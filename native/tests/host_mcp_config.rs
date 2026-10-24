use serde_json::json;
use yoke::{
    capture::Deadline,
    host_config::{self, OwnedFragment},
};
fn d() -> Deadline {
    Deadline::new(30000)
}
fn body() -> Vec<u8> {
    serde_json::to_vec(
        &json!({"command":"C:\\host\\yoke.exe","args":["--root","C:\\owned\\π"],"env":{}}),
    )
    .unwrap()
}
fn install(input: &[u8]) -> (Vec<u8>, OwnedFragment) {
    let patch =
        host_config::json_mcp_patch(input, "yoke_owned", Some(&body()), None, &d()).unwrap();
    (
        host_config::apply_patch(input, &patch, &d()).unwrap(),
        patch.owned_fragment.unwrap(),
    )
}
fn uninstall(input: &[u8], prior: &OwnedFragment) -> Vec<u8> {
    let patch = host_config::json_mcp_patch(input, "yoke_owned", None, Some(prior), &d()).unwrap();
    host_config::apply_patch(input, &patch, &d()).unwrap()
}
#[test]
fn created_document_and_parent_roundtrip_without_normalizing_foreign_json() {
    for original in [
        b"".as_slice(),
        b"{}",
        b"{\r\n  \"human\": [1,2] \r\n}",
        b"{\"mcpServers\":{}}",
        b"{\"mcpServers\":{\"foreign\":{\"command\":\"owned-foreign\"}}}",
    ] {
        let (installed, prior) = install(original);
        assert!(serde_json::from_slice::<serde_json::Value>(&installed).unwrap()["mcpServers"]["yoke_owned"].is_object());
        assert_eq!(uninstall(&installed, &prior), original);
        let noop = host_config::json_mcp_patch(
            &installed,
            "yoke_owned",
            Some(&body()),
            Some(&prior),
            &d(),
        )
        .unwrap();
        assert_eq!(
            host_config::apply_patch(&installed, &noop, &d()).unwrap(),
            installed
        );
    }
    let original =
        "\u{feff}{\r\n  \"human\":\"café 🌍\",\r\n  \"mcpServers\":{}\r\n}\r\n".as_bytes();
    let (installed, prior) = install(original);
    assert_eq!(uninstall(&installed, &prior), original);
}
#[test]
fn foreign_servers_and_formatting_survive_entry_and_container_cleanup() {
    let (installed, prior) = install(b"");
    let text = String::from_utf8(installed).unwrap();
    let grown = text.replacen("}}\n", ",\"foreign\":{\"command\":\"foreign\"}}}\n", 1);
    let removed = uninstall(grown.as_bytes(), &prior);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&removed).unwrap(),
        json!({"mcpServers":{"foreign":{"command":"foreign"}}})
    );
    assert!(
        String::from_utf8(removed)
            .unwrap()
            .contains("\"foreign\":{\"command\":\"foreign\"}")
    );
    let (installed, prior) = install(b"");
    let with_format = String::from_utf8(installed)
        .unwrap()
        .replace("\"mcpServers\":{", "\"mcpServers\":{ \r\n");
    assert_eq!(
        uninstall(with_format.as_bytes(), &prior),
        b"{\"mcpServers\":{ \r\n}}\n"
    );
    let (installed, prior) = install(b"{\"human\":true}");
    let text = String::from_utf8(installed).unwrap();
    let grown = format!("{},\"foreign\":{{}}}}}}", text.strip_suffix("}}").unwrap());
    let removed = uninstall(grown.as_bytes(), &prior);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&removed).unwrap(),
        json!({"human":true,"mcpServers":{"foreign":{}}})
    );
}
#[test]
fn receipt_scope_distinguishes_mirrored_bytes_and_rejects_changed_entries() {
    let value = body();
    let same = String::from_utf8(value.clone()).unwrap();
    let original = format!("{{\"mirror\":{{\"yoke_owned\":{same}}},\"mcpServers\":{{}}}}");
    let (installed, prior) = install(original.as_bytes());
    assert_eq!(uninstall(&installed, &prior), original.as_bytes());
    let mutated = String::from_utf8(installed.clone()).unwrap().replacen(
        "C:\\\\host\\\\yoke.exe",
        "C:\\\\host\\\\other.exe",
        2,
    );
    assert_ne!(mutated.as_bytes(), installed);
    assert!(
        host_config::json_mcp_patch(mutated.as_bytes(), "yoke_owned", None, Some(&prior), &d())
            .is_err()
    );
    assert!(
        host_config::json_mcp_patch(&installed, "yoke_owned", Some(&value), None, &d()).is_err()
    );
    let mut forged = prior.clone();
    forged.hash = "f".repeat(64);
    assert!(
        host_config::json_mcp_patch(&installed, "yoke_owned", None, Some(&forged), &d()).is_err()
    );
    let mut forged = prior.clone();
    forged.selector = forged.selector.replace("yoke_owned", "other");
    assert!(
        host_config::json_mcp_patch(&installed, "yoke_owned", None, Some(&forged), &d()).is_err()
    );
    assert!(
        host_config::json_mcp_patch(b"", "yoke_owned", Some(&value), Some(&prior), &d()).is_err()
    );
}
#[test]
fn strict_json_unicode_budgets_and_stale_patch_fail_without_fallback() {
    for original in [
        b"{\"mcpServers\":null}".as_slice(),
        b"{\"mcpServers\":{},\"mcpServers\":{}}",
        b"{ /*comment*/ }",
        b"{\"human\":1,}",
        b"[]",
        b"\xff",
        b"{\0}",
    ] {
        assert!(
            host_config::json_mcp_patch(original, "yoke_owned", Some(&body()), None, &d()).is_err()
        );
    }
    assert!(host_config::json_mcp_patch(b"{}", "bad/name", Some(&body()), None, &d()).is_err());
    assert!(host_config::json_mcp_patch(b"{}", "yoke_owned", None, None, &d()).is_err());
    assert!(host_config::json_mcp_patch(b"{}", "yoke_owned", Some(b" [] "), None, &d()).is_err());
    assert!(
        host_config::json_mcp_patch(
            &vec![b' '; 1_048_577],
            "yoke_owned",
            Some(&body()),
            None,
            &d()
        )
        .is_err()
    );
    assert!(
        host_config::json_mcp_patch(b"{}", "yoke_owned", Some(&vec![b' '; 65_537]), None, &d())
            .is_err()
    );
    assert!(
        host_config::json_mcp_patch(b"{}", "yoke_owned", Some(&body()), None, &Deadline::new(0))
            .is_err()
    );
    let patch =
        host_config::json_mcp_patch(b"{}", "yoke_owned", Some(&body()), None, &d()).unwrap();
    assert!(host_config::apply_patch(b"{ }", &patch, &d()).is_err());
}
