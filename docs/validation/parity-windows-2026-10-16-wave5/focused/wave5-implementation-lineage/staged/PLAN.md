# Proposed next slice: manifest-balanced source retrieval

Status: local inspection and ignored planning only, 2026-10-15. No production
source edits, tests, Cargo, Node, providers, services or upstream browsing were
run. Root selects and releases implementation separately. Current wave 4 work
does not acquire acceptance from this plan.

## Ranked choices

| Rank | Substantial slice | Value and reason for order |
|---|---|---|
| 1, recommended | Manifest-bound monorepo scopes, fair admission and per-scope retrieval fusion | Small relevant packages can survive a much larger package's candidates and byte budget. Entirely local Rust over already captured sources; changes actual query selection, rather than catalog metadata. Avoids the current archive/refactor files. The original `Balanced monorepo scopes` row remains wholly unreconciled, EP003. |
| 2 | Checked comparison of two explicit published structural Views | Answers which selected nodes, parallel relations, source evidence and community memberships changed. R3 currently refuses historical selected analysis. Strong existing View/report/retention foundations, but cross-snapshot identity ambiguity and current archive ownership make it a larger integration seam. |
| 3 | Frozen Git impact details: rename candidates, areas and exact quotations | Makes the existing blast-radius reports substantially more useful during review. R5 still emits modified renames as separate additions/deletions and lacks area/quote details. Can consume verified reports without adding Git commands, network, hooks or providers; similarity and ownership must remain candidates. |

Default-safe delete needs a genuine usage-completeness contract; a syntax-local
absence check cannot supply it. Directory semantic moves and external acquisition
have additional provider/filesystem or endpoint authority contracts. They are not
better independent slices here. Mixed Office/PDF Views remain valuable R4 work,
but require a separate typed source-space bridge and overlap the active archive
area.

## Why choice 1 is still missing

`retrieval.rs:62` validates one snapshot/graph and scores one global pool with
BM25 and personalized graph relevance. Its diversification at lines 306-339 is
per file, not per package. The source and document budgets are each 32 MB;
balancing only the final list would leave small packages omitted before fusion.
`trail_adapter.rs:110` sends `find_code` into that same path.

`query_features.rs:241` already makes `relativePath` segment-aware. Do not redo
that completed behavior. `manifest_graph.rs` already extracts package/dependency
data and Cargo inheritance, but its npm path does not construct workspace
membership or a retrieval partition. A dependency overlay is not the new ranking
contract.

## Recommended executable contract

1. Add private `retrieval_scopes.rs` with a sealed `CheckedScopePartition` and
   `partition(store, snapshot, deadline)`. Read only exact captured manifest
   bytes through `capture::source_scope_policy` and `query::source_in_scope`.
   Bind the full snapshot identity, each recognized manifest path/hash, canonical
   ownership map, omissions and a versioned policy. No Deserialize, caller roots,
   live directory walks, package managers or configuration execution.
2. Support captured package markers plus npm workspace arrays/object-packages,
   pnpm `packages` selectors/negations and Cargo workspace members/excludes.
   Intersect declarations with captured package roots; never acquire missing
   members. Assign each captured file to its deepest admitted package root,
   with an explicit root/unassigned scope. Nested roots win; duplicate package
   names remain distinct by path. Workspace exclusion changes membership, not
   source-read authority. Unsupported or malformed selectors must be diagnosed,
   never silently interpreted as complete membership.
3. Add explicit native `scopeMode: "manifest_balanced_v1"`; absence retains the
   current global algorithm and response bytes. In the Trail/Graft `find_code`
   adapter, select the new mode automatically only when a checked usable
   multi-package partition exists. Preserve its public argument schema and six
   tools. Any fallback must state that balance was unavailable and why. An
   explicit native balanced request fails on unsupported partition policy;
   fallback must not masquerade as successful balanced retrieval.
4. Admit source files and then entity documents fairly across scopes *before*
   the existing aggregate byte limits. Count manifest parsing within the source
   budget. Compute BM25 with per-scope corpus statistics. Preserve captured
   parallel graph arcs; normalize lexical seed mass per eligible scope before
   the existing bounded graph walk. Produce deterministic local orders, then
   interleave rank layers across positive scopes, using query coverage, local
   score and canonical scope/entity IDs as documented tie breaks. Retain the
   per-file cap. This is an owned ranking policy, not a claim of identical Trail
   ordering or universally better relevance.
5. Balanced results retain exact original entity IDs/hash/ranges/body slices and
   add scope IDs/manifest bindings, local rank, admitted/omitted counts and budget
   reasons. Distinguish candidate totals from lower bounds. Keep `complete=false`.
   Queries publish no scope records, graphs, plans or provider artifacts. Old
   snapshots use their captured manifests even if the working tree changes.

Initial hard limits should include the existing graph/source/document/output
limits plus bounded manifest count/aggregate bytes, scope count, pattern count,
pattern bytes and total pattern-match work. Check the shared deadline during
parsing, membership, scoring and fusion. Reuse strict JSON and
`config_graph::bounded_yaml`; separately enforce the supported workspace schema,
duplicate-key policy and canonical relative selector syntax. Reject absolute,
drive/UNC, traversal and host-path selectors. Labels never grant source or write
authority. This slice adds no persisted authority kind.

