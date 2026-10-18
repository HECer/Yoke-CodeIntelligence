use std::{fs, path::PathBuf};
use tempfile::TempDir;
use yoke::{
    capture::Deadline,
    host_install::{self, Selection, WriteAuthority},
};

const NOW: u64 = 1_000_000;
struct Fixture {
    _dir: TempDir,
    roots: Vec<PathBuf>,
    state: PathBuf,
    exe: PathBuf,
    policy: PathBuf,
}
impl Fixture {
    fn new(count: usize) -> Self {
        let dir = TempDir::new().unwrap();
        let roots = (0..count)
            .map(|i| {
                let p = dir.path().join(format!("target-{i}"));
                fs::create_dir(&p).unwrap();
                p
            })
            .collect();
        let state = dir.path().join("private-state");
        fs::create_dir(&state).unwrap();
        let exe = dir.path().join("native-fixture.exe");
        fs::write(&exe, b"Owned fixture executable is never run\n").unwrap();
        let policy = dir.path().join("host-policy.json");
        fs::write(&policy, b"{\"projects\":[]}\n").unwrap();
        Self {
            _dir: dir,
            roots,
            state,
            exe,
            policy,
        }
    }
    fn selection(&self) -> Selection {
        Selection::new(
            "work",
            &self.roots,
            &self.exe,
            &self.policy,
            &self.state,
            &Deadline::new(10_000),
        )
        .unwrap()
    }
}
fn installed(f: &Fixture) -> (Selection, host_install::Receipt) {
    let selection = f.selection();
    let plan = host_install::plan_install(&selection, NOW, &Deadline::new(10_000)).unwrap();
    let receipt = host_install::apply(
        &selection,
        &WriteAuthority::new(&selection),
        &plan,
        NOW,
        &Deadline::new(10_000),
    )
    .unwrap();
    (selection, receipt)
}

#[cfg(windows)]
#[test]
fn internal_volume_guid_binding_launches_the_real_binary_and_is_not_a_user_path_grant() {
    let f = Fixture::new(1);
    let executable = PathBuf::from(env!("CARGO_BIN_EXE_yoke"));
    let d = Deadline::new(30_000);
    let selection = Selection::new(
        "real-binary",
        &f.roots,
        &executable,
        &f.policy,
        &f.state,
        &d,
    )
    .unwrap();
    let plan = host_install::plan_install(&selection, NOW, &d).unwrap();
    assert!(
        plan.bindings.executable.path.starts_with(r"\\?\Volume{"),
        "Executable authority did not retain a stable physical-volume path"
    );
    let output = std::process::Command::new(&plan.bindings.executable.path)
        .arg("--version")
        .current_dir(f._dir.path())
        .output()
        .expect("CreateProcess could not launch the actual native binary through its GUID path");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let version = String::from_utf8(output.stdout).unwrap();
    assert!(version.contains("yoke") && version.contains(env!("CARGO_PKG_VERSION")));
    let raw_guid_root = PathBuf::from(&plan.bindings.roots[0].path);
    assert!(
        raw_guid_root
            .as_os_str()
            .to_str()
            .unwrap()
            .starts_with(r"\\?\Volume{")
    );
    let denied = Selection::new(
        "raw-guid-user",
        &[raw_guid_root],
        &executable,
        &f.policy,
        &f.state,
        &d,
    );
    assert!(
        denied.is_err(),
        "Internal GUID representation became a user-supplied authority grant"
    );
    assert!(!f._dir.path().join(".yoke").exists());
    assert_eq!(fs::read_dir(&f.state).unwrap().count(), 0);
}

#[test]
fn dry_run_is_deterministic_and_creates_no_source_engine_or_host_state() {
    let f = Fixture::new(2);
    let original=b"\xef\xbb\xbf# Human instructions\r\nKeep \xe6\x96\x87\xe5\xad\x97 and secrets unrelated.\r\n";
    fs::write(f.roots[0].join("AGENTS.md"), original).unwrap();
    let selection = f.selection();
    let first = host_install::plan_install(&selection, NOW, &Deadline::new(10_000)).unwrap();
    let second = host_install::plan_install(&selection, NOW, &Deadline::new(10_000)).unwrap();
    assert_eq!(first, second);
    assert_eq!(first.files.len(), 2);
    assert_eq!(first.profile, "generic-agents-v1");
    assert_eq!(fs::read(f.roots[0].join("AGENTS.md")).unwrap(), original);
    assert!(!f.roots[1].join("AGENTS.md").exists());
    assert_eq!(fs::read_dir(&f.state).unwrap().count(), 0);
    assert!(f.roots.iter().all(|p| !p.join(".yoke").exists()));
    assert!(
        !serde_json::to_string(&first)
            .unwrap()
            .contains("secrets unrelated")
    );
}

