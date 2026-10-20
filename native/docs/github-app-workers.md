# Durable native GitHub review workers

Durable workers extend the signed webhook queue with claims, recovery, release,
status and explicit native execution against existing source snapshots. Separate
[controlled publication](github-app-publication.md) and
[closed-discussion digest](github-discussions.md) contracts are available.
Explicit [App-bound history execution](app-history-integration.md) can drive
persistent local acquisition after a durable completion. Configured live
installation/deployment remains open. A worker completion is a source-bound structural report; publication
and semantic reasoning retain their own contracts and acceptance requirements.

## Claim and recovery contract

The host must explicitly grant Store writes before claiming or releasing work.
Only a selected review job at the observed queue group revision can receive a
fresh claim. The keyed Host must permit that App, installation and repository.
The worker does not infer an active selection from a historical webhook title,
draft flag or another pull request's checkpoint.

Claims bind a canonical host request UUID, worker identifier, job, queue revision,
historical queue checkpoint, claim time, expiry and increasing per-job fence.
The host clock supplies time; request JSON cannot supply or extend it. A competing
claim is refused until the current claim is released or reaches its expiry.
Taking over expired work increments the fence, so the previous owner cannot
release the successor's claim. Completion is terminal for that immutable job;
a fresh claim cannot erase or rerun its completion. A later queued head is a
different job and needs its own explicit selection.

Exact claim retries return the original ID, fence and expiry. Exact release
retries return the original release receipt even after a later claim exists.
Those retries are explicitly historical and do not restore execution authority.
Rebinding a request UUID to changed claim fields or another event type is refused.
UUID scopes are separate for each App sharing a Store.

Every keyed operation replays the bounded history and checks the current App's
work MACs and webhook signatures, including configured rotation keys. Work MACs
use a separate domain from webhook signatures. Other Apps' histories receive
structural checks without granting authority through their unknown keys.

The same `Clock` capability refuses time older than any of its prior observations,
including read-only status observing an expired lease. Across a fresh capability,
process or restart, only persisted transition times establish that time floor.
Read-only status does not persist a new clock observation. Operators must use a
trustworthy host clock; restart cannot prove that a previously observed wall time
has not been reversed.

## Storage and operational bounds

The closed `github_app_work_v1` history is workspace-bound, content-addressed and
signed. Work records use a canonical JSON field order. Sequence continuity,
request uniqueness, historical queue selection, time and fence transitions are
validated on every replay. Invalid history is an error, not an empty result.

Defaults admit at most 1000 records, 64 MB of stored JSON, a 900-second lease and
64 KB of output. The output budget includes the final CLI newline. A complete
receipt must fit before SQL insertion. Capacity or output refusal leaves no new
work record.

Owned write transactions use `BEGIN IMMEDIATE` and a maximum 50 ms SQLite busy
wait, also bounded by the request's remaining time. They contain replay,
preconditions, output admission and the durable insert; they do not span remote
work. Writes refuse an existing caller transaction. Reads preserve an existing
caller transaction. On success or refusal the operation restores the prior
Store-owned deadline and progress handler, actual SQLite busy timeout and SQL
length limit after its statements and owned transaction have left scope.
Arbitrary externally installed raw SQLite callbacks are outside this ownership
contract.

Retention validates the complete closed worker history without OS keys or network
access. Current claim/release events grant no snapshot or CAS root authority;
Git IDs, signatures and request text are never traversed as generic references.
Structural retention validation does not authenticate work or authorize a write.

## Explicit native execution

`RunRequest` contains schema version 1, a durable claim ID, an existing head
snapshot ID, an optional existing base snapshot ID and a canonical request UUID.
It accepts no credential, endpoint, clock, model or project-execution selectors.
Unknown fields and duplicate JSON keys are refused.

Execution requires explicit write and GitHub network permission. It refuses an
existing caller transaction and synchronous invocation inside Tokio. Exact
completed-request retries are checked before source preparation or authorization;
changed fields under the same UUID refuse before token minting or remote reads.

Preparation loads only the specified immutable snapshots and graphs. It validates
captured bytes, graph anchors and native capture/derived/overlay scope provenance
in an owned read transaction, then closes that transaction. It seals the exact
snapshot, graph and scope rows, including absent scope records and at most 16
lineage members. Metadata admission is 64 MB per row and 128 MB in aggregate.
There is no automatic capture, source write or intermediate artifact publication.
Active selection, grant, owner, fence and expiry are refreshed after preparation
and before token minting. No transaction spans HTTP or the impact computation.

