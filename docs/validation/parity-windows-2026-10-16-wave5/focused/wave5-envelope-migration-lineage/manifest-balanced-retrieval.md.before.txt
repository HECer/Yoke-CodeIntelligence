# Manifest-balanced source retrieval

This document defines the finite `yoke-retrieval-scopes-1` policy. It describes
source ownership and ranking within a captured snapshot. Scores are relevance
heuristics; package labels, scope IDs and ranking positions do not establish
compiler binding, runtime behavior or complete reference coverage.

## Select a mode

Session MCP `ci_retrieve` accepts an optional `scopeMode`:

| Value | Behavior |
|---|---|
| Absent or `"global"` | Existing global retrieval, without manifest probing or additive `retrievalScope` metadata. The same other arguments and inputs retain the legacy result bytes. |
| `"manifest_balanced_v1"` | Require a usable captured package partition and rank within physical scopes. A policy that cannot produce that partition is an error. There is no automatic global fallback. |
| `null`, non-string, `"auto"`, or any other string | `invalid_request: scope_mode`, before selecting or indexing a snapshot. |

For example, these are `ci_retrieve` arguments:

```json
{
  "query": "payment",
  "scopeMode": "manifest_balanced_v1",
  "relativePath": "packages",
  "limit": 10,
  "includeBody": true
}
```

The ordinary native profile still has eight tools. The session profile adds no
tool for this feature. Existing query, filter, result-count and timeout options
keep their contracts; `snapshotId` selects immutable captured input rather than
consulting live manifests.

The six public Trail/Graft tool schemas are unchanged. `trail_find_code` and
`graft_find_code` privately choose automatic mode. They have no public
`scopeMode` argument: supplying one is an argument error. Auto uses balanced
ranking only when a checked partition has at least two query-eligible scopes.
The existing eight-line crux or `full: true` definition conversion remains in
place. Auto results include `retrievalScope`, including when global ranking is
selected.

## Captured package policy

Only admitted captured paths and their exact hash-verified bytes are inputs.
There is no live ancestor search, package-manager invocation, dependency
resolution, project execution or model/provider call. A manifest cannot expand
the capture, include an ignored path, change the host root or grant access.
Existing read-only and explicitly admitted ignored-source policies still apply.

| Captured marker | Supported interpretation |
|---|---|
| `package.json` | Strict JSON object. Defines an actual package root, except that an unnamed npm marker at any supported workspace boundary at the same root is virtual, including a boundary declared by npm, pnpm or Cargo. An optional name is an untrusted label; an absent name on an actual package uses the path label. |
| `package.json` `workspaces` | A string array, or an object containing exactly `packages` with a string array. Declares a workspace boundary. |
| `pnpm-workspace.yaml` | One mapping containing exactly `packages` with a string list. Declares a virtual boundary unless an actual package marker exists at the same root. Tags, aliases, anchors and merge keys are unsupported. |
| `Cargo.toml` | Strict TOML. `[package]` defines an actual package; `[workspace]` declares a boundary with optional string-array `members` and `exclude`. Other workspace fields do not alter v1 ownership. A marker with neither table is unsupported. |

Present empty, non-string or control-containing package names are unsupported
source schema. Cargo `package.workspace` pointers, including `"."`, are
unsupported; path dependencies do not create membership. Duplicate keys and
malformed marker syntax produce policy diagnostics, subject to the hard checks
below.

`pnpm-workspace.yml`, `lerna.json`, `pyproject.toml`, `go.mod`, `pom.xml` and
`*.csproj` are other-format hints, not additional parsers. With no supported
actual package roots, these hints produce `unsupported_repository_format`.
Otherwise they remain ordinary captured files. A `.yml` pnpm hint conflicts
with supported roots and is unsupported policy. Mixed-repository scope coverage
therefore remains limited to the supported npm/pnpm and Cargo markers.

