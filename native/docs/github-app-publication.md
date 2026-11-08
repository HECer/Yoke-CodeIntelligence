# Source-bound GitHub comments and outcome recovery

This Windows development adds deterministic proposals, explicit publication,
local status and read-only remote reconciliation over a completed
[native worker](github-app-workers.md). Verification uses owned loopback providers
and a separately selected Windows credential fixture. These checks do not
establish a release, primary integration, live GitHub installation certification,
full Windows acceptance or universal parity.

## Preview and exact approval

Use an existing host-selected Store and the webhook signing profile used for its
queue and worker history. Preview does not load a GitHub App private key or an
installation token, access GitHub, capture sources, run project code, call a
model, create an analysis artifact or change source files.

```powershell
yoke --root C:\work\project --max-bytes 128000 github-app `
  --allow-webhooks --profile C:\host\webhook-profile.json `
  publication --max-body-bytes 48000 preview --request C:\host\preview.json
```

The explicit request has a closed, duplicate-safe schema:

```json
{
  "schemaVersion": 1,
  "completionId": "<64-character lowercase completion record ID>",
  "expectedQueueRevision": 2
}
```

The completion must be an authenticated worker completion for the current host
App and grant. Its immutable job must currently be selected at the exact supplied
group revision. Fresh row and captured-byte checks bind the selected report to
its source. A newly selected revision of the same immutable job may be previewed;
the proposal identity changes even if the rendered comment is identical. An old
completion or a JSON receipt supplied by a caller is never publication authority.

The deterministic English Markdown includes exact head/base/snapshot/report
identities, structural candidate counts, directed observation counts, file
coverage, omissions and full-file byte bindings. Source-controlled paths are
escaped inside code elements. No source-body excerpts or model text are included.
Individual Git blob matches do not prove whole-tree commit equality, compiler
binding, runtime execution, merge correctness or complete analysis.

The returned SHA256 body hash covers the exact Markdown. The proposal ID also
binds workspace, App, installation, repository, PR, immutable job, completion,
review hash, selected revision, snapshots and renderer version. The output
explicitly reports false publication/current-head/token authority and zero
network/model calls. The preview is a historical source-based proposal, not a
current GitHub observation.

The complete body must fit `--max-body-bytes` (default 48000, maximum 60000);
there is no silent clipping. `--max-bytes` includes the complete JSON and final
LF. Worker-history limits default to 1000 records and 64000000 bytes and can be
reduced with `--max-records` and `--max-worker-store-bytes`. Zero limits, missing
webhook authority and write/network flags are refused before any host file,
credential or source Store is opened. All explicit host documents are parsed
before credential lookup. Caller-owned SQLite transactions are refused by the
library entrypoint without committing or rolling back them. SQLite limits,
deadline/progress policy and busy timeout are restored after owned read replay.

## Explicit publication

Prepare the request outside analyzed source after reviewing the complete preview.
Its proposal ID and body hash must match the exact current proposal:

```json
{
  "schemaVersion": 1,
  "completionId": "<completion record ID>",
  "expectedQueueRevision": 2,
  "proposalId": "<preview proposal ID>",
  "bodyHash": "<exact preview body hash>",
  "requestId": "11111111-2222-4333-8444-555555555555",
  "approval": "publish_exact_comment"
}
```

Publication requires separate webhook, local journal write, GitHub network and
publication permissions. These are host arguments, never fields inferred from
source, a webhook, a stored report or a remote comment:

```powershell
yoke --root C:\work\project --allow-writes --max-bytes 128000 github-app `
  --allow-webhooks --allow-github --profile C:\host\webhook-profile.json `
  publication --allow-publication publish --request C:\host\publish.json `
  --auth-profile C:\host\app-auth.json
