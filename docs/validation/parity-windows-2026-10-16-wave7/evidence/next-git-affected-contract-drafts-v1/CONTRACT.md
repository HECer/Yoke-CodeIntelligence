# R5-B owned affected-details contract draft

Status: design and uncompiled tests only. No product implementation, public API,
test execution, causal RED, accepted fixture, or upstream equivalence is claimed.
The root owns later qualification, implementation, promotion and execution.

This draft refines PLAN.md `efffae73664286c64dfbc84ebb507e2c0dae9afe87fa36b08476161b9048fb65`,
CONTRACT-SUPPLEMENT.md and TEST-CASE-DESIGN.md at their full hashes in
INPUT-PINS.json. The supplement's 5,000/50,000/100,000 per-side graph limits
replace the earlier plan's larger graph figures. All policies below are owned
`captured_hierarchy_v1` behavior, not a Graphify or Trail schema/ordering claim.

## Rust surface

Add a public nested module under `git_workflows`, requiring no `lib.rs` change:

```rust
git_workflows::affected_details::derive(
    store: &Store,
    report_id: &str,
    options: &AffectedOptions,
    deadline: &Deadline,
) -> anyhow::Result<AffectedDetails>

AffectedDetails::value(&self) -> &serde_json::Value
AffectedDetails::canonical_bytes(&self) -> &[u8]
```

`AffectedDetails` has private fields and implements `Clone`, `Debug` and
`Serialize`, with serialization delegating to its complete checked JSON value.
It has no public constructor, `Default`, `Deserialize`, public mutable access,
reseal function, or conversion from caller JSON/graphs. This document's examples
are output shapes, never authority accepted by `derive`.

`AffectedOptions` has public Rust fields named by the snake_case column below,
`Clone + Debug + Serialize + Deserialize`, camelCase JSON names, and
`deny_unknown_fields`. Do not attach serde defaults: all 13 JSON fields are
required exactly once. The Rust `Default` supplies all maxima. Direct Rust
struct construction receives the same semantic preflight as deserialized input.

| Rust field | JSON field | Type | Default and maximum |
| --- | --- | --- | --- |
| policy | policy | String | exactly `captured_hierarchy_v1` |
| max_graph_entities | maxGraphEntities | usize | 5,000 per side |
| max_graph_observations | maxGraphObservations | usize | 50,000 per side |
| max_graph_evidence | maxGraphEvidence | usize | 100,000 per side |
| max_original_seeds | maxOriginalSeeds | usize | 2,000 original rows across sides |
| max_expanded_seeds | maxExpandedSeeds | usize | 4,000 unique new side/entity pairs |
| max_hierarchy_depth | maxHierarchyDepth | usize | 16 |
| max_incoming_depth | maxIncomingDepth | usize | 8 |
| max_states | maxStates | usize | 20,000 |
| max_edge_visits | maxEdgeVisits | usize | 200,000 |
| max_rows | maxRows | usize | 20,000 recursive array entries |
| max_source_bytes | maxSourceBytes | usize | 64,000,000 new-validator distinct-hash bytes |
| max_output_bytes | maxOutputBytes | usize | 4,000,000 canonical bytes |

Reject an unknown policy, unknown/missing/duplicate field, wrong type, negative,
fractional or overflowing integer, or a direct Rust value above its maximum.
These are `git_affected_details_input_invalid` before report metadata work.
Zero remains zero. A zero depth selects a finite smaller policy; another zero
refuses with `git_affected_details_capacity_exceeded` only if positive credit is
needed. For example no-change input can have zero states, original seeds,
expanded seeds and new source bytes, while its nonempty graph-index scan still
requires edge credit. The complete output and fixed limitation array require
positive output/row credit. Host options file admission is a later CLI boundary:
at most 64,000 raw bytes before decode, without normalizing caller options.

Public failure classes are static messages, mirroring R5-A's anyhow boundary:
`git_affected_details_input_invalid`, `git_affected_details_capacity_exceeded`,
`git_affected_details_deadline_exceeded`. Shared cancellation/expiration maps to
the last class. Capacity, input denial or cancellation returns no checked result,
partial value, artifact fallback or publication. No public `AffectedError` enum
is proposed by this draft.

## Admission and authority

