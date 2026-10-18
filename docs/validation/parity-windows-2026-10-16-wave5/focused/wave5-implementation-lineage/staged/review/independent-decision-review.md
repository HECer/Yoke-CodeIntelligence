# Independent plan review: manifest-balanced retrieval

Reviewed `PLAN.md` SHA256 `4afb8f1767025c612e6bf93be32900d4fb82547ec4e0c6b97690a0e0b462acf6` against local source on 2026-10-15. The six retrieval/protocol/Trail/manifest/filter/YAML source hashes still equal `SOURCES.json`. This is preparation only: no product edits, Cargo, Node, tests, providers, or upstream research. Only this ignored note was written.

Recommendation: keep choice 1, but settle the following contract decisions before the A/B/C implementation release. They do not block the current acceptance wave. The source-authority boundary and explicit native opt-in are sound; the remaining problems are concrete ambiguities at the consumer seams.

## 1. Correct the end-to-end publication promise

`trail_adapter.rs:101` calls `engine.index()` before every `find_code`; `protocol.rs:492` uses `engine.select()` for session retrieval. These routes can capture/publish before the new partition/scorer runs. Step 5's “Queries publish no scope records, graphs, plans or provider artifacts” therefore overstates the proposed end-to-end behavior if these entry points remain unchanged.

Replace it with: “Partitioning and balanced ranking publish nothing and read only the explicitly supplied checked snapshot/graph. Entry points retain their existing capture/select behavior, which is reported separately.” If zero-publication *entry points* are intended, release a separate explicit stored-snapshot selection change and test that route; do not silently turn Trail's current capture behavior into a historical-read mode. No package declaration may authorize source capture, ignored-directory inclusion, prospective reads, provider work, or writes.

Keep the opaque partition tied to the checked workspace/snapshot plus versioned ownership policy and captured scope proof. B must reject a partition supplied with a different Store/snapshot, rather than trusting getters merely because the type is sealed. Read-only and admitted ignored inputs remain read-only; absent parent manifests remain absent, even when a working-tree ancestor would supply one.

## 2. Freeze one small selector/ownership specification

“Captured package markers,” “admitted package root,” and “usable multi-package partition” need finite definitions. `manifest_graph.rs` recognizes more ecosystems than the three workspace formats, and its existing extraction is a published derived graph, not a reusable checked partition. Do not call `manifest_graph::extract()` or configuration extraction from retrieval.

Add a table defining marker filenames, virtual workspace roots versus actual packages, missing/invalid names, markers from different ecosystems at the same physical directory, and whether standalone nested package markers count without any workspace declaration. Deduplicate a physical root when `package.json` and `Cargo.toml` coexist; keep names/ecosystems as labels rather than giving the same files multiple shares. Specify where files from an excluded package go: a retained physical package scope or the explicit unassigned scope, never silent removal. State whether that scope participates in balancing.

For selector v1, publish the exact grammar (`*`, `**`, `?`, literal segments, or a smaller subset), path base, slash/case policy, dot/root spelling, and negation evaluation. Reject unsupported braces, character classes, extglobs, leading escapes or re-inclusion forms rather than letting the existing simple glob helper interpret them as literals. `query_features.rs:148–186` currently supports only its own small glob language; it is not a package-manager selector contract.

Specify precedence when npm and pnpm declarations coexist, nested workspace ownership, Cargo root-package membership, `members`/`exclude`, explicit package-to-workspace declarations, and any path-dependency membership rule that v1 deliberately does not implement. Prefer an explicit unsupported-policy result on a relevant conflicting declaration over a guessed union. Bound diagnostics and bind each decision to its captured declaration/hash. Duplicate names must remain distinct by root path. The plan already disclaims upstream equivalence; these are owned v1 semantics, not a reason to reproduce entire package managers.

## 3. Separate absence/unsupported policy from failed source validation

Automatic adapter fallback must not be `match partition { Err(_) => global }`. Define a typed outcome such as usable / not-applicable / unsupported-policy; reserve `Err` for foreign scope, invalid proof/blob, malformed authoritative data, capacity, cancellation and deadline failures. The latter must fail the request. Otherwise corrupt captured manifests or an exhausted shared deadline can be hidden behind a plausible global result.

For allowed fallback, reuse the same deadline and already-spent read/work budget; do not reset a fresh 32 MB allowance or repeat all manifest reads under an independent budget. Native explicit `manifest_balanced_v1` should reject unavailable/unsupported policy as documented. Distinguish “no multi-package partition” from “query filters leave fewer than two eligible scopes.” An unreadable declaration is not proof that there was no declaration.

## 4. Define a deterministic admission schedule before saying “fair”

The current source loop (`retrieval.rs:147–166`) and entity-document loop (`169–205`) spend their budgets in stored order. Replacing only the final fusion cannot repair their exclusions, as the plan correctly observes. However, “fairly across scopes” still permits incompatible implementations.

Specify two explicit schedules: whole-file admission and whole-entity-document admission. A reasonable owned v1 is canonical per-scope queues, one admissible unit per scope per pass, deterministic scope order, and deterministic redistribution of unused capacity. State how an oversized head unit is skipped so it cannot block later admissible units in its scope. Whole-unit admission preserves exact bodies; do not invent truncated documents solely to satisfy a quota. The query path filters must determine eligible source/doc queues before quota calculation; manifest reads required for ownership are accounted separately.

