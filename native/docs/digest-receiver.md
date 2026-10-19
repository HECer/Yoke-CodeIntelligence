# Native digest receiver

`digest-receiver` implements the independently owned receiver-v1 contract used
by the [approved digest client](digest-service.md). It accepts authenticated,
bounded JSON submissions and commits their exact wire bytes before returning a
receipt. A repeated identical PUT and a subsequent GET return the original receipt,
including after restart. By default the service advertises
`processingAvailable:false`. Separately authorized [metadata queries](digest-query.md)
add deterministic processing on read; neither mode runs a model or executes source.

Incoming headers, source hashes, snapshot IDs, compiler/coverage labels and
discussion metadata remain sender assertions. The receiver checks the closed
envelope/field shape and content bindings; it cannot replay a remote source CAS
or prove that a header excludes every private detail. It does not convert received
IDs into local source captures, graph facts, filesystem paths or retention roots.
No proprietary brain or upstream service implementation is claimed.

## Host configuration

The host selects an absolute profile path. The following IDs are placeholders;
use the actual sender workspace/App/repository scope from the reviewed proposal:

```json
{
  "schemaVersion": 1,
  "endpoint": "https://digest.example.invalid:8443/",
  "grants": [{
    "id": "sender-workspace",
    "tokenHash": "<64-character lowercase SHA-256 of the raw 32-byte token>",
    "appId": 123,
    "senderWorkspaceId": "<exact 64-character sender workspace ID>",
    "installationId": 42,
    "repositoryId": 123456,
    "repository": "owner/repo"
  }],
  "maxPayloadBytes": 4000000,
  "maxRecords": 1000,
  "maxStoredBytes": 32000000
}
```

Each grant binds its ID and token hash to one App, sender workspace, installation,
repository ID and canonical repository name. The same token can have several
explicit scopes; duplicate grant identities/scopes refuse. There are at most 32
grants. Remote requests cannot create, select or widen grants.

On the sender, `yoke credentials token sender-token` creates a unique native OS
credential and prints its public token hash. Configure that hash on the receiver
and use `credentialName:sender-token` in the sender service profile. The raw token
stays in the sender's OS store; it is not copied into the receiver profile. The
Bearer header carries its hex encoding over TLS, and the receiver hashes the
decoded 32 bytes before constant-time comparisons. Ambient tokens, cookies and
repository settings provide no authority.

The receiver TLS private key also comes from the native OS credential store.
For an independently trusted private service, `yoke credentials tls receiver-tls
--hostname digest.example.invalid` generates an identity and returns public
certificate metadata. Provision its public DER CA/certificate through a trusted
host path on the sender and select it explicitly with `--service-ca /host/ca.der`
on send/recovery. This adds that root without disabling hostname or certificate
verification. Ordinary public trust remains available. No global root-store
change, plaintext private-key file or automatic certificate installation occurs.

```sh
yoke --root /host/receiver-state --allow-writes --timeout 600000 \
  digest-receiver --profile /host/receiver.json \
  --listen 0.0.0.0:8443 --allow-receiver --allow-network --allow-remote \
  --tls-credential receiver-tls
```

The root is explicit host state; it may be an empty existing directory. Startup
creates/opens only the native Store and does not index the directory or open a
source Engine. Profile/permission/TLS selection is admitted before loading the
private key or opening state. The configured origin port must agree with the
listener. TLS is mandatory off loopback, and remote binding requires its own
host flag. The service prints one bounded JSON announcement and waits for its
lifetime/request ceiling. Source, analyzer, capture and language-server options
are unavailable for this command.

Plaintext numeric loopback requires explicit `--allow-fixture`. A fixture may
select listener port zero; the announcement supplies the resulting endpoint.
TLS fixtures can also use this explicitly selected ephemeral port. Production
profiles select their exact origin. There is no HTTP redirect or proxy-trust
fallback. The ordinary client fixture never loads a production token; the
library's separate `for_fixture_token` constructor takes an explicit synthetic
host token solely for an owned numeric-loopback receiver.

## Admission and receipts

Intake uses HTTP/1.1 `PUT /v1/digests/<proposalId>` and `GET` of that same path.
The optional `POST /v1/digest-query` route requires its own host and per-grant
query permissions. The Host authority and TLS server name must match the selected origin.
Authentication precedes body acquisition. Duplicate headers, ambiguous lengths,
transfer/content encodings, Expect/trailers and browser Origin headers refuse.
PUT requires JSON content type, a fixed length and the exact `Idempotency-Key`.
GET has no body and returns a receipt, never the submitted payload.

The body must use `native_digest_submission_v1` and the native handoff schema.
The handoff payload ID is replayed, and the route proposal ID must bind the exact
wire SHA-256, endpoint, sender workspace and App. Current matching grants are
checked again after body acquisition. Explicit library revocation is serialized
with that final admission and commit; revocation during a slow upload prevents
intake. Such revocation is process-local. The host profile supplies the policy
after restart; no remote user-administration endpoint exists.

An accepted record stores the exact UTF-8 body, its hashes, receiver workspace,
principal ID, origin and receipt time. The receipt ID binds that immutable record.
WAL with FULL/EXTRA synchronous durability and an immediate transaction publish
record and receipt together. A duplicate must match the original bytes and
principal/scope and does not replace the receipt or consume another record.
GET uses the current grant; absent or unauthorized scoped records return 404.
An unknown/revoked bearer returns 401. Refusals do not echo tokens, database
messages or submitted text.

The service handles one connection at a time. Hard ceilings are 16 KiB of
headers, 4 MB per submitted body, 1,000 records, 32 MB of aggregate encoded
records, 8,016,384 bytes per encoded record, 10,000 connections per service run,
10 seconds per request and one day per run. The host may lower them. HTTP
responses are byte-admitted before record insertion. All stored rows are
bounded and replayed before serving or mutating; corruption refuses. Only small
scope summaries accompany retained raw records during replay, rather than a
second full decoded corpus. Partial/slow requests, the original host deadline,
host cancellation and shutdown retain their bounds across the worker thread.

Typed retention recognizes the receiver records as inert intake history and
does not traverse their body IDs as CAS references, even when a submitted ID
happens to equal a local historical snapshot. Source captures can therefore
retire under their existing local authority. Received metadata is stored as
plaintext host data under the same Store access controls; no encryption-at-rest
or complete DLP guarantee is added.

This implements receipt-backed intake and client recovery. The separate
[query contract](digest-query.md) describes received metadata processing and its
limits. Configured public production deployment, automatic historical acquisition,
source verification, semantic reasoning and full upstream/proprietary equivalence
require separate work and evidence.
