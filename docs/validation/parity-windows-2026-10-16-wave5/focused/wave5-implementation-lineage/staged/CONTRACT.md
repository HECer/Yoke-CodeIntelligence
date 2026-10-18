# Manifest-balanced retrieval v1: release contract

Status: ignored design only; no implementation or tests are released. This
supersedes the ambiguous parts of PLAN.md `4afb8f17` after the complete review
`ccdde0691a1023945aea456641b70ecd100494a1ef4a9d5c33a0f12f7c972051`.
Root's subsequent decision explicitly permits automatic fallback for malformed
*captured package-manifest source*. It does not permit fallback for failed
source authority, integrity, resource or deadline checks.

## 1. Entry points and effects

- Native session `ci_retrieve` accepts absent `scopeMode`, `"global"`, or
  `"manifest_balanced_v1"`. Absent/global run the existing global path without a
  partition probe and produce identical bytes for the same other arguments and
  checked inputs. Null, non-string, `"auto"` and unknown modes are request errors,
  validated before native select/index/probe. The ordinary eight-tool profile
  and session tool count are unchanged.
- Trail/Graft `find_code` keeps its six-tool public schemas. It internally asks
  for compatibility auto selection. This intentionally adds `retrievalScope`
  metadata even for ordinary no-marker/single-package results. Its eight-line
  and full-definition conversion remains unchanged; do not bypass that code.
- Partitioning and scoring publish nothing and use only their supplied checked
  snapshot/graph. Trail's existing `index()` and session `select()` retain their
  existing capture/publication behavior, separately from this guarantee.
- No marker, selector, name or query option grants capture expansion, ignored
  directory inclusion, a host-root change, prospective reads, provider work or
  writes. Read-only/admitted-ignored inputs keep their existing authority.

## 2. Finite source and workspace policy

All paths below are captured canonical slash paths. Comparisons and ordering use
exact UTF-8 bytes; there is no case folding, NFC conversion or alias inference.
Never consult a live ancestor, directory, package manager or dependency resolver.

| Captured marker | v1 role and relevant fields |
|---|---|
| `package.json` | Strict JSON object. An actual package root unless its `name` is absent and a supported workspace declaration exists at this same root; that case is virtual. Optional `name` is an untrusted label; absent uses the path label. Present empty/non-string/control-containing names are unsupported source schema. General unrelated fields are data. |
| `package.json.workspaces` | Array of strings, or object with exactly `packages: [strings]`. Other object keys/forms are unsupported. Positive selectors only. A root with this declaration is a workspace boundary. |
| `pnpm-workspace.yaml` | One YAML mapping with exactly a string-list `packages` field. Other root fields, tags, anchors, aliases and merge keys are unsupported policy. Duplicate keys and syntax errors are unsupported source schema. Do not expand aliases to interpret v1. The root is a virtual boundary unless an actual package marker also exists there. |
| `Cargo.toml` | Strict TOML. A `[package]` table defines an actual package root, with optional string `name` under the same label rules. `[workspace]` defines a boundary; `members` and `exclude` are optional string arrays. Other workspace fields, including `default-members`, are retained as data and do not alter v1 ownership. Neither table present is unsupported marker role. |
| Cargo `package.workspace` | Explicit package-to-workspace pointers are unsupported in v1, including `"."`; do not guess their resolution. Path dependencies do not add membership. |

`pnpm-workspace.yml`, `lerna.json`, `pyproject.toml`, `go.mod`, `pom.xml` and
`*.csproj` are recognized only as other-format hints. If there are no supported
actual package roots and one of these hints exists, return UnsupportedPolicy
`unsupported_repository_format`; do not parse those files as workspace evidence.
In a supported mixed repository these hints remain ordinary captured files;
the descriptor explicitly says scope coverage is npm/Cargo markers only. A `.yml`
pnpm hint alongside supported roots is unsupported policy, since it conflicts
with an otherwise relevant workspace declaration. This is an owned finite policy,
not full discovery for every package-manager format.

