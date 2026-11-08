//! Released-API fixture qualification for captured Git affected analysis.
use yoke as test_yoke;
#[path = "support/git_affected_fixture.rs"]
mod fixture;
use fixture::{Edit, PATH, hierarchy, oracle, side, state};
use std::collections::BTreeSet;
use yoke::{capture::Deadline, git_workflows, model::hash};

fn qualify(edit: Edit, crlf: bool) {
    let f = hierarchy(edit, crlf);
    let before = state(&f.engine);
    let mut expected_changed = Vec::new();
    for label in ["base", "head"] {
        let (snapshot, graph) = side(&f, label);
        let expected = oracle(&f, label);
        let text = f.text(label, PATH);
        let (captured, content_hash) =
            yoke::query::source(&f.engine.store, &snapshot, PATH).unwrap();
        assert_eq!(captured, text);
        assert_eq!(content_hash, hash(text));
        assert!(text.contains("caf\u{e9} \u{1f30d} e\u{301}"));
        assert_eq!(text.contains("\r\n"), crlf);
        if crlf {
            assert!(
                text.as_bytes()
                    .iter()
                    .enumerate()
                    .filter(|(_, b)| **b == b'\n')
                    .all(|(i, _)| i > 0 && text.as_bytes()[i - 1] == b'\r')
            );
        }
        let actual_entities: Vec<_> = graph.entities.iter().filter(|e| e.path == PATH).collect();
        assert_eq!(
            actual_entities.len(),
            17,
            "Complete independently enumerated source entity set"
        );
        assert_eq!(
            actual_entities
                .iter()
                .map(|e| e.id.clone())
                .collect::<BTreeSet<_>>(),
            expected.entities.values().map(|e| e.id.clone()).collect()
        );
        for entity in expected.entities.values() {
            let rows: Vec<_> = actual_entities
                .iter()
                .filter(|e| e.id == entity.id)
                .collect();
            assert_eq!(rows.len(), 1);
            let row = rows[0];
            assert_eq!(row.key, entity.key);
            assert_eq!(row.snapshot_id, snapshot.id);
            assert_eq!(row.kind, entity.kind);
            assert_eq!(row.name, entity.name);
            assert_eq!(row.language, "typescript");
            assert_eq!(row.producer, yoke::model::ANALYZER);
            assert_eq!((row.start_byte, row.end_byte), (entity.start, entity.end));
            assert_eq!(
                (row.name_start_byte, row.name_end_byte),
                (entity.name_start, entity.name_end)
            );
            assert_eq!(
                row.parent_key.as_deref(),
                entity
                    .owner
                    .map(|name| expected.entities[name].key.as_str())
            );
            let range = fixture::expected_range(text, entity.start, entity.end);
            assert_eq!(
                row.start_line,
                range["startLine"].as_u64().unwrap() as usize
            );
            assert_eq!(row.end_line, range["endLine"].as_u64().unwrap() as usize);
            if let Some(start) = entity.name_start {
                assert_eq!(&text[start..entity.name_end.unwrap()], entity.name);
            }
            if let Some(owner) = entity.owner {
                let parent = &expected.entities[owner];
                assert!(parent.start <= entity.start && entity.end <= parent.end);
            }
        }
        let hierarchy_ids: BTreeSet<_> = graph
            .observations
            .iter()
            .filter(|o| matches!(o.relation.as_str(), "extends" | "implements" | "inherits"))
            .filter(|o| actual_entities.iter().any(|entity| entity.id == o.source))
            .map(|o| o.id.clone())
            .collect();
        assert_eq!(
            hierarchy_ids,
            expected.hierarchy.iter().map(|o| o.id.clone()).collect()
        );
        let actual_calls: BTreeSet<_> = graph
            .observations
            .iter()
            .filter(|o| o.relation == "calls")
            .filter(|o| actual_entities.iter().any(|entity| entity.id == o.source))
            .map(|o| o.id.clone())
            .collect();
        assert_eq!(actual_calls.len(), 11);
        assert_eq!(
            actual_calls,
            expected.calls.iter().map(|o| o.id.clone()).collect()
        );
        for target in [
            "Base.save",
            "Service.save",
            "Derived.save",
            "Sibling.save",
            "Unrelated.save",
        ] {
            let calls = expected.calls_to(target);
            assert_eq!(calls.len(), 2, "Two literal occurrences for {target}");
            assert_ne!(calls[0].id, calls[1].id);
            assert_ne!(calls[0].evidence_id, calls[1].evidence_id);
            assert_ne!(
                (calls[0].start, calls[0].end),
                (calls[1].start, calls[1].end)
            );
            assert_eq!(
                &text[calls[0].start..calls[0].end],
                &text[calls[1].start..calls[1].end]
            );
        }
        assert!(
            expected
                .derived_ancestor_member_ids()
                .is_disjoint(&expected.derived_excluded_member_ids())
        );
        assert!(
            expected
                .base_descendant_member_ids()
                .contains(expected.id("Sibling.save"))
        );
        // These literal oracle sets are for later root-specific behavior tests.
        // No captured-hierarchy expansion or affected-result API is invoked here.
        expected_changed.extend(expected.expected_changed_claims(label, edit));
    }
    assert_eq!(
        f.report["changedSymbols"],
        serde_json::json!(expected_changed),
        "Complete original method AND enclosing-class seeds on both sides"
    );
    assert_eq!(
        git_workflows::load(&f.engine.store, f.id(), &Deadline::new(30_000)).unwrap(),
        f.report
    );
    assert_eq!(state(&f.engine), before);
    assert_eq!(
        std::fs::read(f.dir.path().join(".git/index")).unwrap(),
        f.index_bytes
    );
    assert_eq!(
        std::fs::read_to_string(f.dir.path().join(PATH)).unwrap(),
        f.text("head", PATH)
    );
    assert_eq!(f.report["complete"], false);
    if edit == Edit::None {
        for field in ["changes", "changedSymbols", "impact", "impactObservations"] {
            assert!(
                f.report[field].as_array().unwrap().is_empty(),
                "No-change {field}"
            );
        }
    } else {
        let changes = f.report["changes"].as_array().unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0]["basePath"], PATH);
        assert_eq!(changes[0]["headPath"], PATH);
        assert_eq!(changes[0]["status"], "modified");
        assert_eq!(changes[0]["hunks"].as_array().unwrap().len(), 1);
    }
}

#[test]
fn released_git_derived_body_fixture_qualifies_exact_crlf_unicode_owners_hierarchy_and_parallel_calls()
 {
    qualify(Edit::Derived, true);
}

#[test]
fn released_git_base_body_fixture_qualifies_reverse_descendant_inputs_without_sibling_shortcuts() {
    qualify(Edit::Base, false);
}

#[test]
fn released_git_no_change_fixture_retains_real_hierarchy_and_calls_with_zero_original_seeds() {
    qualify(Edit::None, true);
}
