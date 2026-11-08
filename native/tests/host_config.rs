use yoke::{
    capture::Deadline,
    host_config::{self, ByteEdit, Format, OwnedFragment, Patch},
    model::hash,
};

fn d() -> Deadline {
    Deadline::new(5_000)
}
fn apply(input: &[u8], patch: &Patch) -> Vec<u8> {
    host_config::apply_patch(input, patch, &d()).unwrap()
}

#[test]
fn agents_install_update_remove_preserve_bom_unicode_and_crlf_bytes() {
    let original = b"\xef\xbb\xbf# Human\r\nKeep \xc3\xbc exactly.";
    let first = host_config::agents_patch(
        original,
        Some(b"Use the local tool.\nRead source first.\n"),
        None,
        &d(),
    )
    .unwrap();
    let written = apply(original, &first);
    assert_eq!(written, b"\xef\xbb\xbf# Human\r\nKeep \xc3\xbc exactly.\r\n<!-- yoke:managed:generic-agents-v1:start -->\r\nUse the local tool.\r\nRead source first.\r\n<!-- yoke:managed:generic-agents-v1:end -->\r\n");
    assert_eq!(first.before_hash, hash(original));
    assert_eq!(first.after_hash, hash(&written));
    let updated = host_config::agents_patch(
        &written,
        Some(b"Updated.\n"),
        first.owned_fragment.as_ref(),
        &d(),
    )
    .unwrap();
    let bytes = apply(&written, &updated);
    assert_eq!(bytes, b"\xef\xbb\xbf# Human\r\nKeep \xc3\xbc exactly.\r\n<!-- yoke:managed:generic-agents-v1:start -->\r\nUpdated.\r\n<!-- yoke:managed:generic-agents-v1:end -->\r\n");
    let remove =
        host_config::agents_patch(&bytes, None, updated.owned_fragment.as_ref(), &d()).unwrap();
    assert_eq!(apply(&bytes, &remove), original);
    assert!(remove.owned_fragment.is_none());
}

#[test]
fn agents_removal_preserves_later_human_prefix_and_append() {
    let install = host_config::agents_patch(b"Human\n", Some(b"Owned"), None, &d()).unwrap();
    let installed = apply(b"Human\n", &install);
    let mut changed = b"New human header\n".to_vec();
    changed.extend_from_slice(&installed);
    changed.extend_from_slice(b"Human appendix \xf0\x9f\x8c\xbb\n");
    let remove =
        host_config::agents_patch(&changed, None, install.owned_fragment.as_ref(), &d()).unwrap();
    assert_eq!(
        apply(&changed, &remove),
        b"New human header\nHuman\nHuman appendix \xf0\x9f\x8c\xbb\n"
    );
}

#[test]
fn agents_empty_existing_file_round_trips_without_phantom_bytes() {
    for original in [b"".as_slice(), b"\xef\xbb\xbf", b"Human\n", b"Human\r\n"] {
        let create = host_config::agents_patch(original, Some(b"Owned"), None, &d()).unwrap();
        let written = apply(original, &create);
        let remove =
            host_config::agents_patch(&written, None, create.owned_fragment.as_ref(), &d())
                .unwrap();
        assert_eq!(apply(&written, &remove), original);
    }
}

#[test]
fn agents_marker_presence_never_grants_ownership() {
    let forged = b"<!-- yoke:managed:generic-agents-v1:start -->\nHuman content\n<!-- yoke:managed:generic-agents-v1:end -->\n";
    assert!(host_config::agents_patch(forged, Some(b"Replace"), None, &d()).is_err());
    assert!(host_config::agents_patch(forged, None, None, &d()).is_err());
    assert!(host_config::agents_patch(b"Human", Some(forged), None, &d()).is_err());
}

#[test]
fn agents_reject_changed_receipts_owned_bytes_duplicate_and_nested_markers() {
    let install = host_config::agents_patch(b"Human\n", Some(b"Owned"), None, &d()).unwrap();
    let bytes = apply(b"Human\n", &install);
    let modified = String::from_utf8(bytes.clone())
        .unwrap()
        .replace("Owned", "Human edited owned block");
    assert!(
        host_config::agents_patch(
            modified.as_bytes(),
            None,
            install.owned_fragment.as_ref(),
            &d()
        )
        .is_err()
    );
    let mut forged = install.owned_fragment.clone().unwrap();
    forged.hash = "0".repeat(64);
    assert!(host_config::agents_patch(&bytes, None, Some(&forged), &d()).is_err());
    forged = install.owned_fragment.clone().unwrap();
    forged.selector = "another-owner".into();
    assert!(host_config::agents_patch(&bytes, None, Some(&forged), &d()).is_err());
    let mut duplicate = bytes.clone();
    duplicate.extend_from_slice(&bytes);
    assert!(
        host_config::agents_patch(&duplicate, None, install.owned_fragment.as_ref(), &d()).is_err()
    );
    let nested = String::from_utf8(bytes.clone()).unwrap().replace(
        "Owned",
        "<!-- yoke:managed:generic-agents-v1:start -->\nOwned",
    );
    assert!(
        host_config::agents_patch(
            nested.as_bytes(),
            None,
            install.owned_fragment.as_ref(),
            &d()
        )
        .is_err()
    );
    let inline = b"Human <!-- yoke:managed:generic-agents-v1:start --> hidden";
    assert!(host_config::agents_patch(inline, Some(b"Owned"), None, &d()).is_err());
}