Supported selectors are `.` alone, or slash-separated segments. A segment is a
literal portable component or exactly `*`; exactly `**` is allowed only as the
last segment. `*` matches one nonempty component. Final `**` matches zero or more
components. A literal is exact text, including Unicode or internal spaces, with
no controls, reserved Windows punctuation, leading/trailing component spaces or
trailing dots. Empty components, `.`/`..` components, absolute/drive/UNC paths,
backslashes, `~`, inline stars, `?`, brackets, braces, parentheses/extglobs and
escapes are unsupported selector syntax. `.` names the declaring workspace root.
Selectors match package-root paths relative to the declaring root, never files
or prospective directories. Missing members produce bounded diagnostics.

Only pnpm allows one leading `!`: union all positive matches, then subtract the
union of negative matches, independently of list order. `!!` and re-inclusion
syntax are unsupported. npm and Cargo member arrays do not allow `!`; Cargo
`exclude` is a separate subtracting list. Duplicate selectors are canonically
deduplicated for matching but every raw entry is charged to input ceilings.
A later ordinary positive selector remains union input, not an order-sensitive
re-inclusion directive; the literal negative-final fixture freezes this rule.

If npm and pnpm declarations coexist at a root, accept only identical normalized
positive-selector sets with no pnpm negatives; pnpm is the reported primary and
both bindings are retained. Otherwise return `conflicting_npm_pnpm_membership`.
Cargo and npm declarations at one directory remain separate ecosystem membership
labels; they cannot give the physical directory multiple ranking shares.

For ecosystem membership, use the nearest same-ecosystem workspace ancestor,
including an equal root. Do not cross a nested workspace boundary to try an
outer match. An actual root package automatically belongs to its own workspace;
excluding `.` while such a package exists is unsupported conflict. A nonmember
or excluded captured package stays an independently balanced package scope,
with its membership reason; it is never removed from retrieval. Standalone and
nested package markers count without a workspace declaration. Duplicate names
at different roots stay distinct.

Physical ownership is separate: take the deepest boundary among actual package
roots and virtual workspace roots; an actual package wins at an equal root.
Files otherwise belong to the root `.` unassigned scope. A virtual boundary owns
its otherwise-unassigned files instead of assigning them across that boundary
to an outer package. Deduplicate every physical root across ecosystems, including
`.`. Package, workspace-unassigned and root-unassigned scopes all participate in
ranking when query-eligible. The constructor returns NotApplicable when fewer
than two actual physical package roots exist; unassigned scopes do not turn a
single package into a monorepo. A usable multi-package partition can still have
zero/one eligible scope after query filters.

## 3. Failure taxonomy and shared resources

The probe returns Ready, NotApplicable or UnsupportedPolicy; hard errors remain
`Err`. Every observed source authority/integrity error wins over format fallback.
Validate the Store/workspace/full snapshot/full graph and captured scope proof
before interpreting marker bytes. Preflight all marker counts/raw sizes; load
and verify all recognized marker paths/hashes once before returning a format
fallback. Unknown parser/I/O failures must not be classified by error-message
substrings or caught as generic unavailability.

| Outcome | Examples | Native explicit balanced | Compatibility auto |
|---|---|---|---|
| NotApplicable | `no_supported_package_roots`, `single_actual_package_root` | Structured policy error | Observable global fallback |
| UnsupportedPolicy | Manifest source JSON/YAML/TOML syntax or duplicate keys; wrong workspace/name field shapes; unsupported selectors/format/features; conflicting declarations | Structured policy error | Observable global fallback |
| Hard error | Wrong Store instance/workspace/snapshot/graph/proof/path; blob/hash/size mismatch; denied read; invalid request; cancellation/deadline; any resource/output ceiling; internal invariant/unknown decoder failure | Fail | Fail, never fallback |

Manifest syntax/schema failures are bounded static codes plus canonical path and
content hash, optional numeric line/column; never raw parser/source snippets.
Interpret every safely loaded recognized marker before choosing policy fallback,
so an unsupported first declaration cannot hide a later observed hard error.
Capacity/deadline checks are typed and never relabeled source syntax. Format
fallback does not mean that unread/unsupported workspace membership was verified.
Select the overall UnsupportedPolicy code from the first diagnostic in canonical
`(path,code,selector)` order, not whichever parser finishes first. Return static
`source_syntax`, `source_schema`, `unsupported_selector`,
`unsupported_yaml_feature`, `conflicting_npm_pnpm_membership`,
`explicit_package_workspace`, `workspace_root_excluded`,
`unsupported_repository_format` or `unsupported_marker_role` codes as applicable.
Unmatched supported selectors are nonfatal `missing_member` diagnostics.

