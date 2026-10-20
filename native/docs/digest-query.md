# Received digest metadata queries

The native receiver can deterministically project and query explicitly selected
received digests. This is an independently owned metadata processor, not an
implementation of a proprietary hosted service or a source verification service.
No model, compiler, source capture, sender signing key or project execution is
started by a query. Results are computed on read; no derived record is persisted.

The distinct [host-local verification command](received-source-verification.md)
requires a separate source root/profile, receipt pin and permission. Its checked
declaration correspondence does not alter this metadata processor or promote
its received rows into source facts.

Both the receiver's host `--allow-query` flag and the matching profile grant's
`"allowQuery":true` are required for HTTP queries. Both default to false. An
existing intake grant alone still permits only PUT/receipt GET. Authentication
precedes query-body acquisition. Current grant/revocation checks guard the read
and response preparation. Library revocation is serialized with that read; it
does not retract responses already admitted. Profile changes take effect after
restart. There is no query administration endpoint.

Start the [receiver](digest-receiver.md) with its usual intake/network/write/TLS
permissions plus `--allow-query --max-response-bytes 64000`. The larger response
ceiling is an explicit host choice. Its announcement then reports
`processingAvailable:true`, `queryAvailable:true` and
`processor:"literal_metadata_projection_v1"`. This denotes only the metadata
processing described here; it establishes no semantic analysis or source truth.

## Explicit request and scope

Supply a host JSON file with this closed schema. Placeholder hashes must be
replaced by the actual sender workspace and accepted proposal IDs:

```json
{
  "schemaVersion": 1,
  "appId": 123,
  "senderWorkspaceId": "<64 lowercase hex characters>",
  "installationId": 42,
  "repositoryId": 123456,
  "repository": "owner/repo",
  "proposalIds": ["<accepted 64-character proposal ID>"],
  "kinds": ["symbol", "commit", "pull", "observation", "relation"],
  "text": "",
  "offset": 0,
  "limit": 20
}
```

Select 1–32 unique proposal IDs in explicit order, 1–5 unique row kinds, offset
0–32,000 and page limit 1–1,000. Text is a case-sensitive literal substring of
canonical received metadata JSON plus scope context; it is limited to 256 Unicode
characters/1,024 UTF-8 bytes with no control characters. It is not tokenized,
normalized or interpreted as regex, SQL, instructions or semantic search.

`POST /v1/digest-query` accepts at most 8,192 body bytes and no idempotency key.
The same strict Host/SNI/header/framing rules as intake apply. Every selected
proposal must belong to the current endpoint, principal and exact App/workspace/
installation/repository scope before any rows are returned. Unknown or foreign
selections refuse the whole query with 404; denied scope/permissions return 403
and unknown/revoked credentials return 401. An absent selection is never reported
as a complete empty result. Protocol/work/output failures produce no partial page.

## CLI and library

The local command opens only existing receiver state through SQLite read-only
access. It requires a current host profile and named query-enabled principal,
but no network or credentials:

```sh
yoke --root /host/receiver-state --max-bytes 64000 digest-inbox local \
  --allow-query --profile /host/receiver.json --principal sender-workspace \
  --request /host/query.json
```

SQLite may coordinate WAL through sidecars; this is not a promise of zero
filesystem coordination. The command creates no application state, changes no
schema and writes no source or query records. `--max-visited` and
`--max-search-bytes` lower processing ceilings.

The remote command selects a service profile and current App profile, reads only
the service's native OS token and makes one HTTPS POST. The App profile scopes
the request without loading its signing credential. It does not select a source
workspace and can run from an empty directory:

```sh
yoke --max-bytes 64000 digest-inbox remote --allow-query --allow-network \
  --profile /host/service.json --app-profile /host/app.json \
  --request /host/query.json --ca /host/public-ca.der
```

`--ca` optionally adds a host-selected public DER trust root while retaining
normal certificate/hostname validation. Production credentials never go to
plaintext HTTP. The library has a separate synthetic-token numeric-loopback
fixture API. There are no automatic retries, redirects, ambient proxies,
cookie credentials or automatic source/model permissions. Both commands refuse
global source/provider/write options before opening state.

## Projection and evidence

Rows preserve original array ordinals as JSON pointers into the immutable received
body. Their IDs bind receipt, kind and pointer, so identical symbols, repeated
provider IDs and parallel directed relationships remain distinct. No names or
remote IDs are merged into local graph entities. Commit-parent edges retain
direction and repeated parent positions; reply and review-membership edges retain
their separate types. A commit-parent presence flag searches the original digest;
reply/review presence flags search the original captured thread. They do not
resolve across other selected packets or verify Git objects, source calls or
observed execution.

Each manifest retains the receipt/payload/digest IDs, principal, received time,
reported source scope, source/discussion hashes and supplied coverage/omission
metadata. Each row says `sourceAuthority:false`,
`semanticIdentityEstablished:false` and
`provenance:"authenticated_sender_claim_unverified_source"`. Byte/line positions
inside metadata are sender claims. The usable receiver anchor is the JSON pointer
into the accepted packet; there is no receiver-local source/CAS binding.

Pagination and counts apply only to selected metadata families. `hasMore` reports
remaining matches after the page and `completeForSelectedMetadata` reports page
exhaustion. The root `complete` is always false. Received hashes, coverage flags,
compiler labels, source associations and provider timestamps remain unverified
sender metadata. Discussion bodies, authors, commit messages and implementation
bodies excluded by the handoff remain unavailable for querying or reasoning.

Host processing caps are 32,000 selected-family rows and 32 MB of canonical JSON
search work; encoding/output ceilings are at most 65,536 bytes including the CLI
LF or the receiver's HTTP packet. Stored-record replay retains the existing
1,000-record/32 MB aggregate limits and absolute deadlines/cancellation. All
selected data is scanned for counts even after filling a page. These are
cooperative application bounds, not a hard OS memory/CPU sandbox. Output/work
overflow refuses the whole result. The remote client checks protocol shape,
request/scope/receipt-qualified row bindings and false authority flags before
exposing the result, wrapped with `networkRequests:1`.

The separate [App history driver](app-history-integration.md) provides explicit
bounded acquisition, and [host-local verification](received-source-verification.md)
checks finite received Rust declaration correspondence. Public production,
live App installation, broader packet verification, semantic reasoning and
universal upstream equivalence remain open.
