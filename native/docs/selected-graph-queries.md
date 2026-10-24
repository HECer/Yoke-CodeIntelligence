# Select an immutable graph view through the eight native tools

`ci_find`, `ci_trace`, `ci_context` and source `ci_read` accept an explicit
`graphViewId`. It selects an already published, locally registered graph view.
Selection uses the same checked view implementation as `graph view` CLI queries;
the native MCP profile still exposes exactly eight tool names.

Build a view through the host CLI/library first. For example, a default view can
combine canonical code symbols with captured manifest dependencies and Markdown
citations. Host-published semantic overlays can be included when building it.
Query selection cannot create overlays, start a language server, call a model,
execute repository code, access credentials or fetch network content.

```json
{"graphViewId":"<published-view-id>","query":"example_app","kind":"package"}
```

Use this argument with `ci_find`. Selected find accepts only `query`, `kind`,
`path` and `limit`, plus the shared response budget, timeout and task fields.
`query` is a nonempty name substring of at most 1,000 UTF-8 bytes. Matching is
case insensitive, with exact case-sensitive name matches ordered first. `kind`
matches an exact view node kind such as `package`, `document` or `function`;
omitting it searches all selected node kinds. `path` matches an exact captured
path. Canonical query modes `symbol`, `all`, `text` and `semantic`, symbol-search
options, vector fields and `languageServer` cannot be combined with a view.

`ci_trace` accepts the ordinary entity IDs, direction, depth, relation,
resolution and visit limits. IDs may refer to canonical or selected overlay
nodes. Directed and parallel observations retain their IDs, evidence,
resolution and producer. Semantic provider claims retain their unverified
provider attestation; selection does not turn them into compiler proof.
The visit limit includes distinct initial entities; an initial selection larger
than `maxVisited` fails explicitly. Repeated IDs count once.

Selected `ci_context` accepts `query`, exact `kind`/`path` filters, `limit`
(1–20), `depth` (0–8) and `maxSourceBytes` (100–256,000). It returns selected
nodes, their neighborhood and exact captured source excerpts. Existing
environment-value omissions, UTF-8 bounds and truncation reporting apply.
`sourceTruncated` marks source-budget reductions, including whole selected
sources recorded with the `source_budget_exhausted` omission reason. Overall
`truncated` also reflects omitted search results and bounded neighborhoods.
Canonical context plans and `includeMemory` cannot be combined with a view.

Selected source `ci_read` requires exactly one `entityId` or captured `path`,
with optional byte or line ranges. A selected node's sole validated source
binding is the default range; the original `declarationAnchor` accompanies the
response so a file-level anchor stays file-level. Explicit ranges read exact
bytes from the same captured file. A node with several bindings fails with an
ambiguity error; use an explicit path and range in that snapshot. Artifact
paging uses the ordinary `ci_read artifactId` interface without `graphViewId`.

The view supplies its immutable snapshot, so no query implicitly indexes live
files. An optional `snapshotId` must match the view's snapshot exactly. Historic
views continue reading their CAS bytes after live files change or a newer
snapshot is indexed. Unknown IDs, forged registry scope, unavailable artifacts,
unknown options and incompatible canonical operations fail explicitly.

Successful selected results contain `graphViewId` and a `graphView` descriptor
with workspace, snapshot, base graph hash, view artifact, selected overlays,
canonical coverage and limitations. `complete` remains false: successful
retrieval cannot establish exhaustive code, document or provider coverage.
The canonical stored graph remains unchanged. Results exceeding the requested
response budget retain the complete selected result and its lineage in an
immutable artifact, readable with ordinary artifact paging.

Without `graphViewId`, the existing native query behavior and defaults apply.
Status, prepare, apply and verify do not accept this selector. Discovery through
a view never grants source-write authority; changes require canonical source
hashes and the existing plan, approval and recovery workflow.
