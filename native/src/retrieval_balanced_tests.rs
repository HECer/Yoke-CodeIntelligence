//! Literal arithmetic oracles; these rows have no source-access authority.
use super::*;
fn d() -> Deadline {
    Deadline::new(30000)
}
fn unit(scope: &str, path: &str, id: &str, cost: usize, start: usize) -> AdmissionUnit {
    AdmissionUnit {
        scope_root: scope.into(),
        path: path.into(),
        entity_id: id.into(),
        cost,
        start_byte: start,
        end_byte: start + cost,
    }
}
fn names<'a>(units: &'a [AdmissionUnit], order: &[usize]) -> Vec<&'a str> {
    order.iter().map(|i| units[*i].entity_id.as_str()).collect()
}
#[test]
fn literal_remainder_has_protected_three_two_two_then_pools_one_whole_unit() {
    let units = vec![
        unit("a", "a.rs", "a", 3, 0),
        unit("b", "b.rs", "b", 3, 0),
        unit("c", "c.rs", "c", 1, 0),
    ];
    let plan = schedule_whole_units(&units, 7, &d()).expect("Rounding schedule must be available");
    assert_eq!(
        plan.allowances,
        BTreeMap::from([("a".into(), 3), ("b".into(), 2), ("c".into(), 2)])
    );
    assert_eq!(names(&units, &plan.phase1), ["a", "c"]);
    assert_eq!(names(&units, &plan.phase2), ["b"]);
    assert!(plan.omitted.is_empty());
    assert_eq!(plan.used, 7);
}
#[test]
fn literal_source_28m_schedule_defers_a_large_head_and_keeps_other_packages() {
    let units = vec![
        unit("a", "a/00-large.rs", "a-large", 4_000_000, 0),
        unit("a", "a/01-small.rs", "a-small", 500_000, 0),
        unit("b", "b/00.rs", "b0", 3_000_000, 0),
        unit("b", "b/01.rs", "b1", 500_000, 0),
        unit("b", "b/02.rs", "b2", 1_000_000, 0),
        unit("c", "c/00.rs", "c0", 1_000_000, 0),
        unit("c", "c/01.rs", "c1", 3_000_000, 0),
        unit("d", "d/00.rs", "d", 3_500_000, 0),
        unit("e", "e/00.rs", "e", 3_500_000, 0),
        unit("f", "f/00.rs", "f", 3_500_000, 0),
        unit("g", "g/00.rs", "g", 3_500_000, 0),
        unit("h", "h/00.rs", "h", 3_500_000, 0),
    ];
    let p = schedule_whole_units(&units, 28_000_000, &d())
        .expect("Source unit schedule must be available");
    assert!(p.allowances.values().all(|n| *n == 3_500_000));
    assert_eq!(
        names(&units, &p.phase1),
        ["a-small", "b0", "c0", "d", "e", "f", "g", "h", "b1"]
    );
    assert_eq!(names(&units, &p.phase2), ["a-large", "b2"]);
    assert_eq!(names(&units, &p.omitted), ["c1"]);
    assert_eq!(p.used, 27_500_000);
    assert_eq!(4_000_000 + p.used, 31_500_000);
    // Reverse both scope and per-scope unit order. Indices must remap, while
    // the independently pinned canonical name/phase oracles remain literal.
    let reversed = units.iter().rev().cloned().collect::<Vec<_>>();
    let p = schedule_whole_units(&reversed, 28_000_000, &d()).unwrap();
    assert_eq!(
        names(&reversed, &p.phase1),
        ["a-small", "b0", "c0", "d", "e", "f", "g", "h", "b1"]
    );
    assert_eq!(names(&reversed, &p.phase2), ["a-large", "b2"]);
    assert_eq!(names(&reversed, &p.omitted), ["c1"]);
    assert_eq!(p.used, 27_500_000);
}
#[test]
fn literal_document_32m_charges_overlapping_child_bodies_as_distinct_units() {
    let units = ["a", "b", "c", "d", "e", "f", "g", "h"]
        .iter()
        .flat_map(|scope| {
            let parent = if *scope == "a" { 4_000_000 } else { 3_000_000 };
            [
                unit(
                    scope,
                    &format!("{scope}/main.rs"),
                    &format!("{scope}-parent"),
                    parent,
                    0,
                ),
                unit(
                    scope,
                    &format!("{scope}/main.rs"),
                    &format!("{scope}-child"),
                    1_000_000,
                    100,
                ),
            ]
        })
        .collect::<Vec<_>>();
    let p = schedule_whole_units(&units, 32_000_000, &d())
        .expect("Document unit schedule must be available");
    assert_eq!(
        names(&units, &p.phase1),
        [
            "a-parent", "b-parent", "c-parent", "d-parent", "e-parent", "f-parent", "g-parent",
            "h-parent", "b-child", "c-child", "d-child", "e-child", "f-child", "g-child",
            "h-child"
        ]
    );
    assert!(p.phase2.is_empty());
    assert_eq!(names(&units, &p.omitted), ["a-child"]);
    assert_eq!(p.used, 32_000_000);
    assert_eq!(p.phase1.len(), 15);
}
#[test]
fn zero_cost_units_are_finite_and_an_oversized_head_cannot_block_a_later_unit() {
    let units = vec![
        unit("a", "a/00", "oversize", 8, 0),
        unit("a", "a/01", "empty-a", 0, 0),
        unit("a", "a/02", "small-a", 2, 0),
        unit("b", "b/00", "empty-b", 0, 0),
        unit("b", "b/01", "small-b", 1, 0),
    ];
    let p = schedule_whole_units(&units, 3, &d()).unwrap();
    assert_eq!(
        names(&units, &p.phase1),
        ["empty-a", "empty-b", "small-a", "small-b"]
    );
    assert_eq!(names(&units, &p.omitted), ["oversize"]);
    assert_eq!(p.used, 3);
}
fn row(id: &str, scope: &str, path: &str, score: f64, coverage: usize) -> ScoredRow {
    ScoredRow {
        entity_id: id.into(),
        scope_root: scope.into(),
        path: path.into(),
        score,
        query_coverage_count: coverage,
    }
}
#[test]
fn literal_fusion_prioritizes_local_rank_then_coverage_before_cross_scope_score() {
    let rows = vec![
        row("a-head", "a", "a/one", 1.0, 1),
        row("a-tail", "a", "a/two", 0.9, 2),
        row("b-head", "b", "b/one", 0.8, 2),
        row("c-head", "c", "c/one", 0.7, 2),
    ];
    let p = fuse_rows(&rows, 200, 4, &d()).expect("Fusion must be available");
    assert_eq!(
        p.selected
            .iter()
            .map(|r| rows[r.index].entity_id.as_str())
            .collect::<Vec<_>>(),
        ["b-head", "c-head", "a-head", "a-tail"]
    );
    assert_eq!(
        p.selected.iter().map(|r| r.local_rank).collect::<Vec<_>>(),
        [1, 1, 1, 2]
    );
    assert_eq!(p.positive_candidate_count, 4);
    assert_eq!(p.per_file_rejected_count, 0);
    assert_eq!(p.final_limit_omission_count, 0);
}
#[test]
fn per_file_rejections_are_exhaustive_even_beyond_the_final_limit() {
    let rows = vec![
        row("a0", "a", "a/same", 1.0, 1),
        row("a1", "a", "a/same", 0.8, 1),
        row("a2", "a", "a/same", 0.7, 1),
        row("b0", "b", "b/one", 0.9, 1),
        row("b1", "b", "b/two", 0.6, 1),
    ];
    let p = fuse_rows(&rows, 1, 1, &d()).unwrap();
    assert_eq!(p.positive_candidate_count, 5);
    assert_eq!(p.per_file_rejected_count, 2);
    assert_eq!(p.final_limit_omission_count, 2);
    assert_eq!(
        p.selected,
        vec![RankedIndex {
            index: 0,
            local_rank: 1
        }]
    );
    let all = fuse_rows(&rows, 1, 5, &d()).unwrap();
    assert_eq!(
        all.selected,
        vec![
            RankedIndex {
                index: 0,
                local_rank: 1
            },
            RankedIndex {
                index: 3,
                local_rank: 1
            },
            RankedIndex {
                index: 4,
                local_rank: 2
            }
        ]
    );
}
#[test]
fn local_bm25_matches_the_independent_31_document_literal_oracle() {
    let mut docs = vec![
        LexicalDocument {
            entity_id: "big_first".into(),
            weighted_length: 26,
            term_counts: vec![3],
        },
        LexicalDocument {
            entity_id: "big_second".into(),
            weighted_length: 25,
            term_counts: vec![2],
        },
    ];
    docs.extend((3..=30).map(|i| LexicalDocument {
        entity_id: format!("noise_{i:02}"),
        weighted_length: 24,
        term_counts: vec![1],
    }));
    docs.push(LexicalDocument {
        entity_id: "small_only".into(),
        weighted_length: 124,
        term_counts: vec![1],
    });
    let scores = scope_bm25(&docs, &d()).unwrap();
    for (id, expected) in [
        ("big_first", 0.025006807306715315),
        ("big_second", 0.0221843728261494),
        ("noise_03", 0.016572817741249465),
        ("small_only", 0.006434426376308555),
    ] {
        assert!(
            (scores[id] - expected).abs() < 1e-12,
            "{id}: {} vs {expected}",
            scores[id]
        );
    }
}
#[test]
fn equal_scope_seeds_preserve_zero_mass_scopes_and_equalize_positive_scope_mass() {
    let rows = vec![
        LexicalMassRow {
            entity_id: "a1".into(),
            scope_root: "a".into(),
            bm25: 3.0,
        },
        LexicalMassRow {
            entity_id: "a2".into(),
            scope_root: "a".into(),
            bm25: 1.0,
        },
        LexicalMassRow {
            entity_id: "b".into(),
            scope_root: "b".into(),
            bm25: 2.0,
        },
        LexicalMassRow {
            entity_id: "c".into(),
            scope_root: "c".into(),
            bm25: 0.0,
        },
    ];
    let s = equal_scope_seeds(&rows, &d()).unwrap();
    assert_eq!(
        s,
        BTreeMap::from([
            ("a1".into(), 0.375),
            ("a2".into(), 0.125),
            ("b".into(), 0.5),
            ("c".into(), 0.0)
        ])
    );
}
fn observations() -> Vec<ResolvedObservation> {
    [
        ("o1", "a", Some("b")),
        ("o2", "a", Some("b")),
        ("o3", "a", Some("c")),
    ]
    .into_iter()
    .map(|(id, s, t)| ResolvedObservation {
        observation_id: id.into(),
        source: s.into(),
        target: t.map(str::to_owned),
    })
    .collect()
}
#[test]
fn canonical_arcs_keep_parallel_observations_and_two_self_loop_orientations() {
    let mut observations = observations();
    observations.push(ResolvedObservation {
        observation_id: "o4".into(),
        source: "a".into(),
        target: Some("a".into()),
    });
    observations.push(ResolvedObservation {
        observation_id: "unresolved".into(),
        source: "a".into(),
        target: None,
    });
    observations.push(ResolvedObservation {
        observation_id: "not-admitted".into(),
        source: "a".into(),
        target: Some("outside".into()),
    });
    let arcs = canonical_arcs(&["c".into(), "a".into(), "b".into()], &observations, &d()).unwrap();
    let got = arcs
        .iter()
        .map(|a| {
            (
                a.source.as_str(),
                a.target.as_str(),
                a.observation_id.as_str(),
                a.orientation,
            )
        })
        .collect::<Vec<_>>();
    use Orientation::{Forward as F, Reverse as R};
    assert_eq!(
        got,
        [
            ("a", "a", "o4", F),
            ("a", "a", "o4", R),
            ("a", "b", "o1", F),
            ("a", "b", "o2", F),
            ("a", "c", "o3", F),
            ("b", "a", "o1", R),
            ("b", "a", "o2", R),
            ("c", "a", "o3", R)
        ]
    );
}
#[test]
fn graph_propagation_preserves_literal_parallel_degree_ratio_after_24_iterations() {
    let vertices = vec!["a".into(), "b".into(), "c".into()];
    let arcs = canonical_arcs(&vertices, &observations(), &d()).unwrap();
    let seeds = BTreeMap::from([("a".into(), 1.0), ("b".into(), 0.0), ("c".into(), 0.0)]);
    let p = propagate(&vertices, &arcs, &seeds, true, &d()).unwrap();
    assert_eq!(p.iterations, 24);
    for (id, expected) in [
        ("a", 0.5498366539741663),
        ("b", 0.30010889735055557),
        ("c", 0.15005444867527779),
    ] {
        assert!(
            (p.ranks[id] - expected).abs() < 1e-12,
            "{id}: {}",
            p.ranks[id]
        );
    }
}
#[test]
fn zero_graph_weight_or_zero_lexical_mass_skips_iterations_and_returns_zero_raw_graph_scores() {
    let vertices = vec!["a".into(), "b".into(), "c".into()];
    let arcs = canonical_arcs(&vertices, &observations(), &d()).unwrap();
    for (seeds, enabled) in [
        (
            BTreeMap::from([("a".into(), 1.0), ("b".into(), 0.0), ("c".into(), 0.0)]),
            false,
        ),
        (
            BTreeMap::from([("a".into(), 0.0), ("b".into(), 0.0), ("c".into(), 0.0)]),
            true,
        ),
    ] {
        let p = propagate(&vertices, &arcs, &seeds, enabled, &d()).unwrap();
        assert_eq!(p.iterations, 0);
        assert!(p.ranks.values().all(|s| *s == 0.0));
    }
}
#[test]
fn equal_fusion_ties_use_exact_root_then_entity_id_and_reject_nonfinite_scores() {
    let rows = vec![
        row("z", "a", "a/z", 1.0, 1),
        row("a", "b", "b/a", 1.0, 1),
        row("a", "a", "a/a", 1.0, 1),
    ];
    let p = fuse_rows(&rows, 200, 3, &d()).unwrap();
    assert_eq!(
        p.selected
            .iter()
            .map(|r| rows[r.index].entity_id.as_str())
            .collect::<Vec<_>>(),
        ["a", "a", "z"]
    );
    assert_eq!(
        p.selected
            .iter()
            .map(|r| rows[r.index].scope_root.as_str())
            .collect::<Vec<_>>(),
        ["a", "b", "a"]
    );
    assert!(fuse_rows(&[row("x", "a", "a/x", f64::NAN, 1)], 1, 1, &d()).is_err());
}
#[test]
fn optional_metadata_count_caps_keep_literal_prefixes_and_all_omitted_counts() {
    let rows = MetadataCostRows {
        diagnostics: vec![1; 70],
        scopes: vec![1; 66],
        declaration_bindings: vec![1; 133],
        source_omissions: vec![1; 201],
    };
    let p = plan_optional_metadata(&rows, 256_000, &d()).unwrap();
    assert_eq!(p.kept, [64, 64, 128, 200]);
    assert_eq!(p.omitted, [6, 2, 5, 1]);
    assert_eq!(p.optional_bytes_used, 456);
}
#[test]
fn optional_metadata_256kb_cost_ceiling_admits_literal_62_rows_without_partial_rows() {
    let rows = MetadataCostRows {
        diagnostics: vec![4096; 70],
        scopes: vec![4096; 66],
        declaration_bindings: vec![4096; 133],
        source_omissions: vec![4096; 201],
    };
    let p = plan_optional_metadata(&rows, 256_000, &d()).unwrap();
    assert_eq!(p.kept, [62, 0, 0, 0]);
    assert_eq!(p.omitted, [8, 66, 133, 201]);
    assert_eq!(p.optional_bytes_used, 253_952);
}
#[test]
fn optional_metadata_respects_smaller_caller_remainder_in_diagnostic_first_order() {
    let rows = MetadataCostRows {
        diagnostics: vec![4; 70],
        scopes: vec![4; 66],
        declaration_bindings: vec![4; 133],
        source_omissions: vec![4; 201],
    };
    let p = plan_optional_metadata(&rows, 200, &d()).unwrap();
    assert_eq!(p.kept, [50, 0, 0, 0]);
    assert_eq!(p.omitted, [20, 66, 133, 201]);
    assert_eq!(p.optional_bytes_used, 200);
}
