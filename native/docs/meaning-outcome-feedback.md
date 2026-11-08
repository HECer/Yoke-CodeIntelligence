# Authored outcomes for Meaning claims

The separate `knowledge::feedback::meaning` contract records authored outcomes
for existing local native Meaning **concepts, directed relations and hyperedges**.
It replays complete original/evaluated native View, source, overlay and Meaning
receipt closures before recognizing a target or quotation. It runs no provider,
captures no source and never changes a claim. Knowledge/plan event and transcript
schemas, identities and ranking behavior remain separate.

```text
yoke --root REPOSITORY --allow-writes feedback meaning-record --input EVENT.json
yoke --root REPOSITORY feedback meaning-rank --input RANK.json
```

A closed event draft has `schemaVersion:1`, `target`, `actor`, `session`,
`sequence`, `occurredAtMs`, `outcome`, `rationale` and `sourceEvidence`.
Author/session/time/outcome fields retain the [authored feedback policy](outcome-feedback.md).
The closed target contains `origin`, `evaluated` and `reference`. Each View pin
has the existing five exact fields: `workspaceId`, `snapshotId`, `viewId`,
`viewArtifactId` and `baseGraphHash`. Both selections belong to the current local
Store. Imported JSON/pins are selectors, never checked source capabilities.

The closed reference has `kind` (`concept`, `relation` or `hyperedge`),
`graphId`, `packetIndex` (0..63), and `claimId`. The original View must actually
select that published Meaning graph and packet. `claimId` is the selected item's
original normalized claim identity (`claimId` in a Meaning View, `id` in its native
materialized graph), not a label, alias or namespaced View ID. Kind, graph and
packet are all binding. A missing Meaning layer or claim refuses; it never
produces an empty complete replacement.

Evidence entries contain `side` (`origin` or `evaluated`) and `source` with exact
`path`, `contentHash`, `startByte`, `endByte`, `quote`. At least one is required.
Origin quotes must be wholly contained in one original support range of the
selected claim and match that support's path/hash and primary snapshot bytes.
A neighboring claim's evidence is insufficient. Evaluated quotes must match the
selected evaluated primary snapshot bytes in a path supporting the original
claim. This latter association is deliberately labeled
`evaluatedQuoteCorrespondence:"same_claim_support_path_only"`: changed file
bytes do not establish a surviving concept, compiler binding or semantic identity.
Receipt-only historical snapshots do not become new quote inputs. Exact UTF-8
boundaries, CRLF, side and original quote precision remain authoritative.

The original pin and reference define a stable `subjectId`; evaluated pins
remain per-event observations. Sequences are unique for that subject plus the
unverified actor/session label pair, even across different evaluated Views.
Timestamps must be nondecreasing by sequence, including out-of-order arrivals.
An identical retry returns the original event ID with `inserted:false`.
Changed content at an occupied sequence refuses; no event is overwritten.

Rank input has `schemaVersion:1`, `targets` (1..128 distinct subjects), `asOfMs`,
`decayWindowDays` (1..3650) and `limit` (1..128). Each selected target is replayed;
every event for those subjects replays its own original evaluated selection.
Only the highest eligible sequence per actor/session label votes. Future and
superseded events remain visible with their dispositions. Other subjects are
counted in `eventsOutsideSelectedSubjects`. The existing integer hyperbolic
decay and signed outcome formula apply. Items sort by descending historical
score, then subject ID; limit omissions are explicit. Scores establish no current
applicability, independent corroboration, interpretation truth or actual execution.

Outputs retain the complete selected packet, graph registration, chosen original
claim and support, plus original View limitations, source-scope roles, packet
bindings, coverage and receipt provenance. Borrowed packet serialization applies
whole-output admission before materialization/publication; large source claims
are not cloned or silently omitted to fit. Original directed parallel/reverse/self
relations and ordered hyperedge roles remain separate. No outcome label merges
concepts or promotes an inference into canonical source truth. Original provider
unknown usage/cost remains null, never fabricated as zero.

The separate immutable namespace is `native_meaning_feedback_event_v1`. Defaults
retain the feedback ceilings: 10,000 events, 32 MB namespace JSON, 64 KB drafts,
32 quotes/32 KB total quote text, 32 replay pairs, 64 MB replay input/source
reservations and 4 MB whole output. Existing host options can lower these bounds;
`--max-candidates` bounds the selected subjects. Native replay limits remain.
Insufficient capacity refuses the complete operation. No overflow artifact or
automatic transcript candidate/event creation is introduced.

Writes require explicit CLI authority and an owned immediate SQL transaction.
All proof, sequence, quota and output checks precede insertion; SQL failure rolls
back the event. Caller transactions refuse writes; reads preserve caller read
transactions and SQLite limits. Retention replays every event and original source
closure before recognizing its View/source roots; malformed, rehashed or missing
dependencies refuse maintenance. No arbitrary ID string becomes a retention root.

`executionObserved`, `workspaceApplicationObserved`,
`currentApplicabilityEstablished`, `independentCorroborationEstablished`,
`interpretationTruthEstablished`, `semanticIdentityEstablished`, `sourceAuthority`
and `complete` remain false. These are source-bound authored historical outcomes.
Broader typed targets, semantic transcript candidates, independently corroborated
outcomes, observed execution and model-quality qualification remain open.