Authorization retains its separate four-request contract. An opaque installation
lease is bound to its original Client, origin, App, installation, repository and
permissions. The worker reader needs pull-request read permission; it does not
require contents permission for existing local source analysis. Token time uses
the auth client's clock, with a monotonic observed floor and at least 30 seconds
of remaining life checked before and after every GET and before a new completion.
Tokens and JWTs are never serialized or hashed into work history.

The reader observes repository identity, initial PR metadata, exactly the file
pages implied by `changed_files`, and final matching PR metadata. Open state,
explicit non-draft state, queued head/base IDs, base repository identity, file
count and optional fork identity must agree. All routes are built locally from
typed authority. Bounded Link metadata is checked against the fixed origin and
expected pages; no supplied URL is followed. At most 3000 files and 33 reader
requests are admitted, in addition to authorization's four requests. Defaults
bound each read response to 1 MB, total response bytes to 8 MB and the closed
report to 3 MB. Final output must also fit the caller's worker output budget.

Initial/final PR observation times are host token-clock checks after complete
metadata responses. They are not server timestamps. Matching observations do not
make the remote PR atomic with local SQL, attest whole-tree equality or prove a
merge will succeed. Token minting remains a remote side effect if a later step
refuses. A racing exact duplicate may reuse a historical report after performing
HTTP; its `networkRequests` still counts that attempt's actual requests.

The pure source impact path checks exact captured UTF-8/CRLF bytes against reported
Git blob SHA-1 values, then verifies available patches and byte anchors. Only its
private checked-file capability can establish a matched remote blob; a local JSON
`sourceBinding` emitted for a mismatch is insufficient. At least one checked blob
is required. Removed files are analyzed only with an explicit base snapshot and
are labeled with `reportedBaseSha`. Missing base proof, source mismatches, binary
or unavailable patches, and excluded paths remain named omissions/candidates.
Community analysis is explicitly not run by this pure worker path. Compiler,
semantic and full-review certainty remain false.

A fresh Immediate transaction rechecks the claim, selected group revision,
grants, clock, expiry, source-row seals and captured source bytes. It admits the
complete output before inserting a signed closed completion. Token and lease
time are checked again before insertion and commit. The 50 ms SQL policy bounds
busy waiting, not total hashing or filesystem/commit latency. Source checks are
observations of immutable local state, not a transactional remote filesystem.

Historical result retrieval freshly checks only that selected report's source
dependencies. It performs no HTTP and exposes no current-head, token or execution
authority. Ordinary status verifies authenticated historical records without
rehashing every old source dataset. Retention validates closed report shape,
hashes and typed snapshot/binding metadata; it roots only explicit head/base
snapshots. It neither reattests every source byte nor walks inert report JSON,
Git IDs, MACs or arbitrary embedded IDs for retention authority.

## CLI

The `github-app worker` group exposes `claim --request FILE`, `release --claim ID
--request-id UUID`, `recover --claim ID`, `status`, and `run --request FILE
--auth-profile FILE`. The explicit webhook host profile remains `--profile FILE`.
Claim, release and run require `--allow-writes` and `--allow-webhooks`; run also
requires `--allow-github`. Status and recovery refuse write/network flags. Every
permission and zero budget is admitted before host files, credentials or Store
opening. Worker-specific zero checks do not change ordinary queue zero semantics.
All supplied request/profile files are parsed before either credential is read.
The source Store must already exist; these commands never index a project.

For example, with host profiles and requests prepared separately:

```powershell
yoke --root D:\repo --allow-writes github-app --allow-webhooks --profile C:\host\webhook.json worker claim --request C:\host\claim.json
yoke --root D:\repo github-app --allow-webhooks --profile C:\host\webhook.json worker status
yoke --root D:\repo --allow-writes --max-bytes 4000000 github-app --allow-webhooks --allow-github --profile C:\host\webhook.json worker run --request C:\host\run.json --auth-profile C:\host\auth.json
```

## Evidence boundary

Claim/release/status report no source writes, publication, network requests, model
calls or worker execution. A selected historical queue proof is provenance;
execution additionally requires fresh selected-revision, grant, claim-owner,
fence, expiry, source and provider checks before durable completion.

The scoped runtime checkpoint passed 162 native tests with 2 explicit credential
test ignores across 12 targets, including 33 state/recovery, 28 execution and 5
worker CLI tests. They exercise two-Store races, loopback pagination and
cancellation, source mutation, typed retention, real writer contention, SQL
policy restoration, and permission/budget ordering. TypeScript checks passed with
254 tests and 3 skips. These counts do not imply final all-target Clippy or a
positive owned CLI roundtrip; those require their own source-bound receipts.
Historical failures, corrected test evidence and final checks must be distinguished
using their actual source maps and logs. These checkpoints do not constitute
primary integration, full Windows acceptance, live GitHub validation or universal
parity.