#[test]
fn install_and_uninstall_preserve_human_bom_crlf_and_original_empty_presence() {
    let f = Fixture::new(3);
    let original = b"\xef\xbb\xbf# Human\r\nKeep all user content.\r\n";
    fs::write(f.roots[0].join("AGENTS.md"), original).unwrap();
    fs::write(f.roots[1].join("AGENTS.md"), b"").unwrap();
    let (selection, receipt) = installed(&f);
    assert_eq!(receipt.status, "installed");
    let installed_bytes = fs::read(f.roots[0].join("AGENTS.md")).unwrap();
    assert!(installed_bytes.starts_with(original));
    assert_eq!(
        host_install::status(&selection, &Deadline::new(10_000))
            .unwrap()
            .state,
        "installed"
    );
    let removal =
        host_install::plan_uninstall(&selection, NOW + 1, &Deadline::new(10_000)).unwrap();
    let result = host_install::apply(
        &selection,
        &WriteAuthority::new(&selection),
        &removal,
        NOW + 1,
        &Deadline::new(10_000),
    )
    .unwrap();
    assert_eq!(result.status, "uninstalled");
    assert_eq!(fs::read(f.roots[0].join("AGENTS.md")).unwrap(), original);
    assert!(f.roots[1].join("AGENTS.md").is_file());
    assert_eq!(fs::read(f.roots[1].join("AGENTS.md")).unwrap(), b"");
    assert!(!f.roots[2].join("AGENTS.md").exists());
}

#[test]
fn uninstall_preserves_foreign_appends_and_refuses_changes_inside_owned_block() {
    let f = Fixture::new(1);
    fs::write(
        f.roots[0].join("AGENTS.md"),
        b"Human prefix without newline",
    )
    .unwrap();
    let (selection, _) = installed(&f);
    let path = f.roots[0].join("AGENTS.md");
    let mut bytes = fs::read(&path).unwrap();
    bytes.extend_from_slice(b"\n# Human later\nKeep me.\n");
    fs::write(&path, &bytes).unwrap();
    let removal =
        host_install::plan_uninstall(&selection, NOW + 1, &Deadline::new(10_000)).unwrap();
    host_install::apply(
        &selection,
        &WriteAuthority::new(&selection),
        &removal,
        NOW + 1,
        &Deadline::new(10_000),
    )
    .unwrap();
    assert_eq!(
        fs::read(&path).unwrap(),
        b"Human prefix without newline\n# Human later\nKeep me.\n"
    );
    let next = host_install::plan_install(&selection, NOW + 2, &Deadline::new(10_000)).unwrap();
    host_install::apply(
        &selection,
        &WriteAuthority::new(&selection),
        &next,
        NOW + 2,
        &Deadline::new(10_000),
    )
    .unwrap();
    let prior = fs::read_to_string(&path).unwrap();
    let changed = prior.replace("generic-agents-v1", "foreign-modification");
    fs::write(&path, &changed).unwrap();
    assert!(host_install::plan_uninstall(&selection, NOW + 3, &Deadline::new(10_000)).is_err());
    assert_eq!(fs::read_to_string(path).unwrap(), changed);
}

#[test]
fn every_target_and_host_binding_is_checked_before_first_write() {
    for mutation in ["target", "executable", "policy", "expired", "authority"] {
        let f = Fixture::new(2);
        fs::write(f.roots[0].join("AGENTS.md"), b"first human\n").unwrap();
        fs::write(f.roots[1].join("AGENTS.md"), b"second human\n").unwrap();
        let selection = f.selection();
        let plan = host_install::plan_install(&selection, NOW, &Deadline::new(10_000)).unwrap();
        let mut now = NOW;
        match mutation {
            "target" => fs::write(f.roots[1].join("AGENTS.md"), b"foreign second\n").unwrap(),
            "executable" => fs::write(&f.exe, b"replaced executable").unwrap(),
            "policy" => fs::write(&f.policy, b"changed policy").unwrap(),
            "expired" => now = plan.expires_ms + 1,
            _ => {}
        }
        let other = Fixture::new(1);
        let wrong = other.selection();
        let authority = if mutation == "authority" {
            WriteAuthority::new(&wrong)
        } else {
            WriteAuthority::new(&selection)
        };
        assert!(
            host_install::apply(&selection, &authority, &plan, now, &Deadline::new(10_000))
                .is_err(),
            "{mutation}"
        );
        assert_eq!(
            fs::read(f.roots[0].join("AGENTS.md")).unwrap(),
            b"first human\n"
        );
    }
}

