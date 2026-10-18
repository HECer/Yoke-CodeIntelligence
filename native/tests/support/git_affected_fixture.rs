//! Captured Git fixture and independent affected-result oracle helpers.
#![allow(dead_code)] // Intended to be shared by qualification and later behavior targets.
use super::test_yoke;
#[path = "git_impact_fixture.rs"]
pub mod git;
pub use git::{Fixture, expected_range, state};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use test_yoke::model::{ANALYZER, Graph, Snapshot, hash, identity};

pub const PATH: &str = "src/affected-\u{fc}.ts";
const LF_SOURCE: &str = concat!(
    "// caf\u{e9} \u{1f30d} e\u{301}; literal historical UTF-8 source\n",
    "interface Service {\n  save(value: string): void;\n}\n",
    "class Base implements Service {\n  save(value: string) {\n    void \"\u{3b2}-base\";\n  }\n}\n",
    "class Derived extends Base {\n  save(value: string) {\n    void \"\u{3b4}-base\";\n  }\n}\n",
    "class Sibling extends Base {\n  save(value: string) {\n    void \"sibling\";\n  }\n}\n",
    "class Unrelated {\n  save(value: string) {\n    void \"unrelated\";\n  }\n}\n",
    "function viaBase(x: Base) { x.save(\"b\"); x.save(\"b\"); }\n",
    "function viaContract(x: Service) { x.save(\"c\"); x.save(\"c\"); }\n",
    "function viaDerived(x: Derived) { x.save(\"d\"); x.save(\"d\"); }\n",
    "function viaSibling(x: Sibling) { x.save(\"s\"); x.save(\"s\"); }\n",
    "function viaUnrelated(x: Unrelated) { x.save(\"u\"); x.save(\"u\"); }\n",
    "function indirectly(x: Base) { viaBase(x); }\n",
);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edit {
    Derived,
    Base,
    None,
}

pub fn hierarchy(edit: Edit, crlf: bool) -> Fixture {
    let base = if crlf {
        LF_SOURCE.replace('\n', "\r\n")
    } else {
        LF_SOURCE.into()
    };
    let head = match edit {
        Edit::Derived => base.replace("\u{3b4}-base", "\u{3b4}-head"),
        Edit::Base => base.replace("\u{3b2}-base", "\u{3b2}-head"),
        Edit::None => base.clone(),
    };
    assert_eq!(
        base.len(),
        head.len(),
        "Only the selected body marker changes"
    );
    Fixture::new(&[(PATH, &base)], &[(PATH, &head)])
}

pub fn side(f: &Fixture, label: &str) -> (Snapshot, Graph) {
    assert!(matches!(label, "base" | "head"));
    let id = f.report[format!("{label}SnapshotId")].as_str().unwrap();
    let snapshot = f.engine.store.snapshot(id).unwrap();
    let graph = f.engine.store.graph(id).unwrap();
    assert_eq!(identity(&graph), f.report[format!("{label}GraphHash")]);
    (snapshot, graph)
}

#[derive(Clone, Debug)]
pub struct ExpectedEntity {
    pub id: String,
    pub key: String,
    pub kind: &'static str,
    pub name: &'static str,
    pub owner: Option<&'static str>,
    pub start: usize,
    pub end: usize,
    pub name_start: Option<usize>,
    pub name_end: Option<usize>,
}

#[derive(Clone, Debug)]
pub struct ExpectedObservation {
    pub id: String,
    pub evidence_id: String,
    pub source: &'static str,
    pub target: Option<&'static str>,
    pub relation: &'static str,
    pub label: String,
    pub resolution: &'static str,
    pub start: usize,
    pub end: usize,
}

pub struct Oracle {
    pub snapshot_id: String,
    pub text: String,
    pub entities: BTreeMap<&'static str, ExpectedEntity>,
    pub hierarchy: Vec<ExpectedObservation>,
    pub calls: Vec<ExpectedObservation>,
}

pub fn unique(text: &str, needle: &str) -> (usize, usize) {
    let starts: Vec<_> = text.match_indices(needle).map(|(i, _)| i).collect();
    assert_eq!(
        starts.len(),
        1,
        "Independently specified literal must be unique: {needle}"
    );
    (starts[0], starts[0] + needle.len())
}

fn owner_span(text: &str, prefix: &str) -> (usize, usize) {
    let (start, _) = unique(text, prefix);
    let closing = if text.contains("\r\n") {
        "\r\n}"
    } else {
        "\n}"
    };
    let end = start + text[start..].find(closing).unwrap() + closing.len();
    (start, end)
}

