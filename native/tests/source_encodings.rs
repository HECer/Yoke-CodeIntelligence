//! Source-only candidate tests. Functional qualification belongs to the parent.
use serde_json::{Value, json};
use std::{
    fs,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use yoke::{
    Engine,
    capture::{CaptureOptions, Deadline},
    changes::{self, Plan},
    model::{hash, identity},
    project_session::{HostProfile, HostProject, Session},
    query,
    session_config::{self, Source, Target},
    source_encoding::{Codec, View},
};

fn d() -> Deadline {
    Deadline::new(30_000)
}
fn bytes(text: &str, codec: Codec, bom: bool) -> Vec<u8> {
    match codec {
        Codec::Utf8 => text.as_bytes().to_vec(),
        Codec::Latin1 => text
            .chars()
            .map(|c| {
                assert!(c as u32 <= 255);
                c as u8
            })
            .collect(),
        _ => {
            let mut out = Vec::new();
            if bom {
                out.extend_from_slice(if codec == Codec::Utf16Be {
                    &[0xfe, 0xff]
                } else {
                    &[0xff, 0xfe]
                });
            }
            for unit in text.encode_utf16() {
                out.extend_from_slice(&if codec == Codec::Utf16Be {
                    unit.to_be_bytes()
                } else {
                    unit.to_le_bytes()
                });
            }
            out
        }
    }
}
fn engine(codec: Codec, text: &str, bom: bool) -> (tempfile::TempDir, Engine, Vec<u8>) {
    let root = tempfile::tempdir().unwrap();
    let raw = bytes(text, codec, bom);
    fs::write(root.path().join("main.rs"), &raw).unwrap();
    let mut e = Engine::open(root.path()).unwrap();
    e.capture_options.source_encoding = codec.policy();
    (root, e, raw)
}
fn plan(e: &mut Engine, edits: Value) -> Plan {
    let (s, g, _, _) = e.index(&d()).unwrap();
    changes::prepare(&mut e.store, &s, &g, &json!({"edits":edits}), &d()).unwrap()
}
fn after(e: &Engine, p: &Plan) -> Vec<u8> {
    e.store
        .read_blob(p.files[0].after_hash.as_ref().unwrap())
        .unwrap()
}
fn project(
    base: &HostProfile,
    targets: &[Target],
    docs: &[&[u8]],
    order: &[usize],
) -> session_config::Projection {
    let hashes: Vec<_> = docs.iter().map(hash).collect();
    let sources: Vec<_> = order
        .iter()
        .map(|&i| Source {
            target: &targets[i],
            expected_sha256: &hashes[i],
            bytes: docs[i],
        })
        .collect();
    session_config::project(base, &sources, &d()).unwrap()
}

#[test]
fn strict_codecs_map_only_scalar_boundaries_and_round_trip_original_bytes() {
    let text = "Aé\r\n💡e\u{301}\nend";
    for (selected, physical, bom) in [
        (Codec::Utf8, Codec::Utf8, false),
        (Codec::Utf16, Codec::Utf16Le, true),
        (Codec::Utf16, Codec::Utf16Be, true),
        (Codec::Utf16Le, Codec::Utf16Le, false),
        (Codec::Utf16Be, Codec::Utf16Be, false),
    ] {
        let raw = bytes(text, physical, bom);
        let view = View::decode(raw.clone(), selected.policy(), &|| d().check()).unwrap();
        assert_eq!(view.text(), text);
        assert_eq!(view.encode_like(text, &|| Ok(())).unwrap(), raw);
        let mut bounds: Vec<_> = text.char_indices().map(|(i, _)| i).collect();
        bounds.push(text.len());
        for b in bounds {
            let r = view.to_raw(b).unwrap();
            assert_eq!(view.to_decoded(r).unwrap(), b);
        }
        let bulb = text.find('💡').unwrap();
        let raw_bulb = view.to_raw(bulb).unwrap();
        assert!(view.to_decoded(raw_bulb + 1).is_err());
        assert!(view.to_decoded(raw_bulb + 2).is_err());
        assert!(view.to_raw(bulb + 1).is_err());
        assert!(view.decoded_range(raw.len() + 1, raw.len() + 1).is_err());
        assert!(view.decoded_range(raw.len(), 0).is_err());
        assert_eq!(view.quote(0, raw.len()).unwrap(), text);
        if bom {
            assert!(view.to_decoded(0).is_err());
        }
    }
    // Explicit endian codecs retain U+FEFF as text; no implicit signature stripping.
    for codec in [Codec::Utf16Le, Codec::Utf16Be] {
        let raw = bytes("x", codec, true);
        let v = View::decode(raw.clone(), codec.policy(), &|| Ok(())).unwrap();
        assert_eq!(v.text(), "\u{feff}x");
        assert_eq!(v.encode_like(v.text(), &|| Ok(())).unwrap(), raw);
    }
    let utf8 = View::decode(b"\xef\xbb\xbfx".to_vec(), None, &|| Ok(())).unwrap();
    assert_eq!(utf8.text(), "\u{feff}x");
    assert_eq!(utf8.to_raw(3).unwrap(), 3);
    let opposite = vec![0xff, 0xfe, 0x78, 0x00];
    let view = View::decode(opposite.clone(), Codec::Utf16Be.policy(), &|| Ok(())).unwrap();
    assert_eq!(view.text(), "\u{fffe}\u{7800}");
    assert_eq!(view.encode_like(view.text(), &|| Ok(())).unwrap(), opposite);
    assert!(view.to_raw(usize::MAX).is_err());
    assert!(view.to_decoded(usize::MAX).is_err());
}

#[test]
fn true_latin1_is_bijective_and_cannot_encode_windows1252_or_lossy_replacements() {
    let raw: Vec<_> = (0..=255).collect();
    let view = View::decode(raw.clone(), Codec::Latin1.policy(), &|| Ok(())).unwrap();
    assert_eq!(view.text().chars().nth(128), Some('\u{80}'));
    assert_eq!(view.text().chars().nth(233), Some('é'));
    assert_eq!(view.encode_like(view.text(), &|| Ok(())).unwrap(), raw);
    for text in ["€", "💡", "e\u{301}", "\u{fffd}"] {
        assert!(
            view.encode_like(text, &|| Ok(()))
                .unwrap_err()
                .to_string()
                .contains("unrepresentable_latin1")
        );
    }
    for name in [
        "cp1252",
        "windows-1252",
        "utf-8-sig",
        "utf-32",
        "ascii",
        "shift_jis",
        " utf-16",
        "utf 16",
        "UTF–16",
    ] {
        assert!(Codec::parse(name).is_err(), "{name}");
    }
    for (names, codec) in [
        (
            vec!["utf-8", "utf_8", "UTF8", "u8", "UTF", "cp65001"],
            Codec::Utf8,
        ),
        (vec!["utf-16", "utf_16", "UTF16", "u16"], Codec::Utf16),
        (vec!["utf_16_le", "UTF-16LE", "utf-16-le"], Codec::Utf16Le),
        (vec!["utf_16_be", "UTF-16BE", "utf-16-be"], Codec::Utf16Be),
        (
            vec![
                "latin_1",
                "iso-8859-1",
                "ISO8859-1",
                "8859",
                "cp819",
                "latin",
                "latin1",
                "L1",
            ],
            Codec::Latin1,
        ),
    ] {
        for name in names {
            assert_eq!(Codec::parse(name).unwrap(), codec);
        }
    }
}

#[test]
fn malformed_utf16_and_capacity_deadlines_refuse_without_replacement() {
    for raw in [
        vec![],
        vec![0xff],
        vec![b'a', 0],
        vec![0xff, 0xfe, 1],
        vec![0xff, 0xfe, 0, 0xd8],
        vec![0xff, 0xfe, 0, 0xdc],
        vec![0xfe, 0xff, 0xd8, 0, 0, 65],
    ] {
        assert!(View::decode(raw, Codec::Utf16.policy(), &|| Ok(())).is_err());
    }
    for codec in [Codec::Utf16Le, Codec::Utf16Be] {
        assert!(View::decode(vec![0], codec.policy(), &|| Ok(())).is_err());
    }
    let limit = yoke::source_encoding::MAX_RAW;
    assert!(View::decode(vec![b'x'; limit + 1], Codec::Latin1.policy(), &|| Ok(())).is_err());
    assert!(
        View::decode(
            vec![0xe9; limit / 2 + 1],
            Codec::Latin1.policy(),
            &|| Ok(())
        )
        .is_err()
    );
    assert!(
        View::decode(vec![b'x'; 16384], Codec::Latin1.policy(), &|| {
            Deadline::new(0).check()
        })
        .is_err()
    );
    let cancel = Arc::new(AtomicBool::new(false));
    let parent = d();
    let c = cancel.clone();
    let result = Deadline::run_bounded(&parent, c, || {
        let inherited = d();
        let ticks = std::cell::Cell::new(0);
        View::decode(vec![b'x'; 65536], Codec::Latin1.policy(), &|| {
            ticks.set(ticks.get() + 1);
            if ticks.get() == 3 {
                cancel.store(true, Ordering::SeqCst);
            }
            inherited.check()
        })
        .map(|_| ())
    });
    assert!(result.unwrap_err().to_string().contains("cancelled"));
}

#[test]
fn actual_capture_structural_search_quotes_parallel_edges_and_immutable_replay_use_raw_offsets() {
    for (codec, bom) in [
        (Codec::Utf16, true),
        (Codec::Utf16Le, false),
        (Codec::Utf16Be, false),
        (Codec::Latin1, false),
    ] {
        let text = "// é\r\nfn target() {}\r\nfn caller() { target(); target(); }\r\n";
        let (root, mut e, raw) = engine(codec, text, bom);
        let (s, g, parsed, reused) = e.index(&d()).unwrap();
        assert_eq!((parsed, reused), (1, 0));
        assert_eq!(s.files[0].hash, hash(&raw));
        assert_eq!(s.files[0].size, raw.len());
        assert_eq!(e.store.read_blob(&s.files[0].hash).unwrap(), raw);
        let v = query::source_view(&e.store, &s, "main.rs").unwrap();
        let target = g.entities.iter().find(|n| n.name == "target").unwrap();
        assert_eq!(
            target.start_byte,
            v.to_raw(text.find("fn target").unwrap()).unwrap()
        );
        let read = query::read(&e.store, &s, &g, &json!({"entityId":target.id})).unwrap();
        assert_eq!(read["text"], "fn target() {}");
        assert_eq!(read["sourceView"]["rawContentHash"], hash(&raw));
        assert_eq!(read["sourceView"]["analysisContentHash"], hash(text));
        let calls: Vec<_> = g
            .observations
            .iter()
            .filter(|n| n.relation == "calls" && n.target.as_deref() == Some(target.id.as_str()))
            .collect();
        assert_eq!(calls.len(), 2);
        assert_ne!(calls[0].evidence_ids, calls[1].evidence_ids);
        for call in calls {
            let ev = g
                .evidence
                .iter()
                .find(|x| x.id == call.evidence_ids[0])
                .unwrap();
            assert_eq!(ev.content_hash, hash(&raw));
            assert_eq!(v.quote(ev.start_byte, ev.end_byte).unwrap(), "target()");
            assert_eq!(ev.anchor, "exact_decoded_view_original_byte_boundaries_v1");
        }
        let found = e
            .invoke(
                "ci_find",
                json!({"snapshotId":s.id,"query":"target","kind":"text","groupBySymbol":true}),
            )
            .unwrap();
        assert_eq!(found["status"], "ok", "{found}");
        for item in found["data"]["items"].as_array().unwrap() {
            assert_eq!(
                v.quote(
                    item["startByte"].as_u64().unwrap() as usize,
                    item["endByte"].as_u64().unwrap() as usize
                )
                .unwrap(),
                "target"
            );
        }
        let symbols = e
            .invoke(
                "ci_find",
                json!({"snapshotId":s.id,"namePath":"target","includeBody":true}),
            )
            .unwrap();
        assert_eq!(symbols["status"], "ok", "{symbols}");
        assert_eq!(symbols["data"]["items"][0]["body"], "fn target() {}");
        let header =
            yoke::file_api::inspect(&e.store, &s, &g, &json!({"file":"main.rs"}), &d()).unwrap();
        assert!(
            header["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|x| x["signature"] == "fn target()")
        );
        let old_s = serde_json::to_vec(&s).unwrap();
        let old_g = serde_json::to_vec(&g).unwrap();
        let (_, _, p, r) = e.index(&d()).unwrap();
        assert_eq!((p, r), (0, 1));
        fs::write(root.path().join("main.rs"), b"different live bytes").unwrap();
        e.capture_options.source_encoding = None;
        let old = e
            .invoke("ci_read", json!({"snapshotId":s.id,"path":"main.rs"}))
            .unwrap();
        assert_eq!(old["status"], "ok", "{old}");
        assert_eq!(old["data"]["text"], text);
        assert_eq!(
            serde_json::to_vec(&e.store.snapshot(&s.id).unwrap()).unwrap(),
            old_s
        );
        assert_eq!(
            serde_json::to_vec(&e.store.graph(&s.id).unwrap()).unwrap(),
            old_g
        );
    }
}

#[test]
fn encoded_capture_applies_secret_path_and_decoded_secret_filters_before_publication() {
    for codec in [Codec::Utf16, Codec::Utf16Le, Codec::Utf16Be, Codec::Latin1] {
        let (root, mut e, _) = engine(codec, "fn safe() {}\n", codec == Codec::Utf16);
        for (name, text) in [
            (".env", "owned private path"),
            ("secret.rs", "-----BEGIN PRIVATE KEY-----"),
            ("embedded-nul.rs", "a\0b"),
        ] {
            fs::write(
                root.path().join(name),
                bytes(text, codec, codec == Codec::Utf16),
            )
            .unwrap();
        }
        let (s, _, _, _) = e.index(&d()).unwrap();
        assert_eq!(s.files.len(), 1);
        for (name, reason) in [
            (".env", "protected-path"),
            ("secret.rs", "obvious-secret"),
            ("embedded-nul.rs", "binary-decoded-nul"),
        ] {
            assert!(
                s.omissions
                    .iter()
                    .any(|x| x.path == name && x.reason == reason),
                "{name} {:?}",
                s.omissions
            );
        }
        let bad = e
            .invoke("ci_read", json!({"snapshotId":s.id,"path":"secret.rs"}))
            .unwrap();
        assert_eq!(bad["status"], "error");
        let bad = e
            .invoke(
                "ci_change_prepare",
                json!({"edits":[{"kind":"create","path":"../escape.rs","text":"x"}]}),
            )
            .unwrap();
        assert_eq!(bad["status"], "error");
    }
}

#[test]
fn replacements_creates_bulk_edits_and_plan_replay_encode_before_review_preserving_bom_and_crlf() {
    for (codec, bom) in [
        (Codec::Utf16, true),
        (Codec::Utf16Le, false),
        (Codec::Utf16Be, false),
        (Codec::Latin1, false),
    ] {
        let text = "// é\r\nfn target() {}\r\nfn keep() {}\n";
        let (root, mut e, raw) = engine(codec, text, bom);
        let (s, g, _, _) = e.index(&d()).unwrap();
        let v = query::source_view(&e.store, &s, "main.rs").unwrap();
        let a = text.find("target").unwrap();
        let (a, b) = (v.to_raw(a).unwrap(), v.to_raw(a + 6).unwrap());
        let p = plan(
            &mut e,
            json!([{"kind":"replace","path":"main.rs","expectedHash":hash(&raw),"startByte":a,"endByte":b,"text":"café"}]),
        );
        let actual = after(&e, &p);
        let expected = bytes(&text.replace("target", "café"), codec, bom);
        assert_eq!(actual, expected);
        assert_eq!(&actual[..a], &raw[..a]);
        assert!(actual.ends_with(&raw[b..]));
        assert_eq!(p.source_encoding, codec.policy());
        changes::load_checked_plan(&e.store, &p.id, &d()).unwrap();
        assert_eq!(fs::read(root.path().join("main.rs")).unwrap(), raw);
        let isolated = tempfile::tempdir().unwrap();
        changes::materialize(
            &e.store,
            &e.store.snapshot(&p.result_snapshot_id).unwrap(),
            isolated.path(),
            &d(),
        )
        .unwrap();
        assert_eq!(fs::read(isolated.path().join("main.rs")).unwrap(), expected);
        let create = plan(
            &mut e,
            json!([{"kind":"create","path":"new.rs","text":"fn café() {}\r\n"}]),
        );
        assert_eq!(after(&e, &create), bytes("fn café() {}\r\n", codec, bom));
        let bulk=e.invoke("ci_change_prepare",json!({"snapshotId":s.id,"bulkReplace":{"pattern":"target","replacement":"café","literal":true,"expectedCount":1}})).unwrap();
        assert_eq!(bulk["status"], "ok", "{bulk}");
        let bulk: Plan = serde_json::from_value(bulk["data"]["plan"].clone()).unwrap();
        assert_eq!(after(&e, &bulk), expected);
        let _ = g;
    }
}

#[test]
fn unrepresentable_edits_invalid_raw_boundaries_stale_codec_and_forged_plans_refuse() {
    let (root, mut e, raw) = engine(Codec::Latin1, "fn café() {}\r\n", false);
    let (s, g, _, _) = e.index(&d()).unwrap();
    let records: i64 = e
        .store
        .db
        .query_row("SELECT count(*) FROM records WHERE kind='plan'", [], |r| {
            r.get(0)
        })
        .unwrap();
    for text in ["€", "💡", "\u{fffd}"] {
        let bad=e.invoke("ci_change_prepare",json!({"edits":[{"kind":"replace","path":"main.rs","expectedHash":hash(&raw),"startByte":3,"endByte":7,"text":text}]})).unwrap();
        assert_eq!(bad["status"], "error", "{bad}");
        assert!(bad.to_string().contains("unrepresentable_latin1"));
    }
    assert_eq!(
        e.store
            .db
            .query_row::<i64, _, _>("SELECT count(*) FROM records WHERE kind='plan'", [], |r| r
                .get(0))
            .unwrap(),
        records
    );
    let p = plan(
        &mut e,
        json!([{"kind":"create","path":"new.rs","text":"fn new() {}"}]),
    );
    changes::approve(&e.store, &p.id, false).unwrap();
    let saved = serde_json::to_value(&p).unwrap();
    let mut forged = saved.clone();
    forged["sourceEncoding"] = json!({"kind":"strict_v1","codec":"utf16_le"});
    e.store
        .db
        .execute(
            "UPDATE records SET value=?1 WHERE kind='plan' AND id=?2",
            rusqlite::params![forged.to_string(), p.id],
        )
        .unwrap();
    assert!(changes::load_checked_plan(&e.store, &p.id, &d()).is_err());
    e.store
        .db
        .execute(
            "UPDATE records SET value=?1 WHERE kind='plan' AND id=?2",
            rusqlite::params![saved.to_string(), p.id],
        )
        .unwrap();
    e.capture_options.source_encoding = Codec::Utf16Le.policy();
    e.invoke("ci_status", json!({})).unwrap(); // sync the actual invocation policy
    assert!(changes::approve(&e.store, &p.id, false).is_err());
    assert!(
        changes::apply(
            &e.store,
            root.path(),
            &json!({"planId":p.id,"idempotencyKey":"stale","mode":"workspace"}),
            true,
            &d()
        )
        .is_err()
    );
    assert_eq!(fs::read(root.path().join("main.rs")).unwrap(), raw);
    assert_eq!(
        query::read(&e.store, &s, &g, &json!({"path":"main.rs"})).unwrap()["text"],
        "fn café() {}\r\n"
    );
    let (_utf16_root, mut e, raw) = engine(Codec::Utf16, "fn x() {}\r\n// 💡", true);
    let (s, _, _, _) = e.index(&d()).unwrap();
    for (a, b) in [(0, 2), (3, 4), (raw.len() - 2, raw.len())] {
        let bad=e.invoke("ci_change_prepare",json!({"snapshotId":s.id,"edits":[{"kind":"replace","path":"main.rs","expectedHash":hash(&raw),"startByte":a,"endByte":b,"text":"x"}]})).unwrap();
        assert_eq!(bad["status"], "error", "{bad}");
    }
}

#[test]
fn yaml_resolution_is_order_independent_and_utf8_legacy_bytes_stay_exact() {
    let base = HostProfile::default();
    let targets = [
        Target::Global,
        Target::Project {
            project: "default".into(),
        },
        Target::ProjectLocal {
            project: "default".into(),
        },
    ];
    let docs: [&[u8]; 3] = [
        b"encoding: latin1\n",
        b"encoding: utf-16\n",
        b"encoding: utf-8\n",
    ];
    for order in [[0, 1, 2], [2, 1, 0], [1, 0, 2], [2, 0, 1]] {
        let projected = project(&base, &targets, &docs, &order);
        assert_eq!(
            projected
                .profile()
                .source_encodings
                .as_ref()
                .unwrap()
                .select("default"),
            Some(Codec::Utf8)
        );
    }
    for doc in [
        b"encoding: null\n".as_slice(),
        b"encoding: utf-8\n",
        b"encoding: UTF8\n",
        b"encoding: utf_8\n",
    ] {
        let projected = project(&base, &[Target::Global], &[doc], &[0]);
        assert_eq!(
            projected.profile_bytes(&d()).unwrap(),
            serde_json::to_vec(&base).unwrap()
        );
    }
    let targets = [
        Target::Global,
        Target::ProjectLocal {
            project: "default".into(),
        },
    ];
    let projected = project(
        &base,
        &targets,
        &[b"encoding: latin1\n", b"encoding: null\n"],
        &[1, 0],
    );
    assert_eq!(
        projected
            .profile()
            .source_encodings
            .as_ref()
            .unwrap()
            .select("default"),
        Some(Codec::Latin1)
    );
    let (root, mut e, _) = engine(Codec::Utf8, "\u{feff}fn café() {}\r\n", false);
    let (s, g, _, _) = e.index(&d()).unwrap();
    let expected = identity(&(
        &e.store.workspace_id,
        "disk",
        yoke::model::ANALYZER,
        &s.files,
        &s.omissions,
    ));
    assert_eq!(s.id, expected);
    e.capture_options =
        serde_json::from_value(json!({"sourceEncoding":{"kind":"strict_v1","codec":"utf8"}}))
            .unwrap();
    let (same, same_graph, _, _) = e.index(&d()).unwrap();
    assert_eq!(s.id, same.id);
    assert_eq!(
        serde_json::to_vec(&g).unwrap(),
        serde_json::to_vec(&same_graph).unwrap()
    );
    assert_eq!(
        query::read(&e.store, &s, &g, &json!({"path":"main.rs"})).unwrap(),
        json!({"path":"main.rs","contentHash":s.files[0].hash,"startByte":0,"endByte":s.files[0].size,"text":"\u{feff}fn café() {}\r\n","trust":"source-data"})
    );
    assert!(root.path().exists());
}

#[test]
fn strict_configuration_protocol_types_unknown_fields_and_host_scopes_do_not_grant_authority() {
    for value in [
        json!(true),
        json!(2),
        json!([]),
        json!({}),
        json!("cp1252"),
        json!("utf-32"),
        json!("owned_private_canary"),
    ] {
        let yaml = format!("encoding: {}\n", value);
        let h = hash(&yaml);
        let target = Target::Global;
        let err = session_config::project(
            &HostProfile::default(),
            &[Source {
                target: &target,
                expected_sha256: &h,
                bytes: yaml.as_bytes(),
            }],
            &d(),
        )
        .err()
        .unwrap()
        .to_string();
        assert!(err.contains("source_encoding"), "{err}");
        assert!(!err.contains("owned_private_canary"));
    }
    for policy in [
        json!({"kind":"strict_v2","codec":"latin1"}),
        json!({"kind":"strict_v1","codec":"cp1252"}),
        json!({"kind":"strict_v1","codec":"latin1","replace":true}),
        json!("latin1"),
        json!(false),
    ] {
        assert!(
            serde_json::from_value::<CaptureOptions>(json!({"sourceEncoding":policy})).is_err()
        );
    }
    for policy in [
        json!({"global":"latin1","allowWrites":true}),
        json!({"global":false}),
        json!({"projects":{"default":{"local":"latin1","trust":true}}}),
    ] {
        assert!(serde_json::from_value::<HostProfile>(json!({"sourceEncodings":policy})).is_err());
    }
    let (root, mut e, _) = engine(Codec::Utf8, "fn original() {}", false);
    for (tool, args) in [
        (
            "ci_read",
            json!({"path":"main.rs","sourceEncoding":"latin1"}),
        ),
        ("ci_find", json!({"query":"original","encoding":"latin1"})),
        (
            "ci_change_prepare",
            json!({"edits":[{"kind":"create","path":"new.rs","text":"x","codec":"latin1"}]}),
        ),
    ] {
        assert_eq!(e.invoke(tool, args).unwrap()["status"], "error");
    }
    let host: HostProfile = serde_json::from_value(
        json!({"sourceEncodings":{"projects":{"unregistered":{"local":"latin1"}}}}),
    )
    .unwrap();
    assert!(Session::open(e, host, Some("invalid"), &d()).is_err());
    assert!(!root.path().join("new.rs").exists());
}

#[test]
fn primary_and_secondary_codec_selection_session_resume_and_source_read_only_stay_independent() {
    let (primary, e, _) = engine(Codec::Utf16, "fn primary() {}", true);
    let (secondary, _, latin) = engine(Codec::Latin1, "fn café() {}", false);
    let third = tempfile::tempdir().unwrap();
    fs::write(third.path().join("main.rs"), "fn third() {}").unwrap();
    let mut host:HostProfile=serde_json::from_value(json!({"sourceEncodings":{"projects":{"default":{"local":"utf16"},"secondary":{"project":"latin1"}}},"primaryReadOnly":true})).unwrap();
    host.projects = vec![
        HostProject {
            name: "secondary".into(),
            root: secondary.path().into(),
            read_only: false,
            query_only: false,
        },
        HostProject {
            name: "third".into(),
            root: third.path().into(),
            read_only: false,
            query_only: false,
        },
    ];
    let mut e = e;
    e.allow_writes = true;
    e.capture_options.source_encoding = None;
    let mut session = Session::open(e, host.clone(), Some("encoded"), &d()).unwrap();
    assert_eq!(
        session.engine.capture_options.source_encoding,
        Codec::Utf16.policy()
    );
    assert!(!session.engine.allow_writes);
    session
        .invoke("activate", &json!({"project":"secondary"}), &d())
        .unwrap();
    let p = session
        .invoke(
            "edit.prepare",
            &json!({"edits":[{"kind":"create","path":"new.rs","text":"fn café() {}"}]}),
            &d(),
        )
        .unwrap();
    assert_eq!(p["status"], "ok", "{p}");
    assert_eq!(p["data"]["plan"]["sourceEncoding"]["codec"], "latin1");
    session
        .invoke("activate", &json!({"project":"third"}), &d())
        .unwrap();
    assert_eq!(
        session.invoke("configuration", &json!({}), &d()).unwrap()["sourceEncoding"],
        Value::Null
    );
    session
        .invoke("activate", &json!({"project":"default"}), &d())
        .unwrap();
    drop(session);
    let mut e = Engine::open(primary.path()).unwrap();
    e.allow_writes = true;
    let session = Session::open(e, host.clone(), Some("encoded"), &d()).unwrap();
    drop(session);
    host.source_encodings
        .as_mut()
        .unwrap()
        .projects
        .get_mut("default")
        .unwrap()
        .local = Some(Codec::Utf16Le);
    let mut e = Engine::open(primary.path()).unwrap();
    e.allow_writes = true;
    assert!(Session::open(e, host, Some("encoded"), &d()).is_err());
    assert_eq!(fs::read(secondary.path().join("main.rs")).unwrap(), latin);
}

#[test]
fn lsp_positions_map_crlf_and_nonbmp_without_permitting_half_surrogates() {
    let text = "é💡x\r\ny";
    let raw = bytes(text, Codec::Utf16Be, true);
    let view = View::decode(raw, Codec::Utf16.policy(), &|| Ok(())).unwrap();
    let decoded = text.find('x').unwrap();
    let original = view.to_raw(decoded).unwrap();
    assert_eq!(
        yoke::lsp::byte_to_position(view.text(), view.to_decoded(original).unwrap()).unwrap(),
        json!({"line":0,"character":3})
    );
    let decoded =
        yoke::lsp::position_to_byte(view.text(), &json!({"line":1,"character":0})).unwrap();
    assert_eq!(
        view.quote(view.to_raw(decoded).unwrap(), view.raw().len())
            .unwrap(),
        "y"
    );
    assert!(yoke::lsp::position_to_byte(view.text(), &json!({"line":0,"character":2})).is_err());
    assert!(yoke::lsp::byte_to_position(view.text(), text.find('\n').unwrap()).is_err());
}

#[test]
fn symbolic_replacement_deletion_and_explicit_line_ending_formatting_use_original_codec() {
    for (codec, bom) in [(Codec::Utf16, true), (Codec::Latin1, false)] {
        let text = "// é\r\nfn target() {}\r\nfn keep() {}\n";
        let (_root, mut e, raw) = engine(codec, text, bom);
        let (s, g, _, _) = e.index(&d()).unwrap();
        let entity = g.entities.iter().find(|n| n.name == "target").unwrap();
        let replace=e.invoke("ci_change_prepare",json!({"snapshotId":s.id,"replaceSymbol":{"entityId":entity.id,"text":"fn café() {}"}})).unwrap();
        assert_eq!(replace["status"], "ok", "{replace}");
        let p: Plan = serde_json::from_value(replace["data"]["plan"].clone()).unwrap();
        assert_eq!(
            after(&e, &p),
            bytes(&text.replace("fn target() {}", "fn café() {}"), codec, bom)
        );
        let delete = e
            .invoke(
                "ci_change_prepare",
                json!({"snapshotId":s.id,"deleteSymbol":{"entityId":entity.id}}),
            )
            .unwrap();
        assert_eq!(delete["status"], "ok", "{delete}");
        let p: Plan = serde_json::from_value(delete["data"]["plan"].clone()).unwrap();
        assert_eq!(
            after(&e, &p),
            bytes(&text.replace("fn target() {}", ""), codec, bom)
        );
        let remove=e.invoke("ci_change_prepare",json!({"snapshotId":s.id,"edits":[{"kind":"delete","path":"main.rs","expectedHash":hash(&raw)}]})).unwrap();
        assert_eq!(remove["status"], "ok", "{remove}");
        assert!(remove["data"]["plan"]["files"][0]["afterHash"].is_null());
    }
    let (root, mut e, _) = engine(Codec::Utf16, "fn original() {}\r\n", true);
    e.capture_options.source_encoding = None;
    let host: HostProfile = serde_json::from_value(
        json!({"sourceEncodings":{"global":"utf16"},"sourceLineEndings":{"global":"crlf"}}),
    )
    .unwrap();
    let mut session = Session::open(e, host, Some("newlines"), &d()).unwrap();
    let result = session
        .invoke(
            "edit.prepare",
            &json!({"edits":[{"kind":"create","path":"new.rs","text":"// é\nfn new() {}\r"}]}),
            &d(),
        )
        .unwrap();
    assert_eq!(result["status"], "ok", "{result}");
    let p: Plan = serde_json::from_value(result["data"]["plan"].clone()).unwrap();
    assert_eq!(after(&session.engine, &p), utf16_crlf_fixture());
    changes::load_checked_plan(&session.engine.store, &p.id, &d()).unwrap();
    assert!(root.path().exists());
}
fn utf16_crlf_fixture() -> Vec<u8> {
    bytes("// é\r\nfn new() {}\r\n", Codec::Utf16, true)
}

#[test]
fn facade_symbol_bodies_decode_selected_bytes_without_changing_raw_anchors() {
    for (codec, text) in [
        (
            Codec::Latin1,
            "fn target() { let value = \"\u{c3}\u{a9}\"; }\n",
        ),
        (Codec::Utf16Le, "fn target() { let value = \"plain\"; }\n"),
        (Codec::Utf8, "fn target() { let value = \"é\"; }\n"),
    ] {
        let (_root, e, raw) = engine(codec, text, false);
        // Both selected encodings deliberately also admit a wrong UTF-8 view.
        assert!(std::str::from_utf8(&raw).is_ok());
        let host = serde_json::from_value(json!({"sourceEncodings":{"global":codec}})).unwrap();
        let mut session = Session::open(e, host, Some("encoded-bodies"), &d()).unwrap();
        let value = yoke::agent_repl::invoke_facade(
            &mut session,
            "lsp.find_symbol",
            json!({"name_path_pattern":"target","relative_path":"main.rs","include_body":true}),
            None,
            &d(),
        )
        .unwrap();
        let items = value["items"].as_array().unwrap();
        assert_eq!(items.len(), 1, "{value}");
        let item = &items[0];
        assert_eq!(item["body"], text.trim_end());
        assert_eq!(item["contentHash"], hash(&raw));
        assert_eq!(item["startByte"], 0);
        assert_eq!(item["endByte"], raw.len() - bytes("\n", codec, false).len());
        assert_eq!(item["bodyEndByte"], item["endByte"]);
        assert_eq!(item["bodyTruncated"], false);
        if codec == Codec::Utf8 {
            assert!(item.get("sourceView").is_none());
        } else {
            assert_eq!(item["sourceView"]["rawContentHash"], hash(&raw));
            assert_eq!(item["sourceView"]["analysisContentHash"], hash(text));
            assert_eq!(item["sourceView"]["offsetSpace"], "original_source_bytes");
        }
    }
}

#[test]
fn model_source_selection_refuses_utf8_valid_latin1_before_provider_execution() {
    let (root, mut e, raw) = engine(
        Codec::Latin1,
        "fn target() { let value = \"\u{c3}\u{a9}\"; }\n",
        false,
    );
    assert!(std::str::from_utf8(&raw).is_ok());
    let (snapshot, graph, _, _) = e.index(&d()).unwrap();
    let id = graph
        .entities
        .iter()
        .find(|e| e.name == "target")
        .unwrap()
        .id
        .clone();
    let provider = yoke::providers::ProviderSpec {
        trusted: true,
        command: root.path().join("provider-must-not-start"),
        args: vec![],
        model_id: "encoding-refusal".into(),
        model_version: "v1".into(),
        task_id: "encoding-refusal".into(),
        currency: "EUR".into(),
        max_cost_micros: 10,
        task_limit_micros: 100,
    };
    let error =
        yoke::providers::summarize_sources(&e.store, &snapshot, &graph, &[id], &provider, &d())
            .unwrap_err()
            .to_string();
    assert_eq!(
        error,
        "source_encoding_capability_unavailable: model source selection requires legacy UTF-8 source proof"
    );
    assert!(!provider.command.exists());
    let error = yoke::knowledge::build_local_vectors(&e.store, &snapshot, &graph, &d())
        .unwrap_err()
        .to_string();
    assert_eq!(
        error,
        "source_encoding_capability_unavailable: knowledge source proofs requires legacy UTF-8 source proof"
    );
}

#[test]
fn optional_utf8_piece_consumers_refuse_without_starting_tools_or_creating_overlay_state() {
    let (_root, mut e, _) = engine(Codec::Latin1, "fn café() {}", false);
    let (s, _, _, _) = e.index(&d()).unwrap();
    let spec = yoke::projects::CompilerSpec {
        node: "/not-an-owned-executable".into(),
        typescript: "/not-a-host-module".into(),
        trusted: true,
    };
    let error = yoke::projects::analyze(&e.store, &s, &spec, &d())
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("source_encoding_capability_unavailable"),
        "{error}"
    );
    assert!(
        yoke::editor::open(&mut e, &d())
            .err()
            .unwrap()
            .to_string()
            .contains("source_encoding_capability_unavailable")
    );
    assert_eq!(
        e.store
            .db
            .query_row::<i64, _, _>(
                "SELECT count(*) FROM records WHERE kind='editor-session'",
                [],
                |r| r.get(0)
            )
            .unwrap(),
        0
    );
    for target in [
        Target::Mode {
            name: "owned".into(),
        },
        Target::Context {
            name: "owned".into(),
        },
    ] {
        let yaml = b"encoding: utf-16\n";
        let h = hash(yaml);
        assert!(
            session_config::project(
                &HostProfile::default(),
                &[Source {
                    target: &target,
                    expected_sha256: &h,
                    bytes: yaml
                }],
                &d()
            )
            .is_err()
        );
    }
    let null: HostProfile = serde_json::from_value(json!({"sourceEncodings":null})).unwrap();
    assert_eq!(
        serde_json::to_vec(&null).unwrap(),
        serde_json::to_vec(&HostProfile::default()).unwrap()
    );
}
