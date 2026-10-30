# GitHub webhook HTTP ingress

The native `github-app serve` command receives signed GitHub webhook bodies into
the existing immutable delivery queue. It does not execute queued reviews,
verify current PR heads, create an installation token, invoke a model or publish
comments. Installation authorization is a separate host operation. Full GitHub
App integration and universal replacement parity remain open.

```text
yoke --root SOURCE --allow-writes --timeout 600000 github-app --allow-webhooks \
  serve --profile HOST_PROFILE_JSON --listen 127.0.0.1:0 --max-requests 1000
```

Use the strict host profile and private-stdin webhook secret import described in
[delivery admission](github-app-admission.md). The source must already have an
initialized Store. Permission and service-limit checks precede profile files,
OS credentials, source Store access and listener creation. Snapshot, capture,
language-server and unrelated GitHub network permissions are unavailable.

The command prints one bounded JSON announcement, flushed after listener
creation, containing the actual endpoint. It then waits for its owned monitor
and request threads to finish. A successful exit joins those threads. The
library `github_app::ingress::ServerHandle` additionally returns a final report
from `wait()` or `shutdown()`; dropping the handle requests shutdown and joins.
Use the announcement rather than a readiness connection when testing with
`--max-requests 1`, because accepted connections consume that budget.

## Transport and framing

Plain HTTP is restricted to loopback. A non-loopback numeric bind requires
`--allow-remote-webhooks`, an explicit `--tls-credential NAME`, and at least one
`--tls-server-name NAME`. TLS material is read only from the separate existing
host TLS credential namespace. No credential is generated automatically.
TLS uses TLS 1.3 and HTTP/1.1; configured DNS/IP authorities and any supplied SNI
must be allowed. TLS Host may omit port 443; other ports must be explicit. Plain
HTTP Host must equal the actual bound socket authority. This is server TLS,
without client-certificate authentication. Certificate trust, public DNS,
firewall, routing, service supervision and real remote deployment are host
responsibilities outside this local implementation check.

Only one `POST /github/webhook HTTP/1.1` request is processed per connection,
followed by close. CRLF headers, a unique Host, bounded numeric Content-Length,
JSON content type, canonical delivery ID and SHA256 signature header are
required. Duplicate headers, transfer/content encodings, Expect, upgrades,
trailers, malformed framing and already-buffered extra body/pipelining refuse.
The receiver cannot reject bytes that have not arrived; it never processes a
second request on that connection. Exact body bytes, including UTF-8 and CRLF,
go to HMAC verification and durable storage.

Supported labels are `pull_request` and `ping`. Pull-request bodies use the
existing strict admission, manual grants and immutable queue identity. Ping
requires a signed, bounded, duplicate-safe object with `zen` and `hook_id` and
only the known ping fields; it acknowledges authentication without a job.
Ping and PR schemas are disjoint. HMAC authenticates the body, not event labels
or delivery IDs. No response establishes installation ownership or a current
remote head.

## Durability and budgets

A PR response uses HTTP 202 only after successful SQL commit. The entire
successful wire response is encoded and admitted before mutation. Failed
commit, authentication, framing, grant or output admission cannot produce 202.
A client disconnect after commit can leave a durable row without a delivered
acknowledgement; exact retries use the existing idempotent admission contract.
There is no automatic HTTP request retry or remote side effect.

Default limits are 4 concurrent request threads, 1000 accepted connections,
5000 ms per request, 4096 response bytes and a lifetime equal to the CLI timeout
(600000 ms for the library). Bounds are 1–8 threads, 1–10000 connections,
10–10000 ms per request, 1–65536 response bytes and 1–86400000 ms lifetime.
Global `--timeout` also caps the service. Response bytes include HTTP headers,
JSON and final LF, excluding TLS record overhead. Header parsing is capped at
16384 bytes and 48 headers. Existing queue payload/history/store limits apply.
Zero service, payload or announcement-output limits refuse before listening;
a positive response cap too small for a response refuses delivery before SQL.

All sockets and worker lifetimes inherit host cancellation. Slow header/body
drips do not refresh the absolute request deadline; socket and TLS waits check
it in short intervals. The ingress-only SQL path limits busy waits to the
smaller of the remaining deadline and 50 ms, refuses contention without retry,
and restores the connection policy after transaction cleanup. Ordinary queue
CLI/library operations retain their existing lock-wait policy. Cancellation
is cooperative, without an OS scheduler or all-memory-zeroization guarantee.

The final library report distinguishes accepted connections, successful durable
admissions (including idempotent repeats), authenticated pings, refusals and
written responses. It reports closed listener and joined workers only after
cleanup. A request count is neither a count of new rows nor proof of review
execution. `sourceWritten`, `published`, `currentHeadVerified`,
`installationTokenVerified` and `complete` remain false.

## Validation scope

Owned loopback and TLS fixtures cover framing, exact raw bytes, replay/conflict,
failed SQL commit, positive tiny output budgets, slow drips, worker saturation,
lifetime, inherited cancellation, stalled TLS and independent SQL writer locks.
An owned Windows CLI oracle imports a temporary webhook credential through a
private pipe, starts an ephemeral loopback listener, sends an independently
signed body, checks raw durable bytes and source preservation, and verifies
cleanup. Local fixtures do not certify a live GitHub App, public endpoint,
deployment, worker or publication flow. Actual results belong to the separate
source-bound validation receipts.
