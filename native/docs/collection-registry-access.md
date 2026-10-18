# Checked collection registry access

`Registry::open` retains its existing host-selected, writable behavior.
`Registry::open_existing` opens an existing registry for inspection and graph
queries. It never creates the directory, database, or SQLite sidecars. Unknown
paths and missing databases fail. Every mutation method rejects a read-only
handle before processing source inputs.

The current read-only implementation is Windows-only. It holds a native file
handle that permits other readers but denies write and delete sharing for the
entire registry lifetime. Admission fails if a database writer is already open.
A nonempty WAL or rollback journal also fails; a trusted writer must finish
recovery and checkpointing first. After that guard, SQLite opens the database
with `mode=ro&immutable=1`, query-only mode and in-memory temporary storage.
Immutable mode cannot bypass pending WAL data. Other operating systems require
an equivalent enforced file-sharing guard before this API becomes available.

## Explicit SQLite coordination on Unix

`Registry::open_existing_for_host(directory, allow_registry_coordination, deadline)`
keeps the strict default above. With explicit host permission, Unix uses a
read-only SQLite connection, bounded schema reads, query-only mode, in-memory
temporary storage, and one read transaction pinned during admission. Committed
WAL data is included. The same transaction covers registration, proof and source
reads until the handle is dropped; later SQLite commits become visible only to
a new handle. Windows retains its stronger sharing guard with either setting.

This mode permits SQLite to create/use `-wal` and `-shm` coordination files in
the existing trusted local registry directory. It never initializes a database
or changes logical registry contents. It does not exclude direct filesystem
writers or make a database on a network filesystem safe. Do not select it for
an untrusted or concurrently replaced host directory. Symlinked ancestors,
database files and sidecars are rejected; Unix hardlinked database/sidecar
aliases are rejected because an alias can hide the real WAL pathname. This is
SQLite transaction consistency, not the strict no-sidecar contract. See
[SQLite WAL concurrency](https://sqlite.org/wal.html#concurrency) and
[read-only WAL databases](https://sqlite.org/wal.html#read_only_databases).

The CLI exposes `--allow-registry-coordination` on `collections` and
`meaning-archive load`. Sessions use the host-only `allowRegistryCoordination`
collection policy (default false, omitted from serialized profiles when false).
The policy participates in session fingerprints. Query/MCP arguments and stored
session records cannot grant this permission. Registration, source acquisition,
provider execution and federation grants retain their independent checks.

## Registration compare and swap

The new `register_cas`, `register_view_cas`, `register_federated_cas`, and
`register_federated_sources_cas` methods take `expected: Option<&str>` immediately
before the deadline. `None` explicitly requires an absent binding. `Some(id)`
requires the exact current 64-character registration ID. A missing, unexpected,
or stale binding fails without publishing source blobs or a replacement.
The condition and publication run in one immediate SQLite transaction.

The existing registration methods remain compatible upserts. Serialized native
registrations and their identities are unchanged. `remove` retains its existing
exact expected-ID guard. Registration CAS does not replace the separate live
host authority required for federation.

## Bounded metadata

`list(deadline)` returns `{count, items, executionAuthority}`. Each item has the
same shape as `describe(name, deadline)`:

```text
name, registrationId, kind, snapshotId, workspaceId, graphHash,
fileCount, sourceBytes, nodeCount, observationCount, executionAuthority
```

`kind` is exactly `native`, `view`, `federatedCanonical`, or `federatedSources`.
Views additionally report `viewId` and `selectedOverlayCount`. Foreign items
report `authorization: "current_host_context_rechecked"`. These summaries do
not publish source excerpts, signatures, credentials, or execution permission.

Inspection admits at most 100 registrations and 64 MB of combined serialized
registration input, with the existing 32 MB per-registration bound. It verifies
strict records, content identities, archived capture/view proofs, and original
source bytes. The metadata output contains only bounded names, identities,
fixed labels, and counts. Native historical graphs containing unsafe file
signatures fail with an explicit presentation-repair error. Register a newly
checked derived view to obtain safe presentation metadata; original canonical
bytes and identities are not reinterpreted.

`list_authorized` and `describe_authorized` require a current branded
`FederationArchiveAccess`. Every foreign inspection, query, and source read
rechecks current grants, receiver identity, keys, and receiver path policy.
Ordinary methods refuse foreign entries; stored receipts never grant authority.
Revocation applies even when the registry retains complete original source
bytes. Native archives remain queryable after their original project/store is
deleted.

Native epoch8/epoch9 canonical graphs are reconstructed from every exact archived
source and compared in full before a public query, source read, join, or metadata
inspection. A view must additionally reconstruct its canonical base before
validating selected overlays and projected rows. Recomputing unsigned registration,
graph, view and artifact hashes cannot turn invented symbols or retargeted calls
into accepted source facts. The extraction policy comes from validated capture
proofs and their exact snapshot identity tuples, never entity producer labels.
The original serialized registration and identity remain unchanged.

Native metadata and join lineage include `canonicalReplay`. Supported policies
report `{status: "source_rederived", analyzer: "yoke-tree-sitter-8"}` (or epoch9).
Known historical policies whose extractors are unavailable remain host-registered
archives with `{status: "historical_host_registered_unreplayed", analyzer: null}`.
They retain existing source-bound compatibility and are never relabeled as an
epoch8 replay. An old unversioned derived identity without a supported capture
generation also remains explicitly unreplayed. This compatibility is not a claim
that historical graph claims were reconstructed by their original extractor.

This module does not expose registry paths, grants, callbacks, or filesystem
authority through query JSON. Session, HTTP and CLI host registration are
separate integration layers.
