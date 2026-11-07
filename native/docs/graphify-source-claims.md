# Snapshot-bound declared source claims

This contract began with four public counterexamples at `90bd77e1`. Actual
Windows and Linux native runs compile and fail at the absent importer after
checking the real captured source/evidence premises. The producer implements
host admission, immutable View selection/archive replay and a separately
selected declared-claim path policy.

At source `0e17b87a23f630752e2db8a61995a87ed36371fe`,
[Windows partition 6](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/41239700108/artifacts/13056709360)
passes all twelve public source-claim cases. The
[Windows library in partition 3](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/41239700108/artifacts/13058180155)
passes 874 cases with seven original ignores, including actual input-sharing
refusal, raw JSON/numeric handling, original/corrupt storeless replay and the
new post-first-route cancellation fixture. The public target has no failures,
ignores or filtering. The independently audited full Windows receipt is
`86d741f376519a5bf5945d3586e79da014d58c7350bc6368c6b788ec3c262270`.
It binds all 1,182 selected source hashes and all eight exact target
partitions/logs: 379 default groups, 3,982 passes, zero failures and 53 original
ignores. This does not claim full host, transport, upstream, main or release
acceptance.

The prior exact `08e6acddb889e0d6567e68ac65d56a93760f2274` source has
complete default Windows evidence: 379 target groups, 3,978 passes, no failures
and 53 original ignores. Its separate host/browser proof must remain bound to
that source. The earlier four-case RED checkpoint and the parent
`c41b75b7466fe7a75297f067a045ad1f9c1ce337` category/prefix producer remain
historical evidence; this is no longer a tests-only implementation checkpoint.

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

## Implementation and qualification boundary

Admission reads only the explicitly selected bounded regular host file. The
reader opens without following links and holds physical identities for the file
and its local directory chain. Unix opens each child relative to its held parent;
Windows denies write/delete sharing while the selected chain is held. Reparse,
multiple-link, identity or name changes refuse before publication. It
compares held-file modification/change metadata and repeats the bounded read;
concurrent same-inode writes refuse rather than becoming a mixed document. It
requires an existing snapshot, checks every original observation and complete
evidence list, and validates the entire document before either CAS publication
or registration. Both artifact and scoped-registration conflicts are checked
before physical publication. As with existing Store publishers, an unexpected
physical IO/commit failure can leave an unregistered physical blob; it does
not become a published View.

The artifact retains the complete original document as a UTF-8 string. A local
lossless raw-JSON reader checks duplicate decoded keys and depth/node limits
without converting numeric tokens. It uses a local byte/depth/node/deadline-bounded
token reader; serde features and ordinary `Value` decoding remain unchanged.
Only JSON strings and exact metadata schemas use the existing decoder. Exact integer comparisons use sign,
decimal digits and magnitude; mixed integer/finite-binary64 comparisons use the
float's exact truncated integer plus its fractional part. `pathSourceScoreToken`
reports the original selected token as text, never a calibrated probability.

The ordinary View builder counts declared selections in its existing 29-ID
selection limit, 32-selection materialized limit and aggregate 32 MB input
budget. It adds descriptors only: nodes, observations and evidence are unchanged.
Loading, retention and self-contained archive replay revalidate retained input
against the original canonical scope. Queries read the admitted CAS artifact,
not the original host path. No Graphify MCP import tool is added.

Only `weakest_declared_source_claims_v1` uses these declarations. Its sorted
logical traversal (sorted primary arcs, then sorted implied reverse-contains arcs)
and backward shortest-DAG enumeration rank at most 256 distinct
shortest node paths, using a 257th only to disclose the cap. Original parallel
observations/evidence remain in the returned hops. Root direction markers may
reverse the bound original endpoints; a marker naming another endpoint is
retained as data at admission but refused by this policy. Nested copy markers
do not grant traversal authority. Stored/declared `contains` allows the implied
reverse hop; forward primary declarations take ranking preference over backward
ones. Complete-path ties retain prefix order, including weakest-hop position.
Existing category and first-observation policies keep their original code paths.

The original four public case bodies remain unchanged. Additional cases cover
file/schema/deadline refusals with complete publication-state comparisons, plus
category/invalid-score/large integer-float matrices through actual host admission
and public queries. Equal-rank and 256-prefix fixtures distinguish primary-arc
insertion from implied reverse arcs, using genuine admitted observations.
Authored units cover raw numeric retention, nested duplicate/depth/node refusals,
exact signed integer-float ordering, physical input pins, deterministic concurrent
mutation refusal and storeless original/corrupt archive replay. The Windows result above covers Windows-enabled cases; Unix-only name-pin and
mutation cases require their own source/platform evidence. No scoped result
establishes every transport or archive failure path.

## Initial fixtures and remaining requirements

The initial `native/tests/graphify_source_claims.rs` checkpoint authored four cases:

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
importer. In those RED runs, later numeric/reload/transport/refusal assertions
remain unreached; that first failure cannot qualify them.

The executed fixtures include finite/invalid-score matrices, category/score
conflicts, historical direction markers, directed/undirected routes and bounded
output/cancellation refusals. Storeless original/corrupt-input replay is a
library fixture; it does not replace adversarial complete archive and transport
qualification. Full current CLI/MCP/backend/upstream/Windows/main/release gates
remain required. These examples do not claim Graphify node-link JSON file
interoperability.

## Additional path guard fixtures

The public source-claim test target also contains these authored controls:

- A primary endpoint reversal excludes the weaker route in directed mode;
  explicitly undirected traversal restores it. Changing only the primary
  markers restores the directed route, while nested `also_seen` markers remain
  data. A primary marker naming a third genuine node refuses at query time
  without changing published snapshots, graphs, registrations or CAS blobs.
  Separate controls distinguish an insufficient hop limit, an unreachable
  function in a second captured file, an absent label and two genuinely
  ambiguous label matches. The containing-file reverse arcs remain intact.
- Real Unicode/CRLF source produces two parallel calls, an opposite call and a
  self-call. Both query directions retain every original hop observation and
  evidence object, while ranking uses forward declarations before backward
  ones. A self path has zero hops despite the stored self-call. Invalid hop
  bounds, an expired deadline and deterministically preset cancellation refuse
  without publication changes. This cancellation fixture does not establish
  interruption during path enumeration.
- A real source with 1,024 parallel calls gives one mandatory original hop
  whose observations and evidence alone exceed the existing 256,000-byte
  response ceiling. Admission must first succeed within its own input limit;
  the omitted-policy query must still return its original one-observation
  result. Selecting the declared policy must then refuse the whole response
  rather than drop original evidence. Source bytes, latest snapshot and all
  published state stay intact.

All three additional public fixtures pass at the named `0e17b87a` source
above. Their passing assertions do not establish full transport, archive,
cancellation, resource or release acceptance.

A separate authored library fixture covers cancellation during enumeration.
It imports declarations for the real 111-byte Unicode/CRLF source, publishes an
ordinary View and first proves that all three complete shortest routes are
visited, including the implied containing-file route.
A test-build-only thread-local probe then sets the inherited host cancellation
flag immediately after the first complete route is reached. The ordinary
deadline checks must refuse before visiting the second route. The probe is
restored on scope exit; production builds contain neither its state nor its
call. Every snapshot, graph, registration, CAS blob, latest selection and source
byte must remain unchanged. This fixture passes in the named `0e17b87a`
Windows library. It does not qualify transport-driven cancellation or arbitrary
scheduler/interruption behavior.