#[test]
fn patch_application_checks_entire_input_fragments_ranges_and_output_hash() {
    let install = host_config::agents_patch(b"Human\n", Some(b"Owned"), None, &d()).unwrap();
    assert!(host_config::apply_patch(b"Human changed\n", &install, &d()).is_err());
    let mut altered = install.clone();
    altered.after_hash = "0".repeat(64);
    assert!(host_config::apply_patch(b"Human\n", &altered, &d()).is_err());
    altered = install.clone();
    altered.edits[0].start_byte = usize::MAX;
    assert!(host_config::apply_patch(b"Human\n", &altered, &d()).is_err());
    altered = install.clone();
    altered.edits[0].expected = b"Not present".to_vec();
    assert!(host_config::apply_patch(b"Human\n", &altered, &d()).is_err());
    altered = install.clone();
    altered.edits.push(altered.edits[0].clone());
    assert!(host_config::apply_patch(b"Human\n", &altered, &d()).is_err());
}

#[test]
fn agents_and_patch_work_are_bounded_before_output() {
    assert!(host_config::agents_patch(&vec![b'x'; 1_048_577], Some(b"Owned"), None, &d()).is_err());
    assert!(host_config::agents_patch(b"Human", Some(&vec![b'x'; 65_537]), None, &d()).is_err());
    assert!(host_config::agents_patch(b"Human", Some(b"Bad\0body"), None, &d()).is_err());
    assert!(host_config::agents_patch(b"\xff", Some(b"Owned"), None, &d()).is_err());
    assert!(host_config::agents_patch(b"Human", Some(b"Owned"), None, &Deadline::new(0)).is_err());
    let patch = Patch {
        edits: vec![ByteEdit {
            start_byte: 0,
            end_byte: 0,
            expected: vec![],
            replacement: vec![],
        }],
        before_hash: hash(b""),
        after_hash: hash(b""),
        owned_fragment: None,
    };
    assert!(host_config::apply_patch(b"", &patch, &Deadline::new(0)).is_err());
}

#[test]
fn receipts_are_strict_serialized_data_and_do_not_include_human_prefix() {
    let patch =
        host_config::agents_patch(b"Private human notes\n", Some(b"Owned"), None, &d()).unwrap();
    let encoded = serde_json::to_string(&patch).unwrap();
    assert!(!encoded.contains("Private human notes"));
    let decoded: Patch = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded, patch);
    let unknown = encoded.replacen('{', "{\"grantWrite\":true,", 1);
    assert!(serde_json::from_str::<Patch>(&unknown).is_err());
    let owned = patch.owned_fragment.as_ref().unwrap();
    assert_eq!(owned.format, Format::AgentsMarkdown);
    assert_eq!(owned.selector, "generic-agents-v1");
    let _: OwnedFragment = serde_json::from_str(&serde_json::to_string(owned).unwrap()).unwrap();
}

