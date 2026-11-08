# Source-bound derived graph query view

This is an integration contract proposal. The canonical `Store::graph(snapshotId)` remains immutable. A successful standalone overlay extraction does not by itself make its relationships available to ordinary find, trace, context or collection queries.

## Typed library boundary

Introduce an independently owned `derived_view` module with these public operations:

```rust
pub struct Selection {
    pub source_overlay_ids: Vec<String>,
    pub semantic_overlay_ids: Vec<String>,
}

pub struct View; // Private validated contents and lookup indexes.

pub fn build(
    store: &Store,
    snapshot: &Snapshot,
    base: &Graph,
    selection: &Selection,
    deadline: &Deadline,
) -> Result<View>;

impl View {
    pub fn manifest(&self) -> ViewManifest;
    pub fn find(&self, options: &FindOptions, deadline: &Deadline) -> Result<Value>;
    pub fn trace(&self, options: &TraceOptions, deadline: &Deadline) -> Result<Value>;
    pub fn node(&self, id: &str, deadline: &Deadline) -> Result<Value>;
    pub fn read(&self, store: &Store, options: &ReadOptions, deadline: &Deadline) -> Result<Value>;
}
```

Host/provider permissions remain outside `Selection`. This structure selects already published immutable artifacts; it cannot start a provider, grant project access, read an arbitrary directory, invoke a model or write source. Selection rejects unknown fields, invalid IDs, duplicates, incompatible schemas and capacity excesses. No automatic discovery of every record in the state database is needed for one query.

## Validation and identities

1. Validate exact base snapshot and graph identities against the Store and workspace, using the existing shared deadline. Federated receipts require a separately grant-validated source view; they must not be accepted by weakening canonical Store checks.
2. Load each selected artifact through its registered record, enforce its byte bound before deserialization, verify the content-addressed artifact, and compare the immutable record's snapshot/workspace/base graph hash and producer. Historical format versions need explicit decoders, never a guessed shape.
3. Validate every overlay node/observation/evidence ID, endpoints, source path/hash, UTF-8 range and line binding. Validate that referenced canonical targets exist. A decoded declaration with file-level evidence remains file-level; normalization must not promote it to an exact token.
4. Preserve canonical IDs and namespace overlay-local IDs with their schema/producer and artifact identity. Maintain `baseEntityId`, `baseObservationId`, original evidence IDs and artifact lineage. Same-name entities never merge automatically. Semantic overlay links to exact canonical selections remain links, not replacements of canonical facts.
5. Derive the view ID from snapshot ID, base graph hash, ordered selected artifacts, decoder/query versions and effective typed query configuration. Repeating that selection gives the same identity. Source omissions, uncertain resolutions, provider claims and partial coverage stay visible.

Build indexes by node ID, source path, name tokens, incoming/outgoing arc indices, evidence ID and source hash. Each directed observation has its own index; parallel call sites survive. Cross-project collection namespaces add workspace/snapshot lineage around these view identities rather than flattening projects.

## Query behavior

Find searches selected canonical and overlay labels/kinds with explicit project/path filters and returns whole records, source evidence and view lineage. Source body retrieval uses captured CAS bytes; package/config/Markdown metadata does not become a substitute for source. Exact-name preference and lexical rankings must describe their actual implementation.

Trace reuses the native directed traversal semantics over an indexed view projection. Filter categories preserve `calls`, `references`, `inherits`, authored dependency links and heuristic candidates. Caller opt-in controls candidate relations. Traversal orders are not converted into asserted paths. Context planning composes view find/trace/read without additional model calls.

Read resolves an ID to its validated source bindings and reads only that snapshot's exact CAS bytes. When a node has multiple or enclosing-file anchors, the response states that precision and requires a selected anchor/range when ambiguity matters. Generated display labels are not treated as authoritative original source. Prepared changes still use canonical snapshot hashes and the existing approval/journal path, even when discovery began in an overlay.

## Retention and collection ownership

Recognize `native_derived_source_graph_v1`, `native_semantic_overlay_v1` and any persisted view-manifest record as explicit snapshot/artifact/evidence roots. Unknown formats remain conservatively unreclaimable. Persisting a view does not create source-write authority.

Collection registration may accept a validated view and archive its selected overlay manifests plus exact source bytes transactionally. Historical collection reads use that standalone archive after the original Store retires or purges history; no remote registry entry silently pins a source Store. Removing or replacing a registration uses its exact expected ID. Source archive cleanup is a separate reviewed operation.

## Acceptance gates before interface parity

- An npm/Cargo dependency, Markdown citation and configured call hierarchy become findable and traceable through the same selected view with exact source provenance.
- Opposite and parallel observations, duplicate symbol names and conflicting contexts retain separate identities and labels.
- Source changes, forged overlay scope, missing evidence, unknown fields, invalid UTF-8 boundaries, URI escape and capacity/deadline excesses fail explicitly.
- Old selected views remain readable after a new live snapshot; standalone registered views remain readable after original Store retirement and CAS purge.
- Ordinary index/query operations do not start models, semantic servers, shell commands or network fetches.
- CLI, eight-tool MCP operations and optional compatibility profiles exercise the same typed implementation. Their tests must prove overlay retrieval behavior, not merely demonstrate that renamed tool aliases exist.
