# Native Git workflows

The Windows Rust implementation uses a host-selected Git executable for committed
objects, reference metadata, history and committed branch comparisons. It never
invokes working-tree `status`, `diff`, fetch, checkout, hooks, text conversion or
clean/smudge filters. Captured working source is compared with verified committed
source directly in Rust. Implicit fetching of missing promisor objects is disabled;
a missing object fails the request instead of executing a remote helper.

`git_workflows::analyze` accepts bounded `WorkflowOptions`: a base ref, an optional
committed head ref, optional merge-base selection, incoming impact depth, and owner,
history and branch limits. Ref expressions are validated and resolved to commit
object IDs before subsequent commands. Unicode alphanumeric refs are supported.
Unrelated local branch histories are reported explicitly without claiming overlap.

Historical source uses derived snapshots in the same Store. The native receipt
binds the origin capture, exact manifest, analyzer, original commit bytes, complete
tree membership and every admitted blob. Rust verifies Git object IDs and tree
Merkle relationships; SHA-1 is used only for legacy Git interoperability, while
native source and artifact identities use SHA-256. Native capture scope proofs
authorize source reads, including explicitly requested read-only dependency paths.
Symlinks, submodules, unsafe paths, oversized files, binary data and obvious secrets
are omitted with reasons. Committed ignore rules are applied; private Git info and
global ignore rules are not imported.

Historical workflows explicitly reject a host capture configured with
`includeIgnoredDirs` before running Git or publishing history. Observed disk
ignore outcomes are not committed-tree ignore evidence, even when an opted-in
directory currently contains only ordinary unignored files. Supporting that
combination requires a separate typed historical-ignore proof.

Changes contain base/head source anchors. Changed declarations and incoming
structural impact are source-bound candidates. Parallel call observations retain
their separate evidence. CODEOWNERS and historical authors provide candidate
owners; neither proves team membership, identity or actual ownership. Branch
overlap uses current local refs and does not assert remote freshness or collaboration.

Reports are immutable JSON artifacts with separate source receipts. `load` checks
source and graph bindings and reconstructs the complete native change, declaration,
impact and parallel-evidence arrays. Historical and branch rows have strict schemas;
owner suggestions and branch intersections are checked against their declared inputs.
The retention helper validates these same contracts before recognizing exact commit
and report artifact IDs. `render` supplies JSON, text, Markdown, Mermaid and HTML.
`digest` supplies file, report and history projections from a verified report.
Historical publication does not replace the working latest snapshot.

[`git affected`](git-affected-details.md) adds captured TypeScript hierarchy and
incoming-reference candidates from an existing verified report. For source
quotations and deleted/added-path similarity, use the separate
[Git impact-details command](git-impact-details-cli.md).

New workflows honor the typed host `Engine.capture_epoch` selection for both the
working capture and every committed comparison side. The default remains analyzer
8; explicitly selecting analyzer 9 does not mix analyzer-8 historical declarations
with analyzer-9 working declarations. Reading an existing report or replaying a
missing historical graph follows its immutable source proof, independently of the
current host selection.

`seed_worktree_cache` requires an explicit absolute parent root, its expected Yoke
workspace ID and proof that parent and target share a Git common directory. It opens
parent SQLite read-only, checks database and sidecar links, preflights entry sizes,
and selects only exact current analyzer/language/source/path cache keys. Every
matching parser result is reconstructed from captured target bytes before copying.
Parent graphs, snapshots, records, notes, leases and provider/session state are not
copied. Verification currently reparses inputs; cache receipts are not performance
benchmarks.
The target host's selected analyzer also governs seed capture, parser-cache keys,
source reconstruction and the receipt. Entries for a different parent analyzer are
not copied into the target.

Current limits include 4 MB per source, 64 MB aggregate committed source, 10,000
source entries, 16 MB reports, 32 MB parser seed payloads, managed process output
limits and a shared deadline. Object batches are partitioned by bytes. SQLite cell
limits are applied before parent-cache and Git-record length queries; shared Store
limits are restored after each bounded read. Committed ignore rules are applied
hierarchically, so a policy inside a pruned directory cannot reopen it.
The public result retains `complete: false` because
structural impact and ownership are candidates. Modified rename attribution,
area classification, exact evidence quotations and broader workflow integration remain under
development; this module alone does not establish full Graft functional parity.

Tests use real Windows Git repositories and worktrees, including deletion,
renamed original paths, parallel calls, dirty working sources, hostile conversion
and fsmonitor configuration, missing promisor objects, Unicode refs, unrelated
branches, exact tree proof rejection, immutable reports and isolated cache seeding.
