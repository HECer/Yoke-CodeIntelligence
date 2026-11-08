# Captured directory moves

The native move path supports one complete captured text directory through one
explicitly registered semantic provider. The provider must advertise a matching
standard LSP folder `willRenameFiles` filter. The host edit gate, provider trust
and existing passive/source-execution policy apply before the request.

`jb.move` preserves the source basename under its existing target parent.
For example, `src/tree` with target parent `pkg.rs` selects `pkg.rs/tree`.
The direct native semantic `move` argument names the final destination instead.
Omitted, null and exactly empty symbol selectors retain these directory/file
selection rules; symbol moves keep their separate intent contract.

## Complete membership and source authority

Preparation inventories every physical entry and binds the source and target
parents, directory identities, original modes, every subdirectory including
empty ones, and exact captured file metadata. Existing host spellings must be
canonical. Occupied targets, aliases, descendants, links/reparse points, unsafe
names, protected paths, uncaptured binary/generated files and read-only or
excluded write scopes refuse before provider startup. No uncaptured bytes are
silently moved under a partial captured-file selection.

An empty source directory without captured files is unsupported. Moving an
explicit ignored-capture root or a host-registered provider workspace root is
also unsupported; the operation never rewrites those host selections.

Exactly one registered provider language must occur in the selected tree.
Other captured languages remain visible in reference-repair coverage metadata.
The provider sees the existing private captured-source tree and its established
selector omissions. It may return original-coordinate text edits or null;
provider resource operations and commands cannot establish the native move.
A null response does not establish exhaustive reference repair.

The native client appends the checked directory resource operation and derives
one unpublished byte effect before preparing one guarded plan. Every original
file is removed from its old path and represented at its mapped destination.
Names, source languages and modes are preserved; checked provider text edits
remain separate byte changes. No intermediate file-only change plan is published.
Preparation still has the documented nontransactional CAS/snapshot/graph/policy
publication boundary.

## Plan, verification, application and recovery

An optional `directoryMove` record is included in the plan identity under the
`directory-move-v1` domain. File-only plans omit that field and retain their
released identity and serialization. Checked plan loading replays the directory
record against both immutable manifests and the complete file delta.

Isolated copies and verification materialize the declared empty directories and
modes alongside result-source bytes. Verification checks that the selected tree
retains that declared shape. Compiler execution and workspace writes still need
their independent host permissions, exact plan approval and a passing executed
verification.

Workspace application rechecks source bytes, full membership, directory modes
and parent identities before a durable intent. One native filesystem rename
moves the tree, then the checked text changes are applied and journaled.
There is no cross-filesystem recursive-copy fallback.

Recovery identifies the original or moved tree even when a failure preceded the
first `appliedPaths` update. Every moved member must contain its original or
planned bytes before restoration starts. New entries, changed directories,
foreign bytes or two occupied roots refuse the directory rollback and preserve
those objects. After byte restoration, the original directory name and empty
structure are restored; independently changed reference files retain their
existing conditional rollback rules. `RECOVERY_REQUIRED` preserves the journal
identifier when emergency persistence fails.

These identities and checks support the guarded workflow. They do not establish
an OS sandbox, atomic multi-file visibility, cryptographic provider attestation
or protection against hostile same-account filesystem races.

## Finite contract and validation

A selected tree contains 1–1,000 captured files, at most 2,000 directories and
at most 16,000,000 source bytes counted per physical file. Identical CAS hashes
do not remove separate filesystem reads from that admission. Shared LSP virtual
edit/frame limits and the existing 2,000-change, serialized-plan/snapshot, diff,
source and request limits still apply; no partial tree is labeled complete.

Owned protocol fixtures and explicit host-prerequisite records are distinct
from actual compiler/provider validation. Development results, adversarial
cases and live validation are recorded separately. Directory/module reference
completeness, multiple provider languages, noncaptured/binary trees, host-policy
migration, cross-filesystem moves and full proprietary IDE equivalence remain
outside this bounded contract. Results retain `complete=false`.

The explicit `directory_move_live` gate selects installed `rust-analyzer` and
`rustc` through `YOKE_RUST_ANALYZER` and `YOKE_RUSTC`. It uses an owned dependency-free
Cargo fixture, requires a real module-declaration/reference repair, compiles the
copied result and exercises approved workspace application and recovery. The
ordinary test suite leaves this gate ignored when no host selection is provided.
It has no protocol-fixture or compiler fallback.
