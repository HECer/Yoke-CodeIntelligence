# Explicit digest service handoff

The native `github_app::digest::service` library implements an independently
owned receiver contract. It publishes a freshly replayed captured-source digest
only after exact host approval. This is not an adapter for an undocumented Trail
brain endpoint, an installed GitHub App, a model call or proof of receiver-side
processing. The [native receiver](digest-receiver.md) now implements authenticated,
durable intake for this contract. A production instance must still be configured
separately; no external deployment or publication follows from development tests.

The outbound digest uses the existing handoff projection: captured Rust headers
and imported closed-PR metadata, without discussion text, titles, authors, diff
hunks, commit messages, function bodies or constant/static initializers. Headers,
paths, names, byte coordinates, hashes and repository identifiers still disclose
source information. The exclusion policy is not general DLP. Provider observations
remain unsigned host imports with explicit coverage and source-association limits.
See the [discussion contract](github-discussions.md).

## Host profile and review

An absolute host file selects one canonical origin, one OS service-token name,
explicit repository grants and budgets. For example, using placeholder IDs:

```json
{
  "schemaVersion": 1,
  "endpoint": "https://digest.example.invalid/",
  "credentialName": "review-service",
  "grants": [{
    "installationId": 42,
    "repositoryId": 123456,
    "repository": "owner/repo"
  }],
  "maxPayloadBytes": 4000000,
  "maxResponseBytes": 64000,
  "maxJournalRecords": 1000,
  "maxJournalBytes": 8000000
}
```

Production origins require HTTPS and a service token in the existing native OS
credential namespace. The selected 32-byte token is sent as hex in a Bearer
header. Ambient tokens, proxies, redirects, URL credentials, URL query/fragment
and automatic retries are unavailable. The endpoint must include its canonical
trailing slash and no other path. Network access to a numeric loopback HTTP fixture requires a
separate explicit host flag and `credentialName:null`; it cannot read or forward
a production token. `localhost` is not a numeric fixture address.
An explicit `--service-ca /host/ca.der` on send/recovery selects an additional
bounded host CA without disabling certificate or hostname verification. No
incoming capture or service JSON can select that file.

The existing App profile supplies the current repository grant. Its signing key
is not read for a proposal. Add these options to the existing source/discussion
`digest preview` or `digest history` invocation:

```sh
yoke --root /work/project --max-bytes 4000000 github-app \
  --profile /host/app.json digest --allow-digest \
  --allow-discussions --discussion-capture /host/discussions.json \
  --allow-service --service-profile /host/service.json \
  preview --request /host/digest-request.json
```

The proposal prints the exact structured submission body, byte count, payload
SHA-256 and proposal ID. The proposal ID binds the payload hash, endpoint,
workspace and App ID. Saving or reading the proposal grants no sending authority;
the send operation regenerates the payload from source and imported observations.
Reformatting a displayed JSON body does not define new approved wire bytes.

After reviewing the proposal, an absolute host approval file supplies:

```json
{
  "schemaVersion": 1,
  "proposalId": "<exact 64-character lowercase proposal ID>",
  "payloadHash": "<exact 64-character lowercase payload SHA-256>",
  "requestId": "01234567-89ab-cdef-0123-456789abcdef",
  "approval": "publish_exact_digest"
}
```

The UUID above is an example; use a fresh host request ID. Run the same proposal
invocation with global `--allow-writes`, digest `--allow-service-network` and
`--service-approval /host/approval.json`. These are separate host permissions.
Existing `--allow-github` and `--allow-webhooks` do not authorize service sending.
Source/capture/profile changes that change the approved bytes or endpoint refuse
before intent publication. CLI stale-approval checks precede credential reads;
the send boundary replays again under the journal lock.

## Receiver protocol