Retain the supplement's exact sequence: deadline/options/ID; scoped metadata
length envelope; one retained Store read view; bounded publication metadata;
both complete real captured source scopes including unchanged/unreached manifest
members; whole per-side graph counts; released `git_workflows::load`; private
checked inputs; derivation; bounded canonical encoding. The operation restores
the exact prior SQLite length limit and never commits, rolls back or replaces a
caller's transaction. In autocommit it owns only its local deferred read guard.

The raw envelopes remain snapshot cells <=32,000,000, graph cells <=64,000,000,
4,096 SQLite row overhead, report record <=64,000 and report artifact <=16,000,000.
Use `min(prior_limit, 64_004_096)` and retain the existing released fixed profiles.
Do not raise an inherited cap, invoke a public bare-Graph constructor, or widen
`ReportData`, `WorkflowOptions`, `native_impact`, R3 internals or R5-A schemas.
Both-side source-policy admission precedes inherited source-CAS reads. Bounded
publication metadata CAS remains the explicitly permitted preflight exception.

The two `inputs` bindings contain the actual admitted snapshot, graph hash and
known historical analyzer producer (`yoke-tree-sitter-8` or
`yoke-tree-sitter-9`). Recover the epoch through the captured/source-proof path,
not from the active analyzer constant, a guessed entity producer, or live files.

## Exact result shape

Every listed field is required; no field below is an optional extension point.
All indices are zero-based indices into the named final canonical array. All IDs
and content hashes are original captured IDs or lowercase 64-hex SHA-256 values.
Every table key includes `side`, even if base and head share an ID/hash. The
`inputs` object binds each side's snapshot, graph and epoch once.

| Top-level field | Value |
| --- | --- |
| schemaVersion | integer 1 |
| kind | `git_affected_details_v1` |
| algorithmVersion | `yoke_git_affected_details_1` |
| policy | `captured_hierarchy_v1`, equal to options.policy |
| resultId | domain-separated identity below |
| reportArtifactId, workspaceId | exact verified publication/workspace IDs |
| inputs | `{base:{snapshotId,graphHash,analyzerEpoch},head:{snapshotId,graphHash,analyzerEpoch}}` |
| options | exact 13 caller values, including every explicit zero |
| entities, observations, evidence | the three unique side-bound tables below |
| originalSeeds | distinct original root records, retaining all original row indices |
| originalImpactRefs | all original report impact rows in report order |
| originalImpactObservationRefs | all original report impactObservation rows in report order |
| expandedSeeds | unique new side/entity candidates, excluding all original roots |
| hierarchyStates, hierarchyProofRefs, memberReasons | hierarchy provenance tables |
| walkSeeds, incomingStates, incomingProofRefs | independent incoming provenance tables |
| coverage, consumed | exact objects below |
| rowUnits, wireBytes | exact recursive array count and complete canonical byte length |
| inferenceClass | `source_bound_structural_candidate` |
| complete | false |
| compilerBindingEstablished, runtimeDispatchEstablished | false, false |
| limitations | fixed ordered array below |

Unique tables contain only rows required by an original root, baseline row's
entity, state, member reason, walk seed, or new proof observation. Original
baseline observation references do not cause copies into new proof tables.
For each new proof observation include its source and resolved target entities
and every referenced evidence row. No unused whole-graph table copy is emitted.
An unresolved observation counted in global coverage is not thereby a proof row.

`entities` rows have exactly:

```
side, entityId, snapshotId, path, contentHash,
name, kind, language, producer, parentKey,
range:{startByte,endByte,startLine,endLine},
nameRange:null | {startByte,endByte,startLine,endLine}
```

`parentKey` is the captured string or null, never a name-derived parent claim.
`nameRange` is null only when both captured name offsets are absent. Include no
signature, qualified-name surrogate, source text or new inferred owner ID.

`observations` rows have exactly:

```
side, observationId, snapshotId, sourceEntityId, targetEntityId,
targetLabel, relation, resolution, producer, evidenceIds, relationClass
```

`targetEntityId` is the exact captured optional target. The full `evidenceIds`
array retains original order and repetition. `relationClass` is
`contract_membership` for implements, `structural_inheritance` for extends or
inherits, and `incoming_candidate` for calls/references. The original relation
and resolution are preserved; these labels never upgrade heuristic resolution.

`evidence` rows have exactly:

