# Checked meaning joins

`meaning join` presents explicitly selected concept groups across published
native meaning graphs. Every selection must share the exact workspace,
canonical snapshot and base graph hash supplied in the host review. It replays
the joint stored dependency closure, including original provider receipts and
captured source bytes, before producing output. Current files and provider
executables are unnecessary. It starts no provider and writes no graph, review,
cache, accounting record or overflow artifact.

```powershell
yoke --root C:/Work/project --max-bytes 4000000 meaning join --review C:/Host/join.json
yoke --root C:/Work/project meaning join --review C:/Host/join.json --query '{"operation":"degree"}'
```

The strict review schema is:

```json
{
  "schemaVersion": 1,
  "workspaceId": "WORKSPACE_SHA256",
  "snapshotId": "SNAPSHOT_SHA256",
  "baseGraphHash": "BASE_GRAPH_SHA256",
  "graphIds": ["FIRST_MEANING_GRAPH_SHA256", "SECOND_MEANING_GRAPH_SHA256"],
  "groups": [{
    "members": [
      {"graphId": "FIRST_MEANING_GRAPH_SHA256", "packetIndex": 0, "conceptId": "FIRST_CONCEPT_SHA256"},
      {"graphId": "SECOND_MEANING_GRAPH_SHA256", "packetIndex": 0, "conceptId": "SECOND_CONCEPT_SHA256"}
    ],
    "representative": {"graphId": "FIRST_MEANING_GRAPH_SHA256", "packetIndex": 0, "conceptId": "FIRST_CONCEPT_SHA256"},
    "rationale": "Explicit host presentation decision"
  }]
}
```

Replace placeholders with stored 64-character lowercase SHA-256 identities.
Groups contain 2 through 128 distinct references, are disjoint and name one of
their members as representative. Rationale is nonempty and at most 2048 UTF-8
bytes. The review is bounded to 128,000 bytes. `groups: []` keeps every original
concept as a singleton, including equal labels. No lexical match, semantic
equivalence or authenticated review is inferred.

Each node keeps all original graph/packet-qualified concept records. Directed
relations retain their original identities and records, including parallel
observations and selfloops. Each hyperedge retains every original ordered role
slot; repeated roles and concepts in different roles remain distinct. Evidence,
packet bindings, coverage, input/contract hashes and receipt provenance remain
qualified by their parent graph and packet. Parent proofs retain artifact,
model, analyzer and capture-policy identities. Interpretation and coverage
claims remain the original provider's unverified claims.

Graph selection, group and member order are canonicalized. Policy identity is
SHA-256 of JSON `["meaning-join-policy-v1", canonicalReview]`. Node identity is
SHA-256 of JSON `["meaning-join-node-v1", sortedOriginalMemberReferences]`;
singletons use a one-member array. Projection identity is SHA-256 of JSON
`["meaning-join-projection-v1", policyHash]`. Rationale and representative affect
policy/projection identity. Capacities affect none of these identities. These
hashes provide identities, not signatures or reviewer authentication.

Hash input uses compact UTF-8 JSON without whitespace. Review fields serialize
in `schemaVersion`, `workspaceId`, `snapshotId`, `baseGraphHash`, `graphIds`,
`groups` order; group fields in `members`, `representative`, `rationale` order;
reference fields in `graphId`, `packetIndex`, `conceptId` order. References sort
by graph ID, numeric packet index, then concept ID; groups sort by their sorted
member arrays. Strings follow serde_json escaping: Unicode remains UTF-8,
quotes/backslashes and control characters are escaped. No Unicode normalization
occurs. Graph IDs sort lexically. Arrays retain the canonical order described.

Optional strict queries use these JSON forms:

```json
{"operation":"node","reference":{"graphId":"GRAPH_SHA256","packetIndex":0,"conceptId":"CONCEPT_SHA256"}}
{"operation":"degree"}
{"operation":"trace","roots":[{"graphId":"GRAPH_SHA256","packetIndex":0,"conceptId":"CONCEPT_SHA256"}],"direction":"outgoing","depth":1,"maxVisited":128}
```

Node lookup resolves any original member to its projected node. Degree counts
each directed observation separately; a selfloop contributes once to both
incoming and outgoing degree. Hyperedge degree counts original participant
slots. Trace accepts `outgoing`, `incoming` or `both`, at most 32 distinct roots,
depth 0 through 8 and at most 4096 visited nodes. It traverses directed relations
only. Results include all induced relations on reached nodes, including
selfloops at depth zero, and fully contained hyperedges with unchanged slots.
Parent/evidence metadata and explicit selection semantics accompany results.
Exceeding `maxVisited` refuses the entire request. Serialized projection JSON
cannot be supplied as a query capability; the Rust API requires the opaque
`JoinedGraph` returned by checked replay.

| Capacity | Default | Maximum |
| --- | ---: | ---: |
| `--max-graphs` | 4 | 8 |
| `--max-groups` | 256 | 512 |
| `--max-members` | 4096 | 4096 |
| `--max-concepts` | 1024 | 4096 |
| `--max-relations` | 8192 | 32768 |
| `--max-hyperedges` | 2048 | 8192 |
| `--max-participants` | 16384 | 65536 |
| `--max-evidence` | 16384 | 65536 |
| `--max-input-bytes` | 32000000 | 32000000 |

Counts apply to all original selected records before grouping. Input admission
covers the complete deduplicated stored record/artifact closure; captured
sources retain the joint loader's independent 64,000,000-byte bound. The initial
canonical scope is length-admitted before materialization. Output is bounded to
4,000,000 bytes. CLI `--max-bytes` includes the final newline; queries additionally
require their checked internal projection to fit within 4,000,000 bytes. A
smaller query transport budget never permits unbounded internal materialization.
Deadline, input, parent and capacity refusals produce no partial result or saved
overflow artifact.

Read commands require an existing initialized store and reject global capture,
snapshot, analyzer and language-server execution options. Groups have
`groupingAuthority: explicit_host_assertion`, `semanticIdentityEstablished:
false`, `providerAttestation: unverified`, `originalConceptMergesApplied: 0`,
`modelCalls: 0` and `complete: false`. Automatic semantic deduplication,
cross-history/workspace joins, mixed-document source spaces and generic
serialized projection imports remain outside this command's contract.
