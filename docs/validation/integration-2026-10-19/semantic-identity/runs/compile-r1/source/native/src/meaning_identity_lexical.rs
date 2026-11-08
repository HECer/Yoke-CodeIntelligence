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