```
side, evidenceId, snapshotId, path, contentHash,
range:{startByte,endByte,startLine,endLine}, anchor, trust, producer
```

Ranges are source bytes, not character offsets. Validate exact captured hash,
UTF-8 boundaries, containment, one-based LF line positions and zero-width ranges.
CRLF is preserved. `anchor`, `trust` and `producer` are copied unchanged. The
table stores each `(side,evidenceId)` once while repeated references survive.

## Original references, states and reasons

`originalSeeds` rows:

```
{side, entityId, changedRowIndices,
 eligibility:{status,reason,ownerEntityId}}
```

`changedRowIndices` is the complete ascending list of matching original
`report.changedSymbols` indices, not a newly assigned root ordinal. All original
rows occur exactly once across these lists. `status` is `expandable` or
`unsupported`; `reason` is null for expandable roots, otherwise the first
applicable code below. `ownerEntityId` is the validated direct TypeScript
class/interface owner for expandable roots, otherwise null.

Eligibility order is other language (`unsupported_language`, including TSX),
non-method (`unsupported_entity_kind`), absent/not-found parent (`missing_parent`),
unsupported parent language/kind (`unsupported_owner`), both name offsets absent
(`missing_name_anchor`), exact name constructor
(`constructor_name_not_expanded`), then expandable. Structural corruption is an
error, never an unsupported reason: duplicate keys, a cross-snapshot/path parent,
noncontained child, half-present name offsets, invalid/misplaced UTF-8 name
range, or source spelling differing from captured name. Global duplicate entity
keys are refused before indexes. Parent/name integrity checks apply to roots and entities
used for new membership/proof claims; the released loader still validates all
graph anchors. This scoped new-validator rule and the cross-root original-member
rule below are explicit root decisions received for this draft. They do not
modify the frozen supplement. A malformed UNREACHED optional parent/name is not
a new-claim or refusal guarantee. For emitted/reached rows, malformed present
name offsets or a resolved parent with wrong side/path/containment refuse before
eligibility classification. Missing parent is an unsupported original-root
reason (and a private eligibility-call outcome); both name offsets absent is an
unsupported expansion reason. Neither absence grants ownership.

Method-owner eligibility is separate from generic integrity validation. Emitting
a class/interface/function as a proof endpoint does not require it to have a
class/interface method owner: a class with a captured file parent is valid.
Validate its captured anchors and any resolved parent containment generically;
apply the structural method-owner profile only when claiming member expansion.

`originalImpactRefs` rows are `{originalRowIndex,side,entityId}`;
`originalImpactObservationRefs` rows are
`{originalRowIndex,side,observationId}`. They resolve against the verified report
bound by `reportArtifactId`; no original claims, ordering, distances, relations
or evidence are rewritten. Original incoming impact remains a distinct policy.

`expandedSeeds` rows are `{side,entityId}`. A related member already present among
*any* original roots remains eligible as another root's member reason/walk seed,
but consumes no unique-new-seed slot and is not duplicated in this table.

A hierarchy adjacency entry is eligible exactly when its captured relation is
extends, implements or inherits, its target is Some(target ID) present in the
same admitted side graph, and both endpoints are TypeScript class/interface
entities. Ancestors follow original source -> target; descendants follow the
reverse adjacency in a separate monotone walk. Preserve the captured resolution
category and every parallel target observation. Heuristic or ambiguous target
rows are not filtered out. A null target label never authorizes a lookup/read or
an inferred endpoint. This target-binding definition does not upgrade resolution
to compiler/dispatch proof. Incoming entries similarly require Some(admitted
same-side target) and relation calls/references, with no TypeScript/member-kind
restriction; the incoming walk reverses the original source -> target direction.

`hierarchyStates` rows:

```
{side,rootEntityId,direction,entityId,distance,witnessObservationIds,
 boundary,unexaminedAdjacencyCount}
```

Keys are `(side,rootEntityId,direction,entityId)`. Direction is `ancestor` or
`descendant`. Entity is a captured type. Each eligible original root begins at
its owner with distance zero and an empty witness, in each separate monotone
walk when hierarchy depth >0. Zero hierarchy depth creates no hierarchy states.
`boundary` equals `distance == options.maxHierarchyDepth`; the final count is the
known eligible adjacency length at a boundary, otherwise zero.

