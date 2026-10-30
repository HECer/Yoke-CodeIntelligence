# Explicit model-adjudicated Meaning identity

`meaning_identity` and the separate `meaning-identity` CLI implement a bounded
native identity policy over selected, already published Meaning claims. Source,
original extraction receipts and all selected graphs replay before candidate
discovery. An explicitly trusted host provider then adjudicates the complete
candidate set in one separately accounted attempt. Loading a result starts no
provider. Default indexing and the eight standard MCP tools are unchanged.

This adds model-based equivalence under an explicit policy. It does not establish
semantic truth, authenticated review, compiler identity or observed execution.
Original parent claims are immutable. Existing aliases, groups, native joins,
portable projections and mixed graph composition keep their released formats.

## Source selection and host permission

```text
yoke --root REPOSITORY --max-bytes 16000000 meaning-identity candidates --request REQUEST.json
yoke --root REPOSITORY --allow-writes meaning-identity build --request REQUEST.json --config PROVIDER.json --policy POLICY.json --trust-provider
yoke --root REPOSITORY --max-bytes 16000000 meaning-identity load --job JOB_ID
yoke --root REPOSITORY --max-bytes 16000000 meaning-identity load --job JOB_ID --query '{"operation":"degree"}'
yoke --root REPOSITORY --max-bytes 16000000 meaning-identity status --job JOB_ID
yoke --root REPOSITORY meaning-identity jobs --task TASK_ID --offset 0 --limit 25
```

Build requires global `--allow-writes` and separate `--trust-provider` before
host-file reads or Store access. Read operations refuse `--allow-writes`. All
commands require an existing initialized Store and refuse capture, global
snapshot, analyzer and language-server configuration. No current source capture
or project code execution is performed. Provider executables are explicitly
trusted host programs; an owned working directory does not enforce filesystem
or network isolation. `sourceWritten:false` applies to the native parent only.

The closed request is:

```json
{
  "schemaVersion": 1,
  "selection": {
    "workspaceId": "WORKSPACE_SHA256",
    "snapshotId": "CAPTURED_SNAPSHOT_SHA256",
    "baseGraphHash": "CANONICAL_GRAPH_SHA256",
    "graphIds": ["PUBLISHED_GRAPH_SHA256"]
  },
  "candidatePolicy": {
    "minJaroWinklerBps": 9000,
    "minMinhashBps": 5000
  },
  "additionalPairs": []
}
```

Select 1..4 distinct graph IDs in increasing lexical order from one exact native
workspace/canonical snapshot. Every selected original concept participates in
the finite pair scan; no first-N truncation is used. Each concept retains
`{graphId,packetIndex,conceptId}`. Original/evaluated histories and foreign
workspaces are not implicitly equated. This native source space does not admit
Office/PDF or portable data-only projections as checked sources.

Optional additional pairs have `left` and `right` concept references. Each pair
must resolve to selected original claims, have `left < right` in typed reference
order and be unique. These explicit host candidates bypass lexical filtering;
they do not assert equivalence. At most 256 additional pairs are supported.

The provider JSON uses existing closed host keys: `command`, optional `args`,
`modelId`, `modelVersion`, `taskId`, `currency`, `maxCostMicros`, `taskLimitMicros`.
There is no JSON trust field. Policy has `contractHash` (a host SHA-256 identity),
optional absolute `providerInputFiles` and optional `maxResultBytes` (default
16,000,000; range 16,000,000..64,000,000). Existing executable/input fingerprinting
binds the actual selected implementation. Credentials do not belong in these
files or arguments.

## Independently specified candidate policy

`native_nfkc_context_lower_minhash64_lsh16x4_jaro_v1` uses the released native
alias normalization: NFKC, contextual whole-string Unicode lowercase, NFKC again,
then punctuation/whitespace coalescing with combining marks retained. This is
not full Unicode case folding or transliteration. Accents, scripts and `ß`/`ss`
remain distinct. Each admitted normalized intermediate has at most 512 scalars.
Duplicate normalized aliases are compared once; the label has priority.

