//! Private kernel adversaries. These literal inputs are not checked Git authority.
use super::index::Index;
use super::source::SourceCache;
use super::walk::{self, Direction, WalkResult};
use super::{AffectedOptions, Budget, Capacity, Consumed, Eligibility, Side, SourceAccess};
use crate::analysis::AnalyzerEpoch;
use crate::capture::Deadline;
use crate::model::{Entity, Evidence, Graph, Observation, Snapshot, SourceFile, hash};
use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

const PATH: &str = "src/model.ts";
const BASE: &str = "f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0";
const HEAD: &str = "f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1";
const REPORT: &str = "f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2";
const WORKSPACE: &str = "f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3";
const GRAPH_HASH: &str = "f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4";
const FILE: &str = "a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0";
const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const B: &str = "3333333333333333333333333333333333333333333333333333333333333333";
const C: &str = "2222222222222222222222222222222222222222222222222222222222222222";
const T: &str = "4444444444444444444444444444444444444444444444444444444444444444";
const AM: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const BM: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
const CM: &str = "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";
const TM: &str = "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
const CALLER: &str = "9999999999999999999999999999999999999999999999999999999999999999";
const MIDDLE: &str = "8888888888888888888888888888888888888888888888888888888888888888";
const O0: &str = "0000000000000000000000000000000000000000000000000000000000000000";
const O1: &str = "1111111111111111111111111111111111111111111111111111111111111111";
const O2: &str = "2222222222222222222222222222222222222222222222222222222222222222";
const O3: &str = "3333333333333333333333333333333333333333333333333333333333333333";
const O4: &str = "4444444444444444444444444444444444444444444444444444444444444444";
const O5: &str = "5555555555555555555555555555555555555555555555555555555555555555";
const O6: &str = "6666666666666666666666666666666666666666666666666666666666666666";
const OF: &str = "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";
const E1: &str = "6161616161616161616161616161616161616161616161616161616161616161";
const E2: &str = "6262626262626262626262626262626262626262626262626262626262626262";
const UNUSED_E: &str = "6363636363636363636363636363636363636363636363636363636363636363";
const TEXT: &str = "class A { save() {} }\nclass B { save() {} }\nclass C { save() {} }\nclass T { save() {} }\nfunction caller() {}\nfunction middle() {}\n";

#[derive(Clone)]
struct LiteralSide {
    snapshot: Snapshot,
    graph: Graph,
    blobs: BTreeMap<String, Vec<u8>>,
}

struct LiteralAccess {
    sides: [LiteralSide; 2],
    changed: Vec<Value>,
    impact: Vec<Value>,
    impact_observations: Vec<Value>,
}

impl SourceAccess for LiteralAccess {
    fn report_artifact_id(&self) -> &str {
        REPORT
    }
    fn workspace_id(&self) -> &str {
        WORKSPACE
    }
    fn snapshot(&self, side: Side) -> &Snapshot {
        &self.sides[side.index()].snapshot
    }
    fn graph(&self, side: Side) -> &Graph {
        &self.sides[side.index()].graph
    }
    fn graph_hash(&self, _side: Side) -> &str {
        GRAPH_HASH
    }
    fn analyzer_epoch(&self, _side: Side) -> AnalyzerEpoch {
        AnalyzerEpoch::V8
    }
    fn file(&self, side: Side, path: &str) -> Result<&SourceFile> {
        self.snapshot(side)
            .files
            .iter()
            .find(|file| file.path == path)
            .context("Literal path is not in this side's manifest")
    }
    fn read_source(&self, side: Side, path: &str) -> Result<Vec<u8>> {
        self.sides[side.index()]
            .blobs
            .get(path)
            .cloned()
            .context("Literal source was not supplied")
    }
    fn changed_symbols(&self) -> &[Value] {
        &self.changed
    }
    fn impact(&self) -> &[Value] {
        &self.impact
    }
    fn impact_observations(&self) -> &[Value] {
        &self.impact_observations
    }
}

// Fixture positions are located in literal bytes, without the parser or validator.
fn line(text: &str, byte: usize) -> usize {
    text.as_bytes()[..byte]
        .iter()
        .filter(|byte| **byte == b'\n')
        .count()
        + 1
}

fn entity(
    text: &str,
    id: &str,
    key: &str,
    name: &str,
    kind: &str,
    range: (usize, usize),
    name_range: Option<(usize, usize)>,
    parent: Option<&str>,
) -> Entity {
    Entity {
        id: id.into(),
        key: key.into(),
        snapshot_id: BASE.into(),
        path: PATH.into(),
        name: name.into(),
        qualified_name: format!("untrusted/{name}"),
        kind: kind.into(),
        language: "typescript".into(),
        signature: String::new(),
        start_byte: range.0,
        end_byte: range.1,
        start_line: line(text, range.0),
        end_line: line(text, range.1.saturating_sub(1)),
        name_start_byte: name_range.map(|range| range.0),
        name_end_byte: name_range.map(|range| range.1),
        parent_key: parent.map(str::to_owned),
        producer: "yoke-tree-sitter-8".into(),
    }
}

