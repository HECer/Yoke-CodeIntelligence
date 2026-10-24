# Explicit host-selected meaning groups

`meaning group --review HOST_JSON` projects explicitly selected concept groups
over one published native meaning graph. The library entry point is
`meaning_groups::project`. Before using any submitted reference it replays the
complete parent graph, captured source and original job/cache/response closure.
It starts no provider, captures no current files, writes no records or artifacts,
and leaves the parent graph unchanged.

```powershell
yoke --root C:/Work/project meaning load GRAPH_ID
yoke --root C:/Work/project meaning aliases GRAPH_ID
yoke --root C:/Work/project --max-bytes 4000000 meaning group --review C:/Host/groups.json
```

The strict review file has the following shape. Replace the placeholder IDs with
the exact lowercase 64-character identities returned by `meaning load` or
`meaning aliases`. Concept labels are not identities.

```json
{
  "schemaVersion": 1,
  "graphId": "GRAPH_ID",
  "groups": [
    {
      "members": [
        { "packetIndex": 0, "conceptId": "CONCEPT_A_ID" },
        { "packetIndex": 1, "conceptId": "CONCEPT_B_ID" }
      ],
      "representative": { "packetIndex": 0, "conceptId": "CONCEPT_A_ID" },
      "rationale": "The host selects these two claims as one presentation group."
    }
  ]
}
```

Every group contains 2 through 128 distinct original references and selects one
of them as its representative. Membership is disjoint both within and across
groups. Missing references, overlapping groups, nonmember representatives and
ambiguous/unknown JSON fields refuse the whole request. An empty group list
retains every original concept as an independent singleton. Equal labels,
provider-supplied aliases and lexical scores never select a group automatically.

The file is an explicit host assertion. Its selection and rationale are not
authenticated human review or independent semantic verification. Every original
concept remains an inferred claim with its original provider attestation.

## Preservation and topology

The `meaning_group_projection_v1` result contains every original concept exactly
once, grouped or as a singleton. Each node retains its packet-qualified original
member references and unchanged concept records. The representative chooses an
original claim for presentation; it does not rewrite any member's label,
description, aliases or evidence.

Every directed relation retains its original packet-qualified relation ID and
complete record, together with projected source/target node IDs. Parallel
observations remain separate. A relation whose endpoints project to the same
node remains an explicit selfloop.

Every hyperedge retains its original packet-qualified ID and complete record.
Each projected participant slot records its original position, role,
packet-qualified concept reference and projected node ID. Slots retain their
original order, including repeated projected endpoints. This is a separate
projection schema, not an ordinary MeaningGraph with its distinct-participant
constraint. The projection never drops roles to satisfy that constraint.

All original evidence records remain packet-qualified and unchanged, including
quotes, UTF-8 byte offsets, paths and content hashes. Packet source bindings,
coverage, input/contract identities and original response/accounting provenance
are retained. Those cost/usage values remain provider reports, not independent
attestations. Output and record ordering is deterministic; hyperedge slot order
is preserved.

## Admission

| Option | Default | Maximum |
|---|---:|---:|
| `--max-groups` | 256 | 512 |
| `--max-members` (total submitted references) | 4,096 | 4,096 |
| `--max-concepts` (original concepts) | 1,024 | 4,096 |
| `--max-relations` (all original observations) | 8,192 | 32,768 |
| `--max-hyperedges` | 2,048 | 8,192 |
| `--max-participants` (all original slots) | 16,384 | 65,536 |
| `--max-evidence` (original records) | 16,384 | 65,536 |
| Global `--max-bytes` | Existing CLI default | 4,000,000 including newline |

Every library capacity is independent and zero is a real capacity, including an
output capacity of zero. The CLI retains its existing 1,024-byte minimum including
the newline. Counts apply before
grouping: three original concepts still require capacity three when their
projection has only two nodes. Repeated projected endpoints do not reduce
relation, hyperedge-slot or evidence charges. No request returns clipped output.

The CLI review file and the library's compact serialized review are each bounded
to 128,000 bytes. Each nonblank rationale is at most 2,048 UTF-8 bytes. Newline,
carriage return and tab are permitted; other control characters refuse. Text is
not Unicode-normalized. JSON escaping can consume additional review bytes.

Original proof values remain borrowed while the complete output is counted;
only admitted output is serialized and decoded. Deadlines and host cancellation
apply through validation, traversal, hashing, serialization and final decoding.
These projection limits do not redesign the parent's separate replay/allocation
limits or turn replay into a streaming loader.

The command follows existing meaning read admission: it requires an initialized
store and refuses capture, global snapshot, analyzer, language-server and source
write configuration. Original live files and the provider executable can be
absent. Oversized output creates no overflow artifact or persistent projection.

## Logical identities

Members sort by `(packetIndex, conceptId)`. Groups sort lexicographically by
their sorted member arrays. Representative and rationale remain verbatim.
For these new versioned identities, JSON is compact UTF-8 from the declared
serde schema, with no whitespace. Review object field order is `schemaVersion`,
`graphId`, `groups`; group field order is `members`, `representative`, `rationale`;
reference field order is `packetIndex`, `conceptId`. These are fixed schema
recipes, not RFC 8785 canonicalization.

- `policyHash` is SHA-256 of JSON
  `["meaning-group-policy-v1", canonicalReview]`.
- A node ID is SHA-256 of JSON
  `["meaning-group-node-v1", graphId, sortedOriginalMemberReferences]`.
  A singleton uses the same recipe with one member.
- `projectionId` is SHA-256 of JSON
  `["meaning-group-projection-v1", graphId, policyHash]`.

Representative and rationale affect policy/projection identity. Node identity
depends on original membership. Admission and transport capacities affect none
of these logical identities. They are not signatures or hashes of every output
transport byte. Existing parent/source/claim identities retain their recipes.

`groupingAuthority: explicit_host_assertion`,
`bindingValidation: exact_captured_source_replayed`,
`semanticIdentityEstablished: false`, `providerAttestation: unverified`,
`originalConceptMergesApplied: 0`, `modelCalls: 0` and `complete: false` remain
explicit. Automatic semantic identity/deduplication, authenticated review,
cross-graph matching, provider-assisted decisions and generic projection
consumers with independent source authority remain separate work. The separate
[portable projection consumer](meaning-projection.md) reads native group/join
exports as untrusted data with explicit structural validation. It does not
perform or inherit the parent/source replay described here.