Selectors match existing captured package-root paths relative to the declaring
workspace, rather than files or prospective directories. Supported syntax is
`.` alone, or slash-separated literal portable components, `*` as one complete
component, and `**` only as the last complete component. `*` matches one nonempty
component; final `**` matches zero or more. Exact Unicode text and internal
spaces are permitted. Absolute, drive and UNC paths, backslashes, empty or
`.`/`..` components, controls, reserved Windows punctuation, component-edge
spaces, trailing dots, `~`, inline stars, `?`, brackets, braces, extglobs and
escapes are unsupported. Selectors and ownership use exact UTF-8 comparisons,
without case folding or Unicode normalization.

pnpm alone supports one leading `!`. Positive matches are unioned and negative
matches subtracted, independent of list order; later positives do not re-include
an excluded root. npm and Cargo members do not accept `!`; Cargo uses its
separate `exclude` list. Duplicate selectors are deduplicated for matching but
every raw entry is charged. Missing members yield bounded `missing_member`
diagnostics. Co-located npm/pnpm declarations require identical normalized
positive sets and no pnpm negatives; otherwise policy is conflicting. Both
bindings are retained and pnpm is primary.

Workspace membership and physical ownership are distinct. Membership uses the
nearest same-ecosystem workspace ancestor, including the same root, without
crossing a nested workspace boundary. An actual root package belongs to its own
workspace; any supported negative selector that matches this root is a conflict,
including literal `.` and a pattern such as `**`. Excluded and unmatched
captured packages remain independent ranking scopes. Duplicate package names
at different paths remain distinct.

Physical ownership uses the deepest actual package or virtual workspace
boundary. An actual package wins at the same root. A virtual boundary owns its
otherwise-unassigned files; remaining files belong to the root `.` unassigned
scope. Co-located ecosystems share one physical ranking scope. The scope kinds
are `package`, `workspace_unassigned` and `root_unassigned`; any can participate
when query-eligible. A usable partition requires at least two actual physical
package roots. Unassigned scopes do not turn one package into a monorepo.

## Whole-source and whole-document admission

The checked retrieval run has one borrowed deadline, source cache and debit
ledger through probing and any auto fallback. Its source-input allowance is
32,000,000 bytes, including a maximum 4,000,000 bytes reserved for manifests.
Unused reserve is released after the probe. Manifest paths are verified even
when query filters exclude them. Each canonical path is charged once; two paths
sharing a CAS blob are still separate input paths. Reusing a verified marker
for scoring has zero incremental source cost.

Apply query filters before forming eligible scope queues. Sort scopes by root
and sources by canonical path. Divide the available input allowance equally
among nonempty queues, giving remainder bytes to the first roots. Visit scopes
in order and admit one fitting whole file per pass from each protected share.
Defer oversized heads so that a later small file can fit. Then pool unused
shares and repeat round-robin admission over remaining canonical queues. Each
unit is scanned at most once per phase. A unit that cannot fit is omitted
whole, with a source-budget reason.

Document admission uses the same two-phase schedule with a separate
32,000,000-byte allowance. Documents are entire original entity bodies, ordered
within scopes by `(path, startByte, endByte, entityId)`. Overlapping bodies are
charged independently. File entities are excluded when that file has nonfile
entities. Invalid source hashes or ranges are hard errors. Display clipping is
later and does not alter the scoring corpus.

This gives each eligible scope protected opportunity. It does not establish
that every relevant source was read: a whole file or document can exceed a
share or the remaining pooled budget. Auto global fallback retains the legacy
stored-order admission schedule and scoring formulas, with the same already
spent ledger. Four megabytes of markers outside the query leave 28,000,000
bytes for new scoring-source input, rather than a fresh 32,000,000 bytes.

Other finite ceilings include:

