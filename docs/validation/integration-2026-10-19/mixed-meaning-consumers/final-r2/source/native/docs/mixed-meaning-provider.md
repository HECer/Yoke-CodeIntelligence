# Explicit checked mixed-source meaning builds

Source-replayed graphs also support [checked literal graph queries](mixed-meaning-queries.md)
through `load_checked` and `mixed-meaning query`. These preserve original
provenance, directed observations and ordered roles without provider execution.

`mixed-meaning build` runs one fixed, fully admitted request through an explicitly
trusted host provider. It selects existing frozen native, Office decoded/XML and
optionally PDF page inputs. It captures no source and starts no language server
or analyzed project code automatically. Ordinary indexing, `mixed-view`, source
queries, job inspection and graph loads start no model provider.

```powershell
yoke --root C:/Work/project --timeout 60000 --max-bytes 4000000 mixed-meaning build --selection C:/Host/selection.json --inputs C:/Host/inputs.json --config C:/Host/provider.json --policy C:/Host/policy.json --trust-provider --native-pdf-text
yoke --root C:/Work/project mixed-meaning status JOB_ID
yoke --root C:/Work/project --max-bytes 16000000 mixed-meaning load JOB_ID --native-pdf-text
yoke --root C:/Work/project mixed-meaning jobs --task mixed-review --offset 0 --limit 20
```

Omit `--native-pdf-text` for schema1 selections. The separate PDF permission is
required for schema2 build/load; status/jobs never run a PDF helper. Global
capture, snapshot, ignored-source, analyzer, language-server and source-write
options are refused. An existing initialized workspace store is required.

The selection is the strict schema1/schema2 host file from
[mixed views](mixed-views.md). The input file contains the closed
`MeaningInputRequest` from that same contract: selected unit labels/references
and optional ordered context groups. Malformed request shapes are refused before
source loading or cost reservation. Exact range, original source-policy and
UTF-8 checks apply before a provider can start.

The host provider file has this shape; replace the executable/model placeholders
with a reviewed implementation of the protocol:

```json
{
  "command": "C:/Host/bin/mixed-provider.exe",
  "args": [],
  "modelId": "host-selected-model",
  "modelVersion": "pinned-version",
  "taskId": "mixed-review",
  "currency": "EUR",
  "maxCostMicros": 1000,
  "taskLimitMicros": 10000
}
```

These numbers are host accounting limits, not quoted service prices.
Repository/configuration JSON cannot grant `--trust-provider`. ProviderSpec is
not deserializable source authority; the CLI constructs it only after that host
flag. Credentials belong in the existing OS credential workflow, not arguments.
The trusted provider runs in an owned temporary working directory. Its network,
filesystem, imports and ambient environment are not sandboxed or fully attested.
`sourceWritten: false` is scoped to native parent source operations; it does not
certify effects of an arbitrary trusted executable.

The closed policy file contains `contractHash`, optional absolute
`providerInputFiles` and optional `maxGraphBytes` (default16,000,000;
10,000,000 through64,000,000). The extraction contract is selected by the host.
The existing host fingerprint hashes the resolved executable, recognized
script/config file arguments and explicit supplemental files with the original
deadline. It is an implementation-input observation, not runtime attestation.

## Provider protocol and source spaces

The strict UTF-8 JSON request uses `schema: 1`,
`operation: "extract_mixed_meaning_v1"`, model/version, request/input IDs,
currency, task ID, cost limit and `payload`. The payload has
`kind: "mixed_meaning_provider_input_v1"`, the checked prepared `inputHash`,
`contractHash`, `units` and `contextGroups`. Each unit has its prepared
`sourceId`, typed `source`, exact `text` and `omittedBytes: 0`. Context groups
carry ordered `unitIds` and logical `textBytes`; repeated IDs remain. Those
byte counts describe repeated selected text, not prompt tokens or provider cost.

The provider replies with the same schema, operation, model/version,
`requestId`, wire `inputHash`, currency and the exact ordered `sourceBindings`
array (`sourceId`, typed `source`). Optional `actualCostMicros`, `usage`, `data`
and `error` complete the closed envelope. Data is the native fragment shape of
concepts, directed relations, role-bearing hyperedges and per-unit coverage.
Support `sourceId` identifies a submitted unit; support byte coordinates use
that unit's original typed source space. PDF page and Office decoded/XML
coordinates never become native code-file offsets. Original claim keys,
aliases, ordering, directed parallel/self relations and role slots remain.

Unknown/duplicate fields, invalid UTF-8, incomplete output and incorrect bindings
do not establish a bound response. A completely identified response's valid cost
is accounted before interpretation, invalid usage, provider error or exit-status
rejection. Overruns retain the reported amount, publish no graph and block later
sends under that task. No cost is silently clamped. Unknown sends/billing retain
their reservation. Only durably unsent reservations are canceled. There is no
automatic retry or resume; each explicit new build has a new nonce and attempt.

## Persistence and read behavior

One short ledger/job transaction reserves the attempt, registers immutable input
and payload artifacts and records the fixed job. No transaction spans a provider
process. The single-use reservation authorizes one send. A fully bound reply
has an immutable raw response artifact. A completed graph/artifact/job transition
uses the exact previous job row as a publication precondition. Invalid bound
responses keep their accounting/response history without a completed graph.
Failures after publication identify the committed job for a fresh status read.

Build returns an explicit durable reference instead of truncating a graph into
stdout. CLI output includes its final LF in a1024..64,000,000-byte budget. The
fixed complete reference is admitted before sending. If emission fails after
commit, stderr identifies the job; `jobs --task` also recovers recorded IDs
without provider configuration. Listings use ID order, explicit pagination,
at most4096 records/16MB metadata and1..100 results per page. A capacity or
corruption refusal is never reported as an empty complete listing.

Load requires the original pinned source/receipt closure, exact prepared input,
typed bindings, raw response, immutable ledger response event and materialized
graph to replay. It uses no installed provider. PDF text is rechecked through a
separately host-selected own helper. Retention validates the closed stored graph
from saved observation data and recomputes original native/document/media roots
without a PDF process; saved JSON cannot construct a checked source capability.
Actual graph loading independently replays the source bytes.

`modelCallsThisRun` in a graph is scoped to its original bound host build
attempt, with provider-internal calls unverified. Reads explicitly report
`providerCallsThisRead: 0`. Original response usage/cost remain separate from
later aggregate ledger reconciliation; an original unknown report stays null.
Interpretations remain `inferred_concept`, `providerAttestation: unverified`,
`canonicalSourceAuthority: false` and `complete: false`. Checked quotations
authenticate their bytes, not the interpretation, runtime behavior or absence
of other relationships.

## Bounds and qualification

Existing mixed input/source/metadata bounds remain. The submitted payload is at
most1MB and complete request1,016,384bytes; replies are at most2MB. The shared
claim/coverage/quote bounds from [interpretation checks](mixed-meaning-claims.md)
apply. A10MB worst-case result budget is admitted before a send (prepared input,
fragment, quoted items and bounded descriptors/receipt); source selections are
never shortened to fit. Deadlines/cancellation cover all stages.

Owned local Node protocol fixtures exercise actual process/CLI transport,
Unicode source spans, all four spaces, accounting failure modes, immutable
replay and retention. They evaluate no semantic model, remote billing or model
quality. General mixed-graph lookup/analytics, cross-workspace composition and
full upstream/platform acceptance remain separate work. The legacy native code
Meaning graph, cache recipes and default model-free facade are unchanged.