fn literal(text: &str) -> LiteralAccess {
    let file = SourceFile {
        path: PATH.into(),
        hash: hash(text),
        size: text.len(),
        language: "typescript".into(),
        mode: 0o100644,
    };
    let snapshot = Snapshot {
        id: BASE.into(),
        workspace_id: WORKSPACE.into(),
        kind: "git".into(),
        files: vec![file],
        omissions: Vec::new(),
        capture_consistency: "strict".into(),
    };
    let mut graph = Graph {
        snapshot_id: BASE.into(),
        ..Graph::default()
    };
    graph.entities.push(entity(
        text,
        FILE,
        "file",
        PATH,
        "file",
        (0, text.len()),
        None,
        None,
    ));
    for (name, id, method_id) in [("A", A, AM), ("B", B, BM), ("C", C, CM), ("T", T, TM)] {
        let pattern = format!("class {name} {{");
        let Some(start) = text.find(&pattern) else {
            continue;
        };
        let end = text[start..]
            .find('\n')
            .map_or(text.len(), |end| start + end);
        let key = format!("class:{name}");
        graph.entities.push(entity(
            text,
            id,
            &key,
            name,
            "class",
            (start, end),
            Some((start + 6, start + 6 + name.len())),
            Some("file"),
        ));
        if let Some(offset) = text[start..end].find("save()") {
            let method = start + offset;
            graph.entities.push(entity(
                text,
                method_id,
                &format!("method:{name}.save"),
                "save",
                "method",
                (method, method + "save() {}".len()),
                Some((method, method + 4)),
                Some(&key),
            ));
        }
    }
    for (name, id) in [("caller", CALLER), ("middle", MIDDLE)] {
        let pattern = format!("function {name}()");
        let Some(start) = text.find(&pattern) else {
            continue;
        };
        let end = text[start..]
            .find('\n')
            .map_or(text.len(), |end| start + end);
        graph.entities.push(entity(
            text,
            id,
            &format!("function:{name}"),
            name,
            "function",
            (start, end),
            Some((start + 9, start + 9 + name.len())),
            Some("file"),
        ));
    }
    let base = LiteralSide {
        snapshot,
        graph,
        blobs: [(PATH.into(), text.as_bytes().to_vec())].into(),
    };
    let mut head = base.clone();
    head.snapshot.id = HEAD.into();
    head.graph = Graph {
        snapshot_id: HEAD.into(),
        ..Graph::default()
    };
    let mut access = LiteralAccess {
        sides: [base, head],
        changed: vec![json!({"side":"base","id":AM})],
        impact: Vec::new(),
        impact_observations: Vec::new(),
    };
    let anchor = evidence(E1, &access);
    access.sides[0].graph.evidence.push(anchor);
    access
}

fn row_mut<'a>(access: &'a mut LiteralAccess, id: &str) -> &'a mut Entity {
    access.sides[0]
        .graph
        .entities
        .iter_mut()
        .find(|row| row.id == id)
        .unwrap()
}

fn entity_index(access: &LiteralAccess, id: &str) -> usize {
    access.sides[0]
        .graph
        .entities
        .iter()
        .position(|row| row.id == id)
        .unwrap()
}

fn observation(id: &str, source: &str, target: Option<&str>, relation: &str) -> Observation {
    Observation {
        id: id.into(),
        snapshot_id: BASE.into(),
        source: source.into(),
        target: target.map(str::to_owned),
        target_label: "captured target label".into(),
        relation: relation.into(),
        resolution: "ambiguous".into(),
        evidence_ids: vec![E1.into()],
        producer: "yoke-tree-sitter-8".into(),
    }
}

fn evidence(id: &str, access: &LiteralAccess) -> Evidence {
    Evidence {
        id: id.into(),
        snapshot_id: BASE.into(),
        path: PATH.into(),
        content_hash: access.sides[0].snapshot.files[0].hash.clone(),
        start_byte: 6,
        end_byte: 7,
        start_line: 1,
        end_line: 1,
        anchor: "exact".into(),
        trust: "captured".into(),
        producer: "yoke-tree-sitter-8".into(),
    }
}

fn walk_options(hierarchy: usize, incoming: usize) -> AffectedOptions {
    AffectedOptions {
        max_hierarchy_depth: hierarchy,
        max_incoming_depth: incoming,
        ..AffectedOptions::default()
    }
}

fn walk_literal(
    access: &LiteralAccess,
    options: &AffectedOptions,
) -> Result<(WalkResult, Consumed)> {
    let deadline = Deadline::new(30_000);
    super::validate_unique_entities(access, &deadline)?;
    let mut budget = Budget::new(options, &deadline);
    let indexes = [
        Index::build(Side::Base, access, &mut budget)?,
        Index::build(Side::Head, access, &mut budget)?,
    ];
    let mut sources = SourceCache::new(access);
    let walked = walk::derive(access, &indexes, &mut sources, &mut budget)?;
    Ok((walked, budget.counts))
}

fn finish_literal(access: &LiteralAccess, options: &AffectedOptions) -> Result<(Value, Vec<u8>)> {
    let deadline = Deadline::new(30_000);
    finish_literal_with_deadline(access, options, &deadline)
}

fn finish_literal_with_deadline(
    access: &LiteralAccess,
    options: &AffectedOptions,
    deadline: &Deadline,
) -> Result<(Value, Vec<u8>)> {
    super::validate_unique_entities(access, deadline)?;
    let mut budget = Budget::new(options, deadline);
    let indexes = [
        Index::build(Side::Base, access, &mut budget)?,
        Index::build(Side::Head, access, &mut budget)?,
    ];
    let mut sources = SourceCache::new(access);
    let walked = walk::derive(access, &indexes, &mut sources, &mut budget)?;
    super::output::finish(access, &indexes, &mut sources, &walked, &mut budget)
}

fn eligibility(access: &LiteralAccess, id: &str) -> Result<Eligibility> {
    let options = AffectedOptions::default();
    let deadline = Deadline::new(30_000);
    let mut budget = Budget::new(&options, &deadline);
    let index = Index::build(Side::Base, access, &mut budget)?;
    SourceCache::new(access).eligibility(&index, entity_index(access, id), &mut budget)
}

