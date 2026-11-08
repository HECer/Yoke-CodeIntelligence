//! Read-only lexical alias proposals; labels never establish semantic identity.
use crate::{capture::Deadline, store::Store};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use unicode_normalization::UnicodeNormalization;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Options {
    pub min_score_bps: u16,
    pub max_concepts: usize,
    pub max_pairs: usize,
    pub max_candidates: usize,
    pub max_similarity_work: u64,
    pub max_output_bytes: usize,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            min_score_bps: 9000,
            max_concepts: 512,
            max_pairs: 65_536,
            max_candidates: 4096,
            max_similarity_work: 16_000_000,
            max_output_bytes: 4_000_000,
        }
    }
}
pub fn validate_options(options: &Options) -> Result<()> {
    ensure!(
        (7000..=10000).contains(&options.min_score_bps)
            && options.max_concepts <= 1024
            && options.max_pairs <= 262_144
            && options.max_candidates <= 8192
            && options.max_similarity_work <= 64_000_000
            && (1023..=4_000_000).contains(&options.max_output_bytes),
        "Invalid meaning alias limits"
    );
    Ok(())
}
/// Replays the entire published source/receipt closure before comparing labels.
/// No provider execution, source capture, persistence or concept merging occurs.
pub fn discover(store: &Store, graph_id: &str, options: &Options, d: &Deadline) -> Result<Value> {
    d.check()?;
    validate_options(options)?;
    let graph = crate::meaning_build::load(store, graph_id, d)?;
    report(&graph, graph_id, options, d)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
struct Reference<'a> {
    packet_index: usize,
    concept_id: &'a str,
}
struct Term<'a> {
    raw: &'a str,
    kind: &'static str,
    normalized: String,
    chars: Vec<char>,
    grams: BTreeSet<[char; 3]>,
}
struct Concept<'a> {
    reference: Reference<'a>,
    record: &'a Value,
    evidence: Vec<&'a Value>,
    terms: Vec<Term<'a>>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MatchedTerms<'a> {
    left: &'a str,
    right: &'a str,
    left_kind: &'static str,
    right_kind: &'static str,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Candidate<'a> {
    left: Reference<'a>,
    right: Reference<'a>,
    score_bps: u16,
    basis: &'static str,
    jaro_winkler_bps: u16,
    trigram_jaccard_bps: u16,
    matched_terms: MatchedTerms<'a>,
}
#[derive(Serialize)]
struct ConceptProof<'a> {
    reference: Reference<'a>,
    record: &'a Value,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EvidenceProof<'a> {
    packet_index: usize,
    record: &'a Value,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Statistics {
    concept_count: usize,
    usable_terms: usize,
    unusable_terms: usize,
    pair_comparisons: usize,
    similarity_work: u64,
    candidate_count: usize,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Report<'a> {
    schema_version: u8,
    kind: &'static str,
    policy_name: &'static str,
    graph_id: &'a str,
    workspace_id: &'a Value,
    snapshot_id: &'a Value,
    policy: &'a Options,
    statistics: Statistics,
    candidates: Vec<Candidate<'a>>,
    concepts: Vec<ConceptProof<'a>>,
    evidence: Vec<EvidenceProof<'a>>,
    binding_validation: &'static str,
    provider_attestation: &'static str,
    semantic_identity_established: bool,
    merges_applied: u8,
    complete: bool,
    limitations: [&'static str; 3],
}

pub(crate) fn normalized(raw: &str, d: &Deadline) -> Result<String> {
    let mut compatible = String::new();
    for (index, ch) in raw.nfkc().enumerate() {
        d.check()?;
        ensure!(index < 512, "Meaning alias normalization exceeds capacity");
        compatible.push(ch);
    }
    // Whole-string lowercase retains contextual forms such as final sigma.
    // The admitted intermediate is at most 512 scalars before this allocation.
    let lowercase = compatible.to_lowercase();
    d.check()?;
    let mut out = String::new();
    let mut separator = false;
    let mut count = 0;
    // Lowercasing is deliberately not Unicode case folding or transliteration.
    for ch in lowercase.nfkc() {
        d.check()?;
        count += 1;
        ensure!(count <= 512, "Meaning alias normalization exceeds capacity");
        if ch.is_alphanumeric() || unicode_normalization::char::is_combining_mark(ch) {
            if separator && !out.is_empty() {
                out.push(' ');
            }
            out.push(ch);
            separator = false;
        } else {
            separator = true;
        }
    }
    Ok(out)
}

fn term<'a>(raw: &'a str, kind: &'static str, d: &Deadline) -> Result<Option<Term<'a>>> {
    let normalized = normalized(raw, d)?;
    if normalized.is_empty() {
        return Ok(None);
    }
    let chars: Vec<_> = normalized.chars().collect();
    let grams = chars.windows(3).map(|s| [s[0], s[1], s[2]]).collect();
    Ok(Some(Term {
        raw,
        kind,
        normalized,
        chars,
        grams,
    }))
}

pub(crate) fn jaro_winkler(left: &[char], right: &[char], d: &Deadline) -> Result<u16> {
    if left == right {
        return Ok(10000);
    }
    // Short names need equality: fuzzy spelling is too weak for this policy.
    if left.len().min(right.len()) < 4 {
        return Ok(0);
    }
    let window = (left.len().max(right.len()) / 2).saturating_sub(1);
    let mut left_matched = vec![false; left.len()];
    let mut right_matched = vec![false; right.len()];
    let mut matches = 0u64;
    for (i, ch) in left.iter().enumerate() {
        d.check()?;
        for j in i.saturating_sub(window)..right.len().min(i + window + 1) {
            if !right_matched[j] && *ch == right[j] {
                left_matched[i] = true;
                right_matched[j] = true;
                matches += 1;
                break;
            }
        }
    }
    if matches == 0 {
        return Ok(0);
    }
    let mismatches = left
        .iter()
        .zip(&left_matched)
        .filter(|(_, used)| **used)
        .map(|(ch, _)| ch)
        .zip(
            right
                .iter()
                .zip(&right_matched)
                .filter(|(_, used)| **used)
                .map(|(ch, _)| ch),
        )
        .filter(|(a, b)| a != b)
        .count() as u64;
    let (a, b) = (left.len() as u64, right.len() as u64);
    // Rational arithmetic preserves half-transpositions and deterministic floors.
    let numerator = 2 * matches * matches * (a + b) + (2 * matches - mismatches) * a * b;
    let denominator = 6 * matches * a * b;
    let prefix = if numerator * 10 > denominator * 7 {
        left.iter()
            .zip(right)
            .take(4)
            .take_while(|(a, b)| a == b)
            .count() as u64
    } else {
        0
    };
    Ok(((numerator * (10 - prefix) + denominator * prefix) * 10000 / (10 * denominator)) as u16)
}

fn trigram_jaccard(left: &Term<'_>, right: &Term<'_>) -> u16 {
    if left.chars.len().min(right.chars.len()) < 4 {
        return 0;
    }
    let intersection = left.grams.intersection(&right.grams).count();
    let union = left.grams.len() + right.grams.len() - intersection;
    (intersection * 10000 / union) as u16
}

fn report(graph: &Value, graph_id: &str, options: &Options, d: &Deadline) -> Result<Value> {
    d.check()?;
    validate_options(options)?;
    let packets = graph["packets"]
        .as_array()
        .context("Meaning alias packets missing")?;
    let count: usize = packets
        .iter()
        .map(|p| p["meaning"]["concepts"].as_array().map_or(0, Vec::len))
        .sum();
    ensure!(
        count <= options.max_concepts,
        "Meaning alias concept capacity exceeded"
    );
    let pairs = count * count.saturating_sub(1) / 2;
    ensure!(
        pairs <= options.max_pairs,
        "Meaning alias pair capacity exceeded"
    );
    let mut concepts = Vec::with_capacity(count);
    let mut unusable = 0;
    let mut usable = 0;
    let mut prefix_chars = 0u64;
    let mut work = 0u64;
    for (packet_index, packet) in packets.iter().enumerate() {
        d.check()?;
        let meaning = &packet["meaning"];
        let evidence: BTreeMap<_, _> = meaning["evidence"]
            .as_array()
            .context("Meaning alias evidence missing")?
            .iter()
            .map(|e| {
                Ok((
                    e["id"]
                        .as_str()
                        .context("Meaning alias evidence identity missing")?,
                    e,
                ))
            })
            .collect::<Result<_>>()?;
        for record in meaning["concepts"]
            .as_array()
            .context("Meaning alias concepts missing")?
        {
            d.check()?;
            let mut terms = Vec::new();
            let mut seen = BTreeSet::new();
            let label = record["label"]
                .as_str()
                .context("Meaning alias label missing")?;
            let aliases = record["aliases"]
                .as_array()
                .context("Meaning alias labels missing")?;
            for (raw, kind) in std::iter::once((label, "label"))
                .chain(aliases.iter().map(|a| (a.as_str().unwrap_or(""), "alias")))
            {
                match term(raw, kind, d)? {
                    Some(term) if seen.insert(term.normalized.clone()) => terms.push(term),
                    None => unusable += 1,
                    _ => {}
                }
            }
            terms.sort_by(|a, b| {
                (a.kind != "label", &a.normalized, a.raw).cmp(&(
                    b.kind != "label",
                    &b.normalized,
                    b.raw,
                ))
            });
            usable += terms.len();
            let chars = terms.iter().map(|t| t.chars.len() as u64).sum::<u64>();
            work = work
                .checked_add(
                    prefix_chars
                        .checked_mul(chars)
                        .context("Meaning alias work overflow")?,
                )
                .context("Meaning alias work overflow")?;
            ensure!(
                work <= options.max_similarity_work,
                "Meaning alias similarity work capacity exceeded"
            );
            prefix_chars += chars;
            let support = record["evidenceIds"]
                .as_array()
                .context("Meaning alias support missing")?
                .iter()
                .map(|id| {
                    evidence
                        .get(
                            id.as_str()
                                .context("Meaning alias support identity missing")?,
                        )
                        .copied()
                        .context("Meaning alias support absent")
                })
                .collect::<Result<_>>()?;
            concepts.push(Concept {
                reference: Reference {
                    packet_index,
                    concept_id: record["id"]
                        .as_str()
                        .context("Meaning alias concept identity missing")?,
                },
                record,
                evidence: support,
                terms,
            });
        }
    }
    concepts.sort_by_key(|c| c.reference);
    let mut candidates = Vec::new();
    let mut selected = BTreeSet::new();
    for (index, left) in concepts.iter().enumerate() {
        for right in &concepts[index + 1..] {
            d.check()?;
            let mut best: Option<Candidate<'_>> = None;
            for a in &left.terms {
                for b in &right.terms {
                    d.check()?;
                    let jw = jaro_winkler(&a.chars, &b.chars, d)?;
                    let gram = trigram_jaccard(a, b);
                    let score = jw.max(gram);
                    if score < options.min_score_bps
                        || best.as_ref().is_some_and(|c| c.score_bps >= score)
                    {
                        continue;
                    }
                    best = Some(Candidate {
                        left: left.reference,
                        right: right.reference,
                        score_bps: score,
                        basis: if a.normalized == b.normalized {
                            "normalized_equal"
                        } else if jw >= gram {
                            "jaro_winkler"
                        } else {
                            "trigram_jaccard"
                        },
                        jaro_winkler_bps: jw,
                        trigram_jaccard_bps: gram,
                        matched_terms: MatchedTerms {
                            left: a.raw,
                            right: b.raw,
                            left_kind: a.kind,
                            right_kind: b.kind,
                        },
                    });
                }
            }
            if let Some(candidate) = best {
                ensure!(
                    candidates.len() < options.max_candidates,
                    "Meaning alias candidate capacity exceeded"
                );
                selected.insert(left.reference);
                selected.insert(right.reference);
                candidates.push(candidate);
            }
        }
    }
    candidates.sort_by(|a, b| {
        b.score_bps
            .cmp(&a.score_bps)
            .then_with(|| (a.left, a.right).cmp(&(b.left, b.right)))
    });
    let mut proof_concepts = Vec::new();
    let mut proof_evidence = BTreeMap::new();
    for concept in &concepts {
        if !selected.contains(&concept.reference) {
            continue;
        }
        proof_concepts.push(ConceptProof {
            reference: concept.reference,
            record: concept.record,
        });
        for evidence in &concept.evidence {
            proof_evidence.insert(
                (
                    concept.reference.packet_index,
                    evidence["id"]
                        .as_str()
                        .context("Meaning alias evidence identity missing")?,
                ),
                *evidence,
            );
        }
    }
    let result = Report {
        schema_version: 1,
        kind: "meaning_alias_candidates_v1",
        policy_name: "lexical_alias_v1",
        graph_id,
        workspace_id: &graph["workspaceId"],
        snapshot_id: &graph["snapshotId"],
        policy: options,
        statistics: Statistics {
            concept_count: count,
            usable_terms: usable,
            unusable_terms: unusable,
            pair_comparisons: pairs,
            similarity_work: work,
            candidate_count: candidates.len(),
        },
        candidates,
        concepts: proof_concepts,
        evidence: proof_evidence
            .into_iter()
            .map(|((packet_index, _), record)| EvidenceProof {
                packet_index,
                record,
            })
            .collect(),
        binding_validation: "exact_captured_source_replayed",
        provider_attestation: "unverified",
        semantic_identity_established: false,
        merges_applied: 0,
        complete: false,
        limitations: [
            "Lexical similarity and provider-supplied aliases propose review candidates; they do not establish semantic identity.",
            "All admitted concept pairs were compared. Thresholds, short-name rules and selected inputs limit discovery; semantic coverage is not exhaustive.",
            "Original concepts, directed relations, ordered hyperedges and source evidence remain unchanged. No model was called.",
        ],
    };
    // Admit borrowed evidence before making any owned output copy.
    let bytes = crate::json::encode_bounded(&result, options.max_output_bytes, d)
        .context("Meaning alias output exceeds capacity")?;
    let value = crate::json::parse(&bytes, options.max_output_bytes)?;
    d.check()?;
    Ok(value)
}

#[cfg(test)]
#[path = "meaning_aliases_tests.rs"]
mod tests;
