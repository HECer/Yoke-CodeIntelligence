#![cfg(windows)]
use serde_json::{Value, json};
use std::{fs, path::PathBuf};
use yoke::{
    capture::Deadline,
    host_install::{self, JsonMcpClient, JsonMcpSource, Selection, WriteAuthority},
};
struct Fixture {
    _dir: tempfile::TempDir,
    selection: Selection,
    root: PathBuf,
    source: PathBuf,
    state: PathBuf,
    exe: PathBuf,
    policy: PathBuf,
}
fn fixture(client: JsonMcpClient) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("client π");
    let source = dir.path().join("source 🌍");
    let state = dir.path().join("state");
    for p in [&root, &source, &state] {
        fs::create_dir(p).unwrap();
    }
    let exe = dir.path().join("owned native.exe");
    let policy = dir.path().join("owned policy.json");
    fs::write(&exe, b"owned fixture executable, never run").unwrap();
    fs::write(&policy, b"{}").unwrap();
    let selection = Selection::json_mcp(
        "json-owned",
        std::slice::from_ref(&root),
        &exe,
        &policy,
        &state,
        JsonMcpSource {
            root: &source,
            client,
        },
        &Deadline::new(30000),
    )
    .unwrap();
    Fixture {
        _dir: dir,
        selection,
        root,
        source,
        state,
        exe,
        policy,
    }
}
#[test]
fn all_json_profiles_bind_literal_launch_and_complete_owned_install_uninstall() {
    for (client, profile, file) in [
        (JsonMcpClient::Cursor, "cursor-mcp-v1", "mcp.json"),
        (
            JsonMcpClient::ClaudeCodeExplicit,
            "claude-explicit-mcp-v1",
            ".mcp.json",
        ),
        (
            JsonMcpClient::ClaudeDesktop,
            "claude-desktop-mcp-v1",
            "claude_desktop_config.json",
        ),
    ] {
        for original in [
            None,
            Some(b"".as_slice()),
            Some(b"{\r\n\"human\":true\r\n}"),
        ] {
            let f = fixture(client);
            let path = f.root.join(file);
            if let Some(bytes) = original {
                fs::write(&path, bytes).unwrap();
            }
            let d = Deadline::new(60000);
            let plan = host_install::plan_install(&f.selection, 1000, &d).unwrap();
            assert_eq!(plan.profile, profile);
            assert_eq!(plan.files[0].path, file);
            assert_eq!(f.selection.target_filename(), file);
            assert_eq!(fs::read_dir(&f.state).unwrap().count(), 0);
            assert!(!f.source.join(".yoke").exists());
            let authority = WriteAuthority::new(&f.selection);
            host_install::apply(&f.selection, &authority, &plan, 1001, &d).unwrap();
            let value: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            let entry = &value["mcpServers"]["yoke_json-owned"];
            assert_eq!(entry["command"], f.exe.to_str().unwrap());
            assert_eq!(
                entry["args"],
                json!([
                    "--root",
                    f.source,
                    "serve-session",
                    "--id",
                    "json-owned",
                    "--host-profile",
                    f.policy,
                    "--profile",
                    "native"
                ])
            );
            assert_eq!(entry["env"], json!({}));
            assert!(entry.get("autoApprove").is_none());
            assert!(!f.source.join(".yoke").exists());
            let remove = host_install::plan_uninstall(&f.selection, 1002, &d).unwrap();
            host_install::apply(&f.selection, &authority, &remove, 1003, &d).unwrap();
            if let Some(original) = original {
                assert_eq!(fs::read(&path).unwrap(), original);
            } else {
                assert!(!path.exists());
            }
        }
    }
}
#[test]
fn native_lifecycle_keeps_foreign_servers_and_rejects_unowned_entries() {
    let f = fixture(JsonMcpClient::Cursor);
    let path = f.root.join("mcp.json");
    let d = Deadline::new(60000);
    let foreign = b"{\"mcpServers\":{\"yoke_json-owned\":{\"command\":\"foreign\"}}}";
    fs::write(&path, foreign).unwrap();
    assert!(host_install::plan_install(&f.selection, 1000, &d).is_err());
    assert_eq!(fs::read(&path).unwrap(), foreign);
    assert_eq!(fs::read_dir(&f.state).unwrap().count(), 0);
    fs::write(&path, b"{\"human\":true}").unwrap();
    let plan = host_install::plan_install(&f.selection, 1000, &d).unwrap();
    let authority = WriteAuthority::new(&f.selection);
    host_install::apply(&f.selection, &authority, &plan, 1001, &d).unwrap();
    let text = fs::read_to_string(&path).unwrap();
    let at = text.rfind("}}").unwrap();
    let grown = format!(
        "{},\"foreign\":{{\"command\":\"foreign\"}}{}",
        &text[..at],
        &text[at..]
    );
    fs::write(&path, grown).unwrap();
    let remove = host_install::plan_uninstall(&f.selection, 1002, &d).unwrap();
    host_install::apply(&f.selection, &authority, &remove, 1003, &d).unwrap();
    let value: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(
        value,
        json!({"human":true,"mcpServers":{"foreign":{"command":"foreign"}}})
    );
}
#[test]
fn cross_profile_authority_stale_bindings_and_interpolation_cannot_write_config() {
    let f = fixture(JsonMcpClient::Cursor);
    let d = Deadline::new(60000);
    let plan = host_install::plan_install(&f.selection, 1000, &d).unwrap();
    let other = Selection::json_mcp(
        "json-owned",
        std::slice::from_ref(&f.root),
        &f.exe,
        &f.policy,
        &f.state,
        JsonMcpSource {
            root: &f.source,
            client: JsonMcpClient::ClaudeDesktop,
        },
        &d,
    )
    .unwrap();
    assert_ne!(f.selection.installation_id(), other.installation_id());
    assert!(
        host_install::apply(&f.selection, &WriteAuthority::new(&other), &plan, 1001, &d).is_err()
    );
    assert_eq!(fs::read_dir(&f.state).unwrap().count(), 0);
    fs::write(&f.exe, b"changed owned fixture executable").unwrap();
    assert!(
        host_install::apply(
            &f.selection,
            &WriteAuthority::new(&f.selection),
            &plan,
            1001,
            &d
        )
        .is_err()
    );
    assert!(!f.root.join("mcp.json").exists());
    let interpolation = f._dir.path().join("source ${UNSELECTED}");
    fs::create_dir(&interpolation).unwrap();
    assert!(
        Selection::json_mcp(
            "json-owned",
            std::slice::from_ref(&f.root),
            &f.exe,
            &f.policy,
            &f.state,
            JsonMcpSource {
                root: &interpolation,
                client: JsonMcpClient::Cursor
            },
            &d
        )
        .is_err()
    );
    assert!(!f.source.join(".yoke").exists());
}

