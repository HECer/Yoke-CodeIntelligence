# Snapshot-bound declared source claims: test-first contract

This is an authored admission contract and four public counterexamples.
The importer, View selection and declared-claim policy are **not implemented
at this checkpoint**. These tests have not yet run natively. Their presence
does not establish numeric behavior or full Graphify compatibility.

The selected-category/prefix producer remains published separately at
`c41b75b7466fe7a75297f067a045ad1f9c1ce337`. This branch adds tests and this
document only while that producer finishes its source-pinned qualification.

## Host input and authority

The host explicitly selects one already captured snapshot and a UTF-8 document:

```text
yoke --root <registered-project> --snapshot <snapshot-id> graph import-source-claims --input <host-document.json>
```

The input is data. It grants no project execution, model/provider call, network
access or additional file read. Document paths/attributes do not authorize
another host input. Import requires the explicit existing snapshot. Complete
source/scope/schema/capacity validation precedes CAS publication and registration.
The initial input ceiling is 1,000,000 bytes under the existing caller deadline.

Exactly these fields describe the input:

```json
{
  "schemaVersion": 1,
  "snapshotId": "<captured-snapshot-id>",
  "workspaceId": "<captured-workspace-id>",
  "baseGraphHash": "<canonical-graph-hash>",
  "claims": [
    {
      "observationId": "<original-observation-id>",
      "source": "<original-source-node-id>",
      "target": "<original-target-node-id-or-null>",
      "evidenceIds": ["<original-evidence-id>"],
      "attributes": {
        "confidence": "INFERRED",
        "confidence_score": 0.9,
        "also_seen": [
          {"confidence": "INFERRED", "confidence_score": 0.2},
          false,
          {"confidence": "INFERRED", "confidence_score": true}
        ]
      }
    }
  ]
}
```

The Store, immutable snapshot and canonical graph must agree with all three
scope pins. Each original observation, endpoint and complete evidence-ID list
must match that scope, including existing source-hash/range validation.
An ID-shaped string does not establish membership. Unknown root/claim fields,
duplicate JSON keys, foreign/altered scope and a non-object primary attributes
value refuse without registration or a new snapshot.

`attributes` retains unknown keys and every `also_seen` entry. Multiple
declarations attached to the same observation remain separate ordered claims.
They do not replace canonical observations, discard contrary copies, alter
source evidence or establish confidence as true. Dictionary-valued `also_seen`
copies participate in claim interpretation; other retained entries do not.

The receipt includes `artifactId`, `sourceDocumentHash`, `snapshotId`,
`workspaceId` and `baseGraphHash`. The immutable artifact retains
`sourceDocumentHash` and the complete original UTF-8 `sourceDocument`.
Original numeric lexemes, whitespace and CRLF remain authoritative. Identical
admission is content-addressed; lossy numeric conversion before retention is
insufficient.

## Explicit View selection and replay

The ordinary host View builder selects already admitted IDs:

```json
{"structuralKinds":[],"sourceClaimOverlayIds":["<admitted-artifact-id>"]}
```

Inputs use normal View build/load/publication and immutable archive validation.
They count toward existing aggregate input, selection and output capacities.
Nodes, canonical observations and evidence remain unchanged. A selected claim
overlay binds the retained input so validation can reconstruct the materialized
View. Reloading uses its original snapshot/document after host input deletion
or change and after a newer source is indexed. Empty/omitted selection preserves
existing serialization and identities.

MCP callers may select a published View using `graphViewId`. Import remains a
host operation, rather than an additional Graphify MCP tool or an analyzed-data
path to host file access.

## Declared-claim selection

The authored request selects `weakest_declared_source_claims_v1`. Omitted and
first-observation behavior and `weakest_source_categories_v1` keep their
separate contracts and qualification.

The retained primary consumer at
[serve.py, a0946f96](https://github.com/Graphify-Labs/graphify/blob/a0946f962af31b19bff941b453f6dae9a7077c7a/graphify/serve.py)
and copy expansion at
[build.py, a0946f96](https://github.com/Graphify-Labs/graphify/blob/a0946f962af31b19bff941b453f6dae9a7077c7a/graphify/build.py)
require genuine admitted claims, not forged private View fields:

- All retained dictionary copies contribute independently. Recognized categories
  order before stored numeric scores; a smaller finite score is weaker within
  one category.
- Boolean, string, absent and nonfinite values supply no numeric score. Missing
  scores do not become an invented weaker zero. `EXTRACTED` ignores its score.
  Unknown categories stay unknown regardless of their stored number.
- Integer ordering remains exact above binary64 and u64 boundaries. Floating
  values compare as admitted finite binary64 values, and negative zero ties
  with zero. The original numeric tokens remain retained source data.
- Full current qualification also includes sorted traversal, historical direction
  markers, implied reverse traversal of stored `contains` edges, forward-claim
  preference when both directions exist, and the bounded first-256 distinct
  shortest-node-path prefix. The initial admission tests do **not** establish
  those traversal requirements.

The selected result retains original route observation IDs and complete original
hop evidence, explicit policy/category meaning, the caller deadline and
whole-response refusal. Declarations do not become calibrated probabilities,
compiler binding or observed execution.

## Initial fixtures and remaining requirements

`native/tests/graphify_source_claims.rs` authors four cases:

1. Actual host-CLI import and ordinary host-CLI View selection. A contradictory
   retained `also_seen` score changes the two-hop source route. Original
   observation/evidence objects and captured bytes survive reload after both
   host input and current source change. A paired control changes only the
   nested numeric copy and must select the opposite route. Both selected
   Views continue to return their own immutable result.
2. Adjacent positive integer scores above u64 choose the exact smaller value.
   The test inserts original number tokens without a test-side f64 conversion,
   then swaps those tokens and requires the opposite route on the same graph.
   Neither fixed traversal ties nor a lossy equal-score comparison can satisfy
   both controls.
3. Actual Graphify MCP transport selects the View after input-file removal;
   its exact ten existing tool names expose no host import operation.
4. A successful admission control precedes malformed/foreign-scope cases;
   rejected documents preserve every snapshot/graph/registration row and every
   private fixture CAS blob. Cases include other genuine same-scope observation,
   endpoint and evidence IDs, not only nonexistent hash-shaped strings.

The real source is 111 bytes with SHA-256
`1cd640678892e21fb431a19d99b5bff3ce1b5c64b3e0828dd080169acacd4300`.
Before import, each fixture checks four real call observations and their
captured UTF-8/CRLF hashes/ranges. The intended first RED is the absent public
importer. Later numeric/reload/transport/refusal assertions remain unreached
until admission exists; that first failure cannot qualify them.

Producer and complete archive replay implementation, actual causal RED/GREEN,
finite/invalid-score matrices, category/score conflicts, historical markers,
directed/undirected behavior, resource/cancellation/output refusals and full
current CLI/MCP/backend/upstream/Windows/main/release gates remain required.
These four examples do not replace that scope or claim Graphify node-link JSON
file interoperability.