fn expected_entity(
    snapshot: &str,
    label: &'static str,
    kind: &'static str,
    name: &'static str,
    owner: Option<&'static str>,
    span: (usize, usize),
    name_start: Option<usize>,
) -> (&'static str, ExpectedEntity) {
    let key = format!("{PATH}:{kind}:{}", span.0);
    let value = ExpectedEntity {
        id: identity(&(snapshot, &key)),
        key,
        kind,
        name,
        owner,
        start: span.0,
        end: span.1,
        name_start,
        name_end: name_start.map(|start| start + name.len()),
    };
    (label, value)
}

#[allow(clippy::too_many_arguments)] // Explicit fixture tuple and released identity inputs.
fn expected_observation(
    graph: &Graph,
    oracle: &Oracle,
    source: &'static str,
    target: Option<&'static str>,
    relation: &'static str,
    label: &str,
    resolution: &'static str,
    span: (usize, usize),
) -> ExpectedObservation {
    // The raw-edge ordinal is part of the released evidence key. First locate
    // the independently specified exact source tuple, then bind that ordinal.
    // No candidate expansion, production traversal or canonical sorter is used.
    let matches: Vec<_> = graph
        .evidence
        .iter()
        .enumerate()
        .filter(|(_, e)| e.path == PATH && e.start_byte == span.0 && e.end_byte == span.1)
        .collect();
    assert_eq!(
        matches.len(),
        1,
        "Unique evidence tuple {source}/{relation}/{span:?}"
    );
    let (ordinal, evidence) = matches[0];
    let evidence_id = identity(&(&oracle.snapshot_id, PATH, span.0, span.1, relation, ordinal));
    assert_eq!(evidence.id, evidence_id);
    assert_eq!(evidence.snapshot_id, oracle.snapshot_id);
    assert_eq!(evidence.content_hash, hash(&oracle.text));
    assert_eq!(evidence.anchor, "exact");
    assert_eq!(evidence.trust, "source-data");
    assert_eq!(evidence.producer, ANALYZER);
    let range = expected_range(&oracle.text, span.0, span.1);
    assert_eq!(
        evidence.start_line,
        range["startLine"].as_u64().unwrap() as usize
    );
    assert_eq!(
        evidence.end_line,
        range["endLine"].as_u64().unwrap() as usize
    );
    let target_id = target.map(|name| oracle.entities[name].id.clone());
    let source_entity = &oracle.entities[source];
    let id = identity(&(
        &oracle.snapshot_id,
        &source_entity.key,
        &target_id,
        &evidence_id,
    ));
    let rows: Vec<_> = graph.observations.iter().filter(|o| o.id == id).collect();
    assert_eq!(
        rows.len(),
        1,
        "Exact observation identity for {source}/{relation}/{span:?}"
    );
    let row = rows[0];
    assert_eq!(row.snapshot_id, oracle.snapshot_id);
    assert_eq!(row.source, source_entity.id);
    assert_eq!(row.target, target_id);
    assert_eq!(row.relation, relation);
    assert_eq!(row.target_label, label);
    assert_eq!(row.resolution, resolution);
    assert_eq!(
        row.evidence_ids.as_slice(),
        std::slice::from_ref(&evidence_id)
    );
    assert_eq!(row.producer, ANALYZER);
    ExpectedObservation {
        id,
        evidence_id,
        source,
        target,
        relation,
        label: label.into(),
        resolution,
        start: span.0,
        end: span.1,
    }
}

