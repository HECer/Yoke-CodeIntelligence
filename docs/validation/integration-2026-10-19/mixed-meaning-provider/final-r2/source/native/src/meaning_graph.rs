//! Typed model inferences over exact submitted source; never canonical code facts.
use crate::capture::Deadline;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub const OPERATION: &str = "extract_meaning_v1";
pub const MAX_INPUT_BYTES: usize = 1_000_000;
pub const MAX_FRAGMENT_BYTES: usize = 1_000_000;
pub const MAX_MATERIALIZED_BYTES: usize = 4_000_000;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Binding {
    pub source_id: String,
    pub path: String,
    pub content_hash: String,
    pub start_byte: usize,
    pub end_byte: usize,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Source {
    pub anchor: Binding,
    pub text: String,
    pub omitted_bytes: usize,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MeaningInput {
    pub workspace_id: String,
    pub unit_set_hash: String,
    pub contract_hash: String,
    pub sources: Vec<Source>,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Support {
    pub source_id: String,
    pub start_byte: usize,
    pub end_byte: usize,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConceptClaim {
    pub key: String,
    pub label: String,
    pub description: String,
    pub aliases: Vec<String>,
    pub support: Vec<Support>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RelationClaim {
    pub source: String,
    pub target: String,
    pub relation: String,
    pub description: String,
    pub support: Vec<Support>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Participant {
    pub role: String,
    pub concept: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HyperedgeClaim {
    pub relation: String,
    pub participants: Vec<Participant>,
    pub description: String,
    pub support: Vec<Support>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UnitCoverage {
    pub source_id: String,
    pub outcome: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Fragment {
    pub concepts: Vec<ConceptClaim>,
    pub relations: Vec<RelationClaim>,
    pub hyperedges: Vec<HyperedgeClaim>,
    pub coverage: Vec<UnitCoverage>,
}

fn text(value: &str, maximum: usize, single_line: bool) -> Result<()> {
    ensure!(
        !value.trim().is_empty() && value.len() <= maximum && !crate::paths::obvious_secret(value),
        "Invalid or secret-bearing meaning text"
    );
    ensure!(
        !value
            .chars()
            .any(|c| c.is_control() && (single_line || !matches!(c, '\n' | '\r' | '\t'))),
        "Unsupported meaning control character"
    );
    Ok(())
}
fn label(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.len() <= 80
            && value
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_')),
        "Invalid meaning identifier"
    );
    Ok(())
}
pub fn unit_id(workspace: &str, anchor: &Binding, text: &str) -> String {
    crate::model::identity(&(
        "meaning-unit-v1",
        workspace,
        &anchor.path,
        &anchor.content_hash,
        anchor.start_byte,
        anchor.end_byte,
        crate::model::hash(text),
    ))
}
fn bindings_checked<'a>(
    bindings: &'a [Binding],
    deadline: &Deadline,
) -> Result<BTreeMap<&'a str, &'a Binding>> {
    ensure!(
        !bindings.is_empty() && bindings.len() <= 16,
        "Meaning source count exceeds capacity"
    );
    let mut result = BTreeMap::new();
    for anchor in bindings {
        deadline.check()?;
        ensure!(
            crate::model::valid_id(&anchor.source_id)
                && crate::model::valid_id(&anchor.content_hash)
                && anchor.start_byte < anchor.end_byte
                && anchor.end_byte <= 4 * 1024 * 1024
                && anchor.end_byte - anchor.start_byte <= 16384,
            "Invalid meaning source binding"
        );
        ensure!(
            crate::paths::normalized_syntax(&anchor.path)? == anchor.path
                && anchor.path.len() <= 4096,
            "Invalid meaning source path"
        );
        ensure!(
            result.insert(anchor.source_id.as_str(), anchor).is_none(),
            "Duplicate meaning source identity"
        );
    }
    Ok(result)
}
pub fn validate_input(input: &MeaningInput, deadline: &Deadline) -> Result<()> {
    deadline.check()?;
    ensure!(
        crate::model::valid_id(&input.workspace_id)
            && crate::model::valid_id(&input.contract_hash)
            && crate::model::valid_id(&input.unit_set_hash),
        "Invalid meaning input identity"
    );
    let bindings = input
        .sources
        .iter()
        .map(|s| s.anchor.clone())
        .collect::<Vec<_>>();
    bindings_checked(&bindings, deadline)?;
    let mut total = 0usize;
    for source in &input.sources {
        deadline.check()?;
        text(&source.text, 16384, false)?;
        total = total
            .checked_add(source.text.len())
            .context("Meaning source size overflow")?;
        ensure!(
            total <= 128000
                && source.omitted_bytes == 0
                && source.text.len() == source.anchor.end_byte - source.anchor.start_byte
                && source.anchor.source_id
                    == unit_id(&input.workspace_id, &source.anchor, &source.text),
            "Meaning source bytes, omissions or local unit identity mismatch"
        );
    }
    ensure!(
        input.unit_set_hash == crate::model::identity(&serde_json::to_value(&input.sources)?),
        "Meaning source-set hash mismatch"
    );
    crate::json::encode_bounded(input, MAX_INPUT_BYTES, deadline)?;
    Ok(())
}
fn checked_support(
    support: &[Support],
    bindings: &BTreeMap<&str, (usize, usize)>,
    used: &mut BTreeSet<String>,
    unique: &mut BTreeSet<Support>,
    deadline: &Deadline,
) -> Result<()> {
    ensure!(
        !support.is_empty() && support.len() <= 8,
        "Meaning claim requires bounded explicit support"
    );
    let mut seen = BTreeSet::new();
    for span in support {
        deadline.check()?;
        let binding = bindings
            .get(span.source_id.as_str())
            .context("Meaning support cites an unsubmitted source")?;
        ensure!(
            span.start_byte >= binding.0
                && span.start_byte < span.end_byte
                && span.end_byte <= binding.1
                && seen.insert(span),
            "Meaning support escaped its submitted source or was repeated"
        );
        used.insert(span.source_id.clone());
        unique.insert(span.clone());
        ensure!(
            unique.len() <= 256,
            "Meaning unique support capacity exceeded"
        );
    }
    Ok(())
}
/// Validate the bounded graph shape against supplied range descriptors. This
/// checks no source store and cannot establish UTF-8 boundaries without text.
pub fn validate_fragment_shape(
    fragment: &Fragment,
    bindings: &[Binding],
    deadline: &Deadline,
) -> Result<()> {
    deadline.check()?;
    let bindings = bindings_checked(bindings, deadline)?;
    let ranges = bindings
        .into_iter()
        .map(|(id, b)| (id, (b.start_byte, b.end_byte)))
        .collect();
    validate_fragment_ranges(fragment, &ranges, deadline)
}
/// Shape-only descriptors. No path, snapshot, source text or read authority is
/// constructed here; callers must independently check their own source spaces.
pub(crate) fn validate_fragment_ranges(
    fragment: &Fragment,
    bindings: &BTreeMap<&str, (usize, usize)>,
    deadline: &Deadline,
) -> Result<()> {
    deadline.check()?;
    ensure!(
        !bindings.is_empty() && bindings.len() <= 16,
        "Meaning source count exceeds capacity"
    );
    for (id, (start, end)) in bindings {
        deadline.check()?;
        ensure!(
            crate::model::valid_id(id) && start < end && end - start <= 16384,
            "Invalid meaning support range descriptor"
        );
    }
    ensure!(
        fragment.concepts.len() <= 128
            && fragment.relations.len() <= 256
            && fragment.hyperedges.len() <= 64
            && fragment.coverage.len() == bindings.len(),
        "Meaning claim or coverage capacity exceeded"
    );
    let mut keys = BTreeSet::new();
    let mut used = BTreeSet::new();
    let mut unique = BTreeSet::new();
    for node in &fragment.concepts {
        deadline.check()?;
        label(&node.key)?;
        text(&node.label, 160, true)?;
        text(&node.description, 1000, false)?;
        ensure!(
            keys.insert(node.key.as_str()) && node.aliases.len() <= 8,
            "Duplicate meaning concept key or alias capacity exceeded"
        );
        let mut aliases = BTreeSet::new();
        for alias in &node.aliases {
            text(alias, 160, true)?;
            ensure!(aliases.insert(alias), "Duplicate meaning alias");
        }
        checked_support(&node.support, bindings, &mut used, &mut unique, deadline)?;
    }
    for edge in &fragment.relations {
        deadline.check()?;
        label(&edge.relation)?;
        text(&edge.description, 1000, false)?;
        ensure!(
            keys.contains(edge.source.as_str()) && keys.contains(edge.target.as_str()),
            "Meaning relation has a missing endpoint"
        );
        checked_support(&edge.support, bindings, &mut used, &mut unique, deadline)?;
    }
    for edge in &fragment.hyperedges {
        deadline.check()?;
        label(&edge.relation)?;
        text(&edge.description, 1000, false)?;
        ensure!(
            (2..=16).contains(&edge.participants.len()),
            "Meaning hyperedge participant capacity exceeded"
        );
        let mut participants = BTreeSet::new();
        let mut concepts = BTreeSet::new();
        for participant in &edge.participants {
            label(&participant.role)?;
            ensure!(
                keys.contains(participant.concept.as_str())
                    && participants.insert((&participant.role, &participant.concept)),
                "Meaning hyperedge has a missing or repeated participant"
            );
            concepts.insert(&participant.concept);
        }
        ensure!(
            concepts.len() >= 2,
            "Meaning hyperedge requires distinct concepts"
        );
        checked_support(&edge.support, bindings, &mut used, &mut unique, deadline)?;
    }
    let mut covered = BTreeSet::new();
    for coverage in &fragment.coverage {
        deadline.check()?;
        ensure!(
            bindings.contains_key(coverage.source_id.as_str())
                && covered.insert(coverage.source_id.as_str())
                && coverage.outcome
                    == if used.contains(&coverage.source_id) {
                        "claims"
                    } else {
                        "no_claims"
                    },
            "Meaning coverage is missing, duplicated, foreign or inconsistent with support"
        );
    }
    crate::json::encode_bounded(fragment, MAX_FRAGMENT_BYTES, deadline)?;
    Ok(())
}
fn support_spans(fragment: &Fragment) -> BTreeSet<Support> {
    fragment
        .concepts
        .iter()
        .flat_map(|c| c.support.iter())
        .chain(fragment.relations.iter().flat_map(|c| c.support.iter()))
        .chain(fragment.hyperedges.iter().flat_map(|c| c.support.iter()))
        .cloned()
        .collect()
}
fn quote<'a>(input: &'a MeaningInput, span: &Support) -> Result<(&'a Source, &'a str)> {
    let source = input
        .sources
        .iter()
        .find(|s| s.anchor.source_id == span.source_id)
        .context("Meaning evidence source missing")?;
    let start = span
        .start_byte
        .checked_sub(source.anchor.start_byte)
        .context("Meaning support start escaped source")?;
    let end = span
        .end_byte
        .checked_sub(source.anchor.start_byte)
        .context("Meaning support end escaped source")?;
    Ok((source, crate::paths::utf8_range(&source.text, start, end)?))
}
pub fn normalize(
    input: &MeaningInput,
    fragment: &Fragment,
    deadline: &Deadline,
) -> Result<Fragment> {
    validate_input(input, deadline)?;
    validate_fragment_shape(
        fragment,
        &input
            .sources
            .iter()
            .map(|s| s.anchor.clone())
            .collect::<Vec<_>>(),
        deadline,
    )?;
    let mut quoted = 0usize;
    for span in support_spans(fragment) {
        deadline.check()?;
        quoted = quoted
            .checked_add(quote(input, &span)?.1.len())
            .context("Meaning quotation size overflow")?;
        ensure!(
            quoted <= 128000,
            "Meaning aggregate quotation capacity exceeded"
        );
    }
    let scope = (
        &input.workspace_id,
        &input.unit_set_hash,
        &input.contract_hash,
    );
    let mut mapped = BTreeMap::new();
    let mut concepts = BTreeMap::new();
    for concept in &fragment.concepts {
        deadline.check()?;
        let mut node = concept.clone();
        node.aliases.sort();
        node.support.sort();
        let id = crate::model::identity(&(
            "meaning-concept-v1",
            scope,
            &node.label,
            &node.description,
            &node.aliases,
            &node.support,
        ));
        mapped.insert(concept.key.clone(), id.clone());
        node.key = id.clone();
        if let Some(existing) = concepts.insert(id, node.clone()) {
            ensure!(existing == node, "Meaning concept identity collision");
        }
    }
    let mut relations = BTreeMap::new();
    for relation in &fragment.relations {
        deadline.check()?;
        let mut edge = relation.clone();
        edge.source = mapped[&edge.source].clone();
        edge.target = mapped[&edge.target].clone();
        edge.support.sort();
        let id = crate::model::identity(&("meaning-relation-v1", scope, &edge));
        if let Some(existing) = relations.insert(id, edge.clone()) {
            ensure!(existing == edge, "Meaning relation identity collision");
        }
    }
    let mut hyperedges = BTreeMap::new();
    for hyperedge in &fragment.hyperedges {
        deadline.check()?;
        let mut edge = hyperedge.clone();
        edge.support.sort();
        for participant in &mut edge.participants {
            participant.concept = mapped[&participant.concept].clone();
        }
        let id = crate::model::identity(&("meaning-hyperedge-v1", scope, &edge));
        if let Some(existing) = hyperedges.insert(id, edge.clone()) {
            ensure!(existing == edge, "Meaning hyperedge identity collision");
        }
    }
    let mut coverage = fragment.coverage.clone();
    coverage.sort_by(|a, b| a.source_id.cmp(&b.source_id));
    let normalized = Fragment {
        concepts: concepts.into_values().collect(),
        relations: relations.into_values().collect(),
        hyperedges: hyperedges.into_values().collect(),
        coverage,
    };
    validate_fragment_shape(
        &normalized,
        &input
            .sources
            .iter()
            .map(|s| s.anchor.clone())
            .collect::<Vec<_>>(),
        deadline,
    )?;
    Ok(normalized)
}
pub fn materialize(
    input: &MeaningInput,
    fragment: &Fragment,
    deadline: &Deadline,
) -> Result<Value> {
    let normalized = normalize(input, fragment, deadline)?;
    let scope = (
        &input.workspace_id,
        &input.unit_set_hash,
        &input.contract_hash,
    );
    let mut evidence = Vec::new();
    let mut evidence_ids = BTreeMap::new();
    for support in support_spans(&normalized) {
        deadline.check()?;
        let (source, text) = quote(input, &support)?;
        let id = crate::model::identity(&("meaning-evidence-v1", scope, &support));
        evidence.push(json!({"id":id,"sourceId":support.source_id,"path":source.anchor.path,"contentHash":source.anchor.content_hash,"startByte":support.start_byte,"endByte":support.end_byte,"text":text,"precision":"submitted_source_subrange","trust":"submitted_bytes_unverified_capture"}));
        evidence_ids.insert(support, id);
    }
    let ids = |support: &[Support]| {
        support
            .iter()
            .map(|s| evidence_ids[s].clone())
            .collect::<Vec<_>>()
    };
    let concepts=normalized.concepts.iter().map(|node|json!({"id":node.key,"label":node.label,"description":node.description,"aliases":node.aliases,"evidenceIds":ids(&node.support),"resolution":"inferred_concept","providerAttestation":"unverified"})).collect::<Vec<_>>();
    let relations=normalized.relations.iter().map(|edge|json!({"id":crate::model::identity(&("meaning-relation-v1",scope,edge)),"source":edge.source,"target":edge.target,"relation":edge.relation,"description":edge.description,"evidenceIds":ids(&edge.support),"resolution":"inferred_concept","providerAttestation":"unverified"})).collect::<Vec<_>>();
    let hyperedges=normalized.hyperedges.iter().map(|edge|json!({"id":crate::model::identity(&("meaning-hyperedge-v1",scope,edge)),"relation":edge.relation,"participants":edge.participants,"description":edge.description,"evidenceIds":ids(&edge.support),"resolution":"inferred_concept","providerAttestation":"unverified"})).collect::<Vec<_>>();
    let value = json!({"schemaVersion":1,"workspaceId":input.workspace_id,"unitSetHash":input.unit_set_hash,"contractHash":input.contract_hash,"sourceBindings":input.sources.iter().map(|s|&s.anchor).collect::<Vec<_>>(),"concepts":concepts,"relations":relations,"hyperedges":hyperedges,"evidence":evidence,"coverage":normalized.coverage,"resolution":"inferred_concept","providerAttestation":"unverified","bindingValidation":"submitted_bytes_only","complete":false,"limitations":["Support binds exact submitted UTF-8 bytes; this pure builder does not verify captured-store membership.","Model claims, aliases and relationships are inferences, not verified compiler or runtime facts.","Selected-unit processing is not exhaustive semantic coverage. Hyperedge participant roles and order are retained."]});
    crate::json::encode_bounded(&value, MAX_MATERIALIZED_BYTES, deadline)?;
    Ok(value)
}
