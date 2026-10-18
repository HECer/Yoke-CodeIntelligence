# Native graph export bundles

`graph_exports::generate(&Store, &Snapshot, &Graph, &ExportOptions, &Deadline)` verifies an immutable persisted source/graph scope and generates a registered, content-addressed bundle. It performs zero network requests, model calls or source execution. It preserves the original graph and returns an `artifactId` for exact bundle bytes.

Strict `ExportOptions` accepts `format`, `maxNodes` (default 1,000) and `maxEdges` (default 10,000). Accepted formats are `cypher`, `svg`, `canvas`, `wiki`, `obsidian`, `tree` and `callflow`. Limits are 1–5,000 selected source entities and 1–50,000 observations. Source validation uses the [graph report bounds](graph-extensions.md). Generation publishes at most 64 MB of bundle JSON. Results explicitly report omitted entities and observations and always retain `complete: false` because structural capture cannot establish universal program behavior.

Projection retains a bounded stable source ordering and only edges whose resolved endpoints are selected. Every unresolved retained observation gets its own labeled placeholder. Parallel arcs are never collapsed. Node source hashes/byte ranges and original edge evidence anchors accompany standalone structured/visual exports. Display escaping is independent of the authoritative source bytes.

| Format | Output and behavior |
|---|---|
| Cypher | `graph.cypher`; fixed entity/observation labels and JSON-escaped data values, snapshot/workspace IDs, source hashes/byte ranges and serialized evidence anchors. Every directed parallel observation has its own ID and `MERGE` statement. No database connection or push is performed. |
| SVG | `graph.svg`; offline native vector layout, arrowheads, independently drawn parallel/self arcs, source/evidence metadata and full escaped tooltips. Labels are visually shortened; canonical names/anchors remain in metadata. XML 1.0 forbidden display controls are replaced with U+FFFD and the sanitization is labeled. |
| JSON Canvas | `graph.canvas`; text nodes, source anchors and metadata, directional arrow ends and one edge per original observation. Source names/paths are Markdown-escaped so a source-authored image/link cannot become active Canvas markup. |
| Wiki | `index.md`, `nodes/<entity-id>.md` and actual Leiden `communities/<community-id>.md` articles. The community partition uses the complete validated graph; the article explicitly lists selected members only. It includes deterministic source-hub labels, cohesion and links to exact source cards. |
| Obsidian | Fresh source cards/index and `graph.canvas`. Hashed filenames avoid collisions and path injection. Wiki links connect source cards; human notes sections are provided. |
| Tree | `tree.html`; offline expandable file hierarchy, name/path filtering, source byte details and original directed relationship inspector. Source enters JSON/text nodes, never script/HTML execution. |
| Callflow | `callflow.html`; native offline SVG plus editable Mermaid flowchart source and the same source tree/inspector. Mermaid requires no remote renderer to view the native drawing. It preserves parallel directed arcs. |

HTML declares a restrictive Content Security Policy, has no remote scripts/fonts/resources and uses `textContent` for source-authored values. SVG escapes markup/attributes. Markdown cards neutralize source-authored formatting/link delimiters. Cypher interpolation is limited to generated IDs/fixed syntax and serialized values. These measures preserve the quoted source as data; they do not establish safety of arbitrary third-party viewers or graph database deployments.

The `native_graph_export_v1` record contains the immutable snapshot/workspace/graph identity, bundle artifact ID, format and options. The artifact's own `artifactId` is excluded from the hashed JSON to avoid a circular hash; it is returned after registration. Full bundle publication and record insertion share an immediate transaction and check the deadline before commit.

`write_new_bundle(&ExportBundle, &absolute_host_selected_destination, &Deadline)` is a separate host-authorized filesystem action. It verifies the sealed bundle, all output paths, case-folded collisions and byte limits before creating anything. The parent directory must already exist and all ancestors must be local nonlinked directories. Network paths, relative/parent traversal and arbitrary output names are refused.

Writing is create-only to a **new directory**. It never refreshes, replaces or deletes existing cards, managed notes or a vault. A second export to an existing directory fails before writes, preserving human changes. Re-export to a fresh directory and review the new generated cards. In-place managed-block updates require a separate future reviewable protocol.

The final `yoke-export-manifest.json` records source identities and exact per-file hashes/lengths. A filesystem error/deadline can leave a partial new directory without its final manifest; no completion is claimed, and no recursive cleanup or overwrite follows. This is bounded host file output, not a sandbox against a hostile process concurrently renaming parent directories.
