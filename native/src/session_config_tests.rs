use super::*;
use crate::project_session::HostProject;
fn deadline() -> Deadline {
    Deadline::new(10000)
}
fn apply(base: &HostProfile, target: Target, bytes: &[u8]) -> Result<Projection> {
    let hash = model::hash(bytes);
    project(
        base,
        &[Source {
            target: &target,
            expected_sha256: &hash,
            bytes,
        }],
        &deadline(),
    )
}

#[test]
fn absent_and_empty_global_data_preserve_the_exact_existing_host_serialization() {
    let base = HostProfile::default();
    let original = serde_json::to_vec(&base).unwrap();
    assert_eq!(
        project(&base, &[], &deadline())
            .unwrap()
            .profile_bytes(&deadline())
            .unwrap(),
        original
    );
    for bytes in [
        b"".as_slice(),
        b"# owned comment\r\n",
        b"{}\r\n",
        b"---\r\n",
    ] {
        let p = apply(&base, Target::Global, bytes).unwrap();
        assert_eq!(p.profile_bytes(&deadline()).unwrap(), original);
        assert_eq!(p.plan()["sources"][0]["originalSha256"], model::hash(bytes));
        assert_eq!(p.plan()["sourceExecuted"], false);
    }
}

#[test]
fn bom_unicode_and_crlf_keep_original_hash_and_decoded_literal_prompt_bytes() {
    let bytes = "\u{feff}initial_prompt: |\r\n  Evidence \u{1f4a1} e\u{301}.\r\n".as_bytes();
    let p = apply(
        &HostProfile::default(),
        Target::Project {
            project: "default".into(),
        },
        bytes,
    )
    .unwrap();
    let s = p.profile().steering.as_ref().unwrap();
    assert_eq!(
        s.projects["default"].project.instructions,
        "Evidence \u{1f4a1} e\u{301}.\n"
    );
    assert_eq!(p.plan()["sources"][0]["originalBytes"], bytes.len());
    assert_eq!(p.plan()["sources"][0]["originalSha256"], model::hash(bytes));
    assert!(!p.plan()["templatesEvaluated"].as_bool().unwrap());
}

#[test]
fn all_explicit_source_roles_feed_existing_native_layer_precedence() {
    let base = HostProfile {
        context: "owned-context".into(),
        ..Default::default()
    };
    let input=[
        (Target::Global,b"base_modes: [interactive]\ndefault_modes: [planning]\nexcluded_tools: [read_file]\n".as_slice()),
        (Target::Project{project:"default".into()},b"added_modes: [owned-mode]\nincluded_optional_tools: [read_file]\n"),
        (Target::ProjectLocal{project:"default".into()},b"excluded_apis: [fs]\nincluded_apis: [fs.read_file]\n"),
        (Target::Context{name:"owned-context".into()},b"prompt: Owned context.\nexcluded_tools: [list_dir]\n"),
        (Target::Mode{name:"owned-mode".into()},b"prompt: Owned mode.\nread_only_memory_patterns: ['^private/']\nignored_memory_patterns: ['^hidden/']\n"),
    ];
    let hashes: Vec<_> = input.iter().map(|(_, b)| model::hash(b)).collect();
    let sources: Vec<_> = input
        .iter()
        .zip(&hashes)
        .map(|((target, bytes), hash)| Source {
            target,
            expected_sha256: hash,
            bytes,
        })
        .collect();
    let p = project(&base, &sources, &deadline()).unwrap();
    let host = p.profile();
    let resolved = session_steering::resolve(host, "default", None).unwrap();
    assert_eq!(
        resolved.modes,
        vec!["interactive", "planning", "owned-mode"]
    );
    assert!(resolved.allows("read_file"));
    assert!(!resolved.allows("list_dir"));
    assert!(resolved.api_allowed("fs.read_file"));
    assert!(!resolved.api_allowed("fs.list_dir"));
    let selected = p.plan();
    assert_eq!(selected["sources"].as_array().unwrap().len(), 5);
}

#[test]
fn yaml_read_only_only_tightens_primary_or_registered_named_projects() {
    let mut base = HostProfile::default();
    base.projects.push(HostProject {
        name: "owned".into(),
        root: std::env::current_dir().unwrap(),
        read_only: true,
        query_only: false,
    });
    let p = apply(
        &base,
        Target::Project {
            project: "owned".into(),
        },
        b"read_only: false\nproject_name: Declared display name\n",
    )
    .unwrap();
    assert!(p.profile().projects[0].read_only);
    assert_eq!(p.profile().projects[0].name, "owned");
    assert_eq!(
        p.plan()["sources"][0]["declaredProjectName"],
        "Declared display name"
    );
    base.projects[0].read_only = false;
    assert!(
        apply(
            &base,
            Target::ProjectLocal {
                project: "owned".into()
            },
            b"read_only: true\n"
        )
        .unwrap()
        .profile()
        .projects[0]
            .read_only
    );
    assert!(
        apply(
            &base,
            Target::Project {
                project: "unregistered".into()
            },
            b"read_only: true\n"
        )
        .is_err()
    );
    assert!(
        apply(
            &base,
            Target::Project {
                project: "default".into()
            },
            b"read_only: true\n"
        )
        .unwrap()
        .profile()
        .primary_read_only
    );
    assert!(
        apply(
            &base,
            Target::Global,
            b"projects: [{name: imported, root: C:/private}]\n"
        )
        .is_err()
    );
}

