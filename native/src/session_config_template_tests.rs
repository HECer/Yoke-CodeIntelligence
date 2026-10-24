use super::*;
#[test]
fn host_renderer_receipts_bind_original_source_and_only_replace_prompt_data() {
    let d = Deadline::new(10000);
    let base = HostProfile {
        primary_read_only: true,
        ..Default::default()
    };
    let bytes = b"prompt: '{{ value }}'\n";
    let hash = model::hash(bytes);
    let target = Target::Mode {
        name: "owned".into(),
    };
    let mut calls = 0;
    let mut renderer = |text: &str, _: &Deadline| {
        calls += 1;
        assert_eq!(text, "{{ value }}");
        Ok(RenderedPrompt {
            text: "{{ literal }}\nread_only: false".into(),
            evidence: json!({"host":"explicit"}),
        })
    };
    let p = project_with_prompt_renderer(
        &base,
        &[Source {
            target: &target,
            expected_sha256: &hash,
            bytes,
        }],
        &d,
        &mut renderer,
    )
    .unwrap();
    assert_eq!(calls, 1);
    assert!(p.profile().primary_read_only);
    let v = p.plan();
    assert_eq!(v["templatesEvaluated"], true);
    assert_eq!(v["sources"][0]["originalSha256"], hash);
    assert_eq!(v["sources"][0]["originalBytes"], bytes.len());
    assert_eq!(
        v["hostProfile"]["steering"]["modes"]["owned"]["instructions"],
        "{{ literal }}\nread_only: false"
    );
    assert_eq!(
        v["sources"][0]["promptRendering"]["originalPromptSha256"],
        model::hash("{{ value }}")
    );
    assert!(
        project(
            &base,
            &[Source {
                target: &target,
                expected_sha256: &hash,
                bytes
            }],
            &d
        )
        .is_err()
    );
}
#[test]
fn invalid_policy_precedes_host_renderer_and_invalid_rendered_text_cannot_project() {
    let d = Deadline::new(10000);
    let base = HostProfile::default();
    let target = Target::Mode {
        name: "owned".into(),
    };
    let bytes = b"activation_command: forbidden\nprompt: '{{ value }}'\n";
    let hash = model::hash(bytes);
    let mut never = |_: &str, _: &Deadline| -> Result<RenderedPrompt> {
        panic!("Invalid policy reached renderer")
    };
    let e = project_with_prompt_renderer(
        &base,
        &[Source {
            target: &target,
            expected_sha256: &hash,
            bytes,
        }],
        &d,
        &mut never,
    )
    .err()
    .unwrap();
    assert!(e.to_string().contains("session_config_setting_unsupported"));
    let bytes = b"prompt: '{{ value }}'\n";
    let hash = model::hash(bytes);
    for text in ["x".repeat(8193), "bad\0text".into()] {
        let mut render = |_: &str, _: &Deadline| {
            Ok(RenderedPrompt {
                text: text.clone(),
                evidence: json!({}),
            })
        };
        assert!(
            project_with_prompt_renderer(
                &base,
                &[Source {
                    target: &target,
                    expected_sha256: &hash,
                    bytes
                }],
                &d,
                &mut render
            )
            .is_err()
        );
    }
}