pub fn oracle(f: &Fixture, label: &str) -> Oracle {
    let (snapshot, graph) = side(f, label);
    let text = f.text(label, PATH).to_owned();
    let mut entities = BTreeMap::from([expected_entity(
        &snapshot.id,
        "file",
        "file",
        PATH,
        None,
        (0, text.len()),
        None,
    )]);
    let nl = if text.contains("\r\n") { "\r\n" } else { "\n" };
    for (owner, prefix, kind, method) in [
        (
            "Service",
            "interface Service {",
            "interface",
            "Service.save",
        ),
        (
            "Base",
            "class Base implements Service {",
            "class",
            "Base.save",
        ),
        (
            "Derived",
            "class Derived extends Base {",
            "class",
            "Derived.save",
        ),
        (
            "Sibling",
            "class Sibling extends Base {",
            "class",
            "Sibling.save",
        ),
        ("Unrelated", "class Unrelated {", "class", "Unrelated.save"),
    ] {
        let span = owner_span(&text, prefix);
        let owner_name = span.0 + prefix.find(owner).unwrap();
        entities.extend([expected_entity(
            &snapshot.id,
            owner,
            kind,
            owner,
            Some("file"),
            span,
            Some(owner_name),
        )]);
        let start = span.0 + text[span.0..span.1].find("save(value: string)").unwrap();
        let end = if kind == "interface" {
            start + "save(value: string): void".len()
        } else {
            span.1 - nl.len() - 1
        };
        entities.extend([expected_entity(
            &snapshot.id,
            method,
            "method",
            "save",
            Some(owner),
            (start, end),
            Some(start),
        )]);
    }
    for name in [
        "viaBase",
        "viaContract",
        "viaDerived",
        "viaSibling",
        "viaUnrelated",
        "indirectly",
    ] {
        let prefix = format!("function {name}(");
        let (start, _) = unique(&text, &prefix);
        let end = start + text[start..].find(nl).unwrap();
        entities.extend([expected_entity(
            &snapshot.id,
            name,
            "function",
            name,
            Some("file"),
            (start, end),
            Some(start + "function ".len()),
        )]);
    }
    let mut result = Oracle {
        snapshot_id: snapshot.id,
        text,
        entities,
        hierarchy: vec![],
        calls: vec![],
    };
    for (source, target, relation, clause) in [
        ("Base", "Service", "implements", "implements Service"),
        ("Derived", "Base", "extends", "extends Base"),
        ("Sibling", "Base", "extends", "extends Base"),
    ] {
        let owner = &result.entities[source];
        let local = &result.text[owner.start..owner.end];
        let (local_start, local_end) = unique(local, clause);
        let clause_span = (owner.start + local_start, owner.start + local_end);
        let start = clause_span.1 - target.len();
        let resolved = expected_observation(
            &graph,
            &result,
            source,
            Some(target),
            relation,
            target,
            "heuristic",
            (start, clause_span.1),
        );
        let generic = expected_observation(
            &graph,
            &result,
            source,
            None,
            "inherits",
            clause,
            "unresolved",
            clause_span,
        );
        result.hierarchy.extend([resolved, generic]);
    }
    for (source, target, literal) in [
        ("viaBase", "Base.save", "x.save(\"b\")"),
        ("viaContract", "Service.save", "x.save(\"c\")"),
        ("viaDerived", "Derived.save", "x.save(\"d\")"),
        ("viaSibling", "Sibling.save", "x.save(\"s\")"),
        ("viaUnrelated", "Unrelated.save", "x.save(\"u\")"),
        ("indirectly", "viaBase", "viaBase(x)"),
    ] {
        let caller = &result.entities[source];
        let occurrences: Vec<_> = result.text[caller.start..caller.end]
            .match_indices(literal)
            .map(|(offset, _)| (caller.start + offset, caller.start + offset + literal.len()))
            .collect();
        assert_eq!(
            occurrences.len(),
            if source == "indirectly" { 1 } else { 2 }
        );
        for span in occurrences {
            let target_label = if source == "indirectly" {
                "viaBase"
            } else {
                "x.save"
            };
            let expected = expected_observation(
                &graph,
                &result,
                source,
                Some(target),
                "calls",
                target_label,
                "heuristic",
                span,
            );
            result.calls.push(expected);
        }
    }
    result
}

impl Oracle {
    pub fn id(&self, name: &str) -> &str {
        &self.entities[name].id
    }

    pub fn expected_changed_claims(&self, side: &str, edit: Edit) -> Vec<Value> {
        let names: &[&str] = match edit {
            Edit::Derived => &["Derived", "Derived.save"],
            Edit::Base => &["Base", "Base.save"],
            Edit::None => &[],
        };
        names.iter().map(|name| {
            let entity = &self.entities[name];
            let range = expected_range(&self.text, entity.start, entity.end);
            json!({"id":entity.id,"snapshotId":self.snapshot_id,"path":PATH,"name":entity.name,"kind":entity.kind,
                "startByte":entity.start,"endByte":entity.end,"startLine":range["startLine"],"endLine":range["endLine"],
                "contentHash":hash(&self.text),"side":side,"producer":ANALYZER,"trust":"source_bound_structural_candidate"})
        }).collect()
    }

    pub fn calls_to(&self, target: &str) -> Vec<&ExpectedObservation> {
        self.calls
            .iter()
            .filter(|row| row.target == Some(target))
            .collect()
    }

    pub fn derived_ancestor_member_ids(&self) -> BTreeSet<String> {
        [self.id("Base.save").into(), self.id("Service.save").into()].into()
    }

    pub fn derived_excluded_member_ids(&self) -> BTreeSet<String> {
        [
            self.id("Sibling.save").into(),
            self.id("Unrelated.save").into(),
        ]
        .into()
    }

    pub fn base_descendant_member_ids(&self) -> BTreeSet<String> {
        [
            self.id("Derived.save").into(),
            self.id("Sibling.save").into(),
        ]
        .into()
    }
}
