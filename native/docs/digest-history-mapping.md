# Explicit captured history mapping

The digest history reader can associate already captured Git source snapshots
with its hash-verified commit rows. Select the head in the existing closed
digest request and pass additional snapshot IDs through host CLI arguments:

```text
yoke --root ABSOLUTE_PROJECT github-app --profile ABSOLUTE_PROFILE digest --allow-digest history --allow-history --trust-git --request ABSOLUTE_REQUEST --map-captured-history --captured-snapshot PARENT_SNAPSHOT_ID --captured-snapshot EARLIER_SNAPSHOT_ID --max-bytes 4000000
```

`--captured-snapshot` requires `--map-captured-history`. The head snapshot is
implicit; duplicate IDs, a repeated head, malformed IDs, and two different
captures of the same commit are refused. Additional captures must bind selected
history commits or their direct parents. A disk snapshot or an unrelated branch
does not become history authority. The selected commit is checked before reading
its captured source blobs. Existing capture/graph/commit/tree/blob proofs and
supported analyzer epochs are replayed; caller-supplied source bodies, commit
claims, paths and graph rows are not accepted.

The library entry point is `digest::preview_mapped_history_with_profile`, using
an explicit `MappingSelection` and `MappingOptions`. Ordinary preview/history
calls retain their existing contracts and do not infer or discover extra source
captures. A current host grant is required; no App signing credential is read.

## Results and evidence

The result kind is `native_captured_repository_digest_mapped_history_preview`.
Every selected non-merge commit retains its original identity, message, order
and parent IDs. A row without a selected child/parent capture reports
`unavailable_commit_snapshot` or `unavailable_parent_snapshot`; empty arrays in
those rows are not evidence of no changes. A root commit has a proven absent
parent and needs no synthetic snapshot. Merge and empty-message selection rules
remain those of the bounded history reader.

File relations enumerate changes only over paths captured on either side.
Verified complete Git tree proofs establish path presence, object identity,
type and mode on each side. Rows retain snapshot/commit/tree IDs, captured
content hashes and byte counts. A path still present in Git but unselected by a
capture has `captured: false`; omission does not establish deletion. Unchanged
paths are not emitted. `allTreeChangesEnumerated` stays false because paths
outside both selected captures are not exported.

Symbol rows retain each original source-replayed declaration ID, producer,
snapshot, commit, exact UTF-8 byte range, content hash, full declaration
fingerprint and body-free signature. File-local qualified inline-module names
separate equal names in different modules. Same-file/name/kind associations are
syntax candidates; remaining exact fingerprint groups can expose cross-path
candidates. Duplicate groups preserve every original symbol in ordered
`before`/`after` arrays and report `ambiguous_syntax_candidates`. No Cartesian
pair expansion or forced identity occurs. One-sided rows refer only to the
export projection. If the counterpart source is uncaptured or its export policy
is unsupported, the relation explicitly reports an unavailable projection.

`declarationBytesEqual` compares complete captured declaration fingerprints,
including implementation bytes deliberately absent from signatures. It does
not prove a rename, semantic identity, compiler accessibility or execution.
`semanticIdentityEstablished`, `compilerVerified`, `providerVerified` and
`complete` remain false. Rust export projection still covers explicit file-local
public functions, inline modules, constants and statics; other declaration and
language policies remain unresolved and counted.

## Capacity and authority

All selected captures, including the head, share the existing file, source byte,
snapshot/graph metadata, capture-lineage metadata, entity, AST work, replay-name
and exported-symbol ceilings. Limits do not reset for each snapshot. Mapping
requires untruncated export projections; an exhausted symbol budget refuses the
whole operation. A separate aggregate bound covers registered captured commit
proof bytes, admitted before reading their artifact payloads.

Default mapping ceilings are 32 snapshots, 20,000 file relations, 8,000 symbol
relations, 200,000 comparison work units and 32 MB of captured commit proofs.
Library host options can lower each ceiling, including to zero. The existing
commit traversal and exact JSON output budgets remain independent; the CLI
budget includes its final LF. A capacity failure emits no partial digest.
Relation rows are admitted incrementally under a shared exact encoded-byte
ceiling before retaining them; the final encoder still checks the complete
result. `relationRowBytes` counts compact JSON for those rows, excluding framing
and other result fields. Metadata and entity/AST counters describe capacity
admissions; they do not measure physical disk I/O or summed CPU operations.

One read transaction pins all selected SQL metadata. Source bytes come from
verified captured CAS, not the current checkout. Caller-owned transactions are
refused; SQL length/busy/deadline policy is restored on every return path. There
is no source capture, CAS/SQL publication, provider request, model call, project
execution, credential lookup or automatic historical acquisition. SQLite SHM
coordination for the existing-only Store is distinct from logical publication.