`memberReasons` rows:

```
{side,rootEntityId,direction,typeEntityId,memberEntityId,hierarchyStateIndex,
 eligibility:{status,reason}}
```

The referenced state has distance >=1 and is not the root owner. Include every
direct captured-same-name method joined to that reached type by exact parent_key,
with the same membership/name checks. A parentless method never receives a
related-type reason from matching names, paths, qualified names or containment;
there is no fabricated nullable owner/state link. This is the explicit finite
join boundary agreed by the root. Eligibility status is `eligible` with null reason after an exact
source-proven name match; otherwise it is `unsupported` with the applicable
reason code, including missing_name_anchor for a joined member. Retain
unsupported member reason rows and count them; they do not
create expanded seeds or incoming walks. Invalid partial/misplaced name anchors
still refuse. Related-type overloads are all included. Same-owner overloads, a cycle
back to the root owner, unrelated same-name types and up-then-down siblings do
not create member reasons. No compatibility, override identity, static/private
filter, Unicode normalization or dispatch claim is made.

`hierarchyProofRefs` rows are `{stateIndex,observationId}`. Each records one
eligible adjacency examination from that hierarchy state at distance < the depth
limit. All parallel, self-loop, cycle and already-visited-target observations
remain referenced, even when not chosen as a witness. No continuation past a
depth boundary is performed solely to collect proof. Shared observation table
storage does not remove per-root/direction references.

`walkSeeds` rows:

```
{side,rootEntityId,entityId,origin,memberReasonIndices}
```

Origin is `original` with an empty reason list for the root itself, or `hierarchy`
with every eligible matching reason index for that `(side,root,member)`. One member reached
in both directions has one incoming walk retaining both reasons. Different
original roots always retain separate walks. A global entity table is only
storage deduplication.

`incomingStates` rows:

```
{side,rootEntityId,walkSeedEntityId,entityId,distance,witnessObservationIds,
 boundary,unexaminedAdjacencyCount}
```

Keys include phase (this table), side, original root, walk seed and current
entity. Every walk seed has a distance-zero state even at incoming depth zero.
Walk only reverse resolved calls/references edges; source is the caller and
captured observation orientation is unchanged. Incoming distance starts at that
walk seed, independently of hierarchy distance. Boundary fields use the incoming
depth and its known eligible incoming adjacency. `incomingProofRefs` rows are
`{stateIndex,observationId}`, with the same complete examination rule.

## Witnesses, ordering and work

A witness contains observation IDs in traversal order from the owner/seed
outward. Choose shortest distance first, then the lexicographically least full
ID sequence, comparing UTF-8 bytes. Do not choose only the final ID, enumerate
all paths, merge walk origins, or remove proof when visited-state deduplication
chooses a single witness. Settle a full BFS level before the next one.

Canonical array order is:

| Array | Sort key |
| --- | --- |
| entities / observations / evidence / expandedSeeds | side (base then head), original immutable ID UTF-8 bytes |
| originalSeeds | side, entityId; changedRowIndices ascending |
| originalImpactRefs / originalImpactObservationRefs | originalRowIndex ascending |
| hierarchyStates | side, rootEntityId, direction (ancestor then descendant), distance, entityId |
| memberReasons | side, rootEntityId, direction, hierarchy-state distance, typeEntityId, memberEntityId |
| walkSeeds | side, rootEntityId, entityId; memberReasonIndices ascending |
| incomingStates | side, rootEntityId, walkSeedEntityId, distance, entityId |
| hierarchyProofRefs / incomingProofRefs | stateIndex ascending, observationId UTF-8 bytes |

References are assigned after canonical state/reason order is fixed. Witness
arrays and original evidence vectors preserve their specified sequence, not
table order. Entity IDs, observation IDs and evidence IDs never acquire a
cross-side identity assertion from matching spellings, ranges or R5-A renames.

`consumed` has exactly `graphs` (base/head objects with entities, observations,
evidence counts), `originalSeedRows`, `uniqueExpandedSeeds`, `hierarchyStates`,
`incomingStates`, `states`, `indexObservationScans`, `hierarchyAdjacencyVisits`,
`incomingAdjacencyVisits`, `edgeVisits`, `newEvidenceSourceBytes`.

