# Bounded saved Graph causal-RED package

This ignored directory contains unavailable promotion stubs and unchanged
existing oracles. It is not a functional implementation or test result. Root
alone owns normal promotion, compilation, behavioral RED, and release of the
functional bodies. No Cargo, Node, provider, service, or fixture was run here.

`retrieval_capacity.rs` and its test child are byte-for-byte copies of the
capacity369292 package. Both saved-Graph test children are unchanged copies of
the cccfe018 saved-row addendum and bce1016a strict-schema v2. The saved-Graph
stub adds the agreed `read_graph_in` and short-transaction `read_graph` APIs;
both return explicit implementation_unavailable, as does the unchanged verifier
API. No new authority, permissive fallback, or successful empty result exists.

The package contains 20 ordinary tests and one ignored qualified large-row
test: 7 ordinary capacity tests, 7 ordinary saved-row tests plus its one ignored
large-row test, and 6 ordinary strict-schema tests. The separate partition
capacity fixture file contains 1 ordinary and 5 qualified ignored tests; that
file is not duplicated or registered by this helper-only package.

## Root integration

Copy the five Rust files to their same filenames in native/src and add private
`mod retrieval_capacity;` and `mod retrieval_saved_graph;` in native/src/lib.rs.
Their cfg(test) child registrations are already present in the stubs. The root
must not register the child test files again in lib.rs. Root alone executes:

```text
cargo test --manifest-path native/Cargo.toml --lib retrieval_capacity::tests
cargo test --manifest-path native/Cargo.toml --lib retrieval_saved_graph::tests
cargo test --manifest-path native/Cargo.toml --lib retrieval_saved_graph::schema_tests
```

The large saved-row test remains ignored until separately qualified and released.
Successful setup and compilation are not behavioral RED. With unavailable
stubs, deep tests first fail at the genuine positive verifier baseline; those
failures establish only missing functionality. They do not yet prove schema,
transaction, preallocation, cancellation, or restoration guards. The already
passing observation-dominance and path-syntax controls are not feature RED.

## Functional design after causal release

The exact Connection pointer of the borrowed Transaction is checked against
Store.db before any limit mutation. `None` means only no row. Other database,
capacity, storage-type, malformed-JSON, and integrity failures remain typed
hard failures. The caller retains ownership of its read transaction.

A scoped LENGTH guard saves the exact inherited host limit and lowers it to
min(prior,64000000+4096). Metadata first reads length(CAST(value AS BLOB)) and
typeof(value) in the caller transaction. Capacity and non-text values fail
before a separate payload SELECT. The payload is borrowed from its SQLite row
only during strict bounded decoding; byte/type checks run again. This avoids
an extra full Rust String and Value clone. The guard explicitly restores and
checks the exact limit on every Result path, with a Drop restoration attempt
on unwinding. Known Deadline.failure state takes precedence over opaque I/O
errors after callbacks.

Strict custom/typed decoding rejects unknown Graph, Entity, Observation,
Evidence, Diagnostic, and Coverage fields and duplicate decoded object or
dictionary keys. Collection element limits are checked before requesting the
next element; unknown collection hints cannot preallocate. Deadline checks
occur in element/string/map decoding. Graph bytes remain bounded independently.
No production Value tree or encode/decode roundtrip is used. The Graph's root
and member snapshot IDs must equal the requested valid ID. The verifier calls
the reader and compares its bounded serialized fingerprint; absence is a hard
authority failure in this verifier. The standalone reader's data is not a
captured-source proof.

The streaming fingerprint uses a bounded counting SHA256 io::Write sink. It
rejects a write beyond 64000000 bytes before hashing/traversing further, checks
the inherited deadline on writes, returns the stored typed cause for writer
failure, and maps unknown serialization failures to static Internal with code
graph_serialization. Root's real Run must invoke it on the supplied Graph before
additional saved-Graph loads, and must retain the same relevant transaction for
its immutable-source authority comparison.

No generic Store/global retrieval behavior changes. This design is readiness
information, not acceptance evidence for an unexecuted implementation.
