# Explicit shortest-path source-category selection

The native Graphify adapter accepts `path_policy` on `shortest_path`:

- Omission or `first_observation_v1` retains the existing first-observation
  BFS route and response shape.
- `weakest_source_categories_v1` selects the weakest category among the first
  256 distinct shortest node paths in the explicitly selected direction.
  Stored directions and original observation/evidence IDs remain unchanged.
  `undirected: true` permits reverse traversal without reversing stored claims.

The latter uses the existing declared `SourceCategoryPolicy` default mapping:
`syntactic` is `extracted`; `heuristic` and `inferred_concept` are `inferred`;
`ambiguous` is `ambiguous`; everything else is `unmapped`. From weakest to
strongest the order is unmapped, ambiguous, inferred, extracted. The weakest
hop determines a route's category. These are report-policy categories, not
probabilities, compiler binding or execution proof. No score is invented for a
missing claim. A zero-hop self path has no category.

BFS still determines the minimum hop count under the existing 1..64 bound.
The selected policy builds an increasing-distance predecessor DAG in BFS
first-discovery order. Each predecessor node appears once for counting, while
all original parallel claims remain available for hop provenance and the
weakest original claim determines that hop's category. Equal-rank copies keep
their original View order. Backward enumeration from the target ranks the first
256 complete node paths; discovering a 257th path only sets the cap flag. This
is a complete-path prefix, not a count of first-hop neighbors or observations.
Category ties retain the first enumerated complete path.

The implementation keeps a depth-bounded stack and only the chosen route, not
an exponentially large route set. The admitted View's 20,000-node/100,000-edge
bounds still apply. Each DAG transition, stack step and ranked-hop scan checks
the original absolute deadline. The result adds `pathPolicy`,
`pathSourceCategory` and `pathCategoryMeaning` only for the selected policy. Existing ambiguity handling, final 256,000-byte refusal,
source admission and permission rules remain. The catalog's optional argument
is additive; omission does not select this policy automatically.

The selected response adds `pathHops` alongside the chosen original
`pathObservationIds` and top-level observations. Each hop has traversal endpoint
IDs, its selected observation ID, every traversable original observation and ID,
and their original evidence in View order. Evidence is deduplicated by its
original ID within a hop; original observations are never collapsed. Explicitly
undirected traversal includes both stored directions without rewriting either.
Resolved self/unreachable paths have no hops; ambiguous endpoint responses keep
their existing candidate shape. Omitted and explicit legacy policy responses
are unchanged.

The presentation borrows original objects, indexes evidence once and checks the
deadline throughout collection. The existing bounded JSON encoder counts and
serializes the borrowed result before owned JSON copies are created, under the
same 256,000-byte limit and absolute deadline. The final whole-response byte
refusal remains. A complete provenance result that cannot fit is refused; it is
not silently clipped. At published source `9b429f4d`, all seven policy/rendering
units and eleven public selected-View cases pass on Linux and Windows. The
independent source/text receipts are
`73a518251332532b416c7e8064e4bf393b9f31285914de09431ef92cddca4fcd`
and `eb182a06f6fad09b2fbfe8623a643baf9d35769a780fd45a45af25f47a5b0c00`;
the latter preserves a separate unchanged-core deadline failure and is not
whole Windows/default-suite acceptance. Retained numeric source-claim semantics,
contradictory admitted claim fixtures, the route counting/prefix stage below
and full current upstream CLI/MCP equivalence remain outstanding.

## Qualification

The original causal test was published before implementation at `b929b7b3`.
[Windows job 124536208140](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/41200857455/job/124536208140)
reaches its route-identity assertion and returns the deliberately first stronger
route rather than the expected weaker route: 857 library tests pass, that one
fails, seven original cases are ignored, and none are filtered. The independently
audited [text artifact 13042784204](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/41200857455/artifacts/13042784204)
has ZIP SHA256 `2e65acb95c08a7baa7ae3ab417e574df44d04b0ee254e2cf5417c7a88cdea826`.
All 97 text members/log hashes, the exact whole-target partition and all 1,175
selected source hashes are bound to Git. Causal audit receipt SHA256:
`501d657353ac2ffa199cadb298397630ddf69a26afe9c89bea2e3136c62a565d`.

At corrected source `cf1d0671`,
[Ubuntu shard 4](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/41204955065/job/124548044700)
passes all six selector units, including the unchanged causal test, within the
whole library target (564 passed, six original ignored, zero filtered). The
complete public adapter target passes eight cases, including explicit/omitted
policy behavior and invalid-policy refusal, without ignores or filters.
Independent receipt SHA256
`a514b525c3499182df4cd53f198ec9c3738389f32526250001b9ef2316feb262`
checks all 1,176 selected raw Git source hashes, reconstructs all 378 default
targets without Cargo, and checks exact partitions and every text-log digest for
that shard and two separately failed macOS shards. This is scoped native
selector evidence. Windows selector, full platform, upstream, main and release
acceptance remain open. No native payload has been read or executed locally.

The first implementation at `23a9d8a7` fails compilation with `E0432` in
[Ubuntu shard 0](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/41203888247/job/124544946118):
`graph_analysis_policy.rs` is the private `graph_extensions::policy` module,
not a crate-root module. The import now uses its existing public re-exports,
`graph_extensions::AnalysisCategory` and `SourceCategoryPolicy`. This import
correction subsequently passes the scoped Ubuntu cases above and separate fresh
Windows paired compilation with exactly one credential-backed TLS case
(49 filtered). The failed producer supplies no runtime acceptance; the TLS
consumer supplies no selector-case execution.

