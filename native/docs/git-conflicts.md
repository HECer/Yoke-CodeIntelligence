# Observed Git index conflicts

`git conflicts` inspects the unmerged index of an explicitly selected host
worktree. This supplies actual index observations in addition to the existing
captured-report rename, quote and hierarchy candidates. It does not perform a
merge, edit a file, resolve a conflict, follow another worktree's working paths,
capture new source, publish state or invoke a model.

The selected root must be the Git worktree's top-level directory and already
have an initialized Yoke Store. Linked worktrees work through Git's normal
per-worktree metadata resolution. Each worktree is selected explicitly by
`--root` and uses its own existing Store; registry paths never become read grants.

```sh
yoke --root /worktree --max-bytes 4000000 git conflicts --allow-index-read --trust-git
yoke --root /worktree --max-bytes 4000000 git conflicts --allow-index-read --trust-git --allow-git-sources --scope-snapshot SNAPSHOT_ID
```

Index reading and Git execution require separate host flags. Reading stage
bodies additionally requires `--allow-git-sources` and one to four explicitly
selected native scope snapshots. Their immutable manifests and complete native capture
scope lineage are admitted before Git runs. Every quoted path must pass at least
one selected captured-path policy. These snapshots constrain access; they do not
assert that current index bodies equal a canonical captured file or graph.
No captured CAS source body or current working file is read by this operation.
Imported snapshot labels, including a derived wrapper around a foreign origin,
cannot grant access to local Git bodies. Native disk, Git-derived and editor
overlay lineage retain their checked host capture policy. Explicitly captured
read-only dependency paths remain readable without becoming writable.

The first command observes metadata only. It preserves each stage position and
marks body access unavailable. The second can verify and quote permitted Git
stage objects. Protected/noncanonical path names are omitted with their original
path SHA-256 and row ordinal retained. Missing scope membership, unsupported
modes, unsafe text and unrequested bodies have explicit reasons. Cached content
from one permitted path cannot authorize another path.

Git documents up to three index entries for an unmerged path: the original
version in stage 1 and the two merge sides in stages 2 and 3. `-z` keeps path
bytes unquoted and separates records with NUL. See the primary
[ls-files contract](https://git-scm.com/docs/git-ls-files) and
[read-tree stage model](https://git-scm.com/docs/git-read-tree).
The result calls these roles `base`, `index_ours` and `index_theirs`; they are
index roles, not authenticated branch or author identities. Stage presence,
absence and identical object/mode pairs are facts about that observed index.
They do not infer the cause of a conflict or an automatic resolution.

Two complete unmerged listings and object-format observations must match before
a result is released. The operation does not lock the index, promise future
freshness, or detect an intervening change which restores the identical observed
entries. It does not inspect conflict markers in working files. A file whose
markers were manually removed can still have unmerged index stages.

## Exact source pages

Large stage bodies can be read in bounded UTF-8 pages. Supply an absolute host
JSON path through `--read-source /host/page.json`, with the source permissions and
scope snapshots above:

```json
{
  "schemaVersion": 1,
  "expectedIndexHash": "SHA256_FROM_THE_PREVIOUS_RESULT",
  "path": "src/example.rs",
  "stage": 2,
  "startByte": 0,
  "maxBytes": 16384
}
```

`expectedIndexHash` is exactly 64 lowercase hexadecimal characters. The request
must match the current complete unmerged listing before any body is read.
An absent requested path/stage or unavailable source refuses instead of returning
an empty source. Only the requested stage body is fetched in page mode. Full
raw object bytes are size-admitted, read and hash-verified before a page is
released. Byte offsets belong to `git_blob_utf8`, with stage-qualified object and
content identities; they are never canonical snapshot or editor coordinates.
The start must be an exact UTF-8 boundary. The returned end respects both byte
budgets and UTF-8 boundaries, and `remainingSourceBytes` supports the next page.
A positive request unable to fit its next code point refuses. Explicit zero-byte
requests preserve exact empty intervals and do not imply an empty complete file.

Default previews quote a bounded prefix per safe text occurrence. Repeated
occurrences retain separate rows and consume quote credit independently; blob
read credit is shared only by verified object ID. Invalid UTF-8, unsupported
control text and obvious secrets are verified but not quoted. Symbolic links,
gitlinks and tree modes remain metadata-only; no target or submodule is followed.

## Limits and storage

`--limits /host/limits.json` accepts a closed object containing all eleven fields
below. Every numeric default is also its maximum. Unknown/missing/duplicate keys,
wrong types and values above maxima refuse; JSON cannot grant host permissions.

| Field | Default / maximum |
| --- | ---: |
| `maxPaths` | 2,000 |
| `maxStageRows` | 6,000 |
| `maxIndexBytes` | 2,000,000 per observation |
| `maxBlobObjects` | 6,000 distinct objects |
| `maxBlobBytes` | 4,000,000 |
| `maxSourceBytes` | 32,000,000 distinct verified blob bytes |
| `maxQuoteBytes` | 16,384 per occurrence/page |
| `maxTotalQuoteBytes` | 256,000 |
| `maxScopeFiles` | 20,000 across selected scope/lineage manifests |
| `maxScopeMetadataBytes` | 16,000,000 preflight bytes |
| `maxOutputBytes` | 4,000,000 canonical result bytes |

Zeros remain zero. Per-blob and aggregate source allowances precede body capture.
Scope preflight counts complete selected lineage rows before the shared resolver
runs; its counter is not total inherited SQL I/O or a hard memory/scheduler quota.
The SQLite row ceiling, deadlines and original cancellation bound retained reads.
The caller's SQL length/busy/deadline policy is restored on success and failure.
Caller-owned transactions are refused. One owned read transaction pins metadata;
no SQL/application/CAS/source writes or latest-pointer changes occur.

Global `--timeout` is 1–600,000 ms. Global `--max-bytes` includes the CLI newline
and is independent of `maxOutputBytes`. The complete canonical bytes plus one LF
must fit before stdout is written. Snapshot/capture/analyzer/LSP/write overrides
are unavailable; there is no automatic Store initialization or latest selection.

The sealed Rust `ConflictReport` exposes `value()` and `canonical_bytes()` but
has no public constructor or deserializer. `resultId` hashes the whole typed
result with only that field omitted. Arrays preserve original index positions.
Results retain `complete:false`: a complete observation of unmerged index rows
does not attest repository-wide merge safety, working-file resolution, compiler
behavior, ownership or semantic impact.