| Resource | Limit |
|---|---:|
| Snapshot paths / graph entities / observations | 100,000 / 100,000 / 1,000,000 |
| Admitted graph JSON | 64,000,000 bytes |
| Recognized markers / one marker | 1,024 / 1,000,000 bytes |
| Distinct physical scopes, including unassigned | 256 |
| Raw selectors / one selector / aggregate selector text | 2,048 / 512 / 256,000 UTF-8 bytes |
| Package name / canonical path | 256 / 4,096 UTF-8 bytes |
| Parsed marker depth / expanded nodes per marker | 64 / 20,000 |
| Selector-match work / ownership-map charge | 16,000,000 units / 16,000,000 bytes |
| Returned diagnostics / scope summaries / declaration bindings / source-omission paths | 64 / 64 / 128 / 200 |
| Entire returned scope metadata | 256,000 serialized bytes |

These are checked retrieval-policy ceilings. Earlier capture/index behavior,
inherited snapshot loading and the adapter's existing definition-conversion
limits are separate; this contract does not claim that every allocation or
read in those operations is bounded by the retrieval source allowance.

## Scope-local scores and fusion

Query normalization uses NFD, combining-mark removal, lowercase and distinct
alphanumeric terms. A query has 1..32 terms. Weighted tokens use body weight 1,
identifier-derived name weight 4 and path weight 1.

BM25 is computed separately over admitted documents in each physical scope,
with `k1 = 1.2`, `b = 0.75` and
`idf = ln(1 + (N - df + 0.5) / (df + 0.5))`. Lengths and document frequencies
come from that scope's admitted corpus. Positive lexical mass is normalized
within each scope and divided equally among scopes with positive mass to form
the graph seed.

With `graphWeight > 0`, personalized graph propagation uses admitted documents
and captured resolved observations between them. Each observation contributes
both orientations, parallel observations retain their multiplicity, and a
self-loop contributes two self-arcs. Restart is 0.15, propagation is 0.85,
dangling mass follows the seed, and iteration stops at the first L1 delta below
`1e-9` or after 24 iterations. Zero graph weight skips these iterations. With no
positive lexical seed there are no positive candidates. Seedless scopes can
receive graph-only candidates when graph propagation is used.
Balanced output labels this convention as
`graphSelfLoopConvention: "two_self_arcs_per_captured_self_loop"`.

Each scope normalizes lexical and graph scores by its own positive maximum;
all-zero vectors stay zero. The combined score is
`(1 - graphWeight) * normalizedBm25 + graphWeight * normalizedGraph`.
Within a scope, sort positive candidates by score descending, distinct-query-term
coverage count descending, then entity ID ascending. Apply `perFileLimit` to the
complete local order, then assign consecutive `localRank` values from 1. Fuse
scopes by local rank ascending, coverage descending, score descending, scope
root ascending and entity ID ascending, then apply `limit`.

Raw BM25 values from different scope corpora are not compared directly.
Reduction and arc orders are fixed for repeated checked input. This is a
bidirectional relevance heuristic, not a compiler reference graph or a
retrieval-quality/performance certification.

## Policy fallback and hard errors

The probe distinguishes `Ready`, `NotApplicable` and `UnsupportedPolicy`.
Observed source authority, integrity, resource and deadline failures always
remain errors. Marker counts and sizes are preflighted; all recognized marker
paths are loaded and verified before choosing a format fallback. Syntax,
schema and supported-policy diagnostics use static codes, canonical paths and
hashes, with optional numeric locations; they do not expose parser snippets.

| Probe outcome | Explicit native balanced | Private compatibility auto |
|---|---|---|
| `Ready`, at least two eligible scopes | Balanced | Balanced; `usable_multi_package_query` |
| `Ready`, zero/one eligible scope after filters | Balanced degenerate case | Global; `query_zero_eligible_scopes` or `query_single_eligible_scope`, retaining the partition ID |
| `NotApplicable`, including no/single actual package root | Request error with policy code | Global with `fallback.kind: "not_applicable"` and its policy code |
| `UnsupportedPolicy`, including malformed captured manifest source | Request error with policy code | Global with `fallback.kind: "unsupported_policy"` and its policy code |
| Hard failure | Error | Error; no fallback |

