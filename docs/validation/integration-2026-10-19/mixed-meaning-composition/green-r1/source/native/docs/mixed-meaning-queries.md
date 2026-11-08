# Checked mixed interpretation graph queries

[Explicit composition](mixed-meaning-composition.md) also supports multiple
independently checked parents with graph-qualified concepts and separately
identified, opt-in host assertions. Original evidence and local keys remain.

`mixed_view::provider::load_checked` performs the same source/response/ledger
replay and byte comparison as `provider::load`. Its opaque `CheckedGraph` owns
the checked native/Office/PDF source view and immutable verified graph data.
It has no JSON constructor. `export` returns the existing load representation;
legacy graph identities and stored artifact bytes are unchanged.

After loading, `CheckedGraph::query` uses only owned data, including after Store,
CAS, live source and provider files disappear. Loading a graph with PDF support
still requires a separately supplied native PDF worker. Queries start neither
a provider nor a new PDF process, capture source, persist records or write source.
A frozen capability describes its original checked selection; it does not claim
that a later live workspace or mutable metadata still matches that selection.

```sh
yoke --root /project --timeout 60000 --max-bytes 4000000 \
  mixed-meaning query JOB_ID --query query.json --native-pdf-text
```

Omit `--native-pdf-text` for Office-only selections. The query file is bounded to
64000 bytes and decoded with duplicate-key and unknown-field refusal before
opening the Store. No provider configuration or provider trust flag is needed.
The default eight MCP tools and their schemas remain unchanged.

Closed query shapes (replace `GRAPH_ID` with the loaded graph's 64-hex `id`):

```json
{"operation":"node","reference":{"graphId":"GRAPH_ID","key":"a"}}
{"operation":"find","query":"Storage 🌍"}
{"operation":"degree"}
{"operation":"trace","roots":[{"graphId":"GRAPH_ID","key":"a"}],"direction":"outgoing","depth":2,"maxVisited":128}
```

Find matches a case-sensitive literal substring of original concept labels and
aliases, with no normalization, semantic ranking or concept merging. Query text
is nonempty, at most 1024 UTF-8 bytes, without control characters. Concept keys
retain their provider-local namespace; every reference binds the exact graph
identity. Missing keys and foreign graph identities refuse the request.

Node/find return selected concepts and incident original relations/hyperedges.
Degree selects all concepts, counts each parallel directed relation separately,
counts a self-loop once in each incoming/outgoing degree, and counts each ordered
hyperedge participant slot, including repeated concepts in different roles.
All rows preserve original array indices and claim data. Hyperedge participant
order and roles are unchanged.

Trace follows only original directed relation observations, with `outgoing`,
`incoming` or `both` direction. It accepts 1..32 distinct roots, depth 0..8 and
maxVisited 1..128. Depth 0 selects only roots and traverses no relations. Returned
relations are the original indexed observations encountered from a frontier
within the requested depth; they are not an induced subgraph. Incident hyperedges
are included as context, but their role labels never create traversal direction.
Exceeding maxVisited refuses the entire result rather than returning a clipped
apparently complete traversal.

Every answer nests the exact original graph, including source-validation inputs,
typed support quotations, receipt/accounting, ordered contexts and original
fragment. `providerCallsThisRead=0` describes this query; nested
`modelCallsThisRun=1` retains its explicit original build scope. Output admission
is 1..64000000 bytes for the library; the CLI requires 1024..64000000 including its
newline. The complete provenance must fit; no evidence is silently omitted.
All operations share the caller's absolute deadline. Errors return no partial
query result and grant no source/execution/semantic authority.

`selectionComplete=true` means the requested literal selection over this finite
checked graph was returned without clipping. Top-level `complete=false` and
`canonicalSourceAuthority=false` remain: matching source support does not prove
interpretation truth, exhaustive coverage, alias identity, compiler calls,
runtime dispatch or model/provider attestation. Cross-graph/workspace joins,
broader analytics and external semantic-provider qualification remain separate.
