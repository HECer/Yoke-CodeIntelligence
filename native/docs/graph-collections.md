# Immutable graph collections

The host opens a Registry in an explicit local directory. Repository files cannot
register projects or launch providers. Canonical `register` retains its
snapshot namespace and now carries an exact archived capture-scope receipt.
Missing receipt fields preserve legacy serialized registration identities.
`register_view` accepts an already published selected
GraphView and verifies its original canonical graph and selected overlays through
the same strict loader used by live view queries.

A view registration archives its exact source manifest and UTF-8 source bytes,
original canonical graph, complete selected view, and original selected artifact
bytes and a strict capture-origin receipt chain. Every join verifies original
native capture IDs, configured options, derived/overlay origin bindings and path
scope before source materialization, then source hashes, artifact hashes, view seals,
source anchors and row reconstruction from canonical/selected origins. These
checks do not open the original workspace or require its Store. A retired original
snapshot and purged original source CAS therefore do not invalidate archive reads.

Canonical and selected views of the same snapshot have separate namespaces.
Lineage identifies registration, original workspace/snapshot, selected view,
original base graph and selected producer origins. Nodes, evidence and observations
preserve producer trust and precision; source bindings remain tied to the original
snapshot. Parallel directed observations retain separate IDs. Unresolved external
dependency labels remain unresolved candidates. Same-name types across different
workspaces are candidate links rather than authoritative type identity.

Both strict semantic schemas are supported. Schema 2 archives the original provider
permission/policy context, exact omission hashes and limitations; schema 1 retains
unknown historical context. Manifest-bound context validation grants no filesystem
or execution access. Source-derived legacy file signatures require a repaired
derivative with a separate view ID; exact original artifacts remain data.

An absent historical capture receipt is readable only for an exact known default
native disk capture ID with the original path guards. Configured dependency reads
require the original host option proof. Invalid non-null or unknown future receipts
are rejected; they never fall back to a guessed policy. Legacy virtual/federated
sources without archived native provenance are explicitly unsupported. Archive
reads grant no writes or provider execution, including for node_modules or target.
Generic derived capture receipts bind source manifests and scope only. They do
not independently revalidate a producer-specific Git commit/Merkle proof or an
original guarded change plan; those claims require separate retained evidence.

Collection queries support namespaced find, trace and explicit source reads.
Source reads use the immutable registry archive and return exact original hashes
and UTF-8 anchors. Derived metadata contains environment names only; an explicit
source read remains exact source data. Removal requires the expected registration
ID, and replacing a registration changes its lineage and collection identity.

Registrations are bounded to 32 MB, joins to 64 MB of serialized registration
inputs, and archived source to 4 MB per file and 64 MB per project. SQLite cell
limits precede length/payload reads and are restored afterwards. Managed SQL
progress and busy waits share the caller deadline. Registries reject links in
directories, databases and SQLite sidecars.

Tests cover archived selected views after actual original history retirement and
CAS purge, Unicode source, parallel calls, environment-value metadata omission,
foreign and modified views, resealed invented rows, corrupted selected artifacts,
registration replacement/removal, stale scopes and allocation bounds. Collection
claims remain `complete: false`; producer/source binding does not attest exhaustive
compiler or runtime correctness.
# Live-authorized foreign archives

The Rust host can explicitly call `register_federated` with a non-deserializable `FederationArchiveAccess`, a current host grant ID, and an already received remote ID. The registry stores a typed `sourceAuthority` receipt and hash-verified immutable source bytes. It never stores the host grant, permission to execute code, or a replacement native capture identity. Native registrations omit this optional field, retaining their existing serialized identities.

`join_authorized` and `query_authorized` require current host context. Every materialization resolves the grant again, checks the original receiver workspace and current receiving path policy, and reconstructs the current canonical graph from authenticated source bytes. A read revalidates the registration before returning its exact UTF-8 range. Ordinary `join` and `query` reject foreign registrations. Revoked grants, changed receiver scope, foreign receiver workspaces, and unrecognized authority fields fail closed even when archived bytes remain available.

`register_federated_sources` explicitly archives authenticated sources without asserting canonical graph provenance. It stores no graph facts; its joined scope contains no entities or observations. `read_source_authorized` provides a bounded exact source range after current source authorization. Historical source-classifier compatibility therefore cannot silently become authority over an old graph. Both modes can retain source bytes after original receipt storage is removed, while current host authorization remains mandatory.

