# Explicit shortest-path source-category selection

The native Graphify adapter accepts `path_policy` on `shortest_path`:

- Omission or `first_observation_v1` retains the existing first-observation
  BFS route and response shape.
- `weakest_source_categories_v1` selects the weakest category among all
  shortest observation routes in the explicitly selected traversal direction.
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
The selected policy then walks the increasing-distance DAG, retaining at most
four weakest-prefix states per node. This avoids exponential path enumeration;
every transition checks the original deadline. Original adjacency order gives
deterministic category ties for an identical immutable View. The result adds
`pathPolicy`, `pathSourceCategory` and `pathCategoryMeaning` only for the
selected policy. Existing ambiguity handling, final 256,000-byte refusal,
source admission and permission rules remain. The catalog's optional argument
is additive; omission does not select this policy automatically.

The selected response currently contains the chosen original observation per
hop, as the legacy response does. The immutable View retains all parallel
observations; `get_neighbors` can still expose them with their exact evidence.
An aggregate presentation of every parallel copy per hop, retained numeric
source-claim semantics, route counting/caps and full current upstream CLI/MCP
equivalence remain outstanding. This selector stage does not certify them.

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

## Parallel-hop provenance: test-first follow-up

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

Production behavior is unchanged from `cf1d0671`; the new presentation is not
implemented. The two new native tests await guarded hosted compilation and an
actual causal RED. Numeric source-claim admission, contradictory claim fixtures,
distinct-node 256/257 counting and full current CLI/MCP equivalence remain open.