#[test]
fn jsonc_named_entry_preserves_comments_bom_crlf_and_literal_unowned_bytes() {
    let original = b"\xef\xbb\xbf{\r\n // Human notes\r\n \"mcpServers\": {\"human\": {\"command\": \"human.exe\"}},\r\n \"keep\": 7,\r\n}\r\n";
    let parent = vec!["mcpServers".to_owned()];
    let create = host_config::jsonc_patch(
        original,
        &parent,
        "yoke",
        Some(br#"{"command":"yoke.exe","args":["serve"]}"#),
        None,
        &d(),
    )
    .unwrap();
    let written = apply(original, &create);
    assert_eq!(written, b"\xef\xbb\xbf{\r\n // Human notes\r\n \"mcpServers\": {\"human\": {\"command\": \"human.exe\"},\"yoke\":{\"command\":\"yoke.exe\",\"args\":[\"serve\"]}},\r\n \"keep\": 7,\r\n}\r\n");
    let update = host_config::jsonc_patch(
        &written,
        &parent,
        "yoke",
        Some(br#"{"command":"new.exe"}"#),
        create.owned_fragment.as_ref(),
        &d(),
    )
    .unwrap();
    let updated = apply(&written, &update);
    assert!(
        String::from_utf8_lossy(&updated)
            .contains("\"human\": {\"command\": \"human.exe\"},\"yoke\":{\"command\":\"new.exe\"}")
    );
    let remove = host_config::jsonc_patch(
        &updated,
        &parent,
        "yoke",
        None,
        update.owned_fragment.as_ref(),
        &d(),
    )
    .unwrap();
    assert_eq!(apply(&updated, &remove), original);
}

#[test]
fn jsonc_empty_and_trailing_comma_parents_restore_original_punctuation() {
    let parent = vec!["mcpServers".to_owned()];
    for original in [
        br#"{"mcpServers":{ /*human*/ }}"#.as_slice(),
        br#"{"mcpServers":{"human":{}, /*human*/ }}"#,
    ] {
        let create =
            host_config::jsonc_patch(original, &parent, "yoke", Some(b"{}"), None, &d()).unwrap();
        let written = apply(original, &create);
        let remove = host_config::jsonc_patch(
            &written,
            &parent,
            "yoke",
            None,
            create.owned_fragment.as_ref(),
            &d(),
        )
        .unwrap();
        assert_eq!(apply(&written, &remove), original);
    }
}

#[test]
fn jsonc_decoded_duplicate_keys_unowned_entries_and_ambiguous_parents_fail() {
    let parent = vec!["mcpServers".to_owned()];
    for bytes in [
        br#"{"mcpServers":{"yoke":{}}}"#.as_slice(),
        br#"{"mcpServers":{"yoke":{},"yo\u006be":{}}}"#,
        br#"{"mcpServers":{},"mcpServ\u0065rs":{}}"#,
        br#"{"human":{"x":1,"\u0078":2},"mcpServers":{}}"#,
        br#"{"mcpServers":[]}"#,
        br#"{"mcpServers":{},"bad":"unterminated}"#,
        br#"{"mcpServers":{} /*unterminated}"#,
    ] {
        assert!(host_config::jsonc_patch(bytes, &parent, "yoke", Some(b"{}"), None, &d()).is_err());
    }
    for body in [b"[]".as_slice(), br#"{"x":1,"\u0078":2}"#, b"{} trailing"] {
        assert!(
            host_config::jsonc_patch(
                br#"{"mcpServers":{}}"#,
                &parent,
                "yoke",
                Some(body),
                None,
                &d()
            )
            .is_err()
        );
    }
}

#[test]
fn jsonc_human_append_is_preserved_or_rejected_when_unrecorded_punctuation_would_be_removed() {
    let parent = vec!["mcpServers".to_owned()];
    let original = br#"{"mcpServers":{"human":{}}}"#;
    let create =
        host_config::jsonc_patch(original, &parent, "yoke", Some(b"{}"), None, &d()).unwrap();
    let written = apply(original, &create);
    let appended = String::from_utf8(written)
        .unwrap()
        .replace("\"yoke\":{}", "\"yoke\":{},\"later\":{}");
    let remove = host_config::jsonc_patch(
        appended.as_bytes(),
        &parent,
        "yoke",
        None,
        create.owned_fragment.as_ref(),
        &d(),
    )
    .unwrap();
    assert_eq!(
        apply(appended.as_bytes(), &remove),
        br#"{"mcpServers":{"human":{},"later":{}}}"#
    );
    let empty = br#"{"mcpServers":{}}"#;
    let create = host_config::jsonc_patch(empty, &parent, "yoke", Some(b"{}"), None, &d()).unwrap();
    let written = apply(empty, &create);
    let appended = String::from_utf8(written)
        .unwrap()
        .replace("\"yoke\":{}", "\"yoke\":{},\"later\":{}");
    assert!(
        host_config::jsonc_patch(
            appended.as_bytes(),
            &parent,
            "yoke",
            None,
            create.owned_fragment.as_ref(),
            &d()
        )
        .is_err()
    );
}

#[test]
fn toml_managed_table_preserves_human_comments_and_round_trips_crlf() {
    let original = b"\xef\xbb\xbf# Human\r\nname = 'untouched'\r\n\r\n[human]\r\nvalue = 7";
    let path = vec!["mcp_servers".to_owned(), "yoke".to_owned()];
    let create = host_config::toml_patch(
        original,
        &path,
        Some(b"command = 'yoke.exe'\nargs = ['serve']\n"),
        None,
        &d(),
    )
    .unwrap();
    let written = apply(original, &create);
    assert_eq!(written, b"\xef\xbb\xbf# Human\r\nname = 'untouched'\r\n\r\n[human]\r\nvalue = 7\r\n[mcp_servers.yoke]\r\ncommand = 'yoke.exe'\r\nargs = ['serve']\r\n");
    let remove =
        host_config::toml_patch(&written, &path, None, create.owned_fragment.as_ref(), &d())
            .unwrap();
    assert_eq!(apply(&written, &remove), original);
}

#[test]
fn toml_table_removal_never_reparents_later_human_keys() {
    let path = vec!["mcp_servers".to_owned(), "yoke".to_owned()];
    let create = host_config::toml_patch(
        b"name = 'human'\n",
        &path,
        Some(b"command = 'yoke.exe'\n"),
        None,
        &d(),
    )
    .unwrap();
    let written = apply(b"name = 'human'\n", &create);
    let mut appended = written.clone();
    appended.extend_from_slice(b"human_added = 'keep under yoke'\n");
    assert!(
        host_config::toml_patch(&appended, &path, None, create.owned_fragment.as_ref(), &d())
            .is_err()
    );
    let mut headed = written;
    headed.extend_from_slice(b"# human notes\n[later]\nhuman_added = true\n");
    let remove =
        host_config::toml_patch(&headed, &path, None, create.owned_fragment.as_ref(), &d())
            .unwrap();
    assert_eq!(
        apply(&headed, &remove),
        b"name = 'human'\n# human notes\n[later]\nhuman_added = true\n"
    );
}

#[test]
fn toml_inverse_refuses_to_join_an_unterminated_human_comment_with_a_later_table_header() {
    let original = b"# human";
    let path = vec!["mcp_servers".to_owned(), "yoke".to_owned()];
    let create = host_config::toml_patch(original, &path, Some(b"x=1\n"), None, &d()).unwrap();
    let mut changed = apply(original, &create);
    changed.extend_from_slice(b"[later]\nkeep=2\n");
    assert!(
        host_config::toml_patch(&changed, &path, None, create.owned_fragment.as_ref(), &d())
            .is_err()
    );
    assert_eq!(toml::from_str::<toml::Value>(std::str::from_utf8(&changed).unwrap()).unwrap()["later"]["keep"].as_integer(), Some(2));
}

#[test]
fn toml_inverse_preserves_an_existing_appendix_newline_and_updates_retain_the_owned_separator() {
    let original = b"# human";
    let path = vec!["yoke".to_owned()];
    let create = host_config::toml_patch(original, &path, Some(b"x=1\n"), None, &d()).unwrap();
    let installed = apply(original, &create);
    let mut separated = installed.clone();
    separated.extend_from_slice(b"\n[later]\nkeep=2\n");
    let remove = host_config::toml_patch(
        &separated,
        &path,
        None,
        create.owned_fragment.as_ref(),
        &d(),
    )
    .unwrap();
    assert_eq!(apply(&separated, &remove), b"# human\n[later]\nkeep=2\n");
    let mut immediate = installed;
    immediate.extend_from_slice(b"[later]\nkeep=2\n");
    let update = host_config::toml_patch(
        &immediate,
        &path,
        Some(b"x=3\n"),
        create.owned_fragment.as_ref(),
        &d(),
    )
    .unwrap();
    assert_eq!(
        apply(&immediate, &update),
        b"# human\n[yoke]\nx=3\n[later]\nkeep=2\n"
    );
}

#[test]
fn toml_scanner_ignores_fake_headers_in_multiline_strings_and_arrays_with_quote_runs() {
    let path = vec!["mcp_servers".to_owned(), "yoke".to_owned()];
    for original in [
        b"text = \"\"\"\n[mcp_servers.yoke]\nhello\"\"\"\"\n".as_slice(),
        b"text = '''\n[mcp_servers.yoke]\nhello'''''\n",
        b"text = [\n\"\"\"\n[mcp_servers.yoke]\n\"\"\",\n'[another]',\n]\n",
    ] {
        let source = toml::from_str::<toml::Value>(std::str::from_utf8(original).unwrap()).unwrap();
        assert!(!source.as_table().unwrap().contains_key("mcp_servers"));
        let create = host_config::toml_patch(original, &path, Some(b"x=1\n"), None, &d()).unwrap();
        let installed = apply(original, &create);
        let remove = host_config::toml_patch(
            &installed,
            &path,
            None,
            create.owned_fragment.as_ref(),
            &d(),
        )
        .unwrap();
        assert_eq!(apply(&installed, &remove), original);
    }
}

#[test]
fn toml_quoted_aliases_dotted_conflicts_and_body_headers_fail_without_claiming_ownership() {
    let path = vec!["mcp_servers".to_owned(), "yoke".to_owned()];
    for original in [
        b"[mcp_servers.yoke]\ncommand='human'\n".as_slice(),
        b"[mcp_servers.\"yoke\"]\ncommand='human'\n",
        b"mcp_servers.yoke.command='human'\n",
        b"[mcp_servers.yoke.child]\nx=1\n",
        b"mcp_servers = false\n",
        b"[[mcp_servers.yoke]]\ncommand='human'\n",
    ] {
        assert!(
            host_config::toml_patch(original, &path, Some(b"command='yoke'\n"), None, &d())
                .is_err()
        );
    }
    for body in [
        b"[other]\nx=1\n".as_slice(),
        b"x=1\nx=2\n",
        b"dotted.key=1\n",
    ] {
        assert!(host_config::toml_patch(b"name='human'\n", &path, Some(body), None, &d()).is_err());
    }
}

#[test]
fn structured_formats_reject_stale_selectors_body_capacity_and_deadlines() {
    let parent = vec!["mcpServers".to_owned()];
    let input = br#"{"mcpServers":{}}"#;
    let create = host_config::jsonc_patch(input, &parent, "yoke", Some(b"{}"), None, &d()).unwrap();
    let written = apply(input, &create);
    let other = vec!["other".to_owned()];
    assert!(
        host_config::jsonc_patch(
            &written,
            &other,
            "yoke",
            None,
            create.owned_fragment.as_ref(),
            &d()
        )
        .is_err()
    );
    assert!(
        host_config::jsonc_patch(
            input,
            &parent,
            "yoke",
            Some(&vec![b' '; 65_537]),
            None,
            &d()
        )
        .is_err()
    );
    assert!(
        host_config::jsonc_patch(input, &parent, "yoke", Some(b"{}"), None, &Deadline::new(0))
            .is_err()
    );
    let path = vec!["mcp_servers".to_owned(), "yoke".to_owned()];
    assert!(host_config::toml_patch(b"", &path, Some(&vec![b' '; 65_537]), None, &d()).is_err());
    assert!(host_config::toml_patch(b"", &path, Some(b"x=1"), None, &Deadline::new(0)).is_err());
}

#[test]
fn jsonc_accepts_only_json_whitespace_outside_comments_and_strings() {
    for separator in [b'\x0b', b'\x0c'] {
        let mut bytes = b"{\"mcpServers\":{}".to_vec();
        bytes.push(separator);
        bytes.push(b'}');
        assert!(
            host_config::jsonc_patch(
                &bytes,
                &["mcpServers".into()],
                "yoke",
                Some(b"{}"),
                None,
                &d()
            )
            .is_err()
        );
    }
    let valid = b"{\"human\":\"\\u000b\", /*\x0b*/ \t\r\n\"mcpServers\":{}}";
    let patch = host_config::jsonc_patch(
        valid,
        &["mcpServers".into()],
        "yoke",
        Some(b"{}"),
        None,
        &d(),
    )
    .unwrap();
    let written = apply(valid, &patch);
    let remove = host_config::jsonc_patch(
        &written,
        &["mcpServers".into()],
        "yoke",
        None,
        patch.owned_fragment.as_ref(),
        &d(),
    )
    .unwrap();
    assert_eq!(apply(&written, &remove), valid);
}

#[test]
fn serialized_empty_owned_fragment_returns_error_instead_of_panicking() {
    let mut patch = host_config::agents_patch(b"Human\n", Some(b"Owned"), None, &d()).unwrap();
    let owned = patch.owned_fragment.as_mut().unwrap();
    owned.bytes.clear();
    owned.hash = hash(b"");
    assert!(host_config::apply_patch(b"Human\n", &patch, &d()).is_err());
}

#[test]
fn jsonc_oversized_receipt_selector_is_rejected_before_unbounded_decoding() {
    let input = br#"{"mcpServers":{}}"#;
    let create = host_config::jsonc_patch(
        input,
        &["mcpServers".into()],
        "yoke",
        Some(b"{}"),
        None,
        &d(),
    )
    .unwrap();
    let written = apply(input, &create);
    let mut prior = create.owned_fragment.unwrap();
    prior.selector = format!("[[],\"{}\"]", "x".repeat(65_536));
    assert!(host_config::jsonc_patch(&written, &[], "yoke", None, Some(&prior), &d()).is_err());
}