For each usable normalized term, remove spaces for MinHash shingles. Terms with
fewer than three remaining scalars use one whole-term shingle; otherwise use the
distinct consecutive character trigrams. For seed 0..63, hash compact UTF-8 JSON
`["meaning-identity-minhash-v1", seed, shingle]` with SHA-256. Interpret the first
eight digest bytes as an unsigned big-endian integer and take the minimum across
shingles. The 64 minima form sixteen consecutive four-value LSH bands.

Exact normalized equality always proposes a candidate, including short names.
Otherwise, at least one complete LSH band must match, MinHash estimate must meet
`minMinhashBps`, and native integer Jaro-Winkler must meet `minJaroWinklerBps`.
MinHash estimate is `floor(10000 * equal_minima / 64)`. Jaro-Winkler uses the
released native Unicode-scalar matching and rational floor arithmetic, with
short unequal names below four scalars scoring zero. All usable label/alias
combinations are considered. Ties retain the first label-priority, normalized-
term, original-term combination. Winning raw terms, source kinds and individual
metrics remain visible. Candidates are ordered by typed original references.

Scores and band matches are lexical heuristics, not probabilities or identity
proof. Approximate blocking and configured thresholds can omit semantic matches.
Use explicit additional pairs when desired. No lexical equality or threshold
automatically merges concepts. Empty candidate sets are inspectable without a
provider; build refuses them before executable lookup or reservation.

