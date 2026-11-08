//! Pure copied-source/normalization checks; no provider process is launched.
use super::*;
use crate::source_encoding::Codec;

fn fixture(codec: Codec) -> (tempfile::TempDir, Engine, Snapshot, Graph, Vec<u8>) {
    let root = tempfile::tempdir().unwrap();
    let text = "// é💡\r\nfn target() {}\r\n";
    let mut raw = vec![0xfe, 0xff];
    for u in text.encode_utf16() {
        raw.extend_from_slice(&u.to_be_bytes());
    }
    let mut e = Engine::open(root.path()).unwrap();
    e.capture_options.source_encoding = codec.policy();
    std::fs::write(root.path().join("main.rs"), &raw).unwrap();
    let (s, g, _, _) = e.index(&Deadline::new(30000)).unwrap();
    (root, e, s, g, raw)
}

#[test]
fn private_utf8_copies_have_independent_integrity_and_original_raw_blobs_stay_exact() {
    let (root, e, s, _, raw) = fixture(Codec::Utf16);
    let tree = tempfile::tempdir().unwrap();
    let d = Deadline::new(30000);
    let exclusions = std::collections::BTreeSet::new();
    changes::materialize_source_views(&e.store, &s, tree.path(), &exclusions, &d).unwrap();
    assert_eq!(
        std::fs::read_to_string(tree.path().join("main.rs")).unwrap(),
        "// é💡\r\nfn target() {}\r\n"
    );
    check_copy_verified(&e.store, &s, tree.path(), &exclusions, &d).unwrap();
    assert_eq!(std::fs::read(root.path().join("main.rs")).unwrap(), raw);
    assert_eq!(e.store.read_blob(&s.files[0].hash).unwrap(), raw);
    std::fs::write(tree.path().join("main.rs"), "fn forged() {}\r\n").unwrap();
    assert!(check_copy_verified(&e.store, &s, tree.path(), &exclusions, &d).is_err());
    let original = tempfile::tempdir().unwrap();
    changes::materialize(&e.store, &s, original.path(), &d).unwrap();
    assert_eq!(std::fs::read(original.path().join("main.rs")).unwrap(), raw);
}

#[test]
fn provider_ranges_are_bound_to_decoded_text_but_publish_only_original_byte_offsets() {
    let (_root, e, s, _, raw) = fixture(Codec::Utf16);
    let d = Deadline::new(30000);
    let tree = tempfile::tempdir().unwrap();
    changes::materialize_source_views(&e.store, &s, tree.path(), &Default::default(), &d).unwrap();
    let mut normalize =
        Normalizer::new(&e.store, &s, tree.path(), "labeled-source-view-fixture", &d).unwrap();
    let span = json!({"start":{"line":1,"character":3},"end":{"line":1,"character":9}});
    let anchor = normalize.anchor("main.rs", &span, true).unwrap();
    let v = query::source_view(&e.store, &s, "main.rs").unwrap();
    assert_eq!(anchor["text"], "target");
    assert_eq!(
        anchor["startByte"],
        v.to_raw(v.text().find("target").unwrap()).unwrap()
    );
    assert_eq!(anchor["contentHash"], crate::model::hash(raw));
    assert_ne!(
        anchor["sourceView"]["analysisContentHash"],
        anchor["contentHash"]
    );
    // U+1F4A1 consumes two UTF-16 code units in the first comment line.
    assert!(
        normalize
            .anchor(
                "main.rs",
                &json!({"start":{"line":0,"character":5},"end":{"line":0,"character":6}}),
                false
            )
            .is_err()
    );
    assert!(
        normalize
            .anchor(
                "main.rs",
                &json!({"start":{"line":1,"character":9},"end":{"line":1,"character":3}}),
                false
            )
            .is_err()
    );
}

#[test]
fn optional_copy_mapping_capacity_refuses_before_a_provider_can_start() {
    let root = tempfile::tempdir().unwrap();
    let mut e = Engine::open(root.path()).unwrap();
    e.capture_options.source_encoding = Codec::Latin1.policy();
    let raw = vec![b'x'; 2_000_000];
    std::fs::write(root.path().join("data.txt"), &raw).unwrap();
    let d = Deadline::new(30000);
    let (s, _, _, _) = e.index(&d).unwrap();
    let tree = tempfile::tempdir().unwrap();
    let error =
        changes::materialize_source_views(&e.store, &s, tree.path(), &Default::default(), &d)
            .unwrap_err()
            .to_string();
    assert!(error.contains("source_encoding_capability_unavailable"));
    assert_eq!(std::fs::read(root.path().join("data.txt")).unwrap(), raw);
}

#[test]
fn utf8_sig_private_copy_and_lsp_positions_preserve_raw_bom_offsets() {
    for bom in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let text = "// \u{e9}\u{1f4a1}\r\nfn target() {}\r\n";
        let mut raw = if bom {
            vec![0xef, 0xbb, 0xbf]
        } else {
            Vec::new()
        };
        raw.extend_from_slice(text.as_bytes());
        std::fs::write(root.path().join("main.rs"), &raw).unwrap();
        let mut e = Engine::open(root.path()).unwrap();
        e.capture_options.source_encoding = Codec::Utf8Sig.policy();
        let d = Deadline::new(30000);
        let (s, _, _, _) = e.index(&d).unwrap();
        let tree = tempfile::tempdir().unwrap();
        changes::materialize_source_views(&e.store, &s, tree.path(), &Default::default(), &d)
            .unwrap();
        assert_eq!(
            std::fs::read(tree.path().join("main.rs")).unwrap(),
            text.as_bytes()
        );
        check_copy_verified(&e.store, &s, tree.path(), &Default::default(), &d).unwrap();
        let mut n = Normalizer::new(&e.store, &s, tree.path(), "fixture", &d).unwrap();
        let anchor = n
            .anchor(
                "main.rs",
                &json!({"start":{"line":1,"character":3},"end":{"line":1,"character":9}}),
                true,
            )
            .unwrap();
        assert_eq!(
            anchor["startByte"],
            text.find("target").unwrap() + if bom { 3 } else { 0 }
        );
        assert_eq!(anchor["contentHash"], crate::model::hash(&raw));
    }
}