#[test]
fn global_duplicate_keys_and_ids_refuse_even_when_unreached_or_identical() {
    let baseline = literal(TEXT);
    assert!(walk_literal(&baseline, &walk_options(0, 0)).is_ok());
    for conflict in [false, true] {
        let mut access = literal(TEXT);
        let mut duplicate = row_mut(&mut access, T).clone();
        if conflict {
            duplicate.id = O0.into();
            duplicate.name = "unreached conflict".into();
        }
        access.sides[0].graph.entities.push(duplicate);
        assert!(walk_literal(&access, &walk_options(0, 0)).is_err());
    }
    let mut access = literal(TEXT);
    row_mut(&mut access, T).id = A.into();
    assert!(walk_literal(&access, &walk_options(0, 0)).is_err());
    let mut access = literal(TEXT);
    access.sides[0].graph.observations = vec![
        observation(O1, B, Some(T), "extends"),
        observation(O1, C, Some(T), "extends"),
    ];
    assert!(walk_literal(&access, &walk_options(0, 0)).is_err());
    let mut access = literal(TEXT);
    let row = evidence(E1, &access);
    access.sides[0].graph.evidence = vec![row.clone(), row];
    assert!(walk_literal(&access, &walk_options(0, 0)).is_err());
}

#[test]
fn identical_ids_on_two_sides_have_separate_index_and_walk_identity() {
    let mut access = literal(TEXT);
    access.sides[1] = access.sides[0].clone();
    access.changed.push(json!({"side":"head","id":AM}));
    let (walked, counts) = walk_literal(&access, &walk_options(0, 0)).unwrap();
    assert_eq!(walked.originals.len(), 2);
    assert_eq!(walked.incoming.len(), 2);
    assert_eq!(counts.incoming_states, 2);
    assert_eq!(
        walked
            .incoming
            .iter()
            .map(|state| state.side)
            .collect::<BTreeSet<_>>(),
        [Side::Base, Side::Head].into()
    );
}

#[test]
fn resolved_parent_integrity_precedes_unsupported_language_or_kind() {
    let access = literal(TEXT);
    let options = AffectedOptions::default();
    let deadline = Deadline::new(30_000);
    let mut budget = Budget::new(&options, &deadline);
    let index = Index::build(Side::Base, &access, &mut budget).unwrap();
    // A class with a captured file parent is a valid proof endpoint.
    SourceCache::new(&access)
        .validate_entity(&index, entity_index(&access, A), &mut budget)
        .unwrap();
    for damage in ["snapshot", "path", "containment"] {
        let mut access = literal(TEXT);
        row_mut(&mut access, AM).language = "python".into();
        row_mut(&mut access, AM).kind = "function".into();
        match damage {
            "snapshot" => row_mut(&mut access, A).snapshot_id = HEAD.into(),
            "path" => {
                let mut second = access.sides[0].snapshot.files[0].clone();
                second.path = "src/other.ts".into();
                access.sides[0].snapshot.files.push(second);
                access.sides[0]
                    .blobs
                    .insert("src/other.ts".into(), TEXT.as_bytes().to_vec());
                row_mut(&mut access, A).path = "src/other.ts".into();
            }
            _ => {
                let child_start = row_mut(&mut access, AM).start_byte;
                row_mut(&mut access, A).end_byte = child_start;
            }
        }
        assert!(eligibility(&access, AM).is_err(), "parent {damage}");
    }
}

#[test]
fn eligibility_reasons_are_ordered_and_absent_parent_is_never_inferred() {
    for (case, want) in [
        ("language", "unsupported_language"),
        ("kind", "unsupported_entity_kind"),
        ("absent", "missing_parent"),
        ("not_found", "missing_parent"),
        ("owner", "unsupported_owner"),
        ("name", "missing_name_anchor"),
    ] {
        let mut access = literal(TEXT);
        match case {
            "language" => {
                row_mut(&mut access, AM).language = "tsx".into();
                row_mut(&mut access, AM).kind = "function".into();
            }
            "kind" => {
                row_mut(&mut access, AM).kind = "function".into();
                row_mut(&mut access, AM).parent_key = None;
            }
            "absent" => row_mut(&mut access, AM).parent_key = None,
            "not_found" => {
                row_mut(&mut access, AM).parent_key =
                    Some("qualified/name/does/not/authorize/owner".into())
            }
            "owner" => row_mut(&mut access, A).kind = "function".into(),
            _ => {
                row_mut(&mut access, AM).name_start_byte = None;
                row_mut(&mut access, AM).name_end_byte = None;
            }
        }
        assert_eq!(
            eligibility(&access, AM).unwrap(),
            Eligibility::Unsupported(want),
            "{case}"
        );
    }
    let access = literal(TEXT);
    assert_eq!(
        eligibility(&access, AM).unwrap(),
        Eligibility::Expandable {
            owner: entity_index(&access, A)
        }
    );
}

#[test]
fn partial_misplaced_utf8_and_misspelled_names_are_input_corruption() {
    for damage in ["half", "outside", "spelling"] {
        let mut access = literal(TEXT);
        row_mut(&mut access, AM).language = "tsx".into();
        match damage {
            "half" => row_mut(&mut access, AM).name_end_byte = None,
            "outside" => {
                row_mut(&mut access, AM).name_start_byte = Some(6);
                row_mut(&mut access, AM).name_end_byte = Some(7);
            }
            _ => row_mut(&mut access, AM).name = "Save".into(),
        }
        assert!(eligibility(&access, AM).is_err(), "{damage}");
    }
    let text = "class A { café() {} }\r\n";
    let mut access = literal(text);
    let start = text.find("café").unwrap();
    access.sides[0].graph.entities.push(entity(
        text,
        AM,
        "method:A.cafe",
        "café",
        "method",
        (start, start + "café() {}".len()),
        Some((start, start + "café".len())),
        Some("class:A"),
    ));
    assert!(matches!(
        eligibility(&access, AM).unwrap(),
        Eligibility::Expandable { .. }
    ));
    row_mut(&mut access, AM).name_end_byte = Some(start + 4); // Inside the two-byte é.
    assert!(eligibility(&access, AM).is_err());
    row_mut(&mut access, AM).name_end_byte = Some(start + 5);
    row_mut(&mut access, AM).name = "cafe\u{301}".into();
    assert!(eligibility(&access, AM).is_err());
}

