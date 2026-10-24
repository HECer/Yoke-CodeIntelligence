# Prepared source-bound vector publication

`knowledge::PreparedVectorAdmission` validates an entire destination plan before
a parent-managed embedding batch reserves or sends any model attempt. Its fields
are private and it cannot be deserialized from a provider response. The host passes
an immutable Store snapshot and graph, an explicit versioned model label, ordered
entity IDs, exact dimensions, a serialized-record bound, and an absolute deadline.

Admission verifies the exact published snapshot and graph, every selected entity,
the captured path policy, original source hash, file size, UTF-8 boundaries, unique
IDs, dimensions and coordinate counts. Configured read-only dependencies use the
same `CapturedSourceScope` as normal source retrieval; this does not grant source
writes or permit protected paths. Source blobs are admitted against their captured
size before allocation. The reader caches at most 8 MiB of source text.

An admitted batch requires an absent `(snapshot, model)` vector namespace. It also
checks the shared dense/local vector-store limit of 512 collections and 128 MiB.
The per-record maximum remains 24 MiB. Before model results exist, the byte bound
includes the exactly serialized metadata plus 33 bytes per coordinate: a
conservative finite-f64 JSON representation and separator allowance. Admission
therefore does not assume that returned coordinates serialize as short numbers.
Hosts may select a smaller record bound but cannot raise the core maximum.

`prepare` consumes the admission and accepts the existing dense import shape:

```json
{
  "snapshotId": "<captured snapshot ID>",
  "model": "<versioned model label>",
  "items": [
    {"entityId": "<selected ID>", "contentHash": "<original hash>", "vector": [3, 4]}
  ]
}
```

It requires the exact admitted order, identities, source hashes, count and
dimensions, rechecks the immutable source bindings, and rejects invalid, zero,
underflowing or overflowing norms. It normalizes vectors and sorts stored items
using the existing import rules. The resulting `PreparedVectorCollection` owns
the exact bounded legacy JSON bytes, collection ID and serialized hash. Preparing
it publishes no vector record and starts no model.

The parent batch orchestrator holds `Store::with_exclusive_client` throughout
admission, model attempts, preparation and publication. Network operations occur
without an open SQLite transaction. Final publication opens one immediate
transaction, rechecks namespace absence and actual global capacity, inserts the
vector collection, and persists the sealed, strictly validated batch completion
through `provider_batches::CheckedBatchCompletion`. A completion failure rolls
back both changes. This internal API cannot write arbitrary caller-selected
record kinds or keys.

The ordinary `import_vectors` API retains its existing immutable/idempotent
behavior and collection IDs. Valid imported collections retain the exact original
serialization; the new preflight path has the additional absent-namespace rule.
Source and serialization limits are checked before allocating oversized source or
encoded-import buffers. Source hashes bind supplied vectors to captured bytes;
they do not attest that a provider inspected those bytes or that a ranking is true.