```

The separate [App authentication profile](github-app-auth.md) must name the same
App as the webhook profile and request exactly `{"pull_requests":"write"}`.
Contents, issues, checks and extra permissions are refused. Both profiles and the
closed request are parsed before either OS credential is read. Document admission
does not establish a repository grant or installation ownership; those are checked
again against the selected job and authenticated provider responses. The source
Store must already exist. No command in this group captures or indexes a project.

The publisher replays the signed queue, completed worker and publication history,
checks the exact selected revision and freshly verifies the captured source. It
then checks four installation-authorization responses, repository identity, initial
PR metadata, every admitted file page and final matching PR metadata. The current
head, base, repository, fork and file count must match the completed report. This
is a pair of matching provider observations, not an atomic lock on GitHub or a
proof of whole-tree equality.

A fixed owner lock is held from preflight through transport teardown and outcome
persistence. SQLite transactions do not span HTTP. Before sending, an Immediate
transaction rechecks current grant, selection, source, clock, token time and
capacity, then commits a signed intent. Exactly one POST creates a first comment;
exactly one PATCH updates a previously journal-owned comment. Existing ownership
is App + numeric repository ID + PR, so reinstalling the same App does not create
another comment. A known comment is read and checked before an update. A copied
marker or body, another bot, an unrelated comment ID or an informational URL
cannot grant ownership. All request routes are built locally.

When the desired body already matches the last owned body, the publisher verifies
the exact remote comment and commits an atomic local observation. It sends no
redundant PATCH. That result reports `unchanged: true` and `writeConfirmed: false`.
An accepted remote write cannot be fenced by a later local lock or cancellation;
avoiding a redundant write keeps that case from creating ambiguous recovery.

Publication has no automatic retry. An exact request UUID retry returns the
original historical outcome, with zero new network requests and no renewed
current-head/token authority. Reusing the UUID for changed fields refuses. If a
current installation replaces the original one, exact historical retrieval still
requires a current grant for the same App, repository ID and canonical name; it
keeps the original immutable job and does not authorize another send. If a
response is lost, invalid, cancelled, or cannot be recorded after the intent has
committed, the durable outcome remains `unknown`. A remote comment may already
exist in that case. Retrying does not resend or clear the intent.

## Approved page links

An optional link must come from an already stored, unexpired
[protected native page](github-app-pages.md). Add the same closed reference to
both the preview and exact publication request:

```json
"page": {
  "pageId": "<native page ID>",
  "expectedPageVersion": 1
}
```

Preview and new publication additionally require `publication --allow-page-links`.
The caller cannot supply a URL, bearer token, HTML or replacement comment body.
Native admission authenticates the stored page, exact version, current grant,
captured source and graph, and derives the URL from the current host key. The
page must describe the same completed job and exact selected group revision.
Its original queue checkpoint and the publication's current checkpoint each
have a typed selected-job proof. Unrelated PR arrivals may change the global
checkpoint without invalidating that unchanged target selection.

The proposal uses renderer `yoke-pr-comment-v2` and includes the complete
`pageLink` approval packet. The exact escaped anchor and captured packet are
covered by the body hash and proposal ID. Review these new values before
approving `publish_exact_comment`. Without a page reference, the original v1
request, proposal and journal representation are preserved.

Fresh admission is repeated before a new send. The signed intent timestamp must
fall within the captured page lifetime. After sending, terminal persistence
checks the captured link and current selected source without requiring the page
to remain stored or unexpired. A successful HTTP response can therefore close
when the page expires or is removed while that response is in flight. Other
source, selection, clock, transport or persistence failures retain the existing
conservative unknown-outcome behavior.

Historical retries, local status, typed retention and read-only recovery verify
the immutable captured packet and both queue proofs. They do not need the live
page or new page-link permission. Status and recovery reject the unused
`--allow-page-links` CLI flag. Page removal, expiry and key rotation do not
rewrite an already published body. An old captured URL may consequently stop
serving; a new comment update needs a fresh page approval and its own exact
publication approval. These links attest captured structural evidence, not
public HTTPS reachability or a complete/current GitHub analysis.


## Status and recovery

Local status needs only the webhook host profile and this request:

```json
{"schemaVersion":1,"requestId":"11111111-2222-4333-8444-555555555555"}
```

```powershell
yoke --root C:\work\project github-app --allow-webhooks `
  --profile C:\host\webhook-profile.json publication status `
  --request C:\host\status.json