#[test]
fn receipt_ownership_is_required_and_serialized_plans_cannot_widen_paths() {
    let f = Fixture::new(1);
    let path = f.roots[0].join("AGENTS.md");
    fs::write(&path,b"<!-- yoke:managed:generic-agents-v1:start -->\nUnowned block\n<!-- yoke:managed:generic-agents-v1:end -->\n").unwrap();
    assert!(host_install::plan_install(&f.selection(), NOW, &Deadline::new(10_000)).is_err());
    assert!(host_install::plan_uninstall(&f.selection(), NOW, &Deadline::new(10_000)).is_err());
    fs::write(&path, b"Human content\n").unwrap();
    let selection = f.selection();
    let mut plan = host_install::plan_install(&selection, NOW, &Deadline::new(10_000)).unwrap();
    plan.files[0].path = "../outside.md".into();
    assert!(
        host_install::apply(
            &selection,
            &WriteAuthority::new(&selection),
            &plan,
            NOW,
            &Deadline::new(10_000)
        )
        .is_err()
    );
    assert_eq!(fs::read(path).unwrap(), b"Human content\n");
}

#[test]
fn private_state_never_contains_unrelated_human_config_or_policy_contents() {
    let f = Fixture::new(1);
    let human = "human-only-sensitive-sentinel\n";
    let policy = "{\"fixturePrivateValue\":\"policy-only-sensitive-sentinel\"}\n";
    fs::write(f.roots[0].join("AGENTS.md"), human).unwrap();
    fs::write(&f.policy, policy).unwrap();
    let (_selection, _) = installed(&f);
    let mut pending = vec![f.state.clone()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else {
                let bytes = fs::read(path).unwrap();
                let text = String::from_utf8_lossy(&bytes);
                assert!(!text.contains("human-only-sensitive-sentinel"));
                assert!(!text.contains("policy-only-sensitive-sentinel"));
            }
        }
    }
}

#[test]
fn state_and_target_aliases_bounds_invalid_utf8_and_deadlines_fail_closed() {
    let f = Fixture::new(1);
    assert!(
        Selection::new(
            "work",
            &f.roots,
            &f.exe,
            &f.policy,
            &f.roots[0],
            &Deadline::new(10_000)
        )
        .is_err()
    );
    assert!(
        Selection::new(
            "work",
            &[f.roots[0].clone(), f.roots[0].clone()],
            &f.exe,
            &f.policy,
            &f.state,
            &Deadline::new(10_000)
        )
        .is_err()
    );
    assert!(
        Selection::new(
            "../escape",
            &f.roots,
            &f.exe,
            &f.policy,
            &f.state,
            &Deadline::new(10_000)
        )
        .is_err()
    );
    assert!(
        Selection::new(
            "work",
            &f.roots,
            &f.exe,
            &f.policy,
            &f.state,
            &Deadline::new(0)
        )
        .is_err()
    );
    fs::write(f.roots[0].join("AGENTS.md"), [0xff, 0xfe]).unwrap();
    assert!(host_install::plan_install(&f.selection(), NOW, &Deadline::new(10_000)).is_err());
    fs::write(f.roots[0].join("AGENTS.md"), vec![b'x'; 1_048_577]).unwrap();
    assert!(host_install::plan_install(&f.selection(), NOW, &Deadline::new(10_000)).is_err());
}

#[cfg(windows)]
#[test]
fn windows_existing_writer_and_hardlink_alias_are_refused_before_other_targets_change() {
    use std::os::windows::fs::OpenOptionsExt;
    let f = Fixture::new(2);
    let first = f.roots[0].join("AGENTS.md");
    let second = f.roots[1].join("AGENTS.md");
    fs::write(&first, b"first\n").unwrap();
    fs::write(&second, b"second\n").unwrap();
    let selection = f.selection();
    let plan = host_install::plan_install(&selection, NOW, &Deadline::new(10_000)).unwrap();
    let held = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&second)
        .unwrap();
    assert!(
        host_install::apply(
            &selection,
            &WriteAuthority::new(&selection),
            &plan,
            NOW,
            &Deadline::new(10_000)
        )
        .is_err()
    );
    assert_eq!(fs::read(&first).unwrap(), b"first\n");
    drop(held);
    fs::hard_link(&first, f._dir.path().join("foreign-alias.md")).unwrap();
    assert!(host_install::plan_install(&selection, NOW, &Deadline::new(10_000)).is_err());
}
