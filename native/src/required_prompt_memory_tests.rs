use super::*;
use crate::session_prompt::{Config, memory::Revision};

fn fixture() -> (HostProfile, Snapshot) {
    let content = "owned e\u{301} 🦀\r\n{{ not_a_template }}".to_owned();
    let pin = Pin {
        name: "guide".into(),
        revision_id: "a".repeat(64),
        content_sha256: model::hash(&content),
        storage_workspace_id: "b".repeat(64),
        source_workspace_id: "c".repeat(64),
        snapshot_id: "d".repeat(64),
    };
    let revision = Revision {
        revision_id: pin.revision_id.clone(),
        scope: Scope::Project,
        storage_workspace_id: pin.storage_workspace_id.clone(),
        source_workspace_id: pin.source_workspace_id.clone(),
        snapshot_id: pin.snapshot_id.clone(),
        content_sha256: pin.content_sha256.clone(),
        anchors: vec![],
        trust: "authored_unverified".into(),
    };
    let mut host = HostProfile {
        prompt_templates: Some(Config {
            memory_embeddings: Some(vec!["guide".into(), "optional".into()]),
            ..Default::default()
        }),
        required_prompt_memories: Some(Policy {
            projects: BTreeMap::from([("default".into(), vec![pin])]),
            ..Default::default()
        }),
        ..Default::default()
    };
    host.required_prompt_memory_runtime =
        Some(Runtime::confirm(&host, &Deadline::new(10000)).unwrap());
    let snapshot = Snapshot {
        entries: BTreeMap::from([
            ("guide".into(), Entry::Available { content, revision }),
            ("optional".into(), Entry::Unavailable),
        ]),
    };
    (host, snapshot)
}

#[test]
fn required_notes_preserve_literal_original_bytes_and_do_not_promote_authored_claims() {
    let (host, snapshot) = fixture();
    let result = prepare(&host, "default", Some(&snapshot), &Deadline::new(10000))
        .unwrap()
        .unwrap();
    assert_eq!(result.text.matches("<required_memory ").count(), 1);
    assert!(
        result
            .text
            .contains("owned e\u{301} 🦀\r\n{{ not_a_template }}")
    );
    for key in [
        "humanIdentityAttested",
        "sourceAuthority",
        "overridesInstructionHierarchy",
    ] {
        assert_eq!(result.evidence[key], false);
    }
    assert!(!result.evidence.to_string().contains("not_a_template"));
    assert!(prepare(&host, "other", Some(&snapshot), &Deadline::new(10000)).is_err());
}

#[test]
fn missing_confirmation_and_rebound_policy_or_templates_never_admit_required_notes() {
    let (mut host, snapshot) = fixture();
    let runtime = host.required_prompt_memory_runtime.take().unwrap();
    assert!(prepare(&host, "default", Some(&snapshot), &Deadline::new(10000)).is_err());
    host.required_prompt_memory_runtime = Some(runtime);
    host.prompt_templates
        .as_mut()
        .unwrap()
        .context
        .variables
        .insert("changed".into(), json!(true));
    assert!(admit_host(&host, &Deadline::new(10000)).is_err());
    let (mut host, _) = fixture();
    host.required_prompt_memories
        .as_mut()
        .unwrap()
        .projects
        .get_mut("default")
        .unwrap()[0]
        .revision_id = "e".repeat(64);
    assert!(admit_host(&host, &Deadline::new(10000)).is_err());
}

#[test]
fn unavailable_and_changed_revision_or_workspace_pins_refuse_instead_of_partial_output() {
    for field in 0..6 {
        let (host, mut snapshot) = fixture();
        if field == 0 {
            snapshot.entries.insert("guide".into(), Entry::Unavailable);
        } else if let Entry::Available { revision, .. } = snapshot.entries.get_mut("guide").unwrap()
        {
            let selected = match field {
                1 => &mut revision.revision_id,
                2 => &mut revision.storage_workspace_id,
                3 => &mut revision.source_workspace_id,
                4 => &mut revision.snapshot_id,
                _ => &mut revision.content_sha256,
            };
            *selected = "e".repeat(64);
        }
        assert!(prepare(&host, "default", Some(&snapshot), &Deadline::new(10000)).is_err());
    }
}