Count whole graph vectors per side before indexes. Count every original changed
row before copying, and each new unique nonoriginal side/entity before insertion.
Charge new states before queue allocation. One side-wide observation scan builds
all new indexes and charges every original observation once, including unrelated,
unsupported and unresolved facts. Then charge every examined per-walk adjacency
entry. Thus `edgeVisits == indexObservationScans + hierarchyAdjacencyVisits +
incomingAdjacencyVisits`, and adjacency visits equal the respective proof-ref
counts. `states == hierarchyStates + incomingStates`.

Reserve full captured sizes for distinct content hashes before the new validator
opens them. A reused hash does not bypass per-side/path policy or binding checks.
`newEvidenceSourceBytes` counts this new validator only; inherited whole-source
validation can read files repeatedly under its separate existing profiles.
Row credit counts *every array member anywhere*, including repeated evidence
refs, row-index refs, witness IDs, reason refs, limitations and empty-proof roots.
Object properties and scalar values themselves are not row units. Final counting
must equal the charged count. No input maximum promises to fit another budget.

`coverage` has exactly:

```
withinPolicyComplete:true,
hierarchy:{unresolvedObservations,unsupportedObservations,boundaryStates,
           unexaminedAdjacency},
incoming:{unresolvedObservations,boundaryStates,unexaminedAdjacency},
unsupportedOriginalSeeds, unsupportedMemberReasons
```

Hierarchy global unresolved counts include hierarchy-relation rows with null
targets; unsupported counts include resolved hierarchy rows whose endpoint
language/kind is ineligible. Incoming unresolved counts include null-target
calls/references rows. These global counts are independent of reached walks.
Boundary counts are counts of boundary states; unexamined adjacency is the sum
of their known eligible adjacency lengths. `withinPolicyComplete` certifies only
that the finite selected-depth policy completed without omission. It never
changes `complete:false` or either proof-established false field.

Limitations are exactly, in this order:

```
structural_candidate_not_compiler_or_runtime_proof
same_name_members_not_override_or_rename_identity
captured_report_not_live_ref_freshness
selected_depths_not_exhaustive_reachability
new_source_bytes_exclude_inherited_loader_io
```

## Canonical bytes and result identity

Use compact `serde_json` JSON, no BOM/whitespace/terminal LF, recursively sorted
object keys, the canonical array orders above, and JSON integer values. Strings
retain their exact Unicode bytes without NFC/NFD rewriting. No float appears in
the owned result. CLI LF, when later implemented, is outside these result bytes.

Start resultId with 64 ASCII zeroes, then settle `wireBytes` against the complete
serialized object until its value equals that object's exact byte length. Bound
all streaming counts and intermediate reservations by maxOutputBytes before
copy/allocation; use checked arithmetic and the original shared deadline. Refuse
if the bounded fixed-point construction cannot settle; never truncate output.

The final identity is lowercase hexadecimal:

```
SHA256(b"yoke_git_affected_details_1\0" || canonical(object_without_resultId))
```

The identity-free object removes only resultId and retains settled wireBytes,
all options including limits/zeroes, both input identities, coverage, budgets,
all rows, false proof fields and limitations. Replacing the zero placeholder
with the 64-hex digest preserves wire length. Serialize implements the complete
structured object; canonical_bytes exposes its exact sealed bytes. There is no
API that accepts these bytes to mint/reseal authority.

## Draft coverage and remaining qualified cases

The Rust draft has 12 genuine-fixture behavior definitions. Its fixture is an
uncompiled shared draft, not a qualified input yet. Root must first qualify the
actual released graph observations and changed-seed rows, including possible
generic unresolved inherits rows and all extra captured references. No test
may alter an expected literal to fit an unqualified graph silently.

Later private checked-input cases are still required for full-lex diamond ties,
parallel inheritance, repeated evidence vectors such as [e2,e1,e2], cycles,
overloads, malformed parents/name anchors/evidence, two roots sharing a walk
seed, different distances from two walk seeds, genuine both-side policy ordering,
transaction snapshot consistency, metadata-cell bounds, and cancellation during
walk/sort/encoding. Those mechanics require controlled valid private checked
inputs or genuine captured variants; this draft does not fake them with a public
bare-Graph constructor. The first library implementation cannot claim CLI
completion. Existing R5-A/digest/report bytes and all original inventory/full
certification flags remain unchanged.
