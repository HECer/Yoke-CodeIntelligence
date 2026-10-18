# Native captured-source digest development

The `github_app::digest::preview` library function projects a host-selected
captured native disk or locally proven immutable Git source snapshot into bounded
declaration metadata. This is an
isolated development component. The explicit history reader adds bounded,
captured-head commit messages. Exact most-recent history selection, historical
file/symbol mapping, provider discussions, service handoff and universal export
parity remain open.

## Development checkpoint — 2026-10-18

This branch is a development checkpoint, not a release or an integrated Windows
acceptance result. The latest owned-Git cancellation test demonstrated that a
descendant could remain alive after the digest returned. The tracking-job cleanup
implementation and additional held-handle regressions are present, but have not
yet passed compilation and runtime verification together.

Known pending corrections include preserving typed cleanup failures through
host cancellation/deadline and digest error mapping, recognizing cleanup failures
inside spawn I/O errors, and independently terminating an unassigned suspended
root when job termination is refused. Exact chronological history selection,
historical file/symbol mapping, discussion ingestion and service handoff remain
incomplete. Added regression tests are not evidence of passing behavior.

The follow-up test run was stopped during compilation by the system-memory
guard; subsequent attempts were refused for insufficient memory headroom.
These attempts provide neither a test pass nor a behavioral failure result.
Build outputs, temporary test files and new local receipts now use Q:. Earlier
scoped passing checkpoints remain historical and do not certify this source.

## Current interfaces and boundaries

The explicit host command is:

```powershell
yoke --root C:/work/project github-app --profile C:/host/app-profile.json digest --allow-digest preview --request C:/host/digest-request.json
```

The profile and closed request must use absolute host-selected paths. The
current profile grant is checked before opening the existing Store and again
at the read boundary. No App signing credential is read. Network, webhook,
write, capture and language-server authority are unavailable for this command.
Queue-only configuration is refused. The CLI's exact output budget includes
its final LF; a capacity error emits no digest bytes.

The existing-only Store opener uses read-only SQLite and existing client leases.
It does not create directories, lease files, schema or workspace identities or
change journal mode. Its first SQL/schema reads and lease acquisition obey the
host deadline. Live WAL data remains visible; SQLite SHM coordination is distinct
from logical SQL, source or CAS publication.

The separate history operation requires explicit metadata-read and host Git
execution authority:

```powershell
yoke --root C:/work/project github-app --profile C:/host/app-profile.json digest --allow-digest history --allow-history --trust-git --request C:/host/digest-request.json
```

`--git` optionally selects an absolute host executable outside the repository;
otherwise discovery searches absolute host PATH entries. The current grant and
complete captured Git source proof precede history acquisition. Disk captures
cannot supply Git history. No signing credential or provider connection is used.

History starts at the verified captured commit, independently of current HEAD.
It walks parent IDs in breadth-first, recorded-parent order and reverses selected
non-merge rows. This deterministic order is not exact most-recent chronological
selection. Every visited object, including skipped merges, is size-admitted,
length-framed and hash-verified. Git revision walkers and tree-diff traversal
are not invoked. The same host deadline covers each information/content call,
parsing and final encoding. Unknown object kinds, missing/corrupt objects and
unsupported encodings fail with static errors rather than a successful empty
history. UTF-8 message control characters do not act as record delimiters.

Default hard history ceilings are 1,000 selected commits, 4,000 visited commits,
4,000 queued commits, 16,000 parent-ID occurrences, 1 MB per raw commit object
and 64 MB aggregate raw commit objects. Host options may lower each ceiling.
Selection and visit boundaries report their remaining frontier and reason;
resource-capacity failures refuse the complete output. Exhausted traversal is
reported separately from the overall `complete:false` result.

Commit messages are untrusted repository metadata and may contain source code
or secrets. The source-header body exclusions below do not apply to explicitly
requested commit messages. Each commit reports file differences and symbol
associations as unavailable; empty arrays do not establish an empty change set.
Object hashes verify bytes and parent links, not authorship, signatures or a
remote repository. Discussions and service handoff remain unavailable.

