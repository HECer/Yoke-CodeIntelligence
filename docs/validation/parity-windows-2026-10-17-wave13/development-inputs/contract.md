# Explicit source-bound outcome feedback

This R9 implementation is application knowledge, separate from coding-agent
AgentMemory. It never discovers sessions/transcripts, executes project code,
calls a model, sends data or turns an assertion into source/write authority.

## Source and event contract

`knowledge::feedback` reuses `graph_view::comparison_input` for complete replay
of two explicit native View selections, including original overlay/Meaning
receipts. `origin` identifies the snapshot in which the selected knowledge
record was authored; `evaluated` identifies the source used by an outcome.
Both strict pins carry workspaceId, snapshotId, viewId, viewArtifactId and
baseGraphHash. Their opaque checked pair is not constructible from JSON.

Strict event drafts contain schemaVersion=1, origin, evaluated, knowledgeId,
actor, session, sequence, occurredAtMs, outcome, rationale, sourceEvidence and
transcriptEvidence. Outcomes are success, failure, inconclusive, corroboration
and contradiction. Actor/session are unverified host labels. Sequence is a
positive bounded integer. Sequence slots are unique per knowledge/origin View,
actor and session; identical replay is idempotent and a conflicting slot fails.
Timestamps must be nondecreasing by sequence for that label pair.

The original knowledge ID/shape/hash and all original source anchors are
checked against the origin View. Native evidence quotes use evaluated primary
snapshot files only, exact hashes and UTF-8 byte ranges. Historical receipt-only
sources are not new evidence inputs. Transcript quotes use a separate original
UTF-8 coordinate space and a checked explicitly imported transcript. At least
one original source or transcript quote is required; quotes support only what
was written, not independent execution truth. Event source quotes overlap the
knowledge source paths. Neither inferred semantic identity nor provider
attestation follows. All quotes, source pins and attribution remain immutable.

## Explicit transcripts

`feedback import-transcript --input HOST_TEXT --label LABEL --author LABEL`
accepts only the explicitly selected plain UTF-8 file, at most 1,000,000 bytes.
It stores exact original bytes and strict content-addressed registration in the
project Store. Label/author are host assertions, not ownership authentication.
Protected files, links, obvious credentials and unsupported controls are
refused. There is no application-session discovery or Claude-Mem integration.

`feedback mine --transcript ID` searches original bytes using a documented
bounded literal signal policy. Every candidate contains exact original UTF-8
start/end/quote, policy and candidate kind. Candidates require explicit host
selection to become events; no automatic promotion or authentication occurs.
This deterministic candidate policy is not general model-based transcript
understanding. Case-insensitive matching retains original byte coordinates.

## Transparent ranking

`feedback rank --input HOST_JSON` explicitly selects origin/current Views,
knowledge IDs, asOfMs and decayWindowDays. It replays every included event's
original selections before using its vote. Only events matching the exact
selected origin View participate. Other origin selections are counted as
outside selection. Future-dated events are retained with an exclusion reason.
Among eligible events, only the highest sequence for each actor/session label
pair votes. All prior immutable events remain inspectable.

Success/corroboration contribute +1000, failure/contradiction -1000 and
inconclusive zero. Integer weight is floor(1000 * windowMs / (windowMs + ageMs));
negative votes negate that positive integer weight. This is an explicit
hyperbolic decay policy, not an exponential half-life. u128 intermediates avoid
overflow. Sum and per-event arithmetic are visible. Changed/excluded/deleted
knowledge anchors and observed authored supersession make an item ineligible,
with zero effective score. This does not prove a correction or independence.
Sort is eligible first, then score descending and knowledge ID ascending.
An explicit page limit never changes the full bounded scoring computation.

## Admission and publication

Initial capacities: 128 selected knowledge IDs, 10,000 events/32 MB stored
feedback JSON, 32 distinct replay pairs, 64 MB replayed source reservations,
64 MB replay metadata, 32 source quotes plus 8 transcript quotes per event,
32 KB aggregate quotes, 64 KB draft, 4 MB whole response and 10,000 mined
candidate occurrences. Capacities are real whole-operation refusals.
Inherited replay materialization bounds remain independent.

Read operations require an existing Store and preserve caller read transactions
and SQLite limits. Writes require explicit CLI --allow-writes, reject caller
transactions, validate all inputs/source proofs/sequence/quota/output before
publication and use an immediate SQLite transaction. Transcript CAS publication
is immutable and may leave an unreachable blob after a later I/O failure;
neither multi-resource atomicity nor hostile-writer exclusion is claimed.
Capture/analyzer/language-server global flags are refused. JSON/refusal/output
admission happens before source Store creation. stdout is bounded with one LF,
and failures never save overflow artifacts. Deadline/cancellation propagate.

Retention validates the complete feedback set once, reuses checked replay pairs,
recognizes only exact event/transcript schemas and keeps original source and
transcript blobs reachable. Unknown/corrupt records refuse retirement. No
event deletion or purge privilege is introduced.

## Verification

Before implementation: demonstrate CLI behavioral RED on the unchanged sealed
R4 executable; preserve existing knowledge/notes/decision/skill baseline evidence
with exact source hashes. Then test real source/receipt replay, idempotency,
sequence conflicts, attribution-label collapsing, signed decay arithmetic,
future time, source currentness, supersession, exact transcript UTF-8 ranges,
explicit permission, source pin/quote/record/CAS corruption, metadata/source/
output capacities, protected/linked host paths, cancellation and retention.
No competing builds during the active R2 whole acceptance. Independent review
and fresh final gates are required before primary integration/acceptance.