Each auto request owns one checked run, one deadline reference, one source cache
and one debit ledger through probe and any global fallback. No restart or second
32 MB allowance. Cached paths are charged once per canonical path, even when
different paths share a CAS blob. Manifest bytes reused for scoring are not
charged twice. Native absent/global does not create this probe/run branch.

| Ceiling | Exact v1 limit |
|---|---:|
| Snapshot paths / graph entities / observations | 100,000 / 100,000 / 1,000,000 |
| Stream-counted graph JSON | 64,000,000 bytes |
| Source input / overlapping document bodies | 32,000,000 / 32,000,000 bytes |
| Recognized marker files / one marker / aggregate markers | 1,024 / 1,000,000 / 4,000,000 bytes |
| Distinct scopes, including unassigned boundaries | 256 |
| Raw selectors / each / aggregate UTF-8 bytes | 2,048 / 512 / 256,000 |
| Package name / canonical path | 256 / 4,096 UTF-8 bytes |
| Parsed container depth / expanded nodes per marker | 64 / 20,000 |
| Selector-match work | 16,000,000 units |
| Ownership-map charge | 16,000,000 bytes |
| Scope summaries / declaration bindings / diagnostics returned | 64 / 128 / 64 |
| Returned source-budget omission paths | 200 |
| Entire scope metadata / entire result JSON | 256,000 / at most caller's 4,000,000-byte ceiling |

Do not pass unbounded structures through a general decoder before enforcing
depth/work limits. Quote/comment-aware preflight and typed bounded decoding must
distinguish source syntax from capacity. The existing YAML expansion guard alone
does not supply v1 schema or duplicate-key semantics. Other unchanged query,
per-file source, body-snippet, result-count and timeout limits still apply.

Charge a selector comparison *before* evaluating it by
`1 + selector_utf8_bytes + relative_package_root_utf8_bytes`; deduplicated
selectors are compared in canonical order. Ownership uses a nearest-ancestor
index, not roots-times-files scans; charge each mapping by
`path_utf8_bytes + owner_root_utf8_bytes + 32`. Markers required for ownership are
read regardless of query filters. Stop at any ceiling as a hard capacity error.

## 4. Exact whole-unit admission

The manifest reservation is inside source input, not an extra allowance. It
caps probes at 4,000,000 bytes and guarantees at least 28,000,000 bytes remain
after a successful maximum-size probe. Release unused reserve after probing:
body input available is exactly `32,000,000 - charged_marker_bytes`.

Apply existing query filters before building eligible queues and counting active
scopes. Sort scopes by canonical root; sort file queues by canonical path. Each
file unit costs its exact UTF-8 bytes, except a verified cached marker costs zero
incremental source bytes. Never admit partial source files. Empty units are
finite entries and cost zero. Use the following identical schedule twice:

1. Divide the available budget by the number of nonempty eligible scope queues.
   Each gets floor(B/N); give remainder bytes, one each, to the first B%N roots.
2. Phase 1 visits scopes in root order, one fitting whole unit per scope per
   pass. Scan past oversized heads into a deferred queue; they cannot block a
   later small unit. Debit only that scope's protected allowance. Repeat until
   a complete pass admits nothing. Each unit is scanned at most once per phase.
3. Phase 2 pools all unused allowance, merges remaining units back into each
   scope's canonical order and repeats one fitting unit per scope per pass.
   Units too large for the now-monotonically-decreasing pool are finally omitted
   with the appropriate budget reason. Stop after all queues are exhausted.