Shared lease polling recognizes the platform contention error returned by fs2,
including Windows lock violations. Host cancellation interrupts read-only opens
and maintenance admission waits before the held lease is released. Maintenance
also checks cancellation after installing its lease-restoration guard and before
invoking the operation; another open client continues to refuse an upgrade.

The closed request binds schema version, snapshot ID, installation ID, numeric
repository ID and canonical repository name. The current host grant must match
all repository fields. The snapshot is scoped to the actual Store workspace.
No incoming URL, source body, token or caller-supplied digest is accepted.

Git source admission verifies the native receipt, exact manifest and workspace,
capture inheritance, registered commit artifact, Git commit/tree identities,
committed file membership, modes and blob identities before exposing a private
retained-source capability. Supported replay epochs are 8 and 9. Source files
are read once from captured CAS, independently of the current checkout or `.git`.
Legacy epoch 4/6 validation and archive readers retain their original policy.

The preview requires a direct native disk origin for a Git capture. Its complete
receipt, inheritance, origin manifest and origin capture-proof rows share a
separate 16 MB metadata budget, checked before recursive scope validation or
commit/source reads. Disk previews also budget their capture-proof rows and
refuse derived, overlay or Git selector rows. Longer Git origin chains are
explicitly refused by this preview; general legacy readers are unchanged.

The operation refuses a caller-owned SQL transaction. Its own read transaction
pins metadata while captured CAS bytes are read and hashed once per file. Host
deadlines and cancellation cover parsing, replay, SQL lock polling and exact
output encoding. The caller's Store deadline, SQLite row-length limit and busy
timeout are restored on success and failure. No source capture, SQL publication,
CAS insertion, network request, model call or project execution is performed.

For native `.rs` files classified as Rust, the recorded supported analyzer epoch
selects declaration replay over the already parsed syntax tree. Fresh declaration
IDs and complete native facts must match stored rows. The native signature rule
is shared with the existing full analysis, including UTF-8-safe 4096-byte
truncation and method-kind finalization without key changes. Replay excludes
call resolution, rationale and document contracts. These graph facts and graph
completeness are not attested. Other captured source blobs are verified but
their export policies and entities remain unsupported.

Exported rows contain explicit `pub` declarations reached through visible inline
module ancestry in each file. This is file-local syntax evidence. Crate roots,
out-of-line modules, reexports, conditional compilation, macro expansion and
compiler accessibility remain unknown. Trait and implementation APIs and other
unsupported declarations are counted as unresolved. Rust `static` declarations
not represented by the selected native graph have a distinct source-bound
producer and identity, with a null graph entity ID.

Signatures omit function/module bodies, constant/static initializers and header
comments. Expression-bearing headers, generic defaults and quoted literals are
omitted as unresolved; exact `extern "C"`, `extern "system"` and `extern "Rust"`
ABI strings are allowed. Each emitted fingerprint hashes the full captured
declaration span, including bytes deliberately absent from the signature.
Decoder failures use static public errors and cannot echo stored field values.

Independent default hard ceilings are 10,000 files, 32 MB captured source,
200,000 stored entities, 64 MB selected snapshot/graph metadata, one million AST
nodes, AST depth 128, 16 MB conservative replay-name/text work, 4,000 emitted
symbols and 4 MB exact JSON output. Capture lineage metadata has the independent
16 MB aggregate ceiling described above. Limits may be lowered by host options.
The temporary SQL row ceiling admits the larger independent allowance and the
separately bounded 64 KB commit-artifact registration; it does not replace either
aggregate check. Lowering the selected metadata budget does not reduce lineage
or artifact-registration allowances.
Private syntax and omitted output still consume work budgets. Capacity failures
refuse the operation; symbol truncation reports candidate and omission counts.

Every result declares `complete`, `compilerVerified` and `providerVerified` false.
Only an admitted Git capture declares `sourceCommitVerified` true and returns its
verified commit/tree IDs. That proves local captured objects, not remote provider
identity, repository ownership or the current provider head. These limits must remain visible when this component
is integrated into repository history and independently authorized provider reads.
