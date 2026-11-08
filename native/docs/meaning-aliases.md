# Lexical alias candidates for a published meaning graph

`meaning aliases GRAPH_ID` compares concept labels and their provider-supplied
aliases in one already published native meaning graph. It replays the original
source, job, cache and response closure before doing any comparison. It never
starts a provider, captures current files, writes records or merges concepts.
The library entry point is `meaning_aliases::discover`.

```powershell
yoke --root C:/Work/project --max-bytes 4000000 meaning aliases GRAPH_ID
yoke --root C:/Work/project meaning aliases GRAPH_ID --min-score-bps 9500 --max-candidates 200
```

This command requires an initialized workspace store. Its host flags follow the
existing meaning read commands: source capture, global snapshot selection,
language-server configuration and source-write flags are refused. Graph reads
remain usable when the original live files or provider executable are absent.

## Comparison policy

`lexical_alias_v1` applies Unicode NFKC, contextual whole-string Unicode lowercase
conversion and NFKC again, then coalesces punctuation/whitespace separators. Combining marks remain
part of a term. This is neither full Unicode case folding nor transliteration:
accented forms, different scripts and `ß` versus `ss` remain distinct. An empty
normalized term is counted as unusable. Repeated normalized aliases within a
concept are compared once; the label takes precedence over an equivalent alias.

Every admitted pair of distinct concept references is considered. References
contain both the original packet index and concept ID. Scores compare all usable
label/alias combinations, retaining the first maximum under the deterministic
label-first, normalized-term, original-term order. Scores use a 0–10,000 integer
scale and never denote a confidence probability:

- Equal normalized terms score 10,000 with `basis: normalized_equal`.
- Other terms of at least four Unicode scalar values use the greater of
  Jaro–Winkler and character-trigram set Jaccard similarity.
- Jaro–Winkler uses a matching radius of `floor(max_length/2)-1`, half the
  mismatched matched-character count as transpositions, at most four prefix
  characters, and prefix scaling 0.1 only when the Jaro score exceeds 0.7.
  Rational integer arithmetic floors the final basis-point result.
- Trigram Jaccard is `floor(10000 * intersection_size / union_size)` over
  distinct, consecutive three-character sequences, without boundary padding.
- Unequal terms shorter than four characters receive zero fuzzy similarity.

Results retain the original matched terms and whether each came from a label or
alias, both metric scores and the winning basis. Candidates sort by descending
score, then by their ordered pair of packet/concept references.

## Admission and output

| Option | Default | Maximum |
|---|---:|---:|
| `--min-score-bps` | 9,000 | 10,000; minimum 7,000 |
| `--max-concepts` | 512 | 1,024 |
| `--max-pairs` | 65,536 | 262,144 |
| `--max-candidates` | 4,096 | 8,192 |
| `--max-similarity-work` | 16,000,000 | 64,000,000 |
| Global `--max-bytes` | Existing CLI default | 4,000,000 including newline |

Each capacity remains independent; for example, 512 concepts require 130,816
pairs and therefore exceed the default pair budget. Zero is a real capacity,
including for an empty admitted graph. Capacity exhaustion refuses the whole
operation; it does not return a clipped top-K or partial supporting evidence.

Both NFKC stages are limited to 512 emitted scalar values per original term.
Before pair scoring, the algorithm admits the number of concept pairs and the
sum, across different concepts, of every normalized term-length product. That
conservative work bound also covers fuzzy matching windows. Deadlines and host
cancellation apply to normalization, comparison and serialization. This bounds
the comparison stage; existing meaning replay retains its own graph and receipt
limits rather than becoming a new streaming loader.

The `meaning_alias_candidates_v1` result includes the parent graph/workspace/
snapshot identities, effective policy, counters, candidate references, original
concept records and deduplicated original evidence records. Evidence remains
qualified by packet index; quotes, original UTF-8 byte offsets, paths and content
hashes are not normalized. Large proof values are borrowed and counted before
allocating their serialized output. Oversized output fails with no overflow
artifact or new record.

`bindingValidation: exact_captured_source_replayed` describes replay of the
parent graph. It does not verify a provider's interpretation or alias assertion.
`providerAttestation: unverified`, `semanticIdentityEstablished: false`,
`mergesApplied: 0` and `complete: false` remain explicit. Homonyms, unrelated
concepts with similar names, missing synonyms and incomplete selected inputs
remain possible. Directed relations and ordered-role hyperedges stay unchanged
in the parent graph. Explicit host selections can be represented using the
separate [group projection command](meaning-groups.md), preserving every original
claim and role slot. Such selections are host assertions, not authenticated
review or semantic identity proof. Automatic semantic identity decisions,
cross-graph deduplication and model-assisted tie-breaking remain separate work.