Document admission runs after source admission with its separate 32,000,000-byte
budget. Its scopes are those with eligible documents from admitted sources;
queues sort by `(path,startByte,endByte,entityId)`. A unit is the entire original
entity body length, including overlap with enclosing declarations. Keep the
existing rule excluding a file entity when that file has nonfile entities.
Invalid ranges/hashes are hard errors, not omitted units. No truncated scoring
documents or duplicated ownership shares. Body display clipping is later and
does not change scoring/admission.

Guarantee an equal protected opportunity, not that every relevant file was read:
unread relevance cannot be known and a whole unit may exceed every protected
share. Report exact admitted/omitted counts for these checked queues, separating
query exclusions, source-budget omissions and document-budget omissions.

## 5. Exact scoring and fusion

Reuse the existing query normalization and weighted tokenization: NFD, combining
mark removal, lowercase, distinct alphanumeric query terms; body weight 1,
`identifier_terms(name)` weight 4 and path weight 1. Query has 1..32 terms.

For each admitted document i in physical scope s, let L_i be weighted token
length, tf_i,t weighted term count, N_s admitted document count, avg_s average L,
and df_s,t documents with a positive term count. Sum in canonical term order:

`bm25_i = sum_t ln(1+(N_s-df_s,t+0.5)/(df_s,t+0.5)) * tf_i,t*2.2 /
 (tf_i,t + 1.2*(0.25+0.75*L_i/max(avg_s,1)))`, ignoring zero tf terms.

Use sequential f64 operations in the expression order shown, not unordered or
parallel reductions. Integer weighted lengths sum exactly before average
conversion; scope lexical mass sums in ascending entity-ID order. Independent
per-document computation may be parallel only if these reduction orders remain
fixed. Do not replace the legacy global arithmetic schedule.

Let M_s=sum bm25 in s and K=number of scopes with M_s>0.
Seed_i is `bm25_i/M_s/K` when M_s>0, otherwise zero. If K=0, skip graph
propagation and return no positive candidates. GraphWeight=0 also skips graph
iterations and uses zero raw graph scores.

For graphWeight>0, use all admitted documents, including seedless scopes. Every
captured resolved observation whose endpoints are admitted contributes both
orientations; retain parallel observations. A self-loop contributes two self
arcs. Source graph arrays/identities remain unchanged. Sort vertices by entity ID
and arcs by `(sourceId,targetId,observationId,orientation)` before accumulation.
At each of at most 24 iterations: restart is 0.15*seed; distribute 0.85*rank over
outgoing arc multiplicity; redistribute dangling mass using seed. Compute the
L1 delta in vertex order and stop after the first iteration with delta<1e-9.
Label this bidirectional heuristic and self-loop convention explicitly.
Initialize next[i]=seed[i]*0.15, visit sources in entity-ID order and their sorted
arcs, adding `(rank[source]*0.85)/out_degree` per arc. Accumulate dangling mass
in source order, then add `dangling*seed[i]` in vertex order before the delta.

Within each scope, normalize bm25 and final raw graph rank by their own positive
maximum (all-zero vectors normalize to zero). Score is
`(1-graphWeight)*normalizedBm25 + graphWeight*normalizedGraph`.
Coverage is the document's **count** of distinct query terms with tf>0. Graph-only
documents/scopes enter if their score>0. No NaN/infinity is allowed.

