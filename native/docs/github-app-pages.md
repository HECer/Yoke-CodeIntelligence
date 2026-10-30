# Source-bound GitHub App report pages

This component is in development in the isolated App worktree. Native page
preview, persistent CRUD, typed retention, protected HTTP serving and CLI commands
are implemented. Owned Windows CLI and actual T3 browser checks exercise the
native listener. Exact approved comment links are implemented; final integrated
acceptance remains subsequent work. See [publication](github-app-publication.md)
for the separate link permission, captured queue proofs and recovery behavior. A proposal URL alone does not establish that its public
origin is deployed or reachable. No external deployment or publication has run.

## Proposal inputs and source authority

`github_app::pages::preview` takes the existing Store, an explicitly admitted
host, a `PreviewRequest`, page options and the inherited deadline. Its strict
request document is:

```json
{
  "schemaVersion": 1,
  "completionId": "<64 lowercase hexadecimal characters>",
  "expectedQueueRevision": 2,
  "publicBaseUrl": "https://reports.example.invalid"
}
```

The public base must be a canonical HTTPS origin: no user information, path,
trailing slash, query or fragment. The function never fetches that URL. A request
cannot supply HTML, a graph, source text, a signing key or publication authority.
Duplicate keys, unknown fields and invalid identities fail before replay.

The source factory replays authenticated queue and worker history, checks the
current selected completion and exact revision, verifies the current host grant,
and checks the original captured source dependencies. A claim, caller receipt,
new installation grant for an old job, or stale selection cannot become a new
page proposal. Missing captured bytes fail closed. The original source tree is
not recaptured or executed. A caller-owned SQL transaction is refused and remains
owned by the caller.

## Display contract and limits

The proposal binds workspace, App, numeric repository and PR identities, the
original job and installation, queue checkpoint and revision, source snapshots,
review identity, exact HTML bytes, their hash and the proposed URL. The page ID
is stable for the workspace, App, numeric repository and PR. Access tokens use a
separate HMAC domain from webhook and journal authentication. A display object
cannot mint an opaque worker source or a comment-send permit.

The graph keeps head and removed-source base in separate sections. Each section
preserves native node identities, directed parallel observations, resolution
labels and exact evidence ranges. Node roles distinguish changed symbols,
incoming candidates and candidates elsewhere in a listed path. Labels do not
establish semantic identity. The display excludes source signatures and source
excerpts. Coverage, omissions and whole-tree limitations remain explicit.

Default limits are 5,000 nodes and 20,000 observations **per captured side**,
2,000,000 HTML bytes in total and 4,000,000 complete JSON stdout bytes, including
the final LF. Hard maxima are 10,000 nodes and 40,000 observations per side,
4,000,000 HTML bytes and 16,000,000 stdout bytes. Limits refuse a proposal rather
than silently truncating its graph. JSON escaping and the complete trusted
template count toward the HTML budget.

The client displays at most 80 matching symbols at once. Search, side selection
and pagination operate on the full admitted payload. The displayed counts state
which symbols and observations are visible. Selecting a symbol exposes its
original metadata, connected directed observations and their evidence. These
controls run locally and grant no source, network or write authority.

Dynamic data is escaped JSON and is read with `JSON.parse`; labels are assigned
through text nodes. Scripts, styles and fonts require no external resources.
The inline policy admits only hashes of the trusted script and style. The
template is normalized to LF before both rendering and hashing; a native
regression checks browser-normalized hashes and identical LF/CRLF output.
No browser or HTTP security certification is claimed for the development page.

Preview returns `persisted`, `published`, `pageWriteAuthority`,
`publicationAuthority`, `currentHeadVerified`, `sourceWritten` and `complete`
as false, with zero new network requests and model calls. Replaying historical
provider observations does not establish a current remote head or an atomic
provider lock. `bearerReadCapability` describes the proposed URL's conditional
read access: a page must be stored, unexpired and admitted by the current host
key and repository grant. The URL grants no page write or comment-send authority.

## Persistent cache operations

`store_page`, `read_page` and `remove_page` take an explicit `pages::Context`
with Store, Host, Options and the existing host Clock. All require `allow_pages`;
mutations additionally require `worker.queue.allow_writes`. A proposal supplies
display data and proposed bytes, while a Store request must explicitly approve
its exact source selection, proposal ID, HTML hash and origin with
`approval: "store_exact_page"`. A request cannot supply HTML or graph data.

A signed per-workspace/App revision invalidates old approvals after deletion or
eviction. Identical retries of the currently stored proposal do not refresh its
lifetime. Replacement needs a fresh approved preview. Defaults are a 14-day
lifetime and 500 pages per App, with a workspace-wide 64,000,000-byte limit over
all page and revision rows. Fixed hard bounds are 90 days, 1,000 pages, 1,000 App
revision rows and 128,000,000 bytes. Configured byte admission counts unchanged
rows exactly, including admitted JSON whitespace. Eviction removes expired and
then oldest refreshed pages belonging to the current App. It cannot evict another
App's rows to obtain capacity. A lower limit may therefore refuse without mutation.

Exact retries use the current canonical repository grant and recheck the original
captured sources. They retain the original job and grant no new selection or
publication authority. Reads authenticate the current-key bearer before SQL,
replay the original completion under the current repository grant and check
expiry again after replay and transaction completion. A GET does not refresh or
delete rows. Missing, invalid-bearer and expired reads return absence.

