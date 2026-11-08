use super::*;

fn raw32(text: &str, big: bool, bom: bool) -> Vec<u8> {
    let mut raw = if bom {
        if big {
            vec![0, 0, 0xfe, 0xff]
        } else {
            vec![0xff, 0xfe, 0, 0]
        }
    } else {
        Vec::new()
    };
    for ch in text.chars() {
        raw.extend_from_slice(&if big {
            (ch as u32).to_be_bytes()
        } else {
            (ch as u32).to_le_bytes()
        });
    }
    raw
}

#[test]
fn utf32_scalar_boundaries_boms_and_malformed_units_are_strict() {
    let text = "\0A\u{7f}\u{80}\u{7ff}\u{800}\u{d7ff}\u{e000}\u{fdd0}\u{fffe}\u{ffff}\u{10000}\u{10ffff}\r\n";
    for (codec, big, bom) in [
        (Codec::Utf32, false, true),
        (Codec::Utf32, true, true),
        (Codec::Utf32Le, false, false),
        (Codec::Utf32Be, true, false),
    ] {
        let raw = raw32(text, big, bom);
        let view = View::decode(raw.clone(), codec.policy(), &|| Ok(())).unwrap();
        assert_eq!(view.text(), text);
        assert_eq!(view.encode_like(text, &|| Ok(())).unwrap(), raw);
        let prefix = if bom { 4 } else { 0 };
        for (ordinal, (decoded, ch)) in text.char_indices().enumerate() {
            let original = prefix + ordinal * 4;
            assert_eq!(view.to_raw(decoded).unwrap(), original);
            assert_eq!(view.to_decoded(original).unwrap(), decoded);
            for n in 1..4 {
                assert!(view.to_decoded(original + n).is_err());
            }
            for n in 1..ch.len_utf8() {
                assert!(view.to_raw(decoded + n).is_err());
            }
        }
        assert_eq!(view.to_raw(text.len()).unwrap(), raw.len());
        assert_eq!(view.quote(0, raw.len()).unwrap(), text);
        for n in 0..prefix {
            assert!(view.to_decoded(n).is_err());
        }
        for cut in 1..4 {
            assert!(
                View::decode(raw[..raw.len() - cut].to_vec(), codec.policy(), &|| Ok(())).is_err()
            );
        }
    }
    for (codec, big) in [(Codec::Utf32Le, false), (Codec::Utf32Be, true)] {
        let raw = raw32("x", big, true);
        let view = View::decode(raw.clone(), codec.policy(), &|| Ok(())).unwrap();
        assert_eq!(view.text(), "\u{feff}x");
        assert_eq!(view.to_raw(0).unwrap(), 0);
        assert_eq!(view.encode_like(view.text(), &|| Ok(())).unwrap(), raw);
        assert!(View::decode(raw32("", !big, true), codec.policy(), &|| Ok(())).is_err());
        for scalar in [0xd800u32, 0xdbff, 0xdc00, 0xdfff, 0x110000, u32::MAX] {
            let unit = if big {
                scalar.to_be_bytes()
            } else {
                scalar.to_le_bytes()
            };
            assert!(
                View::decode(unit.to_vec(), codec.policy(), &|| Ok(()))
                    .err()
                    .unwrap()
                    .to_string()
                    .contains("utf32_invalid_scalar")
            );
        }
        assert_eq!(
            View::decode(vec![], codec.policy(), &|| Ok(()))
                .unwrap()
                .text(),
            ""
        );
    }
    for raw in [vec![], vec![0xff, 0xfe], raw32("x", false, false)] {
        assert!(View::decode(raw, Codec::Utf32.policy(), &|| Ok(())).is_err());
    }
    for big in [false, true] {
        let view = View::decode(raw32("", big, true), Codec::Utf32.policy(), &|| Ok(())).unwrap();
        assert_eq!(view.text(), "");
        assert_eq!(view.to_raw(0).unwrap(), 4);
    }
}