Exactly one `PUT /v1/digests/<proposalId>` submits the approved JSON bytes, with
`Content-Type: application/json` and `Idempotency-Key: <proposalId>`. The versioned
body contains `schemaVersion:1`, `kind:native_digest_submission_v1`, `workspaceId`,
`appId` and the native handoff digest. The receiver must atomically bind a proposal
ID to the exact payload SHA-256 and expose the same receipt for subsequent reads.
The client does not rely on idempotence to resend an uncertain write.

A successful PUT returns HTTP 200 or 201 and a closed JSON receipt:

```json
{
  "schemaVersion": 1,
  "proposalId": "<exact proposal ID>",
  "payloadHash": "<SHA-256 of the exact submitted body>",
  "receiptId": "<64-character lowercase receiver receipt ID>",
  "accepted": true
}
```

Duplicate/unknown fields, wrong hashes, oversized responses, unexpected status
or media type, redirects and lost responses leave the local outcome unknown.
Response text and service credentials are not echoed in public errors. A receipt
is a claim by the selected receiver about the submitted payload; it proves no
model quality, completed processing, source execution or provider authentication.

## Journal, status and recovery

Before the PUT, the existing Store commits an HMAC-signed, hash-chained intent
under a separate journal lock. The domain-separated MAC uses the App host signing
key; configured prior keys support rotation. SQLite must use WAL and FULL or
EXTRA synchronous durability. No SQL transaction spans HTTP. Source snapshots,
request and payload/proposal bindings are retained, without persisting the full
outbound body or raw discussion text in the journal. Typed retention recognizes
the source selectors and keeps their captured dependencies; hashes and URLs are
not generic CAS roots. Runtime replay verifies current-host MACs; provider-free
retention performs closed structural replay without signing credentials.

Admission reserves one future receipt record for every pending intent, checks
all possible final output envelopes before the first send, and preserves the
caller's SQL deadline/limits. Hard ceilings are 4 MB of payload, 64 KB per receipt,
1,000 journal records, 8 MB aggregate journal bytes and 8 KB per journal record.
The host may lower these; insufficient reservation refuses before publication.
The same host deadline covers source replay, locks, SQL, HTTP and output. Host
cancellation is checked at boundaries and between response reads; a blocking
HTTP operation remains bounded by its original timeout.

The scoped source/publication policy reinstalls the Store-owned deadline-aware
busy callback before direct transaction SQL, including `BEGIN IMMEDIATE`.
Updating deadline data alone cannot undo an external `busy_timeout` replacement.
The requested deadline also inherits any earlier Store ceiling. The saved numeric
busy timeout and Store deadline/length policy are restored on success or refusal;
this does not promise restoration of arbitrary external callback functions.

An intent alone is an unknown outcome, including a crash before sending. The
same request cannot send again, the same proposal cannot use a different request
ID, and a pending App/repository blocks another submission across installation
changes. A new installation grant does not authorize recovery of a revoked
original scope. After a confirmed
receipt, repeating the exact request returns historical local status without
another network request. Local approval is a workflow gate, not a defense against
another process with the same database/shell privileges.

```sh
yoke --root /work/project github-app --profile /host/app.json digest \
  --allow-digest --allow-service --service-profile /host/service.json \
  service-status --request-id 01234567-89ab-cdef-0123-456789abcdef

yoke --root /work/project --allow-writes github-app --profile /host/app.json digest \
  --allow-digest --allow-service --allow-service-network \
  --service-profile /host/service.json \
  service-recover --request-id 01234567-89ab-cdef-0123-456789abcdef
```

Recovery performs only `GET /v1/digests/<proposalId>` and validates the same exact
receipt before committing a terminal record. HTTP 404 preserves unknown status
and never grants retry authority. Terminal recovery replays locally. Status and
recovery require current App/service repository grants and the original selected
endpoint; a revoked grant refuses. Recovery does not re-submit source or reinterpret
an imported proposal as current source. No automatic send, retry, model invocation,
source edit, GitHub comment or service deployment is introduced.