`remove_page` requires `approval: "remove_exact_page"`, exact page version and
cache revision, and the current repository grant. It can remove an authenticated
page after its source becomes unavailable. Removal is cache removal: an explicitly
approved recreation deliberately restores the stable per-PR bearer URL. Rotation
of the current host key revokes old read tokens; explicitly retained prior keys
authenticate historical rows without becoming current read-token authority.

Writes require WAL with FULL or EXTRA synchronization. Caller-owned transactions
are refused. Page and revision changes commit atomically; caller busy timeout,
SQLite length limit and Store progress policy are restored. Output is admitted
before writes and cancellation is checked after commit. A cancelled response
can therefore follow a committed write; an exact retry identifies that result.
Fresh storage conservatively charges its internal complete preview against the
configured output limit in addition to admitting the compact receipt.

Expiry depends on trustworthy host UTC. The existing worker clock checks
`SystemTime` on each call and rejects regression below its in-process observed
floor. Across restarts only signed persisted transitions establish a floor.
A nonmutating read cannot durably remember an expiry observation: a restarted
clock above the persisted floor but below a prior read observation can make a
page visible again. No monotonic elapsed-time guarantee is claimed.

The retention implementation verifies page graph identity under fixed hard projection bounds,
independently of current configured page limits. A valid non-pageable worker
completion must not break unrelated worker or publication retention. Later
selection revision/checkpoint claims have their own typed queue proof. The
structural retention validator uses existing completed worker roots and checks
the exact safe display packet; it grants no serving, signing or write authority.

## Native listener

`ingress::start_pages` takes explicit host `PageService` capabilities. Serving
requires both `allow_serving` and `options.allow_pages`. A page-only listener
does not require queue-write permission; enabling webhook intake additionally
requires its existing separate permission and queue-write consent.

Only canonical `GET /p/<64 lowercase hex>?t=<64 lowercase hex>` is admitted.
Current-key constant-time authentication precedes the Engine mutex and SQL.
Missing, invalid-token and expired pages produce identical 404 packets. Reads
recheck the current canonical repository grant and exact captured sources. A
revoked grant or unavailable source fails without releasing HTML. Request URLs
never become filesystem paths. Duplicate headers, body framing, unexpected
query fields, encoded paths and untrusted Host/SNI values are refused.

Successful responses contain the exact approved UTF-8 HTML, no-store,
no-referrer, nosniff and the full matching CSP in an HTTP header. The complete
wire packet, including headers, is admitted before allocation/output; a small
positive cap produces a bounded error without partial HTML. Expiry is checked
again after replay and packet construction. The existing bounded connections,
request count, parser, TLS 1.3, absolute deadlines, inherited cancellation and
joined shutdown also apply to page serving. Remote binding requires explicit
permission and separately selected host TLS material. Access tokens are not
logged by the server.

`GET /healthz` reports listener liveness and the enabled page/webhook roles. It
does not read SQL, expose page IDs, count durable queue/cache entries, or attest
provider access or source readiness. Its `complete` flag remains false. The
same Host/SNI, framing, deadline and wire limits apply.

## CLI roles and owned verification

`yoke github-app pages preview --request <host-file>` requires
`github-app --allow-webhooks` for source access, with no write, network, page
mutation or serving consent. It prepares an exact proposal and performs no
external request. Page `store` and `remove` additionally require global
`--allow-writes` and `pages --allow-pages`, plus their exact request approval.
They cannot publish a comment.

`yoke github-app pages --allow-pages --allow-page-serving serve` starts a bounded
page-only listener with the explicitly selected `github-app --profile` document.
It can run without `--allow-webhooks` or `--allow-writes`. A combined listener
uses both of those existing webhook/write flags. Outbound `--allow-github` is
refused for every page operation. Remote page serving requires
`--allow-remote-pages`, `--tls-credential` and admitted `--tls-server-name` values.
All listener fields are ordinary host flags, never incoming JSON selections.

Page CLI bounds include `--ttl-seconds`, `--max-pages`, `--max-page-store-bytes`,
`--max-html-bytes`, `--max-nodes`, `--max-observations`, `--max-records` and
`--max-worker-store-bytes`. Global `--max-bytes` caps complete stdout including
its newline; the existing queue/worker CLI bound limits it to 4,000,000 bytes.
Permission and numeric admission precede host documents; strict request parsing
precedes credential lookup and Store opening. The CLI always uses the host
system clock. It offers no JSON, environment or command-line clock override.

The owned Windows oracle uses a temporary public fixture key whose outer
controller verifies creation and deletion outside the memory-limited native
job. Eight real CLI calls cover preview, store, historical retry without TTL
refresh, removal, stale-approval refusal, bounded read-only service lifetime and
linked-comment preview with and without its separate permission.
It checks all 17 logical tables, original record values, CAS bytes and source
bytes. The actual T3 browser oracle serves stored approved HTML directly from
the native listener, checks desktop/mobile layout, search and exact directed
source evidence, then stops and joins the listener with unchanged Store/CAS/source.
These local fixtures do not verify a live installation or public HTTPS origin.