#[test]
fn cli_json_selection_is_closed_and_planning_has_no_write_or_runtime_authority() {
    let f = fixture(JsonMcpClient::Cursor);
    let selected = f._dir.path().join("selection.json");
    let value = json!({"installation":"json-owned","targetRoots":[f.root],"executable":f.exe,
        "policy":f.policy,"stateRoot":f.state,"adapter":{"kind":"json_mcp_v1","client":"cursor","sourceRoot":f.source}});
    let invoke = || {
        use std::os::windows::process::CommandExt;
        std::process::Command::new(env!("CARGO_BIN_EXE_yoke"))
            .creation_flags(0x0800_0000)
            .current_dir(f._dir.path())
            .args(["host-install", "plan", "--selection"])
            .arg(&selected)
            .output()
            .unwrap()
    };
    fs::write(&selected, serde_json::to_vec(&value).unwrap()).unwrap();
    let output = invoke();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let plan: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(plan["profile"], "cursor-mcp-v1");
    assert!(!f.root.join("mcp.json").exists());
    assert_eq!(fs::read_dir(&f.state).unwrap().count(), 0);
    let mut unknown = value.clone();
    unknown["adapter"]["client"] = json!("automatic");
    let mut grant = value;
    grant["adapter"]["allowHostWrites"] = json!(true);
    for value in [unknown, grant] {
        fs::write(&selected, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(!invoke().status.success());
    }
    assert!(!f.source.join(".yoke").exists());
    assert!(!f.root.join("mcp.json").exists());
}