#[test]
fn duplicate_foreign_scope_and_unselected_names_refuse_before_confirmation() {
    for mode in 0..4 {
        let (mut host, _) = fixture();
        let policy = host.required_prompt_memories.as_mut().unwrap();
        let pin = policy.projects["default"][0].clone();
        match mode {
            0 => policy.projects.get_mut("default").unwrap().push(pin),
            1 => policy.global.push(pin),
            2 => {
                policy.projects.insert("unregistered".into(), vec![pin]);
            }
            _ => policy.projects.get_mut("default").unwrap()[0].name = "not_selected".into(),
        }
        assert!(Runtime::confirm(&host, &Deadline::new(10000)).is_err());
    }
}

#[test]
fn required_output_and_inherited_deadline_cannot_be_replaced_by_optional_partial_results() {
    let (mut host, mut snapshot) = fixture();
    assert!(prepare(&host, "default", Some(&snapshot), &Deadline::new(0)).is_err());
    let Entry::Available { revision, .. } = snapshot.entries["guide"].clone() else {
        unreachable!()
    };
    let mut names = vec![];
    let mut pins = vec![];
    snapshot.entries.clear();
    for index in 0..10 {
        let name = format!("guide_{index}");
        let content = "x".repeat(15000);
        let mut revision = revision.clone();
        revision.content_sha256 = model::hash(&content);
        pins.push(Pin {
            name: name.clone(),
            revision_id: revision.revision_id.clone(),
            content_sha256: revision.content_sha256.clone(),
            storage_workspace_id: revision.storage_workspace_id.clone(),
            source_workspace_id: revision.source_workspace_id.clone(),
            snapshot_id: revision.snapshot_id.clone(),
        });
        names.push(name.clone());
        snapshot
            .entries
            .insert(name, Entry::Available { content, revision });
    }
    host.prompt_templates.as_mut().unwrap().memory_embeddings = Some(names);
    host.required_prompt_memories
        .as_mut()
        .unwrap()
        .projects
        .insert("default".into(), pins);
    host.required_prompt_memory_runtime =
        Some(Runtime::confirm(&host, &Deadline::new(10000)).unwrap());
    snapshot.validate(&Deadline::new(10000)).unwrap();
    assert!(
        prepare(&host, "default", Some(&snapshot), &Deadline::new(10000))
            .err()
            .unwrap()
            .to_string()
            .contains("output_capacity")
    );
}

#[test]
fn absent_policy_serialization_stays_unchanged_and_json_cannot_carry_confirmation() {
    let value = serde_json::to_value(HostProfile::default()).unwrap();
    assert!(value.get("requiredPromptMemories").is_none());
    assert!(value.get("requiredPromptMemoryRuntime").is_none());
    let (host, _) = fixture();
    let decoded: HostProfile = serde_json::from_value(serde_json::to_value(host).unwrap()).unwrap();
    assert!(decoded.required_prompt_memory_runtime.is_none());
    assert!(admit_host(&decoded, &Deadline::new(10000)).is_err());
}

#[test]
fn corrupt_content_cannot_reuse_a_confirmed_revision_hash() {
    let (host, mut snapshot) = fixture();
    if let Entry::Available { content, .. } = snapshot.entries.get_mut("guide").unwrap() {
        content.push_str("altered");
    }
    assert!(prepare(&host, "default", Some(&snapshot), &Deadline::new(10000)).is_err());
}

#[test]
fn revoking_one_confirmation_invalidates_every_clone_without_changing_policy_data() {
    let (host, snapshot) = fixture();
    let copy = host.clone();
    let before = serde_json::to_vec(&host).unwrap();
    copy.required_prompt_memory_runtime
        .as_ref()
        .unwrap()
        .revoke();
    assert_eq!(serde_json::to_vec(&host).unwrap(), before);
    assert!(prepare(&host, "default", Some(&snapshot), &Deadline::new(10000)).is_err());
    assert!(admit_host(&copy, &Deadline::new(10000)).is_err());
}
