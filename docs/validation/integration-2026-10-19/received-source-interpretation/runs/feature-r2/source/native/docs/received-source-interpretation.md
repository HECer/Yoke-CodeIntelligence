# Source-bound received-digest interpretation

The separate host-local `receiver::interpretation` contract binds existing local
model inferences to [independently checked received declarations](received-source-verification.md).
It replays published native Meaning graphs, their exact local sources, canonical
graphs, capture/analyzer lineage, immutable provider responses, cache origins and
accounting. It never accepts an imported graph/report as a checked capability,
resolves a provider, starts a model or writes a record.

First, the host independently captures the local source and explicitly builds a
Meaning graph using the existing [trusted build workflow](meaning-cli.md).
Select `{"bodies":["<local graph entity ID>","<another local graph entity ID>"]}`
in that workflow's host policy to submit exact declaration units. IDs come from
the selected local capture, for example `localSource.graphEntityId` in a verified
report. They are never inferred from sender IDs or labels. Context groups may
select those same local keys. The original build owns explicit provider trust,
attempt reservations, source disclosure, cost limits and durable publication.
This receiver read has no provider configuration or automatic build fallback.

```sh
yoke --root /host/receiver-state --max-bytes 16000000 digest-inbox interpret \
  --profile /host/receiver.json --principal sender \
  --request /host/interpretation.json \
  --source-root /host/independently-captured-project \
  --source-profile /host/app-scope.json \
  --allow-query --allow-source-verification --allow-source-interpretation
```

The closed request contains `schemaVersion:1`, `verification` (the complete
closed verification request, including exact receipt and local snapshot pins),
and `meaningGraphIds` (one to four distinct actual local published graph IDs).
Duplicate/unknown fields or duplicate selections refuse. These host selections
are not added to the sender protocol. Current receiver and source App grants,
both verification permissions and the extra interpretation permission are checked
before existing-only read-only Store access. That extra permission explicitly
allows local source quotations and provider-authored descriptions in stdout.
Ordinary receipt intake, metadata queries and declaration verification still
exclude implementation text and perform no interpretation processing.

Every submitted unit in every selected packet must have the exact local path
and whole-file hash of a verified declaration, and its complete UTF-8 byte range
must fit within that declaration's matched full fingerprint span. The check covers
all context packets and successful `no_claims` units, including parts not quoted
by any inference. A full-file unit with unmatched trailing private code refuses
even if its claims cite only a matching public header. Partial quotes cannot grant
authority to an unmatched submitted unit. Static/other declarations without a
canonical body selector retain the existing Meaning build capabilities; no empty
completed replacement is fabricated.

All containing declarations are retained as correspondence rows with their
original receipt-qualified row ID, JSON pointer and ordinal. Nested module and
function spans can both contain a unit; neither is silently selected as the sole
owner. This is range correspondence, not concept/declaration identity. The entire
checked Meaning layer retains packet/source/claim/evidence namespaces, exact
quotations, original normalized directed parallel/reverse/self relations, ordered
hyperedge roles and repeated concepts in distinct role/concept pairs, coverage and original receipt/usage/
cost provenance. Native immutable response artifacts retain the original provider
fragments; the receiver performs no additional deduplication.

Graph selection follows the Meaning layer's stable graph-ID order. The requested
ID order remains separately recorded. A selected graph must belong to the exact
local workspace, snapshot and canonical graph; an older graph is never implicitly
rebound. An explicit Meaning cache-rebound build can select the new snapshot while
retaining its original receipts. The receipt/declaration read ends before semantic
replay begins; immutable local hash/range pins join the observations without an
atomic cross-Store or remote-head snapshot claim.

`CheckedInterpretation` is private, frozen and has no JSON constructor. Its
bounded export works after both Stores disappear. Output nests the complete
verification report, Meaning layer and per-unit correspondence, with zero
`providerCallsThisRead`, network requests and native writes. Original provider
attempts/costs remain scoped to the selected build receipts. Inferences retain
`providerAttestation:"unverified"`; matching quotations do not establish their
correctness. Semantic identity, compiler binding, observed execution and
exhaustive coverage are false or unproven. Root `sourceAuthority:false` and
`complete:false` remain. Sender source-digest/CAS identity, discussions/history
claims, remote ownership and whole-source equivalence remain unverified.

The independent semantic input limit is at most 32 MB, including the initially
read snapshot/canonical metadata and the full admitted Meaning closure. Original
Meaning bounds remain. Host options also bound all packet-unit occurrences
(at most 4,096), declaration comparisons (at most 4,000,000), successful matches
(at most 65,536) and output (1,024 through 64 MB including the CLI LF).
`--max-meaning-input-bytes` lowers input admission. Existing `--max-symbols` and
`--max-source-bytes` lower verification work. No stage truncates source, units,
claims or correspondence to fit. Cancellation, deadlines, corruption and capacity
refuse the complete operation. These cooperative bounds are not an OS sandbox.

Owned protocol/CLI fixtures validate processing and replay, not model quality,
remote billing or production App deployment. Broader receiver reasoning and
independent verification of other received claims retain their own open gates.