Reserve a bounded manifest sub-budget inside the existing source budget, deduplicate reads reused by scoring, and state which charge counts per file versus distinct CAS blob. Otherwise partition construction can consume all 32 MB before any package body gets a turn. Overlapping nested declaration bodies can likewise consume the document budget repeatedly; preserve that cost accounting rather than treating entity count as source bytes. State the attainable guarantee: each scope gets its turn, and omissions report why a whole unit could not fit; no promise that every relevant file was read when relevance itself requires unread bytes.

Add a hand-calculated admission fixture with unequal file sizes, a head file larger than a fair share, a later small file, exhausted manifest allocation, nested overlapping entities, and the same query narrowed to one package. Assert exact admitted/omitted paths and bytes, not just a small package appearing eventually.

## 5. Pin the fusion formula and graph eligibility

“Interleave rank layers … using query coverage, local score … tie breaks” does not yet define a single order. Write the exact sort tuple, the meaning of coverage (document versus scope, and count versus fraction), normalization denominators, treatment of all-zero scopes, and finite-score checks. Per-scope BM25 raw values are not automatically comparable across scopes with different corpus sizes. If cross-scope order is rank-layer-first, make that priority explicit before coverage/local-score tie breaks.

Define whether a scope with no lexical seed but a graph-reached neighbor can enter fusion, how its local BM25/graph score is normalized, and whether graph propagation crosses package boundaries. Keep the existing bidirectional heuristic label and parallel-arc multiplicity. Preserving parallel arcs is compatible with canonical ordering: sort the traversal inputs including duplicate arcs so floating-point accumulation does not depend on caller/vector iteration order. Do not claim general input-reordering invariance for reordered arrays that change the authoritative stored graph identity; test identical checked inputs or canonically equivalent capture construction.

The ranking RED should run the same frozen fixture in legacy/global and balanced modes, with literal expected names. Make `perFileLimit` large enough or distribute the large package's functions across files so existing per-file diversification cannot supply the desired result accidentally. Require the global control to actually omit the small package at limit 2. Add unequal-coverage and unequal-corpus fixtures, zero lexical mass, a cross-scope graph-only neighbor, duplicate parallel edges, and explicit ties. A single 30-versus-1 example cannot distinguish most incorrect fusion policies.

## 6. Make automatic compatibility behavior observable and contain its drift

Keeping native absence/global byte-identical is good. Add an explicit `"global"` native value if it is mentioned as a supported mode, and freeze absent versus global separately. Pin rejection of null/non-string/unknown modes at both actual protocol execution and schema surfaces; `ci_retrieve` exists in the session tool profile (`protocol.rs:393`), while the ordinary eight-tool native list must remain unchanged.

Automatic Trail/Graft selection is a deliberate default ranking change. It needs an explicit response field for requested/effective mode, policy/partition identity, auto-selection reason and bounded fallback reason; a successful balanced response should be distinguishable from global without examining item names. Preserve the six-tool argument schemas, but update their descriptions/docs to disclose automatic manifest balancing. Preserve the adapter's eight-line/full-definition slice conversion and exact original entity ranges (`trail_adapter.rs:129–205`), rather than bypassing that conversion when balancing is selected. Freeze ordinary single-package/no-marker compatibility output or state the intentional additive metadata drift; “preserve its public argument schema” does not promise response-byte compatibility for this default-changing adapter.

## 7. Put numeric ceilings and output ownership in the release contract

The existing 100,000-entity/1,000,000-observation and two 32 MB limits are useful but do not bound a naive patterns × package-roots × path-length pass or repeated ownership scans. Before implementation, choose numeric manifest count/bytes, scopes, patterns/count/aggregate bytes, total selector-match work, ownership-map bytes, and diagnostic/output caps. For example, start conservatively with at most 1,024 manifest files/4 MB aggregate, 256 physical scopes, 2,048 selectors/256 KB aggregate, and a separate bounded aggregate match-character/work counter; root can adjust these owned limits before release. Use nearest-ancestor indexing for file ownership rather than scanning every root per file. Cap parsed TOML as well as JSON/YAML; `bounded_yaml()` bounds alias expansion but does not by itself establish the new workspace schema/duplicate-key policy.

Describe counts as exact for the checked/admitted corpus, or explicit lower bounds for unread/omitted membership. Keep skipped-by-source-budget, skipped-by-document-budget, excluded-by-query, and per-file-cap rejection distinct. Do not relabel the legacy `balancedOut` early-stop counter as an exhaustive repository count. Return bounded scope summaries and declaration bindings, not the complete ownership map or unbounded per-path diagnostics. Charge these bytes against the actual caller's output budget before constructing a huge response; no artifact fallback or publication should be added by the new scorer.

## Suggested release checkpoint

First freeze the above policy table, numeric bounds, typed partition/fallback outcomes, exact admission schedule and fusion tuple. Then A/B/C can write disjoint tests against those shared decisions. Keep root's causal RED/GREEN and final one-manifest acceptance requirements. No new universal correctness, performance or upstream-equivalence certification follows from this plan review.