#[test]
fn exact_source_proven_constructor_is_excluded_without_staticness_inference() {
    let text = "class A { constructor() {} }\n";
    let mut access = literal(text);
    let start = 10;
    access.sides[0].graph.entities.push(entity(
        text,
        AM,
        "method:A.constructor",
        "constructor",
        "method",
        (start, start + "constructor() {}".len()),
        Some((start, start + 11)),
        Some("class:A"),
    ));
    assert_eq!(
        eligibility(&access, AM).unwrap(),
        Eligibility::Unsupported("constructor_name_not_expanded")
    );
    row_mut(&mut access, AM).name_end_byte = None;
    assert!(eligibility(&access, AM).is_err());
    let mut ordinary = literal(TEXT);
    row_mut(&mut ordinary, AM).signature = "private static save<T>()".into();
    assert!(matches!(
        eligibility(&ordinary, AM).unwrap(),
        Eligibility::Expandable { .. }
    ));
}

#[test]
fn full_sequence_lexicographic_diamond_is_independent_of_input_vector_order() {
    let mut access = literal(TEXT);
    access.sides[0].graph.observations = vec![
        observation(O1, A, Some(B), "extends"),
        observation(OF, B, Some(T), "extends"),
        observation(O2, A, Some(C), "extends"),
        observation(O0, C, Some(T), "extends"),
    ];
    let options = walk_options(2, 0);
    let (walked, _) = walk_literal(&access, &options).unwrap();
    let target = walked
        .hierarchy
        .iter()
        .find(|state| {
            state.direction == Direction::Ancestor && state.entity == entity_index(&access, T)
        })
        .unwrap();
    assert_eq!(target.distance, 2);
    let witness: Vec<_> = target
        .witness
        .iter()
        .map(|&index| access.sides[0].graph.observations[index].id.as_str())
        .collect();
    assert_eq!(witness, [O1, OF]); // [O2,O0] has the smaller final edge only.
    let before = finish_literal(&access, &options).unwrap().1;
    access.sides[0].graph.entities.reverse();
    access.sides[0].graph.observations.reverse();
    access.sides[0].graph.evidence.reverse();
    assert_eq!(finish_literal(&access, &options).unwrap().1, before);
    access.sides[0]
        .graph
        .observations
        .push(observation(O3, A, Some(T), "extends"));
    let (walked, _) = walk_literal(&access, &options).unwrap();
    let target = walked
        .hierarchy
        .iter()
        .find(|state| {
            state.direction == Direction::Ancestor && state.entity == entity_index(&access, T)
        })
        .unwrap();
    assert_eq!(target.distance, 1);
    assert_eq!(
        target
            .witness
            .iter()
            .map(|&index| access.sides[0].graph.observations[index].id.as_str())
            .collect::<Vec<_>>(),
        [O3]
    );
}

fn cycle() -> LiteralAccess {
    let mut access = literal(TEXT);
    access.sides[0].graph.observations = vec![
        observation(O1, A, Some(B), "extends"),
        observation(O2, A, Some(B), "extends"),
        observation(O3, B, Some(A), "implements"),
        observation(O4, A, Some(A), "inherits"),
    ];
    access
}

#[test]
fn cycle_self_loop_and_parallel_edges_preserve_proof_separately_from_visited_states() {
    let access = cycle();
    let (walked, counts) = walk_literal(&access, &walk_options(2, 0)).unwrap();
    assert_eq!(walked.hierarchy.len(), 4);
    assert_eq!(walked.hierarchy_proofs.len(), 8);
    assert_eq!(counts.index_observation_scans, 4);
    assert_eq!(counts.hierarchy_adjacency_visits, 8);
    assert_eq!(walked.members.len(), 2);
    assert!(
        walked
            .members
            .iter()
            .all(|reason| reason.member == entity_index(&access, BM))
    );
    assert_eq!(
        walked.expanded,
        [(Side::Base, entity_index(&access, BM))].into()
    );
    let joined = walked
        .walk_seeds
        .iter()
        .find(|seed| seed.entity == entity_index(&access, BM))
        .unwrap();
    assert_eq!(joined.reasons.len(), 2);
    for direction in [Direction::Ancestor, Direction::Descendant] {
        let root = walked
            .hierarchy
            .iter()
            .find(|state| state.direction == direction && state.entity == entity_index(&access, A))
            .unwrap();
        assert_eq!(root.distance, 0);
        assert!(root.witness.is_empty());
        let proof: BTreeSet<_> = walked
            .hierarchy_proofs
            .iter()
            .filter(|proof| walked.hierarchy[proof.state].direction == direction)
            .map(|proof| {
                access.sides[0].graph.observations[proof.observation]
                    .id
                    .as_str()
            })
            .collect();
        assert_eq!(proof, [O1, O2, O3, O4].into());
    }
    let (bounded, _) = walk_literal(&access, &walk_options(1, 0)).unwrap();
    assert_eq!(bounded.hierarchy_proofs.len(), 5);
    let boundary: Vec<_> = bounded
        .hierarchy
        .iter()
        .filter(|state| state.entity == entity_index(&access, B))
        .map(|state| {
            (
                state.direction,
                state.distance,
                state.boundary,
                state.unexamined,
            )
        })
        .collect();
    assert_eq!(
        boundary,
        [
            (Direction::Ancestor, 1, true, 1),
            (Direction::Descendant, 1, true, 2)
        ]
    );
}