## Parallel-hop provenance candidate

The authored public View fixtures now specify `pathHops` for the explicitly
selected policy. Each hop must identify its traversal endpoints and the chosen
original observation, retain every traversable original parallel observation
and its ID, and return their exact original evidence. Stored edge directions
must remain unchanged. Directed queries exclude opposite claims; explicitly
undirected queries include both stored directions. Omitted and explicit legacy
selection must retain their existing shape.

The fixture has two original `α -> beta` calls, one `beta -> α` call and one
`beta -> gamma` call in a 71-byte UTF-8/CRLF source. Expected source SHA256 and
byte ranges are independently pinned. Historical selected-View queries must
return identical provenance after a new index and an unsaved host edit, without
capturing or publishing state. These are genuine Session/archived-View paths,
not private forged View metadata.

At test-first source `3ddf641c`,
[Ubuntu shard 3](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/41209015323/job/124559889738)
runs the complete ten-case target: the eight original cases pass and both new
cases fail at their intended first missing-hop assertions, with no ignored or
filtered tests. They report `None` rather than `Some(2)` hops at line 455 and
`None` rather than `Some(1)` directed observations at line 560. Independent
receipt SHA256
`7d75e966ac1ae0f00b02e43aacc51342c3d114a5ebae2d144de20a6f0d0378fa`
checks all 95 text members/raw log digests, 1,176 raw Git source hashes,
the independently derived 378-target inventory and exact 47-group partition.
These RED cases do not reach the later provenance/replay assertions.

The candidate adds the presentation described above and authors a further
public fixture with 1,200 parallel source calls. Legacy selection still returns
one original observation; complete selected hop provenance must refuse its byte
capacity without clipping, publishing state or changing archived/host sources.
Those parallel-hop-only cases pass at `9b429f4d`, as recorded above. The
new route-prefix producer below awaits separate fresh native qualification. Public assertions compare each complete evidence
object and its View order. A separate explicitly synthetic rendering unit covers
multiple-site references, opposite lexical ID ordering and one shared site
across observations/hops; it does not establish public source admission.
No new native case has run locally. Numeric
source-claim admission, contradictory claim fixtures, distinct-node 256/257
counting and full current CLI/MCP equivalence remain open.


## Distinct-node route prefix candidate

The selected-only fields are `shortestNodePathsConsidered` (0..256),
`shortestNodePathsCapped`, `shortestNodePathLimit` (256), and
`pathSelectionScope` (`first_256_distinct_shortest_node_paths`). A 257th path
detects the cap and cannot displace the weakest in-prefix route. Parallel
observations remain separate original claims and do not add node paths.
The count is one for a resolved zero-hop self path, zero for an unreachable or
max-hop-excluded target, and at most 256 for a found nonempty path. Cycles do
not appear in the increasing-distance DAG. Omitted and explicit legacy
selection retain their old route and response shape; ambiguous endpoint
responses retain their existing candidate shape.

Predecessor lists use BFS first-discovery order. Backward enumeration exhausts
the predecessor choices for one target predecessor before moving to the next.
For a fixed immutable View this is deterministic. It differs from forward DFS
on reconverging graphs; a multilevel fixture has 257 complete paths but only
two first-hop neighbors and deliberately puts its weaker last suffix outside
the ranked prefix. The original max-hop/deadline and complete parallel-hop
output-refusal bounds remain.

At test-first source `2037686c`, genuine public selected-View fixtures with
exactly 256 or 257 source branches and two original entry calls per branch
reach their intended missing-count assertions on Linux and Windows. Every
branch, both entry observations/evidence IDs, the sole resolved branch exit,
and absence of shorter/additional resolved routes are checked before that
assertion. All eleven older public cases pass; the two new cases fail at line
792 with `None` versus `Some(256)`, without ignores or filters. Independent
receipt `91ccc39c1554b13bf0cc6e61707a08fe2a4b12f242a002eac5374cbf7676468c`
checks each platform's 1,176 Git-bound source hashes, independently derives its
378 targets and checks exact partitions and every text/log digest for 97 whole
target executions. Later new cap/provenance/replay assertions are not reached
by those RED runs.

At that same source, the complete Linux library has 565 passes, three intended
route-identity failures, six original ignores and no filters. All seven older
policy units pass. The three new explicitly synthetic cases choose the
excluded 257th route rather than the required in-prefix route 0 or 255.
Receipt `2f3c7ff1105778992b6589c90ead7540987c8261b97637c26fc7d58f033b09f4`
checks all 48 whole groups/logs with the same source/inventory binding and
binds the public RED receipt. Synthetic category fixtures do not establish
numeric source-claim admission.

The candidate now implements this bounded predecessor-prefix selection and
adds two authored regressions for self/unreachable/max-hop/cyclic/reverse
paths and expired/pre-selection cancelled requests. The cancellation case
changes its inherited flag after constructing the child deadline and asserts
the inner operation's refusal before the outer owner's final cancellation
check. It does not qualify interruption during prefix enumeration.
All original ten policy units and
thirteen public case bodies remain unchanged. Fresh native GREEN for the
changed producer and these additional cases is pending. Source-bound numeric
claim admission/replay, contradictory admitted numeric/category claims and
full current CLI/MCP qualification remain required and open.