```

Status verifies authenticated history, a current repository grant and the exact
historical report's captured source. It reads no App key, makes no HTTP request
and writes no journal record. It does not require the old job to remain selected.
The CLI rejects write, network and publication flags for preview and status.

Recovery requires an explicit unknown intent, an operator-supplied candidate
comment ID and a currently granted installation:

```json
{
  "schemaVersion":1,
  "intentId":"<unknown intent record ID>",
  "commentId":501,
  "installationId":42
}
```

```powershell
yoke --root C:\work\project --allow-writes github-app --allow-webhooks `
  --allow-github --profile C:\host\webhook-profile.json publication recover `
  --request C:\host\recover.json --auth-profile C:\host\app-auth.json
```

Recovery needs journal write and network permission, but does not accept
`--allow-publication`. Its App profile must request only `pull_requests:read` or
`pull_requests:write`. It never POSTs or PATCHes a comment, mints a publication
permit, changes queue selection, reruns a worker or changes the immutable job.
It performs the four authorization requests, a repository GET and an exact
comment GET. The body hash, App attribution, bot actor, comment ID, repository and
URLs must match the recorded intent. A current installation for the same App
and repository may reconcile work from an older installation.

A new queue selection does not prevent historical reconciliation. A successful
reconciliation appends a signed terminal observation and releases that PR's
unknown-outcome block. It reports `reconciled: true`, `writeConfirmed: false`
and no current-head proof. Repeating the exact recovery returns history without
HTTP. A changed known comment ID refuses before HTTP. An absent or mismatching
candidate leaves the outcome unknown; the tool does not infer that the original
write failed or create a replacement. New publication requires its own current
worker completion, preview and exact approval.

## Durability, budgets and evidence

All journal writes require SQLite WAL with FULL or EXTRA synchronization. The
publisher refuses weaker settings before authorization and rechecks them at
each write; it does not silently change the caller's database policy. Write and
read entrypoints refuse an existing caller transaction without committing or
rolling it back. Owned transactions restore Store deadline/progress policy,
actual busy timeout and SQL length limit. Deadlines are checked after commit as
well: cancellation can be reported even though a later status read finds a
durable successful observation.

The owner lock uses an exclusive OS lock on `.yoke/github-publication.lock`.
Windows opens the file without delete sharing, preventing a second pathname
from bypassing a live owner by renaming the locked file. Contention waits only
until the inherited deadline. This serializes cooperating processes using the
same Store, not independent clones or arbitrary remote writers.

The closed `github_app_publication_v1` history uses a disjoint HMAC domain and
canonical content IDs. Replay validates sequence continuity, UUID and proposal
uniqueness, historical selected checkpoints, typed worker/report bindings,
deterministically rendered bodies, ownership transitions and host time floors.
Unknown schemas, forged/malformed records and missing source are errors.
Retention validates typed dependencies without OS keys or network and roots
only the worker's declared snapshots; it does not follow arbitrary IDs in JSON
or grant publication authority.

Journal defaults are 1000 records and 64000000 bytes. Reduce them with
`--max-journal-records` and `--max-journal-bytes`. Capacity reserves one bounded
terminal slot for every unresolved intent across all PRs, plus the proposed
intent and its terminal slot, before authorization. The reservation is deliberately
conservative. A full journal does not permit sending first and recording later.

Remote defaults are 35 scoped requests, 1000000 bytes per response and 8000000
total header/body bytes, in addition to authorization's separate four-request
contract. Reduce them with `--max-requests`, `--max-response-bytes` and
`--max-total-bytes`. Create reserves its one write request; update reserves its
comment GET and write; unchanged observation reserves only its comment GET.
Complete output including LF is admitted before durable transitions. Zero limits
refuse before host files, credentials or Store opening. There are no redirects,
proxies, automatic retries, model calls or source writes.

Passing owned numeric-loopback provider fixtures does not validate a live GitHub
installation. Current limitations include fail-closed handling of nullable App
attribution, deleted owned comments, and historical recovery after a repository
rename/transfer. Closed-discussion digests remain separate work. Protected native pages and
exact approved links have their own scoped development checks. No real comment or external deployment was created during this
development checkpoint.
