# Persistent local history acquisition

The `github_app::digest::history_jobs` library contract checkpoints explicit local
Git acquisition across bounded steps. It does not start an unattended service.
All operations require a current host profile and explicit job permission;
mutations additionally require capture/write authority. These permissions are
never serialized into a job. No App signing key is needed.

The CLI uses existing source state and absolute host-file paths:

```sh
yoke --root /project --allow-writes --max-bytes 4000000 digest-history-job --profile /host/app.json --id JOB_ID --allow-jobs --allow-capture create --request /host/head.json
yoke --root /project --allow-writes --max-bytes 4000000 digest-history-job --profile /host/app.json --id JOB_ID --allow-jobs --allow-capture --trust-git run --revision 0 --max-batches 8
yoke --root /project digest-history-job --profile /host/app.json --id JOB_ID --allow-jobs status
```

`JOB_ID` is a host-chosen 64-character lowercase hexadecimal identity. An optional
absolute `create --plan /host/plan.json` supplies the closed `Plan` schema and
lower limits. `step --revision N` publishes one batch. `run` accepts at most 128
batches, shares the caller's original deadline, and returns only its final batch
result; it does not combine all historical rows into one response. Earlier
successful batches remain durable if a later batch refuses. Read status before
resuming, and supply its revision explicitly. `pause`, `resume` and `cancel`
also require `--revision N` and current write/capture permission.

Status uses existing-only read-only Store access, with no schema creation or
Engine setup. Capture, source/analyzer configuration, semantic-provider flags
and global snapshot overrides are unavailable. A missing store refuses.

A job binds its host-selected ID, workspace, App grant, captured head, direct
disk origin, analyzer epoch and cumulative acquisition limits. Creation is
idempotent only for the same original request and plan. Each step, pause,
resume or cancellation requires the exact current revision. A lost stdout
response is resolved by reading status, rather than repeating an old revision.
Complete and cancelled jobs cannot resume. Their recorded source references
remain retention roots; cancellation is not deletion.

The durable traversal stores raw commit-object bodies and the ordered visited
prefix. It stores no authoritative priority queue or exhaustion flag. Replay
hashes and parses each discovered object, reconstructs parent discovery and
committer-date priority with parent insertion order for ties, and verifies every
recorded visit. All pending dates must be verified before another visit is
chosen. Unrelated proofs, missing proofs for visits, reordered visits and unknown
fields refuse. A step may checkpoint lookahead without visiting a commit when
its object-count allowance ends inside a wide frontier.
The same rule applies when the next object fits the per-batch proof limit but
not the allowance remaining in the current batch: retain the admitted lookahead
and continue in a later step. An object that cannot fit a fresh batch refuses.

Selected non-merge/non-empty-message commits and their direct parents are
captured. Existing references are reused; the sources mapped by each batch are
freshly replayed. Snapshots, graph/FTS rows, scope inheritance, source receipts,
job progress, revision and exact output admission share one immediate SQLite
transaction under WAL and FULL/EXTRA durability. Any failure rolls back the
entire step. Unreferenced CAS bytes may remain and carry no source authority.
The original checkout, Git refs/index/config and latest snapshot are unchanged.

Default cumulative ceilings are 16,000 verified commit objects, 16 MB raw
commit proofs, 64,000 parent IDs, 4,000 frontier entries, 16,000 captures,
one million candidate files, 1 GB admitted source, 256 MB capture commit bytes,
256 MB tree listings and one million tree entries. Host plans can lower these
ceilings. A step defaults to eight visits, 32 lookahead objects and 2 MB new
raw proof bytes, with the existing independent per-batch capture/mapping limits.
At most eight job records and 64 MB aggregate serialized job metadata are admitted;
an individual job is limited to 32 MB serialized JSON. Capacity refuses instead
of reporting exhausted history.

`traversalExhausted` means the verified parent closure has no pending/ready
objects. The status's `recordedCaptureReferences` reports saved publication
progress; status does not reread every historical source body. Its
`sourceBodiesReplayed` is false. Batch mapping separately marks its actual source
replay. Source-query consumers retain their own immutable source checks.
Retention validates closed source selectors and actual source receipts; it
never treats commit messages, Git IDs or arbitrary imported JSON as local roots.

Every result retains `complete:false`, zero model/network calls and explicit
limitations. No semantic identity, compiler coverage, remote authorship,
production App installation, proprietary service equivalence or whole-platform
acceptance is established by traversing a local Git parent closure.