#[test]
fn yaml_duplicate_alias_merge_tag_document_type_and_encoding_ambiguities_refuse() {
    let target = Target::Global;
    let base = HostProfile::default();
    for bytes in [
        b"excluded_tools: []\nexcluded_tools: [read_file]\n".as_slice(),
        b"a: &a [read_file]\nexcluded_tools: *a\n",
        b"<<: {excluded_tools: [read_file]}\n",
        b"excluded_tools: !!seq []\n",
        b"{}\n---\n{}\n",
        b"? [complex, key]\n: value\n",
        b"excluded_tools: [42]\n",
        b"excluded_tools: .nan\n",
        b"[unterminated",
        b"agent_interface: true\n",
        b"\xff",
        b"prompt:\0 hidden",
    ] {
        assert!(
            apply(&base, target.clone(), bytes).is_err(),
            "An ambiguous owned input was accepted"
        );
    }
}

#[test]
fn stale_pins_duplicate_targets_unknown_capabilities_and_templates_refuse() {
    let target = Target::Global;
    let bytes = b"excluded_tools: [read_file]\n";
    let hash = model::hash(bytes);
    assert!(
        project(
            &HostProfile::default(),
            &[Source {
                target: &target,
                bytes,
                expected_sha256: &"0".repeat(64)
            }],
            &deadline()
        )
        .is_err()
    );
    assert!(
        project(
            &HostProfile::default(),
            &[
                Source {
                    target: &target,
                    bytes,
                    expected_sha256: &hash
                },
                Source {
                    target: &target,
                    bytes,
                    expected_sha256: &hash
                }
            ],
            &deadline()
        )
        .is_err()
    );
    for bytes in [
        b"activation_command: owned_private_canary\n".as_slice(),
        b"language_servers: [owned_private_canary]\n",
        b"trusted: true\n",
        b"python: C:/owned_private_canary\n",
    ] {
        let error = apply(&HostProfile::default(), target.clone(), bytes)
            .err()
            .unwrap();
        assert!(!format!("{error:?}").contains("owned_private_canary"));
    }
    for prompt in [
        b"initial_prompt: '{{ embed_memory(\"private\") }}'\n".as_slice(),
        b"initial_prompt: '{% if available_tools %}x{% endif %}'\n",
    ] {
        assert!(
            apply(
                &HostProfile::default(),
                Target::Project {
                    project: "default".into()
                },
                prompt
            )
            .is_err()
        );
    }
    assert!(
        apply(
            &HostProfile::default(),
            Target::Context {
                name: "agent".into()
            },
            b"prompt: Overwrite built-in.\n"
        )
        .is_err()
    );
}

#[test]
fn source_parser_profile_and_deadline_budgets_fail_explicitly() {
    let base = HostProfile::default();
    let target = Target::Global;
    let bytes = vec![b' '; MAX_FILE_BYTES + 1];
    assert!(apply(&base, target.clone(), &bytes).is_err());
    let empty = b"{}";
    let hash = model::hash(empty);
    let many: Vec<_> = (0..MAX_FILES + 1)
        .map(|_| Source {
            target: &target,
            bytes: empty,
            expected_sha256: &hash,
        })
        .collect();
    assert!(project(&base, &many, &deadline()).is_err());
    let deep = format!("excluded_tools: {}x{}\n", "[".repeat(20), "]".repeat(20));
    assert!(apply(&base, target.clone(), deep.as_bytes()).is_err());
    let nodes = format!("excluded_tools: [{}]\n", vec!["null"; 10000].join(","));
    assert!(nodes.len() < MAX_FILE_BYTES);
    assert!(apply(&base, target.clone(), nodes.as_bytes()).is_err());
    assert!(
        project(
            &base,
            &[Source {
                target: &target,
                bytes: empty,
                expected_sha256: &hash
            }],
            &Deadline::new(0)
        )
        .is_err()
    );
    let mut huge = base.clone();
    huge.global_notes_root = Some(std::path::PathBuf::from("x".repeat(MAX_PROFILE_BYTES)));
    assert!(project(&huge, &[], &deadline()).is_err());
}

#[test]
fn distinct_file_order_preserves_profile_semantics_and_receipt_occurrences() {
    let base = HostProfile::default();
    let global = Target::Global;
    let mode = Target::Mode {
        name: "review".into(),
    };
    let a = b"default_modes: [review]\n";
    let b = b"prompt: Owned ordered evidence.\n";
    let ha = model::hash(a);
    let hb = model::hash(b);
    let forward = project(
        &base,
        &[
            Source {
                target: &global,
                bytes: a,
                expected_sha256: &ha,
            },
            Source {
                target: &mode,
                bytes: b,
                expected_sha256: &hb,
            },
        ],
        &deadline(),
    )
    .unwrap();
    let reverse = project(
        &base,
        &[
            Source {
                target: &mode,
                bytes: b,
                expected_sha256: &hb,
            },
            Source {
                target: &global,
                bytes: a,
                expected_sha256: &ha,
            },
        ],
        &deadline(),
    )
    .unwrap();
    assert_eq!(
        forward.profile_bytes(&deadline()).unwrap(),
        reverse.profile_bytes(&deadline()).unwrap()
    );
    assert_eq!(forward.plan()["sources"][0]["target"]["kind"], "global");
    assert_eq!(reverse.plan()["sources"][0]["target"]["kind"], "mode");
}
