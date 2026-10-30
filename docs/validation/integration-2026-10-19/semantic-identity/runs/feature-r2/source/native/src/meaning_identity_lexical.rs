//! Deterministic independently specified MinHash blocking; scores are not probabilities.
use super::*;
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Match {
    pub left_term: String,
    pub right_term: String,
    pub left_kind: String,
    pub right_kind: String,
    pub normalized_equal: bool,
    pub jaro_winkler_bps: u16,
    pub minhash_bps: u16,
    pub matching_lsh_bands: usize,
}
struct Term {
    raw: String,
    kind: &'static str,
    normalized: String,
    chars: Vec<char>,
    signature: [u64; 64],
}
fn signature(chars: &[char], work: &mut u64, o: &Options, d: &Deadline) -> Result<[u64; 64]> {
    let compact: Vec<_> = chars
        .iter()
        .filter(|c| !c.is_whitespace())
        .copied()
        .collect();
    let shingles: BTreeSet<String> = if compact.len() < 3 {
        BTreeSet::from([compact.iter().collect()])
    } else {
        compact.windows(3).map(|w| w.iter().collect()).collect()
    };
    let mut signature = [u64::MAX; 64];
    for (seed, minimum) in signature.iter_mut().enumerate() {
        d.check()?;
        for s in &shingles {
            charge(work, 1, o)?;
            let digest = hash(encode(&("meaning-identity-minhash-v1", seed, s), 4096, d)?);
            let value = u64::from_str_radix(&digest[..16], 16).map_err(|_| fail(BINDING))?;
            *minimum = (*minimum).min(value);
        }
    }
    Ok(signature)
}
fn charge(work: &mut u64, n: u64, o: &Options) -> Result<()> {
    *work = work.checked_add(n).ok_or_else(|| fail(CAPACITY))?;
    require(*work <= o.max_similarity_work, CAPACITY)
}
fn terms(record: &Value, work: &mut u64, o: &Options, d: &Deadline) -> Result<Vec<Term>> {
    let mut terms = Vec::new();
    let mut seen = BTreeSet::new();
    for (raw, kind) in std::iter::once((text(record, "label")?, "label")).chain(
        rows(record, "aliases")?
            .iter()
            .map(|v| (v.as_str().unwrap_or(""), "alias")),
    ) {
        d.check()?;
        let normalized = crate::meaning_aliases::normalized(raw, d).map_err(|_| fail(CAPACITY))?;
        if normalized.is_empty() || !seen.insert(normalized.clone()) {
            continue;
        }
        let chars = normalized.chars().collect::<Vec<_>>();
        let signature = signature(&chars, work, o, d)?;
        terms.push(Term {
            raw: raw.into(),
            kind,
            normalized,
            chars,
            signature,
        });
    }
    terms.sort_by(|a, b| {
        (a.kind != "label", &a.normalized, &a.raw).cmp(&(b.kind != "label", &b.normalized, &b.raw))
    });
    Ok(terms)
}
pub(super) fn discover(
    concepts: &BTreeMap<ConceptRef, &Value>,
    r: &Request,
    o: &Options,
    d: &Deadline,
) -> Result<(Vec<Candidate>, u64)> {
    let mut work = 0;
    let mut terms_by_ref = BTreeMap::new();
    for (reference, record) in concepts {
        terms_by_ref.insert(reference, terms(record, &mut work, o, d)?);
    }
    let refs = concepts.keys().collect::<Vec<_>>();
    let mut candidates = Vec::new();
    let additional: BTreeSet<_> = r
        .additional_pairs
        .iter()
        .map(|p| (&p.left, &p.right))
        .collect();
    for i in 0..refs.len() {
        for j in i + 1..refs.len() {
            d.check()?;
            let mut best: Option<Match> = None;
            for a in &terms_by_ref[refs[i]] {
                for b in &terms_by_ref[refs[j]] {
                    charge(&mut work, 64, o)?;
                    let same = a.normalized == b.normalized;
                    let matching = a
                        .signature
                        .iter()
                        .zip(&b.signature)
                        .filter(|(a, b)| a == b)
                        .count();
                    let mh = (matching * 10000 / 64) as u16;
                    let bands = a
                        .signature
                        .chunks_exact(4)
                        .zip(b.signature.chunks_exact(4))
                        .filter(|(a, b)| a == b)
                        .count();
                    if !same && (bands == 0 || mh < r.candidate_policy.min_minhash_bps) {
                        continue;
                    }
                    charge(&mut work, (a.chars.len() * b.chars.len()) as u64, o)?;
                    let jw = crate::meaning_aliases::jaro_winkler(&a.chars, &b.chars, d)
                        .map_err(|_| fail(BINDING))?;
                    if !same && jw < r.candidate_policy.min_jaro_winkler_bps {
                        continue;
                    }
                    if best.as_ref().is_none_or(|m| {
                        (same, jw, mh) > (m.normalized_equal, m.jaro_winkler_bps, m.minhash_bps)
                    }) {
                        best = Some(Match {
                            left_term: a.raw.clone(),
                            right_term: b.raw.clone(),
                            left_kind: a.kind.into(),
                            right_kind: b.kind.into(),
                            normalized_equal: same,
                            jaro_winkler_bps: jw,
                            minhash_bps: mh,
                            matching_lsh_bands: bands,
                        });
                    }
                }
            }
            let explicit = additional.contains(&(refs[i], refs[j]));
            if best.is_some() || explicit {
                require(candidates.len() < o.max_candidates, CAPACITY)?;
                let id = identity(&("meaning-identity-pair-v1", refs[i], refs[j]));
                candidates.push(Candidate {
                    id,
                    left: refs[i].clone(),
                    right: refs[j].clone(),
                    lexical: best,
                    explicit_host_candidate: explicit,
                });
            }
        }
    }
    Ok((candidates, work))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn reference(n: usize) -> ConceptRef {
        ConceptRef {
            graph_id: "1".repeat(64),
            packet_index: n,
            concept_id: format!("{n:064x}"),
        }
    }
    fn request() -> Request {
        Request {
            schema_version: 1,
            selection: Selection {
                workspace_id: "2".repeat(64),
                snapshot_id: "3".repeat(64),
                base_graph_hash: "4".repeat(64),
                graph_ids: vec!["1".repeat(64)],
            },
            candidate_policy: Default::default(),
            additional_pairs: vec![],
        }
    }
    #[test]
    fn independent_sha256_minhash_vector_and_lsh_bands_are_stable() {
        let d = Deadline::new(10000);
        let o = Options::default();
        let mut work = 0;
        let a = signature(
            &"payment storage".chars().collect::<Vec<_>>(),
            &mut work,
            &o,
            &d,
        )
        .unwrap();
        let b = signature(
            &"paymnt storage".chars().collect::<Vec<_>>(),
            &mut work,
            &o,
            &d,
        )
        .unwrap();
        assert_eq!(
            a[..4],
            [
                1672820719266575881,
                291082007023568740,
                1986713373336965090,
                91392775846455974
            ]
        );
        assert_eq!(
            b[..4],
            [
                58814215100374200,
                291082007023568740,
                3275388386949508747,
                91392775846455974
            ]
        );
        assert_eq!(a.iter().zip(&b).filter(|(a, b)| a == b).count(), 43);
        assert_eq!(
            a.chunks_exact(4)
                .zip(b.chunks_exact(4))
                .filter(|(a, b)| a == b)
                .count(),
            3
        );
    }
    #[test]
    fn fuzzy_candidates_require_both_blocking_and_verification_and_keep_original_terms() {
        let a = json!({"label":"payment storage","aliases":[]});
        let b = json!({"label":"paymnt storage","aliases":[]});
        let concepts = BTreeMap::from([(reference(0), &a), (reference(1), &b)]);
        let r = request();
        let d = Deadline::new(10000);
        let (c, _) = discover(&concepts, &r, &Default::default(), &d).unwrap();
        assert_eq!(c.len(), 1);
        let m = c[0].lexical.as_ref().unwrap();
        assert_eq!(m.minhash_bps, 6718);
        assert_eq!(m.matching_lsh_bands, 3);
        assert!(!m.normalized_equal);
        assert_eq!(m.left_term, "payment storage");
        assert!(m.jaro_winkler_bps >= 9000);
        let mut r = r;
        r.candidate_policy.min_jaro_winkler_bps = 10000;
        assert!(
            discover(&concepts, &r, &Default::default(), &d)
                .unwrap()
                .0
                .is_empty()
        );
        r.additional_pairs.push(Pair {
            left: reference(0),
            right: reference(1),
        });
        let (c, _) = discover(&concepts, &r, &Default::default(), &d).unwrap();
        assert!(c[0].lexical.is_none());
        assert!(c[0].explicit_host_candidate);
    }
    #[test]
    fn unicode_mark_distinctions_alias_priority_and_zero_limits_are_preserved() {
        let a = json!({"label":"ＣＡＦÉ","aliases":["café"]});
        let b = json!({"label":"café","aliases":[]});
        let concepts = BTreeMap::from([(reference(0), &a), (reference(1), &b)]);
        let d = Deadline::new(10000);
        let r = request();
        let o = Options::default();
        let (c, _) = discover(&concepts, &r, &o, &d).unwrap();
        let m = c[0].lexical.as_ref().unwrap();
        assert!(m.normalized_equal);
        assert_eq!(m.left_kind, "label");
        assert_eq!(m.left_term, "ＣＡＦÉ");
        assert_ne!(
            crate::meaning_aliases::normalized("कि", &d).unwrap(),
            crate::meaning_aliases::normalized("की", &d).unwrap()
        );
        assert_ne!(
            crate::meaning_aliases::normalized("Straße", &d).unwrap(),
            crate::meaning_aliases::normalized("STRASSE", &d).unwrap()
        );
        assert!(
            discover(
                &concepts,
                &r,
                &Options {
                    max_candidates: 0,
                    ..o.clone()
                },
                &d
            )
            .is_err()
        );
        assert!(
            discover(
                &concepts,
                &r,
                &Options {
                    max_similarity_work: 0,
                    ..o
                },
                &d
            )
            .is_err()
        );
    }
    #[test]
    fn closed_canonical_reference_requests_and_deadlines_refuse_before_discovery() {
        let mut r = request();
        r.selection.graph_ids = vec!["2".repeat(64), "1".repeat(64)];
        assert!(validate_request(&r, &Deadline::new(1000)).is_err());
        r = request();
        r.additional_pairs = vec![Pair {
            left: reference(1),
            right: reference(0),
        }];
        assert!(validate_request(&r, &Deadline::new(1000)).is_err());
        let mut r = serde_json::to_value(request()).unwrap();
        r["trusted"] = json!(true);
        assert!(crate::json::decode::<Request>(r).is_err());
        assert!(crate::json::parse(br#"{"schemaVersion":1,"schemaVersion":1}"#, 128_000).is_err());
        assert!(validate_request(&request(), &Deadline::new(0)).is_err());
    }
}