#[test]
fn cross_root_original_members_keep_reasons_and_independent_state_credit() {
    let mut access = cycle();
    access.changed = vec![
        json!({"side":"base","id":AM}),
        json!({"side":"base","id":BM}),
        json!({"side":"base","id":AM}),
    ];
    let options = AffectedOptions {
        max_states: 12,
        max_expanded_seeds: 0,
        ..walk_options(2, 0)
    };
    let (walked, counts) = walk_literal(&access, &options).unwrap();
    assert_eq!(counts.original_seed_rows, 3);
    assert_eq!(counts.unique_expanded_seeds, 0);
    assert!(walked.expanded.is_empty());
    assert_eq!(walked.originals.len(), 2);
    assert_eq!(
        walked
            .originals
            .iter()
            .find(|root| root.entity == entity_index(&access, AM))
            .unwrap()
            .changed_rows,
        [0, 2]
    );
    assert_eq!(counts.hierarchy_states, 8);
    assert_eq!(counts.incoming_states, 4);
    let origins: BTreeSet<_> = walked
        .incoming
        .iter()
        .map(|state| (state.root, state.seed))
        .collect();
    assert_eq!(
        origins,
        [
            (entity_index(&access, AM), entity_index(&access, AM)),
            (entity_index(&access, AM), entity_index(&access, BM)),
            (entity_index(&access, BM), entity_index(&access, AM)),
            (entity_index(&access, BM), entity_index(&access, BM))
        ]
        .into()
    );
    let lower = AffectedOptions {
        max_states: 11,
        ..options
    };
    assert!(
        walk_literal(&access, &lower)
            .err()
            .unwrap()
            .is::<Capacity>()
    );
}

#[test]
fn incoming_visited_keys_include_the_expanded_walk_seed() {
    let mut access = literal(TEXT);
    access.sides[0].graph.observations = vec![
        observation(O1, A, Some(B), "extends"),
        observation(O2, A, Some(C), "extends"),
        observation(O3, CALLER, Some(BM), "calls"),
        observation(O4, MIDDLE, Some(CM), "references"),
        observation(O5, CALLER, Some(MIDDLE), "calls"),
        observation(O6, CALLER, Some(CALLER), "calls"),
    ];
    let (walked, _) = walk_literal(&access, &walk_options(1, 2)).unwrap();
    let mut caller_states: Vec<_> = walked
        .incoming
        .iter()
        .filter(|state| state.entity == entity_index(&access, CALLER))
        .map(|state| {
            (
                access.sides[0].graph.entities[state.seed].id.as_str(),
                state.distance,
                state
                    .witness
                    .iter()
                    .map(|&index| access.sides[0].graph.observations[index].id.as_str())
                    .collect::<Vec<_>>(),
            )
        })
        .collect();
    caller_states.sort();
    assert_eq!(caller_states, [(BM, 1, vec![O3]), (CM, 2, vec![O4, O5])]);
    let caller_loop_refs = walked
        .incoming_proofs
        .iter()
        .filter(|proof| access.sides[0].graph.observations[proof.observation].id == O6)
        .count();
    assert_eq!(caller_loop_refs, 1); // The depth-two origin stops at its boundary.
}

