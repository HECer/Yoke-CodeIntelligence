use super::*;
use serde_json::json;

fn project(
    base: &HostProfile,
    sources: &[(crate::session_config::Target, &[u8])],
) -> crate::session_config::Projection {
    let hashes = sources
        .iter()
        .map(|(_, b)| model::hash(b))
        .collect::<Vec<_>>();
    let inputs = sources
        .iter()
        .zip(&hashes)
        .map(|((target, bytes), hash)| crate::session_config::Source {
            target,
            bytes,
            expected_sha256: hash,
        })
        .collect::<Vec<_>>();
    crate::session_config::project(base, &inputs, &Deadline::new(10000)).unwrap()
}
#[test]
fn positive_fractional_seconds_are_bounded_numbers_not_strings_or_renewed_budgets() {
    assert_eq!(timeout(&json!(0.125)).unwrap(), Duration::from_millis(125));
    assert_eq!(timeout(&json!(180)).unwrap(), Duration::from_secs(180));
    assert_eq!(timeout(&json!(180.0)).unwrap(), Duration::from_secs(180));
    for value in [
        json!(0),
        json!(-1),
        json!(false),
        json!("0.5"),
        json!([]),
        json!({}),
        json!(1e100),
        json!(1e-100),
        json!(0.0000000015),
        json!(1.0000000005),
        json!(u64::MAX),
    ] {
        assert!(timeout(&value).is_err(), "{value}");
    }
}
#[test]
fn command_utf8_capacity_nul_and_all_whitespace_are_checked_before_authorization() {
    assert!(command(&json!("λ".repeat(4096))).is_ok());
    for text in [
        "λ".repeat(4097),
        "x".repeat(MAX_COMMAND_BYTES + 1),
        "x\0canary".into(),
        " \t\n\u{2003}".into(),
    ] {
        let error = command(&json!(text)).err().unwrap();
        assert!(!error.to_string().contains("canary"));
    }
}
#[test]
fn inert_yaml_layers_resolve_in_fixed_order_independent_of_input_order_and_modes() {
    use crate::session_config::Target;
    let h = HostProfile::default();
    let p = project(
        &h,
        &[
            (
                Target::ProjectLocal {
                    project: "default".into(),
                },
                b"activation_command: local-exact\nactivation_command_timeout: 0.125\n",
            ),
            (
                Target::Global,
                b"activation_command: global-exact\nactivation_command_timeout: 10\n",
            ),
            (
                Target::Project {
                    project: "default".into(),
                },
                b"activation_command: project-exact\nactivation_command_timeout: 2\n",
            ),
        ],
    );
    let c = p.profile().activation_commands.as_ref().unwrap();
    assert_eq!(c.resolve("default").unwrap().command, Some("local-exact"));
    assert_eq!(
        c.resolve("default").unwrap().timeout,
        Duration::from_millis(125)
    );
    assert_eq!(c.resolve("other").unwrap().command, Some("global-exact"));
    assert!(p.profile().activation_runtime.is_none());
    let plan = p.plan().to_string();
    for raw in ["local-exact", "global-exact", "project-exact"] {
        assert!(!plan.contains(raw));
    }
    assert_eq!(p.plan()["activationCommandsExecuted"], false);
    let cleared = project(
        p.profile(),
        &[(
            Target::ProjectLocal {
                project: "default".into(),
            },
            b"activation_command: null\nactivation_command_timeout: null\n",
        )],
    );
    assert!(
        cleared
            .profile()
            .activation_commands
            .as_ref()
            .unwrap()
            .resolve("default")
            .unwrap()
            .command
            .is_none()
    );
    assert_eq!(
        cleared
            .profile()
            .activation_commands
            .as_ref()
            .unwrap()
            .resolve("default")
            .unwrap()
            .timeout,
        Duration::from_secs(2)
    );
    for target in [
        Target::Mode {
            name: "editing".into(),
        },
        Target::Context {
            name: "agent".into(),
        },
    ] {
        let bytes = b"activation_command: never\n";
        assert!(
            crate::session_config::project(
                &h,
                &[crate::session_config::Source {
                    target: &target,
                    expected_sha256: &model::hash(bytes),
                    bytes
                }],
                &Deadline::new(10000)
            )
            .is_err()
        );
    }
}
#[test]
fn original_absent_null_and_integer_default_profile_bytes_remain_exact() {
    use crate::session_config::Target;
    let h = HostProfile::default();
    let original = serde_json::to_vec(&h).unwrap();
    for bytes in [
        b"{}\n".as_slice(),
        b"activation_command: null\nactivation_command_timeout: null\n",
        b"activation_command: null\nactivation_command_timeout: 180\n",
    ] {
        let p = project(
            &h,
            &[(
                Target::Project {
                    project: "default".into(),
                },
                bytes,
            )],
        );
        assert_eq!(p.profile_bytes(&Deadline::new(10000)).unwrap(), original);
        assert!(p.profile().activation_commands.is_none());
        assert!(p.plan().get("hostProfileRedactions").is_none());
    }
}
#[test]
fn clearing_a_preexisting_last_command_cannot_resurrect_the_base_declaration() {
    use crate::session_config::Target;
    let first = project(
        &HostProfile::default(),
        &[(Target::Global, b"activation_command: previous-private\n")],
    );
    let cleared = project(
        first.profile(),
        &[(
            Target::Global,
            b"activation_command: null\nactivation_command_timeout: 180\n",
        )],
    );
    assert!(cleared.profile().activation_commands.is_none());
    assert_eq!(
        cleared.profile_bytes(&Deadline::new(10000)).unwrap(),
        serde_json::to_vec(&HostProfile::default()).unwrap()
    );
}
#[test]
fn malformed_yaml_numbers_and_forged_runtime_authority_refuse_without_echoing_canaries() {
    use crate::session_config::{Source, Target};
    for bytes in [
        b"activation_command_timeout: .nan\n".as_slice(),
        b"activation_command_timeout: .inf\n",
        b"activation_command_timeout: 1e100\n",
        b"activation_command_timeout: 1e-999\n",
        b"activation_command_timeout: 0\n",
        b"activation_command_timeout: '0.125'\n",
        b"activation_command: []\n",
    ] {
        assert!(
            crate::session_config::project(
                &HostProfile::default(),
                &[Source {
                    target: &Target::Global,
                    expected_sha256: &model::hash(bytes),
                    bytes
                }],
                &Deadline::new(10000)
            )
            .is_err()
        );
    }
    for field in [
        "activationRuntime",
        "allowProjectActivation",
        "trustedProjectPathPatterns",
        "activationAuthority",
    ] {
        let mut value = json!({});
        value[field] = json!({"command":"private-canary","trusted":true});
        let error = crate::json::decode::<HostProfile>(value).err().unwrap();
        assert!(!error.to_string().contains("private-canary"));
    }
}