The historical pinned upstream inventory references
[Graphify dedup](https://github.com/Graphify-Labs/graphify/blob/fe085cedddde3a18554f8749b230bfe47f3dcce7/graphify/dedup.py).
This independently specified native policy does not claim the upstream's exact
normalization, thresholds, entropy/variant rules, lifecycle survivor selection,
model quality or complete contract equivalence.

## Provider contract and complete decision coverage

One bounded stdin request uses schema 1 and operation
`adjudicate_meaning_identity_v1`. It carries model/version/request/input/currency/
task/cost-limit fields and a payload with exact `sourceBinding`, original native
claim projection, request, complete candidate list, fixed algorithm/merge policy,
contract hash and `selectedClaimProjectionOmittedBytes:0`. This is complete
selected claim/evidence context, not an assertion that every raw code file was
submitted to the identity model.

The source binding contains the original selection, `sourceProjectionId`,
`sourceHash` and `candidateSetId`. The private prepared source capability replays
the real native joint closure. Its original singleton projection retains all
claims, packets, coverage, evidence and receipt provenance. Existing joint replay
reserves a stable 64,000-byte capacity per mutable ledger row instead of using
its changing serialized size. Later reconciliation therefore does not rebind
the original source projection. Current job/receipt input capacity is still
charged independently on every replay.

The closed reply repeats schema/operation/model/version/request/input/currency
and the exact `sourceBinding`. It may include `actualCostMicros`, `usage`, `data`
and `error`. Unknown/null cost is never interpreted as zero. Usage has
`inputTokens`, `outputTokens`, `cachedInputTokens`. The fragment is:

```json
{
  "decisions": [{
    "candidateId": "CANDIDATE_SHA256",
    "outcome": "same_entity",
    "rationale": "Unverified model interpretation for this selected context.",
    "support": [{
      "reference": { "graphId": "GRAPH_A", "packetIndex": 0, "conceptId": "CONCEPT_A" },
      "evidenceId": "ORIGINAL_EVIDENCE_A"
    }, {
      "reference": { "graphId": "GRAPH_B", "packetIndex": 1, "conceptId": "CONCEPT_B" },
      "evidenceId": "ORIGINAL_EVIDENCE_B"
    }]
  }]
}
```

Every candidate requires exactly one decision: `same_entity`,
`different_entities` or `uncertain`. Unknown, omitted or duplicate candidates
refuse the whole result. Rationale is nonblank and at most 1,024 UTF-8 bytes;
each decision has 2..16 unique supports with at least one on each side. Every
support must name actual evidence of that exact original selected concept.
The source replay supplies original text/hash/ranges; support IDs are not an
independent semantic verification or execution observation. Decision order and
the complete original fragment remain unchanged.

## Lossless quotient and recovery

`complete_same_entity_clique_or_refuse` considers connected positive decisions,
then requires a positive decision for every unordered pair within each component.
A negative, uncertain or unassessed pair inside such a component refuses the
whole result. A≈B and B≈C alone do not silently establish A≈C. Disconnected
singletons remain separate. Representatives use the first typed member reference
for deterministic presentation; original labels/descriptions/aliases are retained.

Every original concept remains exactly once in node membership. Every original
directed relation retains its packet/graph-qualified reference and complete
record, with both original projected endpoints and new quotient endpoints.
Parallel, reverse and self observations are never deduplicated. Every hyperedge
keeps its original record and each ordered participant position, role, original
reference and old/new projected endpoint. Even a complete collapse to one new
node preserves every slot. This is its own quotient schema, not an ordinary
MeaningGraph with a distinct-participant constraint.

Atomic ledger/job/payload registration precedes a single-use send. No transaction
spans the process. Bound costs are recorded before usage/domain validation; the
raw reply/responded phase is durable before semantic projection. Provider errors,
invalid usage/domain, nonzero exit and overrun preserve a valid bound bill. An
overrun is retained in full and blocks later task sends. Unbound/incomplete
responses keep unknown reservations; only a durably never-sent attempt may cancel.
There is no cache, automatic retry or resume. Each explicit new build has a new
nonce/job/attempt.

Build returns a small durable job reference. Lost stdout or post-commit cleanup
errors name the job; inspect status with a fresh deadline. Load independently
replays selected native source/receipts, exact payload/response binding,
task/attempt ledger and reconstructed quotient bytes. Later billing observations
are separate in status and do not upgrade original unknown response costs/usage.
`CheckedGraph` and `CheckedCandidates` have no JSON constructor; immutable bounded
exports remain usable after all source Stores and provider files are removed.
Jobs pagination checks all admitted job headers and explicitly makes no dependency-
replay claim.

`CheckedGraph::query` and CLI `load --query JSON` provide closed `node`, `degree`
and directed `trace` queries using original concept references. These reuse the
checked native join engine without changing its released output or 4-MB cap;
identity query results use their own kind and a 64-MB ceiling. Node lookup returns
the complete inferred group's original members. Degrees count every original
parallel/reverse/self relation and every ordered hyperedge slot. Trace follows
only directed relations (`outgoing`, `incoming`, `both`), never hyperedge roles.
It selects induced relations and fully contained hyperedges, preserving all
original decision/source/receipt metadata. Depth is 0..8, with 1..32 distinct
original roots and maxVisited at most 4,096 projected nodes. Overflow refuses
the whole traversal; multiple original roots in one group remain one projected
visited node. Queries also work after Store/provider loss and start no provider.
They establish no compiler call, runtime execution or semantic truth.

Typed retention pre-admits bounded job/linked registrations before legacy row
materialization, then validates full source/response/result closures for every
job state. Only typed roots are followed. New payload, response, result and job
text never grant arbitrary extra retention roots. Missing, corrupt or over-budget
dependencies refuse a checked result and retirement; no empty complete result
is substituted.

## Capacities and remaining qualification

Defaults/maxima: 128 original concepts, 8,128 selected unordered pairs, 256
candidates, 64,000,000 / 128,000,000 similarity work units, 64,000,000 /
256,000,000 total input bytes. Work charges every seeded shingle hash, 64 units
per signature comparison and the scalar-product bound per Jaro verification.
Limits may be lowered, including to zero. Original counts apply before grouping.
All comparisons, serialization and replay share the caller's absolute deadline.

Native source preparation retains its existing 32,000,000-byte joint dependency
and 64,000,000-byte frozen source limits, with a 4,000,000-byte source projection.
Payload, raw reply and data fragment caps are 8,000,000, 2,000,000 and 1,000,000
bytes; jobs are at most 256,000 bytes and at most 1,000 headers / 32,000,000
namespace bytes. The full prospective reply/result read budget and durable
reference output are admitted before reservation. Current lower read limits
refuse whole results. CLI output accepts 1,024..64,000,000 bytes including newline;
the ordinary CLI default may be too small for a full proof. No overflow artifact
or clipped complete traversal is created by reads.

Owned Node fixtures qualify protocol, accounting, policy and source/recovery
behavior. Live semantic accuracy, independent billing, authenticated corroboration,
mixed/historical/workspace-wide identity, further upstream rules and full platform
acceptance remain separately unqualified. `semanticIdentityEstablished`,
`canonicalSourceAuthority`, `executionObserved` and `complete` remain false;
`selectionComplete` covers only the finite requested policy result.