## Disjoint ownership and release order

| Owner | Files after release | Responsibility |
|---|---|---|
| A, partition | new `retrieval_scopes.rs`, new `tests/monorepo_scopes.rs` | Checked captured manifests, ownership/membership and bounds. |
| B, scoring | `retrieval.rs`, optional private `retrieval_balanced.rs`, new `tests/monorepo_retrieval.rs` | Fair pre-budget admission, local scoring/fusion and original-anchor preservation; freeze legacy global output first. |
| C, entry points | `trail_adapter.rs`, `protocol.rs`, new `tests/monorepo_profiles.rs` | Native option/schema and actual Trail/Graft query integration, budgets and explicit fallback. No new tool aliases/count changes. |
| Root | `lib.rs` if needed, integration schedule and eventual docs/evidence | Tests-only promotion, actual causal REDs, central Cargo, independent review and full unchanged-source acceptance. |

Agree the opaque partition getters before implementing consumers. No concurrent
owners edit `retrieval.rs` or `protocol.rs`. Keep all plan code unpublished until
the normal release. A dedicated independent reviewer should check source scope,
budget fairness and ranking oracles.

## Causal acceptance and adversarial tests

Start with current public JSON calls: freeze a legacy global result, then obtain
an actual missing-mode RED. After the option route exists, require a true ranking
RED where 30 relevant functions in a large package currently crowd out a
single relevant function in a small package. Hand-written expected names must
show one result from each at limit 2; do not compute the oracle with fusion code.
Use graphWeight 0 for that exact ordering oracle and separate real graph-neighbor
cases for the graph policy. A metadata-only scope list cannot pass these tests.

Additional required cases:

- Actual npm array/object, pnpm include/negation and Cargo workspace fixtures;
  nested workspaces, duplicate names and `pkg` versus `pkg-extra` boundaries.
- A large scope exhausting the old source/document budget before a small scope;
  the latter receives admission, or an explicit bounded omission when the
  documented total budget genuinely cannot cover it. No false exhaustive total.
- Dangling members, malformed/duplicate-key manifests, alias-heavy YAML,
  overlarge selector sets, absolute/UNC/traversal selectors and expired deadlines.
- Host source changes after capture, foreign/tampered snapshots/graphs, ignored
  or read-only captured inputs and no implicit reads beyond captured membership.
- Original Unicode/CRLF source slices and hashes; stable ties under input
  reordering; parallel-edge preservation; per-file limits after scope fusion.
- Native absent/global golden output unchanged; both real Trail and Graft
  `find_code` return the balanced small-package result, without new providers or
  hidden publication. Unsupported partition fallback is observable.

Root must run focused RED/GREEN, existing retrieval/Trail/session/protocol/scope
regressions, then the repository's required full Windows matrix and lint/format
gates on one frozen source manifest. This plan supplies no executed test count.

## Alternatives' concrete boundaries

Choice 2: a new `graph_view_diff.rs` takes **both View IDs**, validates them through
`graph_view::load`/`validated_projection`, and preserves each side's snapshot,
base/projection hashes, overlays, evidence precision and trust. Use unique
structural correspondences only; retain ambiguous groups and multiset parallel
edge changes. Never infer a previous View from `previousSnapshotId`, convert a
Meaning/Office claim to UTF-8 code, or change canonical report IDs. A prepared
comparison admits its output before publication; its new retention validator
replays both Views. Tests need historical source drift, duplicate semantic keys,
parallel/self-loop deltas, changed overlay origins, resealed tampering and budget
failure without publication. No implicit provider work.

Choice 3: a new `git_impact_details.rs` consumes `git_workflows::load` output and
its checked base/head source, leaving old reports immutable. Expose exact-content
relocations separately from bounded edited-rename candidates and ambiguous
pairings. Add deterministic manifest/path-based area candidates and quotations
read from exact hunk anchors, including deleted originals. Never relabel an
approximate rename as identity or path overlap as a merge conflict. Tests need
two identical possible sources, an edited rename, deletion/zero-length hunks,
Unicode/CRLF quotes, source tampering and pair/output/deadline budgets.

## Original contracts remain uncertified

The local ledger contains exactly 353 unique original identities, all with
`fullContractCertification=false`. The recommendation targets
`trail:Balanced monorepo scopes` (pointer `/upstreams/trail/capabilities/13`,
pin `12ac479785309350772fcc80115d57a01639fa0555657e3627e69f7d2649072e`;
upstream paths `graph/scopes.ts`, `ask/fuse.ts`, `map.ts`) and the existing
`trail:publicTool:find_code` surface. The local inventory does not specify every
upstream marker/selector precedence or scoring coefficient. Those exact
semantics and retrieval-quality/performance equivalence remain uncertified.

Choice 2 maps to R3 `graphify:clustering`, `report_analytics`, `graph_query`;
choice 3 maps to R5 `trail:Git diff blast radius`, `Owner suggestions / reports`,
`graphify:affected` and `pr_workflow`. Their full upstream behavior, compiler
binding, remote freshness, proprietary behavior, Linux/macOS runtime and any
upstream performance claims remain open. `SOURCES.json` records inspected local
file hashes; active-wave files can change after this planning observation.