Static policy codes include `source_syntax`, `source_schema`,
`unsupported_selector`, `unsupported_yaml_feature`,
`conflicting_npm_pnpm_membership`, `explicit_package_workspace`,
`workspace_root_excluded`, `unsupported_repository_format` and
`unsupported_marker_role`. Overall unsupported policy follows canonical
diagnostic `(path, code, selector)` order.

Hard error categories are `invalid_request`, `authority`, `source_integrity`,
`denied_source`, `cancelled`, `deadline`, `capacity`, `output_capacity` and
`internal`. Their message format is
`Manifest-balanced retrieval <category>: <static_code>`. Wrong Store, workspace,
snapshot, graph or captured proof, denied paths, missing/mismatched blobs,
invalid ranges, cancellation, deadlines and capacity failures are not malformed
manifest fallback. Unknown failures are not classified by message substrings.
Auto neither catches a hard error to rerun global retrieval nor starts another
deadline, cache or allowance.

## Metadata and output limits

`retrievalScope` carries `schemaVersion: 1`, `policy`, `requestedMode`,
`effectiveMode`, `partitionId`, `selectionReason`, `fallback`,
`actualPackageRootCount`, `queryEligibleScopeCount`, `probe`, `diagnostics` and
omission counters. Unknown package or eligible counts remain null. Policy
fallback does not invent a partition or verify unsupported membership.

Balanced items retain their original entity and source bindings and add
`scopeId`, `localRank`, `queryCoverageCount`, `normalizedBm25Score` and
`normalizedGraphScore` alongside raw `bm25Score`, `graphScore` and
`rankingScore`. `ranking` is `source_body_bm25_manifest_balanced_graph_v1`.
Scope summaries separate admitted/omitted sources and documents, positive
candidates, per-file rejections and final-limit omissions. They describe checked
queues, rather than every source in a repository. Query exclusions are distinct
from budget omissions. The legacy `balancedOut` field is not an exhaustive
coverage counter.

Global auto fallback reports its known aggregate admission counters. It does
not fabricate per-scope scoring counts to fill optional scope summaries.
Checked partition identity and declaration bindings can remain when a Ready
partition falls back because of query filters. `sourceOmissions` can be empty
when detailed rows are not emitted; its `omittedSourceOmissionCount` must then
equal the known aggregate `admission.omittedSourceCount`, not zero when a source
was omitted. More generally, each optional array reports the exact number of
its rows that were not returned.

Admit selected items and mandatory metadata first. Optional arrays use canonical
prefixes in this order: diagnostics, scope summaries, declaration bindings,
source-budget omission paths. Count and byte ceilings can shorten each prefix;
the omitted counts remain exact. The complete ownership map and raw manifest
contents are never returned. Clipping response metadata does not change the
partition identity or mean that omitted sources were checked.

Retrieval data is bounded by the caller's maximum 4,000,000-byte allowance.
Trail/Graft `find_code` reserves the exact logical `{"status":"ok","data":...}`
wrapper overhead before ranking. After the original definition conversion it
counts that full logical envelope again without copying definitions or
buffering another JSON result. Exceeding 4,000,000 serialized bytes returns
`output_capacity: compatibility_envelope` before artifact publication. If
mandatory ranking output cannot fit its supplied ceiling, it fails earlier
with `output_capacity`. There is no immutable-artifact fallback or retry for
these bounded retrieval results, and no promise to rerank to fit after
definition conversion.

Items preserve original UTF-8 path, content hash, entity ID and byte ranges;
normalization for scoring does not rewrite source anchors or bodies. Results
remain `complete: false`: admitted-corpus counts and deterministic ranking are
not exhaustive-source, compiler-reference, runtime or upstream-equivalence
claims. Partitioning and ranking publish no records. Earlier adapter indexing
and snapshot selection retain their existing effects, independently of this
statement. This finite policy does not certify broader ecosystem discovery,
upstream package-manager ordering, performance, or Linux/macOS runtime behavior.
