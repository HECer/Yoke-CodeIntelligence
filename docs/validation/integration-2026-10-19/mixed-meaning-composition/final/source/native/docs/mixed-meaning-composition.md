# Checked mixed graph composition

`mixed_view::provider::composition::compose` consumes 2..4 opaque `CheckedGraph`
parents and one closed, host-authored `Review`. Each parent must first be loaded
through actual source/response replay. Parents may come from different immutable
snapshots or workspaces, with each original source scope independently checked.
The composition owns those frozen capabilities. JSON cannot construct it.

Every concept retains `{graphId,key}`. Original relations and hyperedges retain
`{graphId,index}` observation references, preserving parallel and reverse rows,
self-loops and ordered repeated participant roles. Labels and aliases never
merge concepts. The full original graphs remain nested, including native/Office
XML/decoded/PDF coordinates, exact quotations, original receipts and accounting.

The review is explicit data, with this shape (substitute actual parent graph IDs):

```json
{
  "schemaVersion": 1,
  "graphIds": ["GRAPH_A", "GRAPH_B"],
  "links": [{
    "source": {"graphId": "GRAPH_A", "key": "a"},
    "target": {"graphId": "GRAPH_B", "key": "c"},
    "relation": "candidate",
    "rationale": "Explicit host-authored candidate connection"
  }]
}
```

Ordered graph IDs must exactly match actual supplied parents and be distinct.
Every endpoint must resolve to an original concept. A review admits at most 256
links and 128000 encoded bytes. Relation labels use 1..80 lowercase ASCII letters,
digits, hyphen or underscore; rationales are nonempty, at most 1024 UTF-8 bytes,
without control characters or obvious secrets. Unknown fields refuse. Repeated
identical links are separate authored observations with original review indices.

Links are `host_asserted_unverified_relation`, with attestation unverified. They
are separate from original provider relation observations, not quoted source
facts or reviewed semantic identity. Neither kind proves runtime/compiler calls.
Composition identity binds the entire ordered review and its pinned parent IDs;
changing link order or rationale changes identity. No graph is persisted.

The current CLI selects jobs from its single explicit `--root` Store:

```sh
yoke --root /project --timeout 60000 --max-bytes 64000000 mixed-meaning compose \
  --job-id JOB_A --job-id JOB_B --review review.json --native-pdf-text
```

Use job IDs from build/jobs, and parent graph IDs from `mixed-meaning load` for
the review. The CLI requires 2..4 distinct job IDs. It checks bounded review/query
shape before opening the Store; its host JSON files each have a 64000-byte bound.
Each selected job is actually replayed. A PDF parent requires the separate
`--native-pdf-text` flag; an Office parent receives no PDF worker, including when
another parent in the same command needs it. No provider configuration or model
trust flag is required. Cross-workspace library composition requires separately
loaded host-supplied capabilities; a review cannot select another Store/root.

Optional `--query query.json` uses the same closed node/find/degree/trace shapes
as [single-graph queries](mixed-meaning-queries.md), with original parent concept
references. Add `--include-host-links` explicitly to select/count/traverse authored
links. It requires a query and defaults off. A plain export displays all review
links without performing traversal. The review always remains in provenance.

Degree rows distinguish `originalRelationInDegree`/`originalRelationOutDegree`
from `hostLinkInDegree`/`hostLinkOutDegree` and count ordered hyperedge slots
separately. Excluded host link counts are zero under `includeHostLinks=false`.
Both sets count parallel rows independently and self-loops in both directions.
Trace honors outgoing/incoming/both direction and depth 0..8; it follows authored
links only when explicitly included. It accepts 1..32 distinct parent-qualified
roots and maxVisited 1..128 across the entire composition. Hyperedge roles create
no flow. Original/review row order and evidence remain unchanged. Capacity
overflow refuses the whole result without a clipped complete traversal.

`max_input_bytes` admits 1..64000000 bytes for the complete original-graph JSON
array; CLI `--max-input-bytes` defaults to 64000000. Library output admission is
1..64000000 bytes; CLI output requires 1024..64000000 including the newline. Full
provenance must fit. All work shares the original absolute deadline. Once loaded,
composition/query never accesses a Store/CAS, starts a provider/PDF process,
captures source or writes source/records, including after all parent Stores are
removed. Native default eight-tool MCP schemas are unchanged.

`providerCallsThisRead=0` describes this operation; nested original build counts
retain their own scope. `selectionComplete=true` covers the finite requested
selection without clipping. `complete`, `canonicalSourceAuthority` and
`semanticIdentityEstablished` remain false. Frozen checked bytes do not prove
current live workspace applicability, host/provider attestation, exhaustive
document/layout/OCR interpretation, model accuracy or broader upstream parity.
