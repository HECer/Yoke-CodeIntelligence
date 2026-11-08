# Immutable graph views

`graph_view::build(store, snapshot, graph, options, deadline)` publishes a separate
source-bound view. It does not replace the canonical snapshot or graph. Default
`BuildOptions` extracts manifest, configuration and Markdown overlays. Explicit
`structuralOverlayIds` select published derived artifact IDs; `semanticOverlayIds`
select published semantic overlay IDs. Unknown kinds, fields and duplicate
selections fail explicitly. Selection does not launch providers or execute project
commands.

`load`, `find`, `trace`, `context` and `export` validate the published view identity,
registered CAS artifact, canonical source snapshot and base graph. Queries also
compare the caller's supplied view with the immutable publication. Source hashes,
UTF-8 byte boundaries, paths, snapshot IDs and supplied line numbers are checked.
Loading reconstructs canonical and selected-overlay rows from their registered
origins and compares their labels, hierarchy, observations, evidence and trust
metadata. A newly resealed artifact cannot invent canonical claims or silently
remove parallel observations.
Changed live files do not change historical captured context; absent or corrupt
historical source fails explicitly.

Canonical entity IDs remain unchanged. Overlay IDs are namespaced by artifact
identity, with original IDs and producer lineage retained. Parallel observations
remain separate. Authored Markdown links into canonical symbols resolve only to
present canonical targets. Unknown external targets remain unresolved. File-level
and unique-literal anchors retain those precision labels. Provider claims carry
`source_bound_provider_claim` trust and unverified provider attestation.

`find` accepts `query`, `kind`, `path`, `limit`. Ranking uses case-insensitive name
substring matching with exact-name preference. `trace` accepts `entityIds`,
`direction`, `depth`, `maxVisited`, `relations`, `resolutions`. `context` accepts
`query`, `kind`, `path`, `limit`, `depth`, `maxSourceBytes`; it returns selected
nodes, their directed neighborhood and captured source excerpts. Container
excerpts can cover their source document, while `declarationAnchor` retains the
original narrower declaration evidence. Excerpts clip at UTF-8 boundaries.
Files containing environment-name nodes omit raw context text and report
`environment_values_omitted` in `sourceOmissions`.

`export` supports `json`, `svg`, `canvas`, `cypher`, `wiki`, `obsidian`, `tree`,
`callflow`. JSON returns rich view contents. Other formats return a typed
`ExportBundle` under `bundle`, plus view/base graph/overlay provenance. Their
embedded metadata retains the original source bindings and trust precision.
`graph_exports::generate_verified` is crate-private and obtains a checked view
projection; the public canonical renderer retains its canonical graph guard.
Writing a bundle still uses `write_new_bundle` and requires a fresh destination,
preserving human notes and existing source files.

Records use `native_graph_view_v1` with strict fields `viewId`, `artifactId`,
`snapshotId`, `workspaceId`, `baseGraphHash`, `overlayIds`. The view artifact has
an empty `artifactId`; view identity hashes contents with both identity fields
empty, while the artifact identity hashes the bytes with `id` filled. View exports
use `native_graph_view_export_v1`: canonical export record fields plus
`graphViewId` and `baseGraphHash`; `graphHash` is the derived projection identity.
Retention validation reuses the immutable loading path and returns only validated
artifact IDs. These records establish retention lineage, never write authority.

Semantic selection accepts the two exact `native_semantic_overlay_v1` and
`native_semantic_overlay_v2` contracts. Schema 2 carries the original bounded
provider permission/policy context, exact selector omission paths/hashes and
provider limitations in its selection descriptor. Archived contexts are checked
against the original manifest without granting filesystem or execution authority.
Schema 1 retains its original bytes and absent context; absence is unknown.

New derived views present file signatures as `path [language]`, preserving original
entity/snapshot IDs, source hashes and anchors. Canonical graph and source bytes
remain exact. An older view containing source-derived file signatures requires an
explicit presentation repair and is refused on materialized use. Its exact original
artifact remains readable as data. Rebuilding from the original snapshot produces
a separate repaired view ID; the old ID is never reinterpreted.

Limits include 32 selected overlays, 20,000 nodes, 100,000 observations/evidence,
32 MB view artifacts, 32 MB aggregate selected overlay input and 64 MB captured source. Query traversal and source context
have independent item/depth/byte budgets and share the host deadline. Partial
coverage stays explicit with `complete: false`. This module does not claim
runtime/compiler proof, full semantic retrieval, or independently completed CLI,
MCP and collection integration.