#[test]
fn utf32_capture_plans_and_replay_keep_original_hashes_endian_and_byte_ranges() {
    let text = "// café💡\r\nfn target() {}\r\nfn caller() { target(); target(); }\r\n";
    for (codec, big, bom) in [
        (Codec::Utf32, false, true),
        (Codec::Utf32, true, true),
        (Codec::Utf32Le, false, false),
        (Codec::Utf32Be, true, false),
    ] {
        let root = tempfile::tempdir().unwrap();
        let raw = raw32(text, big, bom);
        fs::write(root.path().join("main.rs"), &raw).unwrap();
        let mut e = Engine::open(root.path()).unwrap();
        e.capture_options.source_encoding = codec.policy();
        let (s, g, _, _) = e.index(&d()).unwrap();
        assert_eq!(s.files[0].hash, hash(&raw));
        assert_eq!(s.files[0].size, raw.len());
        let target = g.entities.iter().find(|n| n.name == "target").unwrap();
        let start = usize::from(bom) * 4 + text[..text.find("target").unwrap()].chars().count() * 4;
        assert_eq!(target.name_start_byte, Some(start));
        let calls: Vec<_> = g
            .observations
            .iter()
            .filter(|n| n.relation == "calls" && n.target.as_deref() == Some(target.id.as_str()))
            .collect();
        assert_eq!(calls.len(), 2);
        assert_ne!(calls[0].evidence_ids, calls[1].evidence_ids);
        let request = json!({"edits":[{"kind":"replace","path":"main.rs","expectedHash":hash(&raw),"startByte":start,"endByte":start+24,"text":"café"}]});
        let p = changes::prepare(&mut e.store, &s, &g, &request, &d()).unwrap();
        assert_eq!(
            after(&e, &p),
            raw32(&text.replacen("target", "café", 1), big, bom)
        );
        changes::load_checked_plan(&e.store, &p.id, &d()).unwrap();
        let invalid = json!({"edits":[{"kind":"replace","path":"main.rs","expectedHash":hash(&raw),"startByte":start+1,"endByte":start+24,"text":"bad"}]});
        assert!(changes::prepare(&mut e.store, &s, &g, &invalid, &d()).is_err());
        let create = changes::prepare(
            &mut e.store,
            &s,
            &g,
            &json!({"edits":[{"kind":"create","path":"new.rs","text":"fn fresh() {}\r\n"}]}),
            &d(),
        )
        .unwrap();
        assert_eq!(
            after(&e, &create),
            raw32(
                "fn fresh() {}\r\n",
                codec == Codec::Utf32Be,
                codec == Codec::Utf32
            )
        );
        assert!(!root.path().join("new.rs").exists());
        assert_eq!(fs::read(root.path().join("main.rs")).unwrap(), raw);
        fs::write(root.path().join("main.rs"), b"different live source").unwrap();
        e.capture_options.source_encoding = None;
        let historical = e
            .invoke("ci_read", json!({"snapshotId":s.id,"path":"main.rs"}))
            .unwrap();
        assert_eq!(historical["status"], "ok", "{historical}");
        assert_eq!(historical["data"]["text"], text);
        assert_eq!(e.store.read_blob(&s.files[0].hash).unwrap(), raw);
    }
}

#[test]
fn utf32_selection_capacity_and_cancellation_do_not_relax_existing_admission() {
    for (alias, codec) in [
        ("UTF-32", Codec::Utf32),
        ("u32", Codec::Utf32),
        ("utf32", Codec::Utf32),
        ("UTF-32LE", Codec::Utf32Le),
        ("utf_32_le", Codec::Utf32Le),
        ("UTF-32BE", Codec::Utf32Be),
        ("utf_32_be", Codec::Utf32Be),
    ] {
        assert_eq!(Codec::parse(alias).unwrap(), codec);
        for selection in [alias.to_string(), serde_json::to_string(alias).unwrap()] {
            let yaml = format!("encoding: {selection}\n");
            let projected = project(
                &HostProfile::default(),
                &[Target::Global],
                &[yaml.as_bytes()],
                &[0],
            );
            assert_eq!(
                projected
                    .profile()
                    .source_encodings
                    .as_ref()
                    .unwrap()
                    .select("default"),
                Some(codec)
            );
        }
        let policy = serde_json::json!({"sourceEncoding":{"kind":"strict_v1","codec":codec}});
        let options: CaptureOptions = serde_json::from_value(policy).unwrap();
        assert_eq!(options.source_encoding, codec.policy());
    }
    assert!(serde_json::from_value::<Codec>(serde_json::json!("utf-32")).is_err());
    for codec in [Codec::Utf32, Codec::Utf32Le, Codec::Utf32Be] {
        let raw = raw32("x", codec == Codec::Utf32Be, codec == Codec::Utf32);
        let view = View::decode(raw, codec.policy(), &|| Ok(())).unwrap();
        let too_large = "x".repeat(yoke::source_encoding::MAX_RAW / 4 + 1);
        assert!(
            view.encode_like(&too_large, &|| Ok(()))
                .unwrap_err()
                .to_string()
                .contains("raw_budget")
        );
        let oversized = raw32(&too_large, codec == Codec::Utf32Be, codec == Codec::Utf32);
        assert!(
            View::decode(oversized, codec.policy(), &|| Ok(()))
                .err()
                .unwrap()
                .to_string()
                .contains("raw_budget")
        );
        let expired = View::decode(
            raw32("x", codec == Codec::Utf32Be, codec == Codec::Utf32),
            codec.policy(),
            &|| Deadline::new(0).check(),
        )
        .err()
        .unwrap();
        assert!(
            expired
                .to_string()
                .to_ascii_lowercase()
                .contains("deadline")
        );
    }
    let calls = std::cell::Cell::new(0);
    let error = View::decode(
        raw32(&"x".repeat(16384), false, false),
        Codec::Utf32Le.policy(),
        &|| {
            calls.set(calls.get() + 1);
            if calls.get() == 3 {
                anyhow::bail!("owned_utf32_cancel");
            }
            Ok(())
        },
    )
    .err()
    .unwrap();
    assert!(error.to_string().contains("owned_utf32_cancel"));
    assert_eq!(calls.get(), 3);
}