#[test]
fn new_proof_evidence_keeps_vector_order_and_repetition_with_unique_row_closure() {
    let mut access = literal(TEXT);
    let mut proof = observation(O1, CALLER, Some(AM), "calls");
    proof.evidence_ids = vec![E2.into(), E1.into(), E2.into()];
    let mut baseline_only = observation(O2, B, Some(C), "references");
    baseline_only.evidence_ids = vec![UNUSED_E.into()];
    access.sides[0].graph.observations = vec![proof, baseline_only];
    access.sides[0].graph.evidence = vec![
        evidence(E2, &access),
        evidence(UNUSED_E, &access),
        evidence(E1, &access),
    ];
    access.impact_observations = vec![json!({"side":"base","id":O2})];
    let (value, bytes) = finish_literal(&access, &walk_options(0, 1)).unwrap();
    assert_eq!(value["observations"].as_array().unwrap().len(), 1);
    assert_eq!(value["observations"][0]["observationId"], O1);
    assert_eq!(value["observations"][0]["evidenceIds"], json!([E2, E1, E2]));
    assert_eq!(
        value["evidence"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["evidenceId"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [E1, E2]
    );
    assert_eq!(
        value["originalImpactObservationRefs"],
        json!([{"originalRowIndex":0,"side":"base","observationId":O2}])
    );
    assert_eq!(
        value["entities"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["entityId"].as_str().unwrap())
            .collect::<BTreeSet<_>>(),
        [AM, CALLER].into()
    );
    fn recursive_rows(value: &Value) -> usize {
        match value {
            Value::Array(rows) => rows.len() + rows.iter().map(recursive_rows).sum::<usize>(),
            Value::Object(fields) => fields.values().map(recursive_rows).sum(),
            _ => 0,
        }
    }
    assert_eq!(
        value["rowUnits"].as_u64().unwrap() as usize,
        recursive_rows(&value)
    );
    assert_eq!(value["wireBytes"].as_u64().unwrap() as usize, bytes.len());
}

#[test]
fn no_change_index_work_includes_unresolved_and_irrelevant_observations() {
    let mut access = literal(TEXT);
    access.changed.clear();
    access.sides[0].graph.observations = vec![
        observation(O1, A, None, "extends"),
        observation(O2, B, None, "calls"),
        observation(O3, C, Some(T), "unrelated"),
    ];
    let options = AffectedOptions {
        max_original_seeds: 0,
        max_expanded_seeds: 0,
        max_states: 0,
        max_source_bytes: 0,
        max_edge_visits: 3,
        ..walk_options(0, 0)
    };
    let (walked, counts) = walk_literal(&access, &options).unwrap();
    assert!(
        walked.originals.is_empty() && walked.hierarchy.is_empty() && walked.incoming.is_empty()
    );
    assert_eq!(counts.index_observation_scans, 3);
    assert_eq!(counts.new_evidence_source_bytes, 0);
    let lower = AffectedOptions {
        max_edge_visits: 2,
        ..options
    };
    assert!(
        walk_literal(&access, &lower)
            .err()
            .unwrap()
            .is::<Capacity>()
    );
}

#[test]
fn both_side_identity_audit_precedes_base_observation_capacity() {
    for duplicate_key in [false, true] {
        let mut access = literal(TEXT);
        access.sides[0]
            .graph
            .observations
            .push(observation(O1, A, Some(B), "extends"));
        access.sides[1] = access.sides[0].clone();
        let mut duplicate = access.sides[1].graph.entities[0].clone();
        if duplicate_key {
            duplicate.id = O0.into();
        } else {
            duplicate.key = "different:key".into();
        }
        access.sides[1].graph.entities.push(duplicate);
        let options = AffectedOptions {
            max_edge_visits: 0,
            ..walk_options(0, 0)
        };
        let deadline = Deadline::new(30_000);
        let budget = Budget::new(&options, &deadline);
        let error = super::validate_unique_entities(&access, &deadline).unwrap_err();
        assert!(!error.is::<Capacity>());
        assert_eq!(budget.counts.index_observation_scans, 0);
        // The normal kernel entry performs that audit before either side's index.
        assert!(
            !walk_literal(&access, &options)
                .err()
                .unwrap()
                .is::<Capacity>()
        );
    }
}

#[test]
fn legacy_evidence_line_forms_project_to_one_exact_crlf_and_eof_range() {
    let text = "class A { save() {} }\r\n";
    let mut access = literal(text);
    let mut row = evidence(E1, &access);
    row.start_byte = 0;
    row.end_byte = text.len();
    row.start_line = 1;
    access.sides[0].graph.evidence = vec![row];
    let options = AffectedOptions::default();
    let deadline = Deadline::new(30_000);
    for legacy_end in [1, 2] {
        access.sides[0].graph.evidence[0].end_line = legacy_end;
        let mut budget = Budget::new(&options, &deadline);
        let index = Index::build(Side::Base, &access, &mut budget).unwrap();
        let mut sources = SourceCache::new(&access);
        sources.validate_evidence(&index, 0, &mut budget).unwrap();
        assert_eq!(
            sources
                .range(&index, PATH, 0, text.len(), &mut budget)
                .unwrap(),
            (0, text.len(), 1, 1)
        );
        // A zero-width EOF interval belongs to the following LF line, not its predecessor.
        assert_eq!(
            sources
                .range(&index, PATH, text.len(), text.len(), &mut budget)
                .unwrap(),
            (text.len(), text.len(), 2, 2)
        );
    }
    access.sides[0].graph.evidence[0].end_line = 3;
    let mut budget = Budget::new(&options, &deadline);
    let index = Index::build(Side::Base, &access, &mut budget).unwrap();
    assert!(
        SourceCache::new(&access)
            .validate_evidence(&index, 0, &mut budget)
            .is_err()
    );
}

#[test]
fn source_hash_cache_still_checks_second_path_manifest_size() {
    let mut access = literal(TEXT);
    let mut second_file = access.sides[0].snapshot.files[0].clone();
    second_file.path = "src/second.ts".into();
    second_file.size += 1;
    access.sides[0].snapshot.files.push(second_file);
    access.sides[0]
        .blobs
        .insert("src/second.ts".into(), TEXT.as_bytes().to_vec());
    let mut second_entity = row_mut(&mut access, T).clone();
    second_entity.path = "src/second.ts".into();
    second_entity.id = O0.into();
    second_entity.key = "second:T".into();
    second_entity.parent_key = None;
    access.sides[0].graph.entities.push(second_entity);
    let options = AffectedOptions::default();
    let deadline = Deadline::new(30_000);
    let mut budget = Budget::new(&options, &deadline);
    let index = Index::build(Side::Base, &access, &mut budget).unwrap();
    let mut sources = SourceCache::new(&access);
    sources
        .validate_entity(&index, entity_index(&access, A), &mut budget)
        .unwrap();
    assert_eq!(budget.counts.new_evidence_source_bytes, TEXT.len());
    assert!(
        sources
            .validate_entity(&index, entity_index(&access, O0), &mut budget)
            .is_err()
    );
}

#[test]
fn joined_overloads_keep_unsupported_name_anchor_reason_without_owner_overloads() {
    let mut access = literal(TEXT);
    access.sides[0]
        .graph
        .observations
        .push(observation(O1, A, Some(B), "extends"));
    let mut eligible_overload = row_mut(&mut access, BM).clone();
    eligible_overload.id = O0.into();
    eligible_overload.key = "method:B.save:overload".into();
    let mut unsupported_overload = eligible_overload.clone();
    unsupported_overload.id = O2.into();
    unsupported_overload.key = "method:B.save:unanchored".into();
    unsupported_overload.name_start_byte = None;
    unsupported_overload.name_end_byte = None;
    let mut owner_overload = row_mut(&mut access, AM).clone();
    owner_overload.id = O3.into();
    owner_overload.key = "method:A.save:overload".into();
    let mut parentless = eligible_overload.clone();
    parentless.id = O4.into();
    parentless.key = "method:parentless:save".into();
    parentless.parent_key = None;
    access.sides[0].graph.entities.extend([
        eligible_overload,
        unsupported_overload,
        owner_overload,
        parentless,
    ]);
    let (walked, counts) = walk_literal(&access, &walk_options(1, 0)).unwrap();
    assert_eq!(walked.members.len(), 3);
    assert_eq!(
        walked
            .members
            .iter()
            .filter(|reason| matches!(reason.eligibility, Eligibility::Expandable { .. }))
            .count(),
        2
    );
    let unsupported = walked
        .members
        .iter()
        .find(|reason| reason.member == entity_index(&access, O2))
        .unwrap();
    assert_eq!(
        unsupported.eligibility,
        Eligibility::Unsupported("missing_name_anchor")
    );
    assert_eq!(counts.unique_expanded_seeds, 2);
    assert_eq!(walked.walk_seeds.len(), 3);
    assert!(walked.members.iter().all(|reason| {
        ![entity_index(&access, O3), entity_index(&access, O4)].contains(&reason.member)
    }));
}

fn wide_diamond() -> LiteralAccess {
    // Two literal type identities at each of twelve levels. Only A has a method.
    let levels = [
        [("L01a", "0101010101010101010101010101010101010101010101010101010101010101"), ("L01b", "0102010201020102010201020102010201020102010201020102010201020102")],
        [("L02a", "0201020102010201020102010201020102010201020102010201020102010201"), ("L02b", "0202020202020202020202020202020202020202020202020202020202020202")],
        [("L03a", "0301030103010301030103010301030103010301030103010301030103010301"), ("L03b", "0302030203020302030203020302030203020302030203020302030203020302")],
        [("L04a", "0401040104010401040104010401040104010401040104010401040104010401"), ("L04b", "0402040204020402040204020402040204020402040204020402040204020402")],
        [("L05a", "0501050105010501050105010501050105010501050105010501050105010501"), ("L05b", "0502050205020502050205020502050205020502050205020502050205020502")],
        [("L06a", "0601060106010601060106010601060106010601060106010601060106010601"), ("L06b", "0602060206020602060206020602060206020602060206020602060206020602")],
        [("L07a", "0701070107010701070107010701070107010701070107010701070107010701"), ("L07b", "0702070207020702070207020702070207020702070207020702070207020702")],
        [("L08a", "0801080108010801080108010801080108010801080108010801080108010801"), ("L08b", "0802080208020802080208020802080208020802080208020802080208020802")],
        [("L09a", "0901090109010901090109010901090109010901090109010901090109010901"), ("L09b", "0902090209020902090209020902090209020902090209020902090209020902")],
        [("L10a", "1001100110011001100110011001100110011001100110011001100110011001"), ("L10b", "1002100210021002100210021002100210021002100210021002100210021002")],
        [("L11a", "1101110111011101110111011101110111011101110111011101110111011101"), ("L11b", "1102110211021102110211021102110211021102110211021102110211021102")],
        [("L12a", "1201120112011201120112011201120112011201120112011201120112011201"), ("L12b", "1202120212021202120212021202120212021202120212021202120212021202")],
    ];
    let mut text = String::from("class A { save() {} }\n");
    for level in &levels {
        for (name, _) in level {
            text.push_str(&format!("class {name} {{}}\n"));
        }
    }
    let mut access = literal(&text);
    for level in &levels {
        for (name, id) in level {
            let start = text.find(&format!("class {name} {{")).unwrap();
            let end = start + format!("class {name} {{}}").len();
            access.sides[0].graph.entities.push(entity(
                &text, id, &format!("class:{name}"), name, "class", (start, end),
                Some((start + 6, start + 6 + name.len())), Some("file"),
            ));
        }
    }
    let mut edges = Vec::new();
    for (_, target) in levels[0] {
        let id = format!("{:064x}", 1000 + edges.len());
        edges.push(observation(&id, A, Some(target), "extends"));
    }
    for pair in levels.windows(2) {
        for (_, source) in pair[0] {
            for (_, target) in pair[1] {
                let id = format!("{:064x}", 1000 + edges.len());
                edges.push(observation(&id, source, Some(target), "extends"));
            }
        }
    }
    access.sides[0].graph.observations = edges;
    access
}

#[test]
fn twelve_level_diamond_charges_states_and_edges_without_enumerating_paths() {
    let access = wide_diamond();
    let options = AffectedOptions {
        max_states: 27, max_edge_visits: 92, max_expanded_seeds: 0,
        ..walk_options(12, 0)
    };
    let (walked, counts) = walk_literal(&access, &options).unwrap();
    // A + 24 types in the ancestor walk, only A in the descendant walk.
    assert_eq!(walked.hierarchy.len(), 26);
    assert_eq!(walked.hierarchy.iter().filter(|state| state.direction == Direction::Ancestor).count(), 25);
    assert_eq!(walked.hierarchy.iter().filter(|state| state.direction == Direction::Descendant).count(), 1);
    assert_eq!(walked.hierarchy_proofs.len(), 46); // 2 initial + 11 complete 2-by-2 joins.
    assert_eq!(counts.index_observation_scans, 46);
    assert_eq!(counts.hierarchy_adjacency_visits, 46);
    assert_eq!(counts.incoming_adjacency_visits, 0);
    assert_eq!(counts.index_observation_scans + counts.hierarchy_adjacency_visits + counts.incoming_adjacency_visits, 92);
    assert_eq!(counts.hierarchy_states, 26);
    assert_eq!(counts.incoming_states, 1); // The one original seed still has its zero-depth state.
    assert!(walked.members.is_empty() && walked.expanded.is_empty());
    let state_keys: BTreeSet<_> = walked.hierarchy.iter().map(|state| (state.direction, state.entity)).collect();
    assert_eq!(state_keys.len(), 26);
    assert!(walked.hierarchy.iter().all(|state| state.witness.len() == state.distance));
    let final_level: Vec<_> = walked.hierarchy.iter().filter(|state| state.distance == 12).collect();
    assert_eq!(final_level.len(), 2);
    assert!(final_level.iter().all(|state| state.boundary && state.unexamined == 0));
    let lower_states = AffectedOptions { max_states: 26, ..options.clone() };
    assert!(walk_literal(&access, &lower_states).err().unwrap().is::<Capacity>());
    let lower_edges = AffectedOptions { max_edge_visits: 91, ..options };
    assert!(walk_literal(&access, &lower_edges).err().unwrap().is::<Capacity>());
}

#[test]
fn complete_derived_edge_credit_counts_scan_hierarchy_and_each_incoming_examination() {
    let mut access = cycle();
    access.sides[0].graph.observations.extend([
        observation(O5, CALLER, Some(AM), "calls"),
        observation(O6, CALLER, Some(AM), "references"),
        observation(OF, CALLER, Some(CALLER), "calls"),
    ]);
    let options = AffectedOptions { max_edge_visits: 18, max_states: 7, ..walk_options(2, 2) };
    let (walked, counts) = walk_literal(&access, &options).unwrap();
    // Seven stored observations, eight hierarchy exams, two parallel incoming
    // exams at the root and one self-loop exam at the caller: 7 + 8 + 3 = 18.
    assert_eq!(counts.index_observation_scans, 7);
    assert_eq!(counts.hierarchy_adjacency_visits, 8);
    assert_eq!(counts.incoming_adjacency_visits, 3);
    assert_eq!(walked.hierarchy_proofs.len(), 8);
    assert_eq!(walked.incoming_proofs.len(), 3);
    assert_eq!(counts.hierarchy_states, 4);
    assert_eq!(counts.incoming_states, 3);
    let (value, _) = finish_literal(&access, &options).unwrap();
    assert_eq!(value["consumed"]["edgeVisits"], 18);
    assert_eq!(value["consumed"]["states"], 7);
    let lower_edges = AffectedOptions { max_edge_visits: 17, ..options.clone() };
    assert!(walk_literal(&access, &lower_edges).err().unwrap().is::<Capacity>());
    let lower_states = AffectedOptions { max_states: 6, ..options };
    assert!(walk_literal(&access, &lower_states).err().unwrap().is::<Capacity>());
}

fn assert_midwork_cancellation(phase: super::WorkPhase, after: usize) {
    let mut access = literal(TEXT);
    access.sides[0].graph.observations = vec![
        observation(O1, A, Some(B), "extends"), observation(OF, B, Some(T), "extends"),
        observation(O2, A, Some(C), "extends"), observation(O0, C, Some(T), "extends"),
        observation(O3, CALLER, Some(AM), "calls"), observation(O4, MIDDLE, Some(CALLER), "references"),
    ];
    let options = walk_options(2, 2);
    // Qualify the same fixture with no observer before requiring a cancellation.
    assert!(finish_literal(&access, &options).is_ok());
    let cancellation = Arc::new(AtomicBool::new(false));
    let parent_deadline = Deadline::new(30_000);
    let mut producer_refused = false;
    let outer = Deadline::run_bounded(&parent_deadline, cancellation.clone(), || {
        // This child inherits the parent's ceiling and installed shared flag;
        // it cannot extend that ceiling. Pass this exact reference to every phase.
        let deadline = Deadline::new(30_000);
        let guard = super::cancel_work_at(phase, after, cancellation.clone());
        let error = finish_literal_with_deadline(&access, &options, &deadline)
            .expect_err("The producer must refuse before returning a partial or complete draft");
        assert!(cancellation.load(Ordering::SeqCst), "phase {phase:?} did not set the shared flag");
        assert!(guard.seen() >= after, "phase {phase:?} did not reach its work boundary");
        assert_eq!(super::public_error(error, &deadline).to_string(), "git_affected_details_deadline_exceeded");
        producer_refused = true;
        Ok(())
    });
    assert!(producer_refused, "phase {phase:?} must be refused by the producer itself");
    assert!(outer.is_err());
    assert!(cancellation.load(Ordering::SeqCst));
}

#[test]
fn cancellation_during_index_scan_returns_the_producers_deadline_class() {
    assert_midwork_cancellation(super::WorkPhase::IndexScan, 2);
}

#[test]
fn cancellation_during_hierarchy_adjacency_returns_the_producers_deadline_class() {
    assert_midwork_cancellation(super::WorkPhase::HierarchyAdjacency, 2);
}

#[test]
fn cancellation_during_incoming_adjacency_returns_the_producers_deadline_class() {
    assert_midwork_cancellation(super::WorkPhase::IncomingAdjacency, 2);
}

#[test]
fn cancellation_during_witness_comparison_returns_the_producers_deadline_class() {
    assert_midwork_cancellation(super::WorkPhase::WitnessComparison, 1);
}

#[test]
fn cancellation_during_output_sort_returns_the_producers_deadline_class() {
    assert_midwork_cancellation(super::WorkPhase::OutputSort, 2);
}

#[test]
fn cancellation_during_canonical_encoding_returns_the_producers_deadline_class() {
    assert_midwork_cancellation(super::WorkPhase::CanonicalEncoding, 2);
}