Sort each scope's positive candidates by `(score descending, coverage descending,
entityId ascending)`. Apply perFileLimit over the complete local order; count
all rejections, then assign consecutive local ranks starting at 1. Final fusion
sorts `(localRank ascending, coverage descending, score descending, scopeRoot
ascending, entityId ascending)`, then takes limit. Rank-layer priority is first;
raw BM25 values from different corpora are never directly compared. All counts
refer to the admitted corpus. Do not reuse legacy `balancedOut` as an exhaustive
counter. Report positive candidates, per-file rejected candidates and final-limit
omissions separately. Preserve original entity/hash/range/body bindings.

Internal sorting makes repeated execution of identical checked inputs stable;
it does not authorize a caller to reorder an immutable graph and claim its old
identity. GraphWeight, relation direction and ranking remain heuristics, not
compiler binding or quality/performance certification.

## 6. Observable auto behavior and bounded output

Auto selects balanced only for Ready plus at least two query-eligible scopes.
Ready plus zero/one eligible scopes uses global with distinct
`query_zero_eligible_scopes` / `query_single_eligible_scope` reasons, retaining its
partition ID. Explicit balanced still works as a zero/one-scope degenerate case.
NotApplicable/UnsupportedPolicy use their own reason codes, never those filter
reasons. All fallback uses the already charged run and original deadline.

`retrievalScope` contains requested/effective mode, policy version, partition ID
or null, selection/fallback code, actual package/eligible-scope counts when known,
probe bytes/work and bounded diagnostics. Balanced mode additionally supplies
bounded scope summaries/declaration path/hash bindings, local ranks and exact
admission/count semantics. Explicitly report omitted summary/diagnostic counts;
never return the full ownership map or raw manifest contents. Truncating metadata
does not imply missing sources were checked. If mandatory metadata plus items
cannot fit the actual caller budget, fail OutputCapacity without artifact
fallback or a fresh attempt. Partition/scoring writes remain zero even on error;
earlier adapter index/select effects retain their original contract.

The exact mandatory metadata keys are:

```json
{
  "schemaVersion": 1,
  "requestedMode": "auto or manifest_balanced_v1",
  "effectiveMode": "global or manifest_balanced_v1",
  "policy": "yoke-retrieval-scopes-1",
  "partitionId": "checked ID or null",
  "selectionReason": "static code",
  "fallback": "null or {kind:not_applicable|unsupported_policy|query_filter,code}",
  "actualPackageRootCount": "integer or null if not established",
  "queryEligibleScopeCount": "integer or null if not established",
  "probe": {"markerFilesChecked": 0,"markerBytesCharged": 0,"selectorWork": 0},
  "diagnostics": [],
  "omittedDiagnosticCount": 0
}
```

The strings above describing unions are schema notation, not literal response
values. Ready auto selection uses `usable_multi_package_query`; explicit balanced
uses `explicit_balanced_request`. Filter fallback uses the two codes already
specified. Other policy fallback uses its typed policy code. Never synthesize
package/eligible counts after unsupported interpretation. Balanced `ranking` is
`source_body_bm25_manifest_balanced_graph_v1`; raw/normalized lexical and graph
scores, `localRank`, `queryCoverageCount` and `scopeId` accompany each original
entity. Its admission section distinguishes `markerBytesCharged`,
`bodyInputBytesCharged`, `inputBytesCharged`, `admittedSourceBytes`,
`documentBytesCharged`, source/document omissions, query exclusions and counts
scoped to the admitted corpus. Return scope summaries in canonical root order
and bindings by path, taking the largest bounded prefixes under both count and
metadata-byte ceilings and reporting exact omitted counts. Item scope IDs remain
present even when that scope's optional summary is omitted. No full map is sent.
Admit the exact mandatory metadata and selected item rows first; if they cannot
fit, fail. Optional arrays then use the smaller of 256,000 bytes and the actual
remaining response budget, in this order: diagnostics, scope summaries,
declaration bindings, source-omission paths. Each array takes its canonical
prefix and has an exact omitted count. Reserve those count fields before
appending rows; do not construct an unbounded intermediate response. Document
omissions need exact counts, not an unbounded per-entity list.

Compatibility fallback keeps the old global formulas and stored-order source/
document schedule, but its source allowance is reduced by already charged probe
paths outside the query. Reuse verified marker text when it is selected. Thus
metadata is intentionally additive and near-limit fallback may omit more input;
the spent resource accounting is observable. Native absent/global is unaffected.

## 7. Release and claims

API.md fixes the opaque A/B/C boundary. FIXTURES.json contains literal scheduler,
ranking/control, ownership, fallback and authority oracles. They are proposed
fixtures, not executed evidence. Root promotes tests first, observes causal RED,
releases disjoint implementation, obtains independent review and runs the full
Windows acceptance on a frozen manifest. Keep original 353 inventory identities
and false full-contract certifications. Exact upstream marker precedence,
ordering, performance and retrieval quality, broader ecosystems, Linux/macOS
runtime and proprietary behavior remain uncertified. No unresolved contract
conflict is deferred to an implementer in the supported v1 subset.
