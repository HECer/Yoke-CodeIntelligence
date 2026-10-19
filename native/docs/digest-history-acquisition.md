# Atomic captured-history acquisition

`digest-history-capture` automatically selects and captures a bounded batch of
historical Git source, then maps those captures before publishing the result.
It starts from an existing, verified native Git snapshot. No additional snapshot
IDs need to be collected by hand for that batch. The result can also supply the
explicit selectors used by [offline history mapping](digest-history-mapping.md)
and source/discussion/service composition.

This is one atomic batch per invocation. Persistent whole-history jobs,
checkpointed automatic continuation across batches and unattended App integration
remain open. The operation does not claim whole-repository/history completeness
or proprietary upstream equivalence.

## Host admission

Use the existing closed digest request containing `schemaVersion`, `snapshotId`,
`installationId`, `repositoryId` and canonical `repository`. The snapshot must
already have exact native commit/tree/blob/graph/capture proofs and a verified
direct native disk origin. The current host App profile must grant the exact
installation/repository scope. Its signing credential is not read.

```sh
yoke --root /project --allow-writes --max-bytes 4000000 \
  digest-history-capture --profile /host/app.json --request /host/digest.json \
  --allow-capture --allow-history --trust-git --max-commits 16
```

All three host permissions and global state-write consent are required before
profile/request/executable/Store access. Profile/request paths must be absolute.
The command opens existing state, never initializes a missing Store or indexes
the current checkout. Source/capture/analyzer/LSP flags are unavailable. Optional
`--git` selects an absolute trusted executable outside the repository; otherwise
the existing host-PATH discovery policy applies.

The library entry point is `digest::acquisition::acquire`, with host-only
`acquisition::Options` whose capture/write permissions default to false. Explicit
library invocation selects the trusted `HostGit` and independent history,
acquisition and replay limits. Caller-owned SQL transactions are refused.

## Source and selection

The existing source replay validates the selected head before Git acquisition.
History follows the existing bounded, hash-verified committer-date-priority
traversal and its non-merge/non-empty-message row rules. The batch selects those
commit IDs plus their direct parents, removes duplicate IDs and retains the
already captured head. Missing objects, unsupported source proof/epoch, revoked
grants or an oversized selection refuse the operation. They do not become an
empty complete result.

Every additional capture uses that head's proven analyzer epoch (8 or 9) and
exact disk-origin capture scope. It reads and verifies immutable Git commit,
tree and blob objects. Current checkout bytes, ignore settings and caller-supplied
capture options cannot widen that scope. Committed ignore policy, path/type/mode
checks, secret/binary omissions and exact UTF-8/CRLF byte preservation retain
the existing native Git capture behavior. No checkout, reset, Git ref/index/config
write, project execution, provider/network operation or credential lookup occurs.

The resulting `native_captured_repository_history_acquisition` report contains
the request, proven head/origin/epoch, `acquiredCaptures`, `capturedSnapshotIds`,
original history/coverage and commits with the existing `historyMapping` projection.
The mapping replays every source/graph/proof and preserves exact original anchors,
duplicate declarations and directed syntax candidates. A missing selected child
or parent mapping prevents publication. Semantic identity, compiler/provider
verification and whole-result `complete` remain false.

History traversal limits remain independent: `--max-commits`, `--max-visits`,
`--max-frontier`, `--max-parent-ids`, `--max-commit-bytes` and
`--max-history-bytes`. Their original coverage/frontier counts remain visible.
Selecting zero commits produces no additional capture and reports
`stateWritten:false` and `complete:false`.

## Atomic publication and recovery boundary

WAL with FULL/EXTRA synchronous durability is required. One owned immediate
transaction pins head/source metadata and contains every selected snapshot,
graph, FTS symbol/document row, publication timestamp, inherited scope proof,
commit-artifact registration and source receipt. Fresh mapping and exact encoded
output admission happen before commit. A proof, SQL, replay, capacity or
deadline failure before commit rolls back the entire batch, including fully prepared earlier
targets. The transaction spans trusted local Git metadata reads; no transaction
spans a network request. The original absolute deadline and host cancellation
remain active, and the caller's SQL length/deadline/busy policy is restored.

CAS files are durable content-addressed writes outside SQLite. A failed batch
can leave unreferenced CAS bytes; it grants no snapshot, graph or source authority
to those bytes and does not delete them as rollback. This is application SQL
atomicity, not a filesystem-wide transaction or encryption/DLP guarantee.

An output failure after commit can hide a successful acknowledgement. Repeating
the same explicitly authorized immutable-head request revalidates the objects and
returns the same capture IDs/result without adding snapshots. Existing snapshot,
graph, scope and source-receipt collisions refuse; they are not overwritten with
new claims. No latest pointer is changed. The ordinary single-commit
`capture_commit_at` API now also publishes its snapshot/graph/scope/artifact/receipt
inside one owned transaction and refuses caller transactions.

## Independent aggregate bounds

Acquisition limits do not reset per commit and are admitted before retaining
object bodies or candidate file content:

| CLI option | Default/hard ceiling |
|---|---:|
| `--max-snapshots` | 32 including the existing head |
| `--max-files` | 10,000 candidate blobs |
| `--max-source-bytes` | 32 MB candidate source/policy bytes |
| `--max-capture-commit-bytes` | 16 MB commit-proof bytes |
| `--max-tree-bytes` | 16 MB tree-listing bytes |
| `--max-tree-entries` | 100,000 tree entries |
| global `--max-bytes` | 4 MB including the final LF |

The CLI defaults to 16 selected history commits. A branching history can still
exceed the distinct capture ceiling and then refuses atomically; no capture is
silently dropped. Candidate acquisition includes policy-only and subsequently
omitted text, so it may exceed published file/byte counts. Reports separate these
admission counters from source replay/mapping work. Existing per-object 1 MB
commit, 4 MB listing/file and 64 MB per-commit candidate source limits remain.

Head validation and final mapping independently enforce their shared aggregate
source, metadata, lineage, entity, AST, comparison, relation and output bounds
across all selected captures. They do not reset those limits for each snapshot.
Re-running a batch with existing snapshots still consumes acquisition/replay
allowances; stored identity is not a substitute for current proof verification.
These are cooperative application limits, not a hard OS memory/CPU sandbox.

No public deployment, live GitHub App installation, model-assisted reasoning or
remote source ownership/verification follows from local acquisition.
