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

## Ranked traversal and explanations

~~~sh
yoke --root /project graph view VIEW_ID query --args '{"query":"what calls processInvoice","mode":"dfs","direction":"in"}'
yoke --root /project graph view VIEW_ID explain --args '{"seeds":["src/invoice.ts"],"contexts":["call","import"]}'
~~~

These commands open a published View in the existing store read-only. Choose
either a text query or a list of exact selectors. A selector matches an entity
ID, exact name, qualified name, or captured path of a file node. Unresolved
and ambiguous selectors report candidate counts. A text query ranks normalized
name, qualified-name and path matches with deterministic ties. The report
returns each ranked seed's score and matched terms.

Both commands accept BFS and DFS, inbound, outbound and bidirectional
traversal, depths from zero to eight, and a visit cap. Query defaults to three
seeds and three hops. Explain defaults to one ranked seed and one hop. Explicit
seeds keep their supplied order. Visit explanations identify an original
observation and a predecessor. A later shorter route can improve an explanation
and expand the remaining depth.

Use `contexts`, `relations` or `resolutions` to restrict the neighborhood. Contexts
such as call, import and reference derive from native relation labels. Without
an explicit context list, recognized intent words in a query can select a
context. An explicit empty list disables inference. The report records the
selected filters and their origin.

The result keeps parallel and opposite observations between selected endpoints,
their original source evidence and the selected snapshot ID. A directed edge
retains its stored direction even when visited from the target. Meaning
incidences and source citations are neutral and available for bidirectional
traversal. The result keeps their original claim IDs and participant records.
Source notes and provider claims retain their original trust attributes.

[Graphify query reference](https://github.com/Graphify-Labs/graphify/blob/fe085cedddde3a18554f8749b230bfe47f3dcce7/graphify/serve.py)
