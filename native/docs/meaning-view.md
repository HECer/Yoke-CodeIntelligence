# Published meaning selections

Meaning Views select already published graphs with explicit `meaningGraphIds`.
They do not build graphs or resolve a provider. An empty selection is omitted
from the serialized View, preserving the existing structural View format and
identities.

`meaning_view::load_selected` checks the entire selected immutable dependency
closure against the requested workspace, snapshot, canonical graph, analyzer
epoch and capture policy. IDs are sorted deterministically; duplicate IDs are
errors. Selecting an older graph does not rebind it to a newer snapshot. A
separately published cache-rebound graph can cite its original receipts while
binding quotations to the selected snapshot.

The optional `MeaningLayer` keeps its own concepts, directed relations,
hyperedges, evidence and citation links. It does not add concepts to the
canonical code graph. A concept can cite several files and has no declaration
anchor or invented path. Citation links identify actual canonical file entities.
Every quotation is an exact, nonempty UTF-8 range from the captured source.
Claim and evidence IDs are namespaced by selected graph and packet, while the
original IDs remain available as `claimId` and `baseEvidenceId`.

Parallel and reverse relations remain separate. Hyperedge participant order,
roles and repeated role labels remain intact. Roles do not establish direction.
Aliases are candidate labels for searching; they do not merge concepts. Claims
retain `inferred_concept` resolution and `unverified` provider attestation.
Explicit `no_claims` packets retain source bindings and coverage.

`validate_selected` reconstructs the layer from its checked publication and
requires exact equality. It returns typed snapshot, artifact and record roots
for retention. The shared input limit is 32,000,000 bytes, including distinct
reachable source blobs, graph/response artifacts and proof records once. Each
mutable ledger task or attempt admits its full 64,000-byte row capacity, so
later accounting updates cannot change the sealed layer's identity. Actual
ledger rows must remain within that bound and pass the same validation. This
admission total reserves capacity; it is not a physical storage measurement. The
meaning reader admits the complete bounded closure before decoding whole
artifacts. Selection and dependency counts are bounded as well as byte sizes.

`archive_selected` produces opaque proof metadata. Archive consumers must also
retain every file in `MeaningArchive::source_scopes()`, including historical
original snapshots. `replay_archive` receives frozen source bytes through a
callback keyed by the supplied snapshot and file. It checks the same source,
capture, canonical graph, request, receipt and accounting lineage without a
live Store, provider, configuration or credential lookup. Missing historical
bytes or receipts are errors. Frozen accounting reports describe the archived
receipt state; they do not attest current host ledger state or provider truth.
Unknown reports remain unknown.

The proof metadata limit is at most 32,000,000 bytes and the separate source
archive limit is at most 64,000,000 bytes. Source bytes still count toward the
shared selected-input limit. Consumers that only accept the existing source
graph must reject a meaningful layer explicitly until they support the typed
layer and its multi-snapshot archive closure.
