# Signed GitHub delivery admission and review candidates

This bounded native CLI/library slice authenticates explicitly supplied delivery
bytes and stores immutable review candidates. Full GitHub App integration and
universal parity remain open. Separate source-bound validation receipts track
scoped admission checks; this document does not certify full product parity.

The host must provide a strict profile outside the event payload:

```json
{"schemaVersion":1,"appId":123,"secretCredential":"review-hook",
 "priorSecretCredentials":[],"grants":[{"installationId":42,
 "repositoryId":9,"repository":"acme/demo"}]}
```

Only explicit host-selected Windows Credential Manager entries in the separate
`yoke-code-intelligence/service/github-webhook/NAME` namespace can supply signing
keys to the CLI. Import exact UTF-8 bytes through a private pipe with
`yoke credentials github-webhook-import NAME`; command arguments, echoed terminal
input, newline trimming, bearer-token fallback and secret hashes are unavailable.
Names are create-only and bounded. Optional previous credential names (up to
three) allow explicit key rotation. Stored records never select credentials.
In-process hosts can construct an opaque `SigningKey` directly from their own
bytes. The type cannot be serialized or printed with Debug.

Normalized delivery headers use a strict host JSON object:

```json
{"schemaVersion":1,"event":"pull_request",
 "deliveryId":"01234567-89ab-4cde-8000-000000000001",
 "signature256":"sha256=<64 lowercase hexadecimal digits>"}
```

The first supported event admits `opened`, `reopened`, `synchronize`,
`ready_for_review`, `converted_to_draft`, `edited` and `closed`. Exact original
UTF-8 body bytes, including CRLF and whitespace, enter HMAC-SHA256 and payload
identity. Constant-time MAC comparison precedes duplicate-key-safe JSON parsing.
Required installation/repository/PR/base-repository identities, SHA1 head/base
pins, state and draft/action consistency must agree. Extra GitHub payload fields
retain their original bytes; repository display names are compared using ASCII
lowercase against the canonical host grants. Extra payload properties
are inert authenticated data. Host profiles, headers and stored event schemas
reject unknown fields.

```text
yoke --root SOURCE github-app ingest --profile HOST_PROFILE_JSON \
  --delivery HOST_HEADERS_JSON --payload RAW_BODY_JSON \
  --allow-webhooks --allow-writes
yoke --root SOURCE github-app queue --profile HOST_PROFILE_JSON --allow-webhooks
yoke --root SOURCE github-app select --profile HOST_PROFILE_JSON --job JOB_ID \
  --expected-revision OBSERVED_GROUP_REVISION --allow-webhooks --allow-writes
```

Permission checks precede any host file, OS credential or project Store access.
Use the selected job's `groupRevision` from the queue result as the selection
precondition; new jobs or another selection can make that observation stale.
Ingest additionally authenticates and validates the supplied body before opening
the existing Store. These commands do not capture source, run a project/provider,
call a model, send a network request or publish a comment.

One closed typed record kind stores exact body text, public MAC/header metadata,
immutable job identities and host selection transitions inline. No new CAS blob
is published. A job binds workspace, app, installation/repository, PR, head SHA,
base SHA and work class. A changed base changes the review diff even for the same
head. Closed-discussion candidates belong to a separate work class; they are not
a generated discussion digest. The first delivery's title/draft are historical
metadata, not current PR metadata.

Same admitted delivery ID plus exact body is idempotent; a different signed body
under that ID refuses. Relabeling an already admitted signed body with another
unbound delivery ID cannot create another delivery or job. Another authenticated
body for the same job can add a delivery without duplicating the job.

Every new job and selection advances its per-PR/work-class revision. A job
arriving between a queue read and selection invalidates that observation.
Selection uses this exact revision, selects one existing job and
suppresses other jobs that already exist in that group. Later arrivals remain
queued. Replaying the same applied selection precondition and job is idempotent
and does not suppress later arrivals. Stale, conflicting preconditions refuse.
Selection is an explicit host scheduling assertion. HMAC covers the body, not
delivery/event header labels, arrival order, installation-token ownership or the
current Git head. No job state proves execution or publication authority.

All operations replay the complete bounded event history and typed identities.
Keyed operations recheck body signatures using only the current host-selected
keys. Queue results include original job provenance, selection revision,
offset/limit/hasMore and explicit incomplete/current-head-unverified flags.
Default bounds are 512,000 raw payload bytes, 1,000 deliveries, jobs and
transitions each and 64,000,000 total stored JSON bytes. Library output defaults
to 4,000,000 bytes including LF; the CLI uses the shared `--max-bytes` setting
(32,000 by default, at most 4,000,000 here). Limits can be reduced to zero;
payloads have a hard 1,000,000-byte
maximum and individual records 2,100,000 bytes. Complete output and store quotas
are admitted before the transactional insert. Caller SQLite limits are restored;
write operations refuse existing caller transactions, while reads preserve them.
Cancellation/deadlines refuse work without a new logical record.

Retention validates this entire closed inline schema/history without OS secret
access. That structural proof gives raw payload fields no CAS-reference
authority; it is distinct from keyed cryptographic verification. Unknown,
oversized, corrupt or inconsistent histories refuse rather than look complete.
There is no atomicity between stdout and SQL or against hostile concurrent file
writers beyond the existing Store boundary.

Bounded [HTTP/TLS ingress](github-app-ingress.md) and separate
[installation authorization](github-app-auth.md) extend this queue. Provider-backed
current-head revalidation, worker claims/completion, controlled publication and
actual closed-PR discussion digests remain open parts of full R8.
