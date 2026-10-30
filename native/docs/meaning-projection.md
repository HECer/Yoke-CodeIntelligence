# Portable Meaning presentation projections

`meaning-projection` reads a host-selected serialized
`meaning_group_projection_v1` or `meaning_join_projection_v1`. The Rust API is
`meaning_projection::Projection::parse`, followed by `inspect` or `query`.
It does not need a source workspace, Store, registry or provider. It creates no
records, source files or overflow artifacts.

```powershell
yoke --max-bytes 4000000 meaning-projection --input C:/Host/projection.json
yoke --max-bytes 4000000 meaning-projection --input C:/Host/projection.json --query '{"operation":"degree"}'
```

The input must be an absolute, bounded host file with existing nonlinked
ancestors and no linked input. The reader reuses the archive reader's directory
guards and bounded reads. Global source, capture, snapshot, analyzer, language
server and write options refuse before reading the input. Nothing inside the
JSON selects a path to read, executable, provider, source workspace or permission.
In particular, evidence paths and receipt/provider descriptions remain data.

The command reports `meaning_portable_projection_result_v1`. Its top-level
`validation: projection_structure_only`, `bindingValidation: not_replayed`,
`sourceAuthority: false`, `providerAttestation: unverified`,
`semanticIdentityEstablished: false`, `originalConceptMergesApplied: 0`,
`modelCalls: 0` and `complete: false` describe this operation. Original metadata,
including any advertised replay/attestation/completeness claims, is nested under
`exportMetadata` and remains untrusted. It cannot replace these top-level fields.
Transport hashes and presentation identities are not signatures or proof of
captured-source membership, authenticated human review or semantic correctness.
The opaque `meaning_join::JoinedGraph` capability is never created by an import.

Admission rejects duplicate JSON keys, malformed UTF-8, unsupported schemas,
unknown fields in typed concept references and malformed node/edge/slot wrappers.
It verifies review, policy, projection and node identity recipes; disjoint and
complete membership of the supplied concepts; representatives and declared
groups; graph/packet qualification; unique original observation/evidence keys;
each projected directed endpoint against its original record; and every ordered
hyperedge slot against its original concept, role and position. All cited
evidence IDs must resolve within the same original graph/packet. Evidence text,
hash shape, relative path syntax and byte-length/range shape are checked without
reading or rehashing a source. Advertised statistics are retained only as claims;
reported inventory counts are recomputed from the supplied records.

This is structural consistency of supplied data. A coherent forgery, omitted
record or modified quote cannot be detected without independently trusted source
and receipt replay. Import does not verify complete parent closure, model or
execution truth, cost accounting or source bytes. Unknown envelope and original
record metadata is retained as passive data, not interpreted as new capabilities.

Queries use the existing strict join query schema:

```json
{"operation":"node","reference":{"graphId":"GRAPH_SHA256","packetIndex":0,"conceptId":"CONCEPT_SHA256"}}
{"operation":"degree"}
{"operation":"trace","roots":[{"graphId":"GRAPH_SHA256","packetIndex":0,"conceptId":"CONCEPT_SHA256"}],"direction":"outgoing","depth":1,"maxVisited":128}
```

Even a single-graph export uses graph-qualified query references. Equal labels
or concept IDs in another packet/graph never resolve interchangeably. Node
lookup and inspection retain whole original member records. Degree counts
parallel observations individually, selfloops once in each direction and all
ordered hyperedge slots. Trace follows directed relations only, with `incoming`,
`outgoing` or `both`, depth 0..8, at most 32 distinct roots and 4096 visited nodes.
Results contain induced relations, including selfloops at depth zero, and only
fully contained hyperedges. Repeated roles and projected endpoints stay distinct.
Every original evidence and packet/parent metadata record accompanies each
result under `exportMetadata`; this metadata may describe unselected records.

| Limit | Default and maximum |
| --- | ---: |
| `--max-input-bytes` | 4,000,000 |
| `--max-concepts` (original members before grouping) | 4,096 |
| `--max-relations` | 32,768 |
| `--max-hyperedges` | 8,192 |
| `--max-participants` (original ordered slots) | 65,536 |
| `--max-evidence` | 65,536 |
| Graphs / packets per graph | 8 / 64 |
| Members per projected node | 128 |
| Host query JSON | 16,384 bytes |

Limits are independent, include zero and refuse the whole request on overflow.
The library output limit is 0..4,000,000 bytes; zero refuses. CLI `--max-bytes`
is 1,024..4,000,000 including its newline. Metadata is retained even for small
selections, so a small query budget can still refuse a large export. No records
are clipped. Deadlines/cancellation apply through validation, iteration and
bounded serialization; parsing itself is one byte/depth/node-bounded step.

Mixed Office/PDF source spaces, cross-history/workspace joining, automatic
semantic identity/deduplication, outcome learning and complete upstream parity
remain separate work. Use checked native replay or complete archives when
source authority is required; this command does not upgrade an export to one.
